//! Blood stains on the floor, from the game's `DecalParam` rows.
//!
//! Two sources feed them, both traced by FwLo's RustSekiro (MIT, see `CREDITS.md`; its
//! `src/gore.rs` and `docs/kb/visuals.md`):
//!
//! - TAE `DecalParamID_DummyPoly` (138) events, such as a deathblow's blood from the neck dummies:
//!   the stain lies on the floor under the dummy, sized by how far below it the floor is
//!   (`nearDistance` to `farDistance`, `nearSize` to `farSize`);
//! - a landed hit (`sekiro_sim::hit_fx`): the attack's own decals and the hit-blood row
//!   `600000000 + material * 1000 + base + size`. These project from the hit along the blow,
//!   tilted by `pitchAngle`, and grow with the distance the ray goes.
//!
//! The row's mask (`dp<id>_m`, BC4) and diffuse (`dp<id>_a`, BC1) come from `other/decaltex.tpf`;
//! we decode both on the CPU into one RGBA image tinted by `diffuseColor`. The deferred decal's
//! blend is not traced: the mask's alpha is lifted (`^0.55`, FwLo's match against recordings).
//! Stains are flat quads on the floor, so walls take none.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sekiro_sim::hit_fx::DecalRow;

use crate::fxr::FxrPlayer;

/// Stains alive at once (the game keeps them under a budget that is not traced).
const MAX_STAINS: usize = 48;

/// A stain to put on the floor.
#[derive(Message, Clone, Copy, Debug)]
pub struct Stain {
    pub id: i32,
    /// The dummy or the contact point.
    pub at: Vec3,
    /// Floor height under `at` (the sim's ground).
    pub floor: f32,
    /// The blow's horizontal direction (attacker to defender) for a hit's projected stains.
    pub blow: Option<Vec3>,
}

#[derive(Message)]
pub struct ClearStains;

#[derive(Component)]
struct Placed {
    id: i32,
    born: f32,
}

#[derive(Resource)]
struct DecalAssets {
    plane: Handle<Mesh>,
    textures: PathBuf,
    materials: HashMap<i32, Option<Handle<StandardMaterial>>>,
    /// Deterministic noise for sizes and rolls.
    rng: u32,
}

impl DecalAssets {
    fn next(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / 16_777_216.0
    }
}

pub struct DecalPlugin {
    pub cache: PathBuf,
}

impl Plugin for DecalPlugin {
    fn build(&self, app: &mut App) {
        let textures = self.cache.join("raw/other/decaltex.tpf");
        app.add_message::<Stain>()
            .add_message::<ClearStains>()
            .insert_resource(PendingTextures(textures))
            .add_systems(Startup, setup)
            .add_systems(Update, place);
    }
}

#[derive(Resource)]
struct PendingTextures(PathBuf);

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, textures: Res<PendingTextures>) {
    commands.insert_resource(DecalAssets {
        plane: meshes.add(Plane3d::default().mesh().size(1.0, 1.0)),
        textures: textures.0.clone(),
        materials: HashMap::new(),
        rng: 0x5eed_1234,
    });
}

