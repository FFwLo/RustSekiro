//! Skill and prosthetic visual effects, driven by the characters' own TAE: SpawnOneShotFFX (96) /
//! SpawnFFX_Blade (118) events say which effect, on which dummy and for how long; StateInfo-gated
//! ones run while Wolf holds the matching state (the equipped tool's resident: 905 Shuriken, 911
//! Firecracker, 915 Flame Vent ...; 914 "no enchantment"; see sound::active_state_infos).
//! The effects themselves are the game's FXRs, played by fxr.rs; only an FFX with no extracted FXR
//! falls back to a hand-made look (`looks`, made to read like the game's: red mist on the Mortal
//! Blade, wind for Whirlwind Slash, sparks, fire, feathers, poison mist, petals, dust).
//! Also the default weapon trail on Wolf's sword while an AttackBehavior hit window runs (TAE 790
//! DisableDefaultWeaponTrail turns it off).

use bevy::prelude::*;
use std::collections::HashMap;

use crate::actor::{Actor, data_for};
use crate::data::Combat;
use crate::model::Dummies;

/// What a layer emits.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Glowing streaks (vfx::Particle).
    Spark,
    /// Soft balls that grow and fade (smoke, mist, dust, fire).
    Puff,
    /// The blade's trail (hilt to tip, kept a moment) in this colour while the event runs: the
    /// arts' own slash trails (they turn the default one off with TAE 790).
    Ribbon,
}

#[derive(Clone, Copy)]
struct Layer {
    kind: Kind,
    /// Linear colour (HDR above 1 glows through the bloom) and alpha for puffs.
    color: [f32; 4],
    /// Per second while the event runs; events of 4 frames or less burst `burst` at once.
    rate: f32,
    burst: u32,
    speed: (f32, f32),
    life: (f32, f32),
    /// Puff radius start -> end; spark width.
    size: (f32, f32),
    gravity: f32,
    /// 0 = along the dummy's forward, 1 = hemisphere, 2 = all directions.
    spread: f32,
    /// Emit at the floor under the dummy (dust, shockwaves).
    floor: bool,
}

const fn spark(color: [f32; 4], rate: f32, burst: u32, speed: (f32, f32), life: (f32, f32), width: f32, gravity: f32, spread: f32) -> Layer {
    Layer { kind: Kind::Spark, color, rate, burst, speed, life, size: (width, width), gravity, spread, floor: false }
}

const fn ribbon(color: [f32; 4]) -> Layer {
    Layer { kind: Kind::Ribbon, color, rate: 0.0, burst: 0, speed: (0.0, 0.0), life: (0.0, 0.0), size: (0.0, 0.0), gravity: 0.0, spread: 0.0, floor: false }
}

const fn puff(color: [f32; 4], rate: f32, burst: u32, speed: (f32, f32), life: (f32, f32), size: (f32, f32), gravity: f32, spread: f32, floor: bool) -> Layer {
    Layer { kind: Kind::Puff, color, rate, burst, speed, life, size, gravity, spread, floor }
}

// Palette (linear; > 1 glows).
const WHITE_HOT: [f32; 4] = [6.0, 6.0, 7.0, 1.0];
const WIND: [f32; 4] = [1.6, 2.2, 3.4, 0.4];
const TRAIL_WHITE: [f32; 4] = [1.0, 1.1, 1.3, 0.5];
const TRAIL_BLUE: [f32; 4] = [0.55, 1.0, 2.2, 0.5];
const TRAIL_RED: [f32; 4] = [2.2, 0.05, 0.03, 0.6];
const TRAIL_DARK: [f32; 4] = [0.3, 0.05, 0.25, 0.5];
const RED_GLOW: [f32; 4] = [5.0, 0.15, 0.08, 1.0];
const RED_MIST: [f32; 4] = [0.5, 0.0, 0.0, 0.35];
const BLACK_MIST: [f32; 4] = [0.03, 0.0, 0.01, 0.4];
const DUST: [f32; 4] = [0.45, 0.42, 0.38, 0.22];
const SMOKE: [f32; 4] = [0.25, 0.25, 0.25, 0.35];
const ORANGE: [f32; 4] = [6.0, 2.5, 0.6, 1.0];
const FIRE: [f32; 4] = [4.0, 1.2, 0.2, 0.55];
const FEATHER: [f32; 4] = [0.03, 0.03, 0.04, 1.0];
const POISON: [f32; 4] = [0.35, 0.05, 0.45, 0.4];
const PETAL: [f32; 4] = [3.0, 0.8, 1.4, 1.0];
const BLUE: [f32; 4] = [1.2, 2.5, 6.0, 1.0];
const GOLD_WIND: [f32; 4] = [2.5, 2.2, 1.0, 0.3];

