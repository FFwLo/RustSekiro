//! `sekiro-extract anims`: decodes a character's Havok skeleton, animation clips and TAE event
//! timelines from `cache/raw/chr` into `cache/anim/<chr>/`.
//!
//! Outputs (all in FromSoftware source space; convert with `sekiro_formats::anim::to_bevy`):
//! - `skeleton.bin`: [`Skeleton`] in postcard.
//! - `<aXXX_YYYYYY>.bin`: [`AnimClip`] in postcard.
//! - `clips.json`: one summary row per clip (frames, duration, blend, root motion, ranges).
//! - `tae.json`: every TAE animation with its events, raw params and template-decoded fields.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow};
use rayon::prelude::*;
use sekiro_formats::anim::{AnimClip, BlendHint, Skeleton};
use sekiro_formats::hkx::{self, Compendium, DecodeStats, TagFile, tagfile};
use sekiro_formats::tae::{self, AnimHeader, DecodedEvent, Template};
use serde::Serialize;

/// The anibnd folders for a character: `<chr>.anibnd.d` and every `<chr>_*.anibnd.d`.
fn anibnd_dirs(chr_root: &Path, chr: &str) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(chr_root).with_context(|| chr_root.display().to_string())? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if path.is_dir()
            && (lower == format!("{chr}.anibnd.d")
                || (lower.starts_with(&format!("{chr}_")) && lower.ends_with(".anibnd.d")))
        {
            dirs.push(path);
        }
    }
    dirs.sort();
    if dirs.is_empty() {
        return Err(anyhow!(
            "no {chr}.anibnd.d under {}: run `sekiro-extract unpack --filter /chr/` first",
            chr_root.display()
        ));
    }
    Ok(dirs)
}

fn files_with_ext(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        })
        .collect();
    out.sort();
    Ok(out)
}

/// Compendiums by ID, from every folder.
fn load_compendiums(dirs: &[PathBuf]) -> Result<HashMap<[u8; 8], std::sync::Arc<Compendium>>> {
    let mut map = HashMap::new();
    for dir in dirs {
        for path in files_with_ext(dir, "compendium")? {
            let c = std::sync::Arc::new(
                Compendium::parse(&std::fs::read(&path)?)
                    .with_context(|| path.display().to_string())?,
            );
            for id in &c.ids {
                map.insert(*id, c.clone());
            }
        }
    }
    Ok(map)
}

fn open_tagfile(
    path: &Path,
    compendiums: &HashMap<[u8; 8], std::sync::Arc<Compendium>>,
) -> Result<TagFile> {
    let data = std::fs::read(path)?;
    let comp = match tagfile::compendium_ref(&data)? {
        Some(id) => Some(
            compendiums
                .get(&id)
                .ok_or_else(|| anyhow!("no compendium with ID {id:02x?}"))?
                .as_ref(),
        ),
        None => None,
    };
    Ok(TagFile::parse(&data, comp)?)
}

#[derive(Serialize)]
struct ClipSummary {
    name: String,
    binder: String,
    frames: u32,
    duration: f32,
    blend: BlendHint,
    tracks: usize,
    root_motion: Option<[f32; 4]>,
    /// Largest absolute local translation component and the bone it is on.
    max_translation: (f32, String),
    /// Smallest and largest local scale component.
    scale_range: [f32; 2],
    bytes: usize,
}

#[derive(Serialize)]
struct Failure {
    file: String,
    error: String,
}

#[derive(Serialize)]
struct ClipIndex {
    chr: String,
    skeleton_bones: usize,
    clips: Vec<ClipSummary>,
    failures: Vec<Failure>,
    vector_axis_overlaps: u32,
    rotation_mask_overlaps: u32,
    max_spline_degree: u8,
    nonzero_tail_bytes: u32,
}

#[derive(Serialize)]
struct TaeOut {
    chr: String,
    template: String,
    files: Vec<TaeFileOut>,
}

#[derive(Serialize)]
struct TaeFileOut {
    file: String,
    id: i32,
    skeleton_name: String,
    sib_name: String,
    animations: Vec<TaeAnimOut>,
}

#[derive(Serialize)]
struct TaeAnimOut {
    /// Full animation ID (category * 1_000_000 + ID within the TAE).
    id: i64,
    name: String,
    header: Option<AnimHeader>,
    /// HKX clip this animation plays, after following imports.
    hkx: String,
    /// Whether `hkx` was decoded into a `.bin` for this character.
    hkx_decoded: bool,
    file_name: String,
    events: Vec<TaeEventOut>,
}

