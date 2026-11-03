use glam::Vec3;
use sekiro_sim::body::{Body, Ground};
use sekiro_sim::docking::{self, Docking, MapEdge};
use sekiro_sim::duel::Duel;
use sekiro_sim::{Buttons, InputFrame};
use std::path::PathBuf;

/// A cliff face along X at z = 0: open floor at y = 0 for z > 0, the top `LIP` metres up behind
/// it (z < 0). The face stops anything below the lip from crossing z = 0.3 (the capsule).
const LIP: f32 = 3.5;
struct Cliff;
impl Ground for Cliff {
    fn height(&self, _: f32, z: f32) -> Option<f32> {
        Some(if z < 0.0 { LIP } else { 0.0 })
    }
    fn floor(&self, _: f32, z: f32, top: f32) -> Option<f32> {
        if z < 0.0 && top >= LIP {
            Some(LIP)
        } else if z < 0.0 {
            None
        } else {
            (top >= 0.0).then_some(0.0)
        }
    }
    fn slide(&self, feet: [f32; 3], delta: [f32; 2]) -> [f32; 2] {
        if feet[1] >= LIP - 0.01 {
            return delta;
        }
        [delta[0], (feet[2] + delta[1]).max(0.3) - feet[2]]
    }
}
fn cliff_edges() -> Docking {
    Docking::new(vec![MapEdge {
        kind: 2,
        variation: 0,
        start: [-3.0, LIP, 0.0],
        end: [3.0, LIP, 0.0],
        source_part: "cliff".into(),
        chain: 0,
    }])
}
fn cache() -> PathBuf {
    sekiro_sim::cache::root()
}
fn press(buttons: Buttons) -> InputFrame {
    InputFrame {
        buttons,
        ..InputFrame::default()
    }
}

#[test]
fn wall_contact_angle_by_side() {
    let feet = Vec3::new(0.0, 0.5, 0.5);
    // Facing the wall (-Z is forward at yaw 0): straight ahead.
    assert_eq!(docking::wall_contact(feet, 0.0, &Cliff), Some(0.0));
    // Facing +X the wall (-Z) is on the left; facing -X on the right; facing away, behind.
    let left = docking::wall_contact(feet, -std::f32::consts::FRAC_PI_2, &Cliff).unwrap();
    assert!((left + 90.0).abs() < 1e-3, "{left}");
    let right = docking::wall_contact(feet, std::f32::consts::FRAC_PI_2, &Cliff).unwrap();
    assert!((right - 90.0).abs() < 1e-3, "{right}");
    let back = docking::wall_contact(feet, std::f32::consts::PI, &Cliff).unwrap();
    assert!((back.abs() - 180.0).abs() < 1e-3, "{back}");
    // Out of reach, and above the top.
    assert_eq!(
        docking::wall_contact(Vec3::new(0.0, 0.5, 1.5), 0.0, &Cliff),
        None
    );
    assert_eq!(
        docking::wall_contact(Vec3::new(0.0, LIP, 0.5), 0.0, &Cliff),
        None
    );
}

#[test]
fn air_catch_band_and_range() {
    let mut docking = cliff_edges();
    let mut body = Body {
        position: [0.0, LIP - 1.8, 0.5],
        grounded: false,
        ..Body::default()
    };
    docking.search(&body, &Cliff);
    assert_eq!(docking.air_hang_type, docking::AIR_HANG_TYPE_SHORT);
    assert_eq!(docking.candidate.as_ref().unwrap().outward, Vec3::Z);
    body.position[2] = 1.6;
    docking.search(&body, &Cliff);
    assert_eq!(docking.air_hang_type, docking::AIR_HANG_TYPE_LONG);
    // Lip too high, too low, or past the edge's end.
    for position in [
        [0.0, LIP - 3.0, 0.5],
        [0.0, LIP - 0.5, 0.5],
        [3.5, LIP - 1.8, 0.5],
    ] {
        body.position = position;
        docking.search(&body, &Cliff);
        assert_eq!(docking.air_hang_type, 0, "{position:?}");
        assert!(docking.candidate.is_none());
    }
    // Foot-hold edges (variation 1) are hang edges too.
    docking.edges[0].variation = 1;
    body.position = [0.0, LIP - 1.8, 0.5];
    docking.search(&body, &Cliff);
    assert_eq!(docking.air_hang_type, docking::AIR_HANG_TYPE_SHORT);
}

/// Jump at the cliff, jump again off it (the wall jump), hold the hang button to catch the
/// lip, then climb onto the top: the route issue #16 needs.
#[test]
fn hks_wall_jump_catches_the_lip_and_climbs() {
    if let Some(mut duel) = cliff_duel() {
        climb(&mut duel, Hold::AfterLaunch);
        assert!((duel.player.body.position[1] - LIP).abs() < 0.01);
        assert!(duel.player.body.position[2] < 0.0, "not on the top");
    }
}

