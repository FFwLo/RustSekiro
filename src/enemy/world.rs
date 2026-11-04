//! The open world's map events: every base map's EMEVD (tools/world_scripts.py ->
//! extracted/enemies/world_scripts.json) run by the boss script's VM (script.rs, its world mode:
//! raw MSB positions, no boss frame), with the map's enemies streamed in around Wolf.
//!
//! On sekiro-rs's map started at an idol (sr_map::idol_start) with `SHINOBI_WORLD_ENEMIES=1`,
//! one script per play block. Every MSB enemy is on the map unless its events disable or unload
//! it (2004[5] / 2004[29]) or it died; those within SPAWN of Wolf are spawned at their MSB spot,
//! unaware on their patrol route with their Part.Enemy think row, and go again past DESPAWN.
//! The events' other requests (SpEffects, AI commands, forced anims, deaths) go to the spawned
//! ones through script.rs `apply`. A rest (FightReset) brings the living back: Restart events
//! (rest 1) start over, End ones (rest 2) end, the rest keep their state; flags stay.
//! The map objects' state (doors, gates, chests: 2005[x]), map parts (2012[1]) and collisions
//! (2011[1] / [4]) are kept on the Run (script.rs `ObjState`, Run::object / map_part_on / hit_on);
//! the instructions m11_01 uses that the VM does not run are listed in docs/kb/map-events.md.
//! gaps: no story flags (the world as at a new game); the objects' state is not drawn or
//! collided by the play world yet, Wolf has no ObjAct action to open a door with; SFX, warps,
//! cutscenes and bullets of these events are not run; the game streams by map block, not by
//! distance.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::script::{self, Act, Run, ScriptDef, Snap, PLAYER};
use super::{Enemy, Mode};
use crate::actor::Actor;
use crate::config::GameConfig;
use crate::data::Combat;
use crate::player::Player;

/// Spawned within this distance of Wolf (m).
const SPAWN: f32 = 60.0;
/// Gone again past this one.
const DESPAWN: f32 = 80.0;
/// The streaming check's interval (s): the exe's WorldBackRead promotes every 0.2 s
/// (constructor 0x14081d460, sekiro-rs docs/MAP-COMPLETENESS.md).
const STREAM_EVERY: f32 = 0.2;
/// Spawns per frame (a new kind loads its 5 MB data once: one kind per frame).
const SPAWNS_PER_FRAME: usize = 4;

/// A world enemy: its map script and MSB entity id.
#[derive(Component, Clone, Copy, Debug)]
pub struct WorldChar {
    pub map: usize,
    pub id: i64,
}

/// The character whose boss bars the HUD shows, set by its map's events (2003[11] Display Boss
/// Health Bar), as BossScript's `bar` is in a boss fight.
#[derive(Resource, Default)]
pub struct WorldBar(pub Option<Entity>);

#[derive(Resource, Default)]
pub struct WorldScripts {
    runs: Vec<WorldRun>,
    started: bool,
    /// BossScript::resets last seen (a rest).
    resets: u32,
    /// Time to the next streaming check.
    stream_in: f32,
    /// The map and entity id of the shown boss bar ([`WorldBar`]).
    bar: Option<(usize, i64)>,
}

struct WorldRun {
    run: Run,
    /// Disabled by its events (Change Character Enable State off).
    off: HashSet<i64>,
    /// Every enemy's MSB spot in the world.
    spots: Vec<(i64, Vec3)>,
}

pub fn active() -> bool {
    std::env::var("SHINOBI_WORLD_ENEMIES").is_ok_and(|v| v == "1") && (crate::encounter::hooks().open_world)() && (crate::encounter::hooks().idol_start)().is_some()
}

