//! Replay of a real-game recording's inputs (tools/rec/replay.py): `python -m tools.rec replay <rec>` writes
//! SHINOBI_REPLAY (per tick: our-frame stick, buttons held, the real anim / position / yaw) and runs this
//! ignored test, which plays it into the sim and writes SHINOBI_REPLAY_OUT (per tick: our anim, state,
//! position, yaw) for the diff. No map; the one enemy, its AI off, stands in for the lock target while
//! the game was locked on (Wolf faces it; out of reach above him), else stands far off: only Wolf's own decisions are compared.
use super::*;

#[test]
#[ignore]
pub(crate) fn replay_recording() {
    let (Ok(inp), Ok(out)) = (std::env::var("SHINOBI_REPLAY"), std::env::var("SHINOBI_REPLAY_OUT")) else {
        eprintln!("SHINOBI_REPLAY / SHINOBI_REPLAY_OUT not set: nothing to replay");
        return;
    };
    let text = std::fs::read_to_string(&inp).expect("replay inputs");
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap_or("").split(',').collect();
    let col = |n: &str| head.iter().position(|h| *h == n).unwrap_or_else(|| panic!("column {n}"));
    let (c_tick, c_sx, c_sy, c_yaw) = (col("tick"), col("stick_x"), col("stick_y"), col("yaw"));
    // (column, press, the held field it drives)
    let buttons = [
        ("attack", Some(Action::Attack)),
        ("guard", Some(Action::Guard)),
        ("step", Some(Action::Step)),
        ("jump", Some(Action::Jump)),
        ("crouch", Some(Action::Crouch)),
        ("item", Some(Action::UseItem)),
        ("prosthetic", Some(Action::Prosthetic)),
        ("art", Some(Action::CombatArt)),
    ];
    let bcols: Vec<usize> = buttons.iter().map(|(n, _)| col(n)).collect();

    let c_art = head.iter().position(|h| *h == "art_id");
    let c_tool = head.iter().position(|h| *h == "tool_id");
    // A hit Wolf landed in the game (export hit_stop: the victim's hitStopType scale): the enemy is
    // out of reach here, so his hit stop starts from the clip's attack (combat.rs start_hit_stop).
    let c_stop = head.iter().position(|h| *h == "hit_stop");
    // Locked on (the game's lock flag): the enemy stands in for the target, at the recorded
    // target's offset from Wolf (export lock_dx / dz), its AI off and never killed.
    let c_lock = head.iter().position(|h| *h == "lock");
    let (c_ldx, c_ldz) = (head.iter().position(|h| *h == "lock_dx"), head.iter().position(|h| *h == "lock_dz"));
    // Enemy mode (SHINOBI_REPLAY_ENEMY=<chr>, tools/rec/enemies.py): that enemy fights with its AI
    // on from the recorded offset (SHINOBI_REPLAY_ENEMY_AT=dx,dz from Wolf), locked onto when the
    // game was; neither dies (both topped up each tick). Its anim, place and the HP / posture it
    // lost each tick are written too.
    let enemy_chr = std::env::var("SHINOBI_REPLAY_ENEMY").ok().filter(|c| !c.is_empty());
    let enemy_at = std::env::var("SHINOBI_REPLAY_ENEMY_AT").ok().and_then(|v| {
        let mut it = v.split(',').filter_map(|x| x.trim().parse::<f32>().ok());
        Some((it.next()?, it.next()?))
    });
    // (its NpcParam row, SHINOBI_REPLAY_ENEMY_ROW: a boss's AI and map script come from it)
    let enemy_row = std::env::var("SHINOBI_REPLAY_ENEMY_ROW").ok().and_then(|v| v.parse::<i64>().ok());
    let mut app = match &enemy_chr {
        Some(chr) => app_cfg(chr, enemy_row, Vec::new()),
        None => app(),
    };
    // The fighting enemy (sim_tests boss_in_play: not a disabled stand-in nor a boss's cast).
    let foe = enemy_chr.as_ref().map(|_| boss_in_play(app.world_mut()));
    // fighting as in the game, not the default deflect-practice (passive) mode
    if let Some(e) = foe {
        app.world_mut().get_mut::<crate::enemy::Enemy>(e).unwrap().aggressive = true;
    }
    if enemy_chr.is_none() {
        move_enemy_far(&mut app);
    }
    // The recordings ran with emblems and the gourd topped up (SekiroTool): no run-outs here either.
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.infinite_emblems = true;
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Player, With<Player>>();
        q.single_mut(world).unwrap().gourd = 99;
    }
    let (mut art, mut tool) = (None, None);
    let mut prev = [false; 8];
    let mut rows = vec![if enemy_chr.is_some() { "tick,anim,state,x,z,yaw,events,e_anim,e_state,e_x,e_z,e_hp_lost,e_posture_gain".to_string() } else { "tick,anim,state,x,z,yaw,events".to_string() }];
    let mut first = true;
    // SHINOBI_REPLAY_ENEMY_TODOME: "tick:dx:dz;..." (enemies.py: the game's a\d{3}_0137xx starts)
    const TODOME_TICKS: i64 = 120;
    let mut todome: Vec<(i64, f32, f32)> = std::env::var("SHINOBI_REPLAY_ENEMY_TODOME").unwrap_or_default().split(';').filter_map(|p| {
        let mut it = p.split(':');
        Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?))
    }).collect();
    let mut skip_until = i64::MIN;
    // SHINOBI_REPLAY_ENEMY_SP: "id:from:to;..." the SpEffects the game's map events put on it
    // (enemies.py; the replay runs no map events), worn from tick `from` to `to`.
    let map_sp: Vec<(i64, i64, i64)> = std::env::var("SHINOBI_REPLAY_ENEMY_SP").unwrap_or_default().split(';').filter_map(|p| {
        let mut it = p.split(':');
        Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?))
    }).collect();
    let mut map_sp_on = std::collections::HashSet::<i64>::new();
    for line in lines {
        let f: Vec<&str> = line.split(',').collect();
        let num = |i: usize| f.get(i).and_then(|s| s.parse::<f32>().ok());
        if first {
            // Wolf faces the recorded way at the start
            if let Some(yaw) = num(c_yaw) {
                let world = app.world_mut();
                let mut q = world.query_filtered::<&mut Actor, With<Player>>();
                q.single_mut(world).unwrap().yaw = yaw;
            }
            if let (Some(_), Some((dx, dz))) = (&enemy_chr, enemy_at) {
                place_foe(&mut app, foe.unwrap(), dx, dz);
            }
            first = false;
        }
        // The game's Todome on it (a boss's next phase): ours takes one at the same tick, then
        // stands where the game's stood from Wolf once it is over.
        let tick = f[c_tick].parse::<i64>().unwrap_or(0);
        if tick < skip_until {
            continue;
        }
        if let Some(foe) = foe.filter(|_| !map_sp.is_empty()) {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let mut ea = world.get_mut::<Actor>(foe).unwrap();
                let mut changed = false;
                for &(id, _, _) in &map_sp {
                    // (an id may come in several spans; only what the replay put on comes off:
                    // the Demon's resident 5305 shows in the game's log late, as he came in range)
                    let on = map_sp.iter().any(|&(i, from, to)| i == id && tick >= from && tick < to);
                    if on && !ea.resident.contains(&id) {
                        ea.resident.push(id);
                        map_sp_on.insert(id);
                        changed = true;
                    } else if !on && map_sp_on.remove(&id) {
                        ea.resident.retain(|&x| x != id);
                        changed = true;
                    }
                }
                if changed {
                    let d = combat.data_of(&ea);
                    ea.anim_group = crate::enemy::anim_group_of(d, ea.resident.iter().copied()).unwrap_or(0);
                }
            });
        }
        if let Some(&(at, dx, dz)) = todome.first().filter(|(at, _, _)| tick >= *at) {
            todome.remove(0);
            {
                let mut pad = app.world_mut().resource_mut::<PadInput>();
                pad.stick = Vec2::ZERO;
                (pad.attack_held, pad.guard_held, pad.dodge_held, pad.prosthetic_held) = (false, false, false, false);
            }
            break_and_deathblow(&mut app, 0);
            // pressed until it lands (the replayed Wolf may still be in a swing's recovery)
            // (a boss's first mark is a deathblow of its own, 12000, then its 20200 kneel waits
            // for the Todome; the game's came from the kneel at 0 HP)
            let mut k = 0;
            while k < TODOME_TICKS || (k < 900 && !enemy_state(&mut app).1.contains("_0137")) {
                let (s, an, _) = enemy_state(&mut app);
                if k % 6 == 0 && !an.contains("_0137") && !s.starts_with("ThrowDef") {
                    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
                }
                app.update();
                k += 1;
            }
            if std::env::var("SHINOBI_REPLAY_ENEMY_TRACE").is_ok() {
                eprintln!("todome {at}: enemy {:?}", enemy_state(&mut app));
            }
            place_foe(&mut app, foe.unwrap(), dx, dz);
            skip_until = at + k;
            continue;
        }
        // the combat art and the prosthetic the real Wolf had (export: their resident SpEffects)
        let a = c_art.and_then(|i| num(i)).map(|v| v as i64);
        if a.is_some() && a != art {
            art = a;
            app.world_mut().resource_mut::<crate::config::GameConfig>().player.combat_art = a.unwrap();
        }
        let t = c_tool.and_then(|i| num(i)).map(|v| v as i64);
        if t.is_some() && t != tool {
            tool = t;
            app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![t.unwrap()];
            let world = app.world_mut();
            world.query::<&mut Player>().single_mut(world).unwrap().tool_slot = 0;
        }
        let held: Vec<bool> = bcols.iter().map(|&i| f.get(i) == Some(&"1")).collect();
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            let stick = Vec2::new(num(c_sx).unwrap_or(0.0), num(c_sy).unwrap_or(0.0));
            // the recorded move vector is the game's own: about 1 when running, halved when walking
            pad.walk = stick.length() > 1e-3 && stick.length() < 0.75;
            pad.stick = stick.clamp_length_max(1.0);
            pad.attack_held = held[0];
            pad.guard_held = held[1];
            pad.dodge_held = held[2];
            pad.prosthetic_held = held[6];
            for (i, (_, act)) in buttons.iter().enumerate() {
                // the art is attack pressed with guard held: the game's art bit stands in for that attack press
                if held[i] && !prev[i] && !(i == 0 && held[7]) {
                    if let Some(a) = act {
                        pad.press(*a);
                    }
                }
            }
        }
        prev.copy_from_slice(&held[..8]);
        let target = (c_lock.and_then(|i| f.get(i)) == Some(&"1")).then(|| c_ldx.and_then(|i| num(i)).zip(c_ldz.and_then(|i| num(i)))).flatten();
        if enemy_chr.is_some() {
            let world = app.world_mut();
            world.resource_mut::<crate::camera::LockOn>().target = (c_lock.and_then(|i| f.get(i)) == Some(&"1")).then_some(foe.unwrap());
        } else {
            let world = app.world_mut();
            let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
            let mut q = world.query_filtered::<(Entity, &mut Transform, &mut Actor, &mut crate::enemy::Enemy), Without<Player>>();
            let (e, mut tf, mut ea, mut en) = q.single_mut(world).unwrap();
            en.ai_off = true;
            ea.hp = ea.hp_max;
            tf.translation = match target {
                // 40 m up: Wolf faces it (the lock's facing is flat) but never hits it.
                Some((dx, dz)) => Vec3::new(wolf.x + dx, wolf.y + 40.0, wolf.z + dz),
                None => Vec3::new(wolf.x, tf.translation.y, wolf.z + 60.0),
            };
            world.resource_mut::<crate::camera::LockOn>().target = target.map(|_| e);
        }
        let before = enemy_chr.as_ref().map(|_| {
            let world = app.world_mut();
            let ea = world.get::<Actor>(foe.unwrap()).unwrap();
            (ea.hp, ea.posture)
        });
        if let Some(scale) = c_stop.and_then(|i| num(i)).filter(|s| *s > 0.0 && enemy_chr.is_none()) {
            let world = app.world_mut();
            let anim = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().anim.clone();
            let time = world.resource::<Combat>().player.attack_windows_on(&anim, None).iter().map(|(_, atk, _)| atk.hit_stop_time).fold(0.0, f32::max);
            world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().hit_stop.start(time * scale);
        }
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(world).unwrap();
        // the clip on screen (procedural locomotion has no `anim` of its own)
        let shown = a.shown_clip(&world.resource::<Combat>().player).0;
        let (pos, yaw, state) = (t.translation, a.yaw, a.state.clone());
        // the HKS events the script fired this frame (W_...), compared with the game's hkbFireEvent log
        let events = world.get_non_send::<crate::hks_wolf::WolfScript>().map_or(String::new(), |w| w.fired.join(";"));
        let mut row = format!("{},{},{},{:.3},{:.3},{:.4},{}", f[c_tick], shown, state, pos.x, pos.z, yaw, events);
        if let Some(e) = foe.filter(|_| f[c_tick].parse::<i64>().is_ok_and(|t| t % 600 == 0) && std::env::var("SHINOBI_REPLAY_ENEMY_TRACE").is_ok()) {
            if let Some(en) = world.get::<crate::enemy::Enemy>(e) {
                eprintln!("enemy {} ai_off {} desc {} cmd {:?}", f[c_tick], en.ai_off, en.ai_desc, en.ai_cmd);
            }
            if let Some(ea) = world.get::<Actor>(e) {
                let y = world.get::<Transform>(e).map_or(0.0, |t| t.translation.y);
                eprintln!("  group {} y {y:.2} air {} vy {:.2} t {:.2} resident {:?}", ea.anim_group, ea.airborne, ea.vel_y, ea.t, ea.resident);
            }
        }
        if let Some((hp0, po0)) = before {
            // the enemy's line, then both topped up so the fight runs on
            let mut q = world.query_filtered::<(&mut Actor, &Transform), With<crate::enemy::Enemy>>();
            let (mut ea, et) = q.get_mut(world, foe.unwrap()).unwrap();
            row += &format!(",{},{},{:.3},{:.3},{:.1},{:.1}", ea.anim, ea.state, et.translation.x, et.translation.z, (hp0 - ea.hp).max(0.0), (ea.posture - po0).max(0.0));
            if ea.hp < ea.hp_max * 0.5 {
                ea.hp = ea.hp_max;
            }
            let mut q = world.query_filtered::<&mut Actor, With<Player>>();
            let mut wa = q.single_mut(world).unwrap();
            if wa.hp < wa.hp_max * 0.5 {
                wa.hp = wa.hp_max;
            }
        }
        rows.push(row);
    }
    std::fs::write(&out, rows.join("\n") + "\n").expect("replay out");
}

