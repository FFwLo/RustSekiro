//! Authored grass surfaces and six MSBS channel bindings, separate from canonical parts.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use glam::{Mat4, Quat, Vec3, Vec3Swizzles};
use sekiro_formats::{dcx::Oodle, flver, msb, reader::Reader};
use serde_json::{Value, json};

use super::{load_mtds, object_flver, piece_flver, pieces, textures};

#[derive(Clone, Debug)]
struct Vertex {
    position: Vec3,
    density: [f32; 6],
}

#[derive(Debug)]
struct Surface {
    vertices: Vec<Vertex>,
    faces: Vec<([usize; 3], Vec3)>,
}

impl Surface {
    fn transformed(&self, matrix: Mat4) -> Self {
        let normal = matrix.inverse().transpose();
        Self {
            vertices: self
                .vertices
                .iter()
                .map(|v| Vertex {
                    position: matrix.transform_point3(v.position),
                    density: v.density,
                })
                .collect(),
            faces: self
                .faces
                .iter()
                .map(|&(indices, n)| (indices, normal.transform_vector3(n).normalize_or(Vec3::Y)))
                .collect(),
        }
    }
}

fn local_placement(placement: &mut Value, inverse_parent: Mat4) -> Result<()> {
    let position: [f32; 3] = serde_json::from_value(placement["translation"].clone())?;
    let rotation: [f32; 4] = serde_json::from_value(placement["rotation"].clone())?;
    let scale: [f32; 3] = serde_json::from_value(placement["scale"].clone())?;
    let local = inverse_parent
        * Mat4::from_scale_rotation_translation(
            Vec3::from_array(scale),
            Quat::from_array(rotation),
            Vec3::from_array(position),
        );
    let (scale, rotation, translation) = local.to_scale_rotation_translation();
    *placement = json!({"translation":translation.to_array(),"rotation":rotation.to_array(),"scale":scale.to_array()});
    Ok(())
}

fn parse(data: &[u8]) -> Result<Surface> {
    let mut r = Reader::new(data);
    let header: Vec<_> = (0..10).map(|_| r.u32()).collect::<Result<_, _>>()?;
    ensure!(
        header[0..3] == [1, 40, 20]
            && header[4] == 36
            && header[6] == 24
            && header[8] == 24
            && header[3] == header[9],
        "invalid GRASS header"
    );
    let (volumes, vertices, faces) = (header[3] as usize, header[5] as usize, header[7] as usize);
    ensure!(
        40u64 + volumes as u64 * 44 + vertices as u64 * 36 + faces as u64 * 24 == data.len() as u64,
        "invalid GRASS table lengths"
    );
    r.skip(volumes * 20);
    let mut points = Vec::with_capacity(vertices);
    for _ in 0..vertices {
        let position = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        let mut density = [0.; 6];
        for d in &mut density {
            *d = r.f32()?;
            ensure!(d.is_finite() && *d >= 0., "invalid grass density");
        }
        ensure!(position.is_finite(), "nonfinite grass position");
        points.push(Vertex { position, density });
    }
    let mut triangles = Vec::with_capacity(faces);
    for _ in 0..faces {
        let normal = Vec3::new(r.f32()?, r.f32()?, r.f32()?);
        let indices = [r.u32()? as usize, r.u32()? as usize, r.u32()? as usize];
        ensure!(
            indices.iter().all(|&v| v < vertices) && normal.is_finite(),
            "invalid grass face"
        );
        triangles.push((indices, normal));
    }
    Ok(Surface {
        vertices: points,
        faces: triangles,
    })
}

fn channels(c: &msb::GrassConfigStruct) -> [i32; 6] {
    [
        c.unk00,
        c.unk04,
        c.grass_type_param_id,
        c.unk0c,
        c.unk10,
        c.unk14,
    ]
}

/// Stable integer generator. Retail seed/distribution remain untraced; no frame randomness.
struct Random(u64);
impl Random {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 40) as f32 / 16777216.
    }
}

fn number(row: &Value, field: &str) -> Result<f32> {
    let n = row[field]
        .as_f64()
        .with_context(|| format!("grass param {field}"))? as f32;
    ensure!(n.is_finite(), "nonfinite grass param {field}");
    Ok(n)
}

fn radical_inverse(mut index: u32, base: u32) -> f32 {
    let mut result = 0.;
    let mut factor = 1.;
    while index > 0 {
        factor /= base as f32;
        result += (index % base) as f32 * factor;
        index /= base;
    }
    result
}

