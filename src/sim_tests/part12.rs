//! Sim tests, part 12: wall hug and ledge hang (docking.rs, docs/kb/docking.md), on synthetic
//! edges over the flat test floor.

use super::*;

/// A wall along X at z = 0 whose face looks to +Z (`c` on the -Z, wall side); `y` the edge
/// height.
fn edges(kind: u8, y: f32) -> crate::traversal::LedgeEdges {
    crate::traversal::LedgeEdges(vec![crate::traversal::LedgeEdge { a: Vec3::new(-3.0, y, 0.0), b: Vec3::new(3.0, y, 0.0), c: Vec3::new(0.0, y - 0.2, -0.1), kind, variation: 0 }])
}

fn place_wolf(app: &mut App, feet: Vec3, yaw: f32, airborne: bool) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>();
    let (mut a, mut t) = q.single_mut(world).unwrap();
    t.translation = feet + Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
    a.yaw = yaw;
    a.airborne = airborne;
    if airborne {
        a.vel_y = 0.0;
        a.play("FreeFall", "");
    }
}

fn run(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

#[test]
pub(crate) fn wolf_hugs_a_wall_moves_along_it_and_lets_go() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(3, 0.0));
    run(&mut app, 10);
    // 1 m in front of the wall, facing it (yaw 0 = -Z).
    place_wolf(&mut app, Vec3::new(0.0, 0.0, 1.0), 0.0, false);
    run(&mut app, 2);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    let mut states = Vec::new();
    for _ in 0..90 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let (state, _, _, pos) = player_actor(&mut app);
    println!("hug: {states:?} at {pos:?}");
    assert!(states.iter().any(|s| s.starts_with("StandToCover2")), "{states:?}");
    assert_eq!(state, "CoverIdle");
    assert!((pos.z - crate::docking::HUG_OUT).abs() < 0.05, "{pos:?}");
    // Along the wall: the stick to +X (camera yaw 0).
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    let mut moving = Vec::new();
    for _ in 0..60 {
        app.update();
        let s = player_actor(&mut app).0;
        if moving.last() != Some(&s) {
            moving.push(s);
        }
    }
    let (_, _, _, moved) = player_actor(&mut app);
    println!("hug move: {moving:?} to {moved:?}");
    assert!(moving.iter().any(|s| s.starts_with("CoverRun") || s.starts_with("CoverWalk")), "{moving:?}");
    assert!(moved.x > pos.x + 0.3, "moved {:.2} along", moved.x - pos.x);
    assert!((moved.z - crate::docking::HUG_OUT).abs() < 0.05, "left the wall line: {moved:?}");
    // The end of the edge (x = 3 - 0.35) holds him.
    run(&mut app, 240);
    let (_, _, _, end) = player_actor(&mut app);
    assert!(end.x <= 3.0 - crate::docking::END_MARGIN + 0.01, "past the end: {end:?}");
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    run(&mut app, 30);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    let mut leaving = Vec::new();
    for _ in 0..60 {
        app.update();
        let s = player_actor(&mut app).0;
        if leaving.last() != Some(&s) {
            leaving.push(s);
        }
    }
    println!("hug end: {leaving:?}");
    assert!(leaving.iter().any(|s| s == "CoverEnd"), "{leaving:?}");
    assert!(!crate::docking::is_docked_state(&player_actor(&mut app).0));
}

#[test]
pub(crate) fn a_fall_past_a_lip_catches_it_with_the_button_held_and_climbs_up() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(2, 4.0));
    run(&mut app, 10);
    // Falling past the face, 0.6 m out, facing the wall.
    place_wolf(&mut app, Vec3::new(0.0, 3.2, 0.6), 0.0, true);
    app.world_mut().resource_mut::<PadInput>().interact_held = true;
    let mut states = Vec::new();
    for _ in 0..240 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    app.world_mut().resource_mut::<PadInput>().interact_held = false;
    let (state, _, airborne, pos) = player_actor(&mut app);
    println!("catch: {states:?} at {pos:?}");
    assert!(states.iter().any(|s| s.starts_with("FreeFallToHang")), "{states:?}");
    assert_eq!(state, "HangIdle");
    assert!(!airborne);
    let feet = pos.y - crate::player::CAPSULE_HALF_HEIGHT;
    assert!((feet - (4.0 - crate::docking::HANG_DOWN)).abs() < 0.05, "feet {feet}");
    // Shimmy along it.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    run(&mut app, 60);
    let (shimmy, _, _, along) = player_actor(&mut app);
    println!("shimmy: {shimmy} to {along:?}");
    assert!((along.x - pos.x).abs() > 0.2, "no shimmy: {along:?}");
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    run(&mut app, 40);
    // Climb: the hang button (a jump kicks off the wall instead, atlas movement.hang-climb-drop).
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    let mut climb = Vec::new();
    let mut top = Vec3::ZERO;
    for _ in 0..150 {
        app.update();
        let (s, _, _, at) = player_actor(&mut app);
        if s == "HangToStand" {
            top = at;
        }
        if climb.last() != Some(&s) {
            climb.push(s);
        }
    }
    println!("climb: {climb:?} to {top:?}");
    assert!(climb.iter().any(|s| s == "HangToStand"), "{climb:?}");
    assert!((top.y - crate::player::CAPSULE_HALF_HEIGHT - 4.0).abs() < 0.1 && top.z < 0.0, "not on top: {top:?}");
}

/// Atlas movement.hang-climb-drop: a jump in the hang, with no stick, is the kick off the wall
/// (BEH_A_HANG_END_CONNECT -> HangJumpReady), never the climb.
#[test]
pub(crate) fn atlas_hang_jump_kicks_off_instead_of_climbing() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(2, 4.0));
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::new(0.0, 3.2, 0.6), 0.0, true);
    app.world_mut().resource_mut::<PadInput>().interact_held = true;
    run(&mut app, 240);
    app.world_mut().resource_mut::<PadInput>().interact_held = false;
    assert_eq!(player_actor(&mut app).0, "HangIdle");
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut seen = Vec::new();
    for _ in 0..60 {
        app.update();
        let s = player_actor(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "HangJumpReady"), "{seen:?}");
    assert!(!seen.iter().any(|s| s == "HangToStand"), "{seen:?}");
}

#[test]
pub(crate) fn a_step_from_the_hang_drops_wolf() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(2, 4.0));
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::new(0.0, 3.2, 0.6), 0.0, true);
    app.world_mut().resource_mut::<PadInput>().interact_held = true;
    run(&mut app, 240);
    app.world_mut().resource_mut::<PadInput>().interact_held = false;
    assert_eq!(player_actor(&mut app).0, "HangIdle");
    app.world_mut().resource_mut::<PadInput>().press(Action::Step);
    let mut states = Vec::new();
    for _ in 0..180 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let (_, _, _, pos) = player_actor(&mut app);
    println!("drop: {states:?} at {pos:?}");
    assert!(states.iter().any(|s| s == "HangToFreeFall"), "{states:?}");
    assert!(pos.y - crate::player::CAPSULE_HALF_HEIGHT < 0.1, "did not fall: {pos:?}");
}

#[test]
pub(crate) fn a_jump_against_a_wall_in_the_air_is_a_wall_jump_once() {
    let mut app = app();
    move_enemy_far(&mut app);
    // A wall at x = 500 facing -X (docking::TEST_WALLS, out of every other test's way).
    crate::docking::TEST_WALLS.lock().unwrap().push((Vec3::new(500.0, 0.0, 0.0), Vec3::NEG_X));
    run(&mut app, 10);
    // In the air 0.4 m from it, facing it (+X: yaw -90 deg).
    place_wolf(&mut app, Vec3::new(499.6, 1.5, 0.0), -std::f32::consts::FRAC_PI_2, true);
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut states = Vec::new();
    let mut top = 0.0f32;
    for i in 0..150 {
        app.update();
        let (s, _, _, at) = player_actor(&mut app);
        top = top.max(at.y - crate::player::CAPSULE_HALF_HEIGHT);
        if states.last() != Some(&s) {
            states.push(s);
        }
        // A second jump in the same flight is not another wall jump.
        if i == 40 {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        }
    }
    println!("wall jump: {states:?}, top {top:.2}");
    crate::docking::TEST_WALLS.lock().unwrap().clear();
    assert!(states.iter().any(|s| s == "WallJumpReady_F"), "{states:?}");
    assert!(states.iter().any(|s| s == "WallJumpStart_F_N"), "{states:?}");
    assert_eq!(states.iter().filter(|s| s.starts_with("WallJumpReady")).count(), 1, "{states:?}");
    assert!(top > 1.5 + 0.5, "no lift: top {top:.2}");
}

