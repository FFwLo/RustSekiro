//! HDR environment cubes and cosine-weighted Lambert diffuse radiance.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use glam::{Mat4, Quat, Vec3};
use sekiro_formats::{
    msb::Msb,
    tpf::{self, DdsFormat},
};
use serde_json::json;

use super::{bc6_hdr, mirror};

type Pixel = [f32; 4];
type Cube = [Vec<Pixel>; 6];

/// Retain the native signed/exponent packing, including alpha, without sRGB conversion.
fn export_volume(texture: &tpf::TpfTexture<'_>, path: &Path) -> Result<()> {
    let h = texture.header()?;
    ensure!(
        texture.kind == 2 && h.format == DdsFormat::Bc7 && !h.srgb,
        "GIIV must be a linear BC7 volume"
    );
    let source = h.top_volume_mip(texture.dds)?;
    let stride = h.top_mip_len().context("GIIV slice size")?;
    let mut bytes: Vec<u8> = [h.width, h.height, h.depth]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let mut pixels = vec![0; h.width as usize * h.height as usize];
    for slice in source.chunks_exact(stride) {
        texture2ddecoder::decode_bc7(slice, h.width as usize, h.height as usize, &mut pixels)
            .map_err(anyhow::Error::msg)?;
        for pixel in &pixels {
            let [b, g, r, a] = pixel.to_le_bytes();
            bytes.extend([r, g, b, a]);
        }
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn volume_name(name: &str) -> Option<(u32, u32, char)> {
    let mut parts = name.strip_prefix("giiv")?.split('_');
    let probe = parts.next()?.parse().ok()?;
    let variant = parts.next()?.parse::<u32>().ok()?;
    let axis = parts.next()?;
    if variant > 5 || !matches!(axis, "x" | "y" | "z" | "w") || parts.next().is_some() {
        return None;
    }
    Some((probe, variant, axis.chars().next()?))
}

/// Direct3D/WebGPU cube order: +X, -X, +Y, -Y, +Z, -Z.
fn direction(face: usize, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1., -v, -u),
        1 => Vec3::new(-1., -v, u),
        2 => Vec3::new(u, 1., v),
        3 => Vec3::new(u, -1., -v),
        4 => Vec3::new(u, -v, 1.),
        _ => Vec3::new(-u, -v, -1.),
    }
    .normalize()
}

fn sample(cube: &Cube, size: usize, d: Vec3) -> Pixel {
    let a = d.abs();
    let (f, u, v, major) = if a.x >= a.y && a.x >= a.z {
        if d.x > 0. {
            (0, -d.z, -d.y, a.x)
        } else {
            (1, d.z, -d.y, a.x)
        }
    } else if a.y >= a.z {
        if d.y > 0. {
            (2, d.x, d.z, a.y)
        } else {
            (3, d.x, -d.z, a.y)
        }
    } else if d.z > 0. {
        (4, d.x, -d.y, a.z)
    } else {
        (5, -d.x, -d.y, a.z)
    };
    let index = |x: f32| (((x / major + 1.) * 0.5 * size as f32) as usize).min(size - 1);
    cube[f][index(v) * size + index(u)]
}

fn half_to_float(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1. } else { 1. };
    let exponent = (h >> 10) & 31;
    let mantissa = (h & 1023) as f32;
    sign * if exponent == 0 {
        mantissa * 2f32.powi(-24)
    } else {
        (1. + mantissa / 1024.) * 2f32.powi(exponent as i32 - 15)
    }
}

fn float_to_half(f: f32) -> u16 {
    let bits = f.clamp(0., 65504.).to_bits();
    let exponent = ((bits >> 23) & 255) as i32 - 127 + 15;
    let mantissa = bits & 0x7fffff;
    if exponent <= 0 {
        if exponent < -10 {
            0
        } else {
            (((mantissa | 0x800000) + (1 << (13 - exponent))) >> (14 - exponent)) as u16
        }
    } else {
        (((exponent as u32) << 10) + ((mantissa + 0x1000) >> 13)) as u16
    }
}

