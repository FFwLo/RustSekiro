//! `sekiro-extract map <id>`: one map area to `cache/maps/<id>/`:
//!
//! - `models/<model>.glb`: one static GLB per map piece and object model the MSB places
//!   (LOD0 only),
//! - `models/tex/<name>.dds`: the textures those GLBs reference, as the original DDS data
//!   (next to the GLBs so viewers that forbid `..` in asset paths load them),
//! - `layout.json`: map piece instances, objects, enemies, player starts, in Bevy space,
//! - `collision.bin`: the hit collision (`h*` binder) as one triangle mesh in Bevy space
//!   (`sekiro_formats::hknp::CollisionMesh` in postcard),
//! - `object_collision.bin`: the static collision placed objects carry in their objbnds
//!   (`sekiro_formats::hknp::ObjectCollision` in postcard, `docs/OBJECT-COLLISION.md`),
//! - `fall_assist.bin`: the fall-prevention assist walls (`f*` binder) per collision part
//!   (`sekiro_formats::hknp::PartCollision` in postcard, `docs/OBJECT-COLLISION.md`),
//! - `collision_lo.bin`: the low-resolution hit collision (`l*` binder) per collision part, the
//!   one characters and the camera collide with (`sekiro_formats::hknp::PartCollision` in
//!   postcard, `docs/HIT-FILTERS.md`).
//!
//! Every position and transform is mirrored across X like the model export (docs/FORMATS.md).

mod bc6_hdr;
mod edges;
mod env;
mod grass;
mod layout;
mod lights;
mod material_audit;
mod object_collision;
mod pieces;
mod sky;
mod textures;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use rayon::prelude::*;
use sekiro_formats::{bxf4, dcx::Oodle, flver, hknp, hkx::Compendium, msb, mtd};
use serde_json::{Value, json};

use pieces::Mtds;
use textures::MapTextures;

/// Mirror across X: FromSoftware left-handed space to Bevy's right-handed space.
fn mirror(m: Mat4) -> Mat4 {
    let s = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    s * m * s
}

/// Appends `mesh`, in FromSoftware space, to `out` placed at `part` in Bevy space: mirrored
/// first, then the mirrored part matrix.
fn extend_placed(out: &mut hknp::CollisionMesh, mesh: &hknp::CollisionMesh, part: &msb::Part) {
    let world = mirror(part.matrix());
    let base = out.vertices.len() as u32;
    out.vertices.extend(mesh.vertices.iter().map(|v| {
        world
            .transform_point3(Vec3::new(-v[0], v[1], v[2]))
            .to_array()
    }));
    out.indices.extend(mesh.indices.iter().map(|i| i + base));
    out.materials.extend_from_slice(&mesh.materials);
}

/// A part's transform in Bevy space as JSON (translation, rotation quaternion xyzw, scale).
fn transform_json(part: &msb::Part) -> Value {
    layout::transform_json(part)
}

/// Yaw in degrees about +Y in Bevy space (the mirror negates FromSoftware's Y rotation).
fn yaw_degrees(part: &msb::Part) -> f32 {
    -part.rotation[1]
}

pub fn summary(cache: &Path, id: &str) -> Result<()> {
    let m = read_msb(cache, id)?;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for p in &m.parts {
        *kinds.entry(format!("{:?}", p.kind)).or_default() += 1;
    }
    println!(
        "{} models, {} parts: {kinds:?}",
        m.models.len(),
        m.parts.len()
    );
    let mut chrs: BTreeMap<String, usize> = BTreeMap::new();
    for p in &m.parts {
        if p.kind == msb::PartKind::Enemy {
            *chrs
                .entry(m.model(p).map(|m| m.name.clone()).unwrap_or_default())
                .or_default() += 1;
        }
    }
    println!("enemies by model: {chrs:?}");
    let mut names: BTreeMap<i16, usize> = BTreeMap::new();
    for p in &m.parts {
        if let Some(c) = &p.collision {
            *names.entry(c.map_name_id).or_default() += 1;
        }
    }
    println!("collision parts by map name id: {names:?}");
    let mut sets: BTreeMap<[i32; 4], usize> = BTreeMap::new();
    for p in &m.parts {
        if let Some(g) = p.gparam {
            *sets.entry(g).or_default() += 1;
        }
    }
    println!("parts by gparam [light set, fog, scattering, env map]: {sets:?}");
    for p in m.parts.iter().filter(|p| p.kind == msb::PartKind::Player) {
        println!(
            "player start {} at {:?}, yaw {}",
            p.name, p.position, p.rotation[1]
        );
    }
    Ok(())
}