#[test]
pub(crate) fn wall_jump_types_follow_the_hks_cones() {
    use crate::docking::{contact_type, jump_type};
    assert_eq!(contact_type(0.0), 'F');
    assert_eq!(contact_type(170.0), 'B');
    assert_eq!(contact_type(-90.0), 'L');
    assert_eq!(contact_type(90.0), 'R');
    // Facing -Z (yaw 0): no stick vertical; into the wall (contact F, stick forward) vertical too;
    // the stick back (+Z) off a front wall: B.
    assert_eq!(jump_type('F', Vec3::ZERO, 0.0), 'N');
    assert_eq!(jump_type('F', Vec3::NEG_Z, 0.0), 'N');
    assert_eq!(jump_type('F', Vec3::Z, 0.0), 'B');
}

#[test]
pub(crate) fn shimmying_off_a_ledge_end_takes_the_corner_onto_the_next() {
    let mut app = app();
    move_enemy_far(&mut app);
    // Two hang edges meeting at an outside corner at x = 3: the first along X (face +Z), the
    // second along Z from (3, 4, 0) to (3, 4, -6) (face +X, c on its -X side).
    let e1 = crate::traversal::LedgeEdge { a: Vec3::new(-3.0, 4.0, 0.0), b: Vec3::new(3.0, 4.0, 0.0), c: Vec3::new(0.0, 3.8, -0.1), kind: 2, variation: 0 };
    let e2 = crate::traversal::LedgeEdge { a: Vec3::new(3.0, 4.0, 0.0), b: Vec3::new(3.0, 4.0, -6.0), c: Vec3::new(2.9, 3.8, -3.0), kind: 2, variation: 0 };
    app.world_mut().insert_resource(crate::traversal::LedgeEdges(vec![e1, e2]));
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::new(1.5, 3.2, 0.6), 0.0, true);
    app.world_mut().resource_mut::<PadInput>().interact_held = true;
    run(&mut app, 240);
    app.world_mut().resource_mut::<PadInput>().interact_held = false;
    assert_eq!(player_actor(&mut app).0, "HangIdle");
    // To his right (+X, facing -Z) round the corner.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    let mut states = Vec::new();
    // The wall under the two lips is the quadrant x < 3, z < 0: how close Wolf's body (the root,
    // and the hands 0.62 m ahead of it, counted +0.5) comes to it (negative: inside), and his
    // biggest step a tick (the user: "animations bypass walls", "corners not smooth").
    let (mut clear, mut step) = (f32::MAX, 0.0f32);
    let mut last = player_actor(&mut app).3;
    for _ in 0..300 {
        app.update();
        let (s, _, _, at) = player_actor(&mut app);
        let gap = if at.x < 3.0 && at.z < 0.0 { -(3.0 - at.x).min(-at.z) } else { Vec2::new((at.x - 3.0).max(0.0), at.z.max(0.0)).length() };
        let yaw = { let w = app.world_mut(); let mut q = w.query_filtered::<&Actor, With<Player>>(); q.single(w).unwrap().yaw };
        let hands = at + Quat::from_rotation_y(yaw) * Vec3::NEG_Z * 0.62;
        let hgap = if hands.x < 3.0 && hands.z < 0.0 { -(3.0 - hands.x).min(-hands.z) } else { Vec2::new((hands.x - 3.0).max(0.0), hands.z.max(0.0)).length() };
        clear = clear.min(gap.min(hgap + 0.5));
        step = step.max(at.distance(last));
        last = at;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    println!("corner clearance {clear:.2}, step {step:.3}");
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    run(&mut app, 60);
    let (_, _, _, pos) = player_actor(&mut app);
    println!("corner: {states:?} at {pos:?}");
    assert!(states.iter().any(|s| s == "HangCornerMoveRight"), "{states:?}");
    // The shimmy runs its loop: no stop while the stick holds (the script's docking stop reads
    // DockingStickAngle / Selector_DockingMoveDir).
    assert!(!states.iter().take_while(|s| !s.contains("Corner")).any(|s| s.contains("MoveEnd")), "{states:?}");
    assert!(clear > 0.5, "into the wall: clearance {clear:.2}");
    assert!(step < 0.12, "a {step:.3} m jump in one tick");
    // On the second face: out +X of x = 3.
    assert!((pos.x - (3.0 + crate::docking::HANG_OUT)).abs() < 0.05, "{pos:?}");
}

#[test]
pub(crate) fn at_a_free_look_wall_end_wolf_peeks_and_lets_go_from_the_peek() {
    let mut app = app();
    move_enemy_far(&mut app);
    let mut e = edges(3, 0.0);
    // COVER_EDGE_TYPE 1: free look at both ends.
    e.0[0].variation = 1;
    app.world_mut().insert_resource(e);
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::new(2.0, 0.0, 1.0), 0.0, false);
    run(&mut app, 2);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    run(&mut app, 90);
    assert_eq!(player_actor(&mut app).0, "CoverIdle");
    // Run to the +X end and keep pushing: the peek.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    let mut states = Vec::new();
    for _ in 0..200 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    println!("peek: {states:?}");
    assert!(states.iter().any(|s| s.starts_with("CoverActionLoop")), "{states:?}");
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    let mut out = Vec::new();
    for _ in 0..60 {
        app.update();
        let s = player_actor(&mut app).0;
        if out.last() != Some(&s) {
            out.push(s);
        }
    }
    println!("peek end: {out:?}");
    assert!(out.iter().any(|s| s == "CoverEndFromCoverLook"), "{out:?}");
}

#[test]
pub(crate) fn a_wall_with_no_look_has_no_peek() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(3, 0.0));
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::new(2.0, 0.0, 1.0), 0.0, false);
    run(&mut app, 2);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    run(&mut app, 90);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
    let mut states = Vec::new();
    for _ in 0..200 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(!states.iter().any(|s| crate::docking::is_peek(s)), "{states:?}");
}

#[test]
pub(crate) fn pressing_the_hug_while_running_at_a_wall_side_on_takes_the_moving_start() {
    let d = crate::docking::Dock::new_for_test(3);
    // Stick along the wall (+X) from the front: MoveToCover (the stick 90 deg off the wall).
    let st = crate::docking::hug_start_state(&d, 0.0, Vec3::X);
    assert!(st.starts_with("MoveToCover"), "{st}");
}

fn place_enemy(app: &mut App, at: Vec3, yaw: f32) {
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&mut Enemy, &mut Actor, &mut Transform), Without<crate::enemy::Cast>>();
    let (mut e, mut a, mut t) = q.single_mut(world).unwrap();
    t.translation = at + Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
    a.yaw = yaw;
    e.make_unaware(&mut a);
}

/// Peeking round a wall's end, an unaware Ochimusha passing at Wolf's side: the wall deathblow
/// (ThrowParam 11010060 -> 0061 on his left: a200_500600 -> 510600, the enemy ThrowDef12600); in a
/// plain hug there is none (atlas movement.docking-deathblows: the wall kills need the peek).
#[test]
pub(crate) fn an_unaware_enemy_passing_the_peeking_wolf_takes_the_wall_deathblow() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    for peek in [false, true] {
        let mut app = app_with("c1010");
        move_enemy_far(&mut app);
        let mut e = edges(3, 0.0);
        e.0[0].variation = 1;
        app.world_mut().insert_resource(e);
        run(&mut app, 10);
        place_wolf(&mut app, Vec3::new(2.0, 0.0, 1.0), 0.0, false);
        run(&mut app, 2);
        app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
        run(&mut app, 90);
        assert_eq!(player_actor(&mut app).0, "CoverIdle");
        if peek {
            app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(1.0, 0.0);
            for _ in 0..200 {
                app.update();
                if player_actor(&mut app).0.starts_with("CoverActionLoop") {
                    break;
                }
            }
            app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
            assert!(player_actor(&mut app).0.starts_with("CoverActionLoop"), "{:?}", player_actor(&mut app).0);
        }
        // Hugging, Wolf faces +Z; the +X end is on his left. The enemy 1.2 m past it, walking
        // on away from him (facing +X: yaw -90 deg).
        let at = player_actor(&mut app).3;
        place_enemy(&mut app, Vec3::new(at.x + 1.2, 0.0, 1.2), -std::f32::consts::FRAC_PI_2);
        app.update();
        app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
        let (mut seen, mut enemy_seen) = (Vec::new(), Vec::new());
        for _ in 0..200 {
            app.update();
            let (s, an) = { let (s, _, _, _) = player_actor(&mut app); (s, player_state(&mut app).1) };
            let tag = format!("{s} {an}");
            if seen.last() != Some(&tag) {
                seen.push(tag);
            }
            let es = enemy_state(&mut app).0;
            if enemy_seen.last() != Some(&es) {
                enemy_seen.push(es);
            }
        }
        println!("wall deathblow (peek {peek}): {seen:?} / enemy {enemy_seen:?}");
        let killed = seen.iter().any(|s| s.starts_with("DeathblowStart a200_5006"));
        assert_eq!(killed, peek, "{seen:?}");
        if peek {
            assert!(seen.iter().any(|s| s.starts_with("Deathblow a200_5106")), "{seen:?}");
            assert!(enemy_seen.iter().any(|s| s.starts_with("ThrowDef126")), "{enemy_seen:?}");
        }
    }
}

