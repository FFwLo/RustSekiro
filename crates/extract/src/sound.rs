//! `sekiro-extract sound`: decodes the game's FSB5 Vorbis banks to WAV and writes an event
//! table per bank.
//!
//! Output, per bank `<b>` (for example `c1010`, `main`):
//! - `cache/sound/<b>/<sample>.wav`: 16-bit PCM;
//! - `cache/sound/<b>/events.json`: `{ "<event>": [{"file": "<sample>.wav", "weight": n}, ...] }`.
//!
//! The Vorbis headers the banks leave out must first be regenerated into `cache/sound/vorbis/`
//! with `uv run tools/sound/vorbis_setups.py cache` (see docs/AUDIO.md).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use lewton::audio::{PreviousWindowRight, read_audio_packet};
use lewton::header::{IdentHeader, SetupHeader, read_header_ident, read_header_setup};
use rayon::prelude::*;
use sekiro_formats::fev::{Fev, sound_stem};
use sekiro_formats::fsb::{CODEC_VORBIS, Fsb, FsbSample, is_fsb5, vorbis_packets, wav_bytes};
use serde::Serialize;

/// Banks decoded when none are named: the shared bank, the Ashina soldier and Ashina outskirts.
const DEFAULT_BANKS: [&str; 3] = ["main", "c1010", "m11"];

struct Codebooks {
    ident: IdentHeader,
    setup: SetupHeader,
}

fn load_codebooks(dir: &Path) -> Result<HashMap<u32, Codebooks>> {
    let mut out = HashMap::new();
    let entries = std::fs::read_dir(dir).with_context(|| {
        format!(
            "{} missing: run `uv run tools/sound/vorbis_setups.py cache` first",
            dir.display()
        )
    })?;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "setup") {
            continue;
        }
        let Some(crc) = p.file_stem().and_then(|s| s.to_str()?.parse::<u32>().ok()) else {
            continue;
        };
        let ident_bytes = std::fs::read(p.with_extension("ident"))?;
        let ident = read_header_ident(&ident_bytes)
            .map_err(|e| anyhow::anyhow!("{}: {e:?}", p.display()))?;
        let setup = read_header_setup(
            &std::fs::read(&p)?,
            ident.audio_channels,
            (ident.blocksize_0, ident.blocksize_1),
        )
        .map_err(|e| anyhow::anyhow!("{}: {e:?}", p.display()))?;
        out.insert(crc, Codebooks { ident, setup });
    }
    Ok(out)
}

/// Decodes one Vorbis sample to interleaved 16-bit PCM, trimmed to the bank's frame count.
fn decode(sample: &FsbSample, bank: &[u8], books: &Codebooks) -> Result<Vec<i16>> {
    let channels = sample.channels as usize;
    if channels != books.ident.audio_channels as usize {
        bail!("channel count differs from the setup header");
    }
    let mut pwr = PreviousWindowRight::new();
    let mut out = Vec::with_capacity(sample.frames as usize * channels);
    for packet in vorbis_packets(&bank[sample.data.clone()]) {
        let pcm = read_audio_packet(&books.ident, &books.setup, packet, &mut pwr)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        if pcm.is_empty() || pcm[0].is_empty() {
            continue;
        }
        for i in 0..pcm[0].len() {
            for ch in &pcm {
                out.push(ch[i]);
            }
        }
    }
    out.truncate(sample.frames as usize * channels);
    Ok(out)
}

#[derive(Serialize)]
struct Variation {
    file: String,
    weight: u32,
}

pub fn export(cache: &Path, banks: &[String], all: bool) -> Result<()> {
    let raw = cache.join("raw/sound");
    let books = load_codebooks(&cache.join("sound/vorbis"))?;
    let mut names: Vec<String> = if all {
        let mut v: Vec<String> = std::fs::read_dir(&raw)?
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension()? == "fsb").then(|| p.file_stem()?.to_str().map(str::to_owned))?
            })
            .collect();
        v.sort();
        v
    } else if banks.is_empty() {
        DEFAULT_BANKS.iter().map(|s| s.to_string()).collect()
    } else {
        banks.to_vec()
    };
    names.dedup();
    for name in names {
        let path = raw.join(format!("{name}.fsb"));
        let data = std::fs::read(&path).with_context(|| path.display().to_string())?;
        if !is_fsb5(&data) {
            println!("{name}: encrypted bank, skipped");
            continue;
        }
        let fsb = Fsb::parse(&data)?;
        if fsb.codec != CODEC_VORBIS {
            println!("{name}: codec {} not supported, skipped", fsb.codec);
            continue;
        }
        let out = cache.join("sound").join(&name);
        std::fs::create_dir_all(&out)?;
        let results: Vec<(String, std::result::Result<(), String>)> = fsb
            .samples
            .par_iter()
            .map(|s| {
                let r = (|| -> std::result::Result<(), String> {
                    let crc = s.vorbis_crc.ok_or("no Vorbis setup CRC")?;
                    let b = books
                        .get(&crc)
                        .ok_or(format!("no setup header for CRC {crc}"))?;
                    let pcm = decode(s, &data, b).map_err(|e| e.to_string())?;
                    let wav = wav_bytes(s.rate, s.channels as u16, &pcm);
                    std::fs::write(out.join(format!("{}.wav", s.name)), wav)
                        .map_err(|e| e.to_string())
                })();
                (s.name.clone(), r)
            })
            .collect();
        let failed: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();
        // Event table: from the FEV sound definitions when present, else from sample names.
        let mut events: BTreeMap<String, Vec<Variation>> = BTreeMap::new();
        let decoded: std::collections::HashSet<&str> = results
            .iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(n, _)| n.as_str())
            .collect();
        if let Ok(fev_bytes) = std::fs::read(raw.join(format!("{name}.fev")))
            && let Ok(fev) = Fev::parse(&fev_bytes)
        {
            for def in &fev.sound_defs {
                let Some(stem) = def.stem() else { continue };
                let list = events.entry(stem).or_default();
                for w in &def.waves {
                    if decoded.contains(w.name.as_str()) {
                        list.push(Variation {
                            file: format!("{}.wav", w.name),
                            weight: w.weight,
                        });
                    }
                }
            }
        }
        let from_fev: std::collections::HashSet<String> = events.keys().cloned().collect();
        for s in &fsb.samples {
            let stem = sound_stem(&s.name);
            if !decoded.contains(s.name.as_str()) || from_fev.contains(stem) {
                continue;
            }
            events.entry(stem.to_owned()).or_default().push(Variation {
                file: format!("{}.wav", s.name),
                weight: 100,
            });
        }
        events.retain(|_, v| !v.is_empty());
        std::fs::write(
            out.join("events.json"),
            serde_json::to_string_pretty(&events)?,
        )?;
        println!(
            "{name}: {} samples, {} decoded, {} failed, {} events",
            fsb.samples.len(),
            fsb.samples.len() - failed.len(),
            failed.len(),
            events.len()
        );
        for (n, e) in failed.iter().take(3) {
            println!("  {n}: {}", e.as_ref().unwrap_err());
        }
    }
    Ok(())
}
