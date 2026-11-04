//! Which TAE windows are live on the frame a clip is replaced, against the user's own sekiro.exe:
//! tools/emu-cases `framewin` runs whole world frames as `hits` does and logs each chr's TAE
//! updates FUN_140b5c920, the ChrActionFlag words (module0 +0x70, ActionCtrl +0x1f8) at the
//! frame's start and after the chr frames, and whose attack records the hit step tests
//! (kb/frame-coverage.md "Frame order at a clip change"). The hit step of frame N reads what the
//! step of frame N - 1 dispatched; a hit's reaction (asked for in the hit step of frame h) and a
//! clip the HKS asks for take their first window in the next frame's step, and the step of frame
//! h still steps the old clip. Sv1's resolve of tick N reads `Actor::tae_anim` / `tae_step`.
//! Skips when extracted/emu/framewin_*.json is missing.

use super::*;
use crate::tae_window::At;
use serde_json::Value;

const DT: f32 = 1.0 / 60.0;

fn load(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("framewin_{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn clip_name(id: i64) -> String {
    format!("a{:03}_{:06}", id / 1_000_000, id % 1_000_000)
}

/// A TAE window: (clip, time before, time after).
type Win = (String, f32, f32);

/// The exe's main-clip TAE update of `who` in frame `f` (the upper layer a000_79xxxx left out).
fn exe_win(v: &Value, f: usize, who: &str) -> Win {
    let c = v["frames"][f][format!("{who}_tae")].as_array().unwrap().iter().find(|c| !(790000..800000).contains(&c[0].as_i64().unwrap())).unwrap();
    (clip_name(c[0].as_i64().unwrap()), c[1].as_f64().unwrap() as f32, c[2].as_f64().unwrap() as f32)
}

/// The first frame with a hit by `attacker`.
fn exe_hit(v: &Value, attacker: &str) -> usize {
    v["frames"].as_array().unwrap().iter().position(|x| x["hits"].as_array().unwrap().iter().any(|h| h["attacker"] == attacker)).unwrap()
}

/// What Sv1's resolve read in one tick, per side: (current clip, the held window as a Win).
#[derive(Resource, Default)]
struct Seen(Vec<[(String, Win); 2]>);

fn win_of(a: &Actor) -> Win {
    match a.tae_step() {
        At::Step(p, t) => (a.tae_anim().to_string(), p, t),
        At::Point(t) => (a.tae_anim().to_string(), t, t),
    }
}

/// Between decide (and clear_fresh) and resolve: the windows the hit test is about to read.
fn probe(q: Query<(&Actor, Has<Player>)>, mut seen: ResMut<Seen>) {
    let mut row: [(String, Win); 2] = Default::default();
    for (a, pc) in &q {
        row[if pc { 0 } else { 1 }] = (a.anim.clone(), win_of(a));
    }
    seen.0.push(row);
}

/// One sim tick's record: what resolve read (Wolf, the enemy) and the combat log of the tick.
struct Tick {
    read: [(String, Win); 2],
    log: String,
}

fn run(app: &mut App, frames: usize, mut each: impl FnMut(&mut App, usize)) -> Vec<Tick> {
    app.init_resource::<Seen>();
    app.add_systems(FixedUpdate, probe.after(crate::actor::clear_fresh).before(crate::actor::ActorSet::Resolve));
    let mut out = Vec::new();
    let mut lines = app.world().resource::<CombatLog>().lines.len();
    for f in 0..frames {
        each(app, f);
        app.world_mut().resource_mut::<Seen>().0.clear();
        app.update();
        let read = app.world_mut().resource_mut::<Seen>().0.pop().unwrap_or_default();
        let log = {
            let l = &app.world().resource::<CombatLog>().lines;
            let s = l[lines.min(l.len())..].iter().map(|x| x.0.clone()).collect::<Vec<_>>().join(" | ");
            lines = l.len();
            s
        };
        out.push(Tick { read, log });
    }
    out
}

/// The General's slash a000_003000 on Wolf from 2.5 m (as emu_hits' duel); `pad` sets Wolf's pad.
fn general_slash(frames: usize, pad: impl Fn(&mut PadInput, usize)) -> (Vec<Tick>, App) {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        for mut t in q.iter_mut(world) {
            t.translation.z = 2.5;
        }
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.hp_max = 5000.0;
        a.hp = 5000.0;
        let mut q = world.query::<(&mut Enemy, &mut Actor)>();
        let (mut e, mut a) = q.single_mut(world).unwrap();
        e.force_anim(&mut a, "a000_003000");
    }
    let t = run(&mut app, frames, |app, f| pad(&mut app.world_mut().resource_mut::<PadInput>(), f));
    (t, app)
}

