//! Sim tests, part 13: the atlas movement pass (docs/kb/mechanics-atlas.json, category
//! movement). Each test names its atlas id.

use super::*;

/// Runs Wolf with the stick held until he is in the move loop.
fn run_until_moving(app: &mut App) {
    for _ in 0..90 {
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
        app.update();
        if player_state(app).0 == "Locomotion" {
            return;
        }
    }
    panic!("never moved: {:?}", player_state(app));
}

/// Lifts Wolf by `drop` over the flat test floor, as if the ground ended under him.
fn lift(app: &mut App, drop: f32) -> f32 {
    let world = app.world_mut();
    let mut t = world.query_filtered::<&mut Transform, With<Player>>().single_mut(world).unwrap();
    t.translation.y += drop;
    t.translation.y
}

/// Atlas movement.fall-state-entry (HKS BEH_R_FALL: on the move, env 224 >= 30 cm): running off
/// a 20 cm step stays in the move loop and lands without a fall; off 60 cm the fall starts once
/// he has dropped 30 cm; standing still, any drop falls at once.
#[test]
pub(crate) fn atlas_fall_state_entry_needs_thirty_centimetres_on_the_move() {
    for (moving, drop, want_fall) in [(true, 0.2f32, false), (true, 0.6, true), (false, 0.2, true)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        if moving {
            run_until_moving(&mut app);
        }
        let top = lift(&mut app, drop);
        let mut fell_at = None;
        let mut landed = false;
        for _ in 0..120 {
            if moving {
                app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
            }
            app.update();
            let world = app.world_mut();
            let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
            if a.state == "FreeFall" && fell_at.is_none() {
                fell_at = Some(top - t.translation.y);
            }
            if !a.airborne {
                landed = true;
                break;
            }
        }
        assert!(landed, "moving {moving} drop {drop}: never landed");
        assert_eq!(fell_at.is_some(), want_fall, "moving {moving} drop {drop}: fell at {fell_at:?}");
        if let (true, Some(h)) = (moving, fell_at) {
            assert!(h >= crate::player::FALL_START_DROP - 0.01 && h < crate::player::FALL_START_DROP + 0.15, "fall began after {h} m");
        }
        if !want_fall {
            assert_eq!(player_state(&mut app).0, "Locomotion");
        }
    }
}

/// Atlas movement.jump-weapon-state (HKS BEH_A_GROUND_JUMP 6148): Wolf jumps only with the
/// weapon state the area wants: drawn outside a non-combat area, sheathed inside one
/// (SP_EF_REF_IN_NON_COMBAT_AREA 600, here SpEffect 108900). Sheathed outside one (our X
/// sheathe, which the game draws out of at once) still jumps (player.rs arm_style_matches).
#[test]
pub(crate) fn atlas_jump_weapon_state_must_match_the_area() {
    for (sheathed, area, want_jump) in [(false, false, true), (true, false, true), (true, true, true), (false, true, false)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            let (mut a, mut p) = world.query::<(&mut Actor, &mut Player)>().single_mut(world).unwrap();
            a.sheathed = sheathed;
            if area {
                p.timed.push((108900, 1000.0));
            }
        }
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let mut jumped = false;
        for _ in 0..20 {
            app.update();
            let s = player_state(&mut app).0;
            jumped |= s.contains("GroundJump");
        }
        assert_eq!(jumped, want_jump, "sheathed {sheathed} non-combat area {area}");
    }
}

