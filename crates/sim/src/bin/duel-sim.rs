//! `duel-sim [--script FILE] [--steps N] [--passive] [--enemy c1010|c5060] [--simple-ai]
//! [--attack-power 1..99] [--progression castle-roof]
//! [--brain lua|scripted] [--brain-log] [--brain-memory off|on|PATH]`: Wolf (input script,
//! default: guard on and off) against an enemy (default the Ashina soldier) and its own AI.
//! Prints state changes, AI actions, unknown AI calls and every hit; `--goals` adds goal starts
//! and interrupts. `--brain` picks who chooses a boss's acts (`docs/BRAIN.md`); `--brain-log`
//! writes its JSONL log to `cache/brain_logs/`; `--brain-memory` keeps the memory of every
//! fight (`docs/BRAIN.md`).

use sekiro_sim::brain::BrainKind;
use sekiro_sim::duel::Duel;
use sekiro_sim::enemy::EnemySpec;
use sekiro_sim::lua_ai::AiEvent;
use sekiro_sim::{InputFrame, demo};

const DEFAULT: &str = "\
# stand still, then hold guard for a while, then attack the soldier
120 0 0 -
1 0 0 guard
300 0 0 guard
60 0 0 -
1 0 0 attack
40 0 0 -
1 0 0 attack
40 0 0 -
1 0 0 attack
200 0 0 -
";

fn main() {
    let mut script = None;
    let mut steps = None;
    let mut passive = false;
    let mut spec = EnemySpec::soldier();
    let mut simple = false;
    let mut goals = false;
    let mut brain = None;
    let mut brain_log = false;
    let mut brain_memory = None;
    let mut attack_power = None;
    let mut progression = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--attack-power" => {
                attack_power = Some(args.next().expect("--attack-power requires a value"))
            }
            "--progression" => {
                progression = Some(args.next().expect("--progression requires a value"))
            }
            "--script" => script = Some(std::fs::read_to_string(args.next().unwrap()).unwrap()),
            "--steps" => steps = args.next().and_then(|s| s.parse().ok()),
            "--passive" => passive = true,
            "--simple-ai" => simple = true,
            "--goals" => goals = true,
            "--brain" => brain = Some(BrainKind::by_name(&args.next().unwrap()).unwrap()),
            "--brain-log" => brain_log = true,
            "--brain-memory" => brain_memory = args.next(),
            "--enemy" => spec = EnemySpec::by_name(&args.next().unwrap()).unwrap(),
            other => panic!("unknown argument {other}"),
        }
    }
    let frames = demo::parse(script.as_deref().unwrap_or(DEFAULT)).unwrap();
    let power = sekiro_sim::progression::AttackPower::from_options(
        attack_power.as_deref(),
        progression.as_deref(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut duel = Duel::load_with(&sekiro_sim::cache::root(), 4.0, spec).expect("load");
    duel.rules
        .set_attack_power(power)
        .expect("attack power data");
    println!("Wolf attack power {}", power.value());
    duel.enemy_active = !passive;
    if simple {
        duel.use_simple_ai();
    }
    if brain.is_some() || brain_log || brain_memory.is_some() {
        let kind = brain.unwrap_or_default();
        let chr = duel.spec.chr.clone();
        match duel.set_brain(kind.build(&chr), brain_log) {
            Ok(Some(path)) => println!("brain {kind:?}, log {}", path.display()),
            Ok(None) => println!("brain {kind:?}"),
            Err(e) => panic!("--brain: {e}"),
        }
        if let (Some(path), Some(d)) = (
            sekiro_sim::brain::memory::resolve_for(
                brain_memory.as_deref(),
                &chr,
                &format!("{:?}", brain.unwrap_or_default()).to_ascii_lowercase(),
                false,
            ),
            duel.boss_brain.as_mut(),
        ) {
            d.use_memory(path);
        }
    }
    let mut planned = 0;
    println!(
        "player rig {} dummies, {} attacks; enemy rig {} dummies, {} attacks",
        duel.fighters[0].rig.len(),
        duel.fighters[0].attacks.len(),
        duel.fighters[1].rig.len(),
        duel.fighters[1].attacks.len()
    );
    let n = steps.unwrap_or(frames.len());
    let (mut lp, mut le) = (String::new(), String::new());
    for i in 0..n {
        let f = frames.get(i).copied().unwrap_or_default();
        let f = InputFrame { ..f };
        let r = duel.step(&f, 1.0 / 60.0);
        let kp = format!(
            "{} {}",
            r.player.behavior.state_path.join("/"),
            r.player.animation.as_deref().unwrap_or("-")
        );
        let ke = format!(
            "{} {}",
            r.enemy.behavior.state_path.join("/"),
            r.enemy.animation.as_deref().unwrap_or("-")
        );
        if kp != lp {
            println!("{i:5} WOLF    {kp}");
            lp = kp;
        }
        if ke != le {
            println!("{i:5} ENEMY   {ke}");
            le = ke;
        }
        for (h, res) in &r.hits {
            println!(
                "{i:5} HIT {} -> {} judge {} atk {} guarded={} deflected={} defender type {} level {} hp {:?}",
                if h.attacker == 0 {
                    "wolf".into()
                } else {
                    format!("enemy {}", h.attacker)
                },
                if h.defender == 0 {
                    "wolf".into()
                } else {
                    format!("enemy {}", h.defender)
                },
                h.judge,
                h.attack.atk_id,
                res.guarded,
                res.deflected,
                res.defender.damage_type,
                res.defender.level,
                [
                    duel.rules.fighters[0].vitality.hp,
                    duel.rules.fighters[1].vitality.hp
                ]
            );
        }
        for e in r
            .player
            .behavior
            .script_errors
            .iter()
            .chain(&r.enemy.behavior.script_errors)
        {
            println!("{i:5} SCRIPT ERROR {e}");
        }
        if let Some(ai) = duel.ai.as_mut() {
            for e in ai.drain_log() {
                match e {
                    AiEvent::Action(a) => println!("{i:5} AI action {a}"),
                    AiEvent::Unknown(_) | AiEvent::Error(_) => println!("{i:5} AI {e:?}"),
                    AiEvent::GoalStart { .. } | AiEvent::Interrupt(_) if goals => {
                        println!("{i:5} AI {e:?}")
                    }
                    _ => {}
                }
            }
        }
        if let Some(b) = &duel.boss_brain {
            for plan in &b.history[planned..] {
                println!("{i:5} BRAIN plan {plan:?}");
            }
            planned = b.history.len();
        }
    }
    if let Some(b) = &duel.boss_brain {
        println!("brain {}: {:?}", b.brain_name(), b.stats);
        println!(
            "tally: {}",
            serde_json::to_string(&b.tally.summary()).unwrap_or_default()
        );
    }
}