fn sim_hit(t: &[Tick]) -> usize {
    t.iter().position(|x| x.log.contains("hit:") || x.log.contains("DEFLECT") || x.log.contains("blocked") || x.log.contains("GUARD BROKEN")).expect("Sv1's hit landed")
}

/// The windows the hit steps of frames h, h + 1, h + 2 read in the exe (the TAE updates of frames
/// h - 1, h, h + 1) against Sv1's resolve of ticks i, i + 1, i + 2 (i its hit tick): the old
/// clip, the old clip stepped once more on from where it was, the reaction from 0.
fn same_shape(name: &str, exe: [Win; 3], sim: [Win; 3]) {
    println!("{name}: exe {exe:?}\n  sim {sim:?}");
    for (k, w) in [&exe, &sim].into_iter().enumerate() {
        let who = ["exe", "sim"][k];
        assert_eq!(w[0].0, w[1].0, "{name} {who}: the hit's frame steps the old clip once more");
        assert!((w[1].1 - w[0].2).abs() < 1e-5 && (w[1].2 - w[1].1 - DT).abs() < 1e-4, "{name} {who}: one more step of the old clip from where it was: {w:?}");
        assert_ne!(w[2].0, w[1].0, "{name} {who}: the reaction's first window is the next frame's");
        assert!(w[2].1 == 0.0 && (w[2].2 - DT).abs() < 1e-5, "{name} {who}: the reaction from 0: {:?}", w[2]);
    }
    // (Sv1's standing idle is procedural, "": the exe's a000_000000.)
    assert!(exe[0].0 == sim[0].0 || (sim[0].0.is_empty() && exe[0].0 == "a000_000000"), "{name}: the old clip");
    // (A block's size-Small reaction is HKS math.random's V1 / V2, a050_120101 / 120104.)
    assert!(exe[2].0 == sim[2].0 || (exe[2].0.starts_with("a050_1201") && sim[2].0.starts_with("a050_1201")), "{name}: the reaction");
}

