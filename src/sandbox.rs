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
//! `[world] map = "construct"` (or `SHINOBI_MAP=construct`; "parkour", its old name, still
//! works): the same arena with the gallery off and the parkour course (`parkour_course`: a hug
//! wall, an L platform with hang ledges and stairs, a cliff for the wall jumps, the wall-jump
//! shaft) and its edges ahead of Wolf, and behind him the stealth courtyard (`STEALTH_WALLS`,
//! `construct_patrols`): soldiers walking their rounds between cover walls.
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
    parkour: bool,
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

    // Floor: the overlay rises towards +X, the snow towards -X; the centre stays the base (the
    // road). The Construct's runs on through its second section to the north.
    let z0 = if parkour { -3.0 * HALF } else { -HALF };
    // The Construct's floor is cut round its pool (`POOL`): the rectangles about the hole.
    let rects: Vec<(f32, f32, f32, f32)> = if parkour {
        let (lo, hi) = POOL;
        vec![(-HALF, lo.x, z0, HALF), (hi.x, HALF, z0, HALF), (lo.x, hi.x, z0, lo.z), (lo.x, hi.x, hi.z, HALF)]
    } else {
        vec![(-HALF, HALF, z0, HALF)]
    };
    let mut floor = Patch::default();
    let mut ground = Patch::default();
    for &(x0, x1, za, zb) in &rects {
        let (nu, nv) = ((((x1 - x0) / 3.0).ceil() as usize).max(1), (((zb - za) / 3.0).ceil() as usize).max(1));
        floor.grid(Vec3::new(x0, 0.0, za), Vec3::new(x1 - x0, 0.0, 0.0), Vec3::new(0.0, 0.0, zb - za), nu, nv, TILE_M, |u, _| {
            let x = x0 + u * (x1 - x0);
            [ramp(x), 0.0, ramp(-x), 0.0]
        });
        ground.grid(Vec3::new(x0, 0.0, za), Vec3::new(x1 - x0, 0.0, 0.0), Vec3::new(0.0, 0.0, zb - za), 1, 1, TILE_M, |_, _| [0.0; 4]);
    }
    // The pool's pit: its bottom (floor material), its four walls facing in and the ramp out at
    // its west end (wall material); drawn and solid.
    let mut pit_floor = Patch::default();
    let mut pit = Patch::default();
    if parkour {
        let (lo, hi) = POOL;
        let (w, l) = (hi.x - lo.x, hi.z - lo.z);
        pit_floor.grid(Vec3::new(lo.x, lo.y, lo.z), Vec3::new(w, 0.0, 0.0), Vec3::new(0.0, 0.0, l), 8, 6, TILE_M, |_, _| [0.0; 4]);
        let up = Vec3::new(0.0, -lo.y, 0.0);
        for (origin, run) in [
            (Vec3::new(lo.x, lo.y, lo.z), Vec3::new(w, 0.0, 0.0)),
            (Vec3::new(hi.x, lo.y, hi.z), Vec3::new(-w, 0.0, 0.0)),
            (Vec3::new(lo.x, lo.y, hi.z), Vec3::new(0.0, 0.0, -l)),
            (Vec3::new(hi.x, lo.y, lo.z), Vec3::new(0.0, 0.0, l)),
        ] {
            pit.grid(origin, run, up, 4, 2, TILE_M, |_, _| [0.0; 4]);
        }
        let (o, down, across) = POOL_RAMP;
        pit.grid(o, down, across, 4, 2, TILE_M, |_, _| [0.0; 4]);
    }
    // Walls facing in, the overlay rising along each (a face's normal is up x its run: +X runs
    // face -Z).
    let mut walls = Patch::default();
    let up = Vec3::new(0.0, WALL_HEIGHT, 0.0);
    let side = 2.0 * HALF;
    let run = HALF - DOOR_HALF;
    let mut faces = vec![
        (Vec3::new(-HALF, 0.0, HALF), Vec3::new(side, 0.0, 0.0)),
        (Vec3::new(HALF, 0.0, HALF), Vec3::new(0.0, 0.0, -side)),
        (Vec3::new(-HALF, 0.0, -HALF), Vec3::new(0.0, 0.0, side)),
    ];
    if parkour {
        faces.extend([
            // The shared wall either side of the door, facing both sections.
            (Vec3::new(HALF, 0.0, -HALF), Vec3::new(-run, 0.0, 0.0)),
            (Vec3::new(-DOOR_HALF, 0.0, -HALF), Vec3::new(-run, 0.0, 0.0)),
            (Vec3::new(-HALF, 0.0, -HALF), Vec3::new(run, 0.0, 0.0)),
            (Vec3::new(DOOR_HALF, 0.0, -HALF), Vec3::new(run, 0.0, 0.0)),
            // The second section's far, east and west walls.
            (Vec3::new(HALF, 0.0, -3.0 * HALF), Vec3::new(-side, 0.0, 0.0)),
            (Vec3::new(HALF, 0.0, -HALF), Vec3::new(0.0, 0.0, -side)),
            (Vec3::new(-HALF, 0.0, -3.0 * HALF), Vec3::new(0.0, 0.0, side)),
        ]);
    } else {
        faces.push((Vec3::new(HALF, 0.0, -HALF), Vec3::new(-side, 0.0, 0.0)));
    }
    for (origin, du) in faces {
        let n = ((24.0 * du.length() / side).round() as usize).max(1);
        walls.grid(origin, du, up, n, 2, TILE_M, |u, _| [u, 0.0, 0.0, 0.0]);
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
    if parkour {
        spawn(&pit_floor, &floor_key, true);
        spawn(&pit, &wall_key, true);
    }
    // `SHINOBI_SANDBOX_NO_GALLERY=1`: floor and walls only (a fight arena). The panels cast no
    // shadow unless `SHINOBI_SANDBOX_PANEL_SHADOW=1`: 242 more casters in two cascades cost
    // frames and their shadows on the floor show nothing.
    let gallery = std::env::var("SHINOBI_SANDBOX_NO_GALLERY").is_err() && !parkour;
    // The parkour course (`map = "parkour"`): its boxes in the wall material, and collision.
    let mut course = Patch::default();
    if parkour {
        for (lo, hi) in parkour_course().0 {
            course.cuboid(lo, hi);
        }
        // The stairs: drawn as steps, walked as a ramp (the course patch is the collision too).
        let mut steps = Patch::default();
        for (lo, hi) in parkour_stairs() {
            steps.cuboid(lo, hi);
        }
        let mut both = Patch::default();
        both.pos = [course.pos.clone(), steps.pos.clone()].concat();
        both.nrm = [course.nrm.clone(), steps.nrm.clone()].concat();
        both.uv = [course.uv.clone(), steps.uv.clone()].concat();
        both.blend = [course.blend.clone(), steps.blend.clone()].concat();
        let n = course.pos.len() as u32;
        both.idx = course.idx.iter().copied().chain(steps.idx.iter().map(|i| i + n)).collect();
        spawn(&both, &wall_key, true);
        // The ramp: from the floor at x -1.5 to the top (3 m) at x 2, 41 degrees.
        course.grid(Vec3::new(-1.5, 0.0, -12.0), Vec3::new(3.5, 3.0, 0.0), Vec3::new(0.0, 0.0, 2.0), 1, 1, TILE_M, |_, _| [0.0; 4]);
        // The outpost's terrace stairs' ramp.
        let (o, across, up) = Outpost::ramp();
        course.grid(o, across, up, 1, 1, TILE_M, |_, _| [0.0; 4]);
    }
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
    // Collision: the floor, the walls, the course and the pool's pit.
    let mut solid = Patch::default();
    for part in [&ground, &walls, &course, &pit_floor, &pit] {
        let base = solid.pos.len() as u32;
        solid.pos.extend_from_slice(&part.pos);
        solid.nrm.extend_from_slice(&part.nrm);
        solid.uv.extend_from_slice(&part.uv);
        solid.blend.extend_from_slice(&part.blend);
        solid.idx.extend(part.idx.iter().map(|i| i + base));
    }
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
    let want = extra_kinds(&config);
    if want.is_empty() {
        return;
    }
    let order = spawn_lineup(&mut commands, &combat, &config, &want);
    let lined_up = order.len();
    // Only a line-up that stood comes back after a reset (none placed: respawn_lineup would
    // try again every frame).
    if lined_up > 0 {
        commands.insert_resource(Lineup(want));
    }
    if let Some(dir) = std::env::var("SHINOBI_SANDBOX_SHOTS").ok().filter(|s| !s.is_empty()) {
        let _ = std::fs::create_dir_all(&dir);
        commands.insert_resource(Catalog { dir, order, idx: 0, timer: -CATALOG_WARMUP, placed: false, taken: false });
    }
    if let Some(d) = debug.as_deref_mut() {
        d.mode = crate::enemy::AiMode::Idle;
    }
    info!("sandbox: {} enemies lined up ({}), standing still (F1 menu: AI mode)", lined_up, std::env::var("SHINOBI_SANDBOX_ENEMIES").unwrap_or_default());
}

