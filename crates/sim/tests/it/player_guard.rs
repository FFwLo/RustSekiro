//! The real player scripts and behaviour graph, driven through guard scenarios. Skips when
//! `cache/` has not been extracted.

use sekiro_sim::PlayerBehavior;
use sekiro_sim::scenario::{self, Guard};

fn load() -> Option<PlayerBehavior> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("raw/chr/c0000.behbnd.d/c0000.hkx").exists() {
        eprintln!("skipping: cache not extracted");
        return None;
    }
    let mut p = PlayerBehavior::load_from_cache(&cache).unwrap();
    let frames = scenario::settle_to_idle(&mut p, 300);
    assert!(frames < 300, "never reached StandIdle");
    Some(p)
}

/// The distinct (state, animation) pairs in the order they appeared.
fn sequence(reports: &[sekiro_sim::TickReport]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for r in reports {
        assert!(r.script_errors.is_empty(), "{:?}", r.script_errors);
        let item = (
            r.state_path.last().cloned().unwrap_or_default(),
            r.animation.clone().unwrap_or_default(),
        );
        if out.last() != Some(&item) {
            out.push(item);
        }
    }
    out
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn hold_guard_then_release() {
    let Some(mut p) = load() else { return };
    let reports = scenario::run(&mut p, &scenario::guard_hold(), false);
    assert_eq!(
        sequence(&reports),
        pairs(&[
            ("StandIdle", "a000_000000"),
            ("StandToDeflectGuard", "a050_203000"),
            ("DeflectGuardIdle", "a050_002000"),
            ("DeflectGuardToStand", "a050_203010"),
            ("StandIdle", "a000_000000"),
        ])
    );
    // The guard starts on the press frame.
    assert_eq!(reports[30].animation.as_deref(), Some("a050_203000"));
}

#[test]
fn repeated_presses_chain_guard_variants() {
    let Some(mut p) = load() else { return };
    let reports = scenario::run(&mut p, &scenario::guard_repeat(), true);
    let idle_frames = reports
        .iter()
        .filter(|r| r.animation.as_deref() == Some("a050_002000"))
        .count();
    // The fourth variant ends into guard idle (its end transition) for one step before the
    // script sees the released button.
    assert!(idle_frames <= 1, "{idle_frames}");
    let anims: Vec<String> = sequence(&reports).into_iter().map(|(_, a)| a).collect();
    assert_eq!(
        anims,
        [
            "a000_000000",
            "a050_203000",
            "a050_203005",
            "a050_203006",
            "a050_203007",
            "a050_002000",
            "a050_203010",
            "a000_000000"
        ]
    );
}

#[test]
fn repeated_presses_without_combo_window_restart_first_variant() {
    let Some(mut p) = load() else { return };
    let inputs = [
        vec![Guard::Released; 5],
        vec![Guard::Pressed, Guard::Held, Guard::Released],
        vec![Guard::Pressed, Guard::Held, Guard::Released],
    ]
    .concat();
    let reports = scenario::run(&mut p, &inputs, false);
    // Without reference 212 the continue behaviour re-enters the first variant.
    assert!(
        reports
            .iter()
            .all(|r| r.animation.as_deref() != Some("a050_203005"))
    );
    let restarts = reports
        .iter()
        .filter(|r| r.events.iter().any(|e| e == "W_StandToDeflectGuard"))
        .count();
    assert_eq!(restarts, 2);
}