/// The hand-made look of an FFX id with no extracted FXR (gap: made by eye). 304xxx are the upgraded tools'
/// variants of 300xxx; 4404xx-4405xx are the weapon-enchantment versions (gated off).
fn looks(ffx: i64) -> &'static [Layer] {
    const DUST_KICK: &[Layer] = &[puff(DUST, 0.0, 5, (0.4, 1.2), (0.4, 0.7), (0.08, 0.35), -0.2, 1.0, true)];
    const MORTAL_AURA: &[Layer] = &[
        puff(RED_MIST, 70.0, 6, (0.05, 0.3), (0.3, 0.7), (0.03, 0.12), -0.5, 2.0, false),
        spark(RED_GLOW, 40.0, 4, (0.2, 0.8), (0.15, 0.35), 0.006, -1.0, 2.0),
    ];
    const MORTAL_TRAIL: &[Layer] = &[puff(BLACK_MIST, 90.0, 8, (0.05, 0.4), (0.3, 0.6), (0.04, 0.16), -0.3, 2.0, false)];
    const MORTAL_SLASH: &[Layer] = &[
        spark(RED_GLOW, 0.0, 70, (3.0, 9.0), (0.15, 0.45), 0.012, 1.0, 1.2),
        puff(RED_MIST, 0.0, 16, (0.5, 2.5), (0.4, 0.9), (0.1, 0.5), -0.2, 1.2, false),
    ];
    const MORTAL_END: &[Layer] = &[puff(BLACK_MIST, 0.0, 14, (0.3, 1.0), (0.5, 1.0), (0.1, 0.45), -0.3, 2.0, false)];
    const WHIRL_TRAIL: &[Layer] = &[ribbon(TRAIL_BLUE), spark(WHITE_HOT, 0.0, 30, (3.0, 7.0), (0.12, 0.3), 0.012, 0.0, 1.0)];
    // A gust ring around Wolf's feet: fast pale streaks out along the floor, a little dust.
    const WHIRL: &[Layer] = &[
        Layer { kind: Kind::Spark, color: [1.8, 2.2, 3.0, 1.0], rate: 0.0, burst: 50, speed: (5.0, 9.0), life: (0.12, 0.25), size: (0.01, 0.01), gravity: 0.0, spread: 0.0, floor: true },
        puff(DUST, 0.0, 8, (1.5, 3.0), (0.25, 0.45), (0.08, 0.3), 0.0, 1.0, true),
    ];
    const SLASH_TRAIL: &[Layer] = &[ribbon(TRAIL_WHITE)];
    const MORTAL_RIBBON: &[Layer] = &[ribbon(TRAIL_RED), puff(BLACK_MIST, 90.0, 8, (0.05, 0.4), (0.3, 0.6), (0.04, 0.16), -0.3, 2.0, false)];
    const DARK_TRAIL: &[Layer] = &[ribbon(TRAIL_DARK), puff(BLACK_MIST, 0.0, 10, (0.3, 1.5), (0.3, 0.6), (0.08, 0.3), -0.2, 2.0, false)];
    const WIND_BURST: &[Layer] = &[puff(WIND, 0.0, 10, (1.0, 3.0), (0.25, 0.45), (0.12, 0.5), 0.0, 1.5, false)];
    const GROUND_SHOCK: &[Layer] = &[
        puff(DUST, 0.0, 18, (1.5, 4.0), (0.35, 0.7), (0.15, 0.5), -0.1, 1.0, true),
        spark(WHITE_HOT, 0.0, 16, (2.0, 5.0), (0.1, 0.25), 0.005, 6.0, 1.0),
    ];
    const BLADE_GLINT: &[Layer] = &[spark(WHITE_HOT, 0.0, 12, (0.5, 2.0), (0.08, 0.2), 0.008, 0.0, 2.0)];
    const CROSS_SLASH: &[Layer] = &[spark(WHITE_HOT, 0.0, 40, (5.0, 10.0), (0.08, 0.2), 0.008, 0.0, 0.5)];
    const KICK: &[Layer] = &[puff(WIND, 40.0, 4, (0.2, 0.8), (0.2, 0.35), (0.05, 0.18), 0.0, 2.0, false)];
    const FIST: &[Layer] = &[spark(WHITE_HOT, 0.0, 18, (2.0, 5.0), (0.08, 0.18), 0.006, 0.0, 1.0), puff(WIND, 0.0, 4, (0.5, 1.5), (0.2, 0.35), (0.08, 0.3), 0.0, 1.0, false)];
    const SHADOW: &[Layer] = &[puff(BLACK_MIST, 0.0, 20, (0.5, 2.0), (0.4, 0.8), (0.12, 0.5), -0.2, 2.0, false)];
    const PETALS: &[Layer] = &[spark(PETAL, 0.0, 50, (1.0, 4.0), (0.6, 1.2), 0.012, 0.6, 2.0)];
    const ARM_PUFF: &[Layer] = &[puff(SMOKE, 0.0, 4, (0.1, 0.4), (0.3, 0.6), (0.03, 0.12), -0.2, 2.0, false)];
    const HAND_SPARKS: &[Layer] = &[spark(ORANGE, 0.0, 26, (2.0, 6.0), (0.1, 0.3), 0.006, 6.0, 1.0), puff(SMOKE, 0.0, 6, (0.2, 0.8), (0.4, 0.8), (0.05, 0.2), -0.3, 1.0, false)];
    const FLAME: &[Layer] = &[puff(FIRE, 120.0, 10, (2.0, 5.0), (0.2, 0.45), (0.04, 0.25), -2.0, 0.15, false), spark(ORANGE, 40.0, 6, (2.0, 6.0), (0.2, 0.5), 0.005, -1.0, 0.3)];
    const AXE: &[Layer] = &[spark(ORANGE, 0.0, 40, (2.0, 7.0), (0.15, 0.4), 0.007, 8.0, 1.5)];
    const FEATHERS: &[Layer] = &[spark(FEATHER, 0.0, 40, (1.0, 4.0), (0.8, 1.5), 0.02, 0.8, 2.0), puff(BLACK_MIST, 0.0, 8, (0.5, 1.5), (0.4, 0.8), (0.1, 0.4), -0.2, 2.0, false)];
    const POISON_MIST: &[Layer] = &[puff(POISON, 0.0, 10, (0.3, 1.2), (0.6, 1.2), (0.08, 0.35), -0.1, 1.0, false)];
    const GUARD_SPARKS: &[Layer] = &[spark(BLUE, 0.0, 20, (2.0, 5.0), (0.1, 0.25), 0.006, 4.0, 1.0)];
    const VORTEX: &[Layer] = &[puff(GOLD_WIND, 80.0, 8, (1.5, 4.0), (0.3, 0.6), (0.1, 0.45), 0.0, 1.2, false)];
    const WHISTLE: &[Layer] = &[puff(WIND, 0.0, 12, (2.0, 4.0), (0.3, 0.5), (0.1, 0.3), 0.0, 2.0, false)];
    match ffx {
        400 | 401 | 410 | 411 | 420 | 430 => DUST_KICK,
        440000 => MORTAL_AURA,
        440001 | 440005 => MORTAL_RIBBON,
        440002 | 440006 => MORTAL_TRAIL,
        440003 | 440004 | 440007 | 440008 => MORTAL_SLASH,
        440009 => MORTAL_END,
        440030 => WHIRL_TRAIL,
        440031 | 440032 => WHIRL,
        440052 | 440090 | 440115 => SLASH_TRAIL,
        440061 => DARK_TRAIL,
        440051 => WIND_BURST,
        440081 | 440108 | 440109 | 440133 | 440091 | 440063 => GROUND_SHOCK,
        600030 => BLADE_GLINT,
        440070 => SLASH_TRAIL,
        440071 | 440072 | 440080 | 440130 | 440132 => CROSS_SLASH,
        440111..=440115 => KICK,
        440040 | 440041 => FIST,
        440062 => SHADOW,
        440603 | 440683 | 440684 => PETALS,
        300170 | 300181 => ARM_PUFF,
        300070..=300074 | 304021..=304025 | 300240 | 300241 | 300080 | 304500 | 304600 | 304601 => HAND_SPARKS,
        300160..=300168 | 300260..=300271 => FLAME,
        300152..=300159 | 300211 => AXE,
        303020..=303025 | 304050..=304055 => FEATHERS,
        300212..=300216 => POISON_MIST,
        300220..=300227 | 304220..=304228 | 30202 | 30204 => GUARD_SPARKS,
        300192..=300198 | 304192..=304198 => VORTEX,
        300180..=300188 | 304182..=304185 | 401004 => AXE,
        300200..=300207 | 304200..=304207 => WHISTLE,
        _ => &[],
    }
}

