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
//! (2011[1] / [4]) are kept on the Run (script.rs `ObjState`) and reach the play world
//! (`apply_state`): sr_map's draw hides switched-off map parts and destroyed, deactivated or
//! opened objects (sekiro-rs EventVisibility), its collision world switches those objects' and
//! collision parts' event parts. Wolf works the MSB's ObjActs with the action button
//! (`objact_interact`), and characters' warp requests whose target is in the play world are
//! carried out. All six play blocks' events run from the start; the instructions they use that
//! the VM does not run are listed in docs/kb/map-events.md.
//! gaps: no story flags (the world as at a new game); object animations are not played (an
//! opened door is hidden); Wolf's own warps, SFX, cutscenes and bullets of these events are not
//! run; the game streams by map block, not by distance.

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
    /// The frame's warp requests (map, request), carried out by `apply_state`.
    warps: Vec<(usize, Act)>,
    /// The events' state last handed to the play world (`apply_state`).
    shown: WorldView,
}

struct WorldRun {
    run: Run,
    /// Disabled by its events (Change Character Enable State off).
    off: HashSet<i64>,
    /// Every enemy's MSB spot in the world (a warped one's warp target).
    spots: Vec<(i64, Vec3)>,
    /// Characters warped while not on the field: where they come in.
    moved: HashMap<i64, (Vec3, f32)>,
}

pub fn active() -> bool {
    std::env::var("SHINOBI_WORLD_ENEMIES").is_ok_and(|v| v == "1") && (crate::encounter::hooks().open_world)() && (crate::encounter::hooks().idol_start)().is_some()
}

/// Every base map's world script (extracted/enemies/world_scripts.json), read once.
fn defs() -> &'static HashMap<String, Arc<ScriptDef>> {
    static DEFS: std::sync::OnceLock<HashMap<String, Arc<ScriptDef>>> = std::sync::OnceLock::new();
    DEFS.get_or_init(|| {
        let path = crate::paths::extracted().join("enemies").join("world_scripts.json");
        match std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<HashMap<String, ScriptDef>>(&t).ok()) {
            Some(all) => all.into_iter().map(|(k, v)| (k, Arc::new(v))).collect(),
            None => {
                warn!("world scripts: no {} (tools/world_scripts.py)", path.display());
                HashMap::new()
            }
        }
    })
}

fn load(blocks: &[String]) -> Vec<WorldRun> {
    let all = defs();
    let mut runs = Vec::new();
    for (i, map) in blocks.iter().enumerate() {
        let Some(def) = all.get(map) else { continue };
        let unrun = def.events.iter().flat_map(|e| &e.ins).filter(|x| !script::handled(x.0, x.1)).count();
        info!("world scripts: {map} ({} enemies, {} events, {} ObjActs, {unrun} instructions not run: docs/kb/map-events.md)", def.chars.len(), def.events.len(), def.objacts.len());
        let run = Run::new_world(def.clone(), 0x5EED_0000 ^ (i as u32 + 1).wrapping_mul(0x9E37_79B9));
        let spots = run.def.chars.keys().filter_map(|&id| Some((id, run.place_of(id)?.0))).collect();
        runs.push(WorldRun { run, off: HashSet::new(), spots, moved: HashMap::new() });
    }
    runs
}

/// The collision the play world's events can switch (sr_map's collision world makes them event
/// parts): the collisions 2011[1] / [4] name and the objects 2005[1] / [2] / [3] / [7] name or
/// an ObjAct works, over the blocks' scripts. Empty unless `SHINOBI_WORLD_ENEMIES=1`.
pub(crate) fn switchable(blocks: &[String]) -> sekiro_rs::map::world::Switchable {
    if !std::env::var("SHINOBI_WORLD_ENEMIES").is_ok_and(|v| v == "1") {
        return Default::default();
    }
    switchable_in(blocks.iter().filter_map(|b| defs().get(b)).map(|d| &**d))
}

