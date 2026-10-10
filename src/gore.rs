//! Blood like the game's: driven by each character's own TAE, so a deathblow bleeds where and when
//! the game's does.
//! - SpawnOneShotFFX (TAE 96) with a blood FFX: a spray from the event's dummy along the dummy's
//!   forward for the event's length (c1010 behind deathblow a000_012200: FFX 220505 on dummy 802,
//!   the side of the neck, frames 46-81; Wolf's blade 220502 / 220503 on sword dummies 121 / 100).
//!   The game's own effect (extracted/fxr/<id>.json) plays through fxr.rs (ffx.rs spawns it); this
//!   hand-made spray (made to read like clip 1: a long dark-red jet of fine drops that thins out)
//!   only stands in when that FXR is not extracted.
//! - DecalParamID_DummyPoly (TAE 138): a blood stain under the dummy, from DecalParam (710011: the
//!   game's own splat mask dp000160000 and diffuse dp000100000, extracted/decal/<id>.png, tinted by
//!   diffuseColorR/G/B, projected straight down (pitchAngle -90), randomSize 100-120 %, a random
//!   roll, at most distThinOutMaxNum stains within distThinOutCheckDist of each other).

use bevy::prelude::*;

use crate::actor::{Actor, data_for};
use crate::data::Combat;
use crate::model::Dummies;
use crate::vfx::{Particle, VfxAssets, VfxRng};

/// Blood FFX ids: 220505 / 220506 the deathblow gush, 220502 / 220503 the blood on Wolf's blade.
fn blood_rate(ffx: i64) -> Option<f32> {
    // gap: emission per second, chosen to read like clip 1 (FXR not decoded).
    match ffx {
        220505 | 220506 => Some(2000.0),
        220502 | 220503 => Some(90.0),
        _ => None,
    }
}

/// Stains alive at once. gap: the game keeps them (lifeTimeSec 999, bLifeEnable 0) under an
/// engine-wide decal budget that is not traced.
const MAX_STAINS: usize = 24;

#[derive(Component)]
struct Stain {
    id: i64,
    born: f32,
}

#[derive(Resource)]
struct GoreAssets {
    plane: Handle<Mesh>,
    drop: Handle<StandardMaterial>,
    mist: Handle<StandardMaterial>,
}

/// SHINOBI_GORE_LOG=1: a line per blood event as it fires.
fn log(f: impl FnOnce() -> String) {
    if std::env::var("SHINOBI_GORE_LOG").is_ok() {
        eprintln!("GORE {}", f());
    }
}

/// extracted/decal/<id>.png with the mask's alpha lifted: the stain reads as a dark pool in the
/// middle and fine flecks at the edge. gap: the deferred-decal blend (maskScale 1.0, the mask's
/// use in the shader) is not traced; alpha^0.55 is chosen to match clip 1's dark pools.
fn stain_texture(id: i64) -> Option<Image> {
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
    let bytes = std::fs::read(crate::paths::extracted().join(format!("decal/{id}.png"))).ok()?;
    let mut img = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, bevy::asset::RenderAssetUsages::default()).ok()?;
    if let Some(data) = img.data.as_mut() {
        for px in data.chunks_exact_mut(4) {
            px[3] = ((px[3] as f32 / 255.0).powf(0.55) * 255.0) as u8;
        }
    }
    Some(img)
}

pub struct GorePlugin;

impl Plugin for GorePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, bleed);
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(GoreAssets {
        plane: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
        // Near-black red with a wet highlight, like the game's blood in daylight.
        drop: materials.add(StandardMaterial { base_color: Color::srgb(0.16, 0.0, 0.0), perceptual_roughness: 0.25, reflectance: 0.4, ..default() }),
        mist: materials.add(StandardMaterial { base_color: Color::srgba(0.12, 0.0, 0.0, 0.22), alpha_mode: AlphaMode::Blend, unlit: true, ..default() }),
    });
}