/// The replayed enemy at Wolf + (dx, dz), facing him.
fn place_foe(app: &mut App, foe: Entity, dx: f32, dz: f32) {
    let world = app.world_mut();
    let wolf = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().translation;
    let mut q = world.query_filtered::<(&mut Transform, &mut Actor), (With<crate::enemy::Enemy>, Without<Player>)>();
    let (mut tf, mut ea) = q.get_mut(world, foe).unwrap();
    tf.translation = Vec3::new(wolf.x + dx, tf.translation.y, wolf.z + dz);
    ea.yaw = crate::player::yaw_of(Vec3::new(-dx, 0.0, -dz).normalize_or(Vec3::NEG_Z));
}

#[test]
pub(crate) fn a_map_events_anim_set_outside_the_chrs_table_switches_it() {
    // The tutorial ronin's NpcParam resident 200034 is the a400 set; m11_00 event 11105275 puts
    // 200030 (a000) on it, an id its own SpEffect table lacks. The game's ronin plays a000 only.
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1012.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1012", Some(10120000), Vec::new());
    let world = app.world_mut();
    let b = boss_in_play(world);
    world.resource_scope(|world, combat: Mut<Combat>| {
        let a = world.get::<Actor>(b).unwrap();
        let d = combat.data_of(a);
        assert_eq!(a.anim_group, 4, "resident {:?}", a.resident);
        assert_eq!(crate::enemy::anim_group_of(d, a.resident.iter().copied().chain([200030])), Some(0));
    });
}