/// Atlas movement.damage-step (HKS _execDamageStep -> _set4DirJumpDir / _set4DirJumpTilt): a
/// dodge out of the face-down get-up (ref 504 up) with the stick 30 deg off straight back is
/// StandStepFromDown_FaceDown_B, turned by the 30 deg the stick is off the B centre (180);
/// straight back it is not turned.
#[test]
pub(crate) fn atlas_damage_step_from_down_tilts_by_the_stick() {
    let off = 30f32.to_radians();
    for (stick, want_tilt) in [(Vec2::new(off.sin(), -off.cos()), 30f32), (Vec2::new(0.0, -1.0), 0.0)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
                a.yaw = 0.0;
                assert!(a.play_state(&combat.player, "StandDamageLargeDownWakeUp_FaceDown"));
                (a.t, a.prev_t) = (0.3, 0.3);
            });
        }
        let mut got = None;
        for i in 0..4 {
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = stick;
                if i == 0 {
                    pad.press(Action::Step);
                }
            }
            app.update();
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            // The tilt shows from the next tick's root yaw.
            if a.state.starts_with("StandStepFromDown_") && got.is_none() {
                got = Some((a.state.clone(), 0.0));
            } else if let Some((st, _)) = got.take() {
                got = Some((st, a.root_yaw.to_degrees()));
                break;
            }
        }
        let (state, tilt) = got.expect("no step from down");
        assert_eq!(state, "StandStepFromDown_FaceDown_B");
        assert!((tilt.abs() - want_tilt).abs() < 1.0, "tilt {tilt} want {want_tilt}");
    }
}

/// Atlas movement.landing-stiffness (HKS BEH_R_LAND 2026-2041): a 6 m free fall (past
/// FALL_HEIGHT_SHORT_STIFF_LAND 5 m) lands in the stiff LandFreeFall, but with the dodge held and
/// the sword drawn Wolf steps out of it (_fireGroundStep -> W_GroundStep); sheathed (non-combat
/// mode) he does not.
#[test]
pub(crate) fn atlas_landing_stiffness_dodge_steps_out_of_a_stiff_landing() {
    for (dodge, sheathed, want) in [(false, false, "LandFreeFall"), (true, false, "GroundStep"), (true, true, "LandFreeFall")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                a.sheathed = sheathed;
                assert!(a.play_state(&combat.player, "FreeFall"));
                a.airborne = true;
                a.vel_y = 0.0;
                t.translation.y += 6.0;
            });
        }
        let mut after = Vec::new();
        let mut landed = false;
        for _ in 0..600 {
            app.world_mut().resource_mut::<PadInput>().dodge_held = dodge;
            app.update();
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            landed |= !a.airborne;
            if landed && after.last() != Some(&a.state) {
                after.push(a.state.clone());
            }
            if after.len() >= 3 {
                break;
            }
        }
        assert!(after.iter().any(|s| s.starts_with(want)), "dodge {dodge} sheathed {sheathed}: {after:?}");
        if want == "LandFreeFall" {
            assert!(!after.iter().any(|s| s.starts_with("GroundStep")), "dodge {dodge} sheathed {sheathed}: {after:?}");
        }
    }
}

/// Atlas movement.landing-stiffness (HKS BEH_R_LAND_WATER 2131): a fall of
/// FALL_HEIGHT_LONG_STIFF_LAND_WATER (20 m) or more into deep water lands long-stiff
/// (LandWaterFreeFallLongStiff a000_210011); a short one in place (LandWaterFreeFallInSitu).
#[test]
pub(crate) fn atlas_landing_stiffness_long_fall_into_water() {
    // A pool far from every other test's ground and pool (swim.rs WATER is global), its surface
    // at 3 m.
    let (lo, hi) = (Vec3::new(6990.0, -5.0, -10.0), Vec3::new(7010.0, 3.0, 10.0));
    crate::swim::WATER.write().unwrap().push((lo, hi));
    for (drop, want) in [(22.0f32, "LandWaterFreeFallLongStiff"), (3.0, "LandWaterFreeFallInSitu")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut p, mut t) = world.query::<(&mut Actor, &mut Player, &mut Transform)>().single_mut(world).unwrap();
                t.translation = Vec3::new(7000.0, hi.y + drop, 0.0) + Vec3::Y * CAPSULE_HALF_HEIGHT;
                assert!(a.play_state(&combat.player, "FreeFall"));
                a.airborne = true;
                a.vel_y = 0.0;
                p.air_top = t.translation.y;
            });
        }
        let mut got = None;
        for _ in 0..600 {
            app.update();
            let s = player_state(&mut app).0;
            if s != "FreeFall" {
                got = Some(s);
                break;
            }
        }
        assert_eq!(got.as_deref(), Some(want), "drop {drop}");
    }
    crate::swim::WATER.write().unwrap().retain(|w| w.0 != lo);
}