/// Hanging under a lip with an unaware Ochimusha standing at it, facing away: the ledge
/// deathblow (0080 -> 0081: a200_500800 -> 510800, ThrowDef12800), and Wolf is back hanging.
#[test]
pub(crate) fn an_unaware_enemy_at_the_lip_over_the_hanging_wolf_takes_the_ledge_deathblow() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    let mut app = app_with("c1010");
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(2, 1.6));
    run(&mut app, 10);
    // Hang: the lip 1.6 m up, so the feet are at 0 (the floor) and the enemy on top.
    place_wolf(&mut app, Vec3::new(0.0, 0.2, 0.6), 0.0, true);
    app.world_mut().resource_mut::<PadInput>().interact_held = true;
    run(&mut app, 240);
    app.world_mut().resource_mut::<PadInput>().interact_held = false;
    assert_eq!(player_actor(&mut app).0, "HangIdle");
    // On top, 1 m back from the lip, facing away from the edge (-Z).
    place_enemy(&mut app, Vec3::new(0.0, 1.6, -1.0), 0.0);
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let (mut seen, mut enemy_seen) = (Vec::new(), Vec::new());
    for _ in 0..360 {
        app.update();
        let (s, an) = { let (s, _, _, _) = player_actor(&mut app); (s, player_state(&mut app).1) };
        let tag = format!("{s} {an}");
        if seen.last() != Some(&tag) {
            seen.push(tag);
        }
        let es = enemy_state(&mut app).0;
        if enemy_seen.last() != Some(&es) {
            enemy_seen.push(es);
        }
    }
    println!("ledge deathblow: {seen:?} / enemy {enemy_seen:?}");
    assert!(seen.iter().any(|s| s.starts_with("DeathblowStart a200_5008") || s.starts_with("DeathblowStart a200_5010")), "{seen:?}");
    assert!(seen.iter().any(|s| s.starts_with("Deathblow a200_5108") || s.starts_with("Deathblow a200_5110")), "{seen:?}");
    assert!(seen.last().is_some_and(|s| s.starts_with("HangIdle")), "not back hanging: {seen:?}");
    // ThrowDef12800 / 13000 have no ThrowDefDeath (the fall off the lip is the death): the enemy
    // is dead from the throw (the sandbox brings it back afterwards).
    assert!(enemy_seen.iter().any(|s| s == "ThrowDef12800" || s == "ThrowDef13000"), "{enemy_seen:?}");
}

/// The ground hang from the top of a lip, facing the drop and facing away: the start clip's own
/// path takes Wolf over and down onto the hang (no big drag at its end).
#[test]
pub(crate) fn the_ground_hang_start_follows_its_clip_onto_the_hang() {
    for (label, yaw) in [("facing the drop", std::f32::consts::PI), ("back to the drop", 0.0)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.world_mut().insert_resource(edges(2, 0.0));
        run(&mut app, 10);
        // On the top (the wall side, -Z), 0.4 m back from the lip; the drop is +Z (face +Z).
        place_wolf(&mut app, Vec3::new(0.0, 0.0, -0.4), yaw, false);
        // The fall check would drop him: the test floor is at 0, the "hang" below it.
        run(&mut app, 2);
        app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
        let mut states = Vec::new();
        let (mut jumps, mut drops) = (0.0f32, 0.0f32);
        let mut last = player_actor(&mut app).3;
        for _ in 0..150 {
            app.update();
            let (s, _, _, at) = player_actor(&mut app);
            jumps = jumps.max((at - last).with_y(0.0).length());
            drops = drops.max((at.y - last.y).abs());
            last = at;
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        let feet = last.y - crate::player::CAPSULE_HALF_HEIGHT;
        println!("ground hang ({label}): {states:?} at {last:?}, biggest step {jumps:.3}, drop {drops:.3}");
        assert!(states.iter().any(|s| s.starts_with("StandToHang2")), "{states:?}");
        assert!((feet + crate::docking::HANG_DOWN).abs() < 0.05, "feet {feet}");
        // Per 60 Hz tick no more than a run's step (6 m/s) across: the clip carried him, not a
        // snap. Down, the clip's own drop: StandToHang2 (a000_217102) slips the 1.6 m over the lip
        // between 0.35 and 0.6 s, up to 0.16 m a tick.
        assert!(jumps < 0.12, "a {jumps:.3} m jump in one tick");
        assert!(drops < 0.18, "a {drops:.3} m drop in one tick");
    }
}

fn wolf_yaw(app: &mut App) -> f32 {
    let w = app.world_mut();
    let mut q = w.query_filtered::<&Actor, With<Player>>();
    q.single(w).unwrap().yaw
}

/// Running, let go and the stick back inside the stop's first 9 frames: the run quick turn
/// (StandRunQuickTurn*180) turns Wolf on the spot instead of a wide circle at run speed, and he
/// runs on the other way.
#[test]
pub(crate) fn a_run_reversed_quick_turns_instead_of_circling() {
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::ZERO, 0.0, false);
    // Run along -Z.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    run(&mut app, 90);
    // Let go for a frame (the stop opens the turn, its TAE ref 14), then the stick back.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    run(&mut app, 2);
    let start = player_actor(&mut app).3;
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    let mut states = Vec::new();
    let mut side = 0.0f32;
    for _ in 0..60 {
        app.update();
        let (s, _, _, at) = player_actor(&mut app);
        side = side.max((at.x - start.x).abs());
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let end = player_actor(&mut app).3;
    let yaw = wolf_yaw(&mut app);
    println!("quick turn: {states:?}, side drift {side:.2}, from {start:?} to {end:?}, yaw {yaw:.2}");
    assert!(states.iter().any(|s| s.starts_with("StandRunQuickTurn")), "{states:?}");
    assert!(side < 0.3, "drifted {side:.2} m sideways: a circle");
    assert!(end.z > start.z + 1.0, "not running back: {start:?} -> {end:?}");
    assert!(turn_between(yaw, std::f32::consts::PI).abs() < 0.1, "yaw {yaw}");
}

/// Off a ledge at a run Wolf carries his speed into the fall (it was zeroed: straight down). The
/// fall state starts after 30 cm (atlas movement.fall-state-entry), about 10 ticks in.
#[test]
pub(crate) fn a_run_off_a_ledge_carries_on_into_the_fall() {
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::ZERO, 0.0, false);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    run(&mut app, 90);
    // The floor drops away under him: lift him 3 m, still on his feet.
    {
        let w = app.world_mut();
        let mut q = w.query::<(&mut Player, &mut Transform)>();
        let (mut p, mut t) = q.single_mut(w).unwrap();
        t.translation.y += 3.0;
        p.air_top = t.translation.y;
    }
    let before = player_actor(&mut app).3;
    run(&mut app, 12);
    let (s, _, airborne, at) = player_actor(&mut app);
    let moved = (at - before).with_y(0.0).length();
    println!("walk-off: {s} airborne {airborne}, {moved:.2} m in 0.2 s");
    // The graph's BEH_R_FALL: W_SelfFallStart (a000_200000), then FreeFall.
    assert!(airborne && (s == "FreeFall" || s == "SelfFallStart"), "{s} {airborne}");
    assert!(moved > 0.3, "fell straight down: {moved:.2} m");
}

/// Running, the stick thrown straight back with no let-go: no quick turn (the script's in-move turn
/// needs a stop's TAE ref 14); the run swings round at the exe's 720 deg/s (FUN_1407da540), a
/// half turn in about 0.25 s.
#[test]
pub(crate) fn a_run_thrown_straight_back_swings_round_at_the_turn_rate() {
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::ZERO, 0.0, false);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    run(&mut app, 120);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    let mut states = Vec::new();
    let mut turned_at = None;
    for i in 0..40 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
        if turned_at.is_none() && turn_between(wolf_yaw(&mut app), std::f32::consts::PI).abs() < 0.05 {
            turned_at = Some(i + 1);
        }
    }
    println!("thrown back: {states:?}, turned after {turned_at:?} ticks");
    assert!(!states.iter().any(|s| s.contains("QuickTurn")), "{states:?}");
    assert!(turned_at.is_some_and(|n| (12..=20).contains(&n)), "turned after {turned_at:?} ticks");
}

fn turn_between(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}


/// Walking, the stick thrown back: the walk's quick turn (StandWalkQuickTurn*180, a000_000142 /
/// 143) turns Wolf round, then he walks on the other way.
#[test]
pub(crate) fn a_walk_reversed_turns_in_place_then_walks_on() {
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::ZERO, 0.0, false);
    app.world_mut().resource_mut::<PadInput>().walk = true;
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    run(&mut app, 90);
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
    run(&mut app, 2);
    let start = player_actor(&mut app).3;
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
    let mut states = Vec::new();
    for _ in 0..90 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let end = player_actor(&mut app).3;
    let yaw = wolf_yaw(&mut app);
    println!("walk turn: {states:?}, {start:?} -> {end:?}, yaw {yaw:.2}");
    assert!(states.iter().any(|s| s.starts_with("StandWalkQuickTurn")), "{states:?}");
    assert_eq!(states.last().map(String::as_str), Some("Locomotion"));
    assert!(end.z > start.z + 0.3, "not walking back: {start:?} -> {end:?}");
    assert!(turn_between(yaw, std::f32::consts::PI).abs() < 0.1, "yaw {yaw}");
}

