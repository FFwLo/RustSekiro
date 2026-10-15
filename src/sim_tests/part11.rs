//! Sim tests, part 11: 15 tests, from `a_mid_stride_loop_swap_does_not_burst_its_sounds` to `throw_direction_effects_come_from_the_material_row`.
//! Shared helpers sit in whichever part defines them; every part sees all of them through `super::*`.

use super::*;

/// Round 65 (refuted lead, kept as a guard): a loop swapped mid-stride (walk -> run) keeps its
/// phase and its one-step window - locomotion_clip_at(prev_t) reads the current move_index, so
/// only the start clip's hand-over to the loop opens from frame 0.
#[test]
pub(crate) fn a_mid_stride_loop_swap_does_not_burst_its_sounds() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    let d = &combat.player;
    let mut a = crate::actor::Actor::new(crate::actor::Side::Player, 1.0, 1.0, 0.0, 0);
    a.procedural("Locomotion");
    a.move_index = 0;
    a.move_start = d.length(&a.move_clip(true));
    let walk = a.move_clip(false);
    a.move_index = 1;
    let run = a.move_clip(false);
    if walk == run || d.length(&walk) <= 0.0 || d.length(&run) <= 0.0 {
        return;
    }
    a.move_index = 0;
    let dt = 1.0 / 60.0;
    a.prev_t = a.move_start + 0.5;
    a.t = a.prev_t + dt;
    a.move_index = 1;
    let (clip, windows, now) = crate::sound::sound_windows(&a, d).expect("windows");
    assert_eq!(clip, run);
    assert!(windows.iter().all(|&(lo, hi)| hi - lo <= dt + 1e-4), "now {now}: {windows:?}");
}

/// Round 65: the "no posture damage" cheat keeps a single bar-filling hit from breaking Wolf
/// (it zeroed the posture only after Resolve had already broken the guard).
#[test]
pub(crate) fn the_no_posture_cheat_blocks_a_bar_filling_hit() {
    let mut app = app_cfg("c1000", None, Vec::new());
    app.init_resource::<crate::debug_menu::DebugMenu>().add_systems(FixedUpdate, crate::debug_menu::player_cheats);
    app.update();
    app.world_mut().resource_mut::<crate::debug_menu::DebugMenu>().no_posture = true;
    app.update();
    let world = app.world_mut();
    let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
    let max = a.posture_max;
    a.add_posture(max * 2.0);
    assert!(!a.posture_broken(), "posture {} / {max}", a.posture);
}

/// Round 66: an SpEffect's interval timer starts at 0 (FUN_140c06860 / FUN_140c03af0) and
/// FUN_140c06b70 ticks at <= 0, so poison 9045 (19.9 s, every 1 s) hurts on the frame it takes
/// hold and 20 times in all. Before, the first tick came a second late: 19 in all.
#[test]
pub(crate) fn poison_ticks_from_its_first_frame() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    let e = app.world_mut().query_filtered::<Entity, With<Enemy>>().single(app.world()).unwrap();
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        world.resource_mut::<crate::status::Statuses>().start_for(&combat, e, 9045);
    });
    let hp = |app: &mut App| app.world_mut().query_filtered::<&Actor, With<Enemy>>().single(app.world()).unwrap().hp;
    let mut last = hp(&mut app);
    let mut ticks = Vec::new();
    for f in 0..(22 * 60) {
        app.update();
        let now = hp(&mut app);
        if now < last {
            ticks.push(f);
        }
        last = now;
    }
    assert_eq!(ticks.first(), Some(&0), "first tick frame {ticks:?}");
    assert_eq!(ticks.len(), 20, "{ticks:?}");
}