#[derive(Serialize)]
struct TaeEventOut {
    #[serde(rename = "type")]
    event_type: i32,
    start: f32,
    end: f32,
    unk04: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<i32>,
    params_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    decoded: Option<DecodedEvent>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn summarize(clip: &AnimClip, skeleton: &Skeleton, binder: &str, bytes: usize) -> ClipSummary {
    let mut max_t = (0.0f32, String::new());
    let mut scale = [f32::INFINITY, f32::NEG_INFINITY];
    for t in &clip.tracks {
        for v in t.translation.iter().flatten() {
            if v.abs() > max_t.0 {
                max_t = (v.abs(), skeleton.bones[t.bone as usize].name.clone());
            }
        }
        for &v in t.scale.iter().flatten() {
            scale = [scale[0].min(v), scale[1].max(v)];
        }
    }
    ClipSummary {
        name: clip.name.clone(),
        binder: binder.to_string(),
        frames: clip.frame_count,
        duration: clip.duration,
        blend: clip.blend,
        tracks: clip.tracks.len(),
        root_motion: clip.root_motion.as_ref().map(|r| r.total()),
        max_translation: max_t,
        scale_range: scale,
        bytes,
    }
}

/// Extracts one character. Returns (clips written, failures).
pub fn export_chr(cache: &Path, chr: &str, template: Option<&Template>) -> Result<(usize, usize)> {
    let chr = chr.to_ascii_lowercase();
    let chr_root = cache.join("raw/chr");
    let dirs = anibnd_dirs(&chr_root, &chr)?;
    let compendiums = load_compendiums(&dirs)?;
    let out_dir = cache.join("anim").join(&chr);
    std::fs::create_dir_all(&out_dir)?;

    // Skeleton: skeleton.hkx in the main anibnd.
    let main_dir = &dirs
        .iter()
        .find(|d| {
            d.file_name()
                .and_then(|n| n.to_str())
                .map(str::to_ascii_lowercase)
                == Some(format!("{chr}.anibnd.d"))
        })
        .cloned()
        .unwrap_or_else(|| dirs[0].clone());
    let skel_path = files_with_ext(main_dir, "hkx")?
        .into_iter()
        .find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("skeleton"))
        })
        .ok_or_else(|| anyhow!("{} has no skeleton.hkx", main_dir.display()))?;
    let skeleton = hkx::skeleton_from_tagfile(&open_tagfile(&skel_path, &compendiums)?)
        .with_context(|| skel_path.display().to_string())?;
    std::fs::write(out_dir.join("skeleton.bin"), skeleton.to_bytes()?)?;
    eprintln!(
        "{chr}: skeleton {} with {} bones",
        skeleton.name,
        skeleton.bones.len()
    );

    // Clips.
    let mut jobs = Vec::new();
    for dir in &dirs {
        let binder = dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".anibnd.d", "");
        for p in files_with_ext(dir, "hkx")? {
            if p != skel_path {
                jobs.push((binder.clone(), p));
            }
        }
    }
    let stats = Mutex::new(DecodeStats::default());
    let results: Vec<_> = jobs
        .par_iter()
        .map(|(binder, path)| {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let run = || -> Result<ClipSummary> {
                let file = open_tagfile(path, &compendiums)?;
                let mut s = DecodeStats::default();
                let clip = hkx::clip_from_tagfile(&file, &name, skeleton.bones.len(), &mut s)?;
                let bytes = clip.to_bytes()?;
                std::fs::write(out_dir.join(format!("{name}.bin")), &bytes)?;
                let mut total = stats.lock().unwrap();
                total.vector_axis_overlaps += s.vector_axis_overlaps;
                total.rotation_mask_overlaps += s.rotation_mask_overlaps;
                total.max_degree = total.max_degree.max(s.max_degree);
                total.nonzero_tail_bytes += s.nonzero_tail_bytes;
                Ok(summarize(&clip, &skeleton, binder, bytes.len()))
            };
            run().map_err(|e| Failure {
                file: format!("{binder}/{name}"),
                error: format!("{e:#}"),
            })
        })
        .collect();
    let mut clips = Vec::new();
    let mut failures = Vec::new();
    for r in results {
        match r {
            Ok(c) => clips.push(c),
            Err(f) => {
                eprintln!("FAILED {}: {}", f.file, f.error);
                failures.push(f);
            }
        }
    }
    clips.sort_by(|a, b| a.name.cmp(&b.name));
    let stats = stats.into_inner().unwrap();
    let decoded: std::collections::HashSet<String> = clips.iter().map(|c| c.name.clone()).collect();
    let (n_clips, n_failed) = (clips.len(), failures.len());
    let total_bytes: usize = clips.iter().map(|c| c.bytes).sum();
    std::fs::write(
        out_dir.join("clips.json"),
        serde_json::to_string_pretty(&ClipIndex {
            chr: chr.clone(),
            skeleton_bones: skeleton.bones.len(),
            clips,
            failures,
            vector_axis_overlaps: stats.vector_axis_overlaps,
            rotation_mask_overlaps: stats.rotation_mask_overlaps,
            max_spline_degree: stats.max_degree,
            nonzero_tail_bytes: stats.nonzero_tail_bytes,
        })?,
    )?;
    eprintln!(
        "{chr}: {n_clips} clips decoded ({:.1} MB), {n_failed} failed",
        total_bytes as f64 / 1e6
    );

    // TAE.
    let mut tae_files = Vec::new();
    let mut events_total = 0usize;
    let mut undecoded: BTreeMap<i32, usize> = BTreeMap::new();
    for dir in &dirs {
        for path in files_with_ext(dir, "tae")? {
            let data = std::fs::read(&path)?;
            let t = tae::parse(&data).with_context(|| path.display().to_string())?;
            let category = tae::category_from_file_name(
                &path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_ascii_lowercase(),
            );
            let full = |id: i64| tae::full_id(category, id);
            let by_id: HashMap<i64, &tae::TaeAnimation> =
                t.animations.iter().map(|a| (full(a.id), a)).collect();
            let animations = t
                .animations
                .iter()
                .map(|a| {
                    // Follow import chains (bounded) to the HKX that actually plays.
                    let mut src = full(a.hkx_source());
                    for _ in 0..8 {
                        match by_id.get(&src) {
                            Some(next) if full(next.hkx_source()) != src => {
                                src = full(next.hkx_source())
                            }
                            _ => break,
                        }
                    }
                    let hkx_name = tae::anim_name(src);
                    let events = a
                        .events
                        .iter()
                        .map(|e| {
                            events_total += 1;
                            let decoded = template.and_then(|t| t.decode(e));
                            if template.is_some() && decoded.is_none() {
                                *undecoded.entry(e.event_type).or_default() += 1;
                            }
                            TaeEventOut {
                                event_type: e.event_type,
                                start: e.start,
                                end: e.end,
                                unk04: e.unk04,
                                group: e.group,
                                params_hex: hex(&e.params),
                                decoded,
                            }
                        })
                        .collect();
                    TaeAnimOut {
                        id: full(a.id),
                        name: tae::anim_name(full(a.id)),
                        header: a.header.clone(),
                        hkx_decoded: decoded.contains(&hkx_name),
                        hkx: hkx_name,
                        file_name: a.file_name.clone(),
                        events,
                    }
                })
                .collect();
            tae_files.push(TaeFileOut {
                file: format!(
                    "{}/{}",
                    dir.file_name().unwrap().to_string_lossy(),
                    path.file_name().unwrap().to_string_lossy()
                ),
                id: t.id,
                skeleton_name: t.skeleton_name,
                sib_name: t.sib_name,
                animations,
            });
        }
    }
    // Aliases: animations without an HKX of their own play another's. Resolve chains across all
    // TAE files so every name maps straight to a decoded clip.
    let direct: HashMap<String, String> = tae_files
        .iter()
        .flat_map(|f| f.animations.iter())
        .filter(|a| a.hkx != a.name)
        .map(|a| (a.name.clone(), a.hkx.clone()))
        .collect();
    let mut aliases: BTreeMap<String, String> = BTreeMap::new();
    for name in direct.keys() {
        let mut src = name.clone();
        for _ in 0..16 {
            match direct.get(&src) {
                Some(next) if *next != src => src = next.clone(),
                _ => break,
            }
        }
        if decoded.contains(&src) && !decoded.contains(name) {
            aliases.insert(name.clone(), src);
        }
    }
    std::fs::write(
        out_dir.join("aliases.json"),
        serde_json::to_string_pretty(&aliases)?,
    )?;
    eprintln!(
        "{chr}: {} animations play another's HKX (aliases.json)",
        aliases.len()
    );
    let n_anims: usize = tae_files.iter().map(|f| f.animations.len()).sum();
    std::fs::write(
        out_dir.join("tae.json"),
        serde_json::to_string(&TaeOut {
            chr: chr.clone(),
            template: template.map(|t| t.game.clone()).unwrap_or_default(),
            files: tae_files,
        })?,
    )?;
    eprintln!("{chr}: TAE {n_anims} animations, {events_total} events");
    if !undecoded.is_empty() {
        eprintln!("{chr}: event types without a template entry: {undecoded:?}");
    }
    Ok((n_clips, n_failed))
}

pub fn export(cache: &Path, chrs: &[String]) -> Result<()> {
    let template_path = cache.join("refs/TAE.Template.SDT.xml");
    let template = match std::fs::read_to_string(&template_path) {
        Ok(text) => Some(Template::parse_xml(&text)?),
        Err(_) => {
            eprintln!(
                "{} missing (run tools/fetch-refs.ps1): TAE events will have raw params only",
                template_path.display()
            );
            None
        }
    };
    let chrs: Vec<String> = if chrs.is_empty() {
        crate::roster_ids(cache, true)?
    } else {
        chrs.to_vec()
    };
    let mut failed = 0;
    for chr in &chrs {
        failed += export_chr(cache, chr, template.as_ref())?.1;
    }
    if failed > 0 {
        eprintln!("{failed} clips failed; see failures in cache/anim/<chr>/clips.json");
    }
    Ok(())
}