#[test]
pub(crate) fn lady_butterflys_jump_attack_lands() {
    // c9997 JumpAttackCommonFunction: out of the rise (ref 1000210) the jump lands on env 248
    // (IsTrulyLanding) into W_JumpAttack3202; unanswered, 3201's fall loop never ended.
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c5090.json")).exists() {
        return;
    }
    let mut app = app_cfg("c5090", Some(50900000), Vec::new());
    for _ in 0..30 {
        app.update();
    }
    let mut states = Vec::<String>::new();
    for f in 0..600 {
        // the AI's request of the jump (its script's Act), the AI itself off
        let world = app.world_mut();
        let b = boss_in_play(world);
        let mut e = world.get_mut::<Enemy>(b).unwrap();
        e.ai_off = true;
        e.ai_cmd = crate::ai::Command { anim: (f < 4).then_some(3200), ..Default::default() };
        app.update();
        let s = enemy_state(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    assert!(states.iter().any(|s| s == "JumpAttack3202"), "{states:?}");
}

#[test]
pub(crate) fn a_soldier_under_wolfs_sword_guards_and_deflects() {
    // Wolf locked on, walking in, swinging every 40 ticks (tools/rec/enemies.py pressure): the
    // General's Goal.Parry (102000_battle.lua) answers his attack notify with EndureAttack 3100 /
    // 3101, which cuts its hit reaction at TAE flag 122. Before: Wolf's own footsteps and swings
    // raised ChangeSoundTarget (46) every frame, the common handler replanned, and the guard
    // never played (86 % of his hits landed as damage; the game's recordings: 36 % guarded).
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/script/aicommon.luabnd.d")).exists() {
        return;
    }
    let mut app = app_cfg("c1020", Some(10202000), Vec::new());
    let foe = boss_in_play(app.world_mut());
    app.world_mut().get_mut::<crate::enemy::Enemy>(foe).unwrap().aggressive = true;
    place_foe(&mut app, foe, 0.0, -3.0);
    let mut guarded = Vec::new();
    for f in 0..3600 {
        let world = app.world_mut();
        world.resource_mut::<crate::camera::LockOn>().target = Some(foe);
        let mut pad = world.resource_mut::<PadInput>();
        pad.stick = Vec2::new(0.0, 0.5);
        if f > 60 && f % 40 == 0 {
            pad.press(Action::Attack);
        }
        app.update();
        let s = enemy_state(&mut app).0;
        if (s.starts_with("GuardDamage") || s.starts_with("JustGuard")) && guarded.last() != Some(&s) {
            guarded.push(s);
        }
    }
    assert!(guarded.len() >= 5, "{guarded:?}");
}

