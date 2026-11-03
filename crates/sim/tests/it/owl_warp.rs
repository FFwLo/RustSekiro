//! The Great Shinobi Owl's warp clips: he fades out (TAE 193), cannot be hit while faded
//! (ChrActionFlag 94), crosses the floor on root motion boosted toward Wolf (TAE 760) and fades
//! back in to attack. Forced with the repeat brain (`BrainKind::Repeat`, the game's
//! `--force-act`). Values are the data's (`docs/OWL.md`, "Warp and vanish"). Skips when
//! `cache/` lacks him.

use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::hits::can_be_hit;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn load(distance: f32, act: i32) -> Option<Duel> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").exists()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
    {
        eprintln!("skipping: the Owl is not extracted");
        return None;
    }
    let mut duel = Duel::load_with(&cache, distance, EnemySpec::owl()).unwrap();
    // His castle battle goal never asks for these acts, so the repeat brain cannot plan them;
    // like the game's `--passive --force-act`, the AI is off and the act is requested directly.
    duel.enemy_active = false;
    duel.force_act = Some(act);
    Some(duel)
}

fn keep_wolf_fresh(duel: &mut Duel) {
    let wolf = &mut duel.rules.fighters[0];
    wolf.vitality.hp = wolf.vitality.max_hp;
    wolf.posture.remaining = wolf.posture.max;
}

/// Steps with Wolf idle until the Owl starts `clip`; false if he never does.
fn until_clip(duel: &mut Duel, clip: &str) -> bool {
    for _ in 0..3000 {
        keep_wolf_fresh(duel);
        let r = duel.step(&InputFrame::default(), DT);
        if r.enemy.animation.as_deref() == Some(clip) {
            return true;
        }
    }
    false
}

fn ground_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// One step of the Owl in a warp clip.
#[derive(Debug, Clone)]
struct Sample {
    time: f32,
    fade: f32,
    hittable: bool,
    distance: f32,
}

/// Steps until the Owl leaves `clip` (or `max` steps), with `input` for Wolf.
fn watch(
    duel: &mut Duel,
    clip: &str,
    max: usize,
    input: impl Fn(usize) -> InputFrame,
) -> (Vec<Sample>, Vec<i32>) {
    let mut out = Vec::new();
    let mut wolf_hits_while_faded = Vec::new();
    for i in 0..max {
        keep_wolf_fresh(duel);
        let r = duel.step(&input(i), DT);
        assert!(r.enemy.behavior.script_errors.is_empty());
        let name = r.enemy.animation.clone().unwrap_or_default();
        if name != clip {
            break;
        }
        for (h, _) in &r.hits {
            if h.attacker == 0 && r.enemy.fade > 0.0 {
                wolf_hits_while_faded.push(h.attack.atk_id);
            }
        }
        out.push(Sample {
            time: r.enemy.anim_time,
            fade: r.enemy.fade,
            hittable: can_be_hit(duel.enemy.tae_frame()),
            distance: ground_distance(r.enemy.position, r.player.position),
        });
    }
    (out, wolf_hits_while_faded)
}

/// The windows in which the Owl was fully faded out, as clip times (first, last).
fn hidden_windows(samples: &[Sample]) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::new();
    let mut open = false;
    for s in samples {
        if s.fade >= 0.999 {
            match (open, out.last_mut()) {
                (true, Some(w)) => w.1 = s.time,
                _ => out.push((s.time, s.time)),
            }
            open = true;
        } else {
            open = false;
        }
    }
    out
}

/// One step's input with only these buttons.
fn press(buttons: Buttons) -> InputFrame {
    InputFrame {
        buttons,
        ..InputFrame::default()
    }
}

/// Wolf mashes attack (pressed every other step).
fn mash(i: usize) -> InputFrame {
    press(Buttons {
        attack: i.is_multiple_of(2),
        ..Buttons::default()
    })
}

