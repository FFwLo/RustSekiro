//! Sim tests, part 12: wall hug and ledge hang (docking.rs, docs/kb/docking.md), on synthetic
//! edges over the flat test floor.

use super::*;

/// A wall along X at z = 0 whose face looks to +Z (`c` on the -Z, wall side); `y` the edge
/// height.
fn edges(kind: u8, y: f32) -> crate::map::LedgeEdges {
    crate::map::LedgeEdges(vec![crate::map::LedgeEdge { a: Vec3::new(-3.0, y, 0.0), b: Vec3::new(3.0, y, 0.0), c: Vec3::new(0.0, y - 0.2, -0.1), kind, variation: 0 }])
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
    // Climb.
    app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
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
    let e1 = crate::map::LedgeEdge { a: Vec3::new(-3.0, 4.0, 0.0), b: Vec3::new(3.0, 4.0, 0.0), c: Vec3::new(0.0, 3.8, -0.1), kind: 2, variation: 0 };
    let e2 = crate::map::LedgeEdge { a: Vec3::new(3.0, 4.0, 0.0), b: Vec3::new(3.0, 4.0, -6.0), c: Vec3::new(2.9, 3.8, -3.0), kind: 2, variation: 0 };
    app.world_mut().insert_resource(crate::map::LedgeEdges(vec![e1, e2]));
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

/// Hugging, an unaware Ochimusha passing at Wolf's side: the wall deathblow (ThrowParam 11010070
/// -> 0071 on his right: a200_500700 -> 510700, the enemy ThrowDef12700).
#[test]
pub(crate) fn an_unaware_enemy_passing_the_hugging_wolf_takes_the_wall_deathblow() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/anim_c1010.bin")).exists() {
        return;
    }
    let mut app = app_with("c1010");
    move_enemy_far(&mut app);
    app.world_mut().insert_resource(edges(3, 0.0));
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::new(0.0, 0.0, 1.0), 0.0, false);
    run(&mut app, 2);
    app.world_mut().resource_mut::<PadInput>().press(Action::Interact);
    run(&mut app, 90);
    assert_eq!(player_actor(&mut app).0, "CoverIdle");
    // Hugging, Wolf faces +Z; his right is -X. The enemy 1.2 m to his right, walking on away
    // from him (facing -X: yaw +90 deg).
    place_enemy(&mut app, Vec3::new(-1.2, 0.0, 1.2), std::f32::consts::FRAC_PI_2);
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
    println!("wall deathblow: {seen:?} / enemy {enemy_seen:?}");
    assert!(seen.iter().any(|s| s.starts_with("DeathblowStart a200_500700")), "{seen:?}");
    assert!(seen.iter().any(|s| s.starts_with("Deathblow a200_510700")), "{seen:?}");
    assert!(enemy_seen.iter().any(|s| s.starts_with("ThrowDef12700") || s.starts_with("ThrowDefDeath12701")), "{enemy_seen:?}");
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

/// Running, the stick thrown back: the run quick turn (StandRunQuickTurn*180) turns Wolf on the
/// spot instead of a wide circle at run speed, and he runs on the other way.
#[test]
pub(crate) fn a_run_reversed_quick_turns_instead_of_circling() {
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    place_wolf(&mut app, Vec3::ZERO, 0.0, false);
    // Run along -Z.
    app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(0.0, 1.0);
    run(&mut app, 90);
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

/// Off a ledge at a run Wolf carries his speed into the fall (it was zeroed: straight down).
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
        let mut q = w.query_filtered::<&mut Transform, With<Player>>();
        q.single_mut(w).unwrap().translation.y += 3.0;
    }
    let before = player_actor(&mut app).3;
    run(&mut app, 6);
    let (s, _, airborne, at) = player_actor(&mut app);
    let moved = (at - before).with_y(0.0).length();
    println!("walk-off: {s} airborne {airborne}, {moved:.2} m in 0.1 s");
    assert!(airborne && s == "FreeFall", "{s} {airborne}");
    assert!(moved > 0.3, "fell straight down: {moved:.2} m");
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

/// At a Sculptor's Idol: E kneels Wolf (StatueAttachStart -> Loop, resting him and asking for
/// the menu); the menu's Rest plays the rest clip, Travel the warp and stands him up at the
/// other idol. The test idols stand far off (x 600) as the idol list is global.
#[test]
pub(crate) fn wolf_kneels_at_an_idol_rests_and_travels() {
    use crate::idol::{Cmd, Idol, IDOLS};
    fn wolf<R>(app: &mut App, f: impl FnOnce(&mut crate::player::Player) -> R) -> R {
        let w = app.world_mut();
        let mut q = w.query::<&mut crate::player::Player>();
        f(&mut q.single_mut(w).unwrap())
    }
    let mut app = app();
    move_enemy_far(&mut app);
    run(&mut app, 10);
    let a = Idol { name: "A".into(), pos: Vec3::new(600.0, 0.0, 0.0), out: Vec3::X };
    let b = Idol { name: "B".into(), pos: Vec3::new(600.0, 0.0, 10.0), out: Vec3::NEG_Z };
    let (spot_a, spot_b) = (a.spot(), b.spot());
    *IDOLS.write().unwrap() = vec![a, b];
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
    run(&mut app, 80);
    let (s, _, _, at) = player_actor(&mut app);
    assert_eq!(s, "StatueAttachLoop");
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

/// A c1010 swing without ChrActionFlag 73 (3002) can be cut off mid-swing; one with it (3001)
/// is closed between its spans (0.03-0.33 s, 0.73 s on) and open inside them (the user: "i
/// cannot interrupt any of their attack").
#[test]
fn a_soldiers_plain_swing_can_be_interrupted() {
    let app = app_cfg("c1010", None, Vec::new());
    let combat = app.world().resource::<crate::data::Combat>();
    let d = &combat.kind(0).data;
    let mut a = Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 1.0, 0);
    let at = |a: &mut Actor, anim: &str, t: f32| {
        a.anim = anim.to_string();
        a.t = t;
    };
    at(&mut a, "a000_003002", 1.0);
    assert!(crate::combat::swing_can_flinch(d, &a), "3002 mid-swing");
    at(&mut a, "a000_003001", 0.5);
    assert!(!crate::combat::swing_can_flinch(d, &a), "3001 between its flag-73 spans");
    at(&mut a, "a000_003001", 1.0);
    assert!(crate::combat::swing_can_flinch(d, &a), "3001 in its recovery span");
    // Past its hit (0.47-0.6 s), before the recovery span opens at 0.73 s: open too.
    at(&mut a, "a000_003001", 0.65);
    assert!(crate::combat::swing_can_flinch(d, &a), "3001 after its hit");
}

/// The General's recovery after the blade has passed takes a flinch; his wind-up does not (the
/// user: "General's recovery move still has hyperarmor").
#[test]
fn the_generals_recovery_is_open() {
    let app = app_cfg("c1020", None, Vec::new());
    let combat = app.world().resource::<crate::data::Combat>();
    let d = &combat.kind(0).data;
    let mut a = Actor::new(crate::actor::Side::Enemy, 100.0, 100.0, 1.0, 0);
    a.anim = "a000_003000".to_string();
    // Hit 0.6-0.8 s, flag 73 spans 0.03-0.3 and 1.8 on.
    for (t, open) in [(0.1, true), (0.5, false), (0.7, false), (1.0, true), (1.5, true), (2.0, true)] {
        a.t = t;
        assert_eq!(crate::combat::swing_can_flinch(d, &a), open, "3000 at {t} s");
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