fn read_msb(cache: &Path, id: &str) -> Result<msb::Msb> {
    let path = cache.join(format!("raw/map/mapstudio/{id}.msb"));
    let data = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    msb::parse(&data).with_context(|| format!("parsing {}", path.display()))
}

/// Rewrite only `object_collision.bin` from the MSB and the unpacked object binders.
pub fn export_object_collision(cache: &Path, id: &str) -> Result<()> {
    object_collision::export(cache, id, &read_msb(cache, id)?)
}

/// Rewrite only `fall_assist.bin` from the MSB and the map's `f` hit binder in cache/raw.
/// Needs the game folder for Oodle (read only).
pub fn export_fall_assist(game: &Path, cache: &Path, id: &str) -> Result<()> {
    let oodle = Oodle::load(game).context("the fall-assist export needs the game's Oodle DLL")?;
    let out = cache.join("maps").join(id);
    std::fs::create_dir_all(&out)?;
    write_part_binder(
        &cache.join("raw"),
        &out,
        id,
        &read_msb(cache, id)?,
        oodle,
        &FALL_ASSIST,
    )
}

/// Rewrite only `collision_lo.bin` from the MSB and the map's `l` hit binder in cache/raw.
/// Needs the game folder for Oodle (read only).
pub fn export_lo_collision(game: &Path, cache: &Path, id: &str) -> Result<()> {
    let oodle = Oodle::load(game)
        .context("the low-resolution collision export needs the game's Oodle DLL")?;
    let out = cache.join("maps").join(id);
    std::fs::create_dir_all(&out)?;
    write_part_binder(
        &cache.join("raw"),
        &out,
        id,
        &read_msb(cache, id)?,
        oodle,
        &LO_COLLISION,
    )
}

/// A hit binder [`write_part_binder`] places per collision part into its own cache file
/// (`sekiro_formats::hknp::PartCollision`).
struct PartBinder {
    /// The binder's prefix: `map/<id>/<prefix><id without its m>.hkxbhd`.
    prefix: &'static str,
    /// The file in `cache/maps/<id>/`.
    file: &'static str,
    /// What the log calls it.
    label: &'static str,
    /// Whether the file stands for every part or none: a model that fails to decode fails the
    /// export rather than leaving its parts out.
    whole: bool,
    decode: fn(&[u8], Option<&Compendium>) -> sekiro_formats::Result<HitDecode>,
}

type HitDecode = (hknp::CollisionMesh, hknp::DecodeStats);

/// The fall-prevention assist walls (`f<map>.hkxbhd`): the exe mounts the binder as `faHKXBHD`
/// next to the `hi` and `lo` hit binders and loads it as `m_fallAssistantHitFileCap`, one
/// extern mesh file per collision model (docs/OBJECT-COLLISION.md).
const FALL_ASSIST: PartBinder = PartBinder {
    prefix: "f",
    file: "fall_assist.bin",
    label: "fall assist",
    whole: false,
    decode: hknp::decode_extern,
};

/// The low-resolution hit collision (`l<map>.hkxbhd`, the exe's `loHKXBHD`): the same
/// compressed meshes as `collision.bin`'s `h` binder with about a seventh of the triangles.
/// The exe gives an ordinary part's `l` body group 39 or 40, which the player, enemies and the
/// camera collide with, and its `h` body group 38, which none of them do (docs/HIT-FILTERS.md,
/// #189). Whole: a part left out of it would be a hole in the floor characters walk on, so a
/// model that fails to decode leaves no `collision_lo.bin` written and characters on the
/// block's `collision.bin`.
const LO_COLLISION: PartBinder = PartBinder {
    prefix: "l",
    file: "collision_lo.bin",
    label: "low-resolution collision",
    whole: true,
    decode: hknp::decode,
};

