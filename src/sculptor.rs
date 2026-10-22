//! The Sculptor on the Construct: the game's c7010 model, skeleton and clips. Talk opens the
//! existing fitting / upgrade menus; it never rests Wolf or changes his idol checkpoint.
//! This is a service NPC, not the story's ESD dialogue / gifts / sake progression.

use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::actor::{Actor, ActorSet, Side};
use crate::config::GameConfig;
use crate::data::{CharData, Combat};
use crate::idol::{Cmd, Phase};
use crate::player::{Player, CAPSULE_HALF_HEIGHT};

pub(crate) const CHR: &str = "c7010";
pub(crate) const NPC_ROW: i64 = 70100080;
/// Outside the replaceable enemy-kind indices: encounter changes must not turn him into a boss.
pub(crate) const KIND: usize = crate::data::NPC_KIND;
pub(crate) const POS: Vec3 = Vec3::new(-7.0, 0.0, 3.0);
/// His facing: toward the Construct's centre.
const YAW: f32 = -1.1659045; // atan2(-7, 3)
/// Wolf sits at c7010's dummy poly 100, 1 m in front of him facing him (event 11105721 warps
/// Wolf there; the user, 2026-10-18: Wolf clipped into him or sat too far for him to reach).
const SEAT: f32 = 1.0;
const REACH: f32 = 2.0;
const HEIGHT: f32 = 0.8;
const SIT_DOWN: &str = "Event711200";
const SITTING: &str = "Event711201";
const GET_UP: &str = "Event711202";
static PRESENT: AtomicBool = AtomicBool::new(false);

pub(crate) use crate::encounter::Npc as Sculptor;

fn in_reach(feet: Vec3) -> bool {
    (feet - POS).with_y(0.0).length() < REACH && (feet.y - POS.y).abs() < HEIGHT
}

pub(crate) fn near(feet: Vec3) -> bool {
    PRESENT.load(Ordering::Relaxed) && in_reach(feet)
}

/// Before the idol and traversal in player::decide. Finish sitting down before opening the
/// Workshop, loop while visiting, then finish getting up before returning control to Wolf.
pub(crate) fn tick(p: &mut Player, a: &mut Actor, data: &CharData, tf: &mut Transform, interact: bool, ended: bool, config: &GameConfig) -> bool {
    let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
    let mut ended = ended;
    if !p.seat.sculptor {
        if interact && crate::map::map_id(config).as_deref() == Some(crate::sandbox::CONSTRUCT_ID)
            && near(feet) && p.seat.idol.is_none() && !a.airborne && a.hp > 0.0
            && crate::player::is_free(&a.state) && a.play_state(data, SIT_DOWN)
        {
            let seat = POS + Vec3::new(-YAW.sin(), 0.0, -YAW.cos()) * SEAT;
            tf.translation = seat.with_y(feet.y) + Vec3::Y * CAPSULE_HALF_HEIGHT;
            a.yaw = YAW + std::f32::consts::PI;
            p.seat.sculptor = true;
            p.seat.phase = Phase::Kneeling;
            p.seat.cmd = None;
            p.seat.open = false;
            ended = false;
        } else {
            return false;
        }
    }
    if a.hp <= 0.0 || !in_reach(feet) || ![SIT_DOWN, SITTING, GET_UP].contains(&a.state.as_str()) {
        p.seat.sculptor = false;
        p.seat.open = false;
        p.seat.cmd = None;
        return false;
    }
    a.move_vel = Vec3::ZERO;
    if p.seat.cmd.take() == Some(Cmd::Leave) {
        a.play_state(data, GET_UP);
        p.seat.phase = Phase::Leaving;
    } else if p.seat.phase == Phase::Leaving {
        if ended {
            p.seat.sculptor = false;
            p.seat.phase = Phase::Menu;
            a.procedural("Locomotion");
        }
    } else if ended {
        if p.seat.phase == Phase::Kneeling {
            a.play_state(data, SITTING);
            p.seat.phase = Phase::Menu;
            p.seat.open = true;
        } else {
            a.t %= data.length(&a.anim).max(1e-3);
            a.prev_t = a.t;
        }
    }
    true
}