/// A sandbox line-up enemy (`spawn_lineup`).
#[derive(Component)]
pub struct SandboxEnemy;

/// The sandbox line-up's kinds (chr ids), kept to bring it back after a reset.
#[derive(Resource, Clone, Debug)]
pub(crate) struct Lineup(pub Vec<String>);

/// A fight reset (R, Retry, the debug menu: enemy.rs reset_fight) despawns every enemy and
/// brings back only the config's: with the whole line-up gone, it stands again in its rows
/// (the catalog goes on with the new entities).
pub(crate) fn respawn_lineup(
    mut commands: Commands,
    combat: Res<Combat>,
    config: Res<GameConfig>,
    lineup: Option<Res<Lineup>>,
    standing: Query<(), With<SandboxEnemy>>,
    catalog: Option<ResMut<Catalog>>,
) {
    let Some(lineup) = lineup else { return };
    if !standing.is_empty() {
        return;
    }
    let order = spawn_lineup(&mut commands, &combat, &config, &lineup.0);
    if order.is_empty() {
        warn!("sandbox: no line-up kind could stand ({:?}); not retried", lineup.0);
        commands.remove_resource::<Lineup>();
        return;
    }
    info!("sandbox: line-up back after a reset ({} enemies)", order.len());
    if let Some(mut c) = catalog {
        c.order = order;
        c.placed = false;
        c.taken = false;
    }
}

/// Spawns `want`'s kinds in rows (see `spawn_enemies`); returns them in spawn order with their
/// spots.
fn spawn_lineup(commands: &mut Commands, combat: &Combat, config: &GameConfig, want: &[String]) -> Vec<(Entity, Vec3)> {
    let mut order = Vec::new();
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
        let e = crate::enemy::spawn_one(commands, combat, config, kind, at, 0.0, false, 0x1234_5678_u32.wrapping_mul(i as u32 + 7) ^ 0x9E37_79B9);
        commands.entity(e).insert(SandboxEnemy);
        order.push((e, at));
    }
    if !skipped.is_empty() {
        info!("sandbox: too big for the line-up (hitRadius > {MAX_RADIUS} m), name one alone to see it: {}", skipped.join(", "));
    }
    order
}

