//! End-user deathblow reproductions using the extracted Owl and castle roof.

use glam::Vec3;
use sekiro_sim::arena::{Arena, ArenaSpec, yaw_toward};
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::input::angle_diff;
use sekiro_sim::{Buttons, InputFrame};

const DT: f32 = 1.0 / 60.0;

fn load(map: bool) -> Option<(Duel, Option<Arena>)> {
    let cache = sekiro_sim::cache::root();
    if !cache.join("anim/c5060/tae.json").is_file()
        || !cache.join("raw/script/m11_01_00_00.luabnd.d").is_dir()
        || (map && !cache.join("maps/m11_01_00_00/collision.bin").is_file())
    {
        eprintln!("skipping: the Owl or requested castle roof is not extracted");
        return None;
    }
    let mut duel = Duel::load_with(&cache, 1.6, EnemySpec::owl()).unwrap();
    duel.enemy_active = false;
    // Without his map events: the first deathblow would start his phase-2 heat-up partway
    // through the get-up clip (`owl_phase2.rs`), and these tests follow the whole throw pair.
    duel.events = None;
    let arena =
        map.then(|| Arena::load(&cache, ArenaSpec::by_name("owl").unwrap(), false).unwrap());
    if let Some(arena) = &arena {
        arena.place(&mut duel);
    }
    Some((duel, arena))
}

#[derive(Clone, Debug)]
struct Sample {
    paired_body: bool,
    positions: [Vec3; 2],
    yaws: [f32; 2],
    /// Wolf's full-body clip and its time.
    wolf_clip: Option<(String, f32)>,
}

fn sample(duel: &Duel) -> Sample {
    Sample {
        paired_body: duel
            .deathblow
            .as_ref()
            .is_some_and(|d| d.phase == sekiro_sim::deathblow::Phase::Body),
        positions: [
            duel.player.body.position.into(),
            duel.enemy.body.position.into(),
        ],
        yaws: [duel.player.body.yaw, duel.enemy.body.yaw],
        wolf_clip: duel
            .player
            .behavior
            .runtime
            .main_clip()
            .map(|c| (c.animation.clone(), c.time)),
    }
}

/// Wolf is in one of the Owl deathblow's clips (`a221_*`, the ThrowParam attacker offset).
fn in_throw_clip(s: &Sample) -> bool {
    s.wolf_clip
        .as_ref()
        .is_some_and(|(c, _)| c.starts_with("a221_5"))
}

fn offset(s: &Sample) -> Vec3 {
    let d = s.positions[1] - s.positions[0];
    let (sin, cos) = s.yaws[0].sin_cos();
    Vec3::new(d.x * cos - d.z * sin, d.y, d.x * sin + d.z * cos)
}

fn start(duel: &mut Duel) {
    let mut broke = false;
    for i in 0..1500 {
        let wolf = &mut duel.rules.fighters[0];
        wolf.vitality.hp = wolf.vitality.max_hp;
        wolf.posture.remaining = wolf.posture.max;
        if !broke {
            let posture = &mut duel.rules.fighters[1].posture;
            posture.remaining = posture.remaining.min(1);
        }
        let report = duel.step(
            &InputFrame {
                buttons: Buttons {
                    attack: if broke { i % 10 == 0 } else { i % 40 == 0 },
                    ..Buttons::default()
                },
                ..InputFrame::default()
            },
            DT,
        );
        assert!(report.player.behavior.script_errors.is_empty());
        assert!(report.enemy.behavior.script_errors.is_empty());
        broke |= report
            .hits
            .iter()
            .any(|(h, r)| h.attacker == 0 && r.posture_broke);
        if let Some(rows) = report.deathblow.started {
            assert!(broke, "deathblow must follow an actual posture break");
            assert_eq!(rows, (15_060_000, 15_060_001));
            eprintln!("deathblow starts: {:?}", sample(duel));
            return;
        }
    }
    panic!("could not start Owl deathblow: {:?}", sample(duel));
}