fn spawn(mut commands: Commands, combat: Res<Combat>) {
    PRESENT.store(false, Ordering::Relaxed);
    let Some(kind) = combat.sculptor.as_ref() else { return };
    if !crate::paths::extracted().join("model_c7010.bin").exists() || !crate::paths::extracted().join("anim_c7010.bin").exists() {
        warn!("Sculptor: missing model / animation export; rerun sekiro-extract npcs extracted c7010");
        return;
    }
    let mut actor = Actor::new(Side::Enemy, 1.0, 1.0, 0.0, -1);
    actor.kind = KIND;
    actor.yaw = YAW;
    actor.no_posture = true;
    actor.dmg_cut = 0.0;
    // The user wants ordinary sitting outside visits, without the map event's carving loop.
    let idle = std::env::var("SHINOBI_SCULPTOR_ANIM").unwrap_or_else(|_| "a000_000000".into());
    actor.play("SculptorIdle", &idle);
    if kind.data.anim(&idle).is_none() {
        warn!("Sculptor: missing clip {idle}");
    }
    commands.spawn((Sculptor, actor, Transform::from_translation(POS + Vec3::Y * CAPSULE_HALF_HEIGHT), Visibility::default(), Name::new("Sculptor")));
    PRESENT.store(true, Ordering::Relaxed);
}

/// While Wolf sits he works on the prosthetic: 20001 (event 11105721, with Wolf's 711200), the
/// 21003 loop (its prosthetic mask and tool sounds; 20000 is the dragonrot cough), then
/// 20002 (with 711202) back to ordinary sitting outside the menu (the user's preference).
fn visit_animation(a: &mut Actor, data: &CharData, visiting: bool) {
    let ended = a.t >= data.length(&a.anim);
    match (a.state.as_str(), visiting, ended) {
        ("SculptorIdle" | "SculptorTalkEnd", true, _) => a.play("SculptorTalkStart", "a000_020001"),
        ("SculptorTalkStart", true, true) => a.play("SculptorTalkLoop", "a000_021003"),
        ("SculptorTalkStart" | "SculptorTalkLoop", false, _) => a.play("SculptorTalkEnd", "a000_020002"),
        ("SculptorTalkEnd", false, true) => a.play("SculptorIdle", "a000_000000"),
        (_, _, true) => {
            a.t %= data.length(&a.anim).max(1e-3);
            a.prev_t = a.t;
        }
        _ => {}
    }
}

fn carving(combat: Res<Combat>, wolf: Query<&Player>, mut npc: Query<(&mut Actor, &mut Transform, &mut Visibility), With<Sculptor>>) {
    let visible = crate::place::loaded_map().as_deref().is_none_or(|id| id == crate::sandbox::CONSTRUCT_ID);
    let visiting = visible && wolf.iter().any(|p| p.seat.sculptor && p.seat.phase != Phase::Leaving);
    for (mut a, mut tf, mut visibility) in &mut npc {
        *visibility = if visible { Visibility::Inherited } else { Visibility::Hidden };
        PRESENT.store(visible, Ordering::Relaxed);
        tf.translation = POS + Vec3::Y * CAPSULE_HALF_HEIGHT;
        a.airborne = false;
        a.vel_y = 0.0;
        let data = combat.data_of(&a);
        visit_animation(&mut a, data, visiting);
        a.move_vel = Vec3::ZERO;
    }
}

/// Screenshot harness: open the real interaction from nearby, with no desktop input.
fn shot_visit(config: Res<GameConfig>, combat: Res<Combat>, mut done: Local<bool>, mut wolf: Query<(&mut Player, &mut Actor, &mut Transform)>) {
    if *done || std::env::var_os("SHINOBI_SCULPTOR_SHOT").is_none() || !PRESENT.load(Ordering::Relaxed) {
        return;
    }
    let Ok((mut p, mut a, mut tf)) = wolf.single_mut() else { return };
    *done = true;
    tf.translation = POS + Vec3::new(1.2, CAPSULE_HALF_HEIGHT, -0.5);
    a.procedural("Locomotion");
    if std::env::var("SHINOBI_SCULPTOR_SHOT").as_deref() == Ok("idle") {
        tf.translation = POS + Vec3::new(2.2, CAPSULE_HALF_HEIGHT, -0.9);
        a.yaw = YAW + std::f32::consts::PI;
        return;
    }
    tick(&mut p, &mut a, &combat.player, &mut tf, true, false, &config);
}

pub struct SculptorPlugin;

