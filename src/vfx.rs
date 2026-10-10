//! Clash effects: sword-on-sword sparks, the flash and light pulse of a deflect, and a blood spray
//! on hits. combat.rs spawns a `Clash` request at the contact point with the game's effect ids for
//! it (`hit_sfx`: HitEffectSfx* params); fxr.rs plays those. Without them (not exported, or no FXR
//! extracted) the hand-made look stands in: deflect a burst of hot orange sparks and a white
//! flash; guard fewer, paler sparks; hit dark red droplets.

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Perfect deflect: the big burst.
    Deflect,
    /// Blocked (guard) or an enemy's deflect against Wolf.
    Guard,
    /// Flesh hit.
    Hit,
}

/// A request from combat: one clash at `pos`, sparks thrown around `normal` (toward the attacker).
#[derive(Component)]
pub struct Clash {
    pub normal: Vec3,
    pub kind: Kind,
    /// The game's effects for it (FFX ids, `hit_sfx`).
    pub sfx: Vec<i64>,
}

impl Clash {
    pub fn bundle(pos: Vec3, normal: Vec3, kind: Kind) -> impl Bundle {
        Self::bundle_fx(pos, normal, kind, Vec::new())
    }
    pub fn bundle_fx(pos: Vec3, normal: Vec3, kind: Kind, sfx: Vec<i64>) -> impl Bundle {
        (Clash { normal: normal.normalize_or(Vec3::Y), kind, sfx }, Transform::from_translation(pos))
    }
}

/// HitEffectSfxConcept(JustGuard)Param column groups in paramdef order, picked by the attack's
/// atkMaterial_forSfx (the same order as the sound tables' groups, sound::HIT_SE_GROUPS).
const SFX_GROUPS: [&str; 15] =
    ["Iron", "Fire", "Wood", "Body", "Eclipse", "Energy", "None", "Dmy1", "Dmy2", "Dmy3", "Maggot", "Wax", "FireFlame", "EclipseGas", "EnergyStrong"];

/// The game's effects for a clash. For each material row (deflect / guard: the attack's
/// defSfxMaterial1/2; hit: the defender's materialSfx1/2, Wolf's protector defenseMaterialSfx1/2)
/// the concept table (deflect: HitEffectSfxConceptJustGuardParam, e.g. 100 "Weapon iron" ->
/// atkIron_1 520 "Jasuga sparks"; guard / hit: HitEffectSfxConceptParam) gives up to two concepts
/// (atk<group>_1 / _2), and HitEffectSfxParam[concept] at the attack's type (Slash / Blow / Thrust)
/// and size (atkPow_forSfx: S M L LL LLL) the FFX id: the General's sword deflected -> 252001,
/// guarded -> 201002, on his armour -> 201001 + 229001 ("meat under armour").
/// gap: the exe's SFX path is not traced; a deflect falls back to HitEffectSfxConceptParam when the
/// JustGuard table has nothing, as the traced sound path does (sound::guard_sounds).
pub fn hit_sfx(combat: &crate::data::Combat, atk: &crate::data::Attack, kind: Kind, def_materials: [i64; 2]) -> Vec<i64> {
    let Some(group) = usize::try_from(atk.atk_material_sfx).ok().and_then(|g| SFX_GROUPS.get(g)) else { return Vec::new() };
    let col = format!(
        "{}_{}",
        match atk.atk_type {
            1 => "Blow",
            2 => "Thrust",
            _ => "Slash",
        },
        ["S", "M", "L", "LL", "LLL"][atk.atk_pow_sfx.clamp(0, 4) as usize]
    );
    let look = |table: &str, mats: [i64; 2]| -> Vec<i64> {
        let (Some(t), Some(sfx)) = (combat.params.get(table), combat.params.get("HitEffectSfxParam")) else { return Vec::new() };
        let mut out = Vec::new();
        for m in mats.into_iter().filter(|m| *m >= 0) {
            for n in 1..=2 {
                let concept = t.get(m.to_string()).and_then(|r| r[format!("atk{group}_{n}")].as_i64()).unwrap_or(0);
                if let Some(id) = (concept > 0).then(|| sfx.get(concept.to_string()).and_then(|r| r[&col].as_i64())).flatten().filter(|id| *id > 0) {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
        out
    };
    let guard = [atk.def_sfx_material1, atk.def_sfx_material2];
    match kind {
        Kind::Deflect => {
            let jg = look("HitEffectSfxConceptJustGuardParam", guard);
            if jg.is_empty() { look("HitEffectSfxConceptParam", guard) } else { jg }
        }
        Kind::Guard => look("HitEffectSfxConceptParam", guard),
        Kind::Hit => look("HitEffectSfxConceptParam", def_materials),
    }
}

/// The defender's two hit-effect materials: the enemy's NpcParam materialSfx1/2, Wolf's body
/// protector defenseMaterialSfx1/2 (146 / 106).
pub fn defender_sfx_materials(combat: &crate::data::Combat, side: crate::actor::Side) -> [i64; 2] {
    let (r, a, b) = match side {
        crate::actor::Side::Enemy => (combat.param("NpcParam", combat.foe.npc_row), "materialSfx1", "materialSfx2"),
        crate::actor::Side::Player => (combat.param("EquipParamProtector", 100000), "defenseMaterialSfx1", "defenseMaterialSfx2"),
    };
    [r[a].as_i64().unwrap_or(-1), r[b].as_i64().unwrap_or(-1)]
}

#[derive(Component)]
pub(crate) struct Particle {
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    pub width: f32,
    /// Streak length per m/s of speed.
    pub stretch: f32,
    pub gravity: f32,
    pub drag: f32,
}

#[derive(Component)]
struct Flash {
    age: f32,
    life: f32,
    size: f32,
}

#[derive(Component)]
struct FlashLight {
    age: f32,
    life: f32,
    peak: f32,
}

#[derive(Resource)]
pub(crate) struct VfxAssets {
    pub streak: Handle<Mesh>,
    pub ball: Handle<Mesh>,
    hot: Handle<StandardMaterial>,
    warm: Handle<StandardMaterial>,
    pale: Handle<StandardMaterial>,
    pub blood: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub(crate) struct VfxRng(u32);

impl VfxRng {
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 100_000) as f32 / 100_000.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    /// A direction within `spread` (0 = along `n`, 1 = a hemisphere) around `n`.
    pub fn cone(&mut self, n: Vec3, spread: f32) -> Vec3 {
        let (t, b) = n.any_orthonormal_pair();
        let phi = self.range(0.0, std::f32::consts::TAU);
        let cos_t = 1.0 - self.f() * spread;
        let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
        (n * cos_t + (t * phi.cos() + b * phi.sin()) * sin_t).normalize_or(n)
    }
}

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VfxRng(0x1234_5679)).add_systems(Startup, setup).add_systems(Update, (test_clashes, spawn_clashes, animate).chain());
    }
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Unlit draws base_color as is: over-bright linear colours (> 1) are what the bloom picks up.
    let glow = |c: Color, k: f32| StandardMaterial { base_color: Color::LinearRgba(c.to_linear() * k), unlit: true, ..default() };
    commands.insert_resource(VfxAssets {
        // A unit streak along +Z (scaled per particle), and a ball for flashes / droplets.
        streak: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ball: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
        hot: materials.add(glow(Color::srgb(1.0, 0.9, 0.6), 12.0)),
        warm: materials.add(glow(Color::srgb(1.0, 0.5, 0.12), 8.0)),
        pale: materials.add(glow(Color::srgb(0.85, 0.9, 1.0), 4.0)),
        blood: materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.01, 0.01), perceptual_roughness: 0.3, ..default() }),
    });
}