fn load(blocks: &[String]) -> Vec<WorldRun> {
    let path = crate::paths::extracted().join("enemies").join("world_scripts.json");
    let Some(mut all) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<HashMap<String, ScriptDef>>(&t).ok()) else {
        warn!("world scripts: no {} (tools/world_scripts.py)", path.display());
        return Vec::new();
    };
    let mut runs = Vec::new();
    for (i, map) in blocks.iter().enumerate() {
        let Some(def) = all.remove(map) else { continue };
        let unrun = def.events.iter().flat_map(|e| &e.ins).filter(|x| !script::handled(x.0, x.1)).count();
        info!("world scripts: {map} ({} enemies, {} events, {unrun} instructions not run: docs/kb/map-events.md)", def.chars.len(), def.events.len());
        let run = Run::new_world(Arc::new(def), 0x5EED_0000 ^ (i as u32 + 1).wrapping_mul(0x9E37_79B9));
        let spots = run.def.chars.keys().filter_map(|&id| Some((id, run.place_of(id)?.0))).collect();
        runs.push(WorldRun { run, off: HashSet::new(), spots });
    }
    runs
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_world(
    mut commands: Commands,
    time: Res<Time>,
    mut combat: ResMut<Combat>,
    mut lib: Option<ResMut<crate::anim::AnimLib>>,
    config: Res<GameConfig>,
    bs: Res<script::BossScript>,
    mut ws: ResMut<WorldScripts>,
    player: Single<(&Actor, &Transform, &Player), Without<Enemy>>,
    wolf_msg: Res<super::WolfMessage>,
    mut q: Query<(Entity, &mut Enemy, &mut Actor, &Transform, &WorldChar), Without<Player>>,
    mut brains: NonSendMut<super::Brains>,
    mut bar: ResMut<WorldBar>,
) {
    let _prof = crate::perf::span("run_world");
    if !ws.started {
        ws.started = true;
        if !active() {
            return;
        }
        let blocks = (crate::encounter::hooks().blocks)();
        ws.runs = load(&blocks);
        ws.resets = bs.resets;
    }
    if ws.runs.is_empty() {
        return;
    }
    // A rest: reset_fight despawned every enemy.
    if ws.resets != bs.resets {
        ws.resets = bs.resets;
        for wr in &mut ws.runs {
            let run = &mut wr.run;
            run.ents.clear();
            run.dead_ids.clear();
            run.prev_hp.clear();
            run.pending.clear();
            let def = run.def.clone();
            for (ev, st) in def.events.iter().zip(run.events.iter_mut()) {
                match ev.rest {
                    1 => *st = script::EvState::default(),
                    2 => st.done = true,
                    _ => {}
                }
            }
        }
        ws.bar = None;
        info!("world scripts: rest");
    }
    let dt = time.delta_secs();
    let (pa, ptf, wolf) = *player;
    let wolf_pos = ptf.translation;
    let mut wolf_sp: HashSet<i64> = wolf.timed.iter().map(|t| t.0).collect();
    if !pa.anim.is_empty() {
        wolf_sp.extend(combat.player.sp_effects_at(&pa.anim, pa.t).iter().map(|(id, _)| *id));
    }
    // Snapshots of the spawned ones, per map; the dead stay dead (no practice respawn).
    let prof_snap = crate::perf::span("world_snap");
    let mut snaps: Vec<HashMap<i64, Snap>> = ws.runs.iter().map(|_| HashMap::new()).collect();
    let mut gone = Vec::new();
    for (ent, mut e, a, tf, wc) in &mut q {
        let Some(wr) = ws.runs.get_mut(wc.map) else { continue };
        let run = &mut wr.run;
        if run.ents.get(&wc.id) != Some(&ent) {
            continue;
        }
        if tf.translation.distance(wolf_pos) > spawn_radius() + DESPAWN - SPAWN || run.gated(wc.id) {
            if trace_on() {
                eprintln!("world despawn {} {} at {} (wolf {wolf_pos})", run.def.map, wc.id, tf.translation);
            }
            gone.push((wc.map, wc.id, ent));
            continue;
        }
        let dead = e.is_dead() && a.hp <= 0.0 && e.ninsatsu_unhit == 0;
        if let Mode::Dead(t) = e.mode {
            if dead && t.is_finite() && t < 1.0 {
                e.mode = Mode::Dead(f32::INFINITY);
            }
        }
        if dead {
            run.dead_ids.insert(wc.id);
        }
        let d = combat.data_of(&a);
        let mut sp: HashSet<i64> = a.resident.iter().copied().collect();
        if !a.anim.is_empty() {
            sp.extend(d.sp_effects_at(&a.anim, a.t).iter().map(|(id, _)| *id));
        }
        sp.insert(e.ai_sp);
        let prev = run.prev_hp.insert(wc.id, a.hp).unwrap_or(a.hp);
        snaps[wc.map].insert(
            wc.id,
            Snap {
                pos: tf.translation,
                hp: a.hp,
                hp_max: a.hp_max,
                posture_ratio: if a.posture_max > 0.0 { (1.0 - a.posture / a.posture_max).max(0.0) } else { 1.0 },
                bars: e.ninsatsu_left(),
                bars_max: e.ninsatsu.1,
                dead,
                sp,
                msg: e.msg.into_iter().collect(),
                ai_state: match e.targeting.state {
                    s if s >= crate::stealth::BATTLE => 3,
                    s if s >= crate::stealth::CAUTION => 2,
                    _ => 0,
                },
                damaged: a.hp < prev,
                grounded: !a.airborne,
            },
        );
    }
    drop(prof_snap);
    for (map, id, ent) in gone {
        ws.runs[map].run.ents.remove(&id);
        commands.entity(ent).despawn();
        brains.map.remove(&ent);
    }
    // The events.
    let prof_ev = crate::perf::span("world_events");
    let mut acts_of: Vec<Vec<Act>> = Vec::new();
    for (wr, chars) in ws.runs.iter_mut().zip(snaps.iter_mut()) {
        let run = &mut wr.run;
        let prev = run.prev_hp.insert(PLAYER, pa.hp).unwrap_or(pa.hp);
        chars.insert(
            PLAYER,
            Snap { pos: wolf_pos, hp: pa.hp, hp_max: pa.hp_max, msg: wolf_msg.msg.into_iter().collect(), ai_state: 3, damaged: pa.hp < prev, grounded: !pa.airborne, sp: wolf_sp.clone(), ..Default::default() },
        );
        let w = script::World { chars: &*chars };
        let mut acts = std::mem::take(&mut run.pending);
        for ev in 0..run.events.len() {
            script::step(run, ev, &w, dt, &mut acts);
        }
        // (An executed ObjAct counts for the frame it happened in.)
        run.objact_used.clear();
        run.frame += 1;
        acts_of.push(acts);
    }
    drop(prof_ev);
    // Their requests.
    let ws_bar = ws.bar;
    let mut bars = Vec::new();
    for (map, acts) in acts_of.into_iter().enumerate() {
        let wr = &mut ws.runs[map];
        for act in acts {
            if script::trace() {
                eprintln!("world {}: {act:?}", wr.run.def.map);
            }
            let id = match &act {
                Act::Enable(id, on) => {
                    if *on {
                        wr.off.remove(id);
                    } else {
                        wr.off.insert(*id);
                    }
                    *id
                }
                Act::Unload(id, on) => {
                    if *on {
                        wr.run.unloaded.insert(*id);
                    } else {
                        wr.run.unloaded.remove(id);
                    }
                    *id
                }
                Act::BossBar(id, on) => {
                    if *on {
                        bars.push(Some((map, *id)));
                    } else if ws_bar == Some((map, *id)) {
                        bars.push(None);
                    }
                    continue;
                }
                // Killed while away: dead when it would come in.
                Act::Death(id, _) if !wr.run.ents.contains_key(id) => {
                    wr.run.dead_ids.insert(*id);
                    continue;
                }
                Act::SetSp(i, _) | Act::ClearSp(i, _) | Act::AiCommand(i, _, _) | Act::Replan(i) | Act::AiId(i, _) | Act::AiState(i, _) => *i,
                Act::ForceAnim(i, _, _) | Act::EzState(i, _) | Act::Immortal(i, _) | Act::Team(i, _) | Act::Death(i, _) => *i,
                _ => continue,
            };
            let Some(&ent) = wr.run.ents.get(&id) else { continue };
            if matches!(act, Act::Enable(..) | Act::Unload(..)) {
                if wr.off.contains(&id) || wr.run.unloaded.contains(&id) {
                    wr.run.ents.remove(&id);
                    commands.entity(ent).despawn();
                    brains.map.remove(&ent);
                }
                continue;
            }
            let Ok((_, mut e, mut a, ..)) = q.get_mut(ent) else {
                // Spawned this frame.
                wr.run.pending.push(act);
                continue;
            };
            script::apply(&combat, &mut brains, ent, &mut e, &mut a, None, act);
        }
    }
    if let Some(&b) = bars.last() {
        ws.bar = b;
    }
    let shown = ws.bar.and_then(|(m, id)| ws.runs.get(m)?.run.ents.get(&id).copied());
    if bar.0 != shown {
        bar.0 = shown;
    }
    // Streaming in, the nearest first.
    ws.stream_in -= dt;
    if ws.stream_in > 0.0 {
        return;
    }
    let _prof_st = crate::perf::span("world_stream");
    let r2 = spawn_radius() * spawn_radius();
    let mut want: Vec<(f32, usize, i64)> = Vec::new();
    for (m, wr) in ws.runs.iter().enumerate() {
        let run = &wr.run;
        for &(id, pos) in &wr.spots {
            let d2 = pos.distance_squared(wolf_pos);
            if d2 > r2 || run.ents.contains_key(&id) || wr.off.contains(&id) || run.unloaded.contains(&id) || run.dead_ids.contains(&id) || run.gated(id) {
                continue;
            }
            want.push((d2, m, id));
        }
    }
    want.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Again at once while some wait for their turn.
    ws.stream_in = if want.len() > SPAWNS_PER_FRAME { 0.0 } else { STREAM_EVERY };
    let mut loaded_kind = false;
    for (_, m, id) in want.into_iter().take(SPAWNS_PER_FRAME) {
        let wr = &mut ws.runs[m];
        let c = wr.run.def.chars[&id].clone();
        let kind = match combat.kind_index(&c.chr, Some(c.npc)) {
            Some(k) => k,
            None if loaded_kind => continue,
            None => {
                loaded_kind = true;
                let Some(k) = combat.ensure_kind(&c.chr, c.npc) else {
                    warn!("world scripts: {} {} has no data", c.chr, c.npc);
                    wr.off.insert(id);
                    continue;
                };
                info!("world scripts: kind {} {} loaded ({} kinds)", c.chr, c.npc, combat.kinds.len());
                if let Some(lib) = lib.as_mut() {
                    lib.sync_kinds(&combat);
                }
                k
            }
        };
        let Some((pos, yaw)) = wr.run.place_of(id) else { continue };
        // On the floor under its MSB height (a step up allowed).
        let floor = crate::terrain::terrain().and_then(|t| t.enemy_floor(pos.x, pos.z, pos.y + crate::terrain::STEP_HEIGHT)).unwrap_or(pos.y);
        let at = pos.with_y(floor + crate::player::CAPSULE_HALF_HEIGHT + 0.05);
        let rng = wr.run.rand();
        let ent = super::spawn_one(&mut commands, &combat, &config, kind, at, yaw, true, rng);
        commands.entity(ent).insert((WorldChar { map: m, id }, super::StartUnaware));
        if let Some(spot) = super::msb_spot_in(&wr.run.def.map, Vec3::ZERO, 0.0, id) {
            commands.entity(ent).insert(spot.route);
        }
        let think = (c.think > 0 && !combat.param("NpcThinkParam", c.think).is_null()).then_some(c.think);
        if think.is_some() {
            commands.entity(ent).queue(move |mut w: EntityWorldMut| {
                if let Some(mut en) = w.get_mut::<Enemy>() {
                    en.think_override = think;
                }
            });
        }
        if trace_on() {
            eprintln!("world spawn {} {id} {} at {at} (wolf {wolf_pos})", wr.run.def.map, c.chr);
        }
        wr.run.ents.insert(id, ent);
    }
}