/// The AI's view of Wolf (HasSpecialEffectId TARGET_ENE_0) includes his timed rows and his
/// lightning charge: the Owl's 506001_battle.lua zeroes its Divine Abduction weight (act 11) while
/// his seal 3506090 is on Wolf, and the Way of Tomoe reads her 9492 charge. Before, enemy.rs
/// think's snapshot took only his TAE-window / chained rows and afflictions (boss bug hunt
/// 2026-10-14: the Owl re-sealed an already sealed Wolf).
#[test]
pub(crate) fn the_ai_sees_wolfs_timed_rows_and_charge() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/script/aicommon.luabnd.d")).exists() {
        return;
    }
    let mut app = app();
    app.update();
    {
        let world = app.world_mut();
        world.query::<&mut Enemy>().single_mut(world).unwrap().aggressive = true;
        world.query::<&mut Player>().single_mut(world).unwrap().timed.push((3506090, 30.0));
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().electro = Some((9492, 30.0));
    }
    for _ in 0..30 {
        app.update();
        let world = app.world_mut();
        world.query::<&mut Player>().single_mut(world).unwrap().timed.retain(|t| t.0 != 3506090);
        world.query::<&mut Player>().single_mut(world).unwrap().timed.push((3506090, 30.0));
        world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap().electro = Some((9492, 30.0));
    }
    let world = app.world_mut();
    let e = world.query::<&Enemy>().single(world).unwrap();
    assert!(e.prev_sp.contains(&3506090), "no seal in the snapshot: {:?}", e.prev_sp);
    assert!(e.prev_sp.contains(&9492), "no charge in the snapshot: {:?}", e.prev_sp);
}

/// Round 66: the Great Shinobi Owl's recovery seal 3506090 (stateInfo 910, 30 s) is one of goods
/// 3000's useLimitSpEffectType rows: the gourd press is W_ItemFailed (HKS 3940-3951), no drink and
/// no charge (its clip a000_259000 is not exported: the press is only spent). The death loop's refresh 8002 -> 8007 (spCategory 193) takes the seal off.
#[test]
pub(crate) fn the_owls_recovery_seal_stops_the_gourd() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.update();
    let gourd = |app: &mut App| app.world_mut().query::<&Player>().single(app.world()).unwrap().gourd;
    let before = gourd(&mut app);
    app.world_mut().query::<&mut Player>().single_mut(app.world_mut()).unwrap().timed.push((3506090, 30.0));
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    app.update();
    let mut states = Vec::new();
    for _ in 0..90 {
        states.push(player_actor(&mut app).0);
        app.update();
    }
    assert!(!states.iter().any(|s| s.starts_with("ItemGourdDrink")), "{states:?}");
    assert_eq!(gourd(&mut app), before);
    // The stay loop's f0 refresh lifts it.
    {
        let world = app.world_mut();
        world.resource_scope(|world, combat: Mut<Combat>| {
            let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
            a.hp = 0.0;
            a.play_state(&combat.player, "GroundRevivalStayLoop_F");
        });
    }
    app.update();
    app.update();
    let timed = app.world_mut().query::<&Player>().single(app.world()).unwrap().timed.clone();
    assert!(!timed.iter().any(|t| t.0 == 3506090), "{timed:?}");
}

/// Round 68: no tool switch while ref 316 (SpEffect 100393, the umbrella's guard / release clips)
/// is up (HKS 6393): a mid-clip switch costed the rest of the clip with the new tool.
#[test]
pub(crate) fn no_tool_switch_in_the_umbrellas_release_window() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![70000, 72000];
    app.update();
    let found = app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        let d = &combat.player;
        let hit = d.anims.keys().filter(|k| k.starts_with("a076_405")).find_map(|k| (0..90).map(|i| i as f32 / 30.0).find(|t| d.sp_effects_at(k, *t).iter().any(|(id, _)| *id == 100393)).map(|t| (k.clone(), t)));
        let Some((k, t)) = hit else { return false };
        let mut a = world.query_filtered::<&mut Actor, With<Player>>().single_mut(world).unwrap();
        a.play("UmbrellaProbe", &k);
        a.t = t;
        a.prev_t = t;
        true
    });
    if !found {
        return;
    }
    let slot = |app: &mut App| app.world_mut().query::<&Player>().single(app.world()).unwrap().tool_slot;
    let before = slot(&mut app);
    app.world_mut().resource_mut::<PadInput>().press(Action::SwitchTool);
    app.update();
    assert_eq!(slot(&mut app), before);
}

