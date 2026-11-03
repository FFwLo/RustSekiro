//! Havok cloth export: `cache/models/<id>.cloth.bin` beside the model's GLB, read from the
//! `*_c.hkx` that sits next to each FLVER (the character's chrbnd, or each equipped part's
//! partsbnd). The decoding is [`sekiro_formats::hkx::cloth`]; this only finds the files.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sekiro_formats::hkx::TagFile;
use sekiro_formats::hkx::cloth::{self, ClothSet};

use super::{Model, find_ci};

/// The folder holding a part's FLVER and cloth: a character's chrbnd (`c1010`) or a parts binder
/// (`BD_M_9040`, whose folder drops any further suffix).
fn folder(raw: &Path, label: &str) -> Option<PathBuf> {
    let chr = raw.join(format!("chr/{label}.chrbnd.d"));
    if chr.is_dir() {
        return Some(chr);
    }
    let base = label.split('_').take(3).collect::<Vec<_>>().join("_");
    let parts = raw.join(format!("parts/{}.partsbnd.d", base.to_ascii_lowercase()));
    parts.is_dir().then_some(parts)
}

/// Writes `<out>/<id>.cloth.bin` when any part of the model has cloth, and removes a stale one
/// otherwise.
pub(super) fn export(raw: &Path, model: &Model, out: &Path, id: &str) -> Result<()> {
    let path = out.join(format!("{id}.cloth.bin"));
    let mut set = ClothSet::default();
    for part in &model.parts {
        let Some(dir) = folder(raw, &part.label) else {
            continue;
        };
        let Some(file) = find_ci(&dir, &format!("{}_c.hkx", part.label)) else {
            continue;
        };
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let tag =
            TagFile::parse(&bytes, None).with_context(|| format!("parsing {}", file.display()))?;
        let mut log = |line: String| eprintln!("  {line}");
        let cloths = cloth::read(&tag, &part.label, &part.flver, &mut log)
            .with_context(|| format!("reading cloth of {}", file.display()))?;
        set.cloths.extend(cloths);
    }
    if set.cloths.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        return Ok(());
    }
    let bytes = set.to_bytes()?;
    std::fs::write(&path, &bytes)?;
    eprintln!(
        "  wrote {} ({} cloths, {} KiB)",
        path.display(),
        set.cloths.len(),
        bytes.len() / 1024
    );
    Ok(())
}
