//! `sekiro-extract anims <x.anibnd.d> <out.bin> [suffix]`: one anibnd's skeleton and every clip
//! (a*.hkx) as an anim bin, the same "SHAN" layout as export.rs write_anim_bin (src/anim.rs
//! loads both). Clip keys are the file stems, minus `suffix` when given: the prosthetic tools'
//! WP_A_07xx_1.anibnd names its clips after Wolf's anims plus "_1" (a076_405010_1), keyed here
//! by Wolf's anim (a076_405010) so the tool plays the clip of Wolf's current anim.

use std::path::Path;

use crate::hkx;

pub fn export(dir: &Path, out_path: &Path, suffix: &str) {
    let files: Vec<std::path::PathBuf> = std::fs::read_dir(dir).expect("read anibnd dir").flatten().map(|e| e.path()).collect();
    let types = files
        .iter()
        .find(|p| p.extension().is_some_and(|x| x == "compendium"))
        .map(|p| hkx::read_types(&std::fs::read(p).unwrap()))
        .unwrap_or_default();
    let stem = |p: &Path| p.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let Some(skel) = files.iter().find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("hkx")) && stem(p).to_lowercase().starts_with("skeleton")) else {
        eprintln!("{}: no skeleton hkx", dir.display());
        return;
    };
    let skel = std::fs::read(skel).unwrap();
    let mut bones = hkx::skeleton(&skel, &hkx::Types::default());
    if bones.is_empty() {
        bones = hkx::skeleton(&skel, &types);
    }
    let mut clips: Vec<(String, hkx::Clip)> = Vec::new();
    let mut names: Vec<&std::path::PathBuf> = files.iter().filter(|p| p.extension().is_some_and(|x| x == "hkx") && stem(p).starts_with('a')).collect();
    names.sort();
    for p in names {
        let d = std::fs::read(p).unwrap();
        let Some(c) = hkx::clip(&d, &types).or_else(|| hkx::clip(&d, &hkx::Types::default())) else {
            eprintln!("{}: no spline clip", p.display());
            continue;
        };
        let s = stem(p);
        let key = if suffix.is_empty() { s.clone() } else { s.strip_suffix(suffix).unwrap_or(&s).to_string() };
        clips.push((key, c));
    }
    let mut out: Vec<u8> = Vec::new();
    let qs = |out: &mut Vec<u8>, q: &crate::spline::Qs| {
        for v in q.t.iter().chain(q.r.iter()).chain(q.s.iter()) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    };
    out.extend_from_slice(b"SHAN");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(bones.len() as u32).to_le_bytes());
    for b in &bones {
        out.extend_from_slice(&(b.name.len() as u16).to_le_bytes());
        out.extend_from_slice(b.name.as_bytes());
        out.extend_from_slice(&b.parent.to_le_bytes());
        qs(&mut out, &b.pose);
    }
    out.extend_from_slice(&(clips.len() as u32).to_le_bytes());
    for (key, c) in &clips {
        out.extend_from_slice(&(key.len() as u16).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&c.frame_duration.to_le_bytes());
        out.extend_from_slice(&(c.frames as u32).to_le_bytes());
        out.extend_from_slice(&(c.track_to_bone.len() as u32).to_le_bytes());
        for t in &c.track_to_bone {
            out.extend_from_slice(&t.to_le_bytes());
        }
        for frame in &c.samples {
            for (i, q) in frame.iter().enumerate() {
                if i < c.track_to_bone.len() {
                    qs(&mut out, q);
                }
            }
        }
    }
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out_path, &out).unwrap();
    println!("{}: {} bones, {} clips -> {}", dir.display(), bones.len(), clips.len(), out_path.display());
}
