//! Cheap stand-ins for the game's lightning FXR: jagged bolts and electric sparks.
use bevy::prelude::*;
use sekiro_sim::{bullets::BulletLook, duel::Duel};

#[derive(Default)]
pub struct LightningFx {
    bolts: std::collections::HashMap<u64, (Vec3, Vec3, f32)>,
}

/// `covered`: serials of bullets the FXR player draws, which get no stand-in bolt.
pub fn draw(
    duel: &Duel,
    time: f32,
    dt: f32,
    fx: &mut LightningFx,
    gizmos: &mut Gizmos,
    covered: &[u64],
) {
    let blue = Color::srgb(0.3, 0.7, 1.0);
    let white = Color::srgb(1.0, 1.0, 1.0);
    for entry in fx.bolts.values_mut() {
        entry.2 += dt;
    }
    for b in &duel.bullets.live {
        if b.look() != BulletLook::Lightning || covered.contains(&b.serial) {
            continue;
        }
        let end = Vec3::from_array(b.position.to_array());
        let shooter = if b.shooter == 0 {
            &duel.player
        } else {
            &duel.enemy
        };
        let start = Vec3::from_array(shooter.body.position) + Vec3::Y * 1.5;
        fx.bolts.insert(b.serial, (start, end, 0.0));
    }
    fx.bolts.retain(|_, entry| entry.2 < 0.15);
    for &(start, end, _) in fx.bolts.values() {
        jagged(start, end, white, time, gizmos);
    }
    for (i, c) in [&duel.player, &duel.enemy].iter().enumerate() {
        let origin = Vec3::from_array(c.body.position) + Vec3::Y;
        let status = &duel.rules.fighters[i].status;
        let charged = status.running.iter().any(|r| (9490..=9499).contains(&r.id));
        let shock = status.has(9445);
        let clip = c.behavior.runtime.main_clip();
        let charging = clip.is_some_and(|a| {
            matches!(
                a.animation.as_str(),
                "a000_003039" | "a000_003042" | "a000_003043"
            ) && (0.8..1.9).contains(&a.time)
        });
        let reversing = clip.is_some_and(|a| {
            matches!(a.animation.as_str(), "a050_308910" | "a050_308911")
                && (0.1..0.35).contains(&a.time)
        });
        if charged || shock || charging {
            gizmos.sphere(origin, 0.55, blue);
            for k in 0..6 {
                let a = time * 18.0 + k as f32;
                let p = origin + Vec3::new(a.sin() * 0.6, (a * 1.7).cos() * 0.8, a.cos() * 0.6);
                jagged(p, p + Vec3::new(0.1, 0.25, -0.1), blue, a, gizmos);
            }
        }
        if reversing {
            gizmos.sphere(origin, 0.8, white);
            let target = Vec3::from_array(duel.enemy.body.position) + Vec3::Y;
            jagged(origin, target, white, time, gizmos);
        }
    }
}

fn jagged(start: Vec3, end: Vec3, color: Color, time: f32, gizmos: &mut Gizmos) {
    let mut prev = start;
    for k in 1..=8 {
        let offset = if k == 8 {
            Vec3::ZERO
        } else {
            Vec3::new((time * 40.0 + k as f32).sin(), (k as f32 * 3.0).cos(), 0.0) * 0.12
        };
        let next = start.lerp(end, k as f32 / 8.0) + offset;
        gizmos.line(prev, next, color);
        prev = next;
    }
}