/// Writes `binder`'s cache file: each MSB collision part's model from the map's binder, placed
/// by the part like `collision.bin`, with the part's triangle range. A model the binder has no
/// file for adds nothing, and its parts are listed in the log. A model that fails to decode
/// is left out with a warning, or, for a [`PartBinder::whole`] binder, fails the export and
/// leaves the last file as it was. A map without the binder has none, so an old file is
/// removed. The file is written whole or not at all.
fn write_part_binder(
    raw: &Path,
    out: &Path,
    id: &str,
    msb: &msb::Msb,
    oodle: &'static Oodle,
    binder: &PartBinder,
) -> Result<()> {
    let PartBinder {
        prefix,
        file,
        label,
        whole,
        decode,
    } = *binder;
    let path = out.join(file);
    let decode = |data: &[u8], compendium: Option<&Compendium>| Ok(decode(data, compendium)?);
    let Some(hits) = place_hit_binder(raw, id, msb, oodle, prefix, decode)? else {
        let binder = format!("{prefix}{}.hkxbhd", &id[1..]);
        // A file from an older export would hold collision the map's data no longer has.
        match std::fs::remove_file(&path) {
            Ok(()) => eprintln!("  {label}: no {binder}, removed the old {}", path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("  {label}: no {binder}, skipped");
            }
            Err(e) => return Err(e).with_context(|| format!("removing {}", path.display())),
        }
        return Ok(());
    };
    if whole && !hits.failed.is_empty() {
        anyhow::bail!(
            "{} of {} models failed to decode ({}), so {} was not written: characters keep the              last one, or {id}'s collision.bin without one",
            hits.failed.len(),
            hits.models,
            hits.failed.join(", "),
            path.display()
        );
    }
    let placed: BTreeSet<&str> = hits.ranges.iter().map(|(n, _)| n.as_str()).collect();
    let without: Vec<&str> = msb
        .parts
        .iter()
        .filter(|p| p.kind == msb::PartKind::Collision && !placed.contains(p.name.as_str()))
        .map(|p| p.name.as_str())
        .collect();
    let collision = hknp::PartCollision {
        parts: hits
            .ranges
            .iter()
            .filter(|(_, [_, count])| *count > 0)
            .map(|(name, [first, count])| hknp::PartRange {
                name: name.clone(),
                first: *first as u32,
                count: *count as u32,
            })
            .collect(),
        mesh: hits.mesh,
    };
    let partial = out.join(format!("{file}.partial"));
    std::fs::write(&partial, collision.to_bytes()?)
        .with_context(|| format!("writing {}", partial.display()))?;
    std::fs::rename(&partial, &path).with_context(|| format!("writing {}", path.display()))?;
    eprintln!(
        "  {label}: {} triangles over {} of {} collision parts ({} models, {} failed, {} bodies, {} bodies of other shapes skipped, {} empty primitive slots); {} parts without a model in the binder{}",
        collision.mesh.triangle_count(),
        collision.parts.len(),
        hits.parts,
        hits.models,
        hits.failed.len(),
        hits.stats.bodies,
        hits.stats.other_shapes,
        hits.stats.convex_skipped,
        without.len(),
        if without.len() <= 16 {
            format!(": {without:?}")
        } else {
            String::new()
        },
    );
    Ok(())
}

/// One of a map's hit binders placed by the MSB's collision parts ([`place_hit_binder`]).
struct PlacedHits {
    /// Every placed part's collision, in Bevy space.
    mesh: hknp::CollisionMesh,
    /// Each placed part's name and `[first triangle, triangle count]` in `mesh`, in MSB order.
    ranges: Vec<(String, [usize; 2])>,
    /// The MSB's collision parts.
    parts: usize,
    /// The distinct models those parts use, and those of them that failed to decode.
    models: usize,
    failed: Vec<String>,
    stats: hknp::DecodeStats,
}

