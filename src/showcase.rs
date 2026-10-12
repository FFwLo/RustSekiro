//! Scripted Wolf for showcase recordings: `SHINOBI_SHOWCASE=fight` plays Wolf's inputs so a take
//! can be recorded without a person at the keyboard and reshot the same way after a fix.
//!
//! - fight: lock on to the nearest enemy, close in to its NpcParam hitRadius + 1.6 m, attack in
//!   bursts of 2-4 slashes while no enemy swing is due within 0.7 s, deflect every swing (guard
//!   pressed 0.1 s before the AttackBehavior event of the enemy's current anim starts, inside the
//!   0.2 s first-press deflect window), take the deathblow when the target's posture breaks, the
//!   equipped combat art every 14 s and the prosthetic every 11 s.
//! - orbit: no lock, the camera turns slowly round Wolf (scenery shots).
//!
//! `SHINOBI_SHOWCASE_SEED=<n>` varies the burst lengths (default 1); god mode and no posture
//! loss are switched on so a take never ends in a death screen. Nothing here is game logic: it
//! only writes PadInput / LockOn after the keyboard is read, like a player would.

use bevy::prelude::*;

use crate::actor::{data_for, Actor};
use crate::camera::{LockOn, OrbitCamera};
use crate::data::Combat;
use crate::enemy::Enemy;
use crate::player::{Action, PadInput, Player};

/// Seconds before the first input: models, textures and the map finish loading.
const WARMUP: f32 = 4.0;
/// Guard is pressed this long before an enemy hit window opens.
const DEFLECT_LEAD: f32 = 0.1;
/// No new attack starts while an enemy swing is due within this time.
const THREAT: f32 = 0.7;

pub struct ShowcasePlugin;

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        let Ok(mode) = std::env::var("SHINOBI_SHOWCASE") else { return };
        let seed = std::env::var("SHINOBI_SHOWCASE_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1u64);
        app.insert_resource(Director { mode, rng: seed.max(1), next_art: WARMUP + 6.0, next_tool: WARMUP + 9.0, ..default() })
            .add_systems(Update, direct.after(crate::player::read_input));
    }
}

#[derive(Resource, Default)]
struct Director {
    mode: String,
    t: f32,
    rng: u64,
    guard_until: f32,
    next_attack: f32,
    burst_left: u32,
    back_until: f32,
    next_art: f32,
    next_tool: f32,
    /// Enemy swings already answered: (enemy, anim, hit start in ms).
    answered: Vec<(Entity, String, i32)>,
}

impl Director {
    /// xorshift64: the same seed gives the same take.
    fn roll(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n as u64) as u32
    }
}

#[allow(clippy::too_many_arguments)]
fn direct(
    time: Res<Time>,
    combat: Res<Combat>,
    mut d: ResMut<Director>,
    mut pad: ResMut<PadInput>,
    mut lock: ResMut<LockOn>,
    mut menu: ResMut<crate::debug_menu::DebugMenu>,
    mut cams: Query<&mut OrbitCamera>,
    wolf: Single<(&Actor, &Transform), With<Player>>,
    enemies: Query<(Entity, &Actor, &Transform, &Enemy), Without<Player>>,
) {
    let dt = time.delta_secs();
    d.t += dt;
    menu.god = true;
    menu.no_posture = true;
    if d.t < WARMUP {
        return;
    }
    let (_, wtf) = *wolf;
    let at = wtf.translation;
    if d.mode == "orbit" {
        lock.target = None;
        for mut c in &mut cams {
            c.yaw += dt * 0.12;
        }
        return;
    }
    // The target: the current lock while it lives, else the nearest living enemy.
    let alive = |e: Entity| enemies.get(e).is_ok_and(|(_, _, _, en)| !en.is_dead());
    if !lock.target.is_some_and(alive) {
        lock.target = enemies.iter().filter(|(_, _, _, en)| !en.is_dead()).min_by(|a, b| a.2.translation.distance(at).total_cmp(&b.2.translation.distance(at))).map(|(e, ..)| e);
    }
    let Some(target) = lock.target else { return };
    let Ok((_, ta, ttf, _)) = enemies.get(target) else { return };
    let dist = (ttf.translation - at).with_y(0.0).length();

    // Deflect: every living enemy within 10 m whose current anim opens a hit window soon.
    let now = d.t;
    let mut threat = false;
    let mut press_guard = false;
    let mut answered = std::mem::take(&mut d.answered);
    for (e, a, tf, en) in &enemies {
        if en.is_dead() || tf.translation.distance(at) > 10.0 {
            continue;
        }
        for (ev, _, _) in data_for(&combat, a).attack_windows(&a.anim) {
            let lead = ev.start - a.t;
            if lead > 0.0 && lead <= THREAT {
                threat = true;
            }
            let key = (e, a.anim.clone(), (ev.start * 1000.0) as i32);
            if lead > -dt && lead <= DEFLECT_LEAD + dt && !answered.contains(&key) {
                answered.push(key);
                press_guard = true;
            }
        }
    }
    answered.retain(|(e, anim, _)| enemies.get(*e).is_ok_and(|(_, a, _, _)| &a.anim == anim));
    d.answered = answered;
    if press_guard {
        pad.press(Action::Guard);
        d.guard_until = now + 0.22;
        d.burst_left = 0;
    }
    pad.guard_held = now < d.guard_until;

    // Footwork: keep to the target's reach plus a step, back off now and then.
    let radius = combat.npc(ta)["hitRadius"].as_f64().unwrap_or(0.5) as f32;
    let want = radius + 1.6;
    pad.stick = if now < d.back_until {
        Vec2::new(0.0, -1.0)
    } else if dist > want + 0.4 {
        Vec2::new(0.0, 1.0)
    } else if dist < want - 0.8 {
        Vec2::new(0.0, -0.6)
    } else {
        Vec2::new((now * 0.6).sin() * 0.4, 0.0)
    };

    // The deathblow: Attack while the target's posture is broken.
    if ta.posture_broken() && dist < want + 1.0 && now >= d.next_attack {
        pad.press(Action::Attack);
        d.next_attack = now + 0.3;
        return;
    }
    if threat || dist > want + 0.8 || pad.guard_held {
        return;
    }
    if now >= d.next_art {
        pad.press(Action::CombatArt);
        d.next_art = now + 14.0;
        d.next_attack = now + 1.6;
        return;
    }
    if now >= d.next_tool {
        pad.press(Action::Prosthetic);
        d.next_tool = now + 11.0;
        d.next_attack = now + 1.0;
        return;
    }
    if now >= d.next_attack {
        if d.burst_left == 0 {
            d.burst_left = 2 + d.roll(3);
        }
        pad.press(Action::Attack);
        d.burst_left -= 1;
        if d.burst_left == 0 {
            d.next_attack = now + 0.9 + d.roll(6) as f32 * 0.15;
            if d.roll(3) == 0 {
                d.back_until = now + 0.45;
            }
        } else {
            d.next_attack = now + 0.38;
        }
    }
}
