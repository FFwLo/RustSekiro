//! The game's own visual effects (FXR), played from the installed files.
//!
//! Our reader ([`sekiro_formats::fxr`]) parses an effect and [`sekiro_formats::fxr_json`] turns it
//! into a named tree; [`crate::fxr_tree`] runs that tree (nodes, emitters, particles) and this
//! module draws it: sprites (600, 602, 603), flipbooks, multi-texture sprites (604), tracers
//! (606, 10012) batched into one dynamic mesh per texture and blend mode, with soft edges where a
//! sprite meets the scene. The player, its batching and soft particles follow FwLo's RustSekiro
//! (MIT, see `CREDITS.md`); the field layouts come from EvenTorset's FXR library (Unlicense).
//!
//! Effects are requested by the simulation: TAE events ([`FxrPlayer::observe`]), bullets, and hits
//! (`sekiro_sim::hit_fx`, [`FxrPlayer::hits`]). `--no-fxr` turns the player off so the
//! hand-made stand-ins draw instead; `--fxr-preview <id>` repeats one effect between the actors.

use std::{collections::HashMap, path::PathBuf};

use bevy::{
    asset::{RenderAssetUsages, embedded_asset, embedded_path},
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use sekiro_sim::duel::{Duel, DuelReport};

use crate::fxr_tree::{
    Action, Config, Field, FxrLib, NodeInst, Rnd, frame_uvs, grow_trails, int, make_node,
    particle_color, prop, scalar, step_node, subdivide, view_fade,
};

#[path = "fxr_stream.rs"]
mod stream;

/// Live effects and particles are bounded so a runaway effect cannot sink the frame.
const EFFECT_CAP: usize = 192;
const PARTICLE_CAP: usize = 8000;
/// An effect with no end is stopped after this long, and dropped after [`EFFECT_DROP`].
const EFFECT_STOP: f32 = 20.0;
const EFFECT_DROP: f32 = 30.0;
/// Who asked: `(actor, effect id, dummy)` for TAE events, `(2, id, serial)` for bullets,
/// `(3, id, n)` for hits, `(HELD, id, serial)` and `(GLINTS, id, item)` for
/// [`FxrPlayer::hold`], `(5, id, serial)` for map effects, `(IDOL_REST, id, n)` for
/// [`FxrPlayer::one_shot`].
type Key = (usize, i32, i32);
/// [`FxrPlayer::hold`] namespaces: map entities (fog walls), and the map items' glints.
pub const HELD: usize = 4;
pub const GLINTS: usize = 6;
/// [`FxrPlayer::one_shot`] namespace: the burst of a rest at an idol.
pub const IDOL_REST: usize = 7;

/// A plugin-supplied bridge keeps the simulation private to the play module.
pub fn plugin(cache: PathBuf, bridge: impl Fn(&mut App) + Send + Sync + 'static) -> impl Plugin {
    struct PlayerPlugin<S> {
        cache: PathBuf,
        bridge: S,
    }
    impl<S: Fn(&mut App) + Send + Sync + 'static> Plugin for PlayerPlugin<S> {
        fn build(&self, app: &mut App) {
            if app.world().contains_resource::<FxrPlayer>() {
                (self.bridge)(app);
                return;
            }
            // DistortMaterial samples the opaque scene, as Camera3d did by default in 0.19.
            app.register_required_components::<Camera3d, bevy::pbr::ScreenSpaceTransmission>();
            embedded_asset!(app, "fxr.wesl");
            embedded_asset!(app, "fxr_multi.wesl");
            embedded_asset!(app, "fxr_distort.wesl");
            app.add_plugins((
                MaterialPlugin::<FxrMaterial>::default(),
                MaterialPlugin::<MultiMaterial>::default(),
                MaterialPlugin::<DistortMaterial>::default(),
            ));
            stream::install(app);
            app.insert_resource(FxrPlayer::new(self.cache.clone()))
                .init_resource::<DepthCopy>()
                .add_systems(Startup, setup_depth)
                .add_systems(Update, (render, soft_depth));
            if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
                use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
                // After the main pass: the particles of the next frame read this frame's depth
                // (one frame late, which a soft fade does not show).
                render_app.add_systems(Core3d, copy_depth.after(Core3dSystems::MainPass));
            }
            app.add_plugins(bevy::render::extract_resource::ExtractResourcePlugin::<
                DepthCopy,
            >::default());
            (self.bridge)(app);
        }
    }
    PlayerPlugin { cache, bridge }
}

fn shader(name: &'static str) -> ShaderRef {
    bevy::asset::AssetPath::from_path_buf(match name {
        "fxr.wesl" => embedded_path!("fxr.wesl"),
        "fxr_distort.wesl" => embedded_path!("fxr_distort.wesl"),
        _ => embedded_path!("fxr_multi.wesl"),
    })
    .with_source("embedded")
    .into()
}

/// One playing effect.
struct Effect {
    id: i64,
    key: Key,
    /// Where it is (and, while `follow`, where its dummy or bullet is).
    at: Transform,
    follow: bool,
    /// Stops emitting when its event ends or its key is no longer requested.
    stoppable: bool,
    stop: bool,
    /// A map effect held by [`FxrPlayer::hold`]: it neither times out nor is dropped by age
    /// while held.
    persistent: bool,
    root: Option<NodeInst>,
    age: f32,
    asleep: bool,
}

/// A stain a hit asks for: DecalParam row, contact point, blow direction on the floor plane.
pub type StainRequest = (i32, [f32; 3], [f32; 2]);

#[derive(Resource)]
pub struct FxrPlayer {
    enabled: bool,
    lib: FxrLib,
    effects: Vec<Effect>,
    batches: HashMap<(i64, bool), Batch>,
    multi_batches: HashMap<MultiKey, Batch>,
    distort_batches: HashMap<i64, Batch>,
    textures: HashMap<i64, Handle<Image>>,
    bullets: HashMap<u64, i32>,
    bullet_sfx: HashMap<i32, i32>,
    last_cues: Vec<(usize, i32, i32, u32)>,
    hit_fx: Option<sekiro_sim::hit_fx::HitFx>,
    npc_param: i32,
    hit_serial: i32,
    /// Particles alive after the last step, for logs and the frame budget.
    pub particles: usize,
    pub spawned: usize,
    pub rejected: usize,
    preview: Option<i32>,
    steps: usize,
    cache: PathBuf,
}

impl FxrPlayer {
    pub(crate) fn new(cache: PathBuf) -> Self {
        let enabled = !std::env::args().any(|a| a == "--no-fxr");
        let lib = if enabled {
            FxrLib::scan(&cache)
        } else {
            FxrLib::default()
        };
        let mut bullet_sfx = HashMap::new();
        if enabled
            && let Ok(params) = sekiro_formats::param::ParamSet::load(
                &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
                &cache.join("refs/paramdex/Defs"),
            )
            && let Some(table) = params.get("Bullet")
        {
            for row in table.rows() {
                if let Ok(sfx) = row.int("sfxId_Bullet") {
                    bullet_sfx.insert(row.id(), sfx as i32);
                }
            }
        }
        info!(
            "FXR: enabled={enabled}, indexed {} installed effects",
            lib.effect_count()
        );
        Self {
            enabled,
            lib,
            effects: Vec::new(),
            batches: HashMap::new(),
            multi_batches: HashMap::new(),
            distort_batches: HashMap::new(),
            textures: HashMap::new(),
            bullets: HashMap::new(),
            bullet_sfx,
            last_cues: Vec::new(),
            hit_fx: None,
            npc_param: -1,
            hit_serial: 0,
            particles: 0,
            spawned: 0,
            rejected: 0,
            preview: std::env::args()
                .collect::<Vec<_>>()
                .windows(2)
                .find(|a| a[0] == "--fxr-preview")
                .and_then(|a| a[1].parse().ok()),
            steps: 0,
            cache,
        }
    }

    /// Whether the player draws this bullet, so its stand-in should not. A bullet with its own
    /// installed effect is drawn by it. The blinding area has none: the smoke the Owl throws is
    /// FFX 650640 on its carrier, so the stand-in sphere is dropped when that effect exists.
    pub fn covers_bullet(
        &mut self,
        bullet_row: i32,
        look: sekiro_sim::bullets::BulletLook,
    ) -> bool {
        use sekiro_sim::bullets::BulletLook;
        let id = i64::from(self.bullet_sfx.get(&bullet_row).copied().unwrap_or(-1));
        // An effect with a model (an arrow's, a shuriken's) is only half drawn: the stand-in stays
        // under it, except for lightning, whose bolt is the tracers.
        let own = id > 0
            && self.lib.has(id)
            && (look == BulletLook::Lightning || !self.lib.has_model(id));
        self.enabled && (own || (look == BulletLook::Smoke && self.lib.has(650640)))
    }

    /// Whether this effect id is installed and can play (a stand-in is used otherwise).
    pub fn has(&mut self, id: i32) -> bool {
        self.enabled && id > 0 && self.lib.has(i64::from(id))
    }

    pub(crate) fn map_actions(&mut self, id: i32) -> (bool, bool) {
        (
            self.lib.uses_action(i64::from(id), "nodeMovement", 46),
            self.lib.uses_action(i64::from(id), "appearance", 10000),
        )
    }

    /// Loads the hit tables (effects, stains) for this fight's NPC when they are not loaded yet.
    pub(crate) fn ensure_tables(&mut self, duel: &Duel) {
        if self.enabled && (self.hit_fx.is_none() || self.npc_param != duel.spec.npc_param) {
            self.npc_param = duel.spec.npc_param;
            self.hit_fx =
                sekiro_sim::hit_fx::HitFx::load_from_cache(&self.cache, duel.spec.npc_param);
        }
    }