/// `SHINOBI_SANDBOX_AT=x,z[,yaw]`: Wolf starts there, facing yaw degrees (the camera behind
/// him).
pub fn place_wolf(config: Res<GameConfig>, mut done: Local<bool>, mut wolf: Query<(&mut Transform, &mut Actor), With<Player>>, mut camera: Query<&mut OrbitCamera>) {
    if *done || !map::map_id(&config).as_deref().is_some_and(is_generated) {
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
pub fn watch(
    time: Res<Time>,
    mut done: Local<bool>,
    ai_debug: Option<Res<crate::enemy::EnemyDebug>>,
    enemies: Query<(&crate::enemy::Enemy, &Actor, &Transform)>,
    wolf: Query<(&Actor, &Transform), With<Player>>,
    meshes: Query<(Entity, &GlobalTransform, Option<&Name>, Option<&ChildOf>, Option<&bevy::camera::primitives::Aabb>), (With<Mesh3d>, Without<crate::map::MapPiece>)>,
    names: Query<&Name>,
    pieces: Query<(Entity, &bevy::camera::primitives::Aabb, &ViewVisibility, &Mesh3d), With<MapPiece>>,
    map_materials: Res<Assets<MapMaterial>>,
    asset_server: Res<AssetServer>,
) {
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
    // Every mesh (map pieces aside) within 6 m of Wolf: what is that thing at his feet?
    if let Ok((_, wtf)) = wolf.single() {
        for (entity, gtf, name, parent, aabb) in &meshes {
            let p = gtf.translation();
            // Near Wolf, or a bigger mesh whose bounds reach under him (a far pivot hides a floor-sized quad).
            let over_wolf = aabb.is_some_and(|a| {
                let c = gtf.transform_point(a.center.into());
                let he = Vec3::from(a.half_extents) * gtf.scale().abs();
                (wtf.translation.x - c.x).abs() <= he.x + 1.0 && (wtf.translation.z - c.z).abs() <= he.z + 1.0 && (wtf.translation.y - c.y).abs() <= he.y + 3.0
            });
            if p.distance(wtf.translation) > 6.0 && !over_wolf {
                continue;
            }
            let parent_name = parent.and_then(|c| names.get(c.parent()).ok()).map_or("-", |n| n.as_str());
            info!("  mesh {entity} {:?} under {parent_name} at {:.2} {:.2} {:.2} scale {:.2?} aabb {:?}", name.map(|n| n.as_str()).unwrap_or("-"), p.x, p.y, p.z, gtf.scale(), aabb.map(|a| (a.center, a.half_extents)));
        }
        // Map material textures that are not loaded (a material with one never draws).
        let mut unloaded = 0;
        for (id, m) in map_materials.iter() {
            let e = &m.extension;
            for (what, h) in [("albedo", &m.base.base_color_texture), ("normal", &e.normal), ("over_albedo", &e.over_albedo), ("over_normal", &e.over_normal), ("snow_albedo", &e.snow_albedo), ("snow_normal", &e.snow_normal), ("c_albedo", &e.c_albedo), ("c_normal", &e.c_normal), ("mask", &e.mask)] {
                if let Some(h) = h {
                    let state = asset_server.get_load_state(h.id());
                    if !matches!(state, Some(bevy::asset::LoadState::Loaded)) {
                        unloaded += 1;
                        info!("  material {id:?} {what} {:?}: {state:?}", h.path().map(|p| p.to_string()));
                    }
                }
            }
        }
        info!("  map materials: {} ({unloaded} textures not loaded)", map_materials.len());
        // Map pieces whose bounds reach under Wolf: drawn or culled?
        info!("  map pieces with bounds: {} (Wolf at {:.2} {:.2} {:.2})", pieces.iter().count(), wtf.translation.x, wtf.translation.y, wtf.translation.z);
        for (entity, a, vis, mesh) in &pieces {
            let (c, he) = (Vec3::from(a.center), Vec3::from(a.half_extents));
            let under = (wtf.translation.x - c.x).abs() <= he.x && (wtf.translation.z - c.z).abs() <= he.z && wtf.translation.y >= c.y - he.y - 3.0 && wtf.translation.y <= c.y + he.y + 3.0;
            info!("  piece {entity} mesh {:?} centre {:.1} {:.1} {:.1} half {:.1} {:.1} {:.1} visible {} under {under}", mesh.0.id(), c.x, c.y, c.z, he.x, he.y, he.z, vis.get());
        }
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

/// The `[world] map` / `SHINOBI_MAP` value of the Construct: the sandbox with the gallery off
/// and the parkour course and stealth courtyard built in (2026-10-17, the user: "make a test map
/// for parkour"; renamed from "parkour" the same day: "the default map should be renamed to
/// construct").
pub const CONSTRUCT_ID: &str = "construct";
/// Its old name, still taken (map.rs map_id turns it into `CONSTRUCT_ID`).
pub const PARKOUR_ALIAS: &str = "parkour";

/// The Construct is loaded (map.rs sets it with the map): no victory screen when its guards are
/// all down (menu.rs watch_fight), it is a place to practise, not a fight.
pub static ON_CONSTRUCT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The outpost behind the start (2026-10-17, the user: "revamp stealth section to match the layout
/// of outpost in Ashina Outskirts after General Naomori Kawarada ... this section needs some type
/// of vertical movement, also add bushes"), laid out after the user's captures of it: a dirt road
/// climbing between a two-storey house with its shed (left of the road, seen from the start) and
/// a stone terrace with stairs up its front and a watchtower on it (right), a second house at
/// the back, crates along the way, a fence across the front with a gap for the road. Wolf
/// climbs by the hang ledges round every roof and lip and by the grapple points
/// (`construct_grapple`). Bevy metres: +X is on Wolf's left as he faces the outpost (+Z).
struct Outpost;

impl Outpost {
    const FENCE_L: (Vec3, Vec3) = (Vec3::new(2.0, 0.0, 11.0), Vec3::new(30.0, 2.5, 11.5));
    const FENCE_R: (Vec3, Vec3) = (Vec3::new(-30.0, 0.0, 11.0), Vec3::new(-2.0, 2.5, 11.5));
    const HOUSE: (Vec3, Vec3) = (Vec3::new(7.0, 0.0, 18.0), Vec3::new(15.0, 5.0, 26.0));
    const SHED: (Vec3, Vec3) = (Vec3::new(15.0, 0.0, 19.0), Vec3::new(19.0, 2.2, 25.0));
    const BACK_HOUSE: (Vec3, Vec3) = (Vec3::new(8.0, 0.0, 34.0), Vec3::new(18.0, 4.5, 42.0));
    const TERRACE: (Vec3, Vec3) = (Vec3::new(-28.0, 0.0, 26.0), Vec3::new(-6.0, 4.0, 44.0));
    const TOWER: (Vec3, Vec3) = (Vec3::new(-24.0, 0.0, 38.0), Vec3::new(-20.0, 8.0, 42.0));
    const CRATES: [(Vec3, Vec3); 4] = [
        (Vec3::new(3.0, 0.0, 14.0), Vec3::new(4.5, 1.6, 15.5)),
        (Vec3::new(-4.5, 0.0, 31.0), Vec3::new(-3.0, 1.6, 32.5)),
        (Vec3::new(4.0, 0.0, 37.0), Vec3::new(5.5, 1.6, 38.5)),
        (Vec3::new(-14.0, 4.0, 32.0), Vec3::new(-12.5, 5.6, 33.5)),
    ];
    /// The stairs up the terrace's front (x -9.5 .. -6.5, z 16 .. 26): 10 steps of 0.4 m.
    const STAIRS_X: (f32, f32) = (-9.5, -6.5);
    const STAIRS_Z: (f32, f32) = (16.0, 26.0);

    fn boxes() -> Vec<(Vec3, Vec3)> {
        let mut b = vec![Self::FENCE_L, Self::FENCE_R, Self::HOUSE, Self::SHED, Self::BACK_HOUSE, Self::TERRACE, Self::TOWER];
        b.extend(Self::CRATES);
        b
    }

    fn stairs() -> Vec<(Vec3, Vec3)> {
        let (x0, x1) = Self::STAIRS_X;
        let (z0, z1) = Self::STAIRS_Z;
        (0..10).map(|i| (Vec3::new(x0, 0.0, z0 + i as f32), Vec3::new(x1, 0.4 * (i + 1) as f32, z1))).collect()
    }

    /// The stairs' walking ramp (origin, across, up the slope).
    fn ramp() -> (Vec3, Vec3, Vec3) {
        let (x0, x1) = Self::STAIRS_X;
        let (z0, z1) = Self::STAIRS_Z;
        (Vec3::new(x0, 0.0, z0), Vec3::new(x1 - x0, 0.0, 0.0), Vec3::new(0.0, Self::TERRACE.1.y, z1 - z0))
    }

    fn edges() -> Vec<map::LedgeEdge> {
        let mut e = Vec::new();
        let all = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];
        for side in all {
            lip(&mut e, Self::HOUSE, side, true, None);
            lip(&mut e, Self::BACK_HOUSE, side, true, None);
            lip(&mut e, Self::TOWER, side, true, None);
        }
        for side in [Vec3::X, Vec3::Z, Vec3::NEG_Z] {
            lip(&mut e, Self::SHED, side, true, None);
        }
        // The terrace: its road side whole, its front up to the stairs.
        lip(&mut e, Self::TERRACE, Vec3::X, true, None);
        lip(&mut e, Self::TERRACE, Vec3::NEG_Z, true, Some((Self::TERRACE.0.x, Self::STAIRS_X.0)));
        lip(&mut e, Self::TERRACE, Vec3::X, false, None);
        lip(&mut e, Self::TERRACE, Vec3::NEG_Z, false, Some((Self::TERRACE.0.x, Self::STAIRS_X.0)));
        // The houses' road-side walls and fronts, and the fence's faces, to hug.
        for (b, sides) in [(Self::HOUSE, &[Vec3::NEG_X, Vec3::NEG_Z][..]), (Self::BACK_HOUSE, &[Vec3::NEG_X, Vec3::NEG_Z][..]), (Self::FENCE_L, &[Vec3::Z, Vec3::NEG_Z][..]), (Self::FENCE_R, &[Vec3::Z, Vec3::NEG_Z][..])] {
            for &side in sides {
                lip(&mut e, b, side, false, None);
            }
        }
        e
    }
}

/// A hang edge round box `b`'s top on its `side` face (outward normal), or with `hang` false a
/// hug edge along its foot; `span` limits it along the face (x for the Z faces, z for the X).
/// The edge runs along (n.z, 0, -n.x), as the course's outline does.
fn lip(e: &mut Vec<map::LedgeEdge>, b: (Vec3, Vec3), n: Vec3, hang: bool, span: Option<(f32, f32)>) {
    let (lo, hi) = b;
    let y = if hang { hi.y } else { lo.y };
    let (p, q) = if n.z != 0.0 {
        let z = if n.z > 0.0 { hi.z } else { lo.z };
        let (a, c) = span.unwrap_or((lo.x, hi.x));
        (Vec3::new(a, y, z), Vec3::new(c, y, z))
    } else {
        let x = if n.x > 0.0 { hi.x } else { lo.x };
        let (a, c) = span.unwrap_or((lo.z, hi.z));
        (Vec3::new(x, y, a), Vec3::new(x, y, c))
    };
    let dir = Vec3::new(n.z, 0.0, -n.x);
    let (p0, p1) = if (q - p).dot(dir) >= 0.0 { (p, q) } else { (q, p) };
    e.push(if hang { hang_edge(p0, p1, n) } else { hug_edge(p0, p1, n) });
}

/// The outpost's grapple points (variation 1, as the gate map's roofs and branches): the hook on
/// a roof's lip, the landing just inside it, `fwd` the face's outward normal (Wolf fires from
/// that side). The two houses, the terrace front and the watchtower.
pub fn construct_grapple() -> Vec<map::GrapplePoint> {
    let point = |x: f32, top: f32, z: f32, fwd: Vec3| map::GrapplePoint { hook: Vec3::new(x, top - 0.2, z), land: Vec3::new(x, top, z) - fwd * 0.7, fwd, variation: 1 };
    vec![
        point(Outpost::HOUSE.0.x, Outpost::HOUSE.1.y, 22.0, Vec3::NEG_X),
        point(11.0, Outpost::HOUSE.1.y, Outpost::HOUSE.0.z, Vec3::NEG_Z),
        point(13.0, Outpost::BACK_HOUSE.1.y, Outpost::BACK_HOUSE.0.z, Vec3::NEG_Z),
        point(-18.0, Outpost::TERRACE.1.y, Outpost::TERRACE.0.z, Vec3::NEG_Z),
        point(Outpost::TERRACE.1.x, Outpost::TERRACE.1.y, 34.0, Vec3::X),
        point(-22.0, Outpost::TOWER.1.y, Outpost::TOWER.0.z, Vec3::NEG_Z),
        point(Outpost::TOWER.1.x, Outpost::TOWER.1.y, 40.0, Vec3::X),
    ]
}

/// The Construct's pool, left of the start (2026-10-18, the user: "swimming now, put the space
/// with water from the left of spawn"): its pit (low corner at its bottom, high corner at the
/// floor), the water `POOL_SURFACE` deep below the floor (swim.rs WATER), 4.6 m of it over the
/// bottom to dive in.
pub const POOL: (Vec3, Vec3) = (Vec3::new(-40.0, -5.0, -8.0), Vec3::new(-20.0, 0.0, 8.0));
const POOL_SURFACE: f32 = -0.4;
/// The ramp out at the pool's west end (x -40 .. -30, z -3 .. 3): from the floor at its wall down
/// to the bottom (origin, down the slope, across), its normal up.
const POOL_RAMP: (Vec3, Vec3, Vec3) = (Vec3::new(-40.0, 0.0, -3.0), Vec3::new(10.0, -5.0, 0.0), Vec3::new(0.0, 0.0, 6.0));

/// The pool's water: the swim box (swim.rs WATER) and its surface, translucent with the game's
/// water ripple normal (m99_water_n, as map.rs's water pieces), seen from below too.
fn spawn_pool(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, assets: &AssetServer) {
    let (lo, hi) = POOL;
    *crate::swim::WATER.write().unwrap() = vec![(lo, Vec3::new(hi.x, POOL_SURFACE, hi.z))];
    let mut water = Patch::default();
    water.grid(Vec3::new(lo.x, POOL_SURFACE, lo.z), Vec3::new(hi.x - lo.x, 0.0, 0.0), Vec3::new(0.0, 0.0, hi.z - lo.z), 8, 6, 3.0, |_, _| [0.0; 4]);
    let normal = crate::paths::extracted().join("tex/m99_water_n.dds").is_file().then(|| {
        assets.load_builder().with_settings(|s: &mut bevy::image::ImageLoaderSettings| s.is_srgb = false).load("tex/m99_water_n.dds")
    });
    let mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.45, 0.52, 0.42),
        normal_map_texture: normal,
        perceptual_roughness: 0.08,
        reflectance: 0.6,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((crate::map::MapPiece, Mesh3d(meshes.add(water.mesh(false, true))), MeshMaterial3d(mat), Transform::default(), Visibility::default(), bevy::light::NotShadowCaster));
}

/// The outpost's bushes (centre on the ground, radius): clumps of the game's tall grass
/// (o007160, the Outskirts' grass object) Wolf hides in crouched (`in_bush`).
const BUSHES: [(Vec3, f32); 6] = [
    (Vec3::new(-3.0, 0.0, 19.5), 1.6),
    (Vec3::new(5.0, 0.0, 22.0), 1.4),
    (Vec3::new(11.0, 0.0, 30.0), 1.8),
    (Vec3::new(17.5, 0.0, 15.0), 1.8),
    (Vec3::new(-1.5, 0.0, 38.0), 1.6),
    (Vec3::new(-17.0, 4.0, 31.0), 1.6),
];

/// Is `feet` inside one of the outpost's bushes (and the Construct loaded)?
pub fn in_bush(feet: Vec3) -> bool {
    ON_CONSTRUCT.load(std::sync::atomic::Ordering::Relaxed) && BUSHES.iter().any(|(c, r)| (feet - *c).with_y(0.0).length() < *r && (feet.y - c.y).abs() < 1.0)
}

/// The bushes' clumps: o007160 (extracted/objmesh/o007160.json, `sekiro-extract objmesh`; its
/// texture m11_o007160_grass_01_a, alpha-tested, both sides) six to a bush, 1.4 wide and 1.8
/// tall (the clump's blades fan out low: at 1.4 a crouched Wolf showed over them). X mirrored as every FromSoftware position.
pub fn spawn_bushes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, assets: Res<AssetServer>) {
    if !ON_CONSTRUCT.load(std::sync::atomic::Ordering::Relaxed) {
        crate::swim::WATER.write().unwrap().clear();
        return;
    }
    spawn_pool(&mut commands, &mut meshes, &mut mats, &assets);
    let path = crate::paths::extracted().join("objmesh/o007160.json");
    let Some(json) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
        warn!("construct: no {} (tools/extract.ps1 exports it): no bushes drawn", path.display());
        return;
    };
    let floats = |v: &serde_json::Value| v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect::<Vec<f32>>()).unwrap_or_default();
    let tex = "tex/m11_o007160_grass_01_a.dds";
    let albedo = crate::paths::extracted().join(tex).is_file().then(|| assets.load(tex));
    let mat = mats.add(StandardMaterial {
        base_color_texture: albedo,
        alpha_mode: AlphaMode::Mask(0.5),
        double_sided: true,
        cull_mode: None,
        perceptual_roughness: 0.9,
        ..default()
    });
    let mut parts = Vec::new();
    for m in json["meshes"].as_array().into_iter().flatten() {
        let (pos, nrm, uv) = (floats(&m["pos"]), floats(&m["normal_vec"]), floats(&m["uv"]));
        let idx: Vec<u32> = m["indices"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|i| i as u32)).collect()).unwrap_or_default();
        let n = pos.len() / 3;
        if n == 0 || idx.len() < 3 {
            continue;
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.chunks(3).map(|p| [-p[0], p[1], p[2]]).collect::<Vec<[f32; 3]>>());
        if nrm.len() == pos.len() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm.chunks(3).map(|p| [-p[0], p[1], p[2]]).collect::<Vec<[f32; 3]>>());
        }
        if uv.len() / 2 == n {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.chunks(2).map(|p| [p[0], p[1]]).collect::<Vec<[f32; 2]>>());
        }
        mesh.insert_indices(Indices::U32(idx));
        parts.push(meshes.add(mesh));
    }
    let mut k = 0u32;
    for (c, r) in BUSHES {
        for i in 0..6 {
            k = k.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let at = if i == 0 { c } else { c + Quat::from_rotation_y(i as f32 * std::f32::consts::TAU / 5.0) * Vec3::X * r * 0.55 };
            let yaw = (k >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
            let tf = Transform::from_translation(at + Vec3::Y * 0.45).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::new(1.4, 1.8, 1.4));
            for mesh in &parts {
                commands.spawn((crate::map::MapPiece, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), tf, Visibility::default(), bevy::light::NotShadowCaster));
            }
        }
    }
    info!("construct: {} bushes drawn", BUSHES.len());
}