/// SHINOBI_WORLD_RADIUS=<m>: SPAWN for a test.
fn spawn_radius() -> f32 {
    std::env::var("SHINOBI_WORLD_RADIUS").ok().and_then(|v| v.parse().ok()).unwrap_or(SPAWN)
}

/// SHINOBI_WORLD_TRACE=1: spawns and despawns (SHINOBI_SYS_PROF=1 for the cost).
fn trace_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SHINOBI_WORLD_TRACE").is_some())
}

/// The exe's character update levels (WorldChrMan "omission update", FUN_140a4e8e0;
/// tunables WorldChrManDbgImp FUN_140a51800 and the exe's config defaults): within 10 m the
/// nearest LV1_COUNT update every tick, the next LV2_COUNT every 2nd, the rest every 5th;
/// farther on screen within ON_SCREEN_FAR every 5th, else every 30th. A character is due when
/// its index and the tick agree modulo its level (FUN_140a4db20), and then runs
/// that many ticks' time (actor.rs `advance`, enemy.rs `think`, map.rs `bodies_on_terrain`,
/// anim.rs `pose_skeletons`). Only the open world's enemies. gaps: the not-drawn-last-frame and
/// draw-group raises, the undecoded second budget flag (level 2), the budget's sort order
/// (nearest first assumed).
pub(super) fn update_lod(
    mut tick: Local<u64>,
    wolf: Single<&Transform, (With<Player>, Without<Enemy>)>,
    cams: Query<(&GlobalTransform, &Projection), With<Camera3d>>,
    mut q: Query<(&Transform, &mut Actor, &WorldChar), Without<Player>>,
) {
    const NEAR: f32 = 10.0;
    const LV1_COUNT: usize = 10;
    const LV2_COUNT: usize = 15;
    const ON_SCREEN_FAR: f32 = 50.0;
    *tick += 1;
    let wolf = wolf.translation;
    // On screen: inside the camera's view cone, a body's width of slack.
    let cam = cams.iter().next().map(|(g, p)| {
        let half = match p {
            Projection::Perspective(pp) => (pp.fov * 0.5).tan() * pp.aspect_ratio.max(1.0),
            _ => 1.0,
        };
        (g.translation(), g.forward().as_vec3(), half.atan() + 0.1)
    });
    let mut list: Vec<(f32, i64, Vec3)> = q.iter().map(|(tf, _, wc)| (tf.translation.distance_squared(wolf), wc.id, tf.translation)).collect();
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut levels: HashMap<i64, u32> = HashMap::new();
    let (mut lv1, mut lv2) = (0, 0);
    for (d2, id, pos) in list {
        let level = if d2 < NEAR * NEAR {
            if lv1 < LV1_COUNT {
                lv1 += 1;
                1
            } else if lv2 < LV2_COUNT {
                lv2 += 1;
                2
            } else {
                5
            }
        } else {
            let seen = cam.is_some_and(|(c, f, half)| f.angle_between(pos + Vec3::Y - c) <= half);
            if seen && d2 < ON_SCREEN_FAR * ON_SCREEN_FAR { 5 } else { crate::actor::ASLEEP }
        };
        levels.insert(id, level);
    }
    for (_, mut a, wc) in &mut q {
        let level = levels.get(&wc.id).copied().unwrap_or(1);
        a.lod = level;
        let level = level as u64;
        a.lod_phase = ((*tick % level + level - wc.id as u64 % level) % level) as u32;
        a.lod_due = a.lod_phase == 0;
    }
}

