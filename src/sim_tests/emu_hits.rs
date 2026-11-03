//! Landed hits against the exe (tools/emu-cases `hits`, extracted/emu/hits_*.json): whole world
//! frames with the hit manager's step FUN_1409a0dc0 run, the General's slash a000_003000
//! (AtkParam_Npc 10200100) on Wolf idle, guard held, guard tapped k frames before the contact,
//! posture low. docs/kb/damage.md "Landed hits".

use super::*;
use serde_json::Value;

const DT: f32 = 1.0 / 60.0;

fn load(name: &str) -> Option<Value> {
    let path = crate::paths::extracted().join("emu").join(format!("hits_{name}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// Wolf's pad in a duel (as hits.rs `Pad`): guard held, or tapped for 2 frames from a frame.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Pad {
    Idle,
    Guard,
    Tap(usize),
}

/// One frame of the sim duel: Wolf's HP, posture, anim, anim time, position; the General's anim
/// time; the combat log lines of the frame.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub hp: f32,
    pub posture: f32,
    pub anim: String,
    pub t: f32,
    pub pos: Vec3,
    pub enemy_t: f32,
    pub log: String,
}

/// The General 2.5 m in front of Wolf, a000_003000 started on frame 0, Wolf's HP raised as the
/// emulator's copy does; `posture` is Sv1's posture damage already taken.
pub(crate) fn sim_duel(pad: Pad, posture: Option<f32>, frames: usize) -> Vec<Frame> {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&mut Actor, &mut Transform), With<Enemy>>();
        for (_, mut t) in q.iter_mut(world) {
            t.translation.z = 2.5;
        }
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.hp_max = 5000.0;
        a.hp = 5000.0;
        if let Some(left) = posture {
            a.posture = a.posture_max - left;
        }
    }
    {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003000");
    }
    let mut out = Vec::new();
    let mut lines = 0;
    for f in 0..frames {
        {
            let mut p = app.world_mut().resource_mut::<PadInput>();
            match pad {
                Pad::Guard => {
                    if f == 0 {
                        p.press(Action::Guard);
                    }
                    p.guard_held = true;
                }
                Pad::Tap(at) => {
                    if f == at {
                        p.press(Action::Guard);
                    }
                    p.guard_held = (at..at + 2).contains(&f);
                }
                Pad::Idle => {}
            }
        }
        app.update();
        let log = {
            let l = &app.world().resource::<CombatLog>().lines;
            let s = l[lines.min(l.len())..].iter().map(|x| x.0.clone()).collect::<Vec<_>>().join(" | ");
            lines = l.len();
            s
        };
        let world = app.world_mut();
        let (a, tf) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        let (hp, posture, anim, t, pos) = (a.hp, a.posture, a.anim.clone(), a.t, tf.translation);
        let e = boss_in_play(world);
        let enemy_t = world.get::<Actor>(e).unwrap().t;
        out.push(Frame { hp, posture, anim, t, pos, enemy_t, log });
    }
    out
}

/// A landed hit, the exe's or Sv1's, in the same terms.
#[derive(Debug)]
struct Hit {
    /// hit, block, deflect, guard_break, broken
    verdict: &'static str,
    hp: f32,
    posture: f32,
    /// Wolf's reaction clip (a000_100202 ...).
    reaction: String,
    /// The guard clip and its time the resolve saw (None: not in a guard clip).
    guard_t: Option<(String, f32)>,
    /// The General's slash time of the pose that made the contact.
    contact_t: f32,
    /// Frames from the hit to Wolf's first knockback step, and the first 12 steps.
    moves_after: usize,
    steps: Vec<f32>,
}

fn clip_name(id: i64) -> String {
    format!("a{:03}_{:06}", id / 1_000_000, id % 1_000_000)
}

fn guard_clip(id: i64) -> bool {
    matches!(id, 50203000 | 50203010 | 50002000)
}

