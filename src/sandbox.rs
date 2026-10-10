//! The sandbox map (`[world] map = "sandbox"`, or `SHINOBI_MAP=sandbox` for one run): a flat
//! walled arena built in code under a real map's lighting (its light set, GI probes, colour
//! grading LUT and auto exposure; map.rs), with every material of that map on a panel of its
//! own so the textures and the layer blends can be checked side by side, and room for a
//! line-up of enemies (`SHINOBI_SANDBOX_ENEMIES`). kb/map.md "Sandbox".
//!
//! Layout (metres, Bevy space; Wolf starts at the origin facing -Z, the duel enemy 4 m ahead):
//! the floor is `HALF` each way from the centre with 5 m walls around it; the gallery stands
//! ahead at -Z, facing Wolf, in rows of `PER_ROW` panels (2 m wide, leaning back 45 degrees so
//! the snow layer, which needs an up-facing surface, shows too); the enemy line-up stands
//! behind Wolf at +Z, facing him.
//! A panel's blend bytes run: overlay 0 -> 1 left to right, snow 0 -> 1 bottom to top, layer C
//! at the real map's constant 0.5. The floor's overlay rises towards +X, its snow towards -X.
//! `extracted/map_sandbox_panels.txt` lists which material each panel shows.
//! `SHINOBI_SANDBOX_AT=x,z[,yaw]` stands Wolf there at the start, facing yaw degrees (0 = -Z,
//! the gallery, the default; 180 = +Z, the line-up; a screenshot of one row: its z + 5).

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::config::GameConfig;
use crate::data::Combat;
use crate::map::{self, BinBatch, MapMaterial, MapPiece, MaterialCache, MaterialKey, MaterialOptions, Terrain};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::actor::Actor;
use crate::camera::OrbitCamera;
use crate::enemy::Enemy;
use crate::player::Player;

/// The `[world] map` value that selects the sandbox.
pub const MAP_ID: &str = "sandbox";
/// Half the floor's side.
const HALF: f32 = 45.0;
const WALL_HEIGHT: f32 = 5.0;
/// Metres per texture repeat on the floor and the walls (the game's map textures run about
/// one repeat every 3-4 m).
const TILE_M: f32 = 4.0;
const PER_ROW: usize = 20;
const PANEL_W: f32 = 2.0;
const PANEL_STEP_X: f32 = 2.4;
const PANEL_STEP_Z: f32 = 3.0;
const GALLERY_Z: f32 = -10.0;

/// The map whose lighting and materials the sandbox uses: `SHINOBI_SANDBOX_SRC`, else the
/// Outskirts gate.
pub fn source() -> String {
    std::env::var("SHINOBI_SANDBOX_SRC").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "m11_01_00_00".to_string())
}

fn is_sandbox(config: &GameConfig) -> bool {
    map::map_id(config).as_deref() == Some(MAP_ID)
}

/// A mesh under construction: a textured patch with the two UV sets and the blend bytes
/// map_material.wgsl reads.
#[derive(Default)]
struct Patch {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    blend: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Patch {
    /// A grid of `nu` x `nv` quads spanning `du` and `dv` from `origin`, its normal
    /// `dv x du` (counter-clockwise seen from that side). `uv_scale` metres per repeat;
    /// `blend(u, v)` with u, v in 0..1 across the patch.
    fn grid(&mut self, origin: Vec3, du: Vec3, dv: Vec3, nu: usize, nv: usize, uv_scale: f32, blend: impl Fn(f32, f32) -> [f32; 4]) {
        let n = dv.cross(du).normalize();
        let base = self.pos.len() as u32;
        for j in 0..=nv {
            for i in 0..=nu {
                let (u, v) = (i as f32 / nu as f32, j as f32 / nv as f32);
                let p = origin + du * u + dv * v;
                self.pos.push(p.to_array());
                self.nrm.push(n.to_array());
                // v runs down the texture: the top of a wall or panel is v = 0.
                self.uv.push([u * du.length() / uv_scale, (1.0 - v) * dv.length() / uv_scale]);
                self.blend.push(blend(u, v));
            }
        }
        let at = |i: usize, j: usize| base + (j * (nu + 1) + i) as u32;
        for j in 0..nv {
            for i in 0..nu {
                self.idx.extend([at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i, j), at(i + 1, j + 1), at(i + 1, j)]);
            }
        }
    }