    /// A DecalParam row, once the tables are loaded.
    pub fn decal(&self, id: i32) -> Option<&sekiro_sim::hit_fx::DecalRow> {
        self.hit_fx.as_ref()?.decal(id)
    }

    /// Floor stains whose TAE event started this step: (DecalParam id, dummy position).
    pub fn event_stains(&self, duel: &Duel) -> Vec<(i32, [f32; 3])> {
        let mut out = Vec::new();
        for (actor, c) in [&duel.player, &duel.enemy].into_iter().enumerate() {
            for &(id, dummy) in &c.tae_frame().decals {
                let at = i16::try_from(dummy)
                    .ok()
                    .and_then(|d| duel.dummy_position(actor, d))
                    .unwrap_or(c.body.position);
                out.push((id, at));
            }
        }
        out
    }

    /// Starts an effect (`start`) or moves the one already playing under `key`.
    fn request(&mut self, id: i32, key: Key, at: Transform, start: bool, stoppable: bool) {
        if !self.enabled || id <= 0 {
            return;
        }
        if !start {
            for e in self.effects.iter_mut().filter(|e| e.key == key && e.follow) {
                e.at = at;
            }
            return;
        }
        if self.effects.len() >= EFFECT_CAP {
            return;
        }
        if !self.lib.has(i64::from(id)) {
            self.rejected += 1;
            return;
        }
        self.spawned += 1;
        if log_enabled() {
            info!("fxr spawn {id} for {key:?} at {:.2?}", at.translation);
        }
        self.effects.push(Effect {
            id: i64::from(id),
            key,
            at,
            follow: true,
            stoppable,
            stop: false,
            persistent: false,
            root: None,
            age: 0.0,
            asleep: false,
        });
    }

    pub(crate) fn live_counts(&self) -> (usize, usize, usize) {
        (
            self.effects.len(),
            self.effects.iter().filter(|e| e.persistent).count(),
            self.particles,
        )
    }

    /// Live particles of the camera-relative map effects within `radius` m (horizontally) of
    /// `at`, for the `SEKIRO_FXR_LOG` weather log (#84).
    pub(crate) fn map_particles_near(&self, id: i32, at: Vec3, radius: f32) -> usize {
        fn near(n: &NodeInst, at: Vec3, radius: f32) -> usize {
            n.particles
                .iter()
                .filter(|p| (p.pos - at).xz().length() < radius)
                .count()
                + n.children
                    .iter()
                    .map(|c| near(c, at, radius))
                    .sum::<usize>()
        }
        self.effects
            .iter()
            .filter(|e| e.key.0 == 5 && e.key.1 == id && !e.asleep)
            .filter_map(|e| e.root.as_ref())
            .map(|root| near(root, at, radius))
            .sum()
    }

    pub(crate) fn reset_fight(&mut self) {
        self.effects.retain(|e| e.persistent);
        self.bullets.clear();
        self.last_cues.clear();
        self.particles = self
            .effects
            .iter()
            .filter_map(|e| e.root.as_ref())
            .map(count_particles)
            .sum();
    }

    /// Holds a map effect (a boss fog wall) at `at` while `on`, and lets it fade out once it is
    /// off. `namespace` and `serial` tell apart effects with the same id; the effect does not
    /// time out.
    pub fn hold(&mut self, namespace: usize, id: i32, serial: i32, at: Transform, on: bool) {
        let key = (namespace, id, serial);
        let held = self
            .effects
            .iter_mut()
            .find(|e| e.key == key && e.persistent);
        match (on, held) {
            (true, None) => {
                let before = self.effects.len();
                self.request(id, key, at, true, false);
                if self.effects.len() > before
                    && let Some(e) = self.effects.last_mut()
                {
                    e.persistent = true;
                    e.follow = false;
                }
            }
            (false, Some(e)) => {
                e.persistent = false;
                e.stop = true;
                e.age = EFFECT_STOP;
            }
            _ => {}
        }
    }

    /// Plays effect `id` once at `at`, to its own end, where it was started. `namespace` and
    /// `serial` tell apart effects with the same id.
    pub fn one_shot(&mut self, namespace: usize, id: i32, serial: i32, at: Transform) {
        let key = (namespace, id, serial);
        self.request(id, key, at, true, false);
        if let Some(e) = self.effects.last_mut().filter(|e| e.key == key) {
            e.follow = false;
        }
    }

    /// Independent map namespace. Sleeping preserves the tree but submits no particles.
    pub(crate) fn map_effect(
        &mut self,
        id: i32,
        serial: i32,
        at: Transform,
        awake: bool,
        frozen: bool,
    ) {
        let key = (5, id, serial);
        if let Some(e) = self.effects.iter_mut().find(|e| e.key == key) {
            if frozen && awake && at.translation != e.at.translation {
                e.root = None;
                e.age = 0.;
            }
            e.at = at;
            e.asleep = !awake;
        } else if awake {
            if self.effects.len() >= EFFECT_CAP
                && let Some(i) = self.effects.iter().position(|e| e.key.0 == 5 && e.asleep)
            {
                self.effects.remove(i);
            }
            let before = self.effects.len();
            self.request(id, key, at, true, false);
            if self.effects.len() > before {
                let e = self.effects.last_mut().unwrap();
                e.persistent = true;
            }
        }
    }

    pub(crate) fn advance_map(&mut self, dt: f32) {
        self.advance_selected(dt, true);
    }

    /// Effects for the step's hits: sparks on a deflect or block, blood on a landed hit, at the
    /// contact point and facing the attacker. Returns the hits' floor stains for the caller.
    pub fn hits(&mut self, duel: &Duel, report: &DuelReport) -> Vec<StainRequest> {
        let _timer = crate::perf_log::SystemTimer::start("fxr hits");
        if !self.enabled || report.hits.is_empty() {
            return Vec::new();
        }
        self.ensure_tables(duel);
        let mut stains = Vec::new();
        let mut requests = Vec::new();
        if let Some(table) = &self.hit_fx {
            for (hit, result) in &report.hits {
                for id in table.stains(hit, result) {
                    stains.push((id, hit.position, hit.direction));
                }
                // Facing the attacker: sparks bounce back toward where the blow came from.
                let back =
                    Vec3::new(-hit.direction[0], 0.0, -hit.direction[1]).normalize_or(Vec3::Z);
                let body = if hit.defender == 0 {
                    &duel.player.body
                } else {
                    duel.population
                        .as_ref()
                        .and_then(|p| p.foes.get(hit.defender - 1))
                        .map_or(&duel.enemy.body, |f| &f.duel.enemy.body)
                };
                let at = Transform::from_translation(on_body(
                    Vec3::from_array(hit.position),
                    Vec3::from_array(body.position),
                ))
                .with_rotation(Quat::from_rotation_arc(Vec3::Z, back));
                for id in table.effects(hit, result) {
                    requests.push((id, at));
                }
            }
        }
        for (id, at) in requests {
            self.hit_serial += 1;
            let key = (3, id, self.hit_serial);
            self.request(id, key, at, true, false);
            if let Some(e) = self.effects.last_mut() {
                e.follow = false;
            }
        }
        stains
    }