#[test]
pub(crate) fn the_general_answers_wolfs_block_with_his_kengeki_cuts() {
    // Wolf holding guard: a blocked swing stops in AttackNoBoundEnemy1 (8700 / 8701), whose own
    // weak "was repelled" row (200205 / 200206, TAE from 0.03 s) his Kengeki_Activate answers
    // (102000_battle.lua: 200206 -> 3056 or 3003). The AI replans one tick into the clip, so the
    // row has to be on from its first tick, as the game's is (combat.rs clash_after). Before,
    // the replan missed it and he walked into his normal acts (3007): the game's 2026-10-22 runs
    // answered 25 of 29 left blocks with 3056 / 3003.
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/script/aicommon.luabnd.d")).exists() {
        return;
    }
    let mut app = app_cfg("c1020", Some(10202000), Vec::new());
    let foe = boss_in_play(app.world_mut());
    app.world_mut().get_mut::<crate::enemy::Enemy>(foe).unwrap().aggressive = true;
    place_foe(&mut app, foe, 0.0, -2.5);
    let mut states: Vec<String> = Vec::new();
    for _ in 0..3600 {
        let world = app.world_mut();
        world.resource_mut::<crate::camera::LockOn>().target = Some(foe);
        world.resource_mut::<PadInput>().guard_held = true;
        app.update();
        let s = enemy_state(&mut app).0;
        if states.last() != Some(&s) {
            states.push(s);
        }
    }
    let after: Vec<&String> = states.windows(2).filter(|w| w[0] == "AttackNoBoundEnemy1_Left").map(|w| &w[1]).collect();
    let answered = after.iter().filter(|s| matches!(s.as_str(), "Ez3056" | "Ez3003")).count();
    assert!(answered >= 3 && answered * 2 > after.len(), "{after:?}");
}