fn decode(data: &[u8]) -> Result<Cube> {
    let h = tpf::Dds::parse(data)?;
    ensure!(
        h.cubemap && h.width == 128 && h.height == 128,
        "expected 128x128 cube"
    );
    ensure!(
        h.format == DdsFormat::Bc6hUf16,
        "expected unsigned BC6H, got {:?}",
        h.format
    );
    let stride: usize = (0..h.mip_count)
        .map(|m| {
            let n = (128usize >> m).max(1);
            n.div_ceil(4).pow(2) * 16
        })
        .sum();
    ensure!(
        data.len() >= h.data_offset + stride * 6,
        "truncated BC6H cube"
    );
    let mut cube: Cube = std::array::from_fn(|_| vec![[0.; 4]; 128 * 128]);
    for (face, pixels) in cube.iter_mut().enumerate() {
        let src = &data[h.data_offset + face * stride..];
        for by in 0..32 {
            for bx in 0..32 {
                let mut block = [[0u16; 4]; 16];
                let offset = (by * 32 + bx) * 16;
                bc6_hdr::decode_bc6_block(&src[offset..offset + 16], &mut block, false);
                for y in 0..4 {
                    for x in 0..4 {
                        pixels[(by * 4 + y) * 128 + bx * 4 + x] =
                            block[y * 4 + x].map(half_to_float);
                    }
                }
            }
        }
    }
    Ok(cube)
}

fn mirrored(cube: &Cube, size: usize) -> Cube {
    std::array::from_fn(|f| {
        (0..size * size)
            .map(|i| {
                let d = direction(
                    f,
                    (i % size) as f32 * 2. / size as f32 + 1. / size as f32 - 1.,
                    (i / size) as f32 * 2. / size as f32 + 1. / size as f32 - 1.,
                );
                sample(cube, size, Vec3::new(-d.x, d.y, d.z))
            })
            .collect()
    })
}

fn irradiance(cube: &Cube) -> Cube {
    // Cosine-distributed hemisphere samples integrate irradiance / pi.
    let samples: Vec<Vec3> = (0u32..256)
        .map(|i| {
            let r = ((i as f32 + 0.5) / 256.).sqrt();
            let phi = (i.reverse_bits() as f64 / 4294967296.) as f32 * std::f32::consts::TAU;
            Vec3::new(r * phi.cos(), r * phi.sin(), (1. - r * r).sqrt())
        })
        .collect();
    std::array::from_fn(|face| {
        (0..256)
            .map(|i| {
                let n = direction(
                    face,
                    ((i % 16) as f32 + 0.5) / 8. - 1.,
                    ((i / 16) as f32 + 0.5) / 8. - 1.,
                );
                let t = (if n.y.abs() < 0.99 { Vec3::Y } else { Vec3::X })
                    .cross(n)
                    .normalize();
                let b = n.cross(t);
                let mut sum = [0., 0., 0., 1.];
                for s in &samples {
                    let p = sample(cube, 128, t * s.x + b * s.y + n * s.z);
                    for c in 0..3 {
                        sum[c] += p[c] / samples.len() as f32;
                    }
                }
                sum
            })
            .collect()
    })
}

fn downsample(cube: &Cube, size: usize) -> Cube {
    let next = size / 2;
    std::array::from_fn(|f| {
        (0..next * next)
            .map(|i| {
                let (x, y) = ((i % next) * 2, (i / next) * 2);
                std::array::from_fn(|c| {
                    (cube[f][y * size + x][c]
                        + cube[f][y * size + x + 1][c]
                        + cube[f][(y + 1) * size + x][c]
                        + cube[f][(y + 1) * size + x + 1][c])
                        * 0.25
                })
            })
            .collect()
    })
}

