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
pub(crate) const KIND: usize = usize::MAX;
pub(crate) const POS: Vec3 = Vec3::new(-7.0, 0.0, 3.0);
const REACH: f32 = 2.0;
const HEIGHT: f32 = 0.8;
static PRESENT: AtomicBool = AtomicBool::new(false);

#[derive(Component)]
pub(crate) struct Sculptor;

fn in_reach(feet: Vec3) -> bool {
    (feet - POS).with_y(0.0).length() < REACH && (feet.y - POS.y).abs() < HEIGHT
}

pub(crate) fn near(feet: Vec3) -> bool {
    PRESENT.load(Ordering::Relaxed) && in_reach(feet)
}

/// Before the idol and traversal in player::decide. Wolf uses his NPC-talk start/end clips,
/// holding the talk pose while the service menu is open. Damage or moving away cancels it.
pub(crate) fn tick(p: &mut Player, a: &mut Actor, data: &CharData, tf: &mut Transform, interact: bool, ended: bool, config: &GameConfig) -> bool {
    let feet = tf.translation - Vec3::Y * CAPSULE_HALF_HEIGHT;
    if !p.seat.sculptor {
        if interact && crate::map::map_id(config).as_deref() == Some(crate::sandbox::CONSTRUCT_ID)
            && near(feet) && p.seat.idol.is_none() && !a.airborne && a.hp > 0.0
            && crate::player::is_free(&a.state) && a.play_state(data, "Event700400")
        {
            let dir = POS - feet;
            a.yaw = f32::atan2(-dir.x, -dir.z);
            p.seat.sculptor = true;
            p.seat.phase = Phase::Menu;
            p.seat.cmd = None;
            p.seat.open = true;
        } else {
            return false;
        }
    }
    if a.hp <= 0.0 || !in_reach(feet) || !matches!(a.state.as_str(), "Event700400" | "Event700420") {
        p.seat.sculptor = false;
        p.seat.open = false;
        p.seat.cmd = None;
        return false;
    }
    a.move_vel = Vec3::ZERO;
    if p.seat.cmd.take() == Some(Cmd::Leave) {
        a.play_state(data, "Event700420");
        p.seat.phase = Phase::Leaving;
    } else if p.seat.phase == Phase::Leaving {
        if ended {
            p.seat.sculptor = false;
            p.seat.phase = Phase::Menu;
            a.procedural("Locomotion");
        }
    } else {
        a.t = a.t.min((data.length(&a.anim) - 1.0 / 30.0).max(0.0));
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
    actor.yaw = f32::atan2(-7.0, 3.0);
    actor.no_posture = true;
    actor.dmg_cut = 0.0;
    let idle = std::env::var("SHINOBI_SCULPTOR_ANIM").unwrap_or_else(|_| "a000_000000".into());
    actor.play("SculptorCarving", &idle);
    if kind.data.anim(&idle).is_none() {
        warn!("Sculptor: missing clip {idle}");
    }
    commands.spawn((Sculptor, actor, Transform::from_translation(POS + Vec3::Y * CAPSULE_HALF_HEIGHT), Visibility::default(), Name::new("Sculptor")));
    PRESENT.store(true, Ordering::Relaxed);
}

fn carving(combat: Res<Combat>, mut npc: Query<(&mut Actor, &mut Transform, &mut Visibility), With<Sculptor>>) {
    let visible = crate::map::loaded_map().as_deref().is_none_or(|id| id == crate::sandbox::CONSTRUCT_ID);
    for (mut a, mut tf, mut visibility) in &mut npc {
        *visibility = if visible { Visibility::Inherited } else { Visibility::Hidden };
        PRESENT.store(visible, Ordering::Relaxed);
        tf.translation = POS + Vec3::Y * CAPSULE_HALF_HEIGHT;
        a.airborne = false;
        a.vel_y = 0.0;
        let len = combat.data_of(&a).length(&a.anim);
        if len > 0.0 && a.t >= len {
            a.t %= len;
            a.prev_t = a.t;
        }
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
    tick(&mut p, &mut a, &combat.player, &mut tf, true, false, &config);
}

/// A three-quarter service view keeps seated c7010 visible beside the menu rather than behind
/// Wolf. The normal player camera resumes on the first frame after the visit.
fn talk_camera(wolf: Query<(&Player, &Transform)>, mut cameras: Query<&mut Transform, (With<crate::camera::OrbitCamera>, Without<Player>)>) {
    let Some((_, wolf)) = wolf.iter().find(|(p, _)| p.seat.sculptor) else { return };
    let dir = (wolf.translation - POS).with_y(0.0).normalize_or(Vec3::Z);
    let side = Vec3::Y.cross(dir);
    let at = POS + dir * 3.0 + side * 2.1 + Vec3::Y * 1.6;
    let look = POS - side * 0.6 + Vec3::Y * 0.7;
    for mut camera in &mut cameras {
        *camera = Transform::from_translation(at).looking_at(look, Vec3::Y);
    }
}

pub struct SculptorPlugin;

impl Plugin for SculptorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(FixedUpdate, carving.in_set(ActorSet::Decide))
            .add_systems(Update, shot_visit)
            .add_systems(PostUpdate, talk_camera.after(crate::camera::follow_player).before(TransformSystems::Propagate));
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
        assert!(!crate::items::parkour_pickups().iter().any(|p| in_reach(p.pos)), "Talk must not compete with an item pickup");
    }

    #[test]
    fn encounter_changes_keep_the_sculptors_character_data() {
        let mut app = App::new();
        let mut config = GameConfig::load().unwrap();
        config.world.map = Some("construct".into());
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
            p.seat.phase = Phase::Menu;
            p.seat.checkpoint = Some(3);
            p.gourd = 1;
            p.resurrections = 0;
            p.revive_locked = true;
            let mut a = Actor::new(Side::Player, 320.0, 120.0, 0.0, -1);
            a.hp = 73.0;
            assert!(a.play_state(&combat.player, "Event700400"));
            let mut tf = Transform::from_translation(POS + Vec3::new(1.0, CAPSULE_HALF_HEIGHT, 0.0));
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, false, &config));
            assert_eq!((a.hp, p.gourd, p.resurrections, p.seat.checkpoint), (73.0, 1, 0, Some(3)));
            assert!(p.revive_locked && !p.seat.respawn && !p.refill_items);
            p.seat.cmd = Some(Cmd::Leave);
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, false, &config));
            assert_eq!(a.state, "Event700420");
            assert!(tick(&mut p, &mut a, &combat.player, &mut tf, false, true, &config));
            assert!(!p.seat.sculptor);
            assert_eq!(a.state, "Locomotion");
            assert_eq!(p.seat.checkpoint, Some(3));
        });
    }
}
