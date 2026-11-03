//! The SpEffect add's "already on" rules, a replace through the build-up, the regens and the
//! buff-duration count, and the General's build-up, run as `speffect` runs them (the add
//! FUN_1409e9c00, then vfunc 44 FUN_140a02a10 frame by frame at 30 and 60 fps on the Naomori-arena
//! snapshot). Writes extracted/emu/speffect2_ticks.json (same layout as speffect_ticks.json) for
//! src/sim_tests/emu_speffect.rs.
//!
//! - FUN_140c02000 "already on" (the same id, the same stateInfo 2 / 5 / 6, FUN_140c05a80's
//!   category rules) sends the add to FUN_140c064c0, which picks the entry the new row takes over
//!   (FUN_140c06860) or refuses it: Ako's 3401 / Ungo's 3421 (category 162), the puppet rows
//!   220030 / 220031 (100), resonance 41 / 42 (110), 5580 / 5581 (130), 6022 / 107712 / 230630
//!   (141, by categoryPriority), aging 9620 / 9640 (150), Knowledge of Medicine 150203 / 150204 /
//!   150200 on the General (181, by categoryPriority), 9250 and the pellet 3021 again, a bleed
//!   6400 and the Elixir's 3044 on a burning Wolf (stateInfo 6).
//! - stateInfo 120 / 121 / 155 fire at the add (FUN_140c03af0): 5370, 1150, 2090.
//! - The floor's 4011 ten times: its replace 4012 goes through the add, so its build-up fills the
//!   burn gauge (FUN_140c005b0 -> FUN_1409e9c00).
//! - Persimmon 3071 and ohagi 3061 + 3062 with Wolf's posture at 100 (FUN_140bfe2c0).
//! - The General with 30201 "[Conversation] damage completely invalid" (disable all five) cleared
//!   in the copy: his burn takes.

use super::speffect::{run_set, Scenario, Setup};
use super::*;

/// The General's conversation row: disablePoison / Disease / Blood / Curse / Freeze all set.
const CONVERSATION: &[i32] = &[30201];

fn scenarios() -> Vec<(Scenario, Setup)> {
    let mut v = Vec::new();
    for fps in [30, 60] {
        let f = |s: f32| (s * fps as f32).round() as usize;
        let ten = |id: i32, every: f32| (0..10).map(|k| (f(every) * k, id)).collect::<Vec<_>>();
        let wolf = |name: &'static str, frames: usize, adds: Vec<(usize, i32)>| (Scenario { name, enemy: false, fps, frames, adds }, Setup::default());
        v.push(wolf("ako_then_ungo", f(20.0), vec![(0, 3401), (f(5.0), 3421)]));
        v.push(wolf("ungo_then_ako", f(20.0), vec![(0, 3421), (f(5.0), 3401)]));
        v.push(wolf("cat100_220030", f(3.0), vec![(0, 220030), (f(1.0), 220031), (f(2.0), 5400)]));
        v.push(wolf("cat110_resonance", f(7.0), vec![(0, 41), (f(1.0), 42), (f(2.0), 41)]));
        v.push(wolf("cat130_5580", f(2.0), vec![(0, 5580), (f(1.0), 5581)]));
        v.push(wolf("cat141_6022", f(3.0), vec![(0, 6022), (f(1.0), 107712), (f(1.5), 230630)]));
        v.push(wolf("cat150_aging", f(3.0), vec![(0, 9620), (f(2.0), 9640)]));
        v.push(wolf("same_id_9250", f(4.0), vec![(0, 9250), (f(1.0), 9250)]));
        v.push(wolf("pellet_again", f(14.0), vec![(0, 3021), (f(3.0), 3021)]));
        v.push(wolf("burn_then_bleed", f(5.0), [ten(9100, 0.1), vec![(f(3.5), 6400)]].concat()));
        v.push(wolf("burn_then_elixir", f(5.0), [ten(9100, 0.1), vec![(f(3.5), 3044)]].concat()));
        v.push(wolf("si120_5370", f(0.5), vec![(0, 5370)]));
        v.push(wolf("si121_1150", f(1.0), vec![(0, 1150)]));
        v.push(wolf("si155_2090", f(0.5), vec![(0, 2090)]));
        v.push(wolf("floor_4011_x10", f(12.0), ten(4011, 0.5)));
        let posture = |name: &'static str, adds: Vec<(usize, i32)>| (Scenario { name, enemy: false, fps, frames: f(5.0), adds }, Setup { posture: Some(100), clear: &[] });
        v.push(posture("persimmon_posture", vec![(0, 3071)]));
        v.push(posture("ohagi_posture", vec![(0, 3061), (0, 3062)]));
        let general = |name: &'static str, frames: usize, adds: Vec<(usize, i32)>| (Scenario { name, enemy: true, fps, frames, adds }, Setup { posture: None, clear: CONVERSATION });
        v.push(general("general_burn", f(12.0), ten(9100, 0.1)));
        v.push(general("general_floor_4011_x10", f(12.0), ten(4011, 0.5)));
        v.push(general("cat181_medicine", f(4.0), vec![(0, 150203), (f(1.0), 150204), (f(2.0), 150203), (f(3.0), 150200)]));
    }
    v
}

pub(crate) fn run(new_emu: &dyn Fn() -> Emu, out: &Path) {
    run_set(new_emu, out, "speffect2_ticks", scenarios());
}
