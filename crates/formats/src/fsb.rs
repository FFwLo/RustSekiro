//! FSB5 sample banks (`sound/*.fsb`).
//!
//! Layout (little endian), as found in the game's banks:
//!
//! - Header, 60 bytes for version 1: `"FSB5"`, version, sample count, size of the sample header
//!   table, size of the name table, size of the data, codec, then flags and a 16-byte hash.
//! - One sample header per sample: a 64-bit word packing "more chunks follow" (bit 0), a
//!   sample-rate index (bits 1-4), stereo (bit 5), the data offset in 16-byte units (bits 6-33)
//!   and the sample count (bits 34-63), followed by optional chunks, each with a 32-bit word
//!   packing "more chunks" (bit 0), size (bits 1-24) and type (bits 25-31).
//! - The name table: one 32-bit offset per sample, then zero-terminated names.
//! - The sample data.
//!
//! Every unencrypted bank in the game uses codec 15 (Vorbis). A Vorbis sample's data is a
//! sequence of audio packets, each prefixed by its 16-bit length; the identification and setup
//! headers are not stored, only the CRC-32 of the setup header (chunk type 11). See
//! `docs/AUDIO.md` for how those headers are regenerated.
//!
//! Some banks (`sm*`, `vm*`, `xm*`, `rm*`) are encrypted; [`Fsb::parse`] rejects them.

use crate::{Error, Result};

/// Codec ids used in the header.
pub const CODEC_PCM16: u32 = 2;
pub const CODEC_VORBIS: u32 = 15;

/// One sample of a bank.
#[derive(Debug, Clone, PartialEq)]
pub struct FsbSample {
    pub name: String,
    pub rate: u32,
    pub channels: u8,
    /// Sample frames.
    pub frames: u32,
    /// Byte range of the sample's data inside the bank file.
    pub data: std::ops::Range<usize>,
    /// Loop start and end in frames, if the sample loops.
    pub looping: Option<(u32, u32)>,
    /// CRC-32 of the Vorbis setup header the sample was encoded with.
    pub vorbis_crc: Option<u32>,
}

/// A parsed bank: codec and sample table; data stays in the caller's buffer.
#[derive(Debug, Clone)]
pub struct Fsb {
    pub version: u32,
    pub codec: u32,
    pub samples: Vec<FsbSample>,
}

fn rate_from_index(i: u64) -> u32 {
    match i {
        1 => 8000,
        2 => 11000,
        3 => 11025,
        4 => 16000,
        5 => 22050,
        6 => 24000,
        7 => 32000,
        8 => 44100,
        9 => 48000,
        _ => 0,
    }
}

fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    b.get(p..p + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| Error::Format("FSB5 truncated".into()))
}

/// Whether `data` starts like an unencrypted FSB5 bank.
pub fn is_fsb5(data: &[u8]) -> bool {
    data.starts_with(b"FSB5")
}

impl Fsb {
    pub fn parse(b: &[u8]) -> Result<Fsb> {
        if !is_fsb5(b) {
            return Err(Error::Format(
                "not an FSB5 bank (encrypted or other)".into(),
            ));
        }
        let version = u32_at(b, 4)?;
        let count = u32_at(b, 8)? as usize;
        let headers_size = u32_at(b, 12)? as usize;
        let names_size = u32_at(b, 16)? as usize;
        let data_size = u32_at(b, 20)? as usize;
        let codec = u32_at(b, 24)?;
        let header_size = if version == 0 { 0x40 } else { 0x3C };
        let names_at = header_size + headers_size;
        let data_at = names_at + names_size;
        if data_at + data_size > b.len() {
            return Err(Error::Format("FSB5 data past end of file".into()));
        }
        let mut p = header_size;
        let mut samples = Vec::with_capacity(count);
        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            let raw = b
                .get(p..p + 8)
                .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
                .ok_or_else(|| Error::Format("FSB5 sample table truncated".into()))?;
            p += 8;
            let mut more = raw & 1 != 0;
            let mut rate = rate_from_index((raw >> 1) & 0xF);
            let mut channels = if (raw >> 5) & 1 != 0 { 2 } else { 1 };
            let offset = (((raw >> 6) & 0x0FFF_FFFF) * 16) as usize;
            let frames = (raw >> 34) as u32;
            let mut looping = None;
            let mut vorbis_crc = None;
            while more {
                let chunk = u32_at(b, p)?;
                p += 4;
                more = chunk & 1 != 0;
                let size = ((chunk >> 1) & 0xFF_FFFF) as usize;
                match chunk >> 25 {
                    1 => channels = b.get(p).copied().unwrap_or(1),
                    2 => rate = u32_at(b, p)?,
                    3 => looping = Some((u32_at(b, p)?, u32_at(b, p + 4)?)),
                    11 => vorbis_crc = Some(u32_at(b, p)?),
                    _ => {}
                }
                p += size;
            }
            let name_off = u32_at(b, names_at + 4 * i).unwrap_or(0) as usize;
            let name = if names_size > 0 {
                let s = &b[names_at + name_off..data_at];
                let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                String::from_utf8_lossy(&s[..end]).into_owned()
            } else {
                format!("{i}")
            };
            offsets.push(offset);
            samples.push(FsbSample {
                name,
                rate,
                channels,
                frames,
                data: 0..0,
                looping,
                vorbis_crc,
            });
        }
        for i in 0..count {
            let start = data_at + offsets[i];
            let end = offsets
                .get(i + 1)
                .map_or(data_at + data_size, |&o| data_at + o);
            samples[i].data = start..end.max(start);
        }
        Ok(Fsb {
            version,
            codec,
            samples,
        })
    }
}

/// The audio packets of an FSB5 Vorbis sample (each stored behind a 16-bit length; a zero or
/// out-of-range length ends the list).
pub fn vorbis_packets(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 2 <= data.len() {
        let n = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2;
        if n == 0 || p + n > data.len() {
            break;
        }
        out.push(&data[p..p + n]);
        p += n;
    }
    out
}

/// Writes 16-bit PCM WAV bytes for interleaved samples.
pub fn wav_bytes(rate: u32, channels: u16, interleaved: &[i16]) -> Vec<u8> {
    let data_len = (interleaved.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    w.extend_from_slice(&(channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in interleaved {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}