    fn mesh(&self, layered: bool, tangents: bool) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        if tangents {
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, map::tangents(&self.pos, &self.nrm, &self.uv, &self.idx));
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv.clone());
        if layered {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uv.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.blend.clone());
        }
        mesh.insert_indices(Indices::U32(self.idx.clone()));
        mesh
    }

    /// Collision triangles (all material 0).
    fn terrain(&self) -> Terrain {
        Terrain::from_tris(self.pos.iter().map(|p| Vec3::from(*p)).collect(), self.idx.chunks_exact(3).map(|t| ([t[0], t[1], t[2]], 0)))
    }
}

/// Builds the sandbox from the source map's bin (`d`: its materials, by triangle count).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    d: &[u8],
    commands: &mut Commands,
    assets: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<MapMaterial>,
    mats: &mut MaterialCache,
    options: &MaterialOptions,
) {
    // The source map's materials, most triangles first.
    let mut tris: HashMap<MaterialKey, usize> = HashMap::new();
    if map::walk_bin(d, |b: BinBatch| *tris.entry(b.key()).or_default() += b.index_count / 3).is_none() {
        warn!("sandbox: the source bin is not a SHMP file");
    }
    let mut keys: Vec<(MaterialKey, usize)> = tris.into_iter().collect();
    keys.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));
    // Floor and wall materials: `SHINOBI_SANDBOX_FLOOR` / `SHINOBI_SANDBOX_WALL` name a base
    // albedo stem (or a part of one); else the biggest ground / soil and stone wall materials.
    let pick = |var: &str, defaults: &[&str]| -> MaterialKey {
        let want = std::env::var(var).ok().filter(|s| !s.is_empty());
        let names: Vec<&str> = want.as_deref().map(|w| vec![w]).unwrap_or_else(|| defaults.to_vec());
        for n in names {
            if let Some((k, _)) = keys.iter().find(|(k, _)| k.1 == 0 && k.0[0].contains(n)) {
                return k.clone();
            }
        }
        keys.first().map(|(k, _)| k.clone()).unwrap_or_default()
    };
    let floor_key = pick("SHINOBI_SANDBOX_FLOOR", &["_ground_", "_soil_", "pavement"]);
    let wall_key = pick("SHINOBI_SANDBOX_WALL", &["stone_wall_block", "stone_wall", "_wall_"]);
    let ramp = |x: f32| ((x - 6.0) / 30.0).clamp(0.0, 1.0);

    // Floor: the overlay rises towards +X, the snow towards -X; the centre stays the base.
    let mut floor = Patch::default();
    floor.grid(Vec3::new(-HALF, 0.0, -HALF), Vec3::new(2.0 * HALF, 0.0, 0.0), Vec3::new(0.0, 0.0, 2.0 * HALF), 30, 30, TILE_M, |u, _| {
        let x = (u - 0.5) * 2.0 * HALF;
        [ramp(x), 0.0, ramp(-x), 0.0]
    });
    // Walls facing in, the overlay rising along each.
    let mut walls = Patch::default();
    let up = Vec3::new(0.0, WALL_HEIGHT, 0.0);
    let side = 2.0 * HALF;
    for (origin, du) in [
        (Vec3::new(-HALF, 0.0, HALF), Vec3::new(side, 0.0, 0.0)),
        (Vec3::new(HALF, 0.0, -HALF), Vec3::new(-side, 0.0, 0.0)),
        (Vec3::new(HALF, 0.0, HALF), Vec3::new(0.0, 0.0, -side)),
        (Vec3::new(-HALF, 0.0, -HALF), Vec3::new(0.0, 0.0, side)),
    ] {
        walls.grid(origin, du, up, 24, 2, TILE_M, |u, _| [u, 0.0, 0.0, 0.0]);
    }
    let mut spawn = |patch: &Patch, key: &MaterialKey, casts: bool| {
        let material = mats.get(assets, materials, options, key);
        let mesh = patch.mesh(key.layered(), !key.0[1].is_empty() && !options.no_normals);
        let mut e = commands.spawn((MapPiece, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::default(), Visibility::default()));
        if !casts {
            e.insert(bevy::light::NotShadowCaster);
        }
    };
    spawn(&floor, &floor_key, true);
    spawn(&walls, &wall_key, true);
    // `SHINOBI_SANDBOX_NO_GALLERY=1`: floor and walls only (a fight arena). The panels cast no
    // shadow unless `SHINOBI_SANDBOX_PANEL_SHADOW=1`: 242 more casters in two cascades cost
    // frames and their shadows on the floor show nothing.
    let gallery = std::env::var("SHINOBI_SANDBOX_NO_GALLERY").is_err();
    let panel_shadow = std::env::var("SHINOBI_SANDBOX_PANEL_SHADOW").is_ok();

    // The gallery: one leaning panel per material, in rows from the front.
    let mut legend = String::from("row col | albedo | normal | overlay | overlay normal | snow | snow normal | layer C | C normal | mask | alpha two-sided | triangles\n");
    for (i, (key, n)) in keys.iter().enumerate().filter(|_| gallery) {
        let (row, col) = (i / PER_ROW, i % PER_ROW);
        // Column 0 on Wolf's left (+X, seen facing -Z); the panel's u runs left to right.
        let x = ((PER_ROW as f32 - 1.0) / 2.0 - col as f32) * PANEL_STEP_X;
        let z = GALLERY_Z - row as f32 * PANEL_STEP_Z;
        let mut panel = Patch::default();
        let rise = PANEL_W * std::f32::consts::FRAC_1_SQRT_2;
        panel.grid(Vec3::new(x + PANEL_W / 2.0, 0.3, z), Vec3::new(-PANEL_W, 0.0, 0.0), Vec3::new(0.0, rise, -rise), 8, 8, PANEL_W / 2.0, |u, v| [u, 0.0, v, 0.5]);
        spawn(&panel, key, panel_shadow);
        let names: Vec<&str> = key.0.iter().map(|s| if s.is_empty() { "-" } else { s.as_str() }).collect();
        legend += &format!("{row:2} {col:2} | {} | a{} {} | {n}\n", names.join(" | "), key.1, key.2 as u8);
    }
    let legend_path = crate::paths::extracted().join("map_sandbox_panels.txt");
    if let Err(e) = std::fs::write(&legend_path, &legend) {
        warn!("sandbox: cannot write {}: {e}", legend_path.display());
    }
    // Collision: the floor and the walls.
    let mut solid = Patch::default();
    solid.grid(Vec3::new(-HALF, 0.0, -HALF), Vec3::new(2.0 * HALF, 0.0, 0.0), Vec3::new(0.0, 0.0, 2.0 * HALF), 1, 1, TILE_M, |_, _| [0.0; 4]);
    solid.pos.extend_from_slice(&walls.pos);
    solid.nrm.extend_from_slice(&walls.nrm);
    solid.uv.extend_from_slice(&walls.uv);
    solid.blend.extend_from_slice(&walls.blend);
    solid.idx.extend(walls.idx.iter().map(|i| i + 4));
    solid.terrain().install();
    info!("sandbox: {} materials{} in {} rows, floor {} ({} m), walls {}; legend {}", keys.len(), if gallery { " on panels" } else { " (gallery off)" }, keys.len().div_ceil(PER_ROW), floor_key.0[0], 2.0 * HALF, wall_key.0[0], legend_path.display());
}