/// Reads map `id`'s hit binder `<prefix><tail>.hkxbhd` / `.hkxbdt` from `raw` (`h` and `l` for
/// the high- and low-resolution hit collision, `f` for the fall-prevention assist walls),
/// decodes the file of each MSB collision part's model with `decode` in parallel, and places each part's collision like any
/// other part ([`extend_placed`]). A model whose file fails to decode is warned about and its
/// parts left out; one the binder has no file for is left out silently. `None` when the map has
/// no such binder (its `.hkxbhd` is not there); any other read error is an error.
fn place_hit_binder(
    raw: &Path,
    id: &str,
    msb: &msb::Msb,
    oodle: &'static Oodle,
    prefix: &str,
    decode: impl Fn(&[u8], Option<&Compendium>) -> Result<(hknp::CollisionMesh, hknp::DecodeStats)>
    + Sync,
) -> Result<Option<PlacedHits>> {
    let tail = &id[1..];
    let dir = raw.join(format!("map/{id}"));
    let path = |ext: &str| dir.join(format!("{prefix}{tail}.{ext}"));
    let bhd = match std::fs::read(path("hkxbhd")) {
        Ok(bhd) => bhd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path("hkxbhd").display())),
    };
    let bdt = std::fs::read(path("hkxbdt"))
        .with_context(|| format!("reading {}", path("hkxbdt").display()))?;
    let files = bxf4::parse(&bhd, &bdt)?;
    let compendium = match files.iter().find(|f| f.file_name().contains("compendium")) {
        Some(f) => Some(Compendium::parse(&f.contents(Some(oodle))?)?),
        None => None,
    };
    let by_name: HashMap<String, &bxf4::BxfFile> = files
        .iter()
        .map(|f| (f.file_name().to_ascii_lowercase(), f))
        .collect();
    let parts: Vec<&msb::Part> = msb
        .parts
        .iter()
        .filter(|p| p.kind == msb::PartKind::Collision)
        .collect();
    let wanted: BTreeSet<String> = parts
        .iter()
        .filter_map(|p| msb.model(p).map(|m| m.name.clone()))
        .collect();
    let decoded: Vec<(String, Result<(hknp::CollisionMesh, hknp::DecodeStats)>)> = wanted
        .par_iter()
        .filter_map(|model| {
            let digits = model.trim_start_matches('h');
            let f = by_name.get(&format!("{prefix}{tail}_{digits}.hkx.dcx"))?;
            let r = f
                .contents(Some(oodle))
                .map_err(anyhow::Error::from)
                .and_then(|data| decode(&data, compendium.as_ref()))
                .with_context(|| format!("decoding {}", f.name));
            Some((model.clone(), r))
        })
        .collect();
    let mut by_model = HashMap::new();
    let mut stats = hknp::DecodeStats::default();
    let mut failed = Vec::new();
    for (model, r) in decoded {
        match r {
            Ok((m, s)) => {
                stats += &s;
                by_model.insert(model, m);
            }
            Err(e) => {
                eprintln!("  warning: {prefix}{tail}.hkxbhd: {e:#}");
                failed.push(model);
            }
        }
    }
    let mut mesh = hknp::CollisionMesh::default();
    let mut ranges = Vec::new();
    for p in &parts {
        let Some(model) = msb.model(p).and_then(|m| by_model.get(&m.name)) else {
            continue;
        };
        ranges.push((
            p.name.clone(),
            [mesh.triangle_count(), model.triangle_count()],
        ));
        extend_placed(&mut mesh, model, p);
    }
    Ok(Some(PlacedHits {
        mesh,
        ranges,
        parts: parts.len(),
        models: wanted.len(),
        failed,
        stats,
    }))
}

/// Upgrade placement metadata without changing geometry, textures or collision.
pub fn refresh_layout(cache: &Path, id: &str) -> Result<()> {
    let path = cache.join(format!("maps/{id}/layout.json"));
    let mut exported: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let msb = read_msb(cache, id)?;
    let audit: Value = serde_json::from_slice(&std::fs::read(
        cache.join(format!("maps/{id}/material-audit.json")),
    )?)?;
    let mut statuses = BTreeMap::new();
    for model in audit["models"]
        .as_array()
        .context("material audit models")?
    {
        let name = model["model"].as_str().context("audit model name")?;
        let status = if exported["models"].get(name).is_some() {
            "exported"
        } else {
            match model["classification"].as_str() {
                Some(
                    "meshless_helper"
                    | "invisible_shadow_helper"
                    | "fog_renderer_handoff"
                    | "no_exportable_lod0",
                ) => "no_exported_primitives",
                Some("physics_only_binder") => "physics_only",
                _ => "model_not_exported",
            }
        };
        statuses.insert(name.to_owned(), status.to_owned());
    }
    let ranges: HashMap<_, _> = exported["collisions"]
        .as_array()
        .context("collision rows")?
        .iter()
        .filter_map(|row| {
            Some((
                row["name"].as_str()?.to_owned(),
                [
                    row["triangles"][0].as_u64()? as usize,
                    row["triangles"][1].as_u64()? as usize,
                ],
            ))
        })
        .collect();
    layout::extend(
        &mut exported,
        &msb,
        &statuses,
        &ranges,
        &layout::map_inventory(&cache.join("raw"))?,
    );
    std::fs::write(path, serde_json::to_vec_pretty(&exported)?)?;
    println!(
        "{id}: refreshed {} canonical parts without re-exporting assets",
        msb.parts.len()
    );
    Ok(())
}

fn load_mtds(raw: &Path) -> Result<Mtds> {
    let dir = raw.join("mtd/allmaterialbnd.mtdbnd.d");
    let mut out = HashMap::new();
    for e in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with(".mtd") {
            continue;
        }
        if let Ok(m) = mtd::parse(&std::fs::read(e.path())?) {
            out.insert(name, m);
        }
    }
    Ok(Mtds(out))
}