/// A dummy id as the TAE writes it -> the dummies to try: 1xxxx = Wolf's right weapon (the blade;
/// 12200 / 12220 are the Mortal Blade's, which is not modelled: the sword stands in), 2xxxx the
/// prosthetic tool's parts (model/tool.rs: 2<model><dummy>; without the part, the left hand / arm
/// dummies stand in).
fn dummy_candidates(id: i64) -> Vec<i16> {
    match id {
        // 1<model><dummy>: Model2's are kept under their full id (model.rs), Model0's under the bare one.
        10000..=19999 => vec![id as i16, (id % 10000) as i16, 300, 301, 311, 121, 100, 20],
        20000..=29999 => vec![id as i16, (id % 10000) as i16, 145, 153, 144, 2],
        _ => vec![id as i16, 220],
    }
}

#[derive(Component)]
struct Puff {
    vel: Vec3,
    age: f32,
    life: f32,
    size: (f32, f32),
    gravity: f32,
}

#[derive(Resource, Default)]
struct FfxAssets {
    /// A unit quad turned to the camera, with a soft round texture (smoke / mist / fire puffs).
    ball: Handle<Mesh>,
    soft: Handle<Image>,
    mats: HashMap<[u32; 4], Handle<StandardMaterial>>,
}

/// Art trail requests (by "is Wolf"): colour and the game time it holds until.
#[derive(Resource, Default)]
struct Ribbons(HashMap<bool, ([f32; 4], f32)>);