#[cfg(test)]
mod tests {
    //! Ashina Castle's own events (m11_01_00_00, board row P10a) through the VM, on the exported
    //! world script (extracted/enemies/world_scripts.json; skipped without it).
    use super::script::{self, Act, Run, ScriptDef, Snap, PLAYER};
    use bevy::prelude::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn castle() -> Option<Run> {
        let path = crate::paths::extracted().join("enemies").join("world_scripts.json");
        let text = std::fs::read_to_string(&path).ok()?;
        let mut all: HashMap<String, ScriptDef> = serde_json::from_str(&text).unwrap();
        Some(Run::new_world(Arc::new(all.remove("m11_01_00_00")?), 1))
    }

    /// The event instance `id` that holds the instruction `bank[iid]` with this first argument.
    fn instance(run: &Run, id: i64, (bank, iid, arg0): (i32, i32, f64)) -> usize {
        run.def.events.iter().position(|e| e.id == id && e.ins.iter().any(|x| x.0 == bank && x.1 == iid && x.2.first() == Some(&arg0))).unwrap()
    }

    fn wolf_at(pos: Vec3, sp: &[i64]) -> HashMap<i64, Snap> {
        HashMap::from([(PLAYER, Snap { pos, hp: 100.0, hp_max: 100.0, grounded: true, sp: sp.iter().copied().collect(), ..Default::default() })])
    }