    /// Called after each simulation step by the registration bridge.
    /// Positions use the simulation's posed dummy polys, including body fallback.
    pub fn observe(&mut self, duel: &Duel) {
        let _timer = crate::perf_log::SystemTimer::start("fxr observe");
        if !self.enabled {
            return;
        }
        self.ensure_tables(duel);
        let mut cues = Vec::new();
        let mut keys = Vec::new();
        self.steps += 1;
        if let Some(id) = self.preview {
            let key = (99, id, 0);
            keys.push(key);
            let position = (Vec3::from_array(duel.player.body.position)
                + Vec3::from_array(duel.enemy.body.position))
                * 0.5
                + Vec3::Y;
            if self.steps >= 60 {
                let start = (self.steps - 60).is_multiple_of(120);
                self.request(id, key, Transform::from_translation(position), start, true);
            }
            if self.steps.is_multiple_of(60) {
                info!(
                    "FXR preview step {}: particles={}, effects={}, spawned={}, rejected={}",
                    self.steps,
                    self.particles,
                    self.effects.len(),
                    self.spawned,
                    self.rejected
                );
            }
        }
        for (actor, character) in [&duel.player, &duel.enemy].into_iter().enumerate() {
            for cue in &character.tae_frame().effects {
                let e = &cue.effect;
                // Conditional and repeating variants need their own runtimes.
                if e.extra_condition != 0 || e.repeat {
                    continue;
                }
                let identity = (actor, e.id, e.dummy, cue.start.to_bits());
                let start = !self.last_cues.contains(&identity);
                cues.push(identity);
                let frame = i16::try_from(e.dummy)
                    .ok()
                    .and_then(|d| duel.dummy_frame(actor, d));
                if e.restrict_to_dummy && frame.is_none() {
                    continue;
                }
                let at = match frame {
                    Some((p, fwd, up)) if !e.ignore_dummy_angle => frame_transform(p, fwd, up),
                    Some((p, ..)) => Transform::from_translation(Vec3::from_array(p)),
                    None => Transform::from_translation(Vec3::from_array(character.body.position))
                        .with_rotation(Quat::from_rotation_y(character.body.yaw)),
                };
                let key = (actor, e.id, e.dummy);
                keys.push(key);
                if start || e.follow_dummy {
                    let stoppable = e.slot >= 0 || e.event_type == 118 || e.restrict_to_dummy;
                    self.request(e.id, key, at, start, stoppable);
                }
            }
        }
        self.last_cues = cues;
        for bullet in &duel.bullets.live {
            let id = self.bullet_sfx.get(&bullet.def.id).copied().unwrap_or(-1);
            let position = Vec3::from_array(bullet.position.to_array());
            let start = !self.bullets.contains_key(&bullet.serial);
            let key = (2, id, bullet.serial as i32);
            keys.push(key);
            let velocity = Vec3::from_array(bullet.velocity.to_array());
            // A bullet that is not moving (a bang, flames, smoke on the floor) throws its effect
            // upward; one in flight, along its flight.
            let facing = if velocity.length() > 0.1 {
                velocity.normalize()
            } else {
                Vec3::Y
            };
            self.request(
                id,
                key,
                Transform::from_translation(position)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, facing)),
                start,
                true,
            );
            self.bullets.insert(bullet.serial, id);
        }
        self.bullets
            .retain(|serial, _| duel.bullets.live.iter().any(|b| b.serial == *serial));
        for e in &mut self.effects {
            if e.stoppable && !keys.contains(&e.key) {
                e.stop = true;
            }
        }
        self.advance(1.0 / 60.0);
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        self.advance_selected(dt, false);
    }

    fn advance_selected(&mut self, dt: f32, map: bool) {
        let _timer = crate::perf_log::SystemTimer::start("fxr_step");
        let mut live = 0usize;
        let lib = &mut self.lib;
        for e in &mut self.effects {
            if (e.key.0 == 5) != map || e.asleep {
                continue;
            }
            if e.root.is_none() {
                let Some(def) = lib.compiled(e.id) else {
                    e.age = EFFECT_DROP + 1.0;
                    e.persistent = false;
                    continue;
                };
                let seed = (e.id as u32)
                    .wrapping_mul(97)
                    .wrapping_add(e.key.2 as u32)
                    .wrapping_add(self.steps as u32 * 31);
                e.root = make_node(lib, &def, seed, 0);
                if map
                    && dt == 0.
                    && let Some(root) = &mut e.root
                {
                    for _ in 0..60 {
                        step_node(root, lib, e.at, 1. / 30., false);
                    }
                    e.age = 2.;
                }
                if e.root.is_none() {
                    e.age = EFFECT_DROP + 1.0;
                    e.persistent = false;
                    continue;
                }
            }
            e.age += dt;
            let stop = e.stop || (e.age > EFFECT_STOP && !e.persistent) || live > PARTICLE_CAP;
            let Some(root) = e.root.as_mut() else {
                continue;
            };
            step_node(
                root,
                lib,
                Transform {
                    scale: Vec3::ONE,
                    ..e.at
                },
                dt,
                stop,
            );
            grow_trails(root);
            live += count_particles(root);
            if root.finished {
                e.age = EFFECT_DROP + 1.0;
                e.persistent = false;
            }
        }
        self.effects
            .retain(|e| e.persistent || e.age <= EFFECT_DROP);
        self.particles = self
            .effects
            .iter()
            .filter(|e| !e.asleep)
            .filter_map(|e| e.root.as_ref())
            .map(count_particles)
            .sum();
    }
}

/// A contact point held on the defender's body: the sim reports where the two hit shapes are
/// closest, which can be a metre off the body or above the head, so it is kept within the body's
/// height (0.5 to 1.5 m above the feet) and 0.6 m of its axis.
fn on_body(contact: Vec3, feet: Vec3) -> Vec3 {
    let height = (contact.y - feet.y).clamp(0.5, 1.5);
    let mut flat = Vec2::new(contact.x - feet.x, contact.z - feet.z);
    if flat.length() > 0.6 {
        flat = flat.normalize() * 0.6;
    }
    Vec3::new(feet.x + flat.x, feet.y + height, feet.z + flat.y)
}

fn count_particles(n: &NodeInst) -> usize {
    n.particles.len() + n.children.iter().map(count_particles).sum::<usize>()
}

/// A dummy's world frame as a transform: forward is +Z, up +Y; X completes a right-handed basis
/// (the dummy axes arrive mirrored, which `game_vec` undoes in the effect's own offsets).
pub fn frame_transform(position: [f32; 3], forward: [f32; 3], up: [f32; 3]) -> Transform {
    let (z, y) = (Vec3::from_array(forward), Vec3::from_array(up));
    let x = y.cross(z).normalize_or(Vec3::X);
    let y = z.cross(x).normalize_or(Vec3::Y);
    Transform::from_translation(Vec3::from_array(position))
        .with_rotation(Quat::from_mat3(&Mat3::from_cols(x, y, z)))
}

// ---------------------------------------------------------------- drawing

#[derive(Default)]
struct Buf {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    /// x: alpha fade threshold, y: hard alpha threshold (both 0..1), z: the soft distance
    /// (0 = hard edge).
    tan: Vec<[f32; 4]>,
}

impl Buf {
    fn quad(&mut self, c: [Vec3; 4], uv: [[f32; 2]; 4], col: [f32; 4], tan: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(c[i].to_array());
            self.uv.push(uv[i]);
            self.col.push(col);
            self.tan.push(tan);
        }
    }
}

/// MultiTextureBillboardEx quads, one mesh per `MultiKey`.
#[derive(Default)]
struct MultiBuf {
    pos: Vec<[f32; 3]>,
    uv1: Vec<[f32; 2]>,
    uv2: Vec<[f32; 2]>,
    uv3n: Vec<[f32; 4]>,
    col: Vec<[f32; 4]>,
    c1: Vec<[f32; 4]>,
    c2: Vec<[f32; 4]>,
    c3: Vec<[f32; 4]>,
    soft: Vec<f32>,
}

struct MultiQuad {
    corners: [Vec3; 4],
    uv1: [[f32; 2]; 4],
    uv1_next: [[f32; 2]; 4],
    uv2: [[f32; 2]; 4],
    uv3: [[f32; 2]; 4],
    col: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
    soft: f32,
}

impl MultiBuf {
    fn quad(&mut self, q: &MultiQuad) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(q.corners[i].to_array());
            self.uv1.push(q.uv1[i]);
            self.uv2.push(q.uv2[i]);
            self.uv3n
                .push([q.uv3[i][0], q.uv3[i][1], q.uv1_next[i][0], q.uv1_next[i][1]]);
            self.col.push(q.col);
            self.c1.push(q.c1);
            self.c2.push(q.c2);
            self.c3.push(q.c3);
            self.soft.push(q.soft);
        }
    }
}

/// (layer1, layer2, layer3 textures, TexBlendType | TexBlendType2 << 4 | TexBlendType3 << 8 |
/// ColorBlendType << 12 | premultiplyAlpha << 16, blend mode).
type MultiKey = (i64, i64, i64, i64, i64);

/// Blend modes 0, 4 and 7 add; the rest blend.
fn additive(blend: i64) -> bool {
    matches!(blend, 0 | 4 | 7)
}

/// Soft particles: pull the quad toward the camera by the particle's half size so it does not
/// cut a hard line through the floor, and fade it where it meets the scene (the shaders,
/// `fxr.wesl` / `fxr_multi.wesl`). Where the appearance's `depthBlend` is set: (scale, offset) are
/// `unkDepthBlend1/2`. Returns the corners and the soft distance (0 = hard).
fn soften(a: &Action, size: f32, cam: &GlobalTransform, c: [Vec3; 4]) -> ([Vec3; 4], f32) {
    if a[Field::DepthBlend].as_bool() != Some(true) {
        return (c, 0.0);
    }
    const NEAR: f32 = 0.1;
    let scale = a
        .get(Field::UnkDepthBlend1)
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    let off = a
        .get(Field::UnkDepthBlend2)
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let h = scale * size * 0.5;
    let (eye, fwd) = (cam.translation(), cam.forward().as_vec3());
    let pull = |p: Vec3| {
        let w = (p - eye).dot(fwd);
        if w <= 1e-3 {
            return p;
        }
        let mut nw = w - (h + off);
        if nw < NEAR + 1.0 {
            nw += (NEAR + 1.0 - nw).min(1.0) * (w - nw);
        }
        eye + (p - eye) * (nw / w)
    };
    (c.map(pull), 2.0 * off + h)
}

fn layer_uv(a: &Action, fields: [Field; 6], age: f32, rnd: &mut Rnd) -> [[f32; 2]; 4] {
    let [scale_u, scale_v, offset_u, speed_u, offset_v, speed_v] = fields;
    let su = scalar(a, scale_u, age, rnd, 1.0);
    let sv = scalar(a, scale_v, age, rnd, 1.0);
    let ou = scalar(a, offset_u, 0.0, rnd, 0.0) + scalar(a, speed_u, age, rnd, 0.0) * age;
    let ov = scalar(a, offset_v, 0.0, rnd, 0.0) + scalar(a, speed_v, age, rnd, 0.0) * age;
    QUAD_UV.map(|[u, v]: [f32; 2]| [u * su + ou, v * sv + ov])
}

const QUAD_UV: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