#[test]
fn the_armoured_mace_follows_its_long_swing_with_3007() {
    // a200_003004's TAE holds 3118061 0.9-1.2 s; 118020_battle.lua picks Act10 (3007) while it is
    // on. Act03 runs 3004 with successDist nil (0), so with Wolf out of reach the attack goal fails
    // at the cancel window (flag 23 at 0.933 s, exe FUN_1405c6d30) and the replan sees 3118061.
    // Before, ours waited for the clip's end, when the TAE's SpEffects no longer count, and never
    // played 3007; the game's 2026-10-22 runs started it 55-57 ticks into all four of its 3004s.
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/script/aicommon.luabnd.d")).exists() {
        return;
    }
    let mut app = app_cfg("c1180", Some(11801100), Vec::new());
    let foe = boss_in_play(app.world_mut());
    app.world_mut().get_mut::<crate::enemy::Enemy>(foe).unwrap().aggressive = true;
    place_foe(&mut app, foe, 0.0, -6.5);
    let mut starts: Vec<(usize, String)> = Vec::new();
    for i in 0..3600 {
        let world = app.world_mut();
        world.resource_mut::<crate::camera::LockOn>().target = Some(foe);
        // Wolf kept 6-7 m off: the band where the script picks 3004 (Act03).
        let fp = world.get::<Transform>(foe).unwrap().translation;
        let mut wt = world.query_filtered::<&mut Transform, With<Player>>().single_mut(world).unwrap();
        let d = Vec3::new(fp.x - wt.translation.x, 0.0, fp.z - wt.translation.z);
        if d.length() > 7.0 {
            wt.translation += d.normalize() * 0.05;
        } else if d.length() < 6.0 {
            wt.translation -= d.normalize() * 0.05;
        }
        app.update();
        let a = enemy_state(&mut app).1;
        if starts.last().map(|s| &s.1) != Some(&a) {
            starts.push((i, a));
        }
    }
    let after: Vec<(usize, &str)> = starts.windows(2).filter(|w| w[0].1 == "a200_003004").map(|w| (w[1].0 - w[0].0, w[1].1.as_str())).collect();
    let chained = after.iter().filter(|(dt, a)| *a == "a200_003007" && (54..=60).contains(dt)).count();
    assert!(chained >= 3 && chained * 2 > after.len(), "{after:?}");
}