fn write_dds(path: &Path, cube: Cube, size: usize, mips: bool) -> Result<()> {
    let levels = if mips { size.ilog2() + 1 } else { 1 };
    // Legacy D3DFMT_A16B16G16R16F avoids Bevy/ddsfile treating DX10 arraySize=1
    // as one array layer rather than the six faces of one cubemap.
    let mut header = [0u32; 32];
    header[0] = u32::from_le_bytes(*b"DDS ");
    header[1] = 124;
    header[2] = 0x2100f;
    header[3] = size as u32;
    header[4] = size as u32;
    header[5] = size as u32 * 8;
    header[7] = levels;
    header[19] = 32;
    header[20] = 4;
    header[21] = 113; // D3DFMT_A16B16G16R16F
    header[27] = 0x401008;
    header[28] = 0xfe00;
    let mut bytes: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    let mut chain = vec![cube];
    let mut n = size;
    for _ in 1..levels {
        chain.push(downsample(chain.last().unwrap(), n));
        n /= 2;
    }
    for f in 0..6 {
        for mip in &chain {
            for pixel in &mip[f] {
                for c in pixel {
                    bytes.extend(float_to_half(*c).to_le_bytes());
                }
            }
        }
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn export(raw: &Path, out: &Path, id: &str, msb: &Msb) -> Result<()> {
    super::textures::export_grading(raw, out, id)?;
    std::fs::create_dir_all(out.join("env"))?;
    let regions: Vec<_> = msb
        .env_regions
        .iter()
        .map(|r| {
            let [rx, ry, rz] = r.rotation.map(f32::to_radians);
            let transform = mirror(Mat4::from_rotation_translation(
                Quat::from_rotation_y(ry) * Quat::from_rotation_z(rz) * Quat::from_rotation_x(rx),
                r.position.into(),
            ));
            let (_, rotation, position) = transform.to_scale_rotation_translation();
            json!({ "name": r.name, "probe": r.probe, "position": position.to_array(),
            "rotation": rotation.to_array(), "size": r.size })
        })
        .collect();
    // Hirata splits GIIV volumes and GILM cubes into _00.tpf and _00_second.tpf.
    // Discover all envmap TPFs instead of assuming six unsuffixed containers.
    let mut paths: Vec<_> = std::fs::read_dir(raw.join(format!("map/{id}")))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| {
                    name.starts_with(&format!("{id}_envmap_")) && name.ends_with(".tpf")
                })
        })
        .collect();
    paths.sort();
    // Process containers in order to avoid concurrent writes if a cube is repeated.
    let counts: Vec<Result<usize>> = paths
        .iter()
        .map(|path| {
            let data = std::fs::read(path)?;
            let mut count = 0;
            for texture in tpf::parse(&data)? {
                let name = texture.name.to_ascii_lowercase();
                let Some(name) = name.strip_prefix(&format!("{id}_")).or_else(|| {
                    (name.starts_with("gilm") || name.starts_with("giiv")).then_some(name.as_str())
                }) else {
                    continue;
                };
                if let Some((probe, variant, axis)) = volume_name(name) {
                    export_volume(
                        &texture,
                        &out.join(format!("env/giiv{probe}_{variant:02}_{axis}.rgba8")),
                    )?;
                    continue;
                }
                if !name.starts_with("gilm") {
                    continue;
                }
                // TPF names are game-owned; only accept the expected numeric form for output paths.
                let Some((probe, suffix)) =
                    name.strip_prefix("gilm").and_then(|s| s.split_once('_'))
                else {
                    continue;
                };
                let probe: u32 = probe.parse().context("probe texture index")?;
                let variant: u32 = suffix.parse().context("probe texture variant")?;
                let cube = mirrored(
                    &decode(texture.dds).with_context(|| texture.name.clone())?,
                    128,
                );
                let diffuse = irradiance(&cube);
                write_dds(
                    &out.join(format!("env/gilm{probe}_{variant:02}.dds")),
                    cube,
                    128,
                    true,
                )?;
                write_dds(
                    &out.join(format!("env/gilm{probe}_{variant:02}_diffuse.dds")),
                    diffuse,
                    16,
                    false,
                )?;
                count += 1;
            }
            if count == 0 {
                eprintln!(
                    "warning: no GILM cubes in {}; retaining hemisphere ambient fallback",
                    path.display()
                );
            }
            Ok(count)
        })
        .collect();
    let count: usize = counts.into_iter().collect::<Result<Vec<_>>>()?.iter().sum();
    std::fs::write(
        out.join("env.json"),
        serde_json::to_vec_pretty(&json!({"regions": regions,
        "variant_hours": [0, 6, 12, 18, 22, 2]}))?,
    )?;
    eprintln!(
        "{id}: {count} HDR probe variants, {} probe regions",
        regions.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coefficient_names_require_explicit_axis_and_supported_variant() {
        assert_eq!(volume_name("giiv0346_03_w"), Some((346, 3, 'w')));
        for name in [
            "giiv../346_03_x",
            "giiv346_06_x",
            "giiv346_03_a",
            "giiv346_03_x_extra",
        ] {
            assert_eq!(volume_name(name), None);
        }
    }
    #[test]
    fn cube_axes_and_mirror() {
        let cube: Cube = std::array::from_fn(|f| vec![[f as f32, 0., 0., 1.]; 16]);
        for f in 0..6 {
            assert_eq!(sample(&cube, 4, direction(f, 0., 0.))[0], f as f32);
        }
        let flipped = mirrored(&cube, 4);
        assert_eq!(flipped[0][0][0], 1.);
        assert_eq!(flipped[1][0][0], 0.);
    }
    #[test]
    fn hdr_and_uniform_lambert() {
        for value in [0., 0.0001, 0.5, 1., 8., 100., 65504.] {
            assert!((half_to_float(float_to_half(value)) - value).abs() <= value * 0.001 + 1e-7);
        }
        let cube = std::array::from_fn(|_| vec![[8., 0.5, 1., 1.]; 128 * 128]);
        for face in irradiance(&cube) {
            for p in face {
                assert_eq!(p, [8., 0.5, 1., 1.]);
            }
        }
    }

    #[test]
    fn cached_bc6_cube_keeps_hdr_and_matches_decoder() {
        let path =
            sekiro_formats::cache::root().join("raw/map/m11_01_00_00/m11_01_00_00_envmap_02.tpf");
        if !path.exists() {
            return;
        }
        let data = std::fs::read(path).unwrap();
        let textures = tpf::parse(&data).unwrap();
        let texture = textures
            .iter()
            .find(|t| t.name.to_ascii_lowercase().contains("gilm"))
            .unwrap();
        let h = texture.header().unwrap();
        let cube = decode(texture.dds).unwrap();
        let mut reference = vec![0u32; 128 * 128];
        texture2ddecoder::decode_bc6_unsigned(
            h.top_mip(texture.dds).unwrap(),
            128,
            128,
            &mut reference,
        )
        .unwrap();
        for (p, r) in cube[0].iter().zip(reference) {
            let [b, g, rr, _] = r.to_le_bytes();
            for (float, byte) in p[..3].iter().zip([rr, g, b]) {
                assert!(((float.clamp(0., 1.) * 255.).round() - byte as f32).abs() <= 1.);
            }
        }
    }

    #[test]
    fn bc6_hdr_endpoint_is_not_clipped() {
        // Mode 00011 has six independent 10-bit RGB endpoints.
        // Max unsigned endpoints decode to half-float 65504, well beyond RGBA8.
        let mut encoded = [0u8; 16];
        encoded[0] = 3;
        for bit in 5..65 {
            encoded[bit / 8] |= 1 << (bit % 8);
        }
        let mut decoded = [[0u16; 4]; 16];
        bc6_hdr::decode_bc6_block(&encoded, &mut decoded, false);
        assert!(
            decoded
                .iter()
                .all(|p| p[..3].iter().all(|v| half_to_float(*v) == 65504.))
        );
    }
}