/// The exe's hit from a run: the hit step of frame h saw the pose after frame h - 1 (Wolf's clip
/// in frame h starts at that time), the reaction clip runs from h + 1.
fn exe_hit(v: &Value) -> Option<Hit> {
    let fr = v["frames"].as_array().unwrap();
    let h = fr.iter().position(|x| !x["hits"].as_array().unwrap().is_empty())?;
    let hit = &fr[h]["hits"][0];
    let b = |k: &str| hit[k].as_i64().unwrap() != 0;
    let verdict = match (b("guarded"), b("deflected"), b("guard_break"), b("broken")) {
        (_, true, _, _) => "deflect",
        (true, _, true, _) => "guard_break",
        (true, ..) => "block",
        (_, _, _, true) => "broken",
        _ => "hit",
    };
    let clips = |x: &Value, who: &str| -> Vec<(i64, f32)> {
        x[who]["tae"].as_array().unwrap().iter().map(|c| (c[0].as_i64().unwrap(), c[1].as_f64().unwrap() as f32)).filter(|c| !(790000..800000).contains(&c.0)).collect()
    };
    let guard_t = clips(&fr[h], "wolf").into_iter().find(|c| guard_clip(c.0)).filter(|c| c.1 > 0.0).map(|c| (clip_name(c.0), c.1));
    let reaction = clip_name(clips(&fr[h + 1], "wolf")[0].0);
    let contact_t = clips(&fr[h], "general").iter().find(|c| c.0 == 3000).map_or(f32::NAN, |c| c.1);
    let pos = |x: &Value| Vec3::new(x["wolf"]["pos"][0].as_f64().unwrap() as f32, 0.0, x["wolf"]["pos"][2].as_f64().unwrap() as f32);
    let all: Vec<f32> = (h + 1..fr.len()).map(|k| (pos(&fr[k]) - pos(&fr[k - 1])).length()).collect();
    let first = all.iter().position(|s| *s > 1e-4)?;
    Some(Hit {
        verdict,
        hp: hit["hp_dmg"].as_f64().unwrap() as f32,
        posture: hit["posture_dmg"].as_f64().unwrap() as f32,
        reaction,
        guard_t,
        contact_t,
        moves_after: first + 1,
        steps: all[first..(first + 12).min(all.len())].to_vec(),
    })
}

/// Sv1's hit from a sim duel: the resolve runs after the frame's step, on that frame's poses.
fn sim_hit(fr: &[Frame], posture0: f32) -> Option<Hit> {
    let i = fr.iter().position(|x| x.log.contains("hit:") || x.log.contains("DEFLECT") || x.log.contains("blocked") || x.log.contains("GUARD BROKEN"))?;
    let log = &fr[i].log;
    let verdict = if log.contains("DEFLECT") {
        "deflect"
    } else if log.contains("GUARD BROKEN") {
        "guard_break"
    } else if log.contains("blocked") {
        "block"
    } else if log.contains("POSTURE BROKEN") {
        "broken"
    } else {
        "hit"
    };
    let prev = &fr[i - 1];
    let guard_t = guard_clip_name(&prev.anim).then(|| (prev.anim.clone(), prev.t + DT));
    let all: Vec<f32> = (i + 1..fr.len()).map(|k| (fr[k].pos - fr[k - 1].pos).with_y(0.0).length()).collect();
    let first = all.iter().position(|s| *s > 1e-4)?;
    Some(Hit {
        verdict,
        hp: prev.hp - fr[i].hp,
        posture: if verdict == "broken" || verdict == "guard_break" { f32::NAN } else { fr[i].posture - posture0 },
        reaction: fr[i].anim.clone(),
        guard_t,
        contact_t: prev.enemy_t + DT,
        moves_after: first + 1,
        steps: all[first..(first + 12).min(all.len())].to_vec(),
    })
}

fn guard_clip_name(anim: &str) -> bool {
    matches!(anim, "a050_203000" | "a050_203010" | "a050_002000")
}

