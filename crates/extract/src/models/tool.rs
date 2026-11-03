//! Rope equipment export, from game data; model identification credited to FwLo (MIT).
use super::*;
use sekiro_formats::hkx::{self, Compendium, DecodeStats, TagFile, tagfile};

pub(super) fn export(cache: &Path, verbose: bool) -> Result<()> {
    let raw = cache.join("raw");
    let dir = raw.join("parts/wp_a_9500.partsbnd.d");
    let Some(path) = find_ci(&dir, "WP_A_9500.flver") else {
        return Ok(());
    };
    let flver = read_flver(&path)?;
    let visible = vec![true; flver.meshes.len()];
    let model = Model {
        skeleton: flver.clone(),
        parts: vec![Part {
            label: "WP_A_9500".into(),
            flver,
            tpfs: vec![dir.join("WP_A_9500.tpf")],
            visible,
            attach: None,
        }],
        notes: vec![],
        draw_masks: None,
    };
    std::fs::write(
        cache.join("models/wp_a_9500.glb"),
        build(&raw, &model, verbose)?,
    )?;
    let anim = dir.join("WP_A_9500.anibnd.d");
    let comp = Compendium::parse(&std::fs::read(anim.join("WP_A_9500.compendium"))?)?;
    let open = |p: &Path| -> Result<TagFile> {
        let bytes = std::fs::read(p)?;
        let external = tagfile::compendium_ref(&bytes)?.map(|_| &comp);
        Ok(TagFile::parse(&bytes, external)?)
    };
    let skeleton = hkx::skeleton_from_tagfile(&open(&anim.join("skeleton.HKX"))?)?;
    let out = cache.join("anim/wp_a_9500");
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("skeleton.bin"), skeleton.to_bytes()?)?;
    let mut count = 0;
    for entry in std::fs::read_dir(&anim)? {
        let p = entry?.path();
        let Some(name) = p.file_stem().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with('a') || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hkx")) {
            continue;
        }
        let clip = hkx::clip_from_tagfile(
            &open(&p)?,
            name,
            skeleton.bones.len(),
            &mut DecodeStats::default(),
        )?;
        std::fs::write(out.join(format!("{name}.bin")), clip.to_bytes()?)?;
        count += 1;
    }
    eprintln!(
        "  rope WP_A_9500: {} bones, {count} clips",
        skeleton.bones.len()
    );
    Ok(())
}