/// Atlas arts.ninjutsu-*: the worn ninjutsu (config player.ninjutsu) turns the killing stealth
/// deathblow into its twin row (ThrowParam main +1 / +2 / +3, ThrowKind 201-203), paid in
/// Spirit Emblems (6 / 5 / 7). Returns the clips Wolf played, the enemy's states, the emblems
/// left and the app.
fn ninjutsu_deathblow(ninjutsu: i64, emblems: u32, ticks: usize) -> (Vec<String>, Vec<String>, u32, App) {
    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.ninjutsu = ninjutsu;
    {
        let world = app.world_mut();
        let mut p = world.query::<&mut Player>().single_mut(world).unwrap();
        p.emblems = emblems;
    }
    make_enemy_unaware(&mut app, 1.5, true);
    app.world_mut().resource_mut::<PadInput>().press(Action::Attack);
    let (mut wolf, mut foe) = (Vec::new(), Vec::new());
    for _ in 0..ticks {
        app.update();
        let pa = player_state(&mut app).1;
        if wolf.last() != Some(&pa) {
            wolf.push(pa);
        }
        let es = enemy_state(&mut app).0;
        if foe.last() != Some(&es) {
            foe.push(es);
        }
    }
    let world = app.world_mut();
    let left = world.query::<&Player>().single(world).unwrap().emblems;
    (wolf, foe, left, app)
}

#[test]
pub(crate) fn the_bloodsmoke_ninjutsu_takes_the_stealth_deathblow_and_raises_the_smoke() {
    let (wolf, foe, left, mut app) = ninjutsu_deathblow(2100, 10, 150);
    assert!(wolf.iter().any(|s| s == "a201_510210"), "{wolf:?}\n{}", log_text(&app));
    assert!(foe.iter().any(|s| s == "ThrowDef12210"), "{foe:?}");
    assert_eq!(left, 4, "six emblems");
    // TAE 2 at frames 40-43: the clouds 300 (the blood reaction) and 301 (blind).
    let world = app.world_mut();
    let ids: Vec<i64> = world.query::<&crate::prosthetic::Projectile>().iter(world).map(|b| b.bullet_id()).collect();
    assert!(ids.contains(&300) && ids.contains(&301), "{ids:?}");
}