#[derive(Component)]
struct Trail {
    owner: Entity,
    /// (base, tip, age) samples, newest last.
    samples: Vec<(Vec3, Vec3, f32)>,
    /// Linear colour and strength of the current trail.
    color: [f32; 4],
    mesh: Handle<Mesh>,
}

pub struct FfxPlugin;

impl Plugin for FfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FfxAssets>()
            .init_resource::<Ribbons>()
            .add_systems(Startup, setup)
            .add_systems(Update, (emit, animate_puffs, weapon_trail, skill_shots));
    }
}

fn setup(mut assets: ResMut<FfxAssets>, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    assets.ball = meshes.add(Rectangle::new(2.0, 2.0));
    // Soft disc: alpha falls off as (1 - r^2)^2, a little noise so puffs read as smoke.
    const N: usize = 64;
    let mut px = Vec::with_capacity(N * N * 4);
    let mut seed = 0x1234_5678u32;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = ((x as f32 + 0.5) / N as f32 * 2.0 - 1.0, (y as f32 + 0.5) / N as f32 * 2.0 - 1.0);
            let r2 = dx * dx + dy * dy;
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = 0.8 + 0.2 * ((seed % 1000) as f32 / 1000.0);
            let a = ((1.0 - r2).max(0.0)).powi(2) * noise;
            px.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    let img = Image::new(
        bevy::render::render_resource::Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        px,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    assets.soft = images.add(img);
}

fn material(assets: &mut FfxAssets, materials: &mut Assets<StandardMaterial>, c: [f32; 4], kind: Kind) -> Handle<StandardMaterial> {
    let key = c.map(f32::to_bits);
    let soft = assets.soft.clone();
    assets
        .mats
        .entry(key)
        .or_insert_with(|| {
            let puff = kind == Kind::Puff;
            materials.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::new(c[0], c[1], c[2], if puff { c[3] } else { 1.0 })),
                base_color_texture: puff.then_some(soft),
                unlit: true,
                alpha_mode: if puff { AlphaMode::Blend } else { AlphaMode::Opaque },
                cull_mode: None,
                ..default()
            })
        })
        .clone()
}

struct Rng(u32);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 100_000) as f32 / 100_000.0
    }
    fn range(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.f()
    }
    fn dir(&mut self, n: Vec3, spread: f32) -> Vec3 {
        if spread >= 2.0 {
            let v = Vec3::new(self.f() * 2.0 - 1.0, self.f() * 2.0 - 1.0, self.f() * 2.0 - 1.0);
            return v.normalize_or(Vec3::Y);
        }
        let (t, b) = n.any_orthonormal_pair();
        let phi = self.f() * std::f32::consts::TAU;
        let cos_t = 1.0 - self.f() * spread.min(1.9);
        let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
        (n * cos_t + (t * phi.cos() + b * phi.sin()) * sin_t).normalize_or(n)
    }
}