fn track(duel: &mut Duel, steering: bool) -> Vec<Sample> {
    let mut track = vec![sample(duel)];
    let mut hit = false;
    for i in 0..1200 {
        let angle = i as f32 * 0.17;
        let input = if steering {
            InputFrame {
                move_stick: [angle.sin(), angle.cos()],
                camera_yaw: angle * 0.73,
                ..InputFrame::default()
            }
        } else {
            InputFrame::default()
        };
        let r = duel.step(&input, DT);
        assert!(r.player.behavior.script_errors.is_empty());
        assert!(r.enemy.behavior.script_errors.is_empty());
        hit |= r.deathblow.hit;
        track.push(sample(duel));
        let throwing = [&r.player, &r.enemy]
            .iter()
            .any(|r| r.behavior.state_path.iter().any(|s| s.contains("Throw")));
        if duel.deathblow.is_none() && !throwing {
            assert!(hit, "throw ended without landing");
            eprintln!("throw finished after {} steps, steering={steering}", i + 1);
            return track;
        }
    }
    panic!("throw animations did not finish");
}

/// Wolf's body clip `a221_510000` opens its move cancel (TAE `ChrActionFlag` 11) at 4.0 s; from
/// then on the stick may walk him out of it, as in the game. Until then neither character may
/// follow the stick, and the Owl never follows Wolf at all.
const BODY_MOVE_CANCEL: f32 = 4.0;

#[test]
fn deathblow_ignores_player_steering() {
    let Some((mut baseline, _)) = load(false) else {
        return;
    };
    let (mut steered, _) = load(false).unwrap();
    start(&mut baseline);
    start(&mut steered);
    let reference = track(&mut baseline, false);
    let actual = track(&mut steered, true);
    let mut max_position = [0.0_f32; 2];
    let mut max_yaw = [0.0_f32; 2];
    let mut max_offset = 0.0_f32;
    let mut first_bad = None;
    let mut released = None;
    for (step, (a, b)) in reference.iter().zip(&actual).enumerate() {
        let wolf_free = !in_throw_clip(b);
        if wolf_free && released.is_none() && step > 0 && in_throw_clip(&actual[step - 1]) {
            released = actual[step - 1].wolf_clip.clone();
        }
        for actor in 0..2 {
            // Once his clip lets him move, Wolf is the player's again.
            if actor == 0 && (wolf_free || released.is_some()) {
                continue;
            }
            let position = a.positions[actor].distance(b.positions[actor]);
            let yaw = angle_diff(a.yaws[actor], b.yaws[actor]).abs().to_degrees();
            max_position[actor] = max_position[actor].max(position);
            max_yaw[actor] = max_yaw[actor].max(yaw);
            if position > 0.05 || yaw > 2.0 {
                first_bad.get_or_insert(format!("step {step}, actor {actor}: {a:?} vs {b:?}"));
            }
        }
        if !in_throw_clip(b) || released.is_some() {
            continue;
        }
        let distance = offset(a).distance(offset(b));
        max_offset = max_offset.max(distance);
        if distance > 0.05 {
            first_bad.get_or_insert(format!(
                "offset at step {step}: {:?} vs {:?}",
                offset(a),
                offset(b)
            ));
        }
    }
    eprintln!(
        "max position errors (m): {max_position:?}; yaw (degrees): {max_yaw:?}; local offset error: {max_offset:.4}; Wolf left the throw from {released:?}"
    );
    assert!(first_bad.is_none(), "{}", first_bad.unwrap());
    if let Some((clip, time)) = &released {
        assert!(
            clip == "a221_510000" && *time >= BODY_MOVE_CANCEL - 0.02,
            "the stick took Wolf out of {clip} at {time:.3} s, before its move cancel"
        );
    }
    assert_eq!(reference.len(), actual.len(), "throw durations differ");
}