/// [`switchable`] over these scripts.
pub(crate) fn switchable_in<'a>(scripts: impl Iterator<Item = &'a ScriptDef>) -> sekiro_rs::map::world::Switchable {
    let mut out = sekiro_rs::map::world::Switchable::default();
    for def in scripts {
        for x in def.events.iter().flat_map(|e| &e.ins) {
            let Some(&id) = x.2.first() else { continue };
            let id = id as i32;
            match (x.0, x.1) {
                (2011, 1) | (2011, 4) if id > 0 => {
                    out.collisions.insert(id);
                }
                (2005, 1) | (2005, 2) | (2005, 3) | (2005, 7) if id > 0 => {
                    out.objects.insert(id);
                }
                _ => {}
            }
        }
        out.objects.extend(def.objacts.iter().map(|a| a.object as i32));
    }
    info!("world scripts: {} collision parts and {} objects switched by the events", out.collisions.len(), out.objects.len());
    out
}

/// What the events' object state means for the play world, over the runs (a later block's
/// opinion of the same entity wins): objects shown and collided with (false: destroyed,
/// deactivated or open), map parts on (2012[1]), collisions on (2011[1] / [4]). Only the
/// entities the events named.
///
/// An object is open when its last 2005[7] Reproduce Object Animation is one of its ObjActs'
/// objAnimId (or an ObjAct of it was worked): the object animation is not played (the cache's
/// objects carry no animation), so an open door is hidden with its collision off.
#[derive(Default, Debug, PartialEq, Clone)]
pub(crate) struct WorldView {
    pub objects: std::collections::BTreeMap<i64, bool>,
    pub parts: std::collections::BTreeMap<i64, bool>,
    pub hits: std::collections::BTreeMap<i64, bool>,
}

pub(crate) fn view<'a>(runs: impl IntoIterator<Item = &'a Run> + Clone) -> WorldView {
    let open_anims: HashMap<i64, Vec<i64>> = runs.clone().into_iter().flat_map(|r| r.def.objacts.iter()).fold(HashMap::new(), |mut m, a| {
        m.entry(a.object).or_default().push(a.anim);
        m
    });
    let mut out = WorldView::default();
    for run in runs {
        for (&id, o) in &run.objects {
            let open = o.anim.is_some_and(|a| open_anims.get(&id).is_some_and(|v| v.contains(&a)));
            out.objects.insert(id, o.active && !o.destroyed && !open);
        }
        out.parts.extend(run.map_parts.iter().map(|(&k, &v)| (k, v)));
        out.hits.extend(run.hits.iter().map(|(&k, &v)| (k, v)));
    }
    out
}

/// An ObjAct Wolf may work now, in the play world's space.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Offer {
    pub map: usize,
    pub id: i64,
    pub at: Vec3,
    pub forward: Vec3,
    pub dist: f32,
    pub player_angle: f32,
    pub obj_angle: f32,
}

/// The ObjActs on offer this frame (published by `apply_state`), and the ones Wolf worked
/// since (taken by run_world).
static OFFERS: std::sync::RwLock<Vec<Offer>> = std::sync::RwLock::new(Vec::new());
static WORKED: std::sync::Mutex<Vec<(usize, i64)>> = std::sync::Mutex::new(Vec::new());

/// The ObjActs of `run` (map index `map`) that are on offer: its object neither destroyed nor
/// deactivated, its ObjActParam row not switched off for the object by 2005[6], a reach above 0
/// (gap: the reach-0 rows, the revolving walls 90071-90076, go through the exe's
/// spQualified path, not traced).
pub(crate) fn offers(run: &Run, map: usize) -> Vec<Offer> {
    run.def
        .objacts
        .iter()
        .filter(|a| a.dist > 0.0)
        .filter(|a| run.object(a.object).is_none_or(|o| o.active && !o.destroyed && o.objact.get(&a.param) != Some(&false)))
        .map(|a| Offer {
            map,
            id: a.id,
            // MSB space, X mirrored (Run::place_of).
            at: Vec3::new(-a.at[0], a.at[1], a.at[2]),
            forward: Vec3::new(-a.forward[0], a.forward[1], a.forward[2]).with_y(0.0).normalize_or_zero(),
            dist: a.dist,
            player_angle: a.player_angle,
            obj_angle: a.obj_angle,
        })
        .collect()
}

