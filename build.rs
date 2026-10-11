//! Build script: the exe's icon (what Explorer and the task bar show for `sv1.exe`), the same
//! red dot as the window icon (`src/icon_dot.rs`), written straight into a compiled resource
//! file (`.res`: RT_ICON images plus the RT_GROUP_ICON directory) and handed to the MSVC
//! linker, which takes `.res` inputs as they are. No resource compiler, no image file and no
//! extra crate are needed. Other targets (the GNU linker does not take `.res`) get no icon.

#[path = "src/icon_dot.rs"]
mod icon_dot;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon_dot.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("sv1_icon.res");
    std::fs::write(&out, res_file()).expect("write the icon resource");
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// A 32-bit DIB icon image (BITMAPINFOHEADER, bottom-up BGRA pixels, then the 1-bit AND
/// mask with 4-byte padded rows), the payload of one RT_ICON resource.
fn icon_image(size: u32) -> Vec<u8> {
    let px = icon_dot::dot(size);
    let mask_row = (size.div_ceil(32) * 4) as usize;
    let mut d = Vec::new();
    for v in [40u32, size, size * 2] {
        d.extend(v.to_le_bytes());
    }
    d.extend(1u16.to_le_bytes()); // planes
    d.extend(32u16.to_le_bytes()); // bits per pixel
    d.extend(0u32.to_le_bytes()); // BI_RGB
    d.extend((size * size * 4 + mask_row as u32 * size).to_le_bytes());
    d.extend([0u8; 16]); // resolution, colours used / important
    for y in (0..size).rev() {
        for x in 0..size {
            let p = &px[((y * size + x) * 4) as usize..][..4];
            d.extend([p[2], p[1], p[0], p[3]]);
        }
    }
    for y in (0..size).rev() {
        let mut row = vec![0u8; mask_row];
        for x in 0..size {
            if px[((y * size + x) * 4 + 3) as usize] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        d.extend(row);
    }
    d
}

/// One resource: the 32-byte header (ordinal type and name) and the padded data.
fn resource(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(32u32.to_le_bytes());
    for v in [0xFFFFu16, kind, 0xFFFF, id] {
        out.extend(v.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes()); // data version
    out.extend(0x1010u16.to_le_bytes()); // memory flags: moveable, discardable
    out.extend(0x0409u16.to_le_bytes()); // language: en-US, what rc.exe writes by default
    out.extend([0u8; 8]); // version, characteristics
    out.extend(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn res_file() -> Vec<u8> {
    let mut out = Vec::new();
    resource(&mut out, 0, 0, &[]); // the empty header entry every .res starts with
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&s| icon_image(s)).collect();
    for (i, img) in images.iter().enumerate() {
        resource(&mut out, RT_ICON, i as u16 + 1, img);
    }
    let mut group = Vec::new();
    group.extend(0u16.to_le_bytes());
    group.extend(1u16.to_le_bytes()); // ICON
    group.extend((SIZES.len() as u16).to_le_bytes());
    for (i, (&s, img)) in SIZES.iter().zip(&images).enumerate() {
        let dim = if s >= 256 { 0 } else { s as u8 };
        group.extend([dim, dim, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend((img.len() as u32).to_le_bytes());
        group.extend((i as u16 + 1).to_le_bytes());
    }
    resource(&mut out, RT_GROUP_ICON, 1, &group);
    out
}