/// The same with the hang button already held through the wall jump's take-off.
#[test]
fn hks_wall_jump_with_hang_held_early() {
    if let Some(mut duel) = cliff_duel() {
        climb(&mut duel, Hold::FromWallJump);
        assert!((duel.player.body.position[1] - LIP).abs() < 0.01);
    }
}

fn cliff_duel() -> Option<Duel> {
    if !cache().join("raw/chr/c1010.behbnd.d/c9997.hkx").exists() {
        return None;
    }
    let mut duel = Duel::load(&cache(), 4.0).unwrap();
    duel.enemy_active = false;
    duel.player.ground = Box::new(Cliff);
    duel.player.docking = cliff_edges();
    duel.player.body.position = [0.0, 0.0, 0.5];
    duel.player.body.yaw = 0.0;
    Some(duel)
}

/// Stands still, jumps, wall jumps on the way down, holds the hang button once the take-off
/// launched (or earlier, see [`Hold`]), waits for HangIdle and climbs up. Ends
/// on the top, where the climb's target floor is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hold {
    /// E pressed once the wall jump launched.
    AfterLaunch,
    /// E held from the wall jump on.
    FromWallJump,
    /// E held from the first jump on.
    Always,
}

fn in_air_grab(duel: &Duel) -> bool {
    duel.player
        .behavior
        .runtime
        .main_clip()
        .is_some_and(|c| c.animation.starts_with("a000_2177"))
}

fn climb(duel: &mut Duel, hold: Hold) {
    let dt = 1.0 / 60.0;
    for _ in 0..30 {
        duel.step(&InputFrame::default(), dt);
    }
    let start = duel.player.body.position[1];
    let jump = Buttons {
        jump: true,
        ..Buttons::default()
    };
    let hang = Buttons {
        action: true,
        ..Buttons::default()
    };
    let mut wall_jumped = false;
    let mut launched = false;
    let mut caught = false;
    let mut peak = f32::MIN;
    duel.step(&press(jump), dt);
    for _ in 0..240 {
        let airborne = !duel.player.body.grounded;
        let mut buttons = if !wall_jumped && airborne && duel.player.body.velocity[1] < 0.0 {
            jump
        } else if launched || (hold != Hold::AfterLaunch && wall_jumped) {
            hang
        } else {
            Buttons::default()
        };
        // Held from the first jump on, E starts the air grab (AirHangGrab*) before any lip is
        // in reach; that must not dock Wolf.
        buttons.action |= hold == Hold::Always;
        if !caught && in_air_grab(duel) {
            assert!(
                duel.player.docking.active.is_none(),
                "docked in the air grab"
            );
        }
        let r = duel.step(&press(buttons), dt);
        peak = peak.max(duel.player.body.position[1] - start);
        wall_jumped |= r
            .player
            .behavior
            .events
            .iter()
            .any(|e| e == "W_WallJumpReady");
        launched |= wall_jumped && duel.player.body.velocity[1] > 0.0;
        caught |= duel.player.docking.active.is_some()
            && r.player
                .animation
                .as_deref()
                .is_some_and(|a| a.starts_with("a000_2171") || a.starts_with("a000_2172"));
        if caught {
            break;
        }
    }
    assert!(wall_jumped, "no wall jump; peak {peak}");
    assert!(caught, "no air catch; peak {peak}");
    let lip = duel.player.docking.active.as_ref().unwrap().point.y;
    // FreeFallToHang (a000_217120) is 3.3 s long and hands over to HangIdle at its end.
    let mut idle = false;
    for _ in 0..240 {
        let r = duel.step(&InputFrame::default(), dt);
        idle |= matches!(
            r.player.animation.as_deref(),
            Some("a000_020000" | "a000_020010")
        );
        if idle {
            break;
        }
    }
    assert!(idle, "did not reach HangIdle");
    let target = duel.player.docking.active.clone().expect("docked");
    assert!((duel.player.body.position[1] - (lip - docking::HANG_DEPTH)).abs() < 0.01);
    // A player presses E a moment after the hang settles, not on its first frame.
    for _ in 0..30 {
        duel.step(&InputFrame::default(), dt);
    }
    // The hands rest on the lip and the pelvis stays out of the face.
    let bones = duel.player.bone_source_matrices();
    let world = duel.player.source_to_world();
    let at = |name: &str| {
        let i = duel.player.skeleton.bone_index(name).unwrap();
        world.transform_point3(bones[i].transform_point3(Vec3::ZERO))
    };
    for hand in ["L_Hand", "R_Hand"] {
        let p = at(hand);
        assert!((p.y - lip).abs() < 0.1, "{hand} {p:?} lip {lip}");
        assert!(
            (p - target.point).dot(target.outward).abs() < 0.1,
            "{hand} {p:?} off the lip line {:?}",
            target.point
        );
    }
    let pelvis = at("Pelvis");
    assert!(
        (pelvis - target.point).dot(target.outward) > 0.05,
        "pelvis in the face: {pelvis:?}"
    );
    let r = duel.step(&press(hang), dt);
    assert!(
        matches!(
            r.player.animation.as_deref(),
            Some("a000_217500" | "a000_217600")
        ),
        "{:?}",
        r.player.animation
    );
    for _ in 0..180 {
        duel.step(&InputFrame::default(), dt);
    }
    assert!(duel.player.body.grounded);
    assert!(duel.player.docking.active.is_none());
    // On the top, in from the lip by the climb's own root motion.
    let feet = Vec3::from(duel.player.body.position);
    assert!((feet - target.point).dot(target.outward) < -0.3, "{feet:?}");
    assert!((feet.y - lip).abs() < 0.3, "{feet:?}");
}