/// The Halton path counts area * mean painted density * baseDensity, carries
/// fractional counts, then warps barycentrics by density + 1/3.
/// Native BVH leaf seeds, shared channel rounding and transform jitter remain approximations.
fn sample(surface: &Surface, channel: usize, row: &Value, seed: u64) -> Result<Vec<Value>> {
    let density = number(row, "baseDensity")?;
    let spacing = number(row, "spacing")?;
    ensure!(
        density >= 0. && spacing > 0.,
        "invalid grass density/spacing"
    );
    let mut random = Random(seed);
    let mut out = Vec::new();
    let mut remainder = 0.;
    let mut sequence = (seed % 33) as u32;
    for &(indices, normal) in &surface.faces {
        let [a, b, c] = indices.map(|i| &surface.vertices[i]);
        let area = (b.position - a.position)
            .cross(c.position - a.position)
            .length()
            * 0.5;
        let painted = [a.density[channel], b.density[channel], c.density[channel]];
        let expected = area * density * painted.iter().sum::<f32>() / 3.;
        ensure!(
            expected.is_finite() && expected < 1_000_000.,
            "excessive grass face density"
        );
        if expected == 0. {
            continue;
        }
        let mut points = Vec::new();
        match row["distributionType"]
            .as_u64()
            .context("grass distribution type")?
        {
            0 => {
                let expected = expected + remainder;
                let count = expected.floor() as usize;
                remainder = expected.fract();
                for _ in 0..count {
                    let u = radical_inverse(sequence, 2).sqrt();
                    let v = radical_inverse(sequence, 3);
                    sequence += 1;
                    let mut weights = [1. - u, u * (1. - v), u * v];
                    for (weight, painted) in weights.iter_mut().zip(painted) {
                        *weight *= painted + 1. / 3.;
                    }
                    let sum = weights.iter().sum::<f32>();
                    points.push(weights.map(|w| w / sum));
                }
            }
            1 => {
                // The native grid projects to XZ and interpolates the triangle height.
                let lo = a.position.min(b.position).min(c.position);
                let hi = a.position.max(b.position).max(c.position);
                let dz = spacing * 3f32.sqrt() * 0.5;
                let ab = (b.position - a.position).xz();
                let ac = (c.position - a.position).xz();
                let determinant = ab.perp_dot(ac);
                if determinant.abs() < 1e-8 {
                    continue;
                }
                for iz in (lo.z / dz).floor() as i32..=(hi.z / dz).ceil() as i32 {
                    let offset = if iz & 1 == 0 { spacing * 0.5 } else { 0. };
                    for ix in (lo.x / spacing).floor() as i32 - 1..=(hi.x / spacing).ceil() as i32 {
                        let delta = glam::Vec2::new(ix as f32 * spacing + offset, iz as f32 * dz)
                            - a.position.xz();
                        let v = delta.perp_dot(ac) / determinant;
                        let w = ab.perp_dot(delta) / determinant;
                        let weights = [1. - v - w, v, w];
                        if weights.iter().any(|&w| w < 0.) {
                            continue;
                        }
                        let coverage =
                            weights.iter().zip(painted).map(|(w, d)| w * d).sum::<f32>() * density;
                        if row["dithering"] == 3 || random.next() < coverage {
                            points.push(weights);
                        }
                    }
                }
            }
            kind => anyhow::bail!("unsupported grass distribution {kind}"),
        }
        for weights in points {
            let point = a.position * weights[0] + b.position * weights[1] + c.position * weights[2];
            let n = normal.try_normalize().unwrap_or(Vec3::Y);
            let angle = Vec3::Y.angle_between(n);
            let max_angle = number(row, "inclinationMax")?.to_radians();
            let influence = number(row, "normalInfluence")? * 0.01;
            let align = Quat::IDENTITY.slerp(
                Quat::from_rotation_arc(Vec3::Y, n),
                if angle > 0. {
                    influence * (max_angle / angle).min(1.)
                } else {
                    0.
                },
            );
            let orientation = number(row, "orientationAngle")?;
            let yaw = if orientation < 0. {
                random.next() * std::f32::consts::TAU
            } else {
                orientation.to_radians()
            };
            let rotation = align * Quat::from_rotation_y(yaw);
            let base = (number(row, "scaleBaseMin")?
                + random.next() * (number(row, "scaleBaseMax")? - number(row, "scaleBaseMin")?))
                * 0.01;
            let height = (number(row, "scaleHeightMin")?
                + random.next()
                    * (number(row, "scaleHeightMax")? - number(row, "scaleHeightMin")?))
                * 0.01;
            // Mirror local placement once, just as the parent and FLVER are mirrored.
            let q = rotation.to_array();
            out.push(json!({"translation":[-point.x,point.y,point.z], "rotation":[q[0],-q[1],-q[2],q[3]], "scale":[base,height,base]}));
        }
    }
    Ok(out)
}