    /// Runs one event for `frames` 60 Hz frames, the ObjAct triggers cleared each frame as
    /// run_world does.
    fn run_for(run: &mut Run, ev: usize, chars: &HashMap<i64, Snap>, frames: u32) -> Vec<Act> {
        let mut acts = Vec::new();
        for _ in 0..frames {
            script::step(run, ev, &script::World { chars }, 1.0 / 60.0, &mut acts);
            run.objact_used.clear();
            run.frame += 1;
        }
        acts
    }

    /// 11115613: with the castle invasion's 8301 on when the map loads, the gate 1111603 plays
    /// its animation 1, its ObjAct 999900 goes off and interaction 1 off / 10 on, and its flags
    /// 11110603 / 61110603 are set; without 8301 the event ends and nothing moves.
    #[test]
    fn the_castle_gate_opens_when_the_invasion_flag_is_set() {
        let Some(mut run) = castle() else { return };
        let ev = instance(&run, 11115613, (2005, 7, 1111603.0));
        let far = wolf_at(Vec3::splat(10_000.0), &[]);
        run_for(&mut run, ev, &far, 10);
        assert!(run.events[ev].done && run.object(1111603).is_none() && !run.flag(11110603));
        let Some(mut run) = castle() else { return };
        run.set_flag(8301, true);
        run_for(&mut run, ev, &far, 2);
        let o = run.object(1111603).expect("the gate's state");
        assert_eq!(o.anim, Some(1));
        assert_eq!(o.objact.get(&999900), Some(&false));
        assert_eq!((o.interaction.get(&1), o.interaction.get(&10)), (Some(&false), Some(&true)));
        assert!(run.flag(11110603) && run.flag(61110603));
    }

