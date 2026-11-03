//! FMOD event projects (`sound/*.fev`), read far enough to map event names to samples.
//!
//! The files are RIFF (`RIFF`/`FEV `) with a `FMT ` chunk (format 0x00450000) and a `LIST`/`PROJ`
//! holding, among others:
//!
//! - `STRR`: a string table (count, offsets, zero-terminated strings) with the project name, the
//!   event names (`c101001001`, ...) and category names;
//! - `LGCY`: the FMOD Designer event data. Its sound definitions list their waveforms as
//!   `type, weight, name length, name ("bank/c1010/c101001001b.wav"), bank index, index in
//!   bank, length in milliseconds`, preceded by the waveform count.
//!
//! We read the string table and the sound definitions' waveform lists. The event-to-definition
//! links, volumes and pitch randomisation inside `LGCY` are not decoded; events are matched to
//! definitions by name (see [`Fev::event_waves`] and `docs/AUDIO.md`).

use crate::{Error, Result};

/// One waveform of a sound definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    /// File stem, such as `c101001001b`.
    pub name: String,
    pub weight: u32,
    pub bank: u32,
    pub index: u32,
    pub length_ms: u32,
}

/// A sound definition: the waveforms one is chosen from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundDef {
    pub waves: Vec<Wave>,
}

impl SoundDef {
    /// The sound id the definition plays: the first real waveform's name without its variation
    /// letters (`c101001001b` gives `c101001001`).
    pub fn stem(&self) -> Option<String> {
        self.waves
            .iter()
            .map(|w| w.name.as_str())
            .find(|n| !n.starts_with("blank") && !n.contains("dummy"))
            .map(|n| sound_stem(n).to_owned())
    }
}

/// `c101001001b` -> `c101001001`: a type letter, then digits; trailing letters are variations.
pub fn sound_stem(name: &str) -> &str {
    let b = name.as_bytes();
    if b.len() > 1 && b[0].is_ascii_lowercase() {
        let digits = b[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 {
            return &name[..1 + digits];
        }
    }
    name
}

#[derive(Debug, Clone, Default)]
pub struct Fev {
    pub strings: Vec<String>,
    pub sound_defs: Vec<SoundDef>,
}

fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    b.get(p..p + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

impl Fev {
    pub fn parse(b: &[u8]) -> Result<Fev> {
        if b.len() < 12 || &b[..4] != b"RIFF" || &b[8..12] != b"FEV " {
            return Err(Error::Format("not an FEV file".into()));
        }
        let mut fev = Fev::default();
        walk(b, 12, b.len(), &mut |id, data| match id {
            b"STRR" => fev.strings = read_strings(data),
            b"LGCY" => fev.sound_defs = read_sound_defs(data),
            _ => {}
        });
        Ok(fev)
    }

    /// Event names in the string table: a type letter followed by digits.
    pub fn event_names(&self) -> impl Iterator<Item = &str> {
        self.strings
            .iter()
            .map(String::as_str)
            .filter(|s| s.as_bytes().get(1).is_some_and(u8::is_ascii_digit) && sound_stem(s) == *s)
    }

    /// All waveforms of the definitions whose stem is `event`.
    pub fn event_waves(&self, event: &str) -> Vec<&Wave> {
        self.sound_defs
            .iter()
            .filter(|d| d.stem().as_deref() == Some(event))
            .flat_map(|d| d.waves.iter())
            .collect()
    }
}

fn walk(b: &[u8], mut p: usize, end: usize, f: &mut dyn FnMut(&[u8; 4], &[u8])) {
    while p + 8 <= end {
        let id: [u8; 4] = b[p..p + 4].try_into().unwrap();
        let size = u32_at(b, p + 4).unwrap_or(0) as usize;
        let body_end = (p + 8 + size).min(end);
        if &id == b"LIST" || &id == b"RIFF" {
            walk(b, p + 12, body_end, f);
        } else {
            f(&id, &b[p + 8..body_end]);
        }
        p = body_end + (size & 1);
    }
}

fn read_strings(d: &[u8]) -> Vec<String> {
    let Some(n) = u32_at(d, 0) else {
        return Vec::new();
    };
    let base = 4 + 4 * n as usize;
    (0..n as usize)
        .filter_map(|i| {
            let o = base + u32_at(d, 4 + 4 * i)? as usize;
            let s = d.get(o..)?;
            let end = s.iter().position(|&c| c == 0)?;
            Some(String::from_utf8_lossy(&s[..end]).into_owned())
        })
        .collect()
}

/// Parses one waveform entry at `p`; returns it and the offset after it.
fn wave_at(d: &[u8], p: usize) -> Option<(Wave, usize)> {
    let weight = u32_at(d, p + 4)?;
    let len = u32_at(d, p + 8)? as usize;
    if u32_at(d, p)? > 16 || weight > 100_000 || !(5..=260).contains(&len) {
        return None;
    }
    let name = d.get(p + 12..p + 12 + len)?;
    if name.last() != Some(&0) || !name[..len - 1].ends_with(b".wav") {
        return None;
    }
    let path = std::str::from_utf8(&name[..len - 5]).ok()?;
    let stem = path.rsplit('/').next().unwrap_or(path).to_owned();
    let q = p + 12 + len;
    Some((
        Wave {
            name: stem,
            weight,
            bank: u32_at(d, q)?,
            index: u32_at(d, q + 4)?,
            length_ms: u32_at(d, q + 8)?,
        },
        q + 12,
    ))
}

fn read_sound_defs(d: &[u8]) -> Vec<SoundDef> {
    let mut defs = Vec::new();
    let mut p = 4;
    while p + 16 < d.len() {
        // A definition is a waveform count followed by that many back-to-back entries.
        let count = u32_at(d, p).unwrap_or(0) as usize;
        if (1..=64).contains(&count)
            && let Some((first, mut q)) = wave_at(d, p + 4)
        {
            let mut waves = vec![first];
            while waves.len() < count {
                match wave_at(d, q) {
                    Some((w, next)) => {
                        waves.push(w);
                        q = next;
                    }
                    None => break,
                }
            }
            if waves.len() == count {
                defs.push(SoundDef { waves });
                p = q;
                continue;
            }
        }
        p += 1;
    }
    defs
}