/// The ObjAct Wolf at `feet` facing `yaw` works with the action button: the nearest within its
/// reach of its dummy poly (horizontally; 1.5 m up or down, gap: not traced), Wolf facing along
/// the dummy's forward within validPlayerAngle and standing behind it (on the side its forward
/// leaves) within validObjAngle: the exe's offer test FUN_1406b0c10 (distance and a facing
/// angle; which angle is which is read from the data: the HUB door's two dummies face into
/// the door from either side).
pub(crate) fn pick_objact(offers: &[Offer], feet: Vec3, yaw: f32) -> Option<Offer> {
    let facing = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let within = |a: Vec3, b: Vec3, deg: f32| a.angle_between(b).to_degrees() <= deg + 1e-3;
    offers
        .iter()
        .filter_map(|o| {
            let d = feet - o.at;
            let flat = d.with_y(0.0);
            if flat.length() > o.dist || d.y.abs() > 1.5 || o.forward == Vec3::ZERO {
                return None;
            }
            if !within(facing, o.forward, o.player_angle) {
                return None;
            }
            if flat.length() > 0.05 && !within(flat, -o.forward, o.obj_angle) {
                return None;
            }
            Some((flat.length(), *o))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

/// Wolf's ObjAct action (idol.rs world_hooks, after the idols): the action button (interact)
/// pressed on the ground and free near an ObjAct on offer works it; run_world fires it next
/// frame (Run::use_objact, 5[02]) and the object takes the ObjAct's objAnimId (open). Holds
/// Wolf for that step. gap: Wolf's own ObjActParam playerAnimId (the door push 700200) is not
/// played; the exe's execute request (FUN_1406ba910) and its eventKickTiming wait are not
/// modelled: the event hears of it at once.
pub(crate) fn objact_interact(_p: &mut Player, a: &mut Actor, _d: &crate::data::CharData, tf: &mut Transform, interact: bool, _ended: bool, _config: &GameConfig) -> bool {
    if !interact || a.airborne || a.hp <= 0.0 || !crate::player::is_free(&a.state) {
        return false;
    }
    let feet = tf.translation - Vec3::Y * crate::player::CAPSULE_HALF_HEIGHT;
    let Some(o) = OFFERS.read().ok().and_then(|v| pick_objact(&v, feet, a.yaw)) else { return false };
    if let Ok(mut w) = WORKED.lock() {
        w.push((o.map, o.id));
    }
    info!("world scripts: Wolf works ObjAct {}", o.id);
    true
}

/// Wolf worked ObjAct `id` of `run`: 5[02] holds for the frame (Run::use_objact) and its object
/// takes the ObjAct's objAnimId (the ObjAct plays it on the object).
pub(crate) fn work_objact(run: &mut Run, id: i64) {
    let Some(a) = run.def.objacts.iter().find(|a| a.id == id).cloned() else { return };
    run.use_objact(id);
    run.objects.entry(a.object).or_default().anim = Some(a.anim);
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
    // The ObjActs Wolf worked (objact_interact).
    for (map, id) in WORKED.lock().map(|mut w| std::mem::take(&mut *w)).unwrap_or_default() {
        if let Some(wr) = ws.runs.get_mut(map) {
            work_objact(&mut wr.run, id);
        }
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
    let mut warps = Vec::new();
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
                Act::Warp(..) => {
                    warps.push((map, act));
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
    ws.warps.extend(warps);
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
        let Some((pos, yaw)) = wr.moved.get(&id).copied().or_else(|| wr.run.place_of(id)) else { continue };
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

/// Where a warp request's target is in the play world: a character (target type 2) where it
/// stands (`on_field`), else the place (type 0 object, 1 area: an MSB point, region or object
/// part) or character in any block's script, the requesting block's first. None when no
/// play-world block has it.
pub(crate) fn warp_target<'a>(runs: &[&'a Run], from: usize, ty: i64, target: i64, on_field: impl Fn(i64) -> Option<(Vec3, f32)>) -> Option<(Vec3, f32)> {
    if ty == 2 {
        if let Some(at) = on_field(target) {
            return Some(at);
        }
    }
    let first = runs.get(from).into_iter();
    first.chain(runs.iter()).find_map(|r| r.place_of(target))
}

/// The events' state in the play world, after run_world: the warp requests (2004[3] Character
/// Warp Request, and 2004[40-42] / [61], all Act::Warp) of characters whose target is in the
/// play world; the objects, map parts and collisions the events switched (sr_map's draw
/// through sekiro-rs's EventVisibility, its collision world's event parts); the ObjActs on
/// offer for [`objact_interact`]. gap: Wolf's own warps are not carried out (they go with
/// cutscenes, idol travel and fall returns, whose conditions the VM only approximates); a warp
/// onto a character's dummy poly lands on its root.
pub(super) fn apply_state(
    mut ws: ResMut<WorldScripts>,
    vis: Option<ResMut<sekiro_rs::map::visibility::EventVisibility>>,
    player: Single<(&Transform, &Actor), (With<Player>, Without<Enemy>)>,
    mut q: Query<(&mut Transform, &mut Actor), (With<WorldChar>, Without<Player>)>,
) {
    if ws.runs.is_empty() {
        return;
    }
    let (ptf, pa) = *player;
    let wolf = (ptf.translation, pa.yaw);
    // Warps.
    for (map, act) in std::mem::take(&mut ws.warps) {
        let Act::Warp(who, ty, target, _, _) = act else { continue };
        if who == PLAYER {
            if script::trace() {
                eprintln!("world: Wolf's warp to {target} (type {ty}) not carried out");
            }
            continue;
        }
        let ent_of = |runs: &[WorldRun], id: i64| runs.iter().find_map(|w| w.run.ents.get(&id).copied());
        let on_field = |id: i64| {
            if id == PLAYER {
                return Some(wolf);
            }
            let (tf, a) = q.get(ent_of(&ws.runs, id)?).ok()?;
            Some((tf.translation, a.yaw))
        };
        let runs: Vec<&Run> = ws.runs.iter().map(|w| &w.run).collect();
        let Some((pos, yaw)) = warp_target(&runs, map, ty, target, on_field) else {
            if script::trace() {
                eprintln!("world: {who}'s warp to {target} (type {ty}): not in the play world");
            }
            continue;
        };
        // A place is the floor point; a character's root is where it stands.
        let pos = if ty == 2 { pos } else { pos + Vec3::Y * (crate::player::CAPSULE_HALF_HEIGHT + 0.05) };
        if let Some(ent) = ent_of(&ws.runs, who) {
            if let Ok((mut tf, mut a)) = q.get_mut(ent) {
                tf.translation = pos;
                a.yaw = yaw;
            }
            continue;
        }
        // Not on the field: it comes in there.
        let owner = ws.runs.iter().position(|w| w.run.def.chars.contains_key(&who)).unwrap_or(map);
        let wr = &mut ws.runs[owner];
        wr.moved.insert(who, (pos, yaw));
        if let Some(s) = wr.spots.iter_mut().find(|s| s.0 == who) {
            s.1 = pos;
        }
    }
    // The ObjActs on offer.
    let offered: Vec<Offer> = ws.runs.iter().enumerate().flat_map(|(m, w)| offers(&w.run, m)).collect();
    if let Ok(mut o) = OFFERS.write() {
        *o = offered;
    }
    // Objects, map parts and collisions.
    let now = view(ws.runs.iter().map(|w| &w.run));
    if now == ws.shown {
        return;
    }
    if let Some(world) = crate::sr_map::collision() {
        for (id, &on) in now.objects.iter().chain(&now.hits) {
            let was = ws.shown.objects.get(id).or(ws.shown.hits.get(id));
            if was != Some(&on) {
                world.set_event_part(*id as i32, on);
            }
        }
    }
    if let Some(mut vis) = vis {
        let mut maps: std::collections::BTreeMap<String, sekiro_rs::map::visibility::EventState> = Default::default();
        for (&id, &on) in now.objects.iter().chain(&now.parts) {
            maps.entry(crate::sr_map::map_of(id)).or_default().by_entity.insert(id as i32, on);
        }
        vis.maps = maps;
    }
    if script::trace() {
        eprintln!("world: {} objects, {} map parts, {} collisions switched by the events", now.objects.len(), now.parts.len(), now.hits.len());
    }
    ws.shown = now;
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
    //! Ashina Castle's own events (m11_01_00_00, board row P10a) and the six play blocks' with
    //! their play-world state (P10b) through the VM, on the exported
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

    fn block(map: &str) -> Option<Run> {
        let path = crate::paths::extracted().join("enemies").join("world_scripts.json");
        let text = std::fs::read_to_string(&path).ok()?;
        let mut all: HashMap<String, ScriptDef> = serde_json::from_str(&text).unwrap();
        Some(Run::new_world(Arc::new(all.remove(map)?), 1))
    }

    /// Wolf's yaw facing along `f` (Actor::yaw: forward is (-sin, 0, -cos)).
    fn yaw_along(f: Vec3) -> f32 {
        f32::atan2(-f.x, -f.z)
    }

    /// The HUB door 1111700 (o111410_8000) has two ObjActs, 1113709 (ObjActParam 999940, open
    /// animation 1) and 1113710 (999941, animation 2), one on each side at its dummy polys 121 /
    /// 120 within 1.4 m (validDist 70). Wolf behind the front dummy facing the door works
    /// 1113709; facing away, past the reach or on the far side he does not. Worked, 20005615
    /// sets the door's flag 11110550 and the door is open in the play world: hidden, its
    /// collision off.
    #[test]
    fn wolf_opens_the_castle_hub_door_with_the_action_button() {
        let Some(mut run) = castle() else { return };
        let ev = instance(&run, 20005615, (2005, 19, 1111700.0));
        let far = wolf_at(Vec3::splat(10_000.0), &[]);
        run_for(&mut run, ev, &far, 30);
        let offers = super::offers(&run, 0);
        let front = *offers.iter().find(|o| o.id == 1113709).expect("the door's front ObjAct is on offer");
        assert!((front.dist - 1.4).abs() < 1e-4 && front.player_angle == 60.0);
        let feet = front.at - front.forward * 0.6;
        let facing = yaw_along(front.forward);
        assert_eq!(super::pick_objact(&offers, feet, facing).map(|o| o.id), Some(1113709));
        assert!(super::pick_objact(&offers, feet, facing + std::f32::consts::PI).is_none(), "facing away");
        assert!(super::pick_objact(&offers, front.at - front.forward * 2.0, facing).is_none(), "out of reach");
        let back = *offers.iter().find(|o| o.id == 1113710).unwrap();
        let from_back = back.at - back.forward * 0.6;
        assert_eq!(super::pick_objact(&offers, from_back, yaw_along(back.forward)).map(|o| o.id), Some(1113710));
        // Still shut: the events named the door, it is shown.
        assert_eq!(super::view([&run]).objects.get(&1111700), Some(&true));
        super::work_objact(&mut run, 1113709);
        run_for(&mut run, ev, &far, 1);
        assert!(run.flag(11110550), "20005615 heard the ObjAct");
        assert_eq!(super::view([&run]).objects.get(&1111700), Some(&false), "open: hidden, collision off");
        // 2 s later the event turns its ObjActs off: no longer on offer.
        run_for(&mut run, ev, &far, 2 * 60 + 5);
        assert!(super::offers(&run, 0).iter().all(|o| o.id != 1113709 && o.id != 1113710));
    }

    /// The castle gate 1111603 opened by 11115613 (8301 on: animation 1, its ObjAct 999900's
    /// objAnimId) is open in the play world; a breakable 2005[1] destroyed is gone; a hand-made
    /// event's 2012[1] Activate Map Part off and 2011[1] Activate Hit off reach the view.
    #[test]
    fn opened_gates_broken_objects_and_switched_parts_reach_the_play_world() {
        let Some(mut run) = castle() else { return };
        let ev = instance(&run, 11115613, (2005, 7, 1111603.0));
        run.set_flag(8301, true);
        run_for(&mut run, ev, &wolf_at(Vec3::splat(10_000.0), &[]), 2);
        assert_eq!(super::view([&run]).objects.get(&1111603), Some(&false));
        let def: ScriptDef = serde_json::from_value(serde_json::json!({
            "map": "test", "boss": 0, "world": true, "chars": {}, "events": [{"id": 1, "slot": 0, "ins": [
                [2012, 1, [1110500, 0]], [2011, 1, [1114100, 0]], [2011, 4, [1114101, 1]], [2005, 1, [1111100, 0]], [2005, 3, [1111101, 0]],
            ]}],
        }))
        .unwrap();
        let mut run = Run::new_world(Arc::new(def), 1);
        run_for(&mut run, 0, &wolf_at(Vec3::ZERO, &[]), 1);
        let v = super::view([&run]);
        assert_eq!(v.parts.get(&1110500), Some(&false));
        assert_eq!((v.hits.get(&1114100), v.hits.get(&1114101)), (Some(&false), Some(&true)));
        assert_eq!((v.objects.get(&1111100), v.objects.get(&1111101)), (Some(&false), Some(&false)));
    }

    /// 11115710's 2004[3] Character Warp Request sends 1110701 to the point 1112726: its target
    /// is in the play world at the point's MSB place (X mirrored); a target no block has is not;
    /// a character target (type 2) is where it stands.
    #[test]
    fn a_warp_request_finds_its_target_in_the_play_world() {
        let Some(run) = castle() else { return };
        let Some(outskirts) = block("m11_00_00_00") else { return };
        let runs = [&outskirts, &run];
        let at = run.place_of(1112726).expect("the warp point's place");
        assert_eq!(super::warp_target(&runs, 1, 1, 1112726, |_| None), Some(at));
        assert_eq!(super::warp_target(&runs, 0, 1, 1112726, |_| None), Some(at), "found in another block too");
        assert_eq!(super::warp_target(&runs, 1, 1, 9_999_999, |_| None), None);
        let stands = (Vec3::new(1.0, 2.0, 3.0), 0.5);
        assert_eq!(super::warp_target(&runs, 1, 2, 1110701, |id| (id == 1110701).then_some(stands)), Some(stands));
    }

    /// The six play-world blocks' events (m11_00, m11_01, m11_02, m13, m17, m20) all run from the
    /// start: per map, the instructions, those the VM does not run and their kinds
    /// (docs/kb/map-events.md "P10b"), and each block's events step without a panic.
    #[test]
    fn the_play_worlds_six_blocks_run_with_their_listed_gap_counts() {
        let maps = ["m11_00_00_00", "m11_01_00_00", "m11_02_00_00", "m13_00_00_00", "m17_00_00_00", "m20_00_00_00"];
        let mut counts = Vec::new();
        for map in maps {
            let Some(mut run) = block(map) else { return };
            let ins: Vec<_> = run.def.events.iter().flat_map(|e| &e.ins).collect();
            let unrun: Vec<_> = ins.iter().filter(|x| !script::handled(x.0, x.1)).collect();
            let kinds: std::collections::BTreeSet<_> = unrun.iter().map(|x| (x.0, x.1)).collect();
            eprintln!("{map}: {} events, {} instructions, {} not run ({} kinds): {kinds:?}", run.def.events.len(), ins.len(), unrun.len(), kinds.len());
            counts.push((map, run.def.events.len(), ins.len(), unrun.len(), kinds.len()));
            let far = wolf_at(Vec3::splat(10_000.0), &[]);
            for ev in 0..run.events.len() {
                run_for(&mut run, ev, &far, 3);
            }
        }
        assert_eq!(counts, LISTED);
    }

    /// The play world's collision as sr_map builds it with the six blocks' switchable parts and
    /// objects: it builds, the HUB door 1111700 is one of them, and the castle gate's collision
    /// parts named by 20005615 (1114381, 1114260) too.
    #[test]
    fn the_play_world_collision_takes_the_events_switchable_parts() {
        let cache = crate::sr_map::cache();
        if !cache.join("maps/m11_01_00_00/object_collision.bin").is_file() {
            return;
        }
        let maps = ["m11_00_00_00", "m11_01_00_00", "m11_02_00_00", "m13_00_00_00", "m17_00_00_00", "m20_00_00_00"];
        let runs: Vec<Run> = maps.iter().filter_map(|m| block(m)).collect();
        if runs.len() < maps.len() {
            return;
        }
        let switch = super::switchable_in(runs.iter().map(|r| &*r.def));
        eprintln!("{} collision parts, {} objects switchable", switch.collisions.len(), switch.objects.len());
        assert!(switch.objects.contains(&1111700) && switch.collisions.contains(&1114381) && switch.collisions.contains(&1114260));
        let layouts = sekiro_rs::map::world::play_blocks(&cache, "m11_01_00_00").unwrap();
        let stories = sekiro_rs::map::story::load_blocks(&cache, &layouts).unwrap();
        let world = sekiro_rs::map::world::collision_world_switched(&cache, &layouts, &stories, &switch).unwrap();
        assert!(world.triangle_count() > 0);
    }

    /// (map, events, instructions, not run, kinds not run): docs/kb/map-events.md.
    const LISTED: [(&str, usize, usize, usize, usize); 6] = [
        ("m11_00_00_00", 393, 10064, 901, 72),
        ("m11_01_00_00", 479, 14955, 1167, 53),
        ("m11_02_00_00", 234, 6087, 717, 65),
        ("m13_00_00_00", 100, 3227, 164, 29),
        ("m17_00_00_00", 324, 9525, 849, 50),
        ("m20_00_00_00", 388, 10142, 810, 65),
    ];
}
