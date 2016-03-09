#![feature(question_mark)]
extern crate byteorder;

use std::fs::{File};
use std::io::{self,Seek,SeekFrom};

use byteorder::{LittleEndian, ReadBytesExt};

const RIFF : u32 = 0x46464952;
const WAVE : u32 = 0x45564157;
const FMT_ : u32 = 0x20746d66;
const DATA : u32 = 0x61746164;
const LIST : u32 = 0x5453494c;

pub const FORMAT_PCM          : u16 = 1;
pub const FORMAT_IEE_FLOAT    : u16 = 3;
pub const FORMAT_WAV_EXTENDED : u16 = 0xfffe;

#[derive(Debug)]
pub enum WavError {
  IoError(io::Error),
  Unsupported(&'static str),
  ParseError(&'static str)
}

#[derive(Debug)]
pub struct WaveInfo {
  audio_format:    u16,
  channels:        u16,
  samples_rate:    u32,
  byte_rate:       u32,
  block_align:     u16,
  bits_per_sample: u16,
  total_frames:    u32
}

#[derive(Debug)]
pub struct WaveFile {
  file:          File,
  info:          WaveInfo,
  current_frame: u32
}

#[derive(Debug,PartialEq)]
pub enum Frame {
  Mono(u32),
  Stereo(u32, u32),
  Multi(Vec<u32>)
}

impl From<io::Error> for WavError {
  fn from(e: io::Error) -> Self {
    WavError::IoError(e)
  }
}

impl From<byteorder::Error> for WavError {
  fn from(e: byteorder::Error) -> Self {
    match e {
      byteorder::Error::UnexpectedEOF => WavError::ParseError("Unexpected EOF"),
      byteorder::Error::Io(e) => WavError::IoError(e)
    }
  }
}

impl Iterator for WaveFile {
  type Item = Frame;

  fn next(&mut self) -> Option<Frame> {
    if self.current_frame >= self.info.total_frames {
      return None;
    }
    let bytes_per_sample = (self.info.bits_per_sample as usize) / 8;
    let mut samples : Vec<u32> = Vec::with_capacity(self.info.channels as usize);

    for _ in 0..self.info.channels {
      match self.file.read_uint::<LittleEndian>(bytes_per_sample) {
        Ok(sample) => samples.push(sample as u32),
        Err(_)     => { return None; }
      }
    }
    match self.info.channels {
      0 => unreachable!(),
      1 => { Some(Frame::Mono(samples[0])) },
      2 => { Some(Frame::Stereo(samples[0], samples[1])) },
      _ => { Some(Frame::Multi(samples)) }
    }
  }
}

impl WaveFile {

  pub fn open<S: Into<String>>(path: S) -> Result<WaveFile, WavError> {
    let filename = path.into();
    let mut file = File::open(filename)?;
    let info = WaveFile::read_header_chunks(&mut file)?;

    Ok(WaveFile { file: file, info: info, current_frame: 0 })
  }

  fn read_header_chunks(file: &mut File) -> Result<WaveInfo, WavError> {
    let mut have_fmt   = false;
    let mut chunk_id   = file.read_u32::<LittleEndian>()?;
    let mut chunk_size : u32;
    let data_size : u32;

    file.read_u32::<LittleEndian>()?;

    let riff_type      = file.read_u32::<LittleEndian>()?;

    if chunk_id != RIFF || riff_type != WAVE {
      return Err(WavError::ParseError("Not a Wavefile"));
    }

    let mut info = WaveInfo{
      audio_format:    0,
      channels:        0,
      samples_rate:    0,
      byte_rate:       0,
      block_align:     0,
      bits_per_sample: 0,
      total_frames:    0
    };


    loop {
      chunk_id   = file.read_u32::<LittleEndian>()?;
      chunk_size = file.read_u32::<LittleEndian>()?;

      match chunk_id {
        FMT_ => {
          have_fmt = true;
          info.audio_format    = file.read_u16::<LittleEndian>()?;
          info.channels        = file.read_u16::<LittleEndian>()?;
          info.samples_rate    = file.read_u32::<LittleEndian>()?;
          info.byte_rate       = file.read_u32::<LittleEndian>()?;
          info.block_align     = file.read_u16::<LittleEndian>()?;
          info.bits_per_sample = file.read_u16::<LittleEndian>()?;
        },
        DATA => {
          data_size = chunk_size;
          break;
        },
        LIST => { file.seek(SeekFrom::Current(chunk_size as i64))?; },
        _    => { return Err(WavError::ParseError("Unexpected Chunk ID")); }
      }
    }

    if !have_fmt {
      return Err(WavError::ParseError("Format Chunk not found"));
    }

    if info.audio_format != FORMAT_PCM {
      return Err(WavError::Unsupported("Non-PCM Format"));
    }

    if info.channels == 0 || info.bits_per_sample < 8 {
      return Err(WavError::ParseError("Invalid channel or bits per sample value found"));
    }

    info.total_frames = data_size / (info.channels as u32 * info.bits_per_sample as u32 / 8 );

    Ok(info)
  }
}

#[test]
fn test_parse_file_info() {
  let file = match WaveFile::open("./fixtures/test.wav") {
    Ok(f) => f,
    Err(e) => panic!("Error: {:?}", e)
  };

  assert_eq!(file.info.audio_format,    FORMAT_PCM);
  assert_eq!(file.info.channels,        2);
  assert_eq!(file.info.samples_rate,    48000);
  assert_eq!(file.info.byte_rate,       288000);
  assert_eq!(file.info.block_align,     6);
  assert_eq!(file.info.bits_per_sample, 24);
  assert_eq!(file.info.total_frames,    501888);
}

#[test]
fn test_read_frame_values() {
  let mut file = match WaveFile::open("./fixtures/test.wav") {
    Ok(f) => f,
    Err(e) => panic!("Error: {:?}", e)
  };

  let frames = file.take(2).collect::<Vec<_>>();
  let expected = vec![
    Frame::Stereo(19581, 19581),
    Frame::Stereo(24337, 24337)
  ];

  for i in 0..expected.len() {
    assert_eq!(frames[i], expected[i]);
  }
}

#[test]
fn test_read_all_frames() {
  let mut file = match WaveFile::open("./fixtures/test.wav") {
    Ok(f) => f,
    Err(e) => panic!("Error: {:?}", e)
  };

  let frames = file.collect::<Vec<_>>();
  assert_eq!(frames.len(), 501888);
}