/// The stain's image: the diffuse's colour with the mask as alpha.
#[allow(clippy::chunks_exact_to_as_chunks)]
fn stain_image(path: &std::path::Path, row: &DecalRow) -> Option<Image> {
    let data = std::fs::read(path).ok()?;
    let set = sekiro_formats::tpf::parse(&data).ok()?;
    let find = |id: i32, suffix: char| {
        let name = format!("dp{id:09}_{suffix}");
        set.iter().find(|t| t.name.eq_ignore_ascii_case(&name))
    };
    let decode = |t: &sekiro_formats::tpf::TpfTexture| {
        let header = t.header().ok()?;
        Some((header.width, header.height, header.decode_rgba8(t.dds)?))
    };
    let mask = decode(find(row.mask_texture, 'm')?)?;
    let diffuse = find(row.diffuse_texture, 'a').and_then(decode);
    let (w, h) = (mask.0, mask.1);
    let mut rgba = vec![255u8; (w * h * 4) as usize];
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        if let Some((dw, dh, d)) = &diffuse {
            // Nearest texel of the diffuse (the two are normally the same size).
            let (x, y) = (i as u32 % w, i as u32 / w);
            let at = ((y * dh / h) * dw + (x * dw / w)) as usize * 4;
            px[..3].copy_from_slice(&d[at..at + 3]);
        }
        let m = f32::from(mask.2[i * 4]) / 255.0;
        px[3] = (m.powf(0.55) * 255.0) as u8;
    }
    Some(Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

/// The bridge from one simulation step to stains: hits (blood from the hit tables) and TAE decal
/// events, with the floor under each from the simulation's ground. Also plays the hits' effects.
pub fn forward(
    fx: &mut FxrPlayer,
    duel: &sekiro_sim::duel::Duel,
    report: &sekiro_sim::duel::DuelReport,
    out: &mut MessageWriter<Stain>,
) {
    let floor = |at: Vec3| {
        duel.player
            .ground
            .floor(at.x, at.z, at.y + 0.5)
            .unwrap_or(duel.player.body.position[1])
    };
    for (id, at, dir) in fx.hits(duel, report) {
        let at = Vec3::from_array(at);
        out.write(Stain {
            id,
            at,
            floor: floor(at),
            blow: Some(Vec3::new(dir[0], 0.0, dir[1])),
        });
    }
    for (id, at) in fx.event_stains(duel) {
        let at = Vec3::from_array(at);
        out.write(Stain {
            id,
            at,
            floor: floor(at),
            blow: None,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn place(
    mut commands: Commands,
    time: Res<Time>,
    player: Res<FxrPlayer>,
    mut assets: ResMut<DecalAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut requests: MessageReader<Stain>,
    mut resets: MessageReader<ClearStains>,
    stains: Query<(Entity, &Placed, &Transform)>,
) {
    let legacy = std::env::var_os("PF_LEGACY_DECALS").is_some();
    if resets.read().next().is_some() && !legacy {
        resets.clear();
        for (entity, _, _) in &stains {
            commands.entity(entity).despawn();
        }
        requests.clear();
        return;
    }
    // Commands are deferred: the query never includes this frame's new stains.
    // Track placements and evictions immediately so a burst obeys both budgets.
    let mut live: Vec<_> = stains
        .iter()
        .map(|(e, p, t)| (e, p.id, p.born, t.translation))
        .collect();
    for r in requests.read() {
        let Some(row) = player.decal(r.id) else {
            continue;
        };
        let Some((at, k)) = locate(row, r) else {
            continue;
        };
        let size = (row.near_size + (row.far_size - row.near_size) * k)
            * (row.random_size[0] as f32
                + assets.next() * (row.random_size[1] - row.random_size[0]) as f32)
            / 100.0;
        let path = assets.textures.clone();
        let material = assets
            .materials
            .entry(r.id)
            .or_insert_with(|| {
                let image = stain_image(&path, row)?;
                let c = |v: u8| f32::from(v) / 255.0;
                Some(materials.add(StandardMaterial {
                    base_color: Color::srgb(
                        c(row.diffuse_color[0]),
                        c(row.diffuse_color[1]),
                        c(row.diffuse_color[2]),
                    ),
                    base_color_texture: Some(images.add(image)),
                    alpha_mode: AlphaMode::Blend,
                    // Wet: a low, even specular.
                    perceptual_roughness: 0.3,
                    reflectance: 0.3,
                    ..default()
                }))
            })
            .clone();
        let Some(material) = material else {
            continue;
        };
        if row.thin_out && row.thin_out_max > 0 {
            let near: Vec<_> = live
                .iter()
                .enumerate()
                .filter(|(_, (_, id, _, pos))| {
                    *id == r.id && pos.xz().distance(at.xz()) < row.thin_out_distance
                })
                .collect();
            if near.len() >= row.thin_out_max as usize
                && let Some((index, _)) = near.iter().min_by(|(_, a), (_, b)| a.2.total_cmp(&b.2))
            {
                let entity = if legacy {
                    live[*index].0
                } else {
                    live.remove(*index).0
                };
                commands.entity(entity).despawn();
            }
        }
        if live.len() >= MAX_STAINS
            && let Some((index, _)) = live
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.2.total_cmp(&b.2))
        {
            let entity = if legacy {
                live[index].0
            } else {
                live.remove(index).0
            };
            commands.entity(entity).despawn();
        }
        let roll = (row.random_roll[0] + assets.next() * (row.random_roll[1] - row.random_roll[0]))
            .to_radians();
        // Each new stain a hair above the last, so overlapping ones do not flicker.
        let lift = 0.004 + 0.0003 * (time.elapsed_secs() % 10.0);
        let entity = commands
            .spawn((
                Mesh3d(assets.plane.clone()),
                MeshMaterial3d(material),
                Transform::from_xyz(at.x, r.floor + lift, at.z)
                    .with_rotation(Quat::from_rotation_y(roll))
                    .with_scale(Vec3::new(size, 1.0, size)),
                Placed {
                    id: r.id,
                    born: time.elapsed_secs(),
                },
                bevy::light::NotShadowCaster,
            ))
            .id();
        if !legacy {
            live.push((entity, r.id, time.elapsed_secs(), at));
        }
    }
}

/// Where the stain lands and where in the row's size ramp it falls (0 near, 1 far), or `None`
/// when the floor is out of its reach.
fn locate(row: &DecalRow, r: &Stain) -> Option<(Vec3, f32)> {
    let span = (row.far_distance - row.near_distance).max(1e-3);
    if let (Some(blow), 2) = (r.blow, row.appear_dir_type) {
        // A ray from the hit along the blow, `pitchAngle` up (+) or down (-), up to `farDistance`.
        let pitch = row.pitch_angle.to_radians();
        let ray = (blow.with_y(0.0).normalize_or(Vec3::X) * pitch.cos() + Vec3::Y * pitch.sin())
            .normalize();
        if ray.y > -1e-3 {
            return None;
        }
        let d = (r.at.y - r.floor) / -ray.y;
        if d > row.far_distance || d < 0.0 {
            return None;
        }
        let k = ((d - row.near_distance) / span).clamp(0.0, 1.0);
        return Some((r.at + ray * d, k));
    }
    // Straight down onto the floor: the box reaches nearDistance to farDistance below the dummy.
    let depth = r.at.y - r.floor;
    if depth < row.near_distance || depth > row.far_distance {
        return None;
    }
    Some((r.at, (depth - row.near_distance) / span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deathblow_bursts_obey_budget_and_respawn_clears_stains_for_twenty_fights() {
        let cache = sekiro_sim::cache::root();
        let duel = sekiro_sim::duel::Duel::load_with(
            &cache,
            6.,
            sekiro_sim::arena::ArenaSpec::by_name("genichiro")
                .unwrap()
                .enemy_spec(),
        )
        .unwrap();
        let mut fx = FxrPlayer::new(cache.clone());
        fx.observe(&duel);
        assert!(fx.decal(50010).is_some());
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Image>()
            .insert_resource(fx)
            .add_plugins(DecalPlugin { cache });
        app.update();
        let mut count = app.world_mut().query_filtered::<Entity, With<Placed>>();
        for fight in 0..20 {
            for _ in 0..8 {
                for _ in 0..16 {
                    app.world_mut().write_message(Stain {
                        id: 50010,
                        at: Vec3::Y,
                        floor: 0.,
                        blow: None,
                    });
                }
                app.update();
            }
            let placed = count.iter(app.world()).count();
            assert!(
                placed > 0 && placed <= MAX_STAINS,
                "fight {fight}: {placed} stains exceed {MAX_STAINS}"
            );
            app.world_mut().write_message(ClearStains);
            app.update();
            assert_eq!(
                count.iter(app.world()).count(),
                0,
                "fight {fight}: stains survived respawn"
            );
        }
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 1);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
    }

    fn row() -> DecalRow {
        DecalRow {
            near_distance: -0.25,
            far_distance: 5.0,
            near_size: 0.5,
            far_size: 3.5,
            appear_dir_type: 0,
            ..default()
        }
    }

    #[test]
    fn a_stain_under_a_dummy_scales_with_the_drop_and_is_out_of_reach_beyond_far() {
        let r = Stain {
            id: 1,
            at: Vec3::new(1.0, 1.7, 2.0),
            floor: 0.0,
            blow: None,
        };
        let (at, k) = locate(&row(), &r).unwrap();
        assert_eq!(at, r.at);
        assert!((k - (1.7 + 0.25) / 5.25).abs() < 1e-5);
        let high = Stain {
            at: Vec3::new(0.0, 6.0, 0.0),
            ..r
        };
        assert!(locate(&row(), &high).is_none());
    }

    #[test]
    fn a_hit_projects_along_the_blow_onto_the_floor() {
        let mut row = row();
        row.appear_dir_type = 2;
        row.pitch_angle = -45.0;
        row.near_distance = 0.0;
        row.far_distance = 10.0;
        let r = Stain {
            id: 1,
            at: Vec3::new(0.0, 1.0, 0.0),
            floor: 0.0,
            blow: Some(Vec3::X),
        };
        let (at, _) = locate(&row, &r).unwrap();
        assert!((at.x - 1.0).abs() < 1e-4 && at.y.abs() < 1e-4);
        // Upward pitches never reach the floor.
        row.pitch_angle = 40.0;
        assert!(locate(&row, &r).is_none());
    }
}