fn layer_uv_distort(a: &Action, age: f32, rnd: &mut Rnd) -> [[f32; 2]; 4] {
    let su = scalar(a, Field::NormalMapScaleU, 0., rnd, 1.);
    let sv = scalar(a, Field::NormalMapScaleV, 0., rnd, 1.);
    let ou = scalar(a, Field::NormalMapOffsetU, 0., rnd, 0.)
        + scalar(a, Field::NormalMapSpeedU, age, rnd, 0.) * age;
    let ov = scalar(a, Field::NormalMapOffsetV, 0., rnd, 0.)
        + scalar(a, Field::NormalMapSpeedV, age, rnd, 0.) * age;
    QUAD_UV.map(|[u, v]| [u * su + ou, v * sv + ov])
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct DistortMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
}
impl Material for DistortMaterial {
    fn vertex_shader() -> ShaderRef {
        shader("fxr_distort.wesl")
    }
    fn fragment_shader() -> ShaderRef {
        shader("fxr_distort.wesl")
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn reads_view_transmission_texture(&self) -> bool {
        true
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

struct Frame<'a> {
    cam: &'a GlobalTransform,
    bufs: &'a mut HashMap<(i64, bool), Buf>,
    multi: &'a mut HashMap<MultiKey, MultiBuf>,
    lights: &'a mut Vec<FxLight>,
    distort: &'a mut HashMap<i64, Buf>,
    trail: &'a mut Vec<(Vec3, Vec3, f32)>,
}

/// One FXR 609 point light of this frame.
#[derive(Clone, Copy, Debug)]
struct FxLight {
    position: Vec3,
    /// Squared diffuse colour.
    color: Color,
    /// Diffuse multiplier times the colour alpha and the view fade, in native units.
    intensity: f32,
    /// `radius`: the light ends here (`saturate(1 - d^2 / R^2)`).
    range: f32,
    /// `unk_ds3_p2_0`: the distance in the native `1 / (1 + 0.7 (d / falloff)^2)` term.
    falloff: f32,
}

impl FxLight {
    /// The Bevy light. `PointLight::radius` carries the falloff distance to the native map
    /// shader (`native_clustered_light` in `map_material.wesl`); FXR lights cast no shadows,
    /// so Bevy reads it only for its own specular sphere, which that shader subtracts again.
    fn point_light(&self) -> PointLight {
        PointLight {
            color: self.color,
            intensity: self.intensity * crate::lighting::LOCAL_LUMENS_PER_UNIT,
            range: self.range,
            radius: self.falloff,
            shadow_maps_enabled: false,
            ..default()
        }
    }
}

#[derive(Default)]
struct FrameBuffers {
    bufs: HashMap<(i64, bool), Buf>,
    multi: HashMap<MultiKey, MultiBuf>,
    lights: Vec<FxLight>,
    distort: HashMap<i64, Buf>,
    trail: Vec<(Vec3, Vec3, f32)>,
}

fn collect(n: &NodeInst, f: &mut Frame) {
    if let Some(c) = &n.cfg {
        collect_node(n, c, f);
    }
    for ch in &n.children {
        collect(ch, f);
    }
}

fn collect_node(n: &NodeInst, c: &Config, f: &mut Frame) {
    let a = &c.appearance;
    let at = a[Field::Type].as_i64().unwrap_or(0);
    if !matches!(at, 600 | 602 | 603 | 604 | 606 | 607 | 609 | 10000 | 10012) {
        return;
    }
    let active_t = (n.time - n.delay).max(0.0);
    let blend = int(a, Field::BlendMode, 2);
    let tex = match at {
        604 => int(a, Field::Layer1, 1),
        _ => int(a, Field::Texture, 1),
    };
    let cam = f.cam;
    let (cam_r, cam_u, cam_f) = (
        cam.right().as_vec3(),
        cam.up().as_vec3(),
        cam.forward().as_vec3(),
    );
    let env = crate::lighting::ENV_PER_UNIT;
    if at == 10000 {
        for p in &n.particles {
            let mut rnd = Rnd { seed: p.seed, k: 0 };
            let fade = view_fade(a, p.pos.distance(cam.translation()));
            let size = scalar(a, Field::ParticleSizeX, p.age, &mut rnd, 0.025)
                + scalar(a, Field::ParticleSizeXMin, 0., &mut rnd, 0.)
                + rnd.next()
                    * (scalar(a, Field::ParticleSizeXMax, 0., &mut rnd, 0.)
                        - scalar(a, Field::ParticleSizeXMin, 0., &mut rnd, 0.));
            let size = size * scalar(a, Field::ParticleSizeMultiplier, p.age, &mut rnd, 1.);
            let mut col = prop(a.get(Field::Color), p.age, &mut rnd, [1.; 4]);
            let rgb = scalar(a, Field::RgbMultiplier, p.age, &mut rnd, 1.);
            for v in &mut col[..3] {
                *v *= env * rgb;
            }
            col[3] *= fade * scalar(a, Field::AlphaMultiplier, p.age, &mut rnd, 1.);
            let (r, u) = (cam_r * size * 0.5, cam_u * size * 0.5);
            f.bufs.entry((tex, additive(blend))).or_default().quad(
                [p.pos - r - u, p.pos + r - u, p.pos + r + u, p.pos - r + u],
                QUAD_UV,
                col,
                [0., 0., size, 0.],
            );
        }
        return;
    }
    for p in &n.particles {
        let mut rnd = Rnd {
            seed: p.seed ^ 0x9999,
            k: 0,
        };
        let world = if p.attached {
            n.world.transform_point(p.pos)
        } else {
            p.pos
        };
        let fade = view_fade(a, world.distance(cam.translation()));
        if fade <= 0.0 {
            continue;
        }
        let mut col = particle_color(a, c, p, active_t, &mut rnd);
        col[3] *= fade;
        for v in &mut col[..3] {
            *v *= env;
        }
        let pmod = &c.particle_modifier;
        let sx = scalar(pmod, Field::ScaleX, p.emit_t, &mut rnd, 1.0);
        let sy = if pmod[Field::UniformScale].as_bool() == Some(true) {
            sx
        } else {
            scalar(pmod, Field::ScaleY, p.emit_t, &mut rnd, 1.0)
        };
        match at {
            607 => {
                let screen = prop(a.get(Field::Color), p.age, &mut rnd, [1.; 4]);
                let modifier = prop(
                    c.particle_modifier.get(Field::Color),
                    active_t,
                    &mut rnd,
                    [1.; 4],
                );
                col[3] = (screen[3]
                    * modifier[3]
                    * scalar(a, Field::AlphaMultiplier, active_t, &mut rnd, 1.)
                    * fade)
                    .clamp(0., 1.);
                let texture = int(a, Field::NormalMap, 0);
                if texture <= 0 {
                    continue;
                }
                let w = scalar(a, Field::SizeX, p.age, &mut rnd, 1.) * sx;
                let h = scalar(a, Field::SizeY, p.age, &mut rnd, 1.) * sy;
                let (right, up) = match int(a, Field::Orientation, 1) {
                    0 | 3 => (Vec3::X, Vec3::Y),
                    2 => (n.world.rotation * Vec3::X, n.world.rotation * Vec3::Y),
                    _ => (cam_r, cam_u),
                };
                let (r, u) = (right * w * 0.5, up * h * 0.5);
                let uv = layer_uv_distort(a, p.age, &mut rnd);
                let intensity = scalar(a, Field::Intensity, p.age, &mut rnd, 0.);
                let radius = scalar(a, Field::Radius, p.age, &mut rnd, 1.).max(0.001);
                let buf = f.distort.entry(texture).or_default();
                let corners = [world - r - u, world + r - u, world + r + u, world - r + u];
                for i in [0, 1, 2, 0, 2, 3] {
                    buf.pos.push(corners[i].to_array());
                    buf.uv.push(uv[i]);
                    buf.col.push([1., 1., 1., col[3]]);
                    buf.tan
                        .push([intensity, 1. / radius, QUAD_UV[i][0], QUAD_UV[i][1]]);
                }
            }
            609 => {
                let limit = scalar(a, Field::MaxViewDistance, active_t, &mut rnd, -1.);
                if limit > 0. && world.distance_squared(cam.translation()) > limit * limit {
                    continue;
                }
                let color = prop(a.get(Field::DiffuseColor), p.age, &mut rnd, [1.; 4]);
                let intensity =
                    scalar(a, Field::DiffuseMultiplier, active_t, &mut rnd, 1.) * color[3].max(0.);
                let radius = scalar(a, Field::Radius, active_t, &mut rnd, 0.);
                // The light object's falloff distance (object +0x90, 1.0 by default in
                // 20d5f0); the builders store 1 / it as the diffuse w the shader's
                // `1 / (1 + 0.7 (d / falloff)^2)` term uses (#141).
                let falloff = scalar(a, Field::UnkDs3P2_0, active_t, &mut rnd, 1.);
                if radius > 0. && intensity > 0. && falloff > 0. {
                    f.lights.push(FxLight {
                        position: world,
                        // 3892e0 (reached from 210e00 with the 609 property block) squares the
                        // diffuse RGB before the colour alpha and multiplier scale it.
                        color: Color::linear_rgb(
                            color[0] * color[0],
                            color[1] * color[1],
                            color[2] * color[2],
                        ),
                        intensity: intensity * fade,
                        range: radius,
                        falloff,
                    });
                }
            }
            600 | 603 | 604 => {
                let w = if at == 600 {
                    scalar(a, Field::Size, p.age, &mut rnd, 1.0) * sx
                } else {
                    scalar(a, Field::Width, p.age, &mut rnd, 1.0) * sx
                };
                let h = if at == 600 || a[Field::UniformScale].as_bool() == Some(true) {
                    w
                } else {
                    scalar(a, Field::Height, p.age, &mut rnd, 1.0) * sy
                };
                let rz = (scalar(a, Field::RotationZ, 0.0, &mut rnd, 0.0)
                    + scalar(a, Field::AngularSpeedZ, p.age, &mut rnd, 0.0)
                        * scalar(a, Field::AngularSpeedMultiplierZ, p.age, &mut rnd, 1.0)
                        * p.age)
                    .to_radians();
                // OrientationMode: 1 / 6 camera, 7 / 4 yaw toward the camera, 2 the node's
                // frame, 0 / 3 global.
                let (right, up) = match int(a, Field::Orientation, 1) {
                    0 | 3 => (Vec3::X, Vec3::Y),
                    2 => (n.world.rotation * Vec3::X, n.world.rotation * Vec3::Y),
                    o @ (4 | 7) => {
                        let axis = if o == 7 {
                            n.world.rotation * Vec3::Y
                        } else {
                            Vec3::Y
                        };
                        let to_cam = (cam.translation() - world)
                            .reject_from(axis)
                            .normalize_or(cam_f);
                        (axis.cross(to_cam).normalize_or(cam_r), axis)
                    }
                    _ => (cam_r, cam_u),
                };
                let (cr, sr) = (rz.cos(), rz.sin());
                let (r2, u2) = (right * cr + up * sr, up * cr - right * sr);
                let (hw, hh) = (r2 * w * 0.5, u2 * h * 0.5);
                let (corners, soft) = soften(
                    a,
                    w.abs().max(h.abs()),
                    cam,
                    [
                        world - hw - hh,
                        world + hw - hh,
                        world + hw + hh,
                        world - hw + hh,
                    ],
                );
                let l2 = int(a, Field::Layer2, 0);
                if at == 604 && l2 > 0 {
                    // The layers (fxr_multi.wesl); unk_ds3_f2_10..13 are the shader's
                    // TexBlendType / 2 / 3 / ColorBlendType (FwLo's reading).
                    let l3 = int(a, Field::Layer3, 0).max(0);
                    let m = |k: Field| int(a, k, 0) & 0xf;
                    let modes = m(Field::UnkDs3F2_10)
                        | m(Field::UnkDs3F2_11) << 4
                        | m(Field::UnkDs3F2_12) << 8
                        | m(Field::UnkDs3F2_13) << 12
                        | ((a[Field::PremultiplyAlpha].as_bool() == Some(true)) as i64) << 16;
                    let scale1 = [
                        scalar(a, Field::Layer1ScaleU, p.age, &mut rnd, 1.0),
                        scalar(a, Field::Layer1ScaleV, p.age, &mut rnd, 1.0),
                    ];
                    let (f_uv, f_next, f_t) = frame_uvs(a, p.age, &mut rnd, scale1);
                    let o1 = [
                        scalar(a, Field::Layer1OffsetU, 0.0, &mut rnd, 0.0)
                            + scalar(a, Field::Layer1SpeedU, p.age, &mut rnd, 0.0) * p.age,
                        scalar(a, Field::Layer1OffsetV, 0.0, &mut rnd, 0.0)
                            + scalar(a, Field::Layer1SpeedV, p.age, &mut rnd, 0.0) * p.age,
                    ];
                    let shift = |uv: [[f32; 2]; 4]| uv.map(|[u, v]| [u + o1[0], v + o1[1]]);
                    let lc = |k: Field, rnd: &mut Rnd| {
                        prop(a.get(k), p.age, rnd, [1.0; 4]).map(|v| v.clamp(0.0, 1.0))
                    };
                    let c1 = lc(Field::Layer1Color, &mut rnd);
                    let interp = a[Field::InterpolateFrames].as_bool() != Some(false);
                    let q = MultiQuad {
                        corners,
                        uv1: shift(f_uv),
                        uv1_next: shift(f_next),
                        uv2: layer_uv(
                            a,
                            [
                                Field::Layer2ScaleU,
                                Field::Layer2ScaleV,
                                Field::Layer2OffsetU,
                                Field::Layer2SpeedU,
                                Field::Layer2OffsetV,
                                Field::Layer2SpeedV,
                            ],
                            p.age,
                            &mut rnd,
                        ),
                        uv3: layer_uv(
                            a,
                            [
                                Field::Layer3ScaleU,
                                Field::Layer3ScaleV,
                                Field::Layer3OffsetU,
                                Field::Layer3SpeedU,
                                Field::Layer3OffsetV,
                                Field::Layer3SpeedV,
                            ],
                            p.age,
                            &mut rnd,
                        ),
                        col,
                        c1: [c1[0], c1[1], c1[2], if interp { f_t } else { 0.0 }],
                        c2: lc(Field::Layer2Color, &mut rnd),
                        c3: lc(Field::Layer3Color, &mut rnd),
                        soft,
                    };
                    f.multi
                        .entry((tex, l2, l3, modes, blend))
                        .or_default()
                        .quad(&q);
                    continue;
                }
                let (uv, _, _) = frame_uvs(a, p.age, &mut rnd, [1.0, 1.0]);
                let tan = [
                    scalar(a, Field::AlphaFadeThreshold, p.age, &mut rnd, 0.0) / 255.0,
                    scalar(a, Field::AlphaThreshold, active_t, &mut rnd, 0.0) / 255.0,
                    soft,
                    // g_ps_bPreAlphaMult: the texture colour is multiplied by its alpha before
                    // the shader squares it.
                    (a[Field::PremultiplyAlpha].as_bool() == Some(true)) as u8 as f32,
                ];
                f.bufs
                    .entry((tex, additive(blend)))
                    .or_default()
                    .quad(corners, uv, col, tan);
            }
            602 => {
                let wl = scalar(a, Field::Width, p.emit_t, &mut rnd, 1.0)
                    * scalar(a, Field::WidthMultiplier, p.age, &mut rnd, 1.0)
                    * sx;
                let len = scalar(a, Field::Length, p.emit_t, &mut rnd, 1.0)
                    * scalar(a, Field::LengthMultiplier, p.age, &mut rnd, 1.0)
                    * sy;
                let d = if p.attached {
                    n.world.rotation * p.dir
                } else {
                    p.dir
                };
                let side = d.cross(cam_f).normalize_or(cam_r) * wl * 0.5;
                let tail = world - d * len;
                f.bufs.entry((-1, additive(blend))).or_default().quad(
                    [tail - side, world - side, world + side, tail + side],
                    QUAD_UV,
                    col,
                    [0.0; 4],
                );
            }
            606 | 10012 => {
                let w = (scalar(a, Field::Width, p.age, &mut rnd, 1.0)
                    * scalar(a, Field::WidthMultiplier, p.emit_t, &mut rnd, 1.0))
                .abs();
                let cross = int(a, Field::Orientation, 0) == 4;
                // A trail with no colour (a distortion-only layer) draws nothing here.
                if p.trail.len() < 2 || a[Field::RgbMultiplier].as_f64() == Some(0.0) {
                    continue;
                }
                subdivide(
                    &p.trail,
                    int(a, Field::SegmentSubdivision, 0).clamp(0, 16) as usize,
                    f.trail,
                );
                let sf = scalar(a, Field::StartFadeEndpoint, p.age, &mut rnd, 0.0).max(0.0) / 100.0;
                let ef = scalar(a, Field::EndFadeEndpoint, p.age, &mut rnd, 0.0).max(0.0) / 100.0;
                let fade_at = |u: f32| -> f32 {
                    let mut k = 1.0;
                    if sf > 0.0 {
                        k *= (u / sf).min(1.0);
                    }
                    if ef > 0.0 {
                        k *= ((1.0 - u) / ef).min(1.0);
                    }
                    k
                };
                let buf = f.bufs.entry((tex, additive(blend))).or_default();
                for w2 in f.trail.windows(2) {
                    let ((a0, x0, u0), (a1, x1, u1)) = (w2[0], w2[1]);
                    let d = (a1 - a0).normalize_or(cam_r);
                    let travel = d.cross(cam_f).normalize_or(cam_u);
                    let s0 = if x0 == Vec3::ZERO { travel } else { x0 } * w * 0.5;
                    let s1 = if x1 == Vec3::ZERO { travel } else { x1 } * w * 0.5;
                    let k = fade_at(u0).min(fade_at(u1));
                    let c0 = [col[0], col[1], col[2], col[3] * k];
                    let quv = [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]];
                    buf.quad([a0 - s0, a1 - s1, a1 + s1, a0 + s0], quv, c0, [0.0; 4]);
                    if cross {
                        let cx = Vec3::X * w * 0.5;
                        buf.quad([a0 - cx, a1 - cx, a1 + cx, a0 + cx], quv, c0, [0.0; 4]);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The sprite batches' material: unlit texture x vertex colour with a soft depth fade
/// (`fxr.wesl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct FxrMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
    /// The scene depth ([`DepthCopy`]): where a soft particle meets the scene.
    #[texture(2, sample_type = "depth", multisampled = true)]
    depth: Handle<Image>,
    additive: bool,
}

impl Material for FxrMaterial {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.additive {
            AlphaMode::Add
        } else {
            AlphaMode::Blend
        }
    }
    fn vertex_shader() -> ShaderRef {
        shader("fxr.wesl")
    }
    fn fragment_shader() -> ShaderRef {
        shader("fxr.wesl")
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

const ATTR_UV3N: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new(
    "FxMultiUv3Next",
    0x5ec1_0604,
    bevy::render::render_resource::VertexFormat::Float32x4,
);
const ATTR_C1: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new(
    "FxMultiColor1",
    0x5ec1_0641,
    bevy::render::render_resource::VertexFormat::Float32x4,
);
const ATTR_C2: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new(
    "FxMultiColor2",
    0x5ec1_0642,
    bevy::render::render_resource::VertexFormat::Float32x4,
);
const ATTR_C3: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new(
    "FxMultiColor3",
    0x5ec1_0643,
    bevy::render::render_resource::VertexFormat::Float32x4,
);
const ATTR_SOFT: bevy::mesh::MeshVertexAttribute = bevy::mesh::MeshVertexAttribute::new(
    "FxSoft",
    0x5ec1_0650,
    bevy::render::render_resource::VertexFormat::Float32,
);

/// MultiTextureBillboardEx (604): three texture layers blended as the game's
/// GXFfxtessellateBlendMultiTexture shader does (`fxr_multi.wesl`, FwLo's port). Output is
/// premultiplied, so one alpha mode serves normal and additive blends.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct MultiMaterial {
    #[texture(0)]
    #[sampler(1)]
    t0: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    t1: Handle<Image>,
    #[texture(4)]
    #[sampler(5)]
    t2: Handle<Image>,
    #[uniform(6)]
    modes: UVec4,
    #[uniform(7)]
    flags: UVec4,
    #[texture(8, sample_type = "depth", multisampled = true)]
    depth: Handle<Image>,
}

impl Material for MultiMaterial {
    fn vertex_shader() -> ShaderRef {
        shader("fxr_multi.wesl")
    }
    fn fragment_shader() -> ShaderRef {
        shader("fxr_multi.wesl")
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(2),
            ATTR_UV3N.at_shader_location(3),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(4),
            ATTR_C1.at_shader_location(5),
            ATTR_C2.at_shader_location(6),
            ATTR_C3.at_shader_location(7),
            ATTR_SOFT.at_shader_location(8),
        ])?];
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- scene depth

/// The camera's depth buffer copied after the opaque pass: soft particles read it. Not Bevy's
/// DepthPrepass, which draws the whole map twice. Never written (a size or sample-count mismatch)
/// it stays 0, which reads as infinitely far, and particles keep hard edges.
#[derive(Resource, Clone, Default, bevy::render::extract_resource::ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct DepthCopy {
    image: Handle<Image>,
}

fn depth_image(size: UVec2) -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
    let mut img = Image::new_uninit(
        Extent3d {
            width: size.x.max(1),
            height: size.y.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Depth32Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.sample_count = 4;
    img.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    img
}

fn setup_depth(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(DepthCopy {
        image: images.add(depth_image(UVec2::ONE)),
    });
}

/// Keeps the copy at the camera's size and lets the camera's depth be copied from.
fn soft_depth(
    copy: Res<DepthCopy>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: ResMut<Assets<FxrMaterial>>,
    mut multi: ResMut<Assets<MultiMaterial>>,
    mut cams: Query<(&Camera, &mut Camera3d)>,
) {
    if std::env::var_os("SEKIRO_NO_SOFT").is_some() {
        return;
    }
    for (cam, mut c3) in &mut cams {
        let usage = bevy::render::render_resource::TextureUsages::from(c3.depth_texture_usages);
        if !usage.contains(bevy::render::render_resource::TextureUsages::COPY_SRC) {
            c3.depth_texture_usages =
                (usage | bevy::render::render_resource::TextureUsages::COPY_SRC).into();
        }
        let Some(size) = cam.physical_target_size() else {
            continue;
        };
        let Some(img) = images.get(&copy.image) else {
            continue;
        };
        if img.size() == size {
            continue;
        }
        let _ = images.insert(&copy.image, depth_image(size));
        // Touch the materials so their bind groups pick up the new texture.
        for _ in sprites.iter_mut() {}
        for _ in multi.iter_mut() {}
    }
}

/// Render world: the camera's depth (opaque pass done) into [`DepthCopy`], when size and sample
/// count match.
fn copy_depth(
    view: bevy::render::renderer::ViewQuery<&bevy::render::view::ViewDepthStencilTexture>,
    copy: Option<Res<DepthCopy>>,
    images: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
    mut ctx: bevy::render::renderer::RenderContext,
) {
    let depth = view.into_inner();
    if std::env::var_os("SEKIRO_FX_DEBUG_NOCOPY").is_some() {
        return;
    }
    let Some(copy) = copy else { return };
    let Some(dst) = images.get(&copy.image) else {
        return;
    };
    let (s, d) = (depth.texture(), &dst.texture);
    if s.size() != d.size()
        || s.sample_count() != d.sample_count()
        || s.format() != d.format()
        || !s
            .usage()
            .contains(bevy::render::render_resource::TextureUsages::COPY_SRC)
    {
        return;
    }
    ctx.command_encoder()
        .copy_texture_to_texture(s.as_image_copy(), d.as_image_copy(), s.size());
}

// ---------------------------------------------------------------- render system

fn load_texture(lib: &FxrLib, id: i64, srgb: bool) -> Option<Image> {
    let path = lib.texture_path(id)?;
    let data = std::fs::read(path).ok()?;
    let tpf = sekiro_formats::tpf::parse(&data).ok()?;
    let first = tpf.first()?;
    first.header().ok()?;
    Image::from_buffer(
        first.dds,
        bevy::image::ImageType::Extension("dds"),
        bevy::image::CompressedImageFormats::all(),
        srgb,
        bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
            address_mode_u: bevy::image::ImageAddressMode::Repeat,
            address_mode_v: bevy::image::ImageAddressMode::Repeat,
            ..bevy::image::ImageSamplerDescriptor::linear()
        }),
        RenderAssetUsages::default(),
    )
    .ok()
}

fn texture_handle(
    player: &mut FxrPlayer,
    images: &mut Assets<Image>,
    id: i64,
    white: &Handle<Image>,
) -> Handle<Image> {
    if id <= 0 {
        return white.clone();
    }
    if let Some(h) = player.textures.get(&id) {
        return h.clone();
    }
    // The effect shaders sample the DDS as plain UNORM (BC1 and BC7 UNORM, never *_SRGB) and
    // square the texture colour themselves, so decoding it as sRGB first applied the gamma twice
    // and starved the soft body of every flame sprite, leaving only the over-bright core.
    let handle = match load_texture(&player.lib, id, false) {
        Some(image) => images.add(image),
        None => {
            warn!("FXR texture {id} is not installed or cannot be decoded");
            white.clone()
        }
    };
    player.textures.insert(id, handle.clone());
    handle
}

/// A degenerate triangle far below, so a batch mesh is never empty (zero-length buffers are not
/// supported by the mesh allocator).
const PAD: [f32; 3] = [0.0, -50.0, 0.0];

trait BatchBuffer {
    fn clear(&mut self);
    fn is_empty(&self) -> bool;
    fn matches(&self, mesh: &Mesh) -> bool;
    fn write_mesh(&self, mesh: &mut Mesh);
    fn update_mesh(&self, meshes: &mut Assets<Mesh>, handle: &Handle<Mesh>) {
        if self.is_empty() || meshes.get(handle).is_none_or(|mesh| self.matches(mesh)) {
            return;
        }
        self.write_mesh(&mut meshes.get_mut(handle).unwrap());
    }
}

// Keep the padding triangle and attribute layout identical across reused buffers.
macro_rules! batch_buffer {
    ($buffer:ty, $normals:expr, $( $field:ident: $variant:ident, $attribute:expr, $pad:expr );+ $(;)?) => {
        impl BatchBuffer for $buffer {
            fn clear(&mut self) {
                $(self.$field.clear();)+
            }

            fn is_empty(&self) -> bool {
                self.pos.is_empty()
            }

            fn matches(&self, mesh: &Mesh) -> bool {
                $(matches!(mesh.attribute($attribute),
                    Some(VertexAttributeValues::$variant(values))
                        if values.get(3..) == Some(self.$field.as_slice())) )&&+
            }

            fn write_mesh(&self, mesh: &mut Mesh) {
                $(
                    if let Some(VertexAttributeValues::$variant(values)) = mesh.attribute_mut($attribute) {
                        values.truncate(3);
                        values.extend_from_slice(&self.$field);
                    } else {
                        let mut values = Vec::with_capacity(self.$field.len() + 3);
                        values.extend_from_slice(&[$pad; 3]);
                        values.extend_from_slice(&self.$field);
                        mesh.insert_attribute($attribute, values);
                    }
                )+
                let n = self.pos.len() + 3;
                if $normals {
                    if let Some(VertexAttributeValues::Float32x3(values)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
                        values.resize(n, [0.0, 0.0, 1.0]);
                    } else {
                        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; n]);
                    }
                }
                if let Some(Indices::U32(indices)) = mesh.indices_mut() {
                    indices.truncate(n);
                    indices.extend(indices.len() as u32..n as u32);
                } else {
                    mesh.insert_indices(Indices::U32((0..n as u32).collect()));
                }
            }
        }
    };
}

batch_buffer!(Buf, true,
    pos: Float32x3, Mesh::ATTRIBUTE_POSITION, PAD;
    uv: Float32x2, Mesh::ATTRIBUTE_UV_0, [0.0; 2];
    col: Float32x4, Mesh::ATTRIBUTE_COLOR, [0.0; 4];
    tan: Float32x4, Mesh::ATTRIBUTE_TANGENT, [0.0; 4];
);
batch_buffer!(MultiBuf, false,
    pos: Float32x3, Mesh::ATTRIBUTE_POSITION, PAD;
    uv1: Float32x2, Mesh::ATTRIBUTE_UV_0, [0.0; 2];
    uv2: Float32x2, Mesh::ATTRIBUTE_UV_1, [0.0; 2];
    uv3n: Float32x4, ATTR_UV3N, [0.0; 4];
    col: Float32x4, Mesh::ATTRIBUTE_COLOR, [0.0; 4];
    c1: Float32x4, ATTR_C1, [0.0; 4];
    c2: Float32x4, ATTR_C2, [0.0; 4];
    c3: Float32x4, ATTR_C3, [0.0; 4];
    soft: Float32, ATTR_SOFT, 0.0;
);

struct Batch {
    mesh: Handle<Mesh>,
    entity: Entity,
    visible: bool,
    cpu: Option<Mesh>,
}

impl Batch {
    fn update(
        &mut self,
        buffer: &impl BatchBuffer,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        stream: Option<&mut ResMut<stream::VertexStreams>>,
    ) {
        let visible = !buffer.is_empty();
        if self.visible != visible {
            commands.entity(self.entity).insert(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            self.visible = visible;
        }
        if let Some(stream) = stream {
            if !visible {
                return;
            }
            let first = self.cpu.is_none();
            let cpu = self.cpu.get_or_insert_with(|| {
                Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                )
            });
            if buffer.matches(cpu) {
                return;
            }
            buffer.write_mesh(cpu);
            if stream
                .bypass_change_detection()
                .publish(self.mesh.id(), cpu)
            {
                stream.set_changed();
            }
            if first {
                // Bevy keeps this small mesh for its layout/material/phase bookkeeping.
                // Dynamic data is written to a reusable GPU buffer instead of re-preparing it.
                let mut proxy = cpu.clone();
                for (_, values) in proxy.attributes_mut() {
                    match values {
                        VertexAttributeValues::Float32(v) => v.truncate(3),
                        VertexAttributeValues::Float32x2(v) => v.truncate(3),
                        VertexAttributeValues::Float32x3(v) => v.truncate(3),
                        VertexAttributeValues::Float32x4(v) => v.truncate(3),
                        _ => unreachable!("FXR float attributes"),
                    }
                }
                proxy.remove_indices();
                meshes
                    .insert(self.mesh.id(), proxy)
                    .expect("existing FXR mesh");
            }
        } else {
            buffer.update_mesh(meshes, &self.mesh);
        }
    }
}

fn log_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SEKIRO_FXR_LOG").is_some())
}

