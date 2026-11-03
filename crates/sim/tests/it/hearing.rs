//! Map foes hear Wolf (#218): the AI sounds his clips make (TAE 237 CreateAISound, AiSoundParam
//! radius) reach a foe within the exe's range, and an unaware foe that hears one turns to
//! CAUTION and searches the spot, as the shipped logic scripts expect.

use sekiro_sim::InputFrame;
use sekiro_sim::duel::Duel;

/// The Outskirts guard with no route (c1010_0001), the one the issue saw ignore Wolf.
fn guard(wolf: &Duel) -> &Duel {
    wolf.population
        .as_ref()
        .unwrap()
        .foes
        .iter()
        .find(|f| f.part == "c1010_0001")
        .unwrap()
        .duel
        .as_ref()
}

#[derive(Clone, Copy, Debug)]
enum Gait {
    /// Full stick: Wolf's run loop (AiSoundParam 1010, 2 m).
    Run,
    /// Full stick with the step button held: the sprint (1020, 4 m) after its step (1100, 4 m).
    Sprint,
    /// Full stick after a crouch press: the crouched run (1011, 0.5 m).
    Crouch,
}

/// Wolf moves past the guard's right side, `side` metres off its facing line, from 8 m behind
/// its feet to 1 m behind them (out of its sight cone the whole way), then stands still there
/// for 4 s. Returns whether the guard left its unaware state (caution, find or battle) and the
/// goal stacks it ran.
fn pass(gait: Gait, side: f32) -> Option<(bool, std::collections::BTreeSet<String>)> {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return None;
    }
    let mut wolf = super::patrol::outskirts(&cache, map, &dir);
    let (feet, yaw) = {
        let body = &guard(&wolf).enemy.body;
        (glam::Vec3::from_array(body.position), body.yaw)
    };
    let front = sekiro_sim::arena::forward(yaw);
    let right = glam::Vec3::new(-front.z, 0.0, front.x);
    let at = |along: f32| (feet + right * side + front * along).to_array();
    let dt = 1.0 / 60.0;
    let mut reacted = false;
    let mut stacks = std::collections::BTreeSet::new();
    let mut watch = |wolf: &Duel| {
        let ai = guard(wolf).ai.as_ref().unwrap();
        reacted |= ai.state.target_state != 0 || ai.state.battle;
        stacks.insert(ai.goal_stack().join(">"));
    };
    // Settle at the start, crouching first for the crouched pass.
    for step in 0..60 {
        wolf.player.body.position = at(-8.0);
        let mut input = InputFrame::default();
        input.buttons.crouch = matches!(gait, Gait::Crouch) && step < 2;
        wolf.step(&input, dt);
        watch(&wolf);
    }
    // 7 m at 3 m/s along the guard's facing, pushing the stick that way.
    let mut along = -8.0;
    while along < -1.0 {
        along += 3.0 * dt;
        wolf.player.body.position = at(along.min(-1.0));
        let mut input = InputFrame::toward_world_yaw(yaw, 1.0);
        input.buttons.dodge = matches!(gait, Gait::Sprint);
        wolf.step(&input, dt);
        watch(&wolf);
    }
    for _ in 0..60 * 4 {
        wolf.player.body.position = at(-1.0);
        wolf.step(&InputFrame::default(), dt);
        watch(&wolf);
    }
    eprintln!("{gait:?} at {side} m: reacted {reacted}, goals {stacks:?}");
    Some((reacted, stacks))
}

/// Sprinting past 3.6 m to the guard's side is heard (the 4 m sprint sound reaches past both
/// bodies' radii); the guard turns to caution and searches. It used to ignore him
/// (GetLatestSoundTargetID was always 0).
#[test]
fn a_guard_hears_wolf_sprint_past_its_side() {
    let Some((reacted, stacks)) = pass(Gait::Sprint, 3.6) else {
        return;
    };
    assert!(
        reacted,
        "the guard ignores Wolf sprinting 3.6 m to its side ({stacks:?})"
    );
}

/// Running past close by (2 m off its side) is heard too: the 2 m run sound.
#[test]
fn a_guard_hears_wolf_run_close_past_its_side() {
    let Some((reacted, stacks)) = pass(Gait::Run, 2.0) else {
        return;
    };
    assert!(
        reacted,
        "the guard ignores Wolf running 2 m to its side ({stacks:?})"
    );
}

