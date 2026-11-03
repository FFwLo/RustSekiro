//! Stand-in looks for bullets and their areas: the game draws them with FXR particle effects,
//! which are not extracted yet, so each bullet gets a simple shape sized from the simulation
//! (`sekiro_sim::bullets`), coloured by what it does ([`BulletLook`]):
//!
//! - shuriken: a small silver ball;
//! - a thrown or lit firecracker, poison ball or powder: a small glowing orange ball;
//! - a firecracker's bang: a bright yellow-white sphere of its hit radius, flashing out;
//! - flames (firecracker, explosive slash): a faint orange sphere of their radius;
//! - poison floor: a flat purple disc of its radius on the ground, for its 18 s;
//! - smoke screen: a grey sphere of the dazzle's 5 m that fades over a second (the dazzle itself
//!   lasts 0.06 s);
//! - recovery-sealing powder: a violet sphere growing with the cloud.
//!
//! Areas that end are faded out over a short time so a one-frame bang can still be seen; the
//! rules only ever use the simulation's sizes and times.
//!
//! The same pool draws the puff of a warp ([`BulletFx::puff`]): a grey ball where a character
//! fades out and where it comes back (the game's FFX 650680 / 650682).

use std::collections::HashMap;

use bevy::prelude::*;
use sekiro_sim::bullets::{Bullet, BulletLook};

/// Markers in the pool (the Owl's busiest tool, the firecracker, has 5 carriers, 5 bangs and
/// 10 flames at once).
const MARKERS: usize = 64;

/// One marker of the pool, by index.
#[derive(Component)]
pub struct Marker(usize);

/// An area still shown after its bullet ended.
#[derive(Clone, Copy)]
struct Fading {
    look: BulletLook,
    position: Vec3,
    radius: f32,
    /// Seconds since the bullet ended, and the seconds the fade takes.
    t: f32,
    length: f32,
}

/// The pool's materials (one per marker, so each can fade on its own) and what is fading.
#[derive(Resource, Default)]
pub struct BulletFx {
    materials: Vec<Handle<StandardMaterial>>,
    /// Last frame's bullets: serial -> (look, position, radius).
    last: HashMap<u64, (BulletLook, Vec3, f32)>,
    fading: Vec<Fading>,
}

/// Radius of a warp puff, metres, and how long it takes to fade, seconds (our choice).
const PUFF_RADIUS: f32 = 0.9;
const PUFF_SECONDS: f32 = 0.6;

impl BulletFx {
    /// Shows a warp puff at `at`, fading out over [`PUFF_SECONDS`].
    pub fn puff(&mut self, at: Vec3) {
        self.fading.push(Fading {
            look: BulletLook::Smoke,
            position: at,
            radius: PUFF_RADIUS,
            t: 0.0,
            length: PUFF_SECONDS,
        });
    }
}

/// How long an area stays visible after its bullet ended, seconds.
fn fade_length(look: BulletLook) -> f32 {
    match look {
        BulletLook::Blast => 0.35,
        BulletLook::Flame => 0.25,
        BulletLook::Smoke => 1.2,
        BulletLook::Seal | BulletLook::Poison => 0.4,
        _ => 0.0,
    }
}

