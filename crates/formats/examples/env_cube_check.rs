//! Read-only orientation check for the extracted sky cubes against the GPARAM sun.
use glam::{Quat, Vec3};

fn half(h: u16) -> f32 {
    let e = (h >> 10) & 31;
    let m = (h & 1023) as f32;
    if e == 0 {
        m * 2f32.powi(-24)
    } else {
        (1. + m / 1024.) * 2f32.powi(e as i32 - 15)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for map in ["m11_00_00_00", "m11_01_00_00"] {
        for variant in [1, 2, 3] {
            let mut scores = Vec::new();
            for entry in std::fs::read_dir(format!("cache/maps/{map}/env"))? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.ends_with(&format!("_{variant:02}.dds")) {
                    continue;
                }
                let bytes = std::fs::read(entry.path())?;
                let stride: usize = (0..8).map(|m| (128usize >> m).pow(2) * 8).sum();
                let mut luminance = Vec::new();
                for y in (32..96).step_by(4) {
                    for x in (32..96).step_by(4) {
                        let offset = 128 + 2 * stride + (y * 128 + x) * 8;
                        let rgb: [f32; 3] = std::array::from_fn(|c| {
                            half(u16::from_le_bytes(
                                bytes[offset + c * 2..offset + c * 2 + 2]
                                    .try_into()
                                    .unwrap(),
                            ))
                        });
                        luminance.push(rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722);
                    }
                }
                luminance.sort_by(f32::total_cmp);
                scores.push((luminance[luminance.len() / 2], name));
            }
            scores.sort_by(|a, b| b.0.total_cmp(&a.0));
            println!(
                "SKY CANDIDATES {map} variant {variant}: {:?}",
                &scores[..scores.len().min(8)]
            );
        }
    }
    for (map, probe) in [
        ("m11_00_00_00", 0),
        ("m11_00_00_00", 240),
        ("m11_01_00_00", 0),
        ("m11_01_00_00", 241),
    ] {
        let g = sekiro_formats::gparam::parse(&std::fs::read(format!(
            "cache/raw/param/drawparam/{}_0000.gparam",
            &map[..6]
        ))?)?;
        for (hour, variant) in [(6., 1), (12., 2), (18., 3)] {
            let bytes =
                std::fs::read(format!("cache/maps/{map}/env/gilm{probe}_{variant:02}.dds"))?;
            let mut best = (0f32, Vec3::ZERO);
            // Eight uncompressed mip levels, face-major DDS layout.
            let stride: usize = (0..8).map(|m| (128usize >> m).pow(2) * 8).sum();
            for face in 0..6 {
                for y in 0..128 {
                    for x in 0..128 {
                        let offset = 128 + face * stride + (y * 128 + x) * 8;
                        let rgb: [f32; 3] = std::array::from_fn(|c| {
                            half(u16::from_le_bytes(
                                bytes[offset + c * 2..offset + c * 2 + 2]
                                    .try_into()
                                    .unwrap(),
                            ))
                        });
                        let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
                        let u = (x as f32 + 0.5) / 64. - 1.;
                        let v = (y as f32 + 0.5) / 64. - 1.;
                        let d = match face {
                            0 => Vec3::new(1., -v, -u),
                            1 => Vec3::new(-1., -v, u),
                            2 => Vec3::new(u, 1., v),
                            3 => Vec3::new(u, -1., -v),
                            4 => Vec3::new(u, -v, 1.),
                            _ => Vec3::new(-u, -v, -1.),
                        }
                        .normalize();
                        if luma > best.0 {
                            best = (luma, d);
                        }
                    }
                }
            }
            let a = g.param("LightSet", "Angle0").unwrap().at(0, hour).unwrap();
            let from = Quat::from_rotation_y(a[1]) * Quat::from_rotation_x(a[0]) * Vec3::Z;
            let sun = Vec3::new(-from.x, from.y, from.z);
            println!(
                "{map} probe {probe} hour {hour}: brightest {:.3} toward {:?}; sun {:?}; difference {:.1} degrees (brightest may be a wall)",
                best.0,
                best.1,
                sun,
                best.1.angle_between(sun).to_degrees()
            );
        }
    }
    Ok(())
}