/// `m000003` in map `m11_00_00_00` -> `raw/map/m11_00_00_00/m11_00_00_00_000003.mapbnd.d/...flver`.
fn piece_flver(raw: &Path, id: &str, model: &str) -> PathBuf {
    let digits = model.trim_start_matches('m');
    raw.join(format!(
        "map/{id}/{id}_{digits}.mapbnd.d/{id}_{digits}.flver"
    ))
}

/// `o000100` -> `raw/obj/o000100.objbnd.d/o000100.flver`.
fn object_flver(raw: &Path, model: &str) -> PathBuf {
    raw.join(format!("obj/{model}.objbnd.d/{model}.flver"))
}

/// Compatibility projection only: never remove canonical parts.
fn legacy_excludes_ungrouped(has_collision: bool, masks: &[u32]) -> bool {
    has_collision && masks.get(8..16).is_some_and(|g| g.iter().all(|&w| w == 0))
}

/// An authored physics-only object binder is valid even though it has no render model.
fn physics_only_members<'a>(names: impl IntoIterator<Item = &'a str>) -> bool {
    let mut physics = false;
    for name in names {
        let name = name.to_ascii_lowercase();
        if name.ends_with(".flver") || name.ends_with(".flver.dcx") {
            return false;
        }
        physics |= name.ends_with(".hkx") || name.ends_with(".hkx.dcx");
    }
    physics
}