#[test]
pub(crate) fn without_the_emblems_the_ninjutsu_is_the_plain_deathblow() {
    let (wolf, foe, left, _) = ninjutsu_deathblow(2100, 5, 90);
    assert!(wolf.iter().any(|s| s == "a201_510200") && !wolf.iter().any(|s| s == "a201_510210"), "{wolf:?}");
    assert!(foe.iter().any(|s| s == "ThrowDef12200"), "{foe:?}");
    assert_eq!(left, 5);
}

#[test]
pub(crate) fn the_bestowal_ninjutsu_reddens_the_sword_for_thirty_seconds() {
    let (wolf, _, left, mut app) = ninjutsu_deathblow(2120, 10, 120);
    assert!(wolf.iter().any(|s| s == "a201_510230"), "{wolf:?}");
    assert_eq!(left, 3, "seven emblems");
    let world = app.world_mut();
    let p = world.query::<&Player>().single(world).unwrap();
    let t = p.timed.iter().find(|t| t.0 == 106040).map(|t| t.1);
    assert!(t.is_some_and(|t| t > 25.0 && t <= 30.0), "106040 on Wolf: {:?}", p.timed);
}

/// The Puppeteer: the enemy gets up from ThrowDef12220 with full HP on team 15 (220040,
/// charmed), immortal for 45 s (220030), then 220022 empties its HP and it dies.
#[test]
pub(crate) fn the_puppeteer_ninjutsu_raises_a_puppet_for_forty_five_seconds() {
    let (wolf, foe, left, mut app) = ninjutsu_deathblow(2110, 10, 60 * 8);
    assert!(wolf.iter().any(|s| s == "a201_510220"), "{wolf:?}");
    assert!(foe.iter().any(|s| s == "ThrowDef12220"), "{foe:?}");
    assert_eq!(left, 5, "five emblems");
    let alive = |app: &mut App| {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let (e, a) = world.query_filtered::<(&Enemy, &Actor), Without<Player>>().single(world).unwrap();
            (e.is_dead(), a.hp, crate::combat::team_of(&combat, a))
        })
    };
    let (dead, hp, team) = alive(&mut app);
    assert!(!dead && hp > 0.0 && team == 15, "a puppet: dead {dead} hp {hp} team {team} {foe:?}");
    let wolf_hp = |app: &mut App| {
        let world = app.world_mut();
        world.query_filtered::<&Actor, With<Player>>().single(world).unwrap().hp
    };
    let before = wolf_hp(&mut app);
    for _ in 0..60 * 40 {
        app.update();
    }
    assert_eq!(wolf_hp(&mut app), before, "the puppet leaves Wolf alone");
    let (dead, hp, _) = alive(&mut app);
    assert!(dead && hp <= 0.0, "dead at 48 s: dead {dead} hp {hp}");
}

/// The Bloodsmoke cloud's 220014 (sightSearchRate 0, hearingSearchEnemyCut 100) on an enemy:
/// facing Wolf 3 m away it never finds him; without it, it does.
#[test]
pub(crate) fn the_bloodsmokes_blindness_hides_wolf_from_an_enemy_facing_him() {
    for blind in [false, true] {
        let mut app = app();
        make_enemy_unaware(&mut app, 3.0, false);
        let mut found = false;
        for _ in 0..120 {
            if blind {
                let world = app.world_mut();
                let mut e = world.query_filtered::<&mut Enemy, Without<crate::enemy::Cast>>().single_mut(world).unwrap();
                e.add_timed(220014, 0.5);
            }
            app.update();
            let world = app.world_mut();
            let e = world.query_filtered::<&Enemy, Without<crate::enemy::Cast>>().single(world).unwrap();
            found |= e.targeting.state > crate::stealth::NONE;
        }
        assert_eq!(found, !blind, "blind {blind}");
    }
}