/// The idol tests set the global idol list (idol.rs IDOLS): one at a time.
static IDOL_LIST: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// At a Sculptor's Idol: E kneels Wolf (StatueAttachStart -> Loop, resting him and asking for
/// the menu); the menu's Rest plays the rest clip, Travel the warp and stands him up at the
/// other idol. The test idols stand far off (x 600) as the idol list is global.
#[test]
pub(crate) fn wolf_kneels_at_an_idol_rests_and_travels() {
    let _list = IDOL_LIST.lock().unwrap_or_else(|e| e.into_inner());
    use crate::idol::{Cmd, Idol, IDOLS};
    fn wolf<R>(app: &mut App, f: impl FnOnce(&mut crate::player::Player) -> R) -> R {
        let w = app.world_mut();
        let mut q = w.query::<&mut crate::player::Player>();
        f(&mut q.single_mut(w).unwrap())
    }
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    let a = Idol { name: "A".into(), pos: Vec3::new(600.0, 0.0, 0.0), out: Vec3::X, ..Default::default() };
    let b = Idol { name: "B".into(), pos: Vec3::new(600.0, 0.0, 10.0), out: Vec3::NEG_Z, ..Default::default() };
    let (spot_a, spot_b) = (a.spot(), b.spot());
    *IDOLS.write().unwrap() = vec![a, b];
    crate::idol::forget_found();
    place_wolf(&mut app, spot_a + Vec3::X * 0.5, 0.0, false);
    run(&mut app, 2);
    // Hurt, so the rest shows.
    {
        let w = app.world_mut();
        let mut q = w.query_filtered::<&mut Actor, With<Player>>();
        let mut a = q.single_mut(w).unwrap();
        a.hp = 10.0;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    run(&mut app, 2);
    assert_eq!(player_actor(&mut app).0, "StatueAttachStart");
    // The first kneel lights the idol: the found banner plays (LIGHTING_SECS) and the menu
    // waits for it, Wolf kneeling on in the loop.
    assert!(wolf(&mut app, |p| p.seat.lighting.is_some()), "the idol was not lit");
    run(&mut app, 80);
    assert!(!wolf(&mut app, |p| p.seat.open), "the menu opened over the found banner");
    run(&mut app, (crate::idol::LIGHTING_SECS * 60.0) as usize - 80);
    let (s, _, _, at) = player_actor(&mut app);
    assert_eq!(s, "StatueAttachLoop");
    assert!(wolf(&mut app, |p| p.seat.lighting.is_none()), "the banner did not end");
    assert!((at - spot_a).with_y(0.0).length() < 0.05, "{at:?}");
    {
        let w = app.world_mut();
        let mut q = w.query_filtered::<&Actor, With<Player>>();
        let a = q.single(w).unwrap();
        assert_eq!(a.hp, a.hp_max, "not rested");
    }
    assert!(wolf(&mut app, |p| std::mem::take(&mut p.seat.open)), "the menu was not asked for");
    wolf(&mut app, |p| p.seat.cmd = Some(Cmd::Rest));
    run(&mut app, 2);
    assert_eq!(player_actor(&mut app).0, "StatueAttachRest");
    run(&mut app, 240);
    assert_eq!(player_actor(&mut app).0, "StatueAttachLoop");
    wolf(&mut app, |p| p.seat.cmd = Some(Cmd::Warp(1)));
    let mut states = Vec::new();
    for _ in 0..600 {
        app.update();
        let s = player_actor(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let (_, _, _, at) = player_actor(&mut app);
    println!("idol: {states:?} at {at:?}");
    assert_eq!(states, ["StatueAttachWarp", "StatueAttachEnd", "StandIdle"]);
    assert!((at - spot_b).with_y(0.0).length() < 0.6, "{at:?}");
    let seat = wolf(&mut app, |p| p.seat.clone());
    assert_eq!((seat.idol, seat.checkpoint), (None, Some(1)));
    // A retry (or a death) brings him back at that idol, not the map's start.
    app.world_mut().resource_mut::<PadInput>().reset = true;
    run(&mut app, 3);
    let (s, _, _, at) = player_actor(&mut app);
    let (want, _) = crate::idol::respawn_at(1).unwrap();
    assert!((at - want).with_y(0.0).length() < 0.1, "{s} at {at:?}, the idol's spot {want:?}");
    IDOLS.write().unwrap().clear();
}

/// Up from an idol (E, then the menu's Leave): the stick moves and turns Wolf every way again,
/// back included.
#[test]
pub(crate) fn up_from_an_idol_the_stick_moves_wolf_every_way() {
    let _list = IDOL_LIST.lock().unwrap_or_else(|e| e.into_inner());
    use crate::idol::{Cmd, Idol, IDOLS};
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    let a = Idol { name: "A".into(), pos: Vec3::new(600.0, 0.0, 0.0), out: Vec3::X, ..Default::default() };
    let spot = a.spot();
    *IDOLS.write().unwrap() = vec![a];
    place_wolf(&mut app, spot + Vec3::X * 0.5, 0.0, false);
    run(&mut app, 2);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    run(&mut app, 90);
    assert_eq!(player_actor(&mut app).0, "StatueAttachLoop");
    {
        let w = app.world_mut();
        let mut q = w.query::<&mut crate::player::Player>();
        q.single_mut(w).unwrap().seat.cmd = Some(Cmd::Leave);
    }
    for _ in 0..300 {
        app.update();
        if player_actor(&mut app).0 == "StandIdle" {
            break;
        }
    }
    run(&mut app, 10);
    let mut moves = Vec::new();
    for stick in [Vec2::new(0.0, -1.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0), Vec2::new(-1.0, 0.0)] {
        let start = player_actor(&mut app).3;
        app.world_mut().resource_mut::<PadInput>().stick = stick;
        run(&mut app, 90);
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::ZERO;
        run(&mut app, 40);
        moves.push((stick, player_actor(&mut app).3 - start, wolf_yaw(&mut app)));
    }
    IDOLS.write().unwrap().clear();
    println!("up from the idol: {moves:?}");
    for (stick, m, yaw) in moves {
        // Stick up is -Z (the camera looks down -Z), right +X; Wolf turns to face his way.
        let want = Vec3::new(stick.x, 0.0, -stick.y);
        assert!(m.with_y(0.0).normalize_or_zero().dot(want) > 0.7, "stick {stick}: moved {m}");
        let facing = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
        assert!(facing.dot(want) > 0.7, "stick {stick}: faces {facing} (yaw {yaw})");
    }
}

/// Up from an idol with the stick held: the stand-up (StatueAttachEnd, 40 frames) gives way to
/// the move when its move window opens at frame 24, not at its end (#277).
#[test]
pub(crate) fn up_from_an_idol_the_held_stick_moves_at_the_stand_ups_window() {
    let _list = IDOL_LIST.lock().unwrap_or_else(|e| e.into_inner());
    use crate::idol::{Cmd, Idol, IDOLS};
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    let a = Idol { name: "A".into(), pos: Vec3::new(600.0, 0.0, 0.0), out: Vec3::X, ..Default::default() };
    let spot = a.spot();
    *IDOLS.write().unwrap() = vec![a];
    place_wolf(&mut app, spot + Vec3::X * 0.5, 0.0, false);
    run(&mut app, 2);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    run(&mut app, 90);
    assert_eq!(player_actor(&mut app).0, "StatueAttachLoop");
    {
        let w = app.world_mut();
        let mut q = w.query::<&mut crate::player::Player>();
        q.single_mut(w).unwrap().seat.cmd = Some(Cmd::Leave);
    }
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    let mut states = Vec::new();
    let mut up_for = 0;
    for _ in 0..120 {
        app.update();
        let s = player_actor(&mut app).0;
        if s == "StatueAttachEnd" {
            up_for += 1;
        }
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    IDOLS.write().unwrap().clear();
    println!("up from the idol, stick held: {states:?}, {up_for} ticks standing up");
    assert!(states.contains(&"StatueAttachEnd".to_string()), "{states:?}");
    // Frame 24 at 30 fps is 48 ticks; the window's one-frame latch adds two. The whole clip is 80.
    assert!((48..=56).contains(&up_for), "stood up for {up_for} ticks: {states:?}");
    assert!(!states.last().is_some_and(|s| s.starts_with("Statue") || s == "StandIdle"), "{states:?}");
}

/// Standing, the stick pushed behind Wolf: the quick-turn move start (StandQuickTurnRunStart*180,
/// or with walk held StandQuickTurnWalkStart*180) turns him and he moves off the other way.
#[test]
pub(crate) fn standing_a_stick_behind_quick_turns_into_the_move() {
    for walk in [false, true] {
        let mut app = app();
        move_enemy_far(&mut app);
        run(&mut app, 10);
        place_wolf(&mut app, Vec3::ZERO, 0.0, false);
        run(&mut app, 20);
        let start = player_actor(&mut app).3;
        app.world_mut().resource_mut::<PadInput>().walk = walk;
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, -1.0);
        let mut states = Vec::new();
        for _ in 0..90 {
            app.update();
            let s = player_actor(&mut app).0;
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        let end = player_actor(&mut app).3;
        let yaw = wolf_yaw(&mut app);
        println!("standing turn (walk {walk}): {states:?} {start:?} -> {end:?}");
        let want = if walk { "StandQuickTurnWalkStart" } else { "StandQuickTurnRunStart" };
        assert!(states.first().is_some_and(|s| s.starts_with(want)), "{states:?}");
        assert_eq!(states.last().map(String::as_str), Some("Locomotion"));
        assert!(end.z > start.z + 0.5, "not moving the other way: {start:?} -> {end:?}");
        assert!(turn_between(yaw, std::f32::consts::PI).abs() < 0.1, "yaw {yaw}");
    }
}

/// A quick item other than the gourd (items.rs): C moves the hand to the next slot, R plays its
/// use state (Ako's Sugar: ItemCandyAttackBuff, a000_250800) and at the consume frame Wolf wears
/// its effect (SpEffect 3401, 30 s) and his attack rate rises; the stack loses one.
#[test]
pub(crate) fn a_sugar_is_eaten_and_raises_the_attack() {
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
        config.player.items = vec![(3400, 3)];
        config.player.quick_items = vec![3000, 3400, 0, 0, 0];
    }
    app.world_mut().init_resource::<crate::items::Used>();
    app.world_mut().resource_mut::<PadInput>().press(Action::NextItem);
    app.update();
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    let mut states = Vec::new();
    for _ in 0..120 {
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<&Actor, With<Player>>();
        let s = q.single(world).unwrap().state.clone();
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s == "ItemCandyAttackBuff"), "{states:?}");
    let world = app.world_mut();
    let mut q = world.query_filtered::<(&Actor, &Player), With<Player>>();
    let (a, p) = q.single(world).unwrap();
    assert_eq!(p.quick_slot, 1);
    assert!(p.timed.iter().any(|(id, left)| *id == 3401 && *left > 25.0), "{:?}", p.timed);
    assert!((a.atk_rate - 1.125).abs() < 1e-3, "atk_rate {}", a.atk_rate);
    assert_eq!(app.world().resource::<crate::items::Used>().0, vec![3400]);
}

/// The Construct's courtyard guards walk their rounds (WalkFront*), unaware of a far-off Wolf;
/// they do not run them (the user: "the enemies run ... instead of casual walking").
#[test]
fn a_courtyard_guard_walks_its_round() {
    let mut app = app_cfg(crate::sandbox::GUARD_CHR, None, Vec::new());
    let (at, yaw, route) = crate::sandbox::construct_patrols().remove(0);
    let guard = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, -150.0);
        let e = world.query_filtered::<Entity, With<Enemy>>().iter(world).next().unwrap();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut en) = q.get_mut(world, e).unwrap();
        t.translation = at.with_y(CAPSULE_HALF_HEIGHT);
        a.yaw = yaw;
        en.make_unaware(&mut a);
        world.entity_mut(e).insert(crate::enemy::PatrolRoute(route));
        e
    };
    let mut states = std::collections::BTreeSet::new();
    let mut moved = 0.0f32;
    for _ in 0..60 * 12 {
        app.update();
        let a = app.world().get::<Actor>(guard).unwrap();
        states.insert(a.state.clone());
        moved = moved.max((app.world().get::<Transform>(guard).unwrap().translation.with_y(0.0) - at).length());
    }
    assert!(moved > 2.0, "it walks its round: {moved:.2} m, states {states:?}");
    assert!(states.iter().any(|s| s.starts_with("WalkFront")), "walking: {states:?}");
    assert!(!states.iter().any(|s| s.starts_with("RunFront")), "not running: {states:?}");
}