/// Enemy kinds per `all:<page>` page of `SHINOBI_SANDBOX_ENEMIES` (`SHINOBI_SANDBOX_PAGE`
/// sets another size).
const PAGE: usize = 20;
/// Widest NpcParam hitRadius that goes into an `all` page: the bosses above it (apes, the
/// serpent, the dragon, the horse) fill the arena on their own.
const MAX_RADIUS: f32 = 2.5;

fn page_size() -> usize {
    std::env::var("SHINOBI_SANDBOX_PAGE").ok().and_then(|v| v.parse().ok()).filter(|&n| n > 0).unwrap_or(PAGE)
}

/// The extra enemy kinds (chr ids) `SHINOBI_SANDBOX_ENEMIES` asks for, which the data loader
/// (data.rs DataPlugin) loads besides the config's: "all:<n>" = page n (of `PAGE`) of every
/// `extracted/enemies/c*.json` ("all" = page 0), "all:*" = the whole roster (102: 20 of them
/// already run at 20-25 fps, the whole lot lags), or a comma-separated list of chr ids
/// (`c1020,c1010`, a kind's default row). Empty off the sandbox or unset.
pub fn extra_kinds(config: &GameConfig) -> Vec<String> {
    if !is_sandbox(config) {
        return Vec::new();
    }
    let Some(want) = std::env::var("SHINOBI_SANDBOX_ENEMIES").ok().filter(|s| !s.is_empty()) else { return Vec::new() };
    if let Some(rest) = want.strip_prefix("all") {
        let mut all: Vec<String> = std::fs::read_dir(crate::paths::extracted().join("enemies"))
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).filter(|n| n.starts_with('c') && n.len() == 5).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        all.sort();
        let per = page_size();
        let pages = all.len().div_ceil(per);
        return match rest.strip_prefix(':') {
            Some("*") => all,
            Some(n) => {
                let page = n.parse::<usize>().unwrap_or(0);
                if page >= pages {
                    warn!("sandbox: enemy page {page} is past the last ({})", pages.saturating_sub(1));
                }
                all.into_iter().skip(page * per).take(per).collect()
            }
            None => {
                info!("sandbox: SHINOBI_SANDBOX_ENEMIES=all shows page 0 of {pages} (all:<n>; all:* for every kind)");
                all.into_iter().take(per).collect()
            }
        };
    }
    want.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// `SHINOBI_SANDBOX_ENEMIES` (see `extra_kinds`): the kinds asked for stand in rows behind