/// The Construct's second section: an empty walled square as big as the first (x -HALF .. HALF,
/// z -3 HALF .. -HALF) north of it, through a `DOOR_HALF` x 2 opening in the shared wall where the
/// floor's road (the base-material strip down the middle, x -6 .. 6) meets it. The config's
/// enemy (enemy.chr, its group, a custom fight) stands in it, away from the course and the
/// courtyard (the user: "isolated in a different section ... comparable to the size of the
/// current map").
const DOOR_HALF: f32 = 3.0;
/// Where the section's enemy stands (its middle), facing the door (+Z: forward = Ry(yaw) * -Z).
pub const ARENA_SPOT: Vec3 = Vec3::new(0.0, 0.0, -2.0 * HALF);
pub const ARENA_YAW: f32 = std::f32::consts::PI;

/// An outpost guard (`construct_patrols`): always the real AI (T leaves it be), so it fights
/// once it finds Wolf.
#[derive(Component)]
pub struct CourtyardGuard;

/// The outpost's guards (chr `GUARD_CHR`): where each starts (its height on the ground or the
/// terrace / tower), facing, and its round (ai::Patrol; a 2 s look-around wait at each point).
/// One paces the road, one walks between the houses, one the terrace; one keeps the tower, one
/// the far end of the road, watching the gate.
pub const GUARD_CHR: &str = "c1010";