/// Atlas armor.flag73-parry-gate: a soldier's swing keeps its hit level between and inside its
/// ChrActionFlag 73 spans (3001 at 0.5 s and 1.0 s, 3002 at 1.0 s); only a spCategory 1001
/// window lowers it (test super_armour_swaps_the_damage_level_in_its_window).
#[test]
fn a_soldiers_plain_swing_can_be_interrupted() {
    let app = app_cfg("c1010", None, Vec::new());
    let combat = app.world().resource::<crate::data::Combat>();
    let d = &combat.kind(0).data;
    let mut a = Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 1.0, 0);
    for (anim, t, level) in [("a000_003001", 0.5, 2), ("a000_003001", 1.0, 2), ("a000_003002", 1.0, 2)] {
        a.anim = anim.to_string();
        a.t = t;
        assert_eq!(crate::combat::damage_motion_level(d, &a, 2), level, "{anim} at {t} s");
    }
}

/// Two guards: one fighting Wolf, the other 12 m off facing away (it cannot see him). The
/// fighter's swings and hits call friends (AtkParam AppearAiSoundId 2000025 / HitAiSoundId
/// 2000020, bCallFriend): the other hears them and comes (the user: "enemies should hear their
/// comrades ... fighting").
#[test]
fn a_guard_hears_its_comrade_fighting() {
    let g = crate::config::GroupEnemy { chr: "c1010".into(), npc_row: None, at: None, msb: None };
    let mut app = app_cfg("c1010", None, vec![g]);
    let (fighter, other) = {
        let world = app.world_mut();
        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Enemy>>().iter(world).collect();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut e) = q.get_mut(world, ents[0]).unwrap();
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, -2.5);
        a.yaw = 0.0;
        e.aggressive = true;
        let (mut t, mut a, mut e) = q.get_mut(world, ents[1]).unwrap();
        t.translation = Vec3::new(12.0, CAPSULE_HALF_HEIGHT, -2.5);
        // Facing +X, away from Wolf at the origin.
        a.yaw = -std::f32::consts::FRAC_PI_2;
        e.make_unaware(&mut a);
        e.aggressive = true;
        (ents[0], ents[1])
    };
    let mut best = 0u8;
    for _ in 0..60 * 12 {
        app.update();
        best = best.max(app.world().get::<Enemy>(other).unwrap().targeting.state);
    }
    let f = app.world().get::<Enemy>(fighter).unwrap().targeting.state;
    assert!(best >= crate::stealth::CAUTION, "the other guard never noticed (fighter state {f}, other best {best})");
}

/// A guard that spots Wolf calls its comrades (AiSoundParam 2000000, bCallFriend) as it finds
/// him (the user: "enemies should hear their comrades spotting").
#[test]
fn a_guard_calls_out_as_it_spots_wolf() {
    let mut app = app_cfg("c1010", None, Vec::new());
    let guard = {
        let world = app.world_mut();
        let e = world.query_filtered::<Entity, With<Enemy>>().iter(world).next().unwrap();
        let mut q = world.query_filtered::<(&mut Transform, &mut Actor, &mut Enemy), With<Enemy>>();
        let (mut t, mut a, mut en) = q.get_mut(world, e).unwrap();
        // 5 m ahead of Wolf, facing him (+Z).
        t.translation = Vec3::new(0.0, CAPSULE_HALF_HEIGHT, -5.0);
        a.yaw = std::f32::consts::PI;
        en.make_unaware(&mut a);
        e
    };
    let mut called = false;
    for _ in 0..60 * 4 {
        app.update();
        called |= app.world().resource::<crate::enemy::AiSounds>().0.iter().any(|s| s.id == 2000000 && s.owner == Some(guard));
    }
    assert!(called, "no call (state {})", app.world().get::<Enemy>(guard).unwrap().targeting.state);
}

/// Wolf in deep water swims (SwimIdle); Ctrl dives only with the Mibu Breathing Technique
/// held (swim.rs; the user: "swimming now ... mibu breathing technique").
#[test]
fn wolf_swims_and_dives_with_the_mibu_technique() {
    // A pool far from every other test's ground (swim.rs WATER is global).
    let (lo, hi) = (Vec3::new(990.0, -5.0, -10.0), Vec3::new(1010.0, -0.4, 10.0));
    crate::swim::WATER.write().unwrap().push((lo, hi));
    let mut app = app_cfg("c1010", None, Vec::new());
    let feet = Vec3::new(1000.0, -3.0, 0.0);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = feet + Vec3::Y * CAPSULE_HALF_HEIGHT;
    }
    run(&mut app, 120);
    let state = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().state.clone()
    };
    let s = state(&mut app);
    assert!(s.starts_with("Swim"), "swimming: {s}");
    let y = {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation.y - CAPSULE_HALF_HEIGHT
    };
    assert!((y - (hi.y - crate::swim::SWIM_DEPTH)).abs() < 0.05, "at the surface: feet {y}");
    // Without the technique Ctrl (the prosthetic key) does not dive.
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.items.retain(|(g, _)| *g != crate::swim::MIBU_BREATHING);
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    run(&mut app, 30);
    let s = state(&mut app);
    assert!(!s.contains("Dive"), "no dive without it: {s}");
    // With it, Ctrl goes under.
    crate::items::add(&mut app.world_mut().resource_mut::<crate::config::GameConfig>(), crate::swim::MIBU_BREATHING, 1);
    app.world_mut().resource_mut::<PadInput>().press(Action::Prosthetic);
    run(&mut app, 10);
    assert_eq!(state(&mut app), "SwimToDive");
    crate::swim::WATER.write().unwrap().retain(|w| w.0 != lo);
}