#[allow(clippy::too_many_arguments)]
fn emit(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<crate::config::GameConfig>,
    mut assets: ResMut<FfxAssets>,
    vfx: Option<Res<crate::vfx::VfxAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    actors: Query<(&Actor, &Dummies, Option<&crate::player::Player>)>,
    globals: Query<&GlobalTransform>,
    mut rng: Local<Option<Rng>>,
    mut frames: Local<u32>,
    mut ribbons: ResMut<Ribbons>,
    mut lib: ResMut<crate::fxr::FxrLib>,
    actor_e: Query<Entity, With<Actor>>,
    mut playing: Query<&mut crate::fxr::FxEffect>,
) {
    // Slotted / held effects end with their TAE event or when the anim changes.
    for mut f in &mut playing {
        if let Some((owner, anim, idx)) = f.key.clone() {
            let alive = actors.iter().zip(actor_e.iter()).find(|(_, e)| *e == owner).is_some_and(|((a, _, _), _)| {
                a.anim == anim && data_for(&combat, &a).anim(&a.anim).and_then(|an| an.events.get(idx)).is_some_and(|ev| a.t < ev.end)
            });
            if !alive {
                f.stop = true;
            }
        }
    }
    let Some(vfx) = vfx else { return };
    *frames += 1;
    if std::env::var("SHINOBI_LOG_DUMMIES").is_ok() && *frames == 200 {
        for (a, dm, p) in &actors {
            if p.is_some() {
                let mut ids: Vec<i16> = dm.0.keys().copied().collect();
                ids.sort();
                info!("wolf dummies ({}): {:?}", a.state, ids);
                let hand = dm.0.get(&20).and_then(|e| globals.get(*e).ok()).map(|g| g.translation()).unwrap_or_default();
                for k in [100i16, 101, 102, 110, 111, 112, 113, 120, 121, 123, 124, 300, 301, 311, 321, 331, 147, 149] {
                    if let Some(g) = dm.0.get(&k).and_then(|e| globals.get(*e).ok()) {
                        info!("dummy {k}: {:.2} m from the sword hand, at {:.2}", g.translation().distance(hand), g.translation());
                    }
                }
            }
        }
    }
    let rng = rng.get_or_insert(Rng(0x2468_1357));
    let dt = time.delta_secs();
    for ((a, dm, player), owner) in actors.iter().zip(actor_e.iter()) {
        if a.anim.is_empty() || a.t < a.prev_t {
            continue;
        }
        let d = data_for(&combat, &a);
        let Some(anim) = d.anim(&a.anim) else { continue };
        let mut states: Option<Vec<i64>> = None;
        for (idx, e) in anim.events.iter().enumerate().filter(|(_, e)| matches!(e.kind, 96 | 118)) {
            let Some(ffx) = e.arg_i64("FFXID").filter(|v| *v > 0) else { continue };
            let game_fx = lib.has(ffx);
            let layers = if game_fx { &[][..] } else { looks(ffx) };
            if layers.is_empty() && !game_fx {
                continue;
            }
            let (from, to) = (a.prev_t.max(e.start), a.t.min(e.end));
            let started = e.start > a.prev_t && e.start <= a.t || (a.prev_t == 0.0 && e.start == 0.0 && a.t > 0.0);
            if to <= from && !started {
                continue;
            }
            if !e.ungated() {
                let gate = e.arg_i64("StateInfo").unwrap_or(0);
                let tool = player.and_then(|p| crate::player::equipped_tool(&combat, &config, p.tool_slot)).map(|t| t.id);
                let active = states.get_or_insert_with(|| crate::sound::active_state_infos(&combat, a, Some(&config), tool));
                if !active.contains(&gate) {
                    continue;
                }
            }
            // SpawnFFX_Blade (118): DummyPolyBladeBaseID (DSAnimStudio TAE.Template.SDT: FFXID,
            // DummyPolySource, DummyPolyBladeBaseID, DummyPolyBladeTipID; Wolf's are the axe's /
            // spear's 21300 with tip -1), else the sword's blade.
            let dmy = if e.kind == 118 {
                e.arg_i64("DummyPolyBladeBaseID").filter(|d| *d >= 0).unwrap_or(10301)
            } else {
                e.arg_i64("DummyPolyID").unwrap_or(10301)
            };
            let Some(g) = dummy_candidates(dmy).iter().find_map(|k| dm.0.get(k)).and_then(|e| globals.get(*e).ok()) else { continue };
            let (mut pos, fwd) = (g.translation(), (g.rotation() * Vec3::Z).normalize_or(Vec3::Y));
            // A weapon effect on a weapon dummy that is not loaded: along Kusabimaru's blade instead,
            // hilt (dummy 300) to tip (301).
            let blade = |k: i16| dm.0.get(&k).and_then(|e| globals.get(*e).ok()).map(|g| g.translation());
            let along = (10000..20000).contains(&dmy) && !dm.0.contains_key(&(dmy as i16)) && !dm.0.contains_key(&((dmy % 10000) as i16));
            let seg = if along { blade(300).zip(blade(301)) } else { None };
            // The game's own effect (FXR), when the archive has it: spawned at the event start on the
            // dummy (IsFollowDummyPoly: rides on it); a slotted one (SlotID >= 0) ends with the event.
            if game_fx {
                if started {
                    // A weapon dummy that is not loaded: a frame along Kusabimaru's blade, hilt 300 -> tip 301.
                    let blade = (dm.0.get(&300).copied(), dm.0.get(&301).copied());
                    let anchor = match (along, blade) {
                        (true, (Some(h), Some(t))) => Some(crate::fxr::Anchor::Blade(h, t)),
                        _ => dummy_candidates(dmy).iter().find_map(|k| dm.0.get(k)).copied().map(crate::fxr::Anchor::Entity),
                    };
                    let follow = e.args.get("IsFollowDummyPoly").and_then(|v| v.as_bool()).unwrap_or(true);
                    if std::env::var("SHINOBI_FFX_LOG").is_ok() {
                        info!("fxr {ffx} on {dmy} ({} {:.2}) at {:.2}", a.anim, a.t, pos);
                    }
                    let mut fx = crate::fxr::FxEffect::new(ffx, anchor, follow, Transform::from_translation(pos));
                    if e.arg_i64("SlotID").is_some_and(|s| s >= 0) || e.kind == 118 {
                        fx.key = Some((owner, a.anim.clone(), idx));
                    }
                    commands.spawn(fx);
                }
                continue;
            }
            let short = e.end - e.start <= 4.5 / crate::data::TAE_FPS;
            if std::env::var("SHINOBI_FFX_LOG").is_ok() && started {
                info!("ffx {ffx} on {dmy} ({} {:.2}) at {:.2}", a.anim, a.t, pos);
            }
            for l in layers {
                if l.kind == Kind::Ribbon {
                    // Held through the event (and a short tail) - sampled by weapon_trail.
                    let left = (e.end - a.t).max(0.0) + 0.06;
                    ribbons.0.insert(a.side == crate::actor::Side::Player, (l.color, time.elapsed_secs() + left));
                    continue;
                }
                let n = if short || l.rate == 0.0 {
                    if started { l.burst } else { 0 }
                } else {
                    (l.rate * (to - from).max(dt.min(to - from + dt)) + rng.f()) as u32 + if started { l.burst } else { 0 }
                };
                let n_axis = if l.floor { Vec3::Y } else { fwd };
                for _ in 0..n {
                    if let Some((h, tip)) = seg {
                        pos = h.lerp(tip, rng.f());
                    }
                    let at = if l.floor { pos.with_y(0.03) } else { pos };
                    let dir = if l.floor {
                        let a = rng.f() * std::f32::consts::TAU;
                        Vec3::new(a.cos(), 0.15, a.sin()).normalize()
                    } else {
                        rng.dir(n_axis, l.spread)
                    };
                    let v = dir * rng.range(l.speed);
                    let life = rng.range(l.life);
                    match l.kind {
                        Kind::Spark => {
                            commands.spawn((
                                Mesh3d(vfx.streak.clone()),
                                MeshMaterial3d(material(&mut assets, &mut materials, l.color, Kind::Spark)),
                                Transform::from_translation(at).with_scale(Vec3::splat(l.size.0)),
                                crate::vfx::Particle { vel: v, age: 0.0, life, width: l.size.0, stretch: 0.02, gravity: l.gravity, drag: 1.0 },
                                bevy::light::NotShadowCaster,
                            ));
                        }
                        Kind::Ribbon => {}
                        Kind::Puff => {
                            commands.spawn((
                                Mesh3d(assets.ball.clone()),
                                MeshMaterial3d(material(&mut assets, &mut materials, l.color, Kind::Puff)),
                                Transform::from_translation(at + dir * 0.02).with_scale(Vec3::splat(l.size.0)),
                                Puff { vel: v, age: 0.0, life, size: l.size, gravity: l.gravity },
                                bevy::light::NotShadowCaster,
                            ));
                        }
                    }
                }
            }
        }
    }
}