/// Round 69: crouched, the new tool unfolds in the crouch expand (CrouchSubWeaponExpand1,
/// a071_412100 for the Firecracker; BEH_R_SUB_WEAPON_EXPAND takes HKB_STATE_CROUCH_IDLE).
#[test]
pub(crate) fn a_crouched_switch_unfolds_crouched() {
    let mut app = app();
    {
        let world = app.world_mut();
        world.resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![70000, 71000];
        let mut q = world.query_filtered::<&mut Transform, With<Enemy>>();
        q.single_mut(world).unwrap().translation.z = 30.0;
    }
    for _ in 0..30 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::Crouch);
    for _ in 0..60 {
        app.update();
    }
    app.world_mut().resource_mut::<PadInput>().press(Action::SwitchTool);
    let mut seen = Vec::new();
    for _ in 0..240 {
        app.update();
        let s = player_state(&mut app).0;
        if seen.last() != Some(&s) {
            seen.push(s);
        }
    }
    assert!(seen.iter().any(|s| s == "CrouchSubWeaponExpand1"), "{seen:?}");
    assert!(!seen.iter().any(|s| s == "SubWeaponExpand"), "{seen:?}");
}

/// Round 70: an enemy's NpcParam resident SpEffects gate its TAE sounds / FFX (c1180's resident
/// 200030 -> stateInfo 270: 236 PlaySound events); only Wolf's residents were counted.
#[test]
pub(crate) fn an_enemys_resident_gates_its_sounds() {
    if !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/extracted/enemies/c1180.json")).exists() {
        return;
    }
    let mut app = app_cfg("c1180", None, Vec::new());
    app.update();
    let world = app.world_mut();
    world.resource_scope(|world, combat: Mut<Combat>| {
        let a = world.query_filtered::<&Actor, With<Enemy>>().single(world).unwrap();
        assert!(a.resident.contains(&200030), "residents {:?}", a.resident);
        let gates = crate::sound::active_state_infos(&combat, a, None, None);
        assert!(gates.contains(&270), "{gates:?}");
    });
}

/// Round 71: a clip's anim-set SpEffect (200031-200034, spCategory 100) overwrites the resident
/// one (200030 -> 270): only its own stateInfo gates the clip's sounds, not both sets.
#[test]
pub(crate) fn a_clips_anim_set_overwrites_the_resident_gate() {
    for chr in ["c1180", "c1220", "c1191", "c7500", "c1260"] {
        if !std::path::Path::new(&format!("{}/extracted/enemies/{chr}.json", env!("CARGO_MANIFEST_DIR"))).exists() {
            continue;
        }
        let mut app = app_cfg(chr, None, Vec::new());
        app.update();
        let found = app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
            let mut a = world.query_filtered::<&mut Actor, With<Enemy>>().single_mut(world).unwrap();
            let res: Vec<i64> = a.resident.iter().copied().filter(|id| (200030..=200034).contains(id)).collect();
            let Some(&r) = res.first() else { return false };
            let d = crate::actor::data_for(&combat, &a);
            let hit = d.anims.keys().find_map(|k| (0..60).map(|i| i as f32 / 30.0).find(|t| d.sp_effects_at(k, *t).iter().any(|(id, _)| (200030..=200034).contains(id) && *id != r)).map(|t| (k.clone(), t)));
            let Some((k, t)) = hit else { return false };
            let own = d.sp_effects_at(&k, t).iter().find(|(id, _)| (200030..=200034).contains(id)).map(|(_, s)| s.state_info).unwrap();
            let old = crate::data::sp_cycle(r).unwrap().state_info;
            a.play("Probe", &k);
            a.t = t;
            let gates = crate::sound::active_state_infos(&combat, &a, None, None);
            assert!(gates.contains(&own) && !gates.contains(&old), "{chr} {k}@{t}: {gates:?} (own {own}, resident {old})");
            true
        });
        if found {
            return;
        }
    }
}