/// Swimming with the stick held moves Wolf (the swim clips' root motion), and under water jump
/// held lifts him (the user: "only floats, not swims around ... nor ascend").
#[test]
fn wolf_swims_forward_and_rises() {
    let (lo, hi) = (Vec3::new(1990.0, -5.0, -10.0), Vec3::new(2010.0, -0.4, 10.0));
    crate::swim::WATER.write().unwrap().push((lo, hi));
    let mut app = app_cfg("c1010", None, Vec::new());
    let pos = |app: &mut App| {
        let world = app.world_mut();
        let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        (t.translation, a.state.clone(), a.anim.clone())
    };
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(2000.0, -3.0 + CAPSULE_HALF_HEIGHT, 0.0);
    }
    run(&mut app, 60);
    let (p0, s0, _) = pos(&mut app);
    let mut states = std::collections::BTreeSet::new();
    for _ in 0..120 {
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::Y;
        app.update();
        states.insert(pos(&mut app).1);
    }
    let (p1, _, anim) = pos(&mut app);
    assert!((p1 - p0).with_y(0.0).length() > 1.5, "swam {:.2} m from {s0}: states {states:?}, anim {anim}", (p1 - p0).with_y(0.0).length());
    crate::swim::WATER.write().unwrap().retain(|w| w.0 != lo);
}

/// Shift held swims fast: the kick chain (a000_031400 / 031410) covers more water than the plain
/// stroke in the same time (the user: "fast swimming with shift").
#[test]
fn shift_swims_fast() {
    let mut dist = Vec::new();
    for (k, fast) in [false, true].into_iter().enumerate() {
        let x = 3000.0 + 100.0 * k as f32;
        let (lo, hi) = (Vec3::new(x - 30.0, -5.0, -40.0), Vec3::new(x + 30.0, -0.4, 40.0));
        crate::swim::WATER.write().unwrap().push((lo, hi));
        let mut app = app_cfg("c1010", None, Vec::new());
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Transform, With<Player>>();
            q.single_mut(world).unwrap().translation = Vec3::new(x, -3.0 + CAPSULE_HALF_HEIGHT, 0.0);
        }
        run(&mut app, 60);
        let at = |app: &mut App| {
            let world = app.world_mut();
            world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation
        };
        let p0 = at(&mut app);
        for _ in 0..180 {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.stick = Vec2::Y;
            pad.dodge_held = fast;
            app.update();
        }
        dist.push((at(&mut app) - p0).with_y(0.0).length());
        crate::swim::WATER.write().unwrap().retain(|w| w.0 != lo);
    }
    assert!(dist[1] > dist[0] * 1.3, "plain {:.1} m, fast {:.1} m in 3 s", dist[0], dist[1]);
}

/// Jump at the surface leaves the water upward: the swim jump (SwimKickMoveJumpReady, then the
/// Start launched by row 221; the user: "add jumping", "there is special jump animation").
#[test]
fn wolf_jumps_out_of_the_water() {
    let (lo, hi) = (Vec3::new(3990.0, -5.0, -10.0), Vec3::new(4010.0, -0.4, 10.0));
    crate::swim::WATER.write().unwrap().push((lo, hi));
    let mut app = app_cfg("c1010", None, Vec::new());
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(4000.0, -3.0 + CAPSULE_HALF_HEIGHT, 0.0);
    }
    run(&mut app, 60);
    let feet = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation.y - CAPSULE_HALF_HEIGHT
    };
    let y0 = feet(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
    let mut top = y0;
    // Its Ready (0.33 s) first, then the launch.
    for _ in 0..80 {
        app.update();
        top = top.max(feet(&mut app));
    }
    assert!(top > hi.y + 0.3, "jumped from {y0:.2} to {top:.2}, surface {}: {}", hi.y, log_text(&app));
    crate::swim::WATER.write().unwrap().retain(|w| w.0 != lo);
}

/// In the water the attack swings the water combo (SwimAttackCombo1, then 2 on a second press)
/// and the gourd drinks its water clip and heals (the user: "add attacks under the water and item
/// usage").
#[test]
fn wolf_attacks_and_drinks_in_the_water() {
    let (lo, hi) = (Vec3::new(4990.0, -5.0, -10.0), Vec3::new(5010.0, -0.4, 10.0));
    crate::swim::WATER.write().unwrap().push((lo, hi));
    let mut app = app_cfg("c1010", None, Vec::new());
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(world).unwrap().translation = Vec3::new(5000.0, -3.0 + CAPSULE_HALF_HEIGHT, 0.0);
    }
    run(&mut app, 150);
    let wolf = |app: &mut App| {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        (a.state.clone(), a.hp, a.hp_max)
    };
    // Held: the thrust; a tap: the cut, and a second tap chains the next step's cut.
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    for _ in 0..10 {
        app.world_mut().resource_mut::<PadInput>().attack_held = true;
        app.update();
    }
    assert_eq!(wolf(&mut app).0, "SwimAttackCombo1", "held: the thrust");
    // (The sim keeps the held flag until it is cleared: no input reader.)
    app.world_mut().resource_mut::<PadInput>().attack_held = false;
    run(&mut app, 220);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    run(&mut app, 5);
    assert_eq!(wolf(&mut app).0, "SwimAttackCombo1Release", "a tap: the cut");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    run(&mut app, 40);
    assert!(wolf(&mut app).0.starts_with("SwimAttackCombo2"), "the second press chains: {}", wolf(&mut app).0);
    run(&mut app, 200);
    // The gourd (the quick slot's first item): hurt, drink, healed.
    {
        let world = app.world_mut();
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.hp = a.hp_max * 0.3;
    }
    let hp0 = wolf(&mut app).1;
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    run(&mut app, 5);
    assert_eq!(wolf(&mut app).0, "SwimItemGourdDrink");
    run(&mut app, 90);
    assert!(wolf(&mut app).1 > hp0, "healed: {} -> {}", hp0, wolf(&mut app).1);
    crate::swim::WATER.write().unwrap().retain(|w| w.0 != lo);
}

/// The death screen's choice (the user, 2026-10-18): with a node left, guard is ": Die" - no
/// resurrection, the node kept, attack no longer revives; the default is two nodes.
#[test]
pub(crate) fn guard_on_the_death_choice_dies() {
    assert_eq!(crate::config::PlayerConfig::default().resurrections, 2, "the base node and the extra one");
    let mut app = app();
    move_enemy_far(&mut app);
    {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    }
    for _ in 0..300 {
        app.update();
        let world = app.world_mut();
        if world.query::<&Player>().single(world).unwrap().revive_open {
            break;
        }
    }
    let nodes = {
        let world = app.world_mut();
        let p = world.query::<&Player>().single(world).unwrap();
        assert!(p.revive_open, "the choice opens");
        p.resurrections
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::Guard);
    run(&mut app, 5);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    run(&mut app, 30);
    let world = app.world_mut();
    let (a, p) = world.query::<(&Actor, &Player)>().single(world).unwrap();
    assert!(p.gave_up && !p.revive_open, "died");
    assert_eq!(p.resurrections, nodes, "the node is kept");
    assert!(a.hp <= 0.0 && !a.state.contains("Revival_"), "no resurrection: {} {}", a.state, a.hp);
}

/// After a resurrection the node left is locked (the user, 2026-10-18): a second death has no
/// choice; a deathblow unlocks it.
#[test]
pub(crate) fn a_resurrection_locks_the_next_until_a_deathblow() {
    let mut app = app();
    move_enemy_far(&mut app);
    let kill = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hp = 0.0;
    };
    let open = |app: &mut App| {
        let world = app.world_mut();
        world.query::<&Player>().single(world).unwrap().revive_open
    };
    kill(&mut app);
    for _ in 0..300 {
        app.update();
        if open(&mut app) {
            break;
        }
    }
    assert!(open(&mut app), "the first death offers the choice");
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    run(&mut app, 200);
    {
        let world = app.world_mut();
        let (a, p) = world.query::<(&Actor, &Player)>().single(world).unwrap();
        assert!(a.hp > 0.0 && p.revive_locked && p.resurrections == 1, "revived, the other node locked");
    }
    kill(&mut app);
    let mut offered = false;
    for _ in 0..400 {
        app.update();
        offered |= open(&mut app);
    }
    assert!(!offered, "locked: no resurrection");
    // A deathblow (the throw's state) unlocks it.
    {
        let world = app.world_mut();
        let (mut a, mut p) = world.query::<(&mut Actor, &mut Player)>().single_mut(world).unwrap();
        a.hp = a.hp_max;
        a.play("Deathblow", "");
        p.revive_locked = true;
    }
    run(&mut app, 2);
    let world = app.world_mut();
    assert!(!world.query::<&Player>().single(world).unwrap().revive_locked, "a deathblow unlocks it");
}