fn spawn_clashes(
    mut commands: Commands,
    assets: Res<VfxAssets>,
    mut rng: ResMut<VfxRng>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<(Entity, &Clash, &Transform)>,
    mut lib: Option<ResMut<crate::fxr::FxrLib>>,
) {
    for (e, c, tf) in &q {
        commands.entity(e).despawn();
        let pos = tf.translation;
        // The game's effects, +Z toward the attacker. gap: HitEffectSfxAngleParam's turn by the
        // attack's direction (atkDir_forSfx) is not applied.
        let game: Vec<i64> = c.sfx.iter().copied().filter(|id| lib.as_mut().is_some_and(|l| l.has(*id))).collect();
        if !game.is_empty() {
            let at = Transform::from_translation(pos).looking_to(-c.normal, Vec3::Y);
            for id in game {
                commands.spawn(crate::fxr::FxEffect::new(id, None, false, at));
            }
            continue;
        }
        // Sparks fly off the blades: mostly sideways around the clash normal and a bit up.
        let n = (c.normal + Vec3::Y * 0.35).normalize_or(Vec3::Y);
        let (count, speed, life, spread) = match c.kind {
            Kind::Deflect => (46, (5.0, 12.0), (0.18, 0.55), 1.2),
            Kind::Guard => (16, (3.0, 7.0), (0.12, 0.35), 1.0),
            Kind::Hit => (30, (1.5, 5.0), (0.3, 0.6), 0.7),
        };
        for i in 0..count {
            let dir = rng.cone(n, spread);
            let v = dir * rng.range(speed.0, speed.1);
            let (mat, width, stretch, gravity, drag) = match c.kind {
                Kind::Deflect => (if i % 3 == 0 { assets.hot.clone() } else { assets.warm.clone() }, rng.range(0.006, 0.014), 0.018, 9.8, 1.5),
                Kind::Guard => (assets.pale.clone(), rng.range(0.004, 0.009), 0.014, 9.8, 2.0),
                Kind::Hit => (assets.blood.clone(), rng.range(0.006, 0.016), 0.012, 9.8, 0.5),
            };
            commands.spawn((
                Mesh3d(assets.streak.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(pos).with_scale(Vec3::splat(width)),
                Particle { vel: v, age: 0.0, life: rng.range(life.0, life.1), width, stretch, gravity, drag },
                bevy::light::NotShadowCaster,
            ));
        }
        if c.kind == Kind::Hit {
            continue;
        }
        // The white-hot flash at the contact and its light on the fighters.
        let (size, peak, color) = match c.kind {
            Kind::Deflect => (0.11, 600_000.0, Color::srgb(1.0, 0.7, 0.35)),
            _ => (0.06, 120_000.0, Color::srgb(0.8, 0.85, 1.0)),
        };
        let flash_mat = materials.add(StandardMaterial {
            base_color: Color::LinearRgba(LinearRgba::rgb(1.0, 0.85, 0.6) * if c.kind == Kind::Deflect { 14.0 } else { 5.0 }),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        commands.spawn((
            Mesh3d(assets.ball.clone()),
            MeshMaterial3d(flash_mat),
            Transform::from_translation(pos).with_scale(Vec3::splat(size * 0.3)),
            Flash { age: 0.0, life: 0.07, size },
            bevy::light::NotShadowCaster,
        ));
        commands.spawn((
            PointLight { color, intensity: peak, range: 6.0, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(pos),
            FlashLight { age: 0.0, life: 0.12, peak },
        ));
    }
}

fn animate(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform), (Without<Flash>, Without<FlashLight>)>,
    mut flashes: Query<(Entity, &mut Flash, &mut Transform, &MeshMaterial3d<StandardMaterial>), Without<FlashLight>>,
    mut lights: Query<(Entity, &mut FlashLight, &mut PointLight)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut particles {
        p.age += dt;
        if p.age >= p.life || tf.translation.y < 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        let damp = (1.0 - p.drag * dt).max(0.0);
        p.vel *= damp;
        tf.translation += p.vel * dt;
        // Shrinks as it cools; streaks stretch along their velocity.
        let k = 1.0 - p.age / p.life;
        let speed = p.vel.length();
        let len = (speed * p.stretch).max(p.width) * k.sqrt();
        if speed > 1e-3 {
            tf.rotation = Quat::from_rotation_arc(Vec3::Z, p.vel / speed);
        }
        tf.scale = Vec3::new(p.width * k, p.width * k, if p.stretch > 0.005 { len } else { p.width * k });
    }
    for (e, mut f, mut tf, mat) in &mut flashes {
        f.age += dt;
        if f.age >= f.life {
            materials.remove(&mat.0);
            commands.entity(e).despawn();
            continue;
        }
        let k = f.age / f.life;
        tf.scale = Vec3::splat(f.size * (0.3 + 0.7 * k.sqrt()));
        if let Some(mut m) = materials.get_mut(&mat.0) {
            let c = m.base_color.to_linear();
            m.base_color = Color::LinearRgba(c * (1.0 - k).max(0.0).powf(0.25) / (1.0 - (k - dt / f.life).max(0.0)).max(1e-3).powf(0.25));
        }
    }
    for (e, mut l, mut light) in &mut lights {
        l.age += dt;
        if l.age >= l.life {
            commands.entity(e).despawn();
            continue;
        }
        light.intensity = l.peak * (1.0 - l.age / l.life).powi(2);
    }
}

/// SHINOBI_VFX_TEST=1: a deflect / guard / hit in turn beside Wolf every 0.8 s (looks).
/// SHINOBI_VFX_SHOTS=<dir>: screenshots 2 / 5 / 10 frames after the first clashes.
fn test_clashes(mut commands: Commands, time: Res<Time>, mut t: Local<f32>, mut n: Local<u32>, mut since: Local<u32>, wolf: Query<&GlobalTransform, With<crate::player::Player>>) {
    if std::env::var("SHINOBI_VFX_TEST").is_err() {
        return;
    }
    *t += time.delta_secs();
    *since += 1;
    if let Ok(dir) = std::env::var("SHINOBI_VFX_SHOTS") {
        if *n >= 1 && *n <= 4 && [2, 5, 10].contains(&*since) {
            let path = format!("{dir}/c{}_{:02}.png", *n, *since);
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
    }
    if *t < 0.8 || time.elapsed_secs() < 6.0 {
        return;
    }
    *t = 0.0;
    *n += 1;
    *since = 0;
    let Ok(w) = wolf.single() else { return };
    let right = (w.rotation() * Vec3::X).with_y(0.0).normalize_or(Vec3::X);
    let kind = [Kind::Hit, Kind::Deflect, Kind::Guard, Kind::Deflect][(*n % 4) as usize];
    commands.spawn(Clash::bundle(w.translation() + right * 0.7 + Vec3::Y * 0.6, right, kind));
}