/// Scale, colour (with alpha), emissive and whether the shape is lit, for a look.
fn style(look: BulletLook, radius: f32) -> Option<(Vec3, Color, LinearRgba, bool)> {
    let ball = |r: f32| Vec3::splat(r);
    Some(match look {
        BulletLook::Hidden | BulletLook::Lightning => return None,
        BulletLook::Missile => (
            ball(0.07),
            Color::srgb(0.78, 0.8, 0.85),
            LinearRgba::rgb(0.6, 0.6, 0.65),
            true,
        ),
        BulletLook::Carrier => (
            ball(0.11),
            Color::srgb(1.0, 0.55, 0.1),
            LinearRgba::rgb(4.0, 1.6, 0.2),
            true,
        ),
        BulletLook::Blast => (
            ball(radius),
            Color::srgba(1.0, 0.95, 0.6, 0.55),
            LinearRgba::rgb(6.0, 5.0, 2.0),
            false,
        ),
        BulletLook::Flame => (
            ball(radius),
            Color::srgba(1.0, 0.45, 0.1, 0.28),
            LinearRgba::rgb(2.0, 0.6, 0.1),
            false,
        ),
        BulletLook::Poison => (
            Vec3::new(radius, 0.08, radius),
            Color::srgba(0.45, 0.1, 0.6, 0.2),
            LinearRgba::rgb(0.3, 0.05, 0.45),
            false,
        ),
        BulletLook::Smoke => (
            ball(radius),
            Color::srgba(0.62, 0.62, 0.6, 0.55),
            LinearRgba::rgb(0.25, 0.25, 0.25),
            false,
        ),
        BulletLook::Seal => (
            ball(radius),
            Color::srgba(0.6, 0.25, 0.9, 0.3),
            LinearRgba::rgb(0.5, 0.15, 0.8),
            false,
        ),
    })
}

/// Spawns the pool (call once at startup).
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> BulletFx {
    let mesh = meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap());
    let mut fx = BulletFx::default();
    for i in 0..MARKERS {
        let material = materials.add(StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        fx.materials.push(material.clone());
        commands.spawn((
            Marker(i),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::IDENTITY,
            Visibility::Hidden,
            bevy::light::NotShadowCaster,
        ));
    }
    fx
}

/// What to draw this frame: live bullets, then fading areas (with an alpha factor).
fn shapes(fx: &mut BulletFx, live: &[Bullet], dt: f32) -> Vec<(BulletLook, Vec3, f32, f32)> {
    let mut now = HashMap::new();
    let mut out = Vec::new();
    for b in live {
        now.insert(b.serial, (b.look(), b.position, b.radius));
        out.push((b.look(), b.position, b.radius, 1.0));
    }
    for (serial, (look, position, radius)) in &fx.last {
        let length = fade_length(*look);
        if !now.contains_key(serial) && length > 0.0 {
            fx.fading.push(Fading {
                look: *look,
                position: *position,
                radius: *radius,
                t: 0.0,
                length,
            });
        }
    }
    fx.last = now;
    for f in &mut fx.fading {
        f.t += dt;
    }
    fx.fading.retain(|f| f.t < f.length);
    for f in &fx.fading {
        out.push((f.look, f.position, f.radius, 1.0 - f.t / f.length));
    }
    out
}

/// Puts the pool's markers on the bullets in flight and the fading areas, hides the rest.
pub fn sync(
    live: &[Bullet],
    dt: f32,
    fx: &mut BulletFx,
    materials: &mut Assets<StandardMaterial>,
    markers: &mut Query<(&Marker, &mut Transform, &mut Visibility)>,
) {
    let shapes: Vec<_> = shapes(fx, live, dt)
        .into_iter()
        .filter_map(|(look, at, r, alpha)| style(look, r).map(|s| (at, s, alpha)))
        .collect();
    for (m, mut tf, mut vis) in markers.iter_mut() {
        let Some(&(at, (scale, color, emissive, lit), alpha)) = shapes.get(m.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        tf.translation = at;
        tf.scale = scale;
        *vis = Visibility::Visible;
        let Some(id) = fx.materials.get(m.0).map(|h| h.id()) else {
            continue;
        };
        let base = color.to_srgba();
        let base_color = Color::srgba(base.red, base.green, base.blue, base.alpha * alpha);
        let emissive = emissive * alpha;
        let alpha_mode = if base.alpha < 1.0 || alpha < 1.0 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };
        // Only touch a material that changes (a mutable access re-uploads it).
        let same = materials.get(id).is_some_and(|mat| {
            mat.base_color == base_color
                && mat.emissive == emissive
                && mat.unlit == !lit
                && mat.alpha_mode == alpha_mode
        });
        if !same && let Some(mut mat) = materials.get_mut(id) {
            mat.base_color = base_color;
            mat.emissive = emissive;
            mat.unlit = !lit;
            mat.alpha_mode = alpha_mode;
        }
    }
}