/// Wolf (+Z, facing him), standing still (the debug AI mode "stand still", F1 menu to change:
/// the passive mode would walk them all up to Wolf); unset: the config line-up only. Spaced
/// by their NpcParam hitRadius (1 m between bodies, 3 m at least) in rows from 9 m back;
/// kinds wider than `MAX_RADIUS` (the scene-sized bosses) are left out of `all` pages and
/// stand alone when named.
pub fn spawn_enemies(mut commands: Commands, combat: Res<Combat>, config: Res<GameConfig>, mut debug: Option<ResMut<crate::enemy::EnemyDebug>>) {
    let mut order = Vec::new();
    let want = extra_kinds(&config);
    if want.is_empty() {
        return;
    }
    let kinds: Vec<usize> = want
        .iter()
        .filter_map(|chr| {
            let k = combat.kind_index(chr, None);
            if k.is_none() {
                warn!("sandbox: no enemy kind {chr}");
            }
            k
        })
        .collect();
    let y = crate::player::CAPSULE_HALF_HEIGHT + 0.05;
    let radius = |kind: usize| combat.param("NpcParam", combat.kind(kind).foe.npc_row)["hitRadius"].as_f64().unwrap_or(0.5) as f32;
    let paged = want.len() > 1;
    let (mut x, mut z, mut row_r, mut prev_r) = (-HALF + 5.0, 9.0, 0.0f32, 0.0f32);
    let mut skipped = Vec::new();
    for (i, &kind) in kinds.iter().enumerate() {
        let r = radius(kind);
        if paged && r > MAX_RADIUS {
            skipped.push(combat.kind(kind).foe.chr.clone());
            continue;
        }
        let step = (prev_r + r + 1.0).max(3.0);
        if x + step + r > HALF - 5.0 {
            // Next row, clear of the widest body in this one; nothing past the back wall.
            z += (2.0 * row_r + 3.0).max(3.0);
            if z + 2.0 * r > HALF - 5.0 {
                warn!("sandbox: no room left in the line-up for {} and the rest", combat.kind(kind).foe.chr);
                break;
            }
            x = -HALF + 5.0 + r;
            row_r = 0.0;
        } else {
            x += if i == 0 { r } else { step };
        }
        row_r = row_r.max(r);
        prev_r = r;
        let at = Vec3::new(x, y, z + r.max(0.5) - 0.5);
        // Yaw 0 faces -Z (Actor::forward): towards Wolf.
        order.push((crate::enemy::spawn_one(&mut commands, &combat, &config, kind, at, 0.0, false, 0x1234_5678_u32.wrapping_mul(i as u32 + 7) ^ 0x9E37_79B9), at));
    }
    if !skipped.is_empty() {
        info!("sandbox: too big for the line-up (hitRadius > {MAX_RADIUS} m), name one alone to see it: {}", skipped.join(", "));
    }
    let lined_up = order.len();
    if let Some(dir) = std::env::var("SHINOBI_SANDBOX_SHOTS").ok().filter(|s| !s.is_empty()) {
        let _ = std::fs::create_dir_all(&dir);
        commands.insert_resource(Catalog { dir, order, idx: 0, timer: -CATALOG_WARMUP, placed: false, taken: false });
    }
    if let Some(d) = debug.as_deref_mut() {
        d.mode = crate::enemy::AiMode::Idle;
    }
    info!("sandbox: {} enemies lined up ({}), standing still (F1 menu: AI mode)", lined_up, std::env::var("SHINOBI_SANDBOX_ENEMIES").unwrap_or_default());
}