fn animate_puffs(mut commands: Commands, time: Res<Time>, camera: Query<&GlobalTransform, With<Camera3d>>, mut q: Query<(Entity, &mut Puff, &mut Transform)>) {
    let dt = time.delta_secs();
    let face = camera.iter().next().map(|c| c.rotation());
    for (e, mut p, mut tf) in &mut q {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        p.vel *= (1.0 - 2.5 * dt).max(0.0);
        tf.translation += p.vel * dt;
        // Grows, and shrinks away in its last third (the blend material keeps its alpha).
        let k = p.age / p.life;
        let grow = p.size.0 + (p.size.1 - p.size.0) * k.sqrt();
        let fade = if k > 0.66 { 1.0 - (k - 0.66) / 0.34 } else { 1.0 };
        tf.scale = Vec3::splat(grow * fade.max(0.05));
        // Billboard: the quad faces the camera.
        if let Some(r) = face {
            tf.rotation = r;
        }
    }
}

/// Default weapon trail, while an AttackBehavior (TAE 1) window runs on Wolf - unless TAE 790
/// DisableDefaultWeaponTrail does: the weapon's EquipParamWeapon traceSfxId0 from dummy
/// traceDmyIdHead0 to traceDmyIdTail0 (every sword row: FXR 401000, 300 -> 301; the row is the
/// equipped art's, 5000 "style: none" without one), played by fxr.rs while the window runs.
/// Without that FXR (and for the hand-made art ribbons, `Ribbons`): a ribbon from the blade's base
/// to its tip, kept for ~0.12 s (gap: made to read like the game's pale streak).
#[allow(clippy::too_many_arguments)]
fn weapon_trail(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    config: Res<crate::config::GameConfig>,
    mut lib: ResMut<crate::fxr::FxrLib>,
    mut effects: Query<&mut crate::fxr::FxEffect>,
    mut game_trail: Local<Option<Entity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    wolf: Query<(Entity, &Actor, &Dummies), With<crate::player::Player>>,
    globals: Query<&GlobalTransform>,
    mut trails: Query<(Entity, &mut Trail, &MeshMaterial3d<StandardMaterial>)>,
    mut made: Local<Option<Handle<StandardMaterial>>>,
    ribbons: Res<Ribbons>,
) {
    const KEEP: f32 = 0.12;
    let dt = time.delta_secs();
    if std::env::var("SHINOBI_FFX_LOG").is_ok() {
        for (_, a, _) in &wolf {
            let n = combat.player.events_at(&a.anim, a.t).filter(|e| e.kind == 1).count();
            if n > 0 || a.anim.starts_with("a100") {
                info!("trail check {} {:.2} attack events {n}", a.anim, a.t);
            }
        }
    }
    let mat = made
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                alpha_mode: AlphaMode::Add,
                cull_mode: None,
                double_sided: true,
                ..default()
            })
        })
        .clone();
    let Ok((we, a, dm)) = wolf.single() else { return };
    let d = &combat.player;
    let default_on = !a.anim.is_empty() && d.events_at(&a.anim, a.t).any(|e| e.kind == 1) && !d.events_at(&a.anim, a.t).any(|e| e.kind == 790);
    let art = ribbons.0.get(&true).filter(|(_, until)| *until > time.elapsed_secs());
    // The game's trail: started when the window opens, stopped (its segments fade out) when it ends.
    let row = combat.param("EquipParamWeapon", if config.player.combat_art > 0 { config.player.combat_art } else { 5000 });
    let sfx = row["traceSfxId0"].as_i64().unwrap_or(-1);
    let ends = (row["traceDmyIdHead0"].as_i64(), row["traceDmyIdTail0"].as_i64());
    let blade = match ends {
        (Some(h), Some(t)) => dm.0.get(&(h as i16)).copied().zip(dm.0.get(&(t as i16)).copied()),
        _ => None,
    };
    let game = sfx > 0 && blade.is_some() && lib.has(sfx);
    if game {
        let live = game_trail.and_then(|e| effects.get_mut(e).ok());
        match (default_on, live) {
            (true, None) => {
                let (h, t) = blade.unwrap();
                *game_trail = Some(commands.spawn(crate::fxr::FxEffect::new(sfx, Some(crate::fxr::Anchor::Blade(h, t)), true, Transform::IDENTITY)).id());
            }
            (false, Some(mut f)) => {
                f.stop = true;
                *game_trail = None;
            }
            _ => {}
        }
    }
    let swinging = (default_on && !game) || art.is_some();
    let color = art.map_or([1.0, 1.05, 1.15, 0.35], |r| r.0);
    if std::env::var("SHINOBI_FFX_LOG").is_ok() && swinging {
        info!("trail on ({} {:.2}) seg {:?}", a.anim, a.t, dm.0.get(&20).is_some());
    }
    // The blade from its base (the right hand, dummy 20) to the tip: the weapon's own blade
    // dummies when the model has them, else 0.9 m along the hand dummy's forward.
    let hand = dm.0.get(&20).and_then(|e| globals.get(*e).ok());
    let tip_dmy = [301, 311, 300].iter().find_map(|k| dm.0.get(k)).and_then(|e| globals.get(*e).ok());
    let seg = hand.map(|h| {
        let base = h.translation();
        let tip = tip_dmy.map_or(base + h.rotation() * Vec3::Z * 0.9, |t| t.translation());
        (base + (tip - base) * 0.2, tip)
    });
    let trail = trails.iter_mut().find(|(_, t, _)| t.owner == we);
    let (te, mut t, tm) = match trail {
        Some(x) => x,
        None => {
            let mesh = meshes.add(Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()));
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::IDENTITY, Trail { owner: we, samples: Vec::new(), mesh, color: [1.0; 4] }, bevy::light::NotShadowCaster));
            return;
        }
    };
    let _ = &tm;
    for s in t.samples.iter_mut() {
        s.2 += dt;
    }
    t.samples.retain(|s| s.2 < KEEP);
    if let (true, Some((b, tip))) = (swinging, seg) {
        t.samples.push((b, tip, 0.0));
        t.color = color;
    }
    let Some(mut m) = meshes.get_mut(&t.mesh) else { return };
    // Never empty (an empty mesh upset the render slab allocator): one degenerate triangle.
    let mut pos: Vec<[f32; 3]> = vec![[0.0, -10.0, 0.0]; 3];
    let mut col: Vec<[f32; 4]> = vec![[0.0; 4]; 3];
    // Newest bright at the tip, fading out with age and toward the hilt (additive: colour x alpha).
    let c = t.color;
    let shade = |age: f32, tip: bool| {
        let k = (1.0 - age / KEEP).clamp(0.0, 1.0) * c[3] * if tip { 1.0 } else { 0.25 };
        [c[0] * k, c[1] * k, c[2] * k, 1.0]
    };
    for w in t.samples.windows(2) {
        let ((b0, t0, a0), (b1, t1, a1)) = (w[0], w[1]);
        for (p, age, tip) in [(b0, a0, false), (t0, a0, true), (b1, a1, false), (b1, a1, false), (t0, a0, true), (t1, a1, true)] {
            pos.push(p.to_array());
            col.push(shade(age, tip));
        }
    }
    let n = pos.len();
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    let _ = te;
}