pub fn construct_patrols() -> Vec<(Vec3, f32, crate::ai::Patrol)> {
    let route = |kind: i64, pts: &[(f32, f32, f32)]| {
        let points: Vec<Vec3> = pts.iter().map(|&(x, y, z)| Vec3::new(x, y, z)).collect();
        let arrivals = (0..points.len() as i64).map(|i| (i, -1, 2.0)).collect();
        let at = points[0];
        // Facing the second point (forward = Ry(yaw) * -Z), or the gate (-Z) when alone.
        let yaw = points.get(1).map_or(0.0, |n| f32::atan2(-(n.x - at.x), -(n.z - at.z)));
        (at, yaw, crate::ai::Patrol::new(kind, points, Vec::new(), arrivals))
    };
    vec![
        route(1, &[(1.0, 0.0, 14.0), (1.0, 0.0, 42.0)]),
        route(0, &[(6.0, 0.0, 28.0), (19.0, 0.0, 28.0), (19.0, 0.0, 32.0), (6.0, 0.0, 32.0)]),
        route(0, &[(-26.0, 4.0, 29.0), (-10.0, 4.0, 29.0), (-10.0, 4.0, 36.0), (-26.0, 4.0, 36.0)]),
        route(0, &[(-22.0, 8.0, 40.0)]),
        route(0, &[(0.0, 0.0, 43.5)]),
    ]
}

