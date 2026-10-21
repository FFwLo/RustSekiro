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
    // Locked on (the game's lock flag): the enemy stands in for the target, at the recorded
    // target's offset from Wolf (export lock_dx / dz), its AI off and never killed.
    let c_lock = head.iter().position(|h| *h == "lock");
    let (c_ldx, c_ldz) = (head.iter().position(|h| *h == "lock_dx"), head.iter().position(|h| *h == "lock_dz"));
    let mut app = app();
    move_enemy_far(&mut app);
    // The recordings ran with emblems and the gourd topped up (SekiroTool): no run-outs here either.
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.infinite_emblems = true;
    {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&mut Player, With<Player>>();
        q.single_mut(world).unwrap().gourd = 99;
    }
    let (mut art, mut tool) = (None, None);
    let mut prev = [false; 8];
    let mut rows = vec!["tick,anim,state,x,z,yaw,events".to_string()];
    let mut first = true;
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
            first = false;
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
        {
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
        app.update();
        let world = app.world_mut();
        let mut q = world.query_filtered::<(&Actor, &Transform), With<Player>>();
        let (a, t) = q.single(world).unwrap();
        // the clip on screen (procedural locomotion has no `anim` of its own)
        let shown = a.shown_clip(&world.resource::<Combat>().player).0;
        let (pos, yaw, state) = (t.translation, a.yaw, a.state.clone());
        // the HKS events the script fired this frame (W_...), compared with the game's hkbFireEvent log
        let events = world.get_non_send::<crate::hks_wolf::WolfScript>().map_or(String::new(), |w| w.fired.join(";"));
        rows.push(format!("{},{},{},{:.3},{:.3},{:.4},{}", f[c_tick], shown, state, pos.x, pos.z, yaw, events));
    }
    std::fs::write(&out, rows.join("\n") + "\n").expect("replay out");
}