#[test]
fn the_triple_warp_fades_him_out_three_times_and_he_cannot_be_hit_while_gone() {
    let Some(mut duel) = load(8.0, 3044) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003044"));
    let (s, _) = watch(&mut duel, "a000_003044", 600, |_| InputFrame::default());
    // TAE 193 in `a000_003044`: faded out over 0.267..0.367, gone to 0.9, back by 1.0; the same
    // at 1.467 / 1.567..2.1 / 2.2 and 3.3 / 3.4..3.933 / 4.033.
    let windows = hidden_windows(&s);
    assert_eq!(windows.len(), 3, "{windows:?}");
    for (w, (from, to)) in windows
        .iter()
        .zip([(0.367, 0.9), (1.567, 2.1), (3.4, 3.933)])
    {
        assert!(
            (w.0 - from).abs() < 0.03 && (w.1 - to).abs() < 0.03,
            "{windows:?}"
        );
    }
    // ChrActionFlag 94 covers every moment he is gone (0..0.9, 1.467..2.1, 3.3..3.933).
    assert!(s.iter().filter(|x| x.fade >= 0.999).all(|x| !x.hittable));
    // Drawn and hittable again between the warps and after the last one.
    let back = |t: f32| s.iter().find(|x| x.time >= t).unwrap();
    for t in [1.1, 2.3, 4.1] {
        assert!(
            back(t).fade == 0.0 && back(t).hittable,
            "{t}: {:?}",
            back(t)
        );
    }
    // The last warp's TAE 760 (reference 7 m, range 0..14 m, arrive 4 m from Wolf) brings him
    // back about 4 m from Wolf.
    let last = s.iter().find(|x| x.time >= 4.033).unwrap();
    assert!((last.distance - 4.0).abs() < 0.75, "{last:?}");
}

#[test]
fn wolf_cannot_hit_him_while_he_is_faded_out() {
    let Some(mut duel) = load(1.4, 3093) else {
        return;
    };
    assert!(until_clip(&mut duel, "a000_003093"));
    // Wolf slashes away from the start: `a000_003093` makes the Owl invulnerable from 0 to
    // 0.767 s (flag 94) and gone from 0.367 s; he leaps back out of reach meanwhile.
    let (s, hits) = watch(&mut duel, "a000_003093", 60, mash);
    assert!(hits.is_empty(), "{hits:?}");
    assert!(s.iter().any(|x| x.fade >= 0.999));
    // Wolf did swing.
    assert!(
        duel.player
            .behavior
            .runtime
            .main_clip()
            .is_some_and(|c| c.animation.starts_with("a050_30")),
        "{:?}",
        duel.player.behavior.runtime.main_clip()
    );
}

#[test]
fn a_single_warp_ends_closer_to_wolf_the_farther_he_was() {
    // `a000_003092`: gone 0.4..0.9 s while TAE 760 (reference 15 m, range 5..25 m, arrive 4 m)
    // scales his root motion by the distance to Wolf, so where he comes back depends on Wolf,
    // not on the clip alone.
    let mut travel = Vec::new();
    for start in [5.0, 12.0, 16.0] {
        let Some(mut duel) = load(start, 3092) else {
            return;
        };
        assert!(until_clip(&mut duel, "a000_003092"));
        let (s, _) = watch(&mut duel, "a000_003092", 600, |_| InputFrame::default());
        let windows = hidden_windows(&s);
        assert_eq!(windows.len(), 1, "{windows:?}");
        let gone = s.iter().find(|x| x.fade >= 0.999).unwrap();
        let back = s
            .iter()
            .find(|x| x.time > windows[0].1 && x.fade == 0.0)
            .unwrap();
        assert!(back.distance < gone.distance - 3.0, "{gone:?} {back:?}");
        travel.push(gone.distance - back.distance);
    }
    assert!(travel[0] < travel[1] && travel[1] < travel[2], "{travel:?}");
}

#[test]
fn the_lock_on_stays_on_him_and_he_fights_on_after_a_warp() {
    let Some(mut duel) = load(8.0, 3092) else {
        return;
    };
    duel.step(
        &press(Buttons {
            lock_on: true,
            ..Buttons::default()
        }),
        DT,
    );
    assert!(until_clip(&mut duel, "a000_003092"));
    let (s, _) = watch(&mut duel, "a000_003092", 600, |_| InputFrame::default());
    assert!(s.iter().any(|x| x.fade >= 0.999));
    // No SpEffect with `stateInfo` 350 in the warp clips: nothing drops the lock.
    assert!(duel.player.lock_target.is_some());
    // Back under his own AI: drawn, hittable, attacking again.
    duel.force_act = None;
    duel.enemy_active = true;
    let mut attacks = 0;
    let mut last = String::new();
    for _ in 0..600 {
        keep_wolf_fresh(&mut duel);
        let r = duel.step(&InputFrame::default(), DT);
        assert_eq!(r.enemy.fade, 0.0);
        let clip = r.enemy.animation.clone().unwrap_or_default();
        if clip != last && clip.starts_with("a000_003") {
            attacks += 1;
        }
        last = clip;
    }
    assert!(can_be_hit(duel.enemy.tae_frame()));
    assert!(attacks >= 1, "no attack in 10 s");
}
