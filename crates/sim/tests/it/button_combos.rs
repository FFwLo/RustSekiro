//! Several buttons at once: the character must keep animating and reach the right states.
//!
//! Regression for a playtest bug where holding two buttons (guard + move, sprint + attack)
//! froze the animation. The cause was the behaviour runtime: an event that two parallel
//! machines listen for (the upper- and lower-body machines of `StandMoveOverwrite`) was only
//! taken by one of them, machines in sync start mode ignored their shared sync variable (so
//! `StandMoveableAction_SM` started in the item-use state), and shared clip nodes were assigned
//! to the wrong state, leaving the full-body slot without a clip.
//! Skips when `cache/` has not been extracted.

use sekiro_sim::{Buttons, InputFrame, PlayerCharacter, demo};

fn load() -> Option<PlayerCharacter> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c0000.behbnd.d/c0000.hkx").exists() {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    let mut wolf = PlayerCharacter::load_from_cache(&cache).unwrap();
    assert!(wolf.settle(1.0 / 60.0, 400) < 400);
    Some(wolf)
}

const DT: f32 = 1.0 / 60.0;

#[test]
fn guard_with_move_and_sprint_with_attack() {
    let Some(mut wolf) = load() else { return };
    let script = "\
1 0 0 guard
20 0 0 guard
1 0 0 guard,attack
40 0 0 guard,attack
30 0 0 -
1 0 1 guard
40 0 1 guard
30 0 0 -
1 0 1 dodge
40 0 1 dodge
1 0 1 dodge,attack
40 0 1 dodge,attack
60 0 0 -
";
    let mut states = Vec::new();
    for (i, f) in demo::parse(script).unwrap().iter().enumerate() {
        let r = wolf.tick(f, DT);
        assert!(
            r.behavior.script_errors.is_empty(),
            "{:?}",
            r.behavior.script_errors
        );
        assert!(
            r.animation.is_some(),
            "no clip at step {i} in {:?}",
            r.behavior.state_path
        );
        let s = r.behavior.state_path.last().cloned().unwrap_or_default();
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    for expected in [
        "StandToDeflectGuard",
        "DeflectGuardAttack",
        "DeflectGuardMove",
        "DeflectGuardToStand",
        "SprintStartFromStep",
        "SprintAttack",
    ] {
        assert!(
            states.iter().any(|s| s == expected),
            "{expected} missing: {states:?}"
        );
    }
    assert!(
        !states.iter().any(|s| s.starts_with("Item")),
        "an item-use state started without the item button: {states:?}"
    );
}

/// Random held-button combinations: every step must have a clip and no script error.
#[test]
fn random_button_mash_never_loses_the_clip() {
    let Some(mut wolf) = load() else { return };
    let mut seed: u64 = 0x5eed_1234_abcd_ef01;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut frame = InputFrame::default();
    for step in 0..3000 {
        if step % 12 == 0 {
            let r = next();
            let bit = |n: u32| r >> n & 1 == 1;
            frame.buttons = Buttons {
                attack: bit(0),
                guard: bit(1),
                jump: bit(2) && bit(3),
                dodge: bit(4),
                crouch: bit(5) && bit(6),
                ..Buttons::default()
            };
            let dir = (r >> 8 & 7) as f32 * std::f32::consts::FRAC_PI_4;
            frame.move_stick = if bit(11) {
                [dir.sin(), dir.cos()]
            } else {
                [0.0, 0.0]
            };
        }
        let r = wolf.tick(&frame, DT);
        assert!(
            r.behavior.script_errors.is_empty(),
            "step {step}: {:?}",
            r.behavior.script_errors
        );
        assert!(
            r.animation.is_some(),
            "no clip at step {step} in {:?}",
            r.behavior.state_path
        );
    }
}