/// Is this map id the sandbox (with or without the parkour course)?
pub fn is_generated(id: &str) -> bool {
    id == MAP_ID || id == CONSTRUCT_ID
}

impl Patch {
    /// A solid box from `lo` to `hi`: its six faces, normals outward (the bottom shows under the
    /// shaft's shelves).
    fn cuboid(&mut self, lo: Vec3, hi: Vec3) {
        let d = hi - lo;
        let blend = |_: f32, _: f32| [0.0; 4];
        self.grid(Vec3::new(lo.x, hi.y, lo.z), Vec3::new(d.x, 0.0, 0.0), Vec3::new(0.0, 0.0, d.z), 2, 2, TILE_M, blend);
        if lo.y > 0.0 {
            self.grid(Vec3::new(lo.x, lo.y, lo.z), Vec3::new(0.0, 0.0, d.z), Vec3::new(d.x, 0.0, 0.0), 2, 2, TILE_M, blend);
        }
        self.grid(Vec3::new(hi.x, lo.y, hi.z), Vec3::new(-d.x, 0.0, 0.0), Vec3::new(0.0, d.y, 0.0), 2, 2, TILE_M, blend);
        self.grid(Vec3::new(lo.x, lo.y, lo.z), Vec3::new(d.x, 0.0, 0.0), Vec3::new(0.0, d.y, 0.0), 2, 2, TILE_M, blend);
        self.grid(Vec3::new(hi.x, lo.y, lo.z), Vec3::new(0.0, 0.0, d.z), Vec3::new(0.0, d.y, 0.0), 2, 2, TILE_M, blend);
        self.grid(Vec3::new(lo.x, lo.y, hi.z), Vec3::new(0.0, 0.0, -d.z), Vec3::new(0.0, d.y, 0.0), 2, 2, TILE_M, blend);
    }
}