    /// Common event 20005615 for the door 1111700 (closed, flag 11110550 off): its open action
    /// is offered (interaction 1 on) until an ObjAct with flag 1113709 or 1113710 is executed,
    /// then 11110550 is set and 2 s later the ObjActs 999940 / 999941 go off. Its last wait,
    /// 13[01] Player Is Looking At Entity, is a gap: the event stays there.
    #[test]
    fn a_castle_door_opens_on_its_objact_and_keeps_its_flag() {
        let Some(mut run) = castle() else { return };
        let ev = instance(&run, 20005615, (2005, 19, 1111700.0));
        let far = wolf_at(Vec3::splat(10_000.0), &[]);
        run_for(&mut run, ev, &far, 30);
        assert_eq!(run.object(1111700).and_then(|o| o.interaction.get(&1).copied()), Some(true));
        assert!(!run.flag(11110550));
        // Another ObjAct's flag does not open it.
        run.use_objact(1113999);
        run_for(&mut run, ev, &far, 1);
        assert!(!run.flag(11110550));
        run.use_objact(1113710);
        run_for(&mut run, ev, &far, 1);
        assert!(run.flag(11110550));
        run_for(&mut run, ev, &far, 2 * 60 + 5);
        let o = run.object(1111700).unwrap();
        assert_eq!((o.interaction.get(&1), o.objact.get(&999940), o.objact.get(&999941)), (Some(&false), Some(&false), Some(&false)));
        assert!(!run.events[ev].done, "13[01] is a gap: it waits there");
    }

    /// The same door already open (11110550 on, a load after it was opened): it skips the
    /// prompt, plays its open animation 1 and turns its two collisions off and on again (the
    /// navmesh rebuilt), then ends.
    #[test]
    fn an_opened_castle_door_comes_back_open_with_its_collision() {
        let Some(mut run) = castle() else { return };
        let ev = instance(&run, 20005615, (2005, 19, 1111700.0));
        run.set_flag(11110550, true);
        let far = wolf_at(Vec3::splat(10_000.0), &[]);
        run_for(&mut run, ev, &far, 1);
        assert_eq!(run.object(1111700).and_then(|o| o.anim), Some(1));
        run_for(&mut run, ev, &far, 1);
        assert!(!run.hit_on(1114381) && !run.hit_on(1114260), "off for a frame");
        run_for(&mut run, ev, &far, 2);
        assert!(run.hit_on(1114381) && run.hit_on(1114260));
        assert!(run.events[ev].done);
    }

    /// Common event 20005546 for the breakable 1111100: Wolf with SpEffect 100317 within 1 m of
    /// it destroys it (2005[1]); not without the SpEffect.
    #[test]
    fn a_breakable_breaks_for_wolf_with_its_speffect_in_reach() {
        let Some(mut run) = castle() else { return };
        let ev = (0..run.def.events.len())
            .find(|&k| run.def.events[k].id == 20005546 && run.def.events[k].ins.get(1).is_some_and(|x| x.2.get(2) == Some(&1111100.0)))
            .unwrap();
        let at = run.place_of(1111100).expect("the object's MSB spot").0;
        run_for(&mut run, ev, &wolf_at(at, &[]), 10);
        assert!(run.object(1111100).is_none_or(|o| !o.destroyed));
        run_for(&mut run, ev, &wolf_at(at, &[100317]), 2);
        assert!(run.object(1111100).is_some_and(|o| o.destroyed));
    }

