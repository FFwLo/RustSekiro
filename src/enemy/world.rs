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
//! gaps: no story flags (the world as at a new game), objects, SFX, warps, cutscenes and bullets
//! of these events are not run; the game streams by map block, not by distance.

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
        info!("world scripts: {map} ({} enemies, {} events)", def.chars.len(), def.events.len());
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
        if tf.translation.distance(wolf_pos) > spawn_radius() + DESPAWN - SPAWN {
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
            if d2 > r2 || run.ents.contains_key(&id) || wr.off.contains(&id) || run.unloaded.contains(&id) || run.dead_ids.contains(&id) {
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

/// The exe's character update levels (WorldChrMan "omission update", FUN_140a4e8e0, 140a4.c:9828;
/// tunables WorldChrManDbgImp FUN_140a51800 and the exe's config defaults): within 10 m the
/// nearest LV1_COUNT update every tick, the next LV2_COUNT every 2nd, the rest every 5th;
/// farther on screen within ON_SCREEN_FAR every 5th, else every 30th. A character is due when
/// its index and the tick agree modulo its level (FUN_140a4db20, 140a4.c:9661), and then runs
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