/// A hug edge along a wall's foot from `p0` to `p1` (on the floor, at the face), `n` the face's
/// outward normal: 0.1 m out and 0.15 m up, as the game's records sit, `c` on the wall side
/// (docking.rs: the wall is on c's side). Variation 1: peek at both ends.
fn hug_edge(p0: Vec3, p1: Vec3, n: Vec3) -> map::LedgeEdge {
    let (a, b) = (p0 + n * 0.1 + Vec3::Y * 0.15, p1 + n * 0.1 + Vec3::Y * 0.15);
    map::LedgeEdge { a, b, c: (a + b) * 0.5 - n * 0.2 - Vec3::Y * 0.17, kind: 3, variation: 1 }
}

/// A hang edge along a lip from `p0` to `p1` (at the top), `n` the face's outward normal; `c` on
/// the wall side, under the lip.
fn hang_edge(p0: Vec3, p1: Vec3, n: Vec3) -> map::LedgeEdge {
    map::LedgeEdge { a: p0, b: p1, c: (p0 + p1) * 0.5 - n * 0.3 - Vec3::Y * 0.5, kind: 2, variation: 0 }
}

/// The stairs up the L platform's west side (x -1.5 .. 2, z -12 .. -10), 7 steps of 0.43 m:
/// drawn, while their collision is the ramp under them (`STAIRS_RAMP`), as the game's stairs are
/// (step by step, the floor snap popped Wolf up and down each step: the user, "not smooth").
pub fn parkour_stairs() -> Vec<(Vec3, Vec3)> {
    let mut s: Vec<(Vec3, Vec3)> = (0..7).map(|i| (Vec3::new(-1.5 + 0.5 * i as f32, 0.0, -12.0), Vec3::new(2.0, 0.43 * (i + 1) as f32, -10.0))).collect();
    s.extend(Outpost::stairs());
    s
}

/// The parkour course (metres; Wolf starts at the origin facing -Z, the duel enemy 4 m ahead):
/// 1. a free-standing hug wall, 2.5 m high, both faces huggable with a peek at both ends:
///    x -7..-1, z -9.5..-9;
/// 2. an L-shaped platform 3 m high (x 2..8, z -14..-8 and x 8..12, z -14..-11) with hang
///    ledges all round its lip (outside corners, and the inside corner at (8, -11)), stairs up its
///    west side (0.43 m steps) for the ground hang and the climb;
/// 3. a 4.5 m cliff (x -14..-7, z -24..-20) with a hug edge at its foot and a hang ledge on its
///    lip: hug, jump straight up the wall (CoverToWallJumpReady -> WallJumpStart_B_N) and catch
///    the ledge with E held; or a wall jump from a running jump at it.
/// 4. a tall tight shaft (inside x 14..17, z -24..-21, 12 m high, a door in its south wall) for
///    wall-to-wall chained wall jumps (the map lifts the one-a-flight limit:
///    docking::WALL_JUMP_LIMIT).
/// The arena's own walls (5 m) take wall jumps anywhere.
pub fn parkour_course() -> (Vec<(Vec3, Vec3)>, Vec<map::LedgeEdge>) {
    let mut boxes = Vec::new();
    let mut edges = Vec::new();
    // 1. The hug wall.
    let (wl, wh) = (Vec3::new(-7.0, 0.0, -9.5), Vec3::new(-1.0, 2.5, -9.0));
    boxes.push((wl, wh));
    edges.push(hug_edge(Vec3::new(wl.x, 0.0, wh.z), Vec3::new(wh.x, 0.0, wh.z), Vec3::Z));
    edges.push(hug_edge(Vec3::new(wh.x, 0.0, wl.z), Vec3::new(wl.x, 0.0, wl.z), Vec3::NEG_Z));
    // 2. The L platform and its lip, round the outline (each corner shared by two edges).
    let h = 3.0;
    boxes.push((Vec3::new(2.0, 0.0, -14.0), Vec3::new(8.0, h, -8.0)));
    boxes.push((Vec3::new(8.0, 0.0, -14.0), Vec3::new(12.0, h, -11.0)));
    let outline = [(2.0, -8.0), (8.0, -8.0), (8.0, -11.0), (12.0, -11.0), (12.0, -14.0), (2.0, -14.0)];
    let inside = |x: f32, z: f32| (x > 2.0 && x < 8.0 && z > -14.0 && z < -8.0) || (x > 8.0 && x < 12.0 && z > -14.0 && z < -11.0);
    for k in 0..outline.len() {
        let (p, q) = (outline[k], outline[(k + 1) % outline.len()]);
        let (p0, p1) = (Vec3::new(p.0, h, p.1), Vec3::new(q.0, h, q.1));
        let along = (p1 - p0).normalize();
        let mut n = Vec3::Y.cross(along);
        let mid = (p0 + p1) * 0.5;
        if inside(mid.x + n.x * 0.2, mid.z + n.z * 0.2) {
            n = -n;
        }
        // The west lip is cut where the stairs meet it (z -12 .. -10): no hanging into the steps.
        if p0.x == 2.0 && p1.x == 2.0 {
            let (z0, z1) = (p0.z.min(p1.z), p0.z.max(p1.z));
            for (a, b) in [(z0, -12.0), (-10.0, z1)] {
                edges.push(hang_edge(Vec3::new(2.0, h, a), Vec3::new(2.0, h, b), n));
            }
            continue;
        }
        edges.push(hang_edge(p0, p1, n));
    }
    // 3. The cliff: its foot huggable, its lip a hang ledge.
    let (cl, ch) = (Vec3::new(-14.0, 0.0, -24.0), Vec3::new(-7.0, 4.5, -20.0));
    boxes.push((cl, ch));
    edges.push(hug_edge(Vec3::new(cl.x, 0.0, ch.z), Vec3::new(ch.x, 0.0, ch.z), Vec3::Z));
    edges.push(hang_edge(Vec3::new(cl.x, ch.y, ch.z), Vec3::new(ch.x, ch.y, ch.z), Vec3::Z));
    // 4. The wall-jump shaft (2026-10-17, the user: "a tight and tall room so Wolf can walljump
    // around").
    let (sx0, sx1, sz0, sz1, sh, t) = (14.0, 17.0, -24.0, -21.0, 12.0, 0.5);
    boxes.push((Vec3::new(sx0 - t, 0.0, sz0 - t), Vec3::new(sx0, sh, sz1 + t)));
    boxes.push((Vec3::new(sx1, 0.0, sz0 - t), Vec3::new(sx1 + t, sh, sz1 + t)));
    boxes.push((Vec3::new(sx0, 0.0, sz0 - t), Vec3::new(sx1, sh, sz0)));
    // The south wall, its door x 15..16.2 up to 2.6 m.
    boxes.push((Vec3::new(sx0, 0.0, sz1), Vec3::new(15.0, sh, sz1 + t)));
    boxes.push((Vec3::new(16.2, 0.0, sz1), Vec3::new(sx1, sh, sz1 + t)));
    boxes.push((Vec3::new(15.0, 2.6, sz1), Vec3::new(16.2, sh, sz1 + t)));
    // 5. The stealth outpost behind the start (`Outpost`).
    boxes.extend(Outpost::boxes());
    edges.extend(Outpost::edges());
    (boxes, edges)
}