#[allow(clippy::too_many_arguments)]
fn bleed(
    mut commands: Commands,
    time: Res<Time>,
    combat: Res<Combat>,
    vfx: Option<Res<VfxAssets>>,
    gore: Res<GoreAssets>,
    mut rng: ResMut<VfxRng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut decal_mats: Local<std::collections::HashMap<i64, Handle<StandardMaterial>>>,
    mut images: ResMut<Assets<Image>>,
    actors: Query<(&Actor, &Dummies)>,
    dummies: Query<&GlobalTransform>,
    stains: Query<(Entity, &Stain, &Transform)>,
    mut lib: Option<ResMut<crate::fxr::FxrLib>>,
) {
    let Some(vfx) = vfx else { return };
    let mut new_stains: Vec<(i64, Vec3)> = Vec::new();
    for (a, dm) in &actors {
        let Some(anim) = data_for(&combat, &a).anim(&a.anim) else { continue };
        if a.t < a.prev_t {
            continue;
        }
        let dummy = |id: i64| dm.0.get(&(id as i16)).and_then(|e| dummies.get(*e).ok());
        for e in anim.events.iter().filter(|e| e.ungated()) {
            match e.kind {
                96 => {
                    let Some(rate) = e.arg_i64("FFXID").filter(|id| !lib.as_mut().is_some_and(|l| l.has(*id))).and_then(blood_rate) else { continue };
                    // The part of this frame inside the event.
                    let (from, to) = (a.prev_t.max(e.start), a.t.min(e.end));
                    if to <= from {
                        continue;
                    }
                    let Some(g) = e.arg_i64("DummyPolyID").and_then(dummy) else {
                        log(|| format!("{} {:.2}: no dummy {:?} ({} dummies)", a.anim, a.t, e.arg_i64("DummyPolyID"), dm.0.len()));
                        continue;
                    };
                    let (pos, fwd) = (g.translation(), (g.rotation() * Vec3::Z).normalize_or(Vec3::Y));
                    // A gush that thins out over the event.
                    let u = ((from - e.start) / (e.end - e.start).max(1e-3)).clamp(0.0, 1.0);
                    log(|| format!("{} {:.2}: spray at {:.2} dir {:.2}", a.anim, a.t, pos, fwd));
                    let n = rate * (1.0 - u).powf(1.5) * (to - from) + rng.f();
                    let big = rate > 200.0;
                    for _ in 0..n as u32 {
                        let dir = rng.cone(fwd, if big { 0.12 } else { 0.6 });
                        let speed = if big { rng.range(2.5, 7.5) * (1.0 - 0.5 * u) } else { rng.range(0.3, 1.5) };
                        // Round drops (the unit ball, radius = width) stretched along their flight.
                        let width = rng.range(0.002, if big { 0.01 } else { 0.004 });
                        commands.spawn((
                            Mesh3d(vfx.ball.clone()),
                            MeshMaterial3d(gore.drop.clone()),
                            Transform::from_translation(pos).with_scale(Vec3::splat(width)),
                            Particle { vel: dir * speed, age: 0.0, life: rng.range(0.5, 1.3), width, stretch: 0.004, gravity: 9.8, drag: 0.4 },
                            bevy::light::NotShadowCaster,
                        ));
                    }
                    // The red haze around the jet.
                    if big && rng.f() < 45.0 * (to - from) {
                        let width = rng.range(0.05, 0.12);
                        commands.spawn((
                            Mesh3d(vfx.ball.clone()),
                            MeshMaterial3d(gore.mist.clone()),
                            Transform::from_translation(pos).with_scale(Vec3::splat(width)),
                            Particle { vel: rng.cone(fwd, 0.4) * rng.range(1.0, 3.0), age: 0.0, life: rng.range(0.25, 0.5), width, stretch: 0.0, gravity: 1.0, drag: 3.0 },
                            bevy::light::NotShadowCaster,
                        ));
                    }
                }
                138 => {
                    let started = (e.start > a.prev_t && e.start <= a.t) || (a.prev_t == 0.0 && e.start == 0.0 && a.t > 0.0 && a.t < 0.05);
                    let (Some(id), Some(g)) = (e.arg_i64("DecalParamID"), e.arg_i64("DummyPolyID").and_then(dummy)) else { continue };
                    if started {
                        log(|| format!("{} {:.2}: stain {id} at {:.2}", a.anim, a.t, g.translation()));
                        new_stains.push((id, g.translation()));
                    }
                }
                _ => {}
            }
        }
    }
    for (id, at) in new_stains {
        let row = combat.param("DecalParam", id);
        if row.is_null() {
            continue;
        }
        let f = |k: &str| row[k].as_f64().unwrap_or(0.0) as f32;
        // Straight down (pitchAngle -90) onto the floor (y = 0). The box reaches from nearDistance to
        // farDistance below the dummy and widens from nearSize to farSize. gap: read as the stain's
        // size at the floor's depth in the box (not traced in the exe).
        let depth = at.y;
        if depth < f("nearDistance") || depth > f("farDistance") {
            continue;
        }
        let k = (depth - f("nearDistance")) / (f("farDistance") - f("nearDistance")).max(1e-3);
        let size = (f("nearSize") + (f("farSize") - f("nearSize")) * k) * rng.range(f("randomSizeMin"), f("randomSizeMax")) / 100.0;
        // Thin out: at most distThinOutMaxNum stains of this row within distThinOutCheckDist.
        let near: Vec<(Entity, f32)> = stains.iter().filter(|(_, s, t)| s.id == id && t.translation.with_y(0.0).distance(at.with_y(0.0)) < f("distThinOutCheckDist")).map(|(e, s, _)| (e, s.born)).collect();
        let max_near = row["distThinOutMaxNum"].as_i64().unwrap_or(0) as usize;
        if row["bDistThinOutEnable"].as_i64() == Some(1) && max_near > 0 && near.len() >= max_near {
            if let Some((old, _)) = near.iter().min_by(|a, b| a.1.total_cmp(&b.1)) {
                commands.entity(*old).despawn();
            }
        }
        let all: Vec<(Entity, f32)> = stains.iter().map(|(e, s, _)| (e, s.born)).collect();
        if all.len() >= MAX_STAINS {
            if let Some((old, _)) = all.iter().min_by(|a, b| a.1.total_cmp(&b.1)) {
                commands.entity(*old).despawn();
            }
        }
        let mat = decal_mats
            .entry(id)
            .or_insert_with(|| {
                let c = |k: &str| row[k].as_f64().unwrap_or(255.0) as f32 / 255.0;
                materials.add(StandardMaterial {
                    base_color: Color::srgb(c("diffuseColorR"), c("diffuseColorG"), c("diffuseColorB")),
                    base_color_texture: stain_texture(id).map(|i| images.add(i)),
                    alpha_mode: AlphaMode::Blend,
                    // Wet: reflecColor 16/16/16 is a low, even specular. gap: the reflectance map
                    // dp009900000_r is not used.
                    perceptual_roughness: 0.3,
                    reflectance: 0.3,
                    ..default()
                })
            })
            .clone();
        let roll = rng.range(f("randomRollMin"), f("randomRollMax")).to_radians();
        // Each new stain a hair above the last, so overlapping ones do not flicker.
        let lift = 0.003 + 0.0002 * (time.elapsed_secs() % 10.0);
        commands.spawn((
            Mesh3d(gore.plane.clone()),
            MeshMaterial3d(mat),
            Transform::from_xyz(at.x, lift, at.z).with_rotation(Quat::from_rotation_y(roll)).with_scale(Vec3::new(size, 1.0, size)),
            Stain { id, born: time.elapsed_secs() },
            bevy::light::NotShadowCaster,
        ));
    }
}