/// The hit's frame: the exe still steps Wolf's old clip (idle a000_000000, the guard hold
/// a050_002000) and its flags are what the next hit step reads (the guard's ShieldBlock bit 4 of
/// +0x70 at the start of frame h + 1); the reaction (a000_100202, a050_120101) takes its first
/// window [0, dt] in frame h + 1. Sv1 before: the reaction played in resolve was read at (0, 0)
/// from the hit's tick on and the old clip's last window was lost.
#[test]
pub(crate) fn the_old_clip_keeps_its_window_on_the_frame_a_reaction_starts() {
    let mut compared = 0;
    for (name, guard) in [("g_open", false), ("g_guard", true)] {
        let Some(v) = load(name) else { continue };
        let h = exe_hit(&v, "general");
        let exe = [exe_win(&v, h - 1, "wolf"), exe_win(&v, h, "wolf"), exe_win(&v, h + 1, "wolf")];
        let pre = |f: usize| u64::from_str_radix(v["frames"][f]["flags_pre"]["wolf"][0].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
        let post = |f: usize| u64::from_str_radix(v["frames"][f]["flags_post"]["wolf"][0].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
        // The flags the hit step reads are the last chr frame's: they hold from frame to frame.
        for f in h - 2..h + 3 {
            assert_eq!(pre(f + 1), post(f), "{name}: flags at the start of frame {}", f + 1);
        }
        assert_eq!(pre(h + 1) & 4 != 0, guard, "{name}: ShieldBlock on the hit step after the hit");
        let (t, app) = general_slash(h + 8, |p, f| {
            if guard {
                if f == 0 {
                    p.press(Action::Guard);
                }
                p.guard_held = true;
            }
        });
        let i = sim_hit(&t);
        let sim = [t[i].read[0].1.clone(), t[i + 1].read[0].1.clone(), t[i + 2].read[0].1.clone()];
        // Sv1's guard hold is a050_002000 as the exe's (the clip times differ: Sv1's contact
        // lands within 2 frames of the exe's, emu_hits).
        same_shape(name, exe, sim.clone());
        let d = &app.world().resource::<Combat>().player;
        let shield = |w: &Win| d.flag(&w.0, At::Step(w.1, w.2), crate::player::FLAG_SHIELD_BLOCK);
        assert_eq!(shield(&sim[1]), guard, "{name}: Sv1's ShieldBlock on the resolve after the hit");
        compared += 1;
    }
    if compared > 0 {
        assert_eq!(compared, 2);
    }
}

/// Wolf's slash a050_300000 on the General 2 m away, AI idle, held in a just-guard ShieldBlock
/// pose until the hit (as emu_knockback's duel): Wolf is deflected.
fn wolf_deflected(frames: usize) -> (Vec<Tick>, App) {
    let mut app = app_geom("c1020", None);
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 2.0;
        world.resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    }
    for _ in 0..10 {
        app.update();
    }
    let pose = {
        let world = app.world_mut();
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
        let d = world.resource::<Combat>().data_of(a);
        let mut found: Vec<(String, f32)> = d
            .anims
            .iter()
            .flat_map(|(k, an)| an.events.iter().filter(|e| e.kind == 0 && e.flag_type() == Some(crate::player::FLAG_SHIELD_BLOCK)).map(move |e| (k.clone(), e.start + 0.05)))
            .filter(|(k, t)| d.flag(k, *t, crate::player::FLAG_SHIELD_BLOCK) && d.has_state_info(k, *t, crate::data::STATE_INFO_JUST_GUARD))
            .collect();
        found.sort_by(|x, y| x.0.cmp(&y.0));
        found.into_iter().next().expect("a just-guard pose")
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let mut held = true;
    let t = run(&mut app, frames, |app, _| {
        if held {
            let world = app.world_mut();
            // Released once the deflect has landed (the log of the last tick).
            held = !world.resource::<CombatLog>().lines.iter().any(|l| l.0.contains("DEFLECT"));
            if held {
                let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
                if a.anim != pose.0 {
                    a.play("Guard", &pose.0);
                }
                a.prev_t = pose.1;
                a.t = pose.1;
                a.held = None;
            }
        }
    });
    (t, app)
}

/// Does `w` dispatch one of Wolf's AttackBehavior windows that can hit?
fn swings(app: &App, w: &Win) -> bool {
    let d = &app.world().resource::<Combat>().player;
    !w.0.is_empty() && d.attack_windows(&w.0).iter().any(|(e, atk, _)| e.dispatched(At::Step(w.1, w.2)) && atk.oppose_target != 0)
}

/// A deflected swing still swings once more: the exe's step of the deflect's frame h steps Wolf's
/// a050_300100 on (0.283 -> 0.300), its attack records are tested by the hit step of h + 1 and
/// gone by h + 2 (the deflect reaction a050_150000 from 0 in frame h + 1). Sv1: the resolve after
/// the deflect reads the held slash window with its AttackBehavior dispatched (it already landed
/// on the General: `Held::hits`), the one after reads the reaction.
#[test]
pub(crate) fn a_deflected_swing_is_tested_once_more() {
    let Some(v) = load("w_deflect") else {
        eprintln!("skipped: no extracted/emu/framewin_w_deflect.json (tools/emu-cases framewin)");
        return;
    };
    let h = exe_hit(&v, "wolf");
    let tested = |f: usize| v["frames"][f]["tested"].as_array().unwrap().iter().any(|x| x == "wolf");
    assert!(tested(h + 1) && !tested(h + 2), "the exe tests Wolf's records on h + 1 only");
    let exe = [exe_win(&v, h - 1, "wolf"), exe_win(&v, h, "wolf"), exe_win(&v, h + 1, "wolf")];
    let (t, app) = wolf_deflected(60);
    let i = t.iter().position(|x| x.log.contains("DEFLECT")).expect("Sv1 deflected the slash");
    let sim = [t[i].read[0].1.clone(), t[i + 1].read[0].1.clone(), t[i + 2].read[0].1.clone()];
    same_shape("w_deflect", exe, sim.clone());
    assert!(swings(&app, &sim[1]), "Sv1's resolve after the deflect tests the slash: {:?}", sim[1]);
    assert!(!swings(&app, &sim[2]), "and not the one after: {:?}", sim[2]);
    // One deflect: the held window's hits keep the General from being met by it again.
    assert_eq!(t.iter().filter(|x| x.log.contains("DEFLECT")).count(), 1);
}

/// A clip the HKS asks for starts after the hit step: guard held, let go on frame 39, the exe's
/// guard release a050_203010 takes its first step in frame 40 while the hit step of frame 40 reads
/// the guard hold a050_002000's window of frame 39 (and blocks). Sv1: the resolve of the tick
/// `decide` plays the release reads the hold's held window; the release's first window is that
/// tick's step.
#[test]
pub(crate) fn a_clip_decide_plays_is_read_after_the_hit_test() {
    let Some(v) = load("g_release39") else {
        eprintln!("skipped: no extracted/emu/framewin_g_release39.json (tools/emu-cases framewin)");
        return;
    };
    let h = exe_hit(&v, "general");
    let (before, at) = (exe_win(&v, h - 1, "wolf"), exe_win(&v, h, "wolf"));
    assert_eq!(before.0, "a050_002000", "the hit step reads the hold");
    assert_eq!((at.0.as_str(), at.1), ("a050_203010", 0.0), "the release steps from 0 after the hit step");
    assert_eq!(v["frames"][h]["hits"][0]["guarded"].as_i64(), Some(1));
    // Sv1's own contact tick with guard held, then let go one tick before it.
    let (g, _) = general_slash(70, |p, f| {
        if f == 0 {
            p.press(Action::Guard);
        }
        p.guard_held = true;
    });
    let c = sim_hit(&g);
    let (t, app) = general_slash(c + 4, |p, f| {
        if f == 0 {
            p.press(Action::Guard);
        }
        p.guard_held = f < c - 1;
    });
    let i = sim_hit(&t);
    assert_eq!(i, c, "the same contact tick");
    let (cur, read) = &t[i].read[0];
    println!("exe {before:?} -> {at:?}; sim resolve: current {cur}, read {read:?}, log {}", t[i].log);
    assert_eq!(cur, "a050_203010", "decide played the release this tick");
    assert_eq!(read.0, "a050_002000", "the resolve reads the hold's window");
    assert!((read.2 - read.1 - DT).abs() < 1e-4 && read.1 > 0.0, "the hold's last step: {read:?}");
    let d = &app.world().resource::<Combat>().player;
    assert!(d.flag(&read.0, At::Step(read.1, read.2), crate::player::FLAG_SHIELD_BLOCK));
    assert!(t[i].log.contains("blocked"), "blocked as the exe: {}", t[i].log);
}