/// SHINOBI_SKILL=<EquipParamWeapon id> (a combat art 5100-7700 or a tool 70000-79200) with
/// SHINOBI_SKILL_SHOTS=<dir>: Wolf equips it, uses it 3 s in (art: attack + guard; tool: F) and
/// screenshots every 3 frames for 1.5 s, then the game exits. For checking the looks.
/// SHINOBI_SKILL_EVERY=<frames> repeats the press.
fn skill_shots(
    mut commands: Commands,
    mut config: ResMut<crate::config::GameConfig>,
    mut pad: ResMut<crate::player::PadInput>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
    mut cam: Query<&mut crate::camera::OrbitCamera>,
    wolf: Query<(&crate::actor::Actor, &Transform), With<crate::player::Player>>,
) {
    let (Ok(id), Ok(dir)) = (std::env::var("SHINOBI_SKILL"), std::env::var("SHINOBI_SKILL_SHOTS")) else { return };
    let Ok(id) = id.parse::<i64>() else { return };
    *frame += 1;
    let f = *frame;
    if f == 2 {
        if (70000..80000).contains(&id) {
            config.player.prosthetics = vec![id];
        } else {
            config.player.combat_art = id;
        }
    }
    // SHINOBI_SKILL_CAM="yaw,pitch" [deg]: hold the camera there.
    if let Ok(v) = std::env::var("SHINOBI_SKILL_CAM") {
        let v: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        if let (Some(&y), Ok(mut oc)) = (v.first(), cam.single_mut()) {
            oc.yaw = y.to_radians();
            oc.pitch = v.get(1).copied().unwrap_or(10.0).to_radians();
        }
    }
    let start = 180;
    // SHINOBI_SKILL_EVERY=<frames>: press again every so many frames (build-ups: the Flame Vent's burn).
    let every = std::env::var("SHINOBI_SKILL_EVERY").ok().and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0);
    if f == start || every.is_some_and(|n| f > start && (f - start) % n == 0) {
        if (70000..80000).contains(&id) {
            pad.press(crate::player::Action::Prosthetic);
        } else {
            pad.press(crate::player::Action::CombatArt);
        }
    }
    // SHINOBI_SKILL_SHOT_RANGE="from,to,step" frames after the press (default 3,90,3).
    let r: Vec<u32> = std::env::var("SHINOBI_SKILL_SHOT_RANGE").unwrap_or("3,90,3".into()).split(',').filter_map(|v| v.parse().ok()).collect();
    let (from, to, step) = if r.len() == 3 { (r[0], r[1], r[2].max(1)) } else { (3, 90, 3) };
    if f >= start + from && f <= start + to && (f - start - from) % step == 0 {
        let path = format!("{dir}/s{:03}.png", f - start);
        if let Ok((a, tf)) = wolf.single() {
            let fwd = tf.rotation * Vec3::Z;
            info!("shot s{:03}: {} {} t {:.2} at {:.2} facing {:.0}", f - start, a.state, a.anim, a.t, tf.translation, fwd.x.atan2(fwd.z).to_degrees());
        }
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
    }
    if f > start + to + 10 {
        exit.write(AppExit::Success);
    }
}