pub(super) fn export(cache: &Path, id: &str, map: &msb::Msb, oodle: &'static Oodle) -> Result<()> {
    if !matches!(id, "m11_00_00_00" | "m11_01_00_00") {
        return Ok(());
    }
    let raw = cache.join("raw");
    let out = cache.join(format!("maps/{id}/grass"));
    std::fs::create_dir_all(out.join("models/tex"))?;
    let params: Value =
        serde_json::from_slice(&std::fs::read(cache.join("params/GrassTypeParam.json"))?)?;
    let rows: BTreeMap<_, _> = params["rows"]
        .as_array()
        .context("grass rows")?
        .iter()
        .map(|r| (r["id"].as_i64().unwrap_or(-1), r))
        .collect();
    let mut surfaces = BTreeMap::new();
    let mut bindings = Vec::new();
    let mut models = BTreeSet::new();
    let mut total = 0;
    let mut types = BTreeMap::<String, Value>::new();
    for (part_index, part) in map.parts.iter().enumerate() {
        if part.kind != msb::PartKind::MapPiece {
            continue;
        }
        let Some(config) = &part.grass_config else {
            continue;
        };
        let Some(model) = map.model(part) else {
            continue;
        };
        let path = piece_flver(&raw, id, &model.name).with_extension("grass");
        if !path.is_file() {
            continue;
        }
        if let std::collections::btree_map::Entry::Vacant(entry) =
            surfaces.entry(model.name.clone())
        {
            entry.insert(parse(&std::fs::read(&path)?)?);
        }
        // Retail applies the MapPiece transform before measuring area or laying
        // out the XZ grid. Express the finished world clump back under that root.
        let surface = surfaces[&model.name].transformed(part.matrix());
        let inverse_parent = super::mirror(part.matrix()).inverse();
        for (channel, type_id) in channels(config).into_iter().enumerate() {
            if type_id <= 0 || !surface.vertices.iter().any(|v| v.density[channel] > 0.) {
                continue;
            }
            let row = rows
                .get(&(type_id as i64))
                .context("missing grass type row")?;
            let name = row["modelName"].as_str().context("grass model name")?;
            ensure!(
                name.starts_with('o') && name[1..].bytes().all(|b| b.is_ascii_digit()),
                "invalid grass model name"
            );
            let seed_key = format!("{}-{channel}-{type_id}", model.name);
            let key = format!("{seed_key}-{part_index}");
            if let std::collections::btree_map::Entry::Vacant(entry) = types.entry(key.clone()) {
                let seed = seed_key
                    .bytes()
                    .fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64));
                let mut placements = sample(&surface, channel, row, seed)?;
                if placements.is_empty() {
                    continue;
                }
                for placement in &mut placements {
                    local_placement(placement, inverse_parent)?;
                }
                models.insert(name.to_owned());
                entry.insert(json!({"model":name,"type_id":type_id,"channel":channel,"params":row,"placements":placements}));
            }
            total += types[&key]["placements"].as_array().unwrap().len();
            bindings.push(
                json!({"part_index":part_index,"part_name":part.name,"surface":key,
                "parent_model":model.name,"parent_transform":super::transform_json(part)}),
            );
        }
    }
    let mtds = load_mtds(&raw)?;
    let mut store = textures::MapTextures::open(&raw, oodle)?;
    let mut parsed = BTreeMap::new();
    for model in &models {
        let path = object_flver(&raw, model);
        let f =
            flver::parse(&std::fs::read(&path).with_context(|| format!("grass model {model}"))?)?;
        let tpf = path.with_extension("tpf");
        if tpf.is_file() {
            store.add_tpf(&tpf)?;
        }
        parsed.insert(model.clone(), f);
    }
    let wanted: BTreeSet<_> = parsed
        .values()
        .flat_map(|f| pieces::wanted_textures(f, &mtds))
        .collect();
    let mut written = BTreeSet::new();
    for (name, normal) in wanted {
        let dds = store
            .dds(&name)?
            .with_context(|| format!("missing grass texture {name}"))?;
        let dds = if normal {
            textures::normal_to_bc5(&dds)?
        } else {
            dds
        };
        std::fs::write(out.join(format!("models/tex/{name}.dds")), dds)?;
        written.insert(name);
    }
    for (name, f) in parsed {
        let built = pieces::build(&f, &mtds, &|name| {
            written.contains(name).then(|| format!("tex/{name}.dds"))
        })?;
        ensure!(built.primitives > 0, "grass model {name} has no geometry");
        std::fs::write(out.join(format!("models/{name}.glb")), built.glb)?;
    }
    let report = json!({"version":1,"map":id,"clumps":total,"models":models,"bindings":bindings,"surfaces":types,
        "distribution":"Halton(2,3), area * mean painted density * baseDensity, density+1/3 barycentric warp; XZ hex grid uses spacing; approximate native seed/BVH grouping and hex dither",
        "wind":"authored params retained; runtime waveform untraced"});
    std::fs::write(out.join("grass.json"), serde_json::to_vec(&report)?)?;
    eprintln!(
        "  grass: {total} clumps, {} bindings, {} models, {} textures",
        bindings.len(),
        models.len(),
        written.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_channels_and_parent_space_survive_sampling() -> Result<()> {
        let vertices = vec![
            Vertex {
                position: Vec3::new(2., 3., 0.),
                density: [1., 0., 0., 0., 0., 0.],
            },
            Vertex {
                position: Vec3::new(4., 3., 0.),
                density: [1., 0., 0., 0., 0., 0.],
            },
            Vertex {
                position: Vec3::new(2., 3., 2.),
                density: [1., 0., 0., 0., 0., 0.],
            },
        ];
        let surface = Surface {
            vertices,
            faces: vec![([0, 1, 2], Vec3::Y)],
        };
        let row = json!({"baseDensity":1.,"distributionType":0,"spacing":0.5,"inclinationMax":30.,"normalInfluence":100.,"orientationAngle":-1.,"scaleBaseMin":100.,"scaleBaseMax":100.,"scaleHeightMin":100.,"scaleHeightMax":100.});
        let samples = sample(&surface, 0, &row, 123)?;
        assert_eq!(samples.len(), 2);
        assert_eq!(samples, sample(&surface, 0, &row, 123)?);
        assert!(sample(&surface, 1, &row, 123)?.is_empty());
        for p in &samples {
            let v: [f32; 3] = serde_json::from_value(p["translation"].clone())?;
            assert!(
                (-4. ..=-2.).contains(&v[0])
                    && (v[1] - 3.).abs() < 1e-5
                    && (0. ..=2.).contains(&v[2])
            );
            assert!(-v[0] - 2. + v[2] <= 2.00001);
        }
        let config = msb::GrassConfigStruct {
            unk00: 135,
            unk04: 136,
            grass_type_param_id: 127,
            unk0c: 132,
            unk10: 0,
            unk14: 0,
        };
        assert_eq!(channels(&config), [135, 136, 127, 132, 0, 0]);
        let parent = Mat4::from_scale_rotation_translation(
            Vec3::splat(2.),
            Quat::from_rotation_y(0.7),
            Vec3::new(20., 6., -3.),
        );
        let world_surface = surface.transformed(parent);
        let mut world_samples = sample(&world_surface, 0, &row, 123)?;
        assert!(
            (7..=8).contains(&world_samples.len()),
            "density must measure the scaled world area, with native fractional-count rounding"
        );
        let bevy_parent = super::super::mirror(parent);
        for placement in &mut world_samples {
            let original: [f32; 3] = serde_json::from_value(placement["translation"].clone())?;
            local_placement(placement, bevy_parent.inverse())?;
            let local: [f32; 3] = serde_json::from_value(placement["translation"].clone())?;
            let scale: [f32; 3] = serde_json::from_value(placement["scale"].clone())?;
            assert!(
                bevy_parent
                    .transform_point3(Vec3::from_array(local))
                    .abs_diff_eq(Vec3::from_array(original), 1e-4)
            );
            assert!(
                Vec3::from_array(scale).abs_diff_eq(Vec3::splat(0.5), 1e-5),
                "the terrain scale must not enlarge the clumps"
            );
        }
        Ok(())
    }

    #[test]
    fn installed_grass_tables_are_bounded() -> Result<()> {
        let cache = sekiro_formats::cache::root().join("raw/map");
        if !cache.is_dir() {
            return Ok(());
        }
        let mut files = 0;
        for id in ["m11_00_00_00", "m11_01_00_00"] {
            for dir in std::fs::read_dir(cache.join(id))? {
                let dir = dir?;
                if !dir.file_type()?.is_dir() {
                    continue;
                }
                for file in std::fs::read_dir(dir.path())? {
                    let path = file?.path();
                    if path.extension().is_some_and(|e| e == "grass") {
                        let bytes = std::fs::read(&path)?;
                        parse(&bytes).with_context(|| path.display().to_string())?;
                        assert!(parse(&bytes[..bytes.len() - 1]).is_err());
                        files += 1;
                    }
                }
            }
        }
        assert!(files > 200);
        Ok(())
    }
}