pub fn export(game: &Path, cache: &Path, id: &str, selected_models: &[String]) -> Result<()> {
    let raw = cache.join("raw");
    let out = cache.join("maps").join(id);
    std::fs::create_dir_all(out.join("models"))?;
    std::fs::create_dir_all(out.join("models/tex"))?;
    let oodle = Oodle::load(game).context("the map export needs the game's Oodle DLL")?;
    if std::env::var_os("SEKIRO_MAP_SKY_ONLY").is_some() {
        anyhow::ensure!(
            id == "m89_00_00_99",
            "sky-only export requires m89_00_00_99"
        );
        return sky::export(&raw, &out, oodle);
    }
    let msb = read_msb(cache, id)?;
    if std::env::var_os("SEKIRO_MAP_GRASS_ONLY").is_some() {
        return grass::export(cache, id, &msb, oodle);
    }
    if selected_models.is_empty() {
        lights::export(&raw, &out, id)?;
        if std::env::var_os("SEKIRO_MAP_LIGHTS_ONLY").is_some() {
            return Ok(());
        }
        env::export(&raw, &out, id, &msb)?;
        if std::env::var_os("SEKIRO_MAP_ENV_ONLY").is_some() {
            return Ok(());
        }
        let edge_header = raw.join(format!("map/{id}/{id}.edgebhd"));
        if edge_header.is_file() || msb.parts.iter().any(|p| p.kind == msb::PartKind::Collision) {
            if edges::export(cache, id, &msb, oodle)? {
                return Ok(());
            }
        } else {
            eprintln!("{id}: collisionless layout has no authored edge archive");
        }
    }
    let mtds = load_mtds(&raw)?;

    // Map piece and object models actually placed, with their FLVER (and object TPF).
    let mut used: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut instances = [0usize; 2];
    for p in &msb.parts {
        let Some(m) = msb.model(p) else { continue };
        if !selected_models.is_empty() && !selected_models.contains(&m.name) {
            continue;
        }
        let path = match p.kind {
            msb::PartKind::MapPiece => {
                instances[0] += 1;
                piece_flver(&raw, id, &m.name)
            }
            msb::PartKind::Object => {
                instances[1] += 1;
                object_flver(&raw, &m.name)
            }
            _ => continue,
        };
        used.entry(m.name.clone()).or_insert(path);
    }
    eprintln!(
        "{id}: {} map piece and {} object instances of {} models",
        instances[0],
        instances[1],
        used.len()
    );
    for name in selected_models {
        anyhow::ensure!(
            used.contains_key(name),
            "model {name} is not placed in {id}"
        );
    }

    // Pass 1: which textures do the pieces want?
    let wanted: BTreeSet<(String, bool)> = used
        .par_iter()
        .filter_map(|(_, path)| {
            let data = std::fs::read(path).ok()?;
            let f = flver::parse(&data).ok()?;
            Some(pieces::wanted_textures(&f, &mtds))
        })
        .flatten()
        .collect::<Vec<_>>()
        .into_iter()
        .collect();

    // Pass 2: write the textures that exist.
    let mut store = MapTextures::open(&raw, oodle)?;
    // Objects carry their own textures next to their FLVER.
    for path in used.values() {
        let tpf = path.with_extension("tpf");
        if tpf.is_file() {
            store.add_tpf(&tpf)?;
        }
    }
    eprintln!(
        "  {} textures wanted, {} indexed",
        wanted.len(),
        store.len()
    );
    let written: BTreeSet<String> = wanted
        .par_iter()
        .filter_map(|(name, is_normal)| {
            let path = out.join("models/tex").join(format!("{name}.dds"));
            if path.is_file() {
                return Some(name.clone());
            }
            let dds = store
                .dds(name.strip_suffix("_map_raw").unwrap_or(name))
                .and_then(|d| match d {
                    Some(d) if *is_normal => textures::normal_to_bc5(&d).map(Some),
                    other => Ok(other),
                });
            match dds {
                Ok(Some(dds)) => match std::fs::write(&path, dds) {
                    Ok(()) => Some(name.clone()),
                    Err(e) => {
                        eprintln!("  warning: writing {}: {e}", path.display());
                        None
                    }
                },
                Ok(None) => None,
                Err(e) => {
                    eprintln!("  warning: texture {name}: {e:#}");
                    None
                }
            }
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect();
    let missing: Vec<&String> = wanted
        .iter()
        .map(|(n, _)| n)
        .filter(|n| !written.contains(*n))
        .collect();
    eprintln!(
        "  wrote {} textures, {} missing{}",
        written.len(),
        missing.len(),
        if missing.is_empty() {
            String::new()
        } else {
            format!(" (e.g. {:?})", missing.iter().take(8).collect::<Vec<_>>())
        }
    );

    // Pass 3: one GLB per model.
    let failed = AtomicUsize::new(0);
    let uri = |name: &str| -> Option<String> {
        written.contains(name).then(|| format!("tex/{name}.dds"))
    };
    let outcomes: Vec<(String, (Option<Value>, String))> = used
        .par_iter()
        .map(|(model, path)| {
            let result = (|| -> Result<_> {
                if model.starts_with('o') && !path.is_file() {
                    let binder = path
                        .parent()
                        .context("object model has no parent")?
                        .with_extension("");
                    if binder.is_file() {
                        let bytes = std::fs::read(&binder)?;
                        let files = sekiro_formats::bnd4::parse(&bytes)?;
                        if physics_only_members(files.iter().map(|f| f.file_name())) {
                            return Ok((None, "physics_only".to_owned()));
                        }
                    }
                }
                let data =
                    std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
                let f = flver::parse(&data)?;
                let built = pieces::build(&f, &mtds, &uri)?;
                if built.primitives == 0 {
                    return Ok((None, "no_exported_primitives".to_owned()));
                }
                std::fs::write(out.join(format!("models/{model}.glb")), &built.glb)?;
                Ok((
                    Some(json!({
                        "glb": format!("models/{model}.glb"),
                        "triangles": built.triangles,
                        "primitives": built.primitives,
                        "min": built.bbox_min,
                        "max": built.bbox_max,
                    })),
                    "exported".to_owned(),
                ))
            })();
            match result {
                Ok(outcome) => (model.clone(), outcome),
                Err(e) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    eprintln!("  warning: {model}: {e:#}");
                    (
                        model.clone(),
                        (
                            None,
                            if path.exists() {
                                "model_export_failed"
                            } else {
                                "missing_model_asset"
                            }
                            .to_owned(),
                        ),
                    )
                }
            }
        })
        .collect();
    let models: BTreeMap<String, Value> = outcomes
        .iter()
        .filter_map(|(name, (model, _))| model.as_ref().map(|m| (name.clone(), m.clone())))
        .collect();
    let model_status: BTreeMap<String, String> = outcomes
        .into_iter()
        .map(|(name, (_, status))| (name, status))
        .collect();
    eprintln!(
        "  wrote {} GLBs ({} failed, {} with nothing visible)",
        models.len(),
        failed.load(Ordering::Relaxed),
        used.len() - models.len() - failed.load(Ordering::Relaxed)
    );

    if !selected_models.is_empty() {
        anyhow::ensure!(
            failed.load(Ordering::Relaxed) == 0,
            "selected model export failed"
        );
        return Ok(());
    }

    // Layout.
    let model_name = |p: &msb::Part| msb.model(p).map(|m| m.name.clone()).unwrap_or_default();
    let mut piece_rows = Vec::new();
    let mut never_drawn = 0;
    let mut bmin = Vec3::splat(f32::MAX);
    let mut bmax = Vec3::splat(f32::MIN);
    let has_collision = msb.parts.iter().any(|p| p.kind == msb::PartKind::Collision);
    for p in msb
        .parts
        .iter()
        .filter(|p| p.kind == msb::PartKind::MapPiece)
    {
        let model = model_name(p);
        let Some(m) = models.get(&model) else {
            continue;
        };
        if legacy_excludes_ungrouped(has_collision, &p.masks) {
            // Only the legacy projection excludes this row; parts retains it.
            never_drawn += 1;
            continue;
        }
        let world = mirror(p.matrix());
        for corner in corners(&m["min"], &m["max"]) {
            let c = world.transform_point3(corner);
            bmin = bmin.min(c);
            bmax = bmax.max(c);
        }
        let mut row = transform_json(p);
        row["name"] = json!(p.name);
        row["model"] = json!(model);
        row["draw_groups"] = json!(groups(p, 1));
        piece_rows.push(row);
    }
    let rows_of = |kinds: &[msb::PartKind]| -> Vec<Value> {
        msb.parts
            .iter()
            .filter(|p| kinds.contains(&p.kind))
            .map(|p| {
                let mut row = transform_json(p);
                row["name"] = json!(p.name);
                row["model"] = json!(model_name(p));
                row["yaw_degrees"] = json!(yaw_degrees(p));
                row["entity_id"] = json!(p.entity_id);
                if p.masks.len() >= 16 && p.kind != msb::PartKind::Collision {
                    row["draw_groups"] = json!(groups(p, 1));
                }
                if let Some(e) = &p.enemy {
                    row["npc_param"] = json!(e.npc_param_id);
                    row["think_param"] = json!(e.think_param_id);
                    row["chara_init"] = json!(e.chara_init_id);
                    row["platoon"] = json!(e.platoon_id);
                    row["dummy"] = json!(p.kind == msb::PartKind::DummyEnemy);
                }
                if let Some(c) = &p.collision {
                    row["hit_filter_id"] = json!(c.hit_filter_id);
                    row["map_name_id"] = json!(c.map_name_id);
                    row["display_groups"] = json!(groups(p, 0));
                    row["light_set"] = json!(p.gparam.map_or(-1, |g| g[0]));
                    row["fog_set"] = json!(p.gparam.map_or(-1, |g| g[1]));
                    row["draw_groups"] = json!(groups(p, 1));
                }
                row
            })
            .collect()
    };

    // Collision.
    let (collision, ranges, collision_note) = export_collision(&raw, id, &msb, oodle)?;
    std::fs::write(out.join("collision.bin"), collision.to_bytes()?)?;
    eprintln!(
        "  collision: {} triangles, {} vertices ({collision_note})",
        collision.triangle_count(),
        collision.vertices.len()
    );
    object_collision::export(cache, id, &msb)?;
    // These are extras: a failure is warned about and leaves the rest of the export (and the
    // last good file) whole. Without `collision_lo.bin` characters fall back to `collision.bin`.
    for binder in [&FALL_ASSIST, &LO_COLLISION] {
        if let Err(e) = write_part_binder(&raw, &out, id, &msb, oodle, binder) {
            eprintln!("  warning: {}: {e:#}", binder.label);
        }
    }

    let mut layout = json!({
        "id": id,
        "source": "sekiro-extract map",
        "convention": "Bevy space: FromSoftware x mirrored (x -> -x); rotations are quaternions xyzw",
        "bounds": { "min": bmin.to_array(), "max": bmax.to_array() },
        "models": models,
        "pieces": piece_rows,
        "objects": rows_of(&[msb::PartKind::Object]),
        "enemies": rows_of(&[msb::PartKind::Enemy, msb::PartKind::DummyEnemy]),
        "players": rows_of(&[msb::PartKind::Player]),
        "collisions": rows_of(&[msb::PartKind::Collision])
            .into_iter()
            .map(|mut row| {
                if let Some(r) = row["name"].as_str().and_then(|n| ranges.get(n)) {
                    row["triangles"] = json!(r);
                }
                row
            })
            .collect::<Vec<_>>(),
        "collision": {
            "file": "collision.bin",
            "triangles": collision.triangle_count(),
            "note": collision_note,
        },
    });
    layout::extend(
        &mut layout,
        &msb,
        &model_status,
        &ranges,
        &layout::map_inventory(&raw)?,
    );
    std::fs::write(out.join("layout.json"), serde_json::to_vec_pretty(&layout)?)?;
    if id == "m89_00_00_99" {
        sky::export(&raw, &out, oodle)?;
    }
    material_audit::export(cache, id, oodle, &mtds, &store)?;
    grass::export(cache, id, &msb, oodle)?;
    eprintln!(
        "  layout: {} pieces ({never_drawn} zero-group rows retained in parts, excluded from legacy pieces), bounds {:?}..{:?}; wrote {}",
        piece_rows.len(),
        bmin.to_array(),
        bmax.to_array(),
        out.display()
    );
    Ok(())
}

/// One 8-word group block of a part's mask struct: 0 display groups, 1 draw groups
/// (SoulsFormats' MSBS `UnkStruct1` layout: display[8], draw[8], collision mask[32]).
fn groups(p: &msb::Part, block: usize) -> Vec<u32> {
    p.masks
        .get(block * 8..block * 8 + 8)
        .map(|g| g.to_vec())
        .unwrap_or_default()
}

fn corners(min: &Value, max: &Value) -> Vec<Vec3> {
    let v = |x: &Value, i: usize| x[i].as_f64().unwrap_or(0.0) as f32;
    let (a, b) = (
        Vec3::new(v(min, 0), v(min, 1), v(min, 2)),
        Vec3::new(v(max, 0), v(max, 1), v(max, 2)),
    );
    (0..8)
        .map(|i| {
            Vec3::new(
                if i & 1 == 0 { a.x } else { b.x },
                if i & 2 == 0 { a.y } else { b.y },
                if i & 4 == 0 { a.z } else { b.z },
            )
        })
        .collect()
}

/// Decodes the high-resolution hit collision of every collision part in the MSB.
///
/// HKX vertices are in the collision model's own space (the hknp bodies carry identity
/// transforms); the MSB collision part transform places them, like any other part. Checked
/// against the player start points, which sit within a metre of collision only with it.
fn export_collision(
    raw: &Path,
    id: &str,
    msb: &msb::Msb,
    oodle: &'static Oodle,
) -> Result<(hknp::CollisionMesh, HashMap<String, [usize; 2]>, String)> {
    let decode = |data: &[u8], compendium: Option<&Compendium>| Ok(hknp::decode(data, compendium)?);
    let PlacedHits {
        mesh,
        ranges,
        parts,
        models,
        failed,
        stats: total,
    } = place_hit_binder(raw, id, msb, oodle, "h", decode)?
        .with_context(|| format!("map {id} has no h{}.hkxbhd", &id[1..]))?;
    let ranges: HashMap<String, [usize; 2]> = ranges.into_iter().collect();

    // Validation: every player start should stand on collision.
    let verts: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from(*v)).collect();
    let start_gap = msb
        .parts
        .iter()
        .filter(|p| p.kind == msb::PartKind::Player)
        .map(|p| {
            let s = Vec3::new(-p.position[0], p.position[1], p.position[2]);
            verts
                .par_iter()
                .map(|v| v.distance(s))
                .reduce(|| f32::MAX, f32::min)
        })
        .fold(0.0f32, f32::max);
    let materials: BTreeSet<u32> = mesh.materials.iter().copied().collect();
    let note = format!(
        "{} collision parts, {} models, {} failed, {} bodies, {} sections, {} primitives ({} quads, {} convex skipped), {} triangles without material, {} distinct materials, furthest player start {:.2} m from a collision vertex",
        parts,
        models,
        failed.len(),
        total.bodies,
        total.sections,
        total.primitives,
        total.quads,
        total.convex_skipped,
        total.unmatched_material,
        materials.len(),
        start_gap,
    );
    Ok((mesh, ranges, note))
}

#[cfg(test)]
mod tests {
    use super::{legacy_excludes_ungrouped, physics_only_members};

    #[test]
    fn cinematic_pieces_do_not_need_collision_draw_groups() {
        assert!(!legacy_excludes_ungrouped(false, &[0; 48]));
        assert!(legacy_excludes_ungrouped(true, &[0; 48]));
        let mut masks = [0; 48];
        masks[8] = 1;
        assert!(!legacy_excludes_ungrouped(true, &masks));
        assert!(!legacy_excludes_ungrouped(true, &[]));
    }

    #[test]
    fn physics_only_object_is_not_a_missing_render_model() {
        assert!(physics_only_members(["o115923.hkx"]));
        assert!(physics_only_members(["O115923.HKX.DCX"]));
        assert!(!physics_only_members(["o115923.hkx", "o115923.flver"]));
        assert!(!physics_only_members(["o115923.hkx", "o115923.flver.dcx"]));
        assert!(!physics_only_members(["o115923.tpf"]));
        assert!(!physics_only_members([]));
    }
}