/// The rock wall just after the grapple tree from m11_00 start 0 (issue #16): its hang edge,
/// h023000 chain 2 (a foot-hold edge, variation 1), sits on the collision with a drop in front,
/// and the airborne search finds it from a jump at the face.
#[test]
fn m11_00_wall_after_grapple_tree_has_a_catchable_ledge() {
    let dir = cache().join("maps/m11_00_00_00");
    if !dir.join("edges.json").exists() || !dir.join("collision.bin").exists() {
        return;
    }
    let mut docking = Docking::load(&dir.join("edges.json")).unwrap();
    let world = sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap();
    let (index, edge) = docking
        .edges
        .iter()
        .enumerate()
        .find(|(_, e)| e.source_part == "h023000" && e.chain == 2 && e.kind == 2)
        .expect("h023000 chain 2 hang edge");
    assert_eq!(edge.variation, 1);
    let mid = (Vec3::from(edge.start) + Vec3::from(edge.end)) * 0.5;
    let target = docking
        .target(index, mid, &world)
        .expect("ledge on collision");
    let feet = mid + target.outward * 0.5;
    let floor = world
        .floor(feet.x, feet.z, mid.y - 0.5)
        .expect("floor under the face");
    assert!(mid.y - floor > 4.0, "a wall, not a step: {}", mid.y - floor);
    let yaw = target.yaw();
    assert!(
        docking::wall_contact(Vec3::new(feet.x, floor, feet.z), yaw, &world)
            .is_some_and(|a| a.abs() < 45.0),
        "no wall in front at the foot"
    );
    let body = Body {
        position: [feet.x, mid.y - 1.8, feet.z],
        yaw,
        grounded: false,
        ..Body::default()
    };
    docking.search(&body, &world);
    let candidate = docking.candidate.as_ref().expect("air catch candidate");
    assert_eq!(candidate.edge, index);
    assert_eq!(docking.air_hang_type, docking::AIR_HANG_TYPE_SHORT);
}

/// The whole route on the real collision: from the foot of that wall, jump, wall jump, catch
/// the lip and climb onto the top.
#[test]
fn m11_00_wall_after_grapple_tree_route() {
    for hold in [Hold::AfterLaunch, Hold::Always] {
        m11_00_route(hold);
    }
}

fn m11_00_route(hold: Hold) {
    let dir = cache().join("maps/m11_00_00_00");
    let Some(mut duel) = cliff_duel() else {
        return;
    };
    if !dir.join("edges.json").exists() {
        return;
    }
    let world = sekiro_sim::collision::CollisionWorld::load_map(&dir).unwrap();
    let docking = Docking::load(&dir.join("edges.json")).unwrap();
    let (index, edge) = docking
        .edges
        .iter()
        .enumerate()
        .find(|(_, e)| e.source_part == "h023000" && e.chain == 2 && e.kind == 2)
        .unwrap();
    let mid = (Vec3::from(edge.start) + Vec3::from(edge.end)) * 0.5;
    let target = docking.target(index, mid, &world).unwrap();
    let feet = mid + target.outward * 0.5;
    let floor = world.floor(feet.x, feet.z, mid.y - 0.5).unwrap();
    duel.player.body.position = [feet.x, floor, feet.z];
    duel.player.body.yaw = target.yaw();
    duel.player.ground = Box::new(world);
    duel.player.docking = docking;
    climb(&mut duel, hold);
    assert!(
        duel.player.body.position[1] > mid.y - 0.3,
        "{:?}",
        duel.player.body.position
    );
}