fn new_batch(commands: &mut Commands, meshes: &mut Assets<Mesh>) -> Batch {
    let mesh = meshes.add(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    ));
    Batch {
        entity: mesh_entity(commands, mesh.clone()),
        mesh,
        visible: false,
        cpu: None,
    }
}

fn mesh_entity(commands: &mut Commands, mesh: Handle<Mesh>) -> Entity {
    commands
        .spawn((
            Mesh3d(mesh),
            Visibility::Hidden,
            Transform::IDENTITY,
            bevy::camera::visibility::NoFrustumCulling,
            bevy::light::NotShadowCaster,
            bevy::light::NotShadowReceiver,
            // Batches appear whenever an effect first uses a texture; loading never
            // waits for them.
            crate::map::ready::RuntimeMesh,
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
fn render(
    mut commands: Commands,
    mut player: ResMut<FxrPlayer>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sprite_materials: ResMut<Assets<FxrMaterial>>,
    mut multi_materials: ResMut<Assets<MultiMaterial>>,
    mut distort_materials: ResMut<Assets<DistortMaterial>>,
    mut images: ResMut<Assets<Image>>,
    depth: Res<DepthCopy>,
    mut white: Local<Option<Handle<Image>>>,
    mut lights: Local<Vec<Entity>>,
    mut buffers: Local<FrameBuffers>,
    mut streams: Option<ResMut<stream::VertexStreams>>,
) {
    if !player.enabled {
        return;
    }
    let Some(cam) = camera.iter().next() else {
        return;
    };
    let _timer = crate::perf_log::SystemTimer::start("fxr");
    let white = white
        .get_or_insert_with(|| {
            images.add(Image::new_fill(
                bevy::render::render_resource::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                &[255, 255, 255, 255],
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ))
        })
        .clone();
    let FrameBuffers {
        bufs,
        multi,
        lights: frame_lights,
        distort,
        trail,
    } = &mut *buffers;
    for b in bufs.values_mut().chain(distort.values_mut()) {
        b.clear();
    }
    for b in multi.values_mut() {
        b.clear();
    }
    frame_lights.clear();
    let mut frame = Frame {
        cam,
        bufs,
        multi,
        lights: frame_lights,
        distort,
        trail,
    };
    for e in player.effects.iter().filter(|e| !e.asleep) {
        if let Some(root) = &e.root {
            collect(root, &mut frame);
        }
    }
    // The 16 nearest the camera (many alike, such as item glints, would tie on brightness).
    let eye = frame.cam.translation();
    frame.lights.sort_by(|a, b| {
        a.position
            .distance_squared(eye)
            .total_cmp(&b.position.distance_squared(eye))
            .then(b.intensity.total_cmp(&a.intensity))
    });
    frame.lights.truncate(16);
    while lights.len() > frame.lights.len() {
        commands.entity(lights.pop().unwrap()).despawn();
    }
    for (i, fx_light) in frame.lights.iter().enumerate() {
        let light = fx_light.point_light();
        let transform = Transform::from_translation(fx_light.position);
        if let Some(&entity) = lights.get(i) {
            commands.entity(entity).insert((light, transform));
        } else {
            lights.push(commands.spawn((light, transform)).id());
        }
    }
    let (bufs, multi) = (frame.bufs, frame.multi);
    for key in frame.distort.keys() {
        if player.distort_batches.contains_key(key) {
            continue;
        }
        let texture = load_texture(&player.lib, *key, false)
            .map(|image| images.add(image))
            .unwrap_or_else(|| white.clone());
        let material = distort_materials.add(DistortMaterial { texture });
        let batch = new_batch(&mut commands, &mut meshes);
        commands
            .entity(batch.entity)
            .insert(MeshMaterial3d(material));
        player.distort_batches.insert(*key, batch);
    }
    for (key, batch) in &mut player.distort_batches {
        batch.update(
            &frame.distort[key],
            &mut commands,
            &mut meshes,
            streams.as_mut(),
        );
    }
    // New batches for keys seen for the first time.
    for key in bufs.keys() {
        if player.batches.contains_key(key) {
            continue;
        }
        let texture = texture_handle(&mut player, &mut images, key.0, &white);
        let material = sprite_materials.add(FxrMaterial {
            texture,
            depth: depth.image.clone(),
            additive: key.1,
        });
        let batch = new_batch(&mut commands, &mut meshes);
        commands
            .entity(batch.entity)
            .insert(MeshMaterial3d(material));
        player.batches.insert(*key, batch);
    }
    for key in multi.keys() {
        if player.multi_batches.contains_key(key) {
            continue;
        }
        let &(l1, l2, l3, modes, blend) = key;
        let t0 = texture_handle(&mut player, &mut images, l1, &white);
        let t1 = texture_handle(&mut player, &mut images, l2, &white);
        let t2 = texture_handle(
            &mut player,
            &mut images,
            if l3 > 0 { l3 } else { l2 },
            &white,
        );
        let material = multi_materials.add(MultiMaterial {
            t0,
            t1,
            t2,
            modes: UVec4::new(
                (modes & 0xf) as u32,
                (modes >> 4 & 0xf) as u32,
                (modes >> 8 & 0xf) as u32,
                (modes >> 12 & 0xf) as u32,
            ),
            flags: UVec4::new(
                (modes >> 16 & 1) as u32,
                additive(blend) as u32,
                (l3 > 0) as u32,
                0,
            ),
            depth: depth.image.clone(),
        });
        let batch = new_batch(&mut commands, &mut meshes);
        commands
            .entity(batch.entity)
            .insert(MeshMaterial3d(material));
        player.multi_batches.insert(*key, batch);
    }
    for (key, batch) in &mut player.batches {
        batch.update(&bufs[key], &mut commands, &mut meshes, streams.as_mut());
    }
    for (key, batch) in &mut player.multi_batches {
        batch.update(&multi[key], &mut commands, &mut meshes, streams.as_mut());
    }
    if log_enabled()
        && (bufs.values().any(|b| !b.is_empty()) || multi.values().any(|b| !b.is_empty()))
    {
        let mut v: Vec<String> = bufs
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(k, b)| format!("{}/{}:{}", k.0, k.1 as u8, b.pos.len() / 6))
            .collect();
        v.extend(
            multi
                .iter()
                .filter(|(_, b)| !b.is_empty())
                .map(|(k, b)| format!("multi {}:{}", k.0, b.pos.len() / 6)),
        );
        v.sort();
        info!(
            "fxr quads (step {}, {} particles) {}",
            player.steps,
            player.particles,
            v.join(" ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<FxrMaterial>()
            .init_asset::<MultiMaterial>()
            .init_asset::<DistortMaterial>()
            .init_resource::<DepthCopy>()
            .add_systems(Update, render);
        let mut player = FxrPlayer::new(PathBuf::from("missing-fxr-test-cache"));
        player.enabled = true;
        let nodes: Vec<_> = [600, 604, 607]
            .into_iter()
            .map(|kind| {
                serde_json::json!({
                    "type": 2200, "configs": [{"type": 1004,
                        "appearance": {"type": kind, "layer1": 1, "layer2": 2,
                            "layer3": 3, "normalMap": 4}}]
                })
            })
            .collect();
        player.lib.insert_for_test(
            1,
            serde_json::json!({"root": {"type": 2000, "nodes": nodes}}),
        );
        player.request(
            1,
            (0, 1, 0),
            Transform::from_xyz(0.0, 0.0, -3.0),
            true,
            true,
        );
        player.advance(0.0);
        app.insert_resource(player);
        app.world_mut()
            .spawn((Camera3d::default(), GlobalTransform::IDENTITY));
        app
    }

    fn modified_meshes(app: &mut App) -> usize {
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .drain()
            .filter(|e| matches!(e, AssetEvent::Modified { .. }))
            .count()
    }

    #[test]
    fn streaming_keeps_pipeline_meshes_static_while_particles_move() {
        let mut app = batch_test_app();
        app.init_resource::<stream::VertexStreams>();
        app.update();
        assert_eq!(modified_meshes(&mut app), 3);
        let tick = app
            .world()
            .get_resource_ref::<stream::VertexStreams>()
            .unwrap()
            .last_changed();
        app.update();
        assert_eq!(
            app.world()
                .get_resource_ref::<stream::VertexStreams>()
                .unwrap()
                .last_changed(),
            tick,
            "unchanged vertices must not be extracted again"
        );
        let mut before = app.world().resource::<stream::VertexStreams>().versions();
        before.sort_unstable();
        let mut player = app.world_mut().resource_mut::<FxrPlayer>();
        player.effects[0].at.translation.x += 1.;
        player.advance(0.);
        app.update();
        assert_eq!(
            modified_meshes(&mut app),
            0,
            "motion must not re-prepare the layout mesh"
        );
        let mut after = app.world().resource::<stream::VertexStreams>().versions();
        after.sort_unstable();
        assert_eq!(after.len(), 3);
        assert_ne!(before, after, "moving vertices must reach the stream");
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .iter()
                .all(|(_, mesh)| mesh.count_vertices() == 3 && mesh.indices().is_none())
        );
    }

    #[test]
    fn unchanged_batches_do_not_modify_mesh_assets() {
        let mut app = batch_test_app();
        app.update();
        assert_eq!(modified_meshes(&mut app), 3);
        app.update();
        assert_eq!(modified_meshes(&mut app), 0);
        let mut player = app.world_mut().resource_mut::<FxrPlayer>();
        player.effects[0].at.translation.x += 1.0;
        player.advance(0.0);
        app.update();
        assert_eq!(
            modified_meshes(&mut app),
            3,
            "changed positions still upload"
        );
        app.update();
        assert_eq!(modified_meshes(&mut app), 0);
    }

    #[test]
    fn empty_batches_hide_without_rewriting_and_reappear() {
        let mut app = batch_test_app();
        app.update();
        assert_eq!(modified_meshes(&mut app), 3);
        app.world_mut().resource_mut::<FxrPlayer>().effects[0].asleep = true;
        for _ in 0..2 {
            app.update();
            assert_eq!(modified_meshes(&mut app), 0);
            let world = app.world_mut();
            let batches: Vec<_> = world
                .query_filtered::<&Visibility, With<Mesh3d>>()
                .iter(world)
                .copied()
                .collect();
            assert_eq!(batches, vec![Visibility::Hidden; 3]);
            for (_, mesh) in world.resource::<Assets<Mesh>>().iter() {
                assert_eq!(mesh.count_vertices(), 9, "empty batches retain their data");
            }
        }
        app.world_mut().resource_mut::<FxrPlayer>().effects[0].asleep = false;
        app.update();
        assert_eq!(modified_meshes(&mut app), 0);
        let world = app.world_mut();
        assert!(
            world
                .query_filtered::<&Visibility, With<Mesh3d>>()
                .iter(world)
                .all(|v| *v == Visibility::Inherited)
        );
    }

    #[test]
    fn hit_contacts_are_held_on_the_body() {
        let feet = Vec3::new(10.0, 2.0, 10.0);
        let p = on_body(Vec3::new(10.6, 4.1, 10.0), feet);
        assert!((p.y - 3.5).abs() < 1e-5, "capped at 1.5 m");
        assert!(((p.x - feet.x).powi(2) + (p.z - feet.z).powi(2)).sqrt() <= 0.6001);
        let low = on_body(Vec3::new(10.1, 2.0, 10.0), feet);
        assert!((low.y - 2.5).abs() < 1e-5, "at least 0.5 m up");
        assert!((low.x - 10.1).abs() < 1e-5, "near contacts stay put");
    }

    #[test]
    fn dummy_frames_keep_forward_and_up() {
        let t = frame_transform([1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        assert!((t.rotation * Vec3::Z - Vec3::Z).length() < 1e-5);
        assert!((t.rotation * Vec3::Y - Vec3::Y).length() < 1e-5);
        // A frame facing +X: forward +X, up +Y gives X to -Z (right-handed).
        let t = frame_transform([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((t.rotation * Vec3::Z - Vec3::X).length() < 1e-5);
        assert!((t.rotation * Vec3::X - Vec3::NEG_Z).length() < 1e-5);
    }

    #[test]
    fn a_requested_effect_plays_out_and_is_dropped() {
        let mut player = FxrPlayer::new(PathBuf::from("missing-fxr-test-cache"));
        // A synthetic effect: a 0.1 s node emitting 2 billboards of 0.2 s every 0.05 s.
        let tree = serde_json::json!({"id": 1, "root": {"type": 2200, "stateConfigMap": [0], "nodes": [], "configs": [{
            "type": 1004,
            "nodeAttributes": {"type": 128, "delay": 0.0, "duration": 0.1},
            "emitter": {"type": 300, "interval": 0.05, "perEmission": 2.0, "totalEmissions": -1.0, "maxConcurrent": -1.0},
            "emitterShape": {"type": 400, "direction": 0},
            "directionSpread": {"type": 500},
            "particleAttributes": {"type": 129, "duration": 0.2, "attachment": 1},
            "particleModifier": {"type": 131, "speed": 1.0},
            "appearance": {"type": 603}}]}});
        player.lib.insert_for_test(1, tree);
        player.request(1, (0, 1, 0), Transform::IDENTITY, true, true);
        player.advance(1.0 / 60.0);
        assert_eq!(player.effects.len(), 1);
        assert_eq!(player.particles, 2);
        for _ in 0..90 {
            player.advance(1.0 / 60.0);
        }
        assert!(player.effects.is_empty());
        assert_eq!(player.particles, 0);
    }

    #[test]
    fn stoppable_effects_end_when_their_key_goes() {
        let mut player = FxrPlayer::new(PathBuf::from("missing-fxr-test-cache"));
        // An endless node: it only ends when stopped.
        let tree = serde_json::json!({"id": 2, "root": {"type": 2200, "stateConfigMap": [0], "nodes": [], "configs": [{
            "type": 1004,
            "nodeAttributes": {"type": 128, "delay": 0.0, "duration": -1.0},
            "emitter": {"type": 300, "interval": 0.05, "perEmission": 1.0, "totalEmissions": -1.0, "maxConcurrent": -1.0},
            "emitterShape": {"type": 400, "direction": 0},
            "directionSpread": {"type": 500},
            "particleAttributes": {"type": 129, "duration": 0.1, "attachment": 1},
            "particleModifier": {"type": 131, "speed": 0.0},
            "appearance": {"type": 603}}]}});
        player.lib.insert_for_test(2, tree);
        player.request(2, (0, 2, 7), Transform::IDENTITY, true, true);
        for _ in 0..30 {
            player.advance(1.0 / 60.0);
        }
        assert_eq!(player.effects.len(), 1);
        player.effects[0].stop = true;
        for _ in 0..30 {
            player.advance(1.0 / 60.0);
        }
        assert!(player.effects.is_empty());
    }

    #[test]
    fn batches_collect_six_vertices_per_quad() {
        let mut b = Buf::default();
        b.quad([Vec3::ZERO; 4], QUAD_UV, [1.0; 4], [0.0; 4]);
        assert_eq!(b.pos.len(), 6);
        assert_eq!(b.tan.len(), 6);
    }

    #[test]
    fn soft_distance_applies_only_with_depth_blend() {
        let cam = GlobalTransform::IDENTITY;
        let corners = [Vec3::new(0.0, 0.0, -5.0); 4];
        let off = serde_json::json!({});
        assert_eq!(soften(&Action::new(&off), 1.0, &cam, corners).1, 0.0);
        let on =
            serde_json::json!({"depthBlend": true, "unkDepthBlend1": 1.0, "unkDepthBlend2": 0.5});
        let (c, soft) = soften(&Action::new(&on), 2.0, &cam, corners);
        assert!(soft > 0.0);
        assert!(c[0].z > corners[0].z, "pulled toward the camera");
    }

    /// The native deferred point-light diffuse for a surface facing the light (N.L = 1), as
    /// `native_clustered_light` in `map_material.wesl` computes it.
    fn native_diffuse(light: &FxLight, d: f32) -> f32 {
        let fall = (1. - (d / light.range).powi(2)).clamp(0., 1.);
        let x = (d / light.falloff).powi(2);
        light.intensity * fall / (1. + 0.7 * x)
    }

    /// #141: the item glint (FFX 61) lights a small spot, not a 1.5 m pool, while a typical
    /// fire light (radius 5, falloff 2.5, the most common pair) still reaches the walls.
    #[test]
    fn item_glint_light_falls_off_within_half_a_metre() {
        let cache = sekiro_sim::cache::root();
        if !cache
            .join("raw/sfx/sfxbnd_commoneffects.ffxbnd.d/f000000061.fxr")
            .exists()
        {
            eprintln!("skipping: cache not extracted");
            return;
        }
        let mut player = FxrPlayer::new(cache);
        player.hold(GLINTS, 61, 0, Transform::IDENTITY, true);
        for _ in 0..90 {
            player.advance(1. / 60.);
        }
        let cam = GlobalTransform::from(Transform::from_xyz(0., 1.5, 3.));
        let mut buffers = FrameBuffers::default();
        let mut frame = Frame {
            cam: &cam,
            bufs: &mut buffers.bufs,
            multi: &mut buffers.multi,
            lights: &mut buffers.lights,
            distort: &mut buffers.distort,
            trail: &mut buffers.trail,
        };
        for e in &player.effects {
            collect(e.root.as_ref().unwrap(), &mut frame);
        }
        let [glint] = buffers.lights[..] else {
            panic!("one glint light, got {:?}", buffers.lights);
        };
        // Authored: radius 1.5, falloff 0.25, multiplier 0 to 6.4 over the first second.
        assert!((glint.range - 1.5).abs() < 1e-4);
        assert!((glint.falloff - 0.25).abs() < 1e-4);
        assert!((glint.intensity - 6.4).abs() < 1e-3, "{}", glint.intensity);
        assert_eq!(glint.point_light().radius, glint.falloff);
        let centre = native_diffuse(&glint, 0.);
        assert!(native_diffuse(&glint, 0.5) < 0.3 * centre);
        assert!(native_diffuse(&glint, 1.0) < 0.1 * centre);
        let fire = FxLight {
            range: 5.,
            falloff: 2.5,
            ..glint
        };
        assert!(native_diffuse(&fire, 2.) > 0.5 * native_diffuse(&fire, 0.));
    }
}