/// `SHINOBI_SANDBOX_AT=x,z[,yaw]`: Wolf starts there, facing yaw degrees (the camera behind
/// him).
pub fn place_wolf(config: Res<GameConfig>, mut done: Local<bool>, mut wolf: Query<(&mut Transform, &mut Actor), With<Player>>, mut camera: Query<&mut OrbitCamera>) {
    if *done || !is_sandbox(&config) {
        return;
    }
    let Ok((mut tf, mut actor)) = wolf.single_mut() else { return };
    *done = true;
    let Some(d) = std::env::var("SHINOBI_SANDBOX_AT").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<f32>>()).filter(|d| d.len() >= 2) else {
        return;
    };
    tf.translation.x = d[0];
    tf.translation.z = d[1];
    if let Some(yaw) = d.get(2) {
        actor.yaw = yaw.to_radians();
        for mut cam in &mut camera {
            cam.yaw = actor.yaw;
        }
    }
    info!("sandbox: Wolf at {}, {} facing {:.0} deg", d[0], d[1], actor.yaw.to_degrees());
}

/// `SHINOBI_SANDBOX_WATCH=<sec>`: logs the debug AI mode and every enemy's state, anim,
/// stealth state and position once at that time (why does the line-up move?).
pub fn watch(time: Res<Time>, mut done: Local<bool>, ai_debug: Option<Res<crate::enemy::EnemyDebug>>, enemies: Query<(&crate::enemy::Enemy, &Actor, &Transform)>, wolf: Query<(&Actor, &Transform), With<Player>>) {
    if *done {
        return;
    }
    let Some(at) = std::env::var("SHINOBI_SANDBOX_WATCH").ok().and_then(|v| v.parse::<f32>().ok()) else {
        *done = true;
        return;
    };
    if time.elapsed_secs() < at {
        return;
    }
    *done = true;
    info!("sandbox watch at {at} s: debug AI mode {:?}", ai_debug.as_ref().map(|d| d.mode));
    for (a, tf) in &wolf {
        info!("  Wolf state {} anim {} t {:.2} at {:.1} {:.1} yaw {:.0}", a.state, a.anim, a.t, tf.translation.x, tf.translation.z, a.yaw.to_degrees());
    }
    for (e, a, tf) in &enemies {
        info!("  kind {} state {} anim {} t {:.2} stealth {} aggressive {} at {:.1} {:.1}", a.kind, a.state, a.anim, a.t, e.targeting.state, e.aggressive, tf.translation.x, tf.translation.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid patch faces `dv x du` with counter-clockwise triangles, tiles its UVs by the
    /// metre and, as collision, is a floor at its height.
    #[test]
    fn grid_patch() {
        let mut floor = Patch::default();
        floor.grid(Vec3::new(-10.0, 1.0, -10.0), Vec3::new(20.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 20.0), 4, 4, 4.0, |u, v| [u, 0.0, v, 0.5]);
        assert_eq!(floor.pos.len(), 25);
        assert_eq!(floor.idx.len(), 4 * 4 * 6);
        assert!(floor.nrm.iter().all(|n| (n[1] - 1.0).abs() < 1e-6));
        // Winding: every triangle's geometric normal agrees with the vertex normal.
        for t in floor.idx.chunks_exact(3) {
            let [a, b, c] = [Vec3::from(floor.pos[t[0] as usize]), Vec3::from(floor.pos[t[1] as usize]), Vec3::from(floor.pos[t[2] as usize])];
            assert!((b - a).cross(c - a).y > 0.0);
        }
        assert_eq!(floor.uv[0], [0.0, 5.0]);
        assert_eq!(floor.uv[24], [5.0, 0.0]);
        assert_eq!(floor.blend[24], [1.0, 0.0, 1.0, 0.5]);
        let t = floor.terrain();
        assert_eq!(t.triangle_count(), 32);
        assert!((t.floor(3.0, -7.0, 10.0).unwrap() - 1.0).abs() < 1e-5);
        // A wall facing -Z (origin at -X, du along +X, dv up).
        let mut wall = Patch::default();
        wall.grid(Vec3::new(-10.0, 0.0, 10.0), Vec3::new(20.0, 0.0, 0.0), Vec3::new(0.0, 5.0, 0.0), 2, 1, 4.0, |u, _| [u; 4]);
        assert!(wall.nrm.iter().all(|n| (n[2] + 1.0).abs() < 1e-6));
    }

    /// The gallery panels lean back towards -Z and face Wolf (+Z and up).
    #[test]
    fn panel_lean() {
        let mut panel = Patch::default();
        let rise = PANEL_W * std::f32::consts::FRAC_1_SQRT_2;
        panel.grid(Vec3::new(1.0, 0.3, -10.0), Vec3::new(-PANEL_W, 0.0, 0.0), Vec3::new(0.0, rise, -rise), 2, 2, 1.0, |u, v| [u, 0.0, v, 0.5]);
        let n = Vec3::from(panel.nrm[0]);
        assert!((n - Vec3::new(0.0, 0.5f32.sqrt(), 0.5f32.sqrt())).length() < 1e-5, "{n}");
        // The top row sits higher and further back than the bottom row.
        assert!(panel.pos[8][1] > panel.pos[0][1] && panel.pos[8][2] < panel.pos[0][2]);
        // Snow weight 0 at the bottom, 1 at the top; overlay left to right.
        assert_eq!(panel.blend[0][2], 0.0);
        assert_eq!(panel.blend[8][2], 1.0);
        assert_eq!((panel.blend[0][0], panel.blend[2][0]), (0.0, 1.0));
    }
}

/// `SHINOBI_SANDBOX_SHOTS=<dir>`: the catalog. Wolf is put in front of each line-up enemy in
/// turn, a screenshot `<dir>/<nn>_<chr>_<row>.png` taken, and the game exits after the last:
/// every kind under the real lighting, one page (`all:<n>`) per run.
#[derive(Resource)]
pub(crate) struct Catalog {
    dir: String,
    /// The line-up in spawn order, each with its spot.
    order: Vec<(Entity, Vec3)>,
    idx: usize,
    timer: f32,
    placed: bool,
    taken: bool,
}

/// Seconds before the first shot (models load) and between placing and shooting.
const CATALOG_WARMUP: f32 = 4.0;
const CATALOG_SETTLE: f32 = 1.5;

pub(crate) fn catalog_shots(
    mut commands: Commands,
    time: Res<Time>,
    mut catalog: ResMut<Catalog>,
    combat: Res<Combat>,
    mut wolf: Query<(&mut Transform, &mut Actor), With<Player>>,
    enemies: Query<(&Transform, &Actor, &Name), (With<Enemy>, Without<Player>)>,
    mut camera: Query<&mut OrbitCamera>,
    mut exit: MessageWriter<AppExit>,
) {
    catalog.timer += time.delta_secs();
    if catalog.idx >= catalog.order.len() {
        if catalog.timer > 1.0 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let Ok((mut tf, mut actor)) = wolf.single_mut() else { return };
    let (entity, spot) = catalog.order[catalog.idx];
    let Ok((etf, ea, name)) = enemies.get(entity) else {
        catalog.idx += 1;
        return;
    };
    if catalog.timer < 0.0 {
        return;
    }
    if !catalog.placed {
        // Skipped: a kind that left its spot (NPCs that warp or fall away at spawn).
        if etf.translation.distance(spot) > 5.0 {
            warn!("sandbox catalog: {} left its spot ({:.1} m), skipped", name.as_str(), etf.translation.distance(spot));
            catalog.idx += 1;
            return;
        }
        // In front of the enemy (it faces -Z), further back for the big ones (NpcParam
        // hitRadius), a step to the side so Wolf does not hide it from the camera behind him;
        // both face each other.
        let radius = combat.npc(ea)["hitRadius"].as_f64().unwrap_or(0.5) as f32;
        let back = (3.5 + 3.0 * (radius - 0.5).max(0.0)).min(14.0);
        tf.translation.x = etf.translation.x + 1.2;
        tf.translation.z = etf.translation.z - back;
        actor.yaw = std::f32::consts::PI;
        for mut cam in &mut camera {
            cam.yaw = actor.yaw;
            cam.pitch = 6f32.to_radians();
        }
        catalog.placed = true;
        catalog.timer = 0.0;
    }
    if catalog.timer >= CATALOG_SETTLE && !catalog.taken {
        let file = name.as_str().trim_start_matches("Enemy ").replace(' ', "_");
        let path = format!("{}/{:02}_{file}.png", catalog.dir, catalog.idx);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        catalog.taken = true;
    }
    if catalog.timer >= CATALOG_SETTLE + 0.6 {
        catalog.idx += 1;
        catalog.placed = false;
        catalog.taken = false;
        catalog.timer = 0.0;
    }
}