    /// A hand-made event: IF Parameter Comparison, IF Character Type (Wolf alone is type 0), END
    /// IF Player Is Not In Own World (never with "is not in" on, alone), GOTO IF Comparison,
    /// the network flags and Character Warp Request.
    #[test]
    fn parameter_comparison_character_type_and_own_world_run_as_offline() {
        let ins = serde_json::json!([
            [1003, 13, [0, 1]],
            [0, 1, [1, 0, 3, 3]],
            [4, 3, [1, 10000, 0, 0, 1.0]],
            [4, 3, [-2, 10000, 2, 0, 1.0]],
            [0, 0, [0, 1, 1]],
            [1000, 105, [5, 2, 4, 1]],
            [2003, 62, [500, 1]],
            [1014, 5, []],
            [2003, 63, [600, 602, 1]],
            [2004, 3, [1110701, 1, 1112726, -1]],
            [1003, 13, [0, 0]],
            [2003, 2, [700, 1]],
        ]);
        let def: ScriptDef = serde_json::from_value(serde_json::json!({
            "map": "test", "boss": 0, "world": true, "chars": {}, "events": [{"id": 1, "slot": 0, "ins": ins}],
        }))
        .unwrap();
        let mut run = Run::new_world(Arc::new(def), 1);
        let acts = run_for(&mut run, 0, &wolf_at(Vec3::ZERO, &[]), 1);
        assert!(!run.flag(500), "GOTO IF Comparison 4 > 1 jumped over it");
        assert!(run.flag(600) && run.flag(601) && run.flag(602));
        assert!(acts.iter().any(|a| matches!(a, Act::Warp(1110701, 1, 1112726, false, -1))));
        assert!(!run.flag(700) && run.events[0].done, "1003[13] with 'is not in' off ends it");
    }

    /// Every instruction m11_01's events use that the VM does not run is a listed gap
    /// (docs/kb/map-events.md), with its count.
    #[test]
    fn the_castles_unrun_instructions_are_the_listed_gaps() {
        let Some(run) = castle() else { return };
        let mut gaps: std::collections::BTreeMap<(i32, i32), usize> = Default::default();
        for e in &run.def.events {
            for x in e.ins.iter().filter(|x| !script::handled(x.0, x.1)) {
                *gaps.entry((x.0, x.1)).or_default() += 1;
            }
        }
        let listed: std::collections::BTreeMap<(i32, i32), usize> = [
            ((3, 24), 9), ((4, 6), 12), ((12, 2), 16), ((12, 5), 1), ((13, 1), 18), ((2000, 2), 12), ((2000, 5), 9), ((2002, 1), 2),
            ((2003, 4), 1), ((2003, 23), 1), ((2003, 28), 4), ((2003, 36), 69), ((2003, 76), 1), ((2003, 79), 6), ((2003, 80), 6),
            ((2003, 81), 7), ((2003, 82), 22), ((2003, 87), 16), ((2003, 88), 10), ((2004, 7), 1), ((2004, 10), 140), ((2004, 13), 5),
            ((2004, 15), 37), ((2004, 16), 1), ((2004, 22), 12), ((2004, 26), 40), ((2004, 30), 5), ((2004, 31), 129), ((2004, 34), 39),
            ((2004, 35), 2), ((2004, 37), 5), ((2004, 39), 77), ((2004, 49), 27), ((2004, 50), 6), ((2004, 58), 11), ((2004, 62), 6),
            ((2004, 64), 3), ((2004, 65), 133), ((2006, 1), 74), ((2006, 2), 60), ((2006, 3), 1), ((2006, 4), 27), ((2006, 5), 43),
            ((2007, 1), 6), ((2007, 13), 1), ((2007, 14), 1), ((2007, 17), 2), ((2007, 18), 3), ((2008, 4), 8), ((2009, 3), 11),
            ((2010, 3), 12), ((2010, 6), 1), ((2010, 8), 16),
        ]
        .into_iter()
        .collect();
        assert_eq!(gaps, listed);
    }
}