#[test]
fn deathblow_keeps_owl_attached_to_the_throw() {
    let Some((mut duel, _)) = load(false) else {
        return;
    };
    start(&mut duel);
    let samples = track(&mut duel, false);
    // The animated pair's independent model-space attachment is not exposed by Duel.
    // Use the requested smoothness fallback, including entry and release of the pair.
    let offsets: Vec<_> = samples.iter().map(offset).collect();
    for (step, value) in offsets.iter().enumerate() {
        eprintln!("offset {step}: {value:.4}");
    }
    let (step, jump) = offsets
        .windows(2)
        .enumerate()
        .map(|(i, p)| (i + 1, p[0].distance(p[1])))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    eprintln!("maximum local offset jump: {jump:.4} m at step {step}");
    assert!(
        jump <= 0.3,
        "offset jump at {step}: {:?} -> {:?} ({jump:.4} m)",
        offsets[step - 1],
        offsets[step]
    );
}

#[test]
fn deathblow_near_a_wall_does_not_clip() {
    wall_case(WallCase::OwlBacked);
}

/// Wolf with his back to the wall and closer to the Owl than the Owl's sorb dummy (1.3 m): the
/// absorb would put Wolf inside the wall, so the pair has to give way the other side.
#[test]
fn deathblow_with_wolf_backed_to_a_wall_does_not_clip() {
    wall_case(WallCase::WolfBacked);
}

#[derive(Clone, Copy, Debug)]
enum WallCase {
    /// The Owl 0.5 m from the wall, Wolf 1.6 m in front of him.
    OwlBacked,
    /// Wolf 0.4 m from the wall, the Owl 1.0 m in front of him.
    WolfBacked,
}