/// The Construct's floor is cut round the pool and the chasm: no rectangle covers either hole
/// and the rest is all floor.
#[test]
fn the_construct_floor_leaves_its_holes_open() {
    let holes = [crate::sandbox::POOL, crate::sandbox::CHASM];
    let rects = crate::sandbox::floor_rects_for_test((-45.0, 45.0, -135.0, 45.0), &holes);
    let on_floor = |x: f32, z: f32| rects.iter().any(|r| r.0 <= x && x <= r.1 && r.2 <= z && z <= r.3);
    assert!(!on_floor(-30.0, 0.0), "the pool");
    assert!(!on_floor(26.0, -20.0) && !on_floor(26.0, 5.0), "the chasm");
    assert!(on_floor(26.0, 20.0) && on_floor(26.0, -60.0) && on_floor(0.0, 0.0) && on_floor(40.0, 0.0), "the rest");
    let area: f32 = rects.iter().map(|r| (r.1 - r.0) * (r.3 - r.2)).sum();
    assert!((area - (90.0 * 180.0 - 20.0 * 16.0 - 10.0 * 54.0)).abs() < 1e-2, "area {area}");
}

/// Uses quick item `goods` (alone in slot 0, `n` held) through its clip and items.rs apply_used;
/// returns the states seen.
fn use_goods(app: &mut App, goods: i64, n: u32, frames: usize) -> Vec<String> {
    {
        let mut config = app.world_mut().resource_mut::<crate::config::GameConfig>();
        config.player.items.retain(|(g, _)| *g != goods);
        config.player.items.push((goods, n));
        config.player.quick_items = vec![goods, 0, 0, 0, 0];
    }
    app.world_mut().init_resource::<crate::items::Used>();
    {
        let world = app.world_mut();
        world.query::<&mut Player>().single_mut(world).unwrap().quick_slot = 0;
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    let mut states = Vec::new();
    for _ in 0..frames {
        app.update();
        let (s, ..) = player_state(app);
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    states
}

/// The coin purses give their Sen; the Ceremonial Tanto takes half the max HP for five Spirit
/// Emblems (the user, 2026-10-18: "Items").
#[test]
pub(crate) fn coin_purses_and_the_tanto_pay_out() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.add_systems(Update, crate::items::apply_used);
    let sen = app.world().resource::<crate::config::GameConfig>().player.sen;
    let states = use_goods(&mut app, 3708, 1, 120);
    assert!(states.iter().any(|s| s == "ItemCoinBag"), "{states:?}");
    assert_eq!(app.world().resource::<crate::config::GameConfig>().player.sen, sen + 1000, "the Bulging Coin Purse");
    {
        let world = app.world_mut();
        world.query::<&mut Player>().single_mut(world).unwrap().emblems = 0;
    }
    let states = use_goods(&mut app, 3800, 1, 150);
    assert!(states.iter().any(|s| s == "ItemPaperDollExchangeWhite"), "{states:?}");
    let world = app.world_mut();
    let (a, p) = world.query::<(&Actor, &Player)>().single(world).unwrap();
    assert!((a.hp - a.hp_max * 0.5).abs() < 1.0, "half the HP: {} / {}", a.hp, a.hp_max);
    assert_eq!(p.emblems, 5);
}

/// Dousing Powder puts out a burn and keeps the next build-up lower for its 30 s.
#[test]
pub(crate) fn dousing_powder_puts_out_the_burn() {
    // c1550's data carries the burn row 9115.
    let mut app = app_cfg("c1550", None, Vec::new());
    move_enemy_far(&mut app);
    app.add_systems(Update, crate::items::apply_used);
    app.world_mut().init_resource::<crate::items::Used>();
    app.update();
    let wolf = app.world_mut().query_filtered::<Entity, With<Player>>().single(app.world()).unwrap();
    let burning = |app: &mut App| {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut st = world.resource_mut::<crate::status::Statuses>();
            (1..=20).find(|_| st.hit_wolf(&combat, wolf, 5000, &[], &[9115], crate::status::Guard::None).iter().any(|l| l == "BURNING"))
        })
    };
    let plain = burning(&mut app).expect("burns");
    let on = |app: &mut App| app.world().resource::<crate::status::Statuses>().0.get(&wolf).is_some_and(|s| !s.effects.is_empty());
    assert!(on(&mut app));
    let states = use_goods(&mut app, 3210, 1, 120);
    assert!(states.iter().any(|s| s.starts_with("ItemPowderMedicine")), "{states:?}");
    assert!(!on(&mut app), "put out");
    // Its 3212 (defBloodResistDamageRate 0.65) worn: more hits to burn again.
    let worn = {
        let world = app.world_mut();
        world.query::<&Actor>().iter(world).find(|a| !a.item_sp.is_empty()).map(|a| a.item_sp.clone()).unwrap_or_default()
    };
    assert!(worn.contains(&3212), "{worn:?}");
    let resisted = {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut st = world.resource_mut::<crate::status::Statuses>();
            st.clear(wolf);
            (1..=40).find(|_| st.hit_wolf(&combat, wolf, 5000, &worn, &[9115], crate::status::Guard::None).iter().any(|l| l == "BURNING"))
        })
    };
    assert!(resisted.expect("still burns") > plain, "{plain} hits plain, {resisted:?} resisted");
}

/// A Fistful of Ash thrown at the enemy ahead: its cloud blinds him (the firecracker's flinch);
/// the Ceramic Shard flies as its Bullet 3491.
#[test]
pub(crate) fn thrown_ash_blinds_and_the_shard_flies() {
    let mut app = app();
    app.add_systems(Update, crate::items::apply_used);
    // The enemy 3 m ahead, passive.
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Transform, &mut crate::enemy::Enemy), Without<Player>>();
        for (mut t, _) in q.iter_mut(world) {
            t.translation = Vec3::new(0.0, t.translation.y, 1.0);
        }
    }
    let mut reacted = false;
    use_goods(&mut app, 3510, 1, 1);
    for _ in 0..150 {
        app.update();
        let world = app.world_mut();
        reacted |= world.query_filtered::<&Actor, Without<Player>>().iter(world).any(|a| a.state == "AssassinationBloodReaction");
    }
    assert!(reacted, "the ash blinds");
    let mut flew = false;
    use_goods(&mut app, 3500, 1, 1);
    for _ in 0..90 {
        app.update();
        let world = app.world_mut();
        flew |= world.query::<&Name>().iter(world).any(|n| n.as_str() == "Bullet 3491");
    }
    assert!(flew, "the shard flies");
}

/// The Snap Seed (goods 3530) beside one of Lady Butterfly's phantoms (c1510, NpcParam 6023:
/// ref 1000043): its Bullet 3530 undoes it (W_DeathStartBackReality, a000_010360 / 10361).
#[test]
pub(crate) fn the_snap_seed_undoes_a_phantom() {
    let mut app = app_cfg("c1510", None, Vec::new());
    app.add_systems(Update, crate::items::apply_used);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, (With<crate::enemy::Enemy>, Without<Player>)>();
        for mut t in q.iter_mut(world) {
            t.translation = Vec3::new(0.0, t.translation.y, 6.0);
        }
    }
    let mut died = None;
    use_goods(&mut app, 3530, 1, 1);
    for _ in 0..240 {
        app.update();
        let world = app.world_mut();
        if let Some(a) = world.query_filtered::<&Actor, Without<Player>>().iter(world).find(|a| a.state.starts_with("DeathStartBackReality")) {
            died = Some(a.anim.clone());
            break;
        }
    }
    assert!(matches!(died.as_deref(), Some("a000_010360" | "a000_010361")), "{died:?}");
    // A real villager (c1500, no 6023) beside him is not touched.
    let mut app = app_cfg("c1500", None, Vec::new());
    app.add_systems(Update, crate::items::apply_used);
    use_goods(&mut app, 3530, 1, 120);
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, Without<Player>>().single(world).unwrap();
    assert!(a.hp > 0.0 && !a.state.starts_with("Death"), "{} {}", a.state, a.hp);
}

