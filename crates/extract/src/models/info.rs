//! `sekiro-extract model-info`: human-readable dumps of FLVER, TPF and MTD files.

use std::path::Path;

use anyhow::{Context, Result};
use sekiro_formats::{flver, mtd, tpf};

/// Prints a human-readable summary of a FLVER or TPF file.
pub fn info(path: &Path, dump: Option<&Path>) -> Result<()> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mtd"))
    {
        let m = mtd::parse(&data)?;
        println!("shader {} ({})", m.shader, m.description);
        for p in &m.params {
            println!("  param {} {} = {:?}", p.name, p.kind, p.value);
        }
        for t in &m.textures {
            println!(
                "  texture {} uv={} sdi={} path='{}'",
                t.kind, t.uv_number, t.shader_data_index, t.path
            );
        }
        return Ok(());
    }
    if tpf::is_tpf(&data) {
        for t in tpf::parse(&data)? {
            let h = t.header()?;
            println!(
                "{:<40} fmt={:3} {:?} srgb={} {}x{} mips={}",
                t.name, t.format, h.format, h.srgb, h.width, h.height, h.mip_count
            );
            if let Some(dir) = dump {
                std::fs::create_dir_all(dir)?;
                let rgba = super::textures::decode_rgba(&h, t.dds)?;
                let png = super::textures::encode_png(h.width, h.height, &rgba)?;
                std::fs::write(dir.join(format!("{}.png", t.name)), png)?;
            }
        }
        return Ok(());
    }
    let f = flver::parse(&data)?;
    println!(
        "version {:#x}, {} dummies, {} materials, {} nodes, {} meshes",
        f.version,
        f.dummies.len(),
        f.materials.len(),
        f.nodes.len(),
        f.meshes.len()
    );
    for (i, m) in f.materials.iter().enumerate() {
        println!("material {i}: {} | {}", m.name, m.mtd);
        for t in &m.textures {
            println!("    {} = '{}' scale {:?}", t.param, t.path, t.scale);
        }
    }
    for (i, m) in f.meshes.iter().enumerate() {
        let lod0 = m.lod0().map(|f| f.indices.len()).unwrap_or(0);
        let max_bone = m
            .vertices
            .bone_indices
            .iter()
            .flat_map(|b| b.iter())
            .max()
            .copied();
        let flags: Vec<String> = m
            .face_sets
            .iter()
            .map(|f| format!("{:x}", f.flags))
            .collect();
        let layouts: Vec<usize> = m.buffers.iter().map(|b| b.layout).collect();
        for (c, uv) in m.vertices.uvs.iter().enumerate() {
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for p in uv {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            println!("mesh {i} uv{c}: {lo:?}..{hi:?}");
        }
        if let Some(fs) = m.lod0() {
            println!(
                "mesh {i} lod0: strip={} cull={}",
                fs.triangle_strip, fs.cull_backfaces
            );
        }
        println!(
            "mesh {i}: dyn={} mat={} ({}) node={} bones={} verts={} lod0_idx={} maxbone={:?} uvs={} tans={} cols={} fs=[{}] layouts={:?}",
            m.dynamic,
            m.material,
            f.materials[m.material].name,
            m.node,
            m.bone_indices.len(),
            m.vertices.len(),
            lod0,
            max_bone,
            m.vertices.uvs.len(),
            m.vertices.tangents.len(),
            m.vertices.colors.len(),
            flags.join(","),
            layouts
        );
    }
    for (i, l) in f.layouts.iter().enumerate() {
        let s: Vec<String> = l
            .iter()
            .map(|m| format!("{:?}:{:?}#{}", m.semantic, m.kind, m.index))
            .collect();
        println!("layout {i}: {}", s.join(" "));
    }
    for (i, n) in f.nodes.iter().enumerate() {
        println!(
            "node {i}: {} parent={} flags={:#x} t={:?} r={:?} s={:?}",
            n.name, n.parent, n.flags, n.translation, n.rotation, n.scale
        );
    }
    for (i, d) in f.dummies.iter().enumerate() {
        println!(
            "dummy {i}: ref={} parent={} attach={} pos={:?} fwd={:?} up={:?}",
            d.reference_id, d.parent_bone, d.attach_bone, d.position, d.forward, d.upward
        );
    }
    Ok(())
}