/// The General's slash on Wolf as the exe lands it: verdict, reaction clip, HP / posture (the
/// exe's save scales the General's attack by k = 686.08 / 128, its area SpEffect), the guard chip
/// of a block (guardCutCancelRate -30), the knockback's first frame and travel, and the deflect
/// verdict by the guard clip's time at the contact.
#[test]
pub(crate) fn landed_hits_match_the_exe() {
    let Some(open) = load("g_open") else {
        eprintln!("skipped: no extracted/emu/hits_g_open.json (tools/emu-cases hits)");
        return;
    };
    let exe_open = exe_hit(&open).expect("the exe's slash landed");
    let sim_open = sim_hit(&sim_duel(Pad::Idle, None, 70), 0.0).expect("Sv1's slash landed");
    let k = exe_open.hp / sim_open.hp;
    assert!((k - 686.08 / 128.0).abs() < 0.01, "scale {k}: exe {exe_open:?} sim {sim_open:?}");
    // Sv1's posture counts up from 0 (damage taken); the exe's +0x148 counts down from 420.
    // The exe's low-posture cases leave Wolf 40 of 420: in Sv1 units 40 / k left before the break.
    let low = 40.0 / k;
    let cases: [(&str, Pad, Option<f32>); 4] = [("g_open", Pad::Idle, None), ("g_guard", Pad::Guard, None), ("g_break", Pad::Guard, Some(low)), ("g_open_low", Pad::Idle, Some(low))];
    for (name, pad, left) in cases {
        let Some(v) = load(name) else { continue };
        let exe = exe_hit(&v).unwrap();
        let fr = sim_duel(pad, left, 70);
        let posture0 = fr.first().map_or(0.0, |f| f.posture);
        let sim = sim_hit(&fr, posture0).unwrap_or_else(|| panic!("{name}: no hit in Sv1"));
        println!("{name}: exe {exe:?}\n  sim {sim:?}");
        assert_eq!(exe.verdict, sim.verdict, "{name}");
        // A block's size-Small reaction is HKS math.random's V1 / V2 (a050_120101 / 120104).
        let same_reaction = exe.reaction == sim.reaction || (exe.reaction.starts_with("a050_1201") && sim.reaction.starts_with("a050_1201"));
        assert!(same_reaction, "{name}: reaction exe {} sim {}", exe.reaction, sim.reaction);
        // HP: the exe's int of the scaled amount; Sv1's in unscaled points.
        assert!((sim.hp * k - exe.hp).abs() <= k, "{name}: HP exe {} sim {} x {k}", exe.hp, sim.hp);
        if sim.posture.is_finite() && exe.verdict != "broken" {
            assert!((sim.posture * k - exe.posture).abs() <= k, "{name}: posture exe {} sim {} x {k}", exe.posture, sim.posture);
        }
        // The knockback: the defender stands on the hit frame and the reaction's first frame.
        assert_eq!(exe.moves_after, sim.moves_after, "{name}: first knockback frame");
        if exe.verdict != "block" && exe.verdict != "broken" {
            // (A block's V1 / V2 clips and the break's carry different root motion.)
            let (te, ts): (f32, f32) = (exe.steps.iter().sum(), sim.steps.iter().sum());
            assert!((te - ts).abs() < 0.03 * te, "{name}: 12-frame travel exe {te} sim {ts} ({:?} / {:?})", exe.steps, sim.steps);
        }
        // gap: Sv1's hurt capsules meet the slash a few frames from the exe's (exe pose 0.683 idle
        // / 0.667 guarding, Sv1 0.650 / 0.683): within 2 frames.
        assert!((exe.contact_t - sim.contact_t).abs() <= 2.0 * DT + 1e-3, "{name}: contact pose exe {} sim {}", exe.contact_t, sim.contact_t);
    }
    // The deflect window by the guard clip's time at the contact (a050_203000 from its first
    // frame): the exe deflects at 0.017 .. 0.2, blocks from 0.233; an early tap whose guard has
    // gone on to a050_203010 blocks.
    let mut sim_taps = Vec::new();
    for p in 8..42 {
        if let Some(h) = sim_hit(&sim_duel(Pad::Tap(p), None, 70), 0.0) {
            sim_taps.push(h);
        }
    }
    let mut compared = 0;
    for kk in [1, 2, 3, 4, 6, 8, 10, 12, 14, 15, 16, 20, 24, 28] {
        let Some(v) = load(&format!("g_tap{kk}")) else { continue };
        let exe = exe_hit(&v).unwrap();
        let Some((clip, t)) = exe.guard_t.clone() else { continue };
        let Some(sim) = sim_taps.iter().find(|s| s.guard_t.as_ref().is_some_and(|(sc, st)| *sc == clip && (st - t).abs() < 0.004)) else {
            println!("tap{kk}: {clip} at {t:.3}: no Sv1 tap meets the slash there");
            continue;
        };
        println!("tap{kk}: {clip} at {t:.3}: exe {} sim {}", exe.verdict, sim.verdict);
        assert_eq!(exe.verdict, sim.verdict, "tap{kk}: {clip} at {t}");
        compared += 1;
    }
    assert!(compared >= 8, "only {compared} taps compared");
}
