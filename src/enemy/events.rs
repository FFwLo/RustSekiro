//! Other characters a boss's phase rules bring into its fight (tools/boss_events.py "enable" /
//! "disable" / "warp_player" actions), for bosses without a map script (script.rs runs the
//! others' own events): e.g. Genichiro's 0 bars swap him for the Way of Tomoe (m11_01 11115820:
//! Change Character Enable State 1110800 off, 1110801 on, after the cutscene that warps Wolf to
//! 1112813). Also each enemy's event message (TAE 231) and Wolf's event warp.
//!
//! Positions come from the MSB in the boss part's frame; the arena is built around the boss's
//! spot (map.rs: the boss at (0, y, -4) facing +Z, X mirrored), so a character `at` (x, y, z)
//! of the boss stands at boss start + (-x, 0, z), facing yaw pi - its relative yaw.

use bevy::prelude::*;
use super::Enemy;
use crate::actor::Actor;
use crate::config::GameConfig;
use crate::data::Combat;
use crate::player::Player;

/// A character a boss event brought in (its death starts the fight over).
#[derive(Component)]
pub struct FromEvent;

/// Wolf's warp from a boss event (the cutscene's player warp point): position and yaw.
#[derive(Resource, Default)]
pub struct EventWarp(pub Option<(Vec3, f32)>);

/// Arena position and yaw of an MSB point `at` / `yaw` (degrees) relative to the boss.
pub(super) fn place(start: Vec3, at: &serde_json::Value, yaw: &serde_json::Value) -> (Vec3, f32) {
    let n = |i: usize| at.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let d = yaw.as_f64().unwrap_or(0.0) as f32;
    // The height difference counts on the boss's own arena (map.rs boss_arena: the ground
    // under it is the real one); on another floor it is dropped. gap: no navmesh either way.
    let y = if crate::map::on_boss_arena() { start.y + n(1) } else { start.y };
    (Vec3::new(start.x - n(0), y, start.z + n(2)), std::f32::consts::PI - d.to_radians())
}

/// Runs a boss's pending event requests: spawns the characters it enables and queues Wolf's
/// warp. Returns whether the boss itself is to be removed ("disable").
pub(super) fn apply_requests(commands: &mut Commands, combat: &Combat, config: &GameConfig, e: &mut Enemy, warp: &mut EventWarp) -> bool {
    for req in std::mem::take(&mut e.spawn_req) {
        let chr = req["chr"].as_str().unwrap_or("");
        let row = req["npc"].as_i64();
        let Some(kind) = combat.kind_index(chr, row) else {
            warn!("boss event: {chr} {row:?} is not loaded");
            continue;
        };
        let (pos, yaw) = place(e.start, &req["at"], &req["yaw"]);
        let rng = e.rng ^ (req["entity"].as_i64().unwrap_or(0) as u32);
        let id = super::spawn_one(commands, combat, config, kind, pos, yaw, e.aggressive, rng);
        commands.entity(id).insert(FromEvent);
        info!("boss event: {chr} enabled");
    }
    if let Some(w) = e.warp_req.take() {
        warp.0 = Some(place(e.start, &w["at"], &w["yaw"]));
    }
    std::mem::take(&mut e.disable_req)
}

/// Each enemy's event message: the latest TAE 231 its anim is in (see `Enemy::msg`).
pub(super) fn messages(combat: Res<Combat>, mut q: Query<(&mut Enemy, &Actor)>) {
    for (mut e, a) in &mut q {
        let d = combat.data_of(a);
        if let Some(ev) = d.events_at(&a.anim, a.t).filter(|ev| ev.kind == 231).max_by(|x, y| x.start.total_cmp(&y.start)) {
            e.msg = ev.arg_i64("EzStateRequestID");
            // A new one (it starts this step, or its anim just began).
            if (ev.start > a.prev_t || a.prev_t == 0.0) && ev.start <= a.t {
                e.msg_serial = e.msg_serial.wrapping_add(1);
            }
        }
    }
}

/// Wolf's event warp (after Genichiro's cutscene).
pub(super) fn apply_event_warp(mut warp: ResMut<EventWarp>, player: Single<(&mut Transform, &mut Actor), With<Player>>) {
    let Some((pos, yaw)) = warp.0.take() else { return };
    let (mut tf, mut a) = player.into_inner();
    tf.translation = Vec3::new(pos.x, tf.translation.y, pos.z);
    a.yaw = yaw;
    a.move_vel = Vec3::ZERO;
}
