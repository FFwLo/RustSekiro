//! The full character (input, script, behaviour graph, TAE, body) through the built-in demo.
//! Skips when `cache/` has not been extracted.

use sekiro_sim::{PlayerCharacter, demo};

#[test]
fn demo_runs_jumps_attacks_and_guards() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c0000.behbnd.d/c0000.hkx").exists() {
        eprintln!("skipping: cache not extracted");
        return;
    }
    let mut wolf = PlayerCharacter::load_from_cache(&cache).unwrap();
    let dt = 1.0 / 60.0;
    assert!(wolf.settle(dt, 400) < 400);
    let frames = demo::parse(demo::DEMO).unwrap();
    let mut states: Vec<String> = Vec::new();
    let mut airborne = 0;
    let mut max_height: f32 = 0.0;
    for f in &frames {
        let r = wolf.tick(f, dt);
        assert!(
            r.behavior.script_errors.is_empty(),
            "{:?}",
            r.behavior.script_errors
        );
        let s = r.behavior.state_path.last().cloned().unwrap_or_default();
        if states.last() != Some(&s) {
            states.push(s);
        }
        if !r.grounded {
            airborne += 1;
        }
        max_height = max_height.max(r.position[1]);
    }
    let seen = |name: &str| states.iter().any(|s| s == name);
    for expected in [
        "StandMoveStart",
        "SprintStartFromStep",
        "GroundJumpStart",
        "LandGroundJump",
        "GroundAttackCombo1",
        "GroundAttackCombo2",
        "GroundAttackCombo3",
        "StandToDeflectGuard",
        "DeflectGuardIdle",
        "GroundStep",
        "CrouchStart",
        "StandIdle",
    ] {
        assert!(seen(expected), "{expected} missing from {states:?}");
    }
    // The jump leaves the ground for about a second and rises a couple of metres.
    assert!((40..80).contains(&airborne), "airborne {airborne} steps");
    assert!(max_height > 1.5 && max_height < 3.0, "apex {max_height}");
    // Root motion moved the character.
    let p = wolf.body.position;
    assert!(p[0].abs() + p[2].abs() > 10.0, "{p:?}");
}
