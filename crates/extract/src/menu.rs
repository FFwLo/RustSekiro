//! Menu atlas sprites used by the HUD, decoded once into the derived cache.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use sekiro_formats::{bnd4, dvd::Vfs, menu_layout, tpf};

use crate::models::textures::{decode_rgba, encode_png};

pub fn export(vfs: &Vfs, cache: &Path) -> Result<()> {
    let binder = vfs.read("/menu/hi/01_common.sblytbnd.dcx")?;
    let entries = bnd4::parse(&binder)?;
    let layout = entries
        .iter()
        .find(|f| f.file_name() == "SB_FE.layout")
        .context("SB_FE.layout missing")?;
    let atlas = menu_layout::parse(&layout.contents(vfs.oodle())?)?;
    let bytes = vfs.read("/menu/hi/01_common.tpf.dcx")?;
    let textures = tpf::parse(&bytes)?;
    let texture = textures
        .iter()
        .find(|t| format!("{}.png", t.name) == atlas.image)
        .context("SB_FE texture missing")?;
    let header = texture.header()?;
    let pixels = decode_rgba(&header, texture.dds)?;
    let out = cache.join("menu/wire");
    std::fs::create_dir_all(&out)?;
    for name in [
        "MENU_Wire_Distance",
        "MENU_Wire_Marker",
        "MENU_Wire_01",
        "MENU_Wire_02",
        "MENU_Wire_Cursor",
    ] {
        let r = atlas
            .regions
            .get(&format!("{name}.png"))
            .context("wire sprite missing")?;
        ensure!(
            r.x.checked_add(r.width).is_some_and(|x| x <= header.width)
                && r.y
                    .checked_add(r.height)
                    .is_some_and(|y| y <= header.height),
            "wire sprite outside atlas"
        );
        let mut rgba = Vec::with_capacity((r.width * r.height * 4) as usize);
        for y in r.y..r.y + r.height {
            let start = ((y * header.width + r.x) * 4) as usize;
            rgba.extend_from_slice(&pixels[start..start + (r.width * 4) as usize]);
        }
        // half=1 stores texels at half the logical size. Expand after cropping, so
        // atlas coordinates always address the physical source. SB_FE has two
        // such regions (MENU_CA_Money and MENU_Saving); the five wire crops use 0.
        let scale = if r.half { 2 } else { 1 };
        if r.half {
            let mut expanded = Vec::with_capacity(rgba.len() * 4);
            for y in 0..r.height * scale {
                for x in 0..r.width * scale {
                    let at = (((y / scale) * r.width + x / scale) * 4) as usize;
                    expanded.extend_from_slice(&rgba[at..at + 4]);
                }
            }
            rgba = expanded;
        }
        std::fs::write(
            out.join(format!("{name}.png")),
            encode_png(r.width * scale, r.height * scale, &rgba)?,
        )?;
        eprintln!(
            "{name}: {}x{} -> {}",
            r.width * scale,
            r.height * scale,
            out.display()
        );
    }
    eprintln!("SB_FE: {} regions", atlas.regions.len());
    Ok(())
}