impl Plugin for SculptorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(FixedUpdate, carving.in_set(ActorSet::Decide))
            .add_systems(Update, shot_visit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talk_reach_checks_horizontal_distance_and_floor_height() {
        assert!(in_reach(POS + Vec3::X));
        assert!(!in_reach(POS + Vec3::X * 3.0));
        assert!(!in_reach(POS + Vec3::Y * 1.0));
        assert!(!crate::pickups::parkour_pickups().iter().any(|p| in_reach(p.pos)), "Talk must not compete with an item pickup");
    }

    #[test]
    fn encounter_changes_keep_the_sculptors_character_data() {
        let mut app = App::new();
        let mut config = GameConfig::load().unwrap();
        config.world.map = Some("construct".into());
        crate::encounter::set(crate::sandbox::map_hooks());
        app.insert_resource(config).add_plugins(crate::data::DataPlugin);
        let mut combat = app.world_mut().resource_mut::<Combat>();
        assert_eq!(combat.kind(KIND).foe.chr, CHR);
        assert_eq!(combat.kind(KIND).foe.npc_row, NPC_ROW);
        let clips = combat.kind(KIND).data.anims.len();
        combat.load_encounter("c7100", None, &[]);
        assert_eq!(combat.kind(KIND).foe.chr, CHR);
        assert_eq!(combat.kind(KIND).data.anims.len(), clips);
        assert!(!combat.kinds.iter().any(|k| k.foe.chr == CHR), "friendly NPC is not an encounter enemy");
    }

    #[test]
    fn leaving_the_workshop_does_not_rest_wolf_or_change_his_checkpoint() {
        let mut app = crate::sim_tests::app();
        app.update();
        let mut config = GameConfig::load().unwrap();
        config.world.map = Some("construct".into());
        app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
            p.seat.sculptor = true;
            p.seat.phase = Phase::Kneeling;
            p.seat.checkpoint = Some(3);
            p.gourd = 1;
            p.resurrections = 0;
            p.revive_locked = true;
            let mut a = Actor::new(Side::Player, 320.0, 120.0, 0.0, -1);
            a.hp = 73.0;
            assert!(a.play_state(&combat.player, SIT_DOWN));
            let mut tf = Transform::from_translation(POS + Vec3::new(1.0, CAPSULE_HALF_HEIGHT, 0.0));
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, false, &config));
            assert!(!p.seat.open, "the menu waits for Wolf to sit down");
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, true, &config));
            assert_eq!(a.state, SITTING);
            assert_eq!(p.seat.phase, Phase::Menu);
            assert!(p.seat.open);
            a.t = combat.player.length(&a.anim) + 0.1;
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, true, &config));
            assert!(a.t < combat.player.length(&a.anim), "the seated loop keeps playing");
            assert_eq!((a.hp, p.gourd, p.resurrections, p.seat.checkpoint), (73.0, 1, 0, Some(3)));
            assert!(p.revive_locked && !p.seat.respawn && !p.refill_items);
            p.seat.cmd = Some(Cmd::Leave);
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, false, &config));
            assert_eq!(a.state, GET_UP);
            assert!(p.seat.sculptor, "Wolf cannot move until he finishes getting up");
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, true, &config));
            assert!(!p.seat.sculptor);
            assert_eq!(a.state, "Locomotion");
            assert_eq!(p.seat.checkpoint, Some(3));
        });
    }

    #[test]
    fn the_player_simulation_finishes_sitting_before_opening_the_workshop() {
        let mut app = crate::sim_tests::app();
        app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            for mut enemy in world.query::<&mut crate::enemy::Enemy>().iter_mut(world) {
                enemy.disabled = true;
            }
            let (mut p, mut a, mut tf) = world.query::<(&mut Player, &mut Actor, &mut Transform)>().single_mut(world).unwrap();
            p.seat.sculptor = true;
            p.seat.phase = Phase::Kneeling;
            tf.translation = POS + Vec3::new(1.2, CAPSULE_HALF_HEIGHT, -0.5);
            assert!(a.play_state(&combat.player, SIT_DOWN));
        });
        for _ in 0..240 {
            app.update();
        }
        let world = app.world_mut();
        let (p, a, tf) = world.query::<(&Player, &Actor, &Transform)>().single(world).unwrap();
        assert!(p.seat.sculptor && p.seat.open && p.seat.phase == Phase::Menu,
            "visit={}, open={}, phase={:?}, state={}, time={}, pos={:?}", p.seat.sculptor, p.seat.open, p.seat.phase, a.state, a.t, tf.translation);
        assert_eq!(a.state, SITTING);
    }

    #[test]
    fn sculptor_works_on_the_prosthetic_for_the_visit_and_returns_to_sitting() {
        let data: CharData = serde_json::from_str(&std::fs::read_to_string(crate::paths::extracted().join("enemies/c7010.json")).unwrap()).unwrap();
        let mut a = Actor::new(Side::Enemy, 1.0, 1.0, 0.0, -1);
        a.play("SculptorIdle", "a000_000000");
        visit_animation(&mut a, &data, true);
        assert_eq!(a.anim, "a000_020001");
        a.t = data.length(&a.anim);
        visit_animation(&mut a, &data, true);
        assert_eq!(a.anim, "a000_021003");
        a.t = data.length(&a.anim) + 0.1;
        visit_animation(&mut a, &data, true);
        assert!(a.t < data.length(&a.anim));
        visit_animation(&mut a, &data, false);
        assert_eq!(a.anim, "a000_020002");
        a.t = data.length(&a.anim);
        visit_animation(&mut a, &data, false);
        assert_eq!(a.anim, "a000_000000");
    }
}