/// Every usable item of the starter bag (config.toml): its use state plays its clip to the end,
/// back to standing, and one is spent (the Homeward Idol is not consumed; it warps in the game).
/// The user, 2026-10-18: "make sure again every item works and has proper animations".
#[test]
pub(crate) fn every_bag_item_plays_its_use_and_is_spent() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.add_systems(Update, crate::items::apply_used);
    let bag: Vec<i64> = crate::config::GameConfig::load().unwrap().player.items.iter().map(|(g, _)| *g).collect();
    let mut tried = 0;
    for goods in bag {
        let anim = crate::items::param("EquipParamGoods", goods)["goodsUseAnim"].as_i64().unwrap_or(255);
        let Some(state) = crate::items::use_state(anim) else { continue };
        if goods == 3980 {
            continue;
        }
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            a.hp = a.hp_max;
        }
        let states = use_goods(&mut app, goods, 2, 330);
        tried += 1;
        assert!(states.iter().any(|s| s == state), "goods {goods}: no {state} in {states:?}");
        let (s, clip, ..) = player_state(&mut app);
        assert!(!s.contains("Item"), "goods {goods}: still in {s} ({clip}) after 5.5 s");
        let left = crate::items::count(app.world().resource::<crate::config::GameConfig>(), goods);
        assert_eq!(left, 1, "goods {goods} ({state}): spent once");
    }
    assert!(tried >= 20, "{tried}");
}

/// Crouched, the items with a Crouch use state (CrouchItemPillTonic ...) play that one and
/// Wolf stays crouched after.
#[test]
pub(crate) fn crouched_items_play_their_crouch_clips() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.add_systems(Update, crate::items::apply_used);
    app.world_mut().init_resource::<crate::items::Used>();
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..120 {
        app.update();
    }
    for (goods, state) in [(3020, "CrouchItemPillTonic"), (3060, "CrouchItemOhagi"), (3070, "CrouchItemKaki"), (3200, "CrouchItemPowderMedicineRecoverPoison"), (3500, "CrouchItemPottery"), (3510, "CrouchItemStone"), (3700, "CrouchItemCoinBag"), (3800, "CrouchItemPaperDollExchangeWhite")] {
        let states = use_goods(&mut app, goods, 2, 300);
        assert!(states.iter().any(|s| s == state), "goods {goods}: no {state} in {states:?}");
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert!(a.crouch && !a.state.contains("Item"), "goods {goods}: {} crouch {}", a.state, a.crouch);
    }
}

/// Yashariku's Sugar halves Wolf's max HP and max posture for its 30 s (3411 maxHpRate and
/// maxStaminaRate 0.5), HP held under it, and the max comes back when it ends.
#[test]
pub(crate) fn yasharikus_sugar_halves_the_max_hp() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.add_systems(Update, crate::items::apply_used);
    let (full, full_posture) = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        (a.hp_max, a.posture_max)
    };
    use_goods(&mut app, 3410, 1, 240);
    {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
        assert!((a.hp_max - full * 0.5).abs() < 0.01, "{} of {full}", a.hp_max);
        assert!(a.hp <= a.hp_max);
        // maxStaminaRate 0.5: the posture bar halves too.
        assert!((a.posture_max - full_posture * 0.5).abs() < 0.01, "{} of {full_posture}", a.posture_max);
        assert!(a.posture <= a.posture_max);
    }
    // Its 30 s run out.
    {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.timed.retain(|(id, _)| *id != 3411);
    }
    app.update();
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
    assert!((a.hp_max - full).abs() < 0.01, "{} back to {full}", a.hp_max);
    assert!((a.posture_max - full_posture).abs() < 0.01, "{} back to {full_posture}", a.posture_max);
    assert!(a.hp <= full * 0.5 + 0.01, "HP is not given back: {}", a.hp);
}

/// The thrown items fly where Wolf faces: their first rows (3490 / 3510 / 3520) stand still and
/// hand their aim on (it went world -Z whatever his facing; the user, 2026-10-18: "ceramic shard
/// doesn't get thrown where player faces").
#[test]
pub(crate) fn throwables_fly_where_wolf_faces() {
    for (goods, flier) in [(3500, "Bullet 3491"), (3510, "Bullet 3511"), (3520, "Bullet 3521")] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.add_systems(Update, crate::items::apply_used);
        app.world_mut().init_resource::<crate::items::Used>();
        app.update();
        {
            let world = app.world_mut();
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            // Facing +X.
            a.yaw = -std::f32::consts::FRAC_PI_2;
        }
        use_goods(&mut app, goods, 1, 1);
        let mut seen = None;
        for _ in 0..90 {
            app.update();
            let world = app.world_mut();
            if let Some(t) = world.query::<(&Name, &Transform)>().iter(world).find(|(n, _)| n.as_str() == flier).map(|(_, t)| *t) {
                seen = Some(t.rotation * Vec3::NEG_Z);
                break;
            }
        }
        let d = seen.unwrap_or_else(|| panic!("goods {goods}: no {flier}"));
        assert!(d.with_y(0.0).normalize().dot(Vec3::X) > 0.9, "goods {goods}: {flier} flies {d:.2}, not +X");
    }
}

/// A killed enemy's lots (drops.rs): the sim's General (NpcParam 10202060) leaves his Spirit
/// Emblems (ResourceItemLotParam 10000100: 2 at 80 %, 5 at 20 %) as a pickup and his 12
/// Resurrective Power (10005100, 100 %) goes to a used node (the user, 2026-10-18: "fill in the
/// gaps").
#[test]
pub(crate) fn a_kill_drops_emblems_and_resurrective_power() {
    let mut app = app();
    app.add_systems(Update, crate::drops::drop_loot);
    {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.resurrections = 0;
        p.res_power = 0;
    }
    let (_, died) = break_and_deathblow_died(&mut app, 200);
    assert!(died);
    for _ in 0..5 {
        app.update();
    }
    let emblems = crate::pickups::PICKUPS.read().unwrap().iter().any(|p| p.goods == crate::items::SPIRIT_EMBLEM && matches!(p.count, 2 | 5) && !p.taken);
    assert!(emblems, "no emblems lying");
    let world = app.world_mut();
    let p = world.query::<&Player>().single(world).unwrap();
    assert_eq!((p.resurrections, p.res_power), (0, 12));
}

/// A death a map event forces with shouldReceiveSouls off (2004[4], Enemy::unrewarded) pays
/// nothing: the General's 100 % Resurrective Power lot is not drawn.
#[test]
pub(crate) fn an_unpaid_event_death_drops_nothing() {
    let mut app = app();
    app.add_systems(Update, crate::drops::drop_loot);
    {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.resurrections = 0;
        p.res_power = 0;
        world.query::<&mut crate::enemy::Enemy>().single_mut(world).unwrap().unrewarded = true;
    }
    let (_, died) = break_and_deathblow_died(&mut app, 200);
    assert!(died);
    for _ in 0..5 {
        app.update();
    }
    let world = app.world_mut();
    let p = world.query::<&Player>().single(world).unwrap();
    assert_eq!((p.resurrections, p.res_power), (0, 0));
}

/// Five-color Rice: a use lets its rice fall (Bullet 3580) and spends one of five; run out, the
/// empty shake; an idol rest fills it again.
#[test]
pub(crate) fn five_color_rice_falls_runs_out_and_refills() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.add_systems(Update, (crate::items::apply_used, crate::items::refill_rested));
    app.world_mut().init_resource::<crate::items::Used>();
    let mut fell = false;
    let states = use_goods(&mut app, 3580, 5, 1);
    for _ in 0..120 {
        app.update();
        let world = app.world_mut();
        fell |= world.query::<&Name>().iter(world).any(|n| n.as_str() == "Bullet 3580");
    }
    let _ = states;
    assert!(fell, "the rice falls");
    assert_eq!(crate::items::count(app.world().resource::<crate::config::GameConfig>(), 3580), 4);
    let states = use_goods(&mut app, 3580, 0, 150);
    assert!(states.iter().any(|s| s == "ItemColoredRiceEmpty"), "{states:?}");
    {
        let world = app.world_mut();
        world.query::<&mut Player>().single_mut(world).unwrap().refill_items = true;
    }
    app.update();
    assert_eq!(crate::items::count(app.world().resource::<crate::config::GameConfig>(), 3580), 5);
}

/// Divine Confetti against an apparition: the Shichimen Warrior's ghost barrier (230641, x0.33)
/// cuts Wolf's HP damage; worn, the Confetti's spirit (3451) erases it, posture too.
#[test]
pub(crate) fn divine_confetti_erases_the_ghost_barrier() {
    let mut app = app_cfg("c1080", None, Vec::new());
    app.update();
    let world = app.world_mut();
    let a = world.query_filtered::<&Actor, Without<Player>>().iter(world).next().unwrap().clone();
    let combat = world.resource::<Combat>();
    // One fixed attack (the map's order is random; not every row is a plain sword hit).
    let atk = combat.player.attacks.iter().min_by_key(|(k, _)| k.as_str()).unwrap().1.clone();
    let (hp, posture) = crate::combat::ghost_barrier(combat, &a, &atk, &[]);
    assert!((hp - 0.33).abs() < 0.01 && posture == 1.0, "{hp} {posture}");
    let (hp, posture) = crate::combat::ghost_barrier(combat, &a, &atk, &[3451]);
    assert!(hp == 1.0 && (posture - 1.0 / 0.33).abs() < 0.05, "{hp} {posture}");
}