#[cfg(test)]
mod parkour_tests {
    use bevy::prelude::*;

    /// The guards' rounds stay clear of the outpost's buildings (half a metre of body room; a
    /// box under the route's height is its floor), inside the outpost, and each stands on
    /// something: the ground, the terrace or the tower.
    #[test]
    fn the_guards_walk_clear_of_the_walls() {
        let boxes = super::Outpost::boxes();
        let near_wall = |p: Vec3| boxes.iter().any(|(lo, hi)| hi.y > p.y + 0.1 && p.x > lo.x - 0.5 && p.x < hi.x + 0.5 && p.z > lo.z - 0.5 && p.z < hi.z + 0.5);
        let floor = |p: Vec3| p.y == 0.0 || boxes.iter().any(|(lo, hi)| (hi.y - p.y).abs() < 0.01 && p.x > lo.x && p.x < hi.x && p.z > lo.z && p.z < hi.z);
        for (_, _, route) in super::construct_patrols() {
            let n = route.points.len();
            for i in 0..n {
                let (a, b) = (route.points[i], route.points[(i + 1) % n]);
                for k in 0..=40 {
                    let p = a.lerp(b, k as f32 / 40.0);
                    assert!(!near_wall(p), "route point {p} is in a wall");
                    assert!(floor(p), "route point {p} stands on nothing");
                    assert!(p.z > 11.5 && p.z < 44.5 && p.x.abs() < 30.0, "{p} is outside the outpost");
                }
            }
        }
    }

    /// Every grapple point's landing is on its roof, and its hook at the lip.
    #[test]
    fn the_grapple_points_land_on_their_roofs() {
        let boxes = super::Outpost::boxes();
        for p in super::construct_grapple() {
            assert!(boxes.iter().any(|(lo, hi)| (hi.y - p.land.y).abs() < 0.01 && p.land.x > lo.x && p.land.x < hi.x && p.land.z > lo.z && p.land.z < hi.z), "{p:?} lands on no roof");
        }
    }

    /// Every course edge docks Wolf on the open side of its wall: hugging, his spot is outside
    /// the boxes; hanging, the spot is off the platform.
    #[test]
    fn the_course_edges_face_out_of_their_walls() {
        let (boxes, edges) = super::parkour_course();
        let inside = |p: Vec3| boxes.iter().any(|(lo, hi)| p.x > lo.x + 0.01 && p.x < hi.x - 0.01 && p.z > lo.z + 0.01 && p.z < hi.z - 0.01 && p.y < hi.y);
        let outpost = super::Outpost::edges();
        assert_eq!(edges.iter().filter(|e| e.kind == 3).count(), 3 + outpost.iter().filter(|e| e.kind == 3).count());
        assert_eq!(edges.iter().filter(|e| e.kind == 2).count(), 8 + outpost.iter().filter(|e| e.kind == 2).count());
        for e in &edges {
            let d = crate::docking::Dock::new_from_edge(e).expect("edge too short");
            let mid = (e.a + e.b) * 0.5;
            // Half a metre out from the edge along `out` is open air / floor, half a metre the
            // other way is wall.
            let out = mid + d.out * 0.5 + Vec3::Y * 0.1 - if e.kind == 2 { Vec3::Y * 0.6 } else { Vec3::ZERO };
            let back = mid - d.out * 0.5 - if e.kind == 2 { Vec3::Y * 0.6 } else { Vec3::ZERO } + Vec3::Y * 0.1;
            assert!(!inside(out), "kind {} edge {:?} -> {:?}: out side {:?} is inside a box", e.kind, e.a, e.b, out);
            assert!(inside(back), "kind {} edge {:?} -> {:?}: wall side {:?} is open", e.kind, e.a, e.b, back);
        }
    }
}