fn wall_case(case: WallCase) {
    let Some((mut duel, Some(arena))) = load(true) else {
        return;
    };
    let center = arena.floor_at(arena.layout.center());
    let mut placement = None;
    let mut wall = None;
    for i in 0..360 {
        let angle = (i as f32).to_radians();
        let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
        let Some(distance) = arena.world.raycast(center + Vec3::Y * 0.8, direction, 25.0) else {
            continue;
        };
        if distance < 3.0 {
            continue;
        }
        let (owl, wolf) = match case {
            WallCase::OwlBacked => {
                let owl = center + direction * (distance - 0.5);
                (owl, owl - direction * 1.6)
            }
            WallCase::WolfBacked => {
                let wolf = center + direction * (distance - 0.4);
                (wolf - direction * 1.0, wolf)
            }
        };
        let floors = [owl, wolf].map(|p| arena.world.floor(p.x, p.z, center.y + 0.3));
        if floors
            .iter()
            .all(|h| h.is_some_and(|h| (h - center.y).abs() < 0.1))
        {
            placement = Some((owl, wolf));
            wall = Some((center + direction * distance, direction));
            break;
        }
    }
    let (owl, wolf) = placement.expect("no wall with a same-level walkable approach found");
    for (character, position, target) in
        [(&mut duel.player, wolf, owl), (&mut duel.enemy, owl, wolf)]
    {
        character.body.position = position.to_array();
        character.body.yaw = yaw_toward(position, target);
        character.body.fall_top = center.y;
    }
    eprintln!(
        "wall setup {case:?}: Wolf {wolf:?}, Owl {owl:?}, floor {:.4}",
        center.y
    );
    start(&mut duel);
    if let (WallCase::WolfBacked, Some(wall)) = (case, wall) {
        // The breaking slashes push the pair around; put them back for the throw itself, Wolf
        // 0.6 m from the wall (just clear of it) and the Owl close in front, so the absorb (which moves Wolf back
        // to 1.3 m from the Owl) runs into the wall.
        let (point, direction) = wall;
        let on_floor = |p: Vec3| {
            Vec3::new(
                p.x,
                arena.world.floor(p.x, p.z, p.y + 0.5).unwrap_or(p.y),
                p.z,
            )
        };
        let wolf = on_floor(point - direction * 0.6);
        let owl = on_floor(wolf - direction * 1.2);
        for (character, position, target) in
            [(&mut duel.player, wolf, owl), (&mut duel.enemy, owl, wolf)]
        {
            character.body.position = position.to_array();
            character.body.yaw = yaw_toward(position, target);
        }
    }
    let samples = track(&mut duel, false);
    let body_start = samples
        .iter()
        .position(|s| {
            s.wolf_clip
                .as_ref()
                .is_some_and(|(c, _)| c == "a221_510000")
        })
        .expect("no body clip");
    let gap = |s: &Sample| {
        let d = s.positions[1] - s.positions[0];
        d.x.hypot(d.z)
    };
    eprintln!(
        "{case:?}: gap at body start {:.3} m, after the absorb {:.3} m",
        gap(&samples[body_start]),
        gap(&samples[body_start + 36])
    );
    // 0.6 s into the body the pair stands 1.10 m apart in the open (the Owl's dummy 230 plus
    // both clips' root motion); a wall behind Wolf must not squeeze that.
    assert!(
        (gap(&samples[body_start + 36]) - 1.10).abs() < 0.02,
        "the absorb did not keep the pair's spacing"
    );
    let mut violations = Vec::new();
    for (step, pair) in samples.windows(2).enumerate() {
        for actor in 0..2 {
            let from = pair[0].positions[actor];
            let to = pair[1].positions[actor];
            let delta = to - from;
            let length = delta.length();
            if length < 1e-5 {
                continue;
            }
            // Lift feet slightly to avoid reporting motion along the supporting floor.
            for height in [0.05, 0.8] {
                if let Some(hit) =
                    arena
                        .world
                        .raycast(from + Vec3::Y * height, delta / length, length)
                    && hit > 1e-4
                    && hit < length - 1e-4
                {
                    violations.push(format!("step {step}, actor {actor}, height {height}: {from:?} -> {to:?}, geometry at {hit:.4}/{length:.4} m"));
                }
            }
        }
    }
    // The roof here is not one flat level: its tiles step between 53.95, 53.99, 54.00 and
    // 54.06 m, and next to the railing it rises 0.22 m over about a metre (sampled along Wolf's
    // path), so a character that walks a metre ends on a slightly different, still walkable,
    // level. What must not happen is a jump onto something: no step may carry a character more
    // than a stride sideways or up more than a tile step, and each must end standing on a floor.
    for (step, pair) in samples.windows(2).enumerate() {
        // Body entry may reserve wall space with one shared translation, before absorption.
        // It must keep the pair together; later animation steps retain the stride bound.
        if !pair[0].paired_body && pair[1].paired_body {
            let moves = [0, 1].map(|actor| {
                let d = pair[1].positions[actor] - pair[0].positions[actor];
                glam::Vec2::new(d.x, d.z)
            });
            assert!(moves[0].distance(moves[1]) < 0.1);
            assert!(moves.iter().all(|d| d.length() <= 4.1));
            continue;
        }
        for actor in 0..2 {
            let delta = pair[1].positions[actor] - pair[0].positions[actor];
            if delta.x.hypot(delta.z) > 0.2 || delta.y > 0.1 {
                violations.push(format!(
                    "step {step}, actor {actor}: jumped {delta:?} in one step"
                ));
            }
        }
    }
    for (actor, p) in samples.last().unwrap().positions.iter().enumerate() {
        // The capsule rests a few centimetres above the floor ray on the slope.
        let floor = arena.world.floor(p.x, p.z, p.y + 0.3);
        if !floor.is_some_and(|h| (p.y - h).abs() < 0.06) {
            violations.push(format!("final actor {actor}: {p:?}, floor {floor:?}"));
        }
    }
    eprintln!(
        "final positions: {:?}; {} collision/floor violations",
        samples.last().unwrap(),
        violations.len()
    );
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
