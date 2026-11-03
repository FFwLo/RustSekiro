//! A running jump: the move stick held forward through the jump press and wind-up must still
//! take off (as in the game). Skips when `cache/` has not been extracted.

use sekiro_sim::{Buttons, Character, InputFrame};

const DT: f32 = 1.0 / 60.0;

#[test]
fn running_jump_takes_off_with_the_stick_held() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c0000.behbnd.d/c0000.hkx").exists() {
        return;
    }
    let mut wolf = Character::load_from_cache(&cache).unwrap();
    wolf.settle(DT, 400);
    let mut states: Vec<String> = Vec::new();
    let mut airborne = false;
    for i in 0..200 {
        let input = InputFrame {
            move_stick: [0.0, 1.0],
            buttons: Buttons {
                jump: (60..63).contains(&i),
                ..Buttons::default()
            },
            ..InputFrame::default()
        };
        let r = wolf.tick(&input, DT);
        let s = r.behavior.state_path.last().cloned().unwrap_or_default();
        if states.last() != Some(&s) {
            eprintln!(
                "{i} {s} {:?} y {:.2} grounded {}",
                r.animation, r.position[1], r.grounded
            );
            states.push(s);
        }
        airborne |= i > 60 && !r.grounded;
    }
    assert!(airborne, "never left the ground: {states:?}");
}