/// The exe's ranges keep the quiet passes quiet: a run 3.6 m off (the 2 m run sound plus the
/// 0.1 m ear distance falls short of the gap between the bodies), and a crouched pass even at
/// 2 m (0.5 m crouched-run sound).
#[test]
fn a_guard_misses_quiet_passes() {
    for (gait, side) in [(Gait::Run, 3.6), (Gait::Crouch, 3.6), (Gait::Crouch, 2.0)] {
        let Some((reacted, stacks)) = pass(gait, side) else {
            return;
        };
        assert!(
            !reacted,
            "the guard hears Wolf's {gait:?} {side} m to its side ({stacks:?})"
        );
    }
}

/// A guard that hears Wolf without seeing him searches the spot and gives up: it turns to
/// CAUTION, runs the scripts' caution search (ConfirmCautionTarget, 5100) and, with no new
/// sound, clears the sound target and goes back to standing guard. It used to stay unaware.
#[test]
fn a_guard_searches_a_sound_then_gives_up() {
    let cache = sekiro_sim::cache::root();
    let map = "m11_00_00_00";
    let dir = cache.join(format!("maps/{map}"));
    if !dir.join("collision.bin").exists() || !cache.join("anim/c1010").is_dir() {
        return;
    }
    let mut wolf = super::patrol::outskirts(&cache, map, &dir);
    let (feet, yaw) = {
        let body = &guard(&wolf).enemy.body;
        (glam::Vec3::from_array(body.position), body.yaw)
    };
    let front = sekiro_sim::arena::forward(yaw);
    let right = glam::Vec3::new(-front.z, 0.0, front.x);
    let dt = 1.0 / 60.0;
    // Sprint on the spot 3.6 m to its right, 1 m behind it, until it hears him.
    let spot = (feet + right * 3.6 - front).to_array();
    let mut heard = false;
    for _ in 0..60 * 3 {
        wolf.player.body.position = spot;
        let mut input = InputFrame::toward_world_yaw(yaw, 1.0);
        input.buttons.dodge = true;
        wolf.step(&input, dt);
        let ai = guard(&wolf).ai.as_ref().unwrap();
        if ai.state.target_state == 1 {
            heard = true;
            break;
        }
    }
    assert!(heard, "the guard hears Wolf sprinting beside it");
    // Wolf gone (60 m off, out of sight): it searches, then forgets the spot.
    let far = (feet + front * -60.0).to_array();
    let mut log = Vec::new();
    for step in 0..60 * 40 {
        wolf.player.body.position = far;
        wolf.step(&InputFrame::default(), dt);
        let ai = guard(&wolf).ai.as_ref().unwrap();
        let stack = ai.goal_stack().join(">");
        if log
            .last()
            .is_none_or(|(_, s, st)| *s != stack || *st != ai.state.target_state)
        {
            log.push((step as f32 * dt, stack.clone(), ai.state.target_state));
        }
    }
    for (t, s, st) in &log {
        eprintln!("{t:6.2} s state {st}: {s}");
    }
    // The caution reaction (EndureAttack 1010) plays once: its TAE marks the AI state
    // (SpEffect 200001), which stops the scripts' state transition from replaying it.
    let reactions = log
        .iter()
        .filter(|(_, s, _)| s.starts_with("EndureAttack"))
        .count();
    assert_eq!(reactions, 1, "the caution reaction repeats");
    assert!(
        log.iter()
            .any(|(_, s, _)| s.contains("ConfirmCautionTarget>Wait")),
        "no caution search"
    );
    let ai = guard(&wolf).ai.as_ref().unwrap();
    assert_eq!(ai.state.target_state, 0, "the guard stays in caution");
    assert!(!ai.state.battle);
}

/// The exe's hearing rule with c1010's ears (ear_dist 0.1, ear_soundcut_dist 0) and two 0.4 m
/// bodies: the 4 m sprint carries 4.9 m between centres, the 2 m run 2.9 m, the 0.5 m crouched
/// run 1.4 m; a listener's hearingSearchEnemyCut and a cut distance shorten or silence it.
#[test]
fn hearing_reach_follows_the_exe_rule() {
    use sekiro_sim::duel::population::Hearing;
    let ear = Hearing {
        ear: 0.1,
        cut: 0.0,
        rate: 1.0,
    };
    assert!(ear.hears(4.0, 4.85, 0.8));
    assert!(!ear.hears(4.0, 4.95, 0.8));
    assert!(ear.hears(2.0, 2.85, 0.8));
    assert!(!ear.hears(2.0, 3.6, 0.8));
    assert!(!ear.hears(0.5, 2.0, 0.8));
    let half = Hearing { rate: 0.5, ..ear };
    assert!(!half.hears(4.0, 3.6, 0.8));
    let deaf = Hearing { cut: 4.0, ..ear };
    assert!(!deaf.hears(4.0, 0.0, 0.8));
}