/// Round 73: EMEVD immortality on Wolf (WolfImmortal) is the exe's NoDead clamp in SetHp: HP that
/// would drop below 1 from above 0 is 1; not immortal, or already at 0, nothing changes.
#[test]
pub(crate) fn an_immortal_wolf_stays_at_1_hp() {
    use crate::player::no_dead_clamp;
    assert_eq!(no_dead_clamp(true, 50.0, -200.0), Some(1.0));
    assert_eq!(no_dead_clamp(true, 50.0, 0.5), Some(1.0));
    assert_eq!(no_dead_clamp(true, 50.0, 10.0), None);
    assert_eq!(no_dead_clamp(false, 50.0, -200.0), None);
    assert_eq!(no_dead_clamp(true, 0.0, -5.0), None);
}

/// Round 76: BEH_R_LAND 2026 - every non-sprint ground jump sets EnablePositioningJumpLand
/// (HKS 2800), so a vertical jump off a 2.3 m ledge (plus its own rise: 3-5 m) lands stiff (LandFreeFall), not only the
/// locked-on jumps; and a sprint jump (SprintJumpStart lasts to the ground) from 6 m too.
#[test]
pub(crate) fn jumps_off_ledges_land_stiff() {
    // (how, extra drop, want)
    for (sprint, drop) in [(false, 2.3f32), (true, 6.0)] {
        let mut app = app();
        move_enemy_far(&mut app);
        app.update();
        if sprint {
            let world = app.world_mut();
            world.resource_scope(|world, combat: Mut<Combat>| {
                let (mut a, mut t) = world.query_filtered::<(&mut Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                assert!(a.play_state(&combat.player, "SprintJumpStart"));
                a.airborne = true;
                a.vel_y = 0.0;
                t.translation.y += drop;
            });
        } else {
            app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
            let mut up = false;
            for _ in 0..60 {
                app.update();
                let world = app.world_mut();
                let (a, mut t) = world.query_filtered::<(&Actor, &mut Transform), With<Player>>().single_mut(world).unwrap();
                if a.airborne {
                    t.translation.y += drop;
                    up = true;
                    break;
                }
            }
            assert!(up, "never left the ground");
        }
        let mut landed = None;
        for _ in 0..600 {
            app.update();
            let world = app.world_mut();
            let a = world.query_filtered::<&Actor, With<Player>>().single(world).unwrap();
            if !a.airborne {
                landed = Some(a.state.clone());
                break;
            }
        }
        assert_eq!(landed.as_deref(), Some("LandFreeFall"), "sprint {sprint} drop {drop}");
    }
}

/// Round 76: a locked-on jump flies along the stick, not its sector's fixed line
/// (_setPositioningJumpTilt, act 3025: angle - centre). A stick 30 deg right of the target is the
/// F_R jump (centre 45) tilted back to 30.
#[test]
pub(crate) fn a_locked_jump_follows_the_sticks_angle() {
    let mut app = app();
    let enemy = {
        let world = app.world_mut();
        let (e, mut t) = world.query_filtered::<(Entity, &mut Transform), With<Enemy>>().single_mut(world).unwrap();
        t.translation.z = -30.0;
        e
    };
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.world_mut().resource_mut::<crate::camera::LockOn>().target = Some(enemy);
    for _ in 0..20 {
        app.update();
    }
    let (start, yaw) = {
        let world = app.world_mut();
        let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        (t.translation, a.yaw)
    };
    let mut angles = Vec::new();
    for (i, deg) in [30.0f32].iter().enumerate() {
        let _ = i;
        let r = deg.to_radians();
        app.world_mut().resource_mut::<PadInput>().stick = Vec2::new(r.sin(), r.cos());
        app.world_mut().resource_mut::<PadInput>().press(Action::Jump);
        let mut states = Vec::new();
        for _ in 0..45 {
            app.update();
            let s = player_state(&mut app).0;
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        let world = app.world_mut();
        let (a, t) = world.query_filtered::<(&Actor, &Transform), With<Player>>().single(world).unwrap();
        let moved = (t.translation - start).with_y(0.0);
        let local = Quat::from_rotation_y(-a.yaw) * moved;
        let got = local.x.atan2(-local.z).to_degrees();
        angles.push((got, states, yaw));
    }
    let (got, states, _) = &angles[0];
    assert!(states.iter().any(|s| s.contains("F_R")), "{states:?}");
    assert!((got - 30.0).abs() < 8.0, "moved at {got:.1} deg (want ~30, sector 45): {states:?}");
}

/// Round 79: Aging 9620 (maxHpRate 0.125) on an enemy that dies and respawns in place: the
/// respawn set hp from the aged bar, then the cleared affliction's set_aging(None) multiplied
/// hp_max back up, so the enemy came back at 1/8 of its bar.
#[test]
pub(crate) fn an_aged_enemy_respawns_at_a_full_bar() {
    let mut app = app();
    move_enemy_far(&mut app);
    app.world_mut().resource_mut::<crate::enemy::EnemyDebug>().mode = crate::enemy::AiMode::Idle;
    app.update();
    let e = app.world_mut().query_filtered::<Entity, With<Enemy>>().single(app.world()).unwrap();
    let max0 = app.world().get::<Actor>(e).unwrap().hp_max;
    app.world_mut().resource_scope(|world, combat: Mut<Combat>| {
        world.resource_mut::<crate::status::Statuses>().start_for(&combat, e, 9620);
    });
    app.update();
    assert!(app.world().get::<Actor>(e).unwrap().hp_max < max0, "not aged");
    app.world_mut().get_mut::<Actor>(e).unwrap().hp = 0.0;
    let mut died = false;
    for _ in 0..(60 * 60) {
        app.update();
        let Some(en) = app.world().get::<Enemy>(e) else { break };
        died |= en.is_dead();
        if died && !en.is_dead() {
            break;
        }
    }
    assert!(died, "never died");
    for _ in 0..5 {
        app.update();
    }
    let a = app.world().get::<Actor>(e).unwrap();
    assert_eq!(a.hp_max, max0);
    assert_eq!(a.hp, a.hp_max);
}

/// TAE effect events (SpawnOneShotFFX 96, SpawnFFX_ByFloor 112, SpawnFFX_Blade 118) whose start
/// each actor crosses: who, anim, FFXID, dummy. Test-side only (the scripted-kill effect check,
/// HANDOFF 4.57).
#[derive(Resource, Default)]
pub(crate) struct FxSeen(std::collections::BTreeSet<String>);

pub(crate) fn record_fx(combat: Res<Combat>, mut seen: ResMut<FxSeen>, q: Query<(&Actor, Has<Player>)>) {
    for (a, wolf) in &q {
        if a.anim.is_empty() || a.t < a.prev_t {
            continue;
        }
        let d = crate::actor::data_for(&combat, a);
        // The clip the game plays (ffx.rs: an anim id may resolve to another set's clip).
        let Some((clip, windows, _)) = crate::sound::sound_windows(a, d) else { continue };
        let Some(anim) = d.anim(&clip) else { continue };
        for e in anim.events.iter().filter(|e| matches!(e.kind, 96 | 112 | 118) && windows.iter().any(|&(lo, hi)| e.start > lo && e.start <= hi)) {
            let ffx = e.arg_i64("FFXID").unwrap_or(0);
            if e.kind == 112 || ffx <= 0 {
                continue;
            }
            let dmy = e.arg_i64(if e.kind == 118 { "DummyPolyBladeBaseID" } else { "DummyPolyID" }).unwrap_or(-1);
            let si = e.arg_i64("StateInfo").unwrap_or(0);
            seen.0.insert(format!("{} {} {ffx} on {dmy} si {si}", if wolf { "wolf" } else { "enemy" }, clip));
        }
    }
}

/// Manual: the scripted kills' effects (Monk, Ape, Old Dragons), run with --ignored --nocapture.
#[test]
#[ignore]
pub(crate) fn zz_scripted_kill_effects() {
    for (chr, row, rounds, wait) in [("c5000", 50000000, 4, 900), ("c5100", 51000000, 4, 1800), ("c5100", 51000100, 3, 1200), ("c5300", 53000000, 3, 900)] {
        if !std::path::Path::new(&format!("{}/extracted/enemies/{chr}.json", env!("CARGO_MANIFEST_DIR"))).exists() {
            continue;
        }
        let mut app = app_cfg(chr, Some(row), Vec::new());
        app.init_resource::<FxSeen>().add_systems(Update, record_fx);
        for _ in 0..30 {
            app.update();
        }
        let mut states = Vec::new();
        for _ in 0..rounds {
            states.extend(break_and_deathblow(&mut app, 600));
            for _ in 0..wait {
                app.update();
            }
            let bs = app.world().resource::<crate::enemy::BossScript>();
            if bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()) {
                break;
            }
        }
        states.dedup();
        let bs = app.world().resource::<crate::enemy::BossScript>();
        eprintln!("KILLFX {chr} {row} defeated {} states {states:?}", bs.defeats > 0 || bs.run.as_ref().is_some_and(|r| r.defeated.is_some()));
        for l in &app.world().resource::<FxSeen>().0 {
            eprintln!("KILLFX   {l}");
        }
    }
}

/// The deathblow count drops at Wolf's atkAttribute 5 hit (judge 800, a211_511700 at 1.23 s;
/// FUN_140b6e6a0), not at the throw's start: until then the Monk's map script (4[37]) still sees
/// her last bar, so 12505963's 20200 (bars 0 + Wolf's latched message 10) cuts her Todome there.
#[test]
pub(crate) fn the_deathblow_count_drops_at_the_ninsatsu_hit() {
    let mut app = app_with("c5000");
    for _ in 0..2 {
        break_and_deathblow(&mut app, 500);
    }
    break_and_deathblow(&mut app, 1);
    let mut drop = None;
    let mut cut = None;
    for f in 0..300 {
        app.update();
        let (s, _, _) = enemy_state(&mut app);
        let (_, pa, pt, _) = player_state(&mut app);
        let w = app.world_mut();
        let b = boss_in_play(w);
        let left = w.get::<Enemy>(b).unwrap().ninsatsu_left();
        if drop.is_none() && left == 0 {
            drop = Some((pa.clone(), pt, f));
        }
        if cut.is_none() && s == "Event20200" {
            cut = Some(f);
        }
    }
    let (anim, t, f) = drop.expect("the count drops");
    assert!(anim.ends_with("511700") && (1.2..1.3).contains(&t), "dropped in {anim} at {t}");
    assert_eq!(cut, Some(f), "20200 on the drop");
}

#[test]
#[ignore]
pub(crate) fn zz_monk_todome_timing() {
    let mut app = app_with("c5000");
    for _ in 0..2 {
        break_and_deathblow(&mut app, 500);
    }
    break_and_deathblow(&mut app, 1);
    let mut last = String::new();
    for f in 0..900 {
        app.update();
        let (s, an, t) = enemy_state(&mut app);
        let (ps, pa, _, _) = player_state(&mut app);
        let extra = {
            let w = app.world_mut();
            let b = boss_in_play(w);
            let e = w.get::<Enemy>(b).unwrap();
            let held = w.resource::<crate::player::ActiveThrow>().0.map(|t| t.target == b);
            format!("bars {} (unhit {}) held {held:?} wolfmsg {:?}", e.ninsatsu_left(), e.ninsatsu_unhit, w.resource::<crate::enemy::WolfMessage>().msg)
        };
        let line = format!("{s}/{an} | wolf {ps}/{pa} | {extra}");
        if line != last {
            eprintln!("MONK f{f} t{t:?} {line}");
            last = line;
        }
    }
}

/// TAE 117 SpawnFFX_ThrowDirection: ThrowDirectionSfxParam's material row, the event's index
/// as the column (Wolf's deathblow index 11: meat 220501, meat under armour 229501).
#[test]
pub(crate) fn throw_direction_effects_come_from_the_material_row() {
    let f = crate::ffx::FootSfx::load();
    assert_eq!(f.throw_id(109, 11), Some(220501));
    assert_eq!(f.throw_id(114, 11), Some(229501));
    assert_eq!(f.throw_id(132, 15), Some(255503));
    assert_eq!(f.throw_id(109, 12), None);
    assert_eq!(f.throw_id(-1, 11), None);
}

/// Per-boss behaviour trace for the clip comparison (docs/kb/clips.md all-bosses chapters):
/// `BOSS=c7100:71000000 SECS=120 cargo test zz_boss_moveset_trace -- --ignored --nocapture`.
/// Wolf walks in past 3.5 m, guards on each of the boss's hit windows (a deflect press 2 TAE
/// frames ahead) and slashes while it recovers; each boss state change prints with the time,
/// the distance and the perilous kind, then a summary (attacks per minute, perilous share).
#[test]
#[ignore]
pub(crate) fn zz_boss_moveset_trace() {
    let spec = std::env::var("BOSS").unwrap_or_else(|_| "c7100:71000000".into());
    let (chr, row) = spec.split_once(':').map(|(c, r)| (c.to_string(), r.parse::<i64>().ok())).unwrap_or((spec.clone(), None));
    let secs: u32 = std::env::var("SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    // CLOSE=<m>: how near Wolf walks in before he stops (the clips' no-damage player stays at ~1-2 m).
    let close: f32 = std::env::var("CLOSE").ok().and_then(|s| s.parse().ok()).unwrap_or(3.5);
    // GOD=1: Wolf's HP stays full (the pacing of a boss against a living player, as in the
    // no-damage clip; without it he dies to grabs and the boss idles over his body).
    let god = std::env::var("GOD").is_ok_and(|v| v == "1");
    // ATTACK_EVERY=<frames>: how often Wolf presses attack in range (default 30 = every 0.5 s;
    // the clips' player strikes less, so the boss's combos reach their ComboFinal perilous).
    let attack_every: u32 = std::env::var("ATTACK_EVERY").ok().and_then(|s| s.parse().ok()).unwrap_or(30);
    let mut app = app_cfg(&chr, row, Vec::new());
    for _ in 0..30 {
        app.update();
    }
    {
        let w = app.world_mut();
        let b = boss_in_play(w);
        w.get_mut::<Enemy>(b).unwrap().aggressive = true;
        if !w.contains_resource::<crate::camera::LockOn>() {
            w.insert_resource(crate::camera::LockOn::default());
        }
    }
    let (mut last, mut attacks, mut perilous, mut pressed_for) = (String::new(), 0u32, 0u32, String::new());
    let (mut last_anim, mut last_t) = (String::new(), 0.0f32);
    for i in 0..(secs * 60) {
        let (es, anim, t, dist, hit_in, kind, attack, wolf, to_boss) = {
            let world = app.world_mut();
            let b = boss_in_play(world);
            let (pp, wolf) = {
                let (tf, pa) = world.query_filtered::<(&Transform, &Actor), With<Player>>().single(world).unwrap();
                (tf.translation, format!("{} hp{:.0}", pa.state, pa.hp))
            };
            let goals = world.get_non_send_resource::<crate::enemy::Brains>().and_then(|br| br.map.get(&b)).map(|br| br.describe()).unwrap_or_default();
            let wolf = format!("{wolf} | {goals}");
            // The stick that walks Wolf at the boss (headless: no OrbitCamera, so the camera yaw is 0).
            let cam_yaw = world.query::<&crate::camera::OrbitCamera>().iter(world).next().map_or(0.0, |c| c.yaw);
            let to_boss = {
                let bp = world.get::<Transform>(b).unwrap().translation;
                let local = Quat::from_rotation_y(-cam_yaw) * (bp - pp).with_y(0.0).normalize_or_zero();
                Vec2::new(local.x, -local.z)
            };
            let bp = world.get::<Transform>(b).unwrap().translation;
            let ba = world.get::<Actor>(b).unwrap();
            let combat = world.resource::<Combat>();
            let d = combat.data_of(&ba);
            let hit_in = d.attack_windows(&ba.anim).iter().filter(|(_, atk, _)| atk.atk_phys > 0.0).map(|(e, _, _)| e.start - ba.t).filter(|x| *x > -0.001).reduce(f32::min);
            let kind = d.perilous_warning(&ba.anim, 0.0, f32::INFINITY).map(|_| crate::enemy::perilous_kind(d, &ba.anim));
            // A damaging bullet (Lady Butterfly's kunai 3024, the Owl's shuriken) is an attack too.
            let attack = d.attack_windows(&ba.anim).iter().any(|(_, atk, _)| atk.atk_phys > 0.0 || atk.throw_flag == 1)
                || d.anim(&ba.anim).is_some_and(|an| an.events.iter().filter(|e| e.kind == 2).filter_map(|e| e.arg_i64("BehaviorJudgeID")).any(|j| d.bullets.get(&j.to_string()).is_some_and(|b| b.attack.as_ref().is_some_and(|a| a.atk_phys > 0.0))));
            (ba.state.clone(), ba.anim.clone(), ba.t, bp.with_y(0.0).distance(pp.with_y(0.0)), hit_in, kind, attack, wolf, to_boss)
        };
        {
            let w = app.world_mut();
            let b = boss_in_play(w);
            if god {
                let mut pa = w.query_filtered::<&mut Actor, With<Player>>().single_mut(w).unwrap();
                pa.hp = pa.hp_max;
            }
            w.resource_mut::<crate::camera::LockOn>().target = Some(b);
            let mut pad = w.resource_mut::<PadInput>();
            pad.stick = if dist > close { to_boss } else { Vec2::ZERO };
            match hit_in {
                Some(dt) if dt <= 2.0 / TAE_FPS && kind.is_none() && !pressed_for.starts_with(&anim) => {
                    pad.press(Action::Guard);
                    pressed_for = format!("{anim}@{i}");
                }
                // The clips' player answers every perilous: Mikiri a thrust (Step as it lands), jump a
                // sweep, step back from a grab.
                Some(dt) if kind.is_some() && !pressed_for.starts_with(&anim) => match kind.unwrap() {
                    "thrust" if dt <= 3.0 / TAE_FPS => {
                        pad.stick = to_boss;
                        pad.press(Action::Step);
                        pressed_for = format!("{anim}@{i}");
                    }
                    "sweep" if dt <= 4.0 / TAE_FPS => {
                        pad.press(Action::Jump);
                        pressed_for = format!("{anim}@{i}");
                    }
                    "grab" if dt <= 8.0 / TAE_FPS => {
                        pad.stick = -to_boss;
                        pad.press(Action::Step);
                        pressed_for = format!("{anim}@{i}");
                    }
                    _ => {}
                },
                Some(_) => {}
                None => {
                    pressed_for.clear();
                    if dist < 3.0 && i % attack_every == 0 && !attack {
                        pad.press(Action::Attack);
                    }
                }
            }
        }
        app.update();
        // A restarted clip (ComboRepeat plays the same anim again) counts as a new attack.
        let restarted = anim == last_anim && t < last_t && attack;
        last_anim = anim.clone();
        last_t = t;
        if es != last || restarted {
            if attack {
                attacks += 1;
                perilous += kind.is_some() as u32;
            }
            eprintln!("BOSS {:7.2}s d{dist:5.1} {es} ({anim} t{t:.2}){}{} | wolf {wolf}", i as f32 / 60.0, if attack { " ATK" } else { "" }, kind.map(|k| format!(" PERILOUS {k}")).unwrap_or_default());
            last = es;
        }
    }
    let mins = secs as f32 / 60.0;
    eprintln!("BOSS SUMMARY {spec}: {attacks} attacks ({:.1}/min), {perilous} perilous ({:.0}%)", attacks as f32 / mins, 100.0 * perilous as f32 / attacks.max(1) as f32);
}
