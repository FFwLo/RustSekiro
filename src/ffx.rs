//! Skill and prosthetic visual effects, driven by the characters' own TAE: SpawnOneShotFFX (96) /
//! SpawnFFX_Blade (118) events say which effect, on which dummy and for how long; StateInfo-gated
//! ones run while Wolf holds the matching state (the equipped tool's resident: 905 Shuriken, 911
//! Firecracker, 915 Flame Vent ...; 914 "no enchantment"; see sound::active_state_infos).
//! The effects themselves are the game's FXRs, played by fxr.rs. An FFX the game's archives do not
//! hold (the eight sfxbnd files have no f<id>.fxr: 440114, 304022, 303023, 300214 ...) shows
//! nothing, as in the game; the hand-made looks that stood in for them are gone.
//! Foot dust: FootSfxParam_Entity (792) and SpawnFFX_ByFloor (112) pick the effect from
//! FootSfxParam (see `FootSfx`).
//! Also the default weapon trail on Wolf's sword while an AttackBehavior hit window runs (TAE 790
//! DisableDefaultWeaponTrail turns it off).

use bevy::prelude::*;
use std::collections::HashMap;

use crate::actor::{Actor, data_for};
use crate::data::Combat;
use crate::model::Dummies;

/// An FFX event whose effect ends with it (or when the anim changes): a slotted one (SlotID >= 0),
/// SpawnFFX_Blade (118), and one with IsRestrictToDummyPoly: the game then sets the effect's
/// External(0) to 1 ("set to 1 when ... the SpawnOneShotFFX event ends and it has
/// IsRestrictToDummyPoly enabled", reference/fxr src/fxr.ts ExternalValue.Sekiro.Terminate), and
/// 1211 of the 1213 extracted effects have the state "External(0) >= 1 then terminate". Without
/// it the arts' and tools' effects with an endless node ran to the 20 s cap (manual test #4).
/// gap: the two effects without that state stop as well.
pub(crate) fn ends_with_event(e: &crate::data::Event) -> bool {
    let restrict = e.args.get("IsRestrictToDummyPoly").and_then(|v| v.as_bool()) == Some(true);
    e.arg_i64("SlotID").is_some_and(|s| s >= 0) || e.kind == 118 || restrict
}

/// A dummy id as the TAE writes it -> the dummies to try: 1xxxx = Wolf's right weapon (the blade;
/// 12200 / 12220 are the Mortal Blade's, which is not modelled: the sword stands in), 2xxxx the
/// prosthetic tool's parts (model/tool.rs: 2<model><dummy>; without the part, the left hand / arm
/// dummies stand in).
pub(crate) fn dummy_candidates(id: i64) -> Vec<i16> {
    match id {
        // 1<model><dummy>: Model2's are kept under their full id (model.rs), Model0's under the bare one.
        10000..=19999 => vec![id as i16, (id % 10000) as i16, 300, 301, 311, 121, 100, 20],
        20000..=29999 => vec![id as i16, (id % 10000) as i16, 145, 153, 144, 2],
        _ => vec![id as i16, 220],
    }
}

/// FootSfxParam: row = the character's foot effect (Wolf TentativePlayerParam
/// PlayerFootEffect_bySFX 1, NPCs NpcParam footEffectSfxId), column 0-199 = the TAE index.
/// TAE 792 FootSfxParam_Entity (FUN_140b54690 -> FUN_1409da4c0): its FootSFXIndex column of the
/// row (FUN_1410be4e0 / FUN_1410be490: 0 or past 199 = none; the row comes from the character,
/// chr+0x1ff8 -> +0xb0 -> +0x28), e.g. Wolf's walk 1 -> 101001, run 11 -> 101011.
#[derive(Resource, Default)]
pub struct FootSfx {
    rows: HashMap<i64, Vec<i64>>,
    player_row: Option<i64>,
    /// ThrowDirectionSfxParam (TAE 117 SpawnFFX_ThrowDirection): row = a hit material (109 meat,
    /// 114 meat under armour, 132 phantom, 134 poisonous, 135 white), column = the event's
    /// ThrowDirectionSFXIndex (Wolf's deathblows: 11 -> 220501 on meat).
    throw_rows: HashMap<i64, Vec<i64>>,
}

impl FootSfx {
    pub fn load() -> Self {
        let read = |name: &str| -> Vec<serde_json::Value> {
            let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
            let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Vec::new() };
            match &v["rows"] {
                serde_json::Value::Array(a) => a.clone(),
                serde_json::Value::Object(o) => o.values().cloned().collect(),
                _ => Vec::new(),
            }
        };
        let table = |name: &str, n: usize| -> HashMap<i64, Vec<i64>> {
            read(name)
                .iter()
                .filter_map(|r| {
                    let mut cols = vec![0i64; n];
                    for (k, v) in r.as_object()? {
                        if let (Some(i), Some(v)) = (k.strip_prefix("sfxId_").and_then(|i| i.parse::<usize>().ok()), v.as_i64()) {
                            if i < cols.len() {
                                cols[i] = v;
                            }
                        }
                    }
                    Some((r["id"].as_i64()?, cols))
                })
                .collect()
        };
        let rows = table("FootSfxParam", 200);
        let throw_rows = table("ThrowDirectionSfxParam", 90);
        let player_row = read("TentativePlayerParam").iter().find(|r| r["id"].as_i64() == Some(0)).and_then(|r| r["PlayerFootEffect_bySFX"].as_i64());
        Self { rows, player_row, throw_rows }
    }

    /// TAE 117's effect: the ThrowDirectionSfxParam row of `material`, column `index`.
    pub fn throw_id(&self, material: i64, index: i64) -> Option<i64> {
        let v = *self.throw_rows.get(&material)?.get(usize::try_from(index).ok()?)?;
        (v > 0).then_some(v)
    }

    /// The effect of one column of a row, if any.
    pub fn id(&self, row: i64, col: i64) -> Option<i64> {
        let v = *self.rows.get(&row)?.get(usize::try_from(col).ok()?)?;
        (v > 0).then_some(v)
    }
}

pub struct FfxPlugin;

impl Plugin for FfxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FootSfx::load())
            // emit reads the TAE edges (prev_t, t), which move once per fixed step.
            .add_systems(FixedUpdate, emit.after(crate::actor::ActorSet::Advance).after(crate::anim::PoseSet))
            .add_systems(Update, (weapon_trail, skill_shots.after(crate::player::read_input), tae_sp_vfx));
        // Skill shots: every frame is 1/60 s of game time, so a shot lands on the same anim time
        // however slow the frame (the first press loads the art's effects and hitches).
        if std::env::var("SHINOBI_SKILL_SHOTS").is_ok() {
            app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f64(1.0 / 60.0)));
        }
    }
}

/// The SpEffects an enemy's own TAE puts on it (SpEffect events 67 / 66 / 401, data
/// `sp_effects_at`) show their SpEffectVfx as the afflictions do (status.rs `show`): initSfxId
/// once when it starts, midstSfxId riding its dummy while it lasts, stopped when it ends. The
/// Demon of Hatred's burning body 3702011 (vfx 3702003 / 3702004 -> 670215 / 670216 on dummies
/// 820 / 821), the Guardian Ape's 3510500 -> 651050 (dummy 31), the Divine Dragon's 3520000 ->
/// 652012 (dummy 600). gap: the resident NpcParam SpEffects' VFX (the bosses' 23061x "enemy
/// turn" glow, gated on state 955) are not shown.
#[allow(clippy::type_complexity)]
pub(crate) fn tae_sp_vfx(
    mut commands: Commands,
    combat: Res<crate::data::Combat>,
    rows: Option<Res<crate::status::VfxRows>>,
    actors: Query<(Entity, &crate::actor::Actor), With<crate::enemy::Enemy>>,
    dummies: Query<&crate::model::Dummies>,
    tfs: Query<&GlobalTransform>,
    mut fx: Query<&mut crate::fxr::FxEffect>,
    mut live: Local<HashMap<(Entity, i64), Vec<Entity>>>,
) {
    let Some(rows) = rows else { return };
    let mut now: std::collections::HashSet<(Entity, i64)> = std::collections::HashSet::new();
    for (e, a) in &actors {
        if a.anim.is_empty() || a.hp <= 0.0 {
            continue;
        }
        let d = crate::actor::data_for(&combat, a);
        for (id, s) in d.sp_effects_at(&a.anim, a.t) {
            if s.vfx_id <= 0 && s.vfx_id1 <= 0 {
                continue;
            }
            now.insert((e, id));
            if live.contains_key(&(e, id)) {
                continue;
            }
            let mut kept = Vec::new();
            for vfx in [s.vfx_id, s.vfx_id1] {
                let Some(&[init, init_dmy, midst, midst_dmy]) = rows.0.get(&vfx) else { continue };
                for (fid, dmy, keep) in [(init, init_dmy, false), (midst, midst_dmy, true)] {
                    if fid <= 0 {
                        continue;
                    }
                    let anchor = dummies.get(e).ok().and_then(|d| d.0.get(&(dmy as i16)).copied()).unwrap_or(e);
                    let at = tfs.get(anchor).map(|g| g.compute_transform()).unwrap_or_default();
                    let mut f = crate::fxr::FxEffect::new(fid, Some(crate::fxr::Anchor::Entity(anchor)), true, at);
                    f.keep = keep;
                    debug!("SpEffect {id} ({}): effect {fid} on dummy {dmy}", a.anim);
                    let spawned = commands.spawn(f).id();
                    if keep {
                        kept.push(spawned);
                    }
                }
            }
            live.insert((e, id), kept);
        }
    }
    live.retain(|k, v| {
        let on = now.contains(k);
        if !on {
            for f in v.iter() {
                if let Ok(mut f) = fx.get_mut(*f) {
                    f.stop = true;
                }
            }
        }
        on
    });
}

#[allow(clippy::too_many_arguments)]
fn emit(
    mut commands: Commands,
    combat: Res<Combat>,
    config: Res<crate::config::GameConfig>,
    actors: Query<(&Actor, &Dummies, Option<&crate::player::Player>)>,
    globals: Query<&GlobalTransform>,
    mut frames: Local<u32>,
    mut lib: ResMut<crate::fxr::FxrLib>,
    actor_e: Query<Entity, With<Actor>>,
    mut playing: Query<&mut crate::fxr::FxEffect>,
    foot: Res<FootSfx>,
) {
    // Slotted / held effects end with their TAE event (after its length on their own clock,
    // FxEffect::window), when the anim changes, or when a looping anim (an idle, started over at
    // t 0 under the same key) starts over.
    for mut f in &mut playing {
        if let Some((owner, anim, idx)) = f.key.clone() {
            let window = f.window;
            let age = f.age();
            let alive = actors.iter().zip(actor_e.iter()).find(|(_, e)| *e == owner).is_some_and(|((a, _, _), _)| {
                // The event's end on the clip's own clock (round 96: the effect's age kept running
                // through a hit stop that froze the clip, ending it before its end frame); the
                // age window stays as a backstop.
                a.anim == anim && a.t >= a.prev_t && data_for(&combat, &a).anim(&a.anim).and_then(|an| an.events.get(idx)).is_some_and(|ev| a.t <= ev.end + 1e-3) && age < window + 1.0
            });
            if !alive {
                f.stop = true;
            }
        }
    }
    *frames += 1;
    // SHINOBI_LOG_DUMMIES[=frame] (default 200): Wolf's dummies at that frame, the tool parts' too.
    let log_at = std::env::var("SHINOBI_LOG_DUMMIES").ok().map(|v| v.parse::<u32>().unwrap_or(200));
    if log_at == Some(*frames) {
        for (a, dm, p) in &actors {
            if p.is_some() {
                let mut ids: Vec<i16> = dm.0.keys().copied().collect();
                ids.sort();
                info!("wolf dummies ({}): {:?}", a.state, ids);
                let hand = dm.0.get(&20).and_then(|e| globals.get(*e).ok()).map(|g| g.translation()).unwrap_or_default();
                for k in [100i16, 101, 102, 110, 111, 112, 113, 120, 121, 123, 124, 300, 301, 311, 321, 331, 147, 149, 21, 20300, 20301, 21100, 21130, 21300, 21301] {
                    if let Some(g) = dm.0.get(&k).and_then(|e| globals.get(*e).ok()) {
                        info!("dummy {k}: {:.2} m from the sword hand, at {:.2}", g.translation().distance(hand), g.translation());
                    }
                }
            }
        }
    }
    for ((a, dm, player), owner) in actors.iter().zip(actor_e.iter()) {
        if a.t < a.prev_t {
            continue;
        }
        let d = data_for(&combat, &a);
        // The TAE anim's step, or Wolf's procedural walk / run: the shown locomotion clip (its
        // foot dust, TAE 792 / 112) - the same windows as its footstep sounds.
        let Some((clip, windows, _)) = crate::sound_rules::sound_windows(a, d) else { continue };
        let procedural = a.anim.is_empty();
        let Some(anim) = d.anim(&clip) else { continue };
        let mut states: Option<Vec<i64>> = None;
        let foot_row = if player.is_some() { foot.player_row } else { combat.npc(a)["footEffectSfxId"].as_i64() };
        // TAE 117: Wolf's throw target's material (an enemy's own 117s have no partner here).
        let throw_material = player.and_then(|p| p.throw_target).and_then(|t| actors.iter().zip(actor_e.iter()).find(|(_, e)| *e == t)).map(|((ta, _, _), _)| combat.npc(ta)["materialSfx1"].as_i64().unwrap_or(-1));
        for (idx, e) in anim.events.iter().enumerate().filter(|(_, e)| matches!(e.kind, 96 | 112 | 114 | 117 | 118 | 792)) {
            // gap: SpawnFFX_ByFloor (112) has no case in the TAE dispatcher (FUN_140b59170) and
            // its handler is not found; its FFXIDs (Wolf: 0 / 1 / 11 / 111 on the feet 5 / 6)
            // are FootSfxParam columns (column 0 = column 1 there), so it is read as 792 is.
            // The floor material's part (if any) is not traced.
            let ffx = match e.kind {
                792 => foot_row.zip(e.arg_i64("FootSFXIndex")).and_then(|(r, c)| foot.id(r, c)),
                112 => foot_row.zip(e.arg_i64("FFXID")).and_then(|(r, c)| foot.id(r, c)),
                // gap: the exe's row choice is not traced (FUN_140b56d40 -> ChrThrowDirectionSfxSlot
                // takes a final sfx id; no ThrowDirectionSfxParam getter found, HANDOFF 4.57): read
                // as the throw partner's NpcParam materialSfx1 (only the material rows 109 / 114 /
                // 132 / 134 / 135 have effects, all blood of that material).
                117 => throw_material.zip(e.arg_i64("ThrowDirectionSFXIndex")).and_then(|(m, i)| foot.throw_id(m, i)),
                // SpawnFFX_GoodsAndMagic (ParamType 1 EquipParamGoods): the quick item in use's
                // own effect, SFXIndexID 0 / 1 / 2 -> its castSfxId / fireSfxId / effectSfxId
                // (Pellet 310020, the powders' puff 3100x0 / x1, the coin purse's 310070 / 71 ...).
                // gap: ParamType 0 (magic) is not used by Wolf's clips; the exe's lookup is not traced.
                // The gourd's drinks are no item_use: goods 3000 (its repeat 3001: fireSfxId 310015).
                114 => player.and_then(|p| p.item_use.as_ref().map(|(g, _)| *g).or_else(|| a.state.contains("GourdDrink").then_some(if a.state.contains("Repeat") { 3001 } else { 3000 }))).and_then(|goods| {
                    let col = match e.args.get("SFXIndexID")? {
                        serde_json::Value::String(s) => s.split(':').next()?.trim().parse::<i64>().ok()?,
                        v => v.as_i64()?,
                    };
                    let key = ["castSfxId", "fireSfxId", "effectSfxId"].get(usize::try_from(col).ok()?)?;
                    crate::items::param("EquipParamGoods", goods)[*key].as_i64().filter(|v| *v > 0)
                }),
                _ => e.arg_i64("FFXID").filter(|v| *v > 0),
            };
            let Some(ffx) = ffx else { continue };
            if !lib.has(ffx) {
                if std::env::var("SHINOBI_FFX_LOG").is_ok() && windows.iter().any(|&(lo, hi)| e.start > lo && e.start <= hi) {
                    info!("ffx {ffx} skipped: not in the game's effects ({clip} {:.2})", a.t);
                }
                continue;
            }
            // The effect spawns at the event's start (a locomotion clip's events only start; gap:
            // held ones are not kept up).
            if !windows.iter().any(|&(lo, hi)| e.start > lo && e.start <= hi) {
                continue;
            }
            if !e.ungated() {
                let gate = e.arg_i64("StateInfo").unwrap_or(0);
                let tool = player.and_then(|p| crate::player::equipped_tool(&combat, &config, p.tool_slot)).map(|t| t.id);
                // Wolf's also take player.rs's `resident_gates`: the art's "has emblems" one
                // (Mortal Draw's 993 gates its 440005-440008, which the skills recording plays with
                // every 440001 / 440003) and the timed SpEffects'.
                let active = states.get_or_insert_with(|| {
                    let mut v = crate::sound_rules::active_state_infos(&combat, a, Some(&config), tool);
                    if player.is_some() {
                        v.extend(combat.player.resident_gates.iter().copied());
                    }
                    v
                });
                if !active.contains(&gate) {
                    continue;
                }
            }
            // SpawnFFX_Blade (118): DummyPolyBladeBaseID (DSAnimStudio TAE.Template.SDT: FFXID,
            // DummyPolySource, DummyPolyBladeBaseID, DummyPolyBladeTipID; Wolf's are the axe's /
            // spear's 21300 with tip -1), else the sword's blade.
            let dmy = if e.kind == 118 {
                e.arg_i64("DummyPolyBladeBaseID").filter(|d| *d >= 0).unwrap_or(10301)
            } else {
                e.arg_i64("DummyPolyID").unwrap_or(10301)
            };
            let Some(g) = dummy_candidates(dmy).iter().find_map(|k| dm.0.get(k)).and_then(|e| globals.get(*e).ok()) else {
                if std::env::var("SHINOBI_FFX_LOG").is_ok() {
                    info!("ffx {ffx} skipped: no dummy {dmy} ({clip} {:.2})", a.t);
                }
                continue;
            };
            let pos = g.translation();
            // The game's own effect (FXR): spawned at the event start on the dummy
            // (IsFollowDummyPoly: rides on it); some end with the event (`ends_with_event`). A
            // weapon effect on a weapon dummy that is not loaded: a frame along Kusabimaru's blade,
            // hilt (dummy 300) to tip (301).
            let along = (10000..20000).contains(&dmy) && !dm.0.contains_key(&(dmy as i16)) && !dm.0.contains_key(&((dmy % 10000) as i16));
            let blade = (dm.0.get(&300).copied(), dm.0.get(&301).copied());
            let anchor = match (along, blade) {
                (true, (Some(h), Some(t))) => Some(crate::fxr::Anchor::Blade(h, t)),
                _ => dummy_candidates(dmy).iter().find_map(|k| dm.0.get(k)).copied().map(crate::fxr::Anchor::Entity),
            };
            let follow = e.args.get("IsFollowDummyPoly").and_then(|v| v.as_bool()).unwrap_or(e.kind != 112);
            if std::env::var("SHINOBI_FFX_LOG").is_ok() {
                // Ahead / side / up of the character (forward -Z, Actor::forward), as the
                // recorder's fx rows read (side = +x in the game's own axes).
                let rel = globals.get(owner).ok().map(|w| {
                    let (d, f) = (pos - w.translation(), (w.rotation() * Vec3::NEG_Z).normalize_or(Vec3::NEG_Z));
                    Vec3::new(d.dot(f), d.dot(f.cross(Vec3::Y)), d.y)
                });
                info!("fxr {ffx} on {dmy} ({clip} {:.2}) at {:.2}, ahead / side / up of the actor {:.2?}", a.t, pos, rel);
            }
            // Foot dust with IsIgnoreDummyPolyAngle (all of Wolf's 792s): at the foot,
            // turned as the character, not as the foot.
            // gap: 112 has no such flag (taken as set), and a following one (792
            // FootSFXIndex 31) stays where it started.
            let flat = matches!(e.kind, 112 | 792) && e.args.get("IsIgnoreDummyPolyAngle").and_then(|v| v.as_bool()).unwrap_or(true);
            let mut fx = if flat {
                let turn = globals.get(owner).map(|g| g.rotation()).unwrap_or_default();
                crate::fxr::FxEffect::new(ffx, None, false, Transform::from_translation(pos).with_rotation(turn))
            } else {
                crate::fxr::FxEffect::new(ffx, anchor, follow, Transform::from_translation(pos))
            };
            // sound.rs plays 's' + the FFX id for these two (fxr.rs then skips the root's own).
            fx.own_sound_played = matches!(e.kind, 96 | 118);
            if !procedural && ends_with_event(e) {
                let key = (owner, a.anim.clone(), idx);
                // One instance per event: a new start replaces the old one.
                for mut old in &mut playing {
                    if old.key.as_ref() == Some(&key) {
                        old.stop = true;
                    }
                }
                fx.key = Some(key);
                fx.window = (e.end - e.start).max(0.0);
            }
            commands.spawn(fx);
        }
    }
}

/// Default weapon trail, while an AttackBehavior (TAE 1) window runs on Wolf - unless TAE 790
/// DisableDefaultWeaponTrail does: the weapon's EquipParamWeapon traceSfxId0 from dummy
/// traceDmyIdHead0 to traceDmyIdTail0 (every sword row: FXR 401000, 300 -> 301; the row is the
/// equipped art's, 5000 "style: none" without one), played by fxr.rs while the window runs. The
/// arts' own trails are their TAE effects (emit). The hand-made ribbon that stood in without the
/// FXR is gone (every row's trace effect is in the game's archives).
fn weapon_trail(
    mut commands: Commands,
    combat: Res<Combat>,
    config: Res<crate::config::GameConfig>,
    mut lib: ResMut<crate::fxr::FxrLib>,
    mut effects: Query<&mut crate::fxr::FxEffect>,
    mut game_trail: Local<Option<Entity>>,
    wolf: Query<(&Actor, &Dummies), With<crate::player::Player>>,
) {
    let Ok((a, dm)) = wolf.single() else { return };
    let d = &combat.player;
    let on = !a.anim.is_empty() && d.events_at(&a.anim, a.t).any(|e| e.kind == 1) && !d.events_at(&a.anim, a.t).any(|e| e.kind == 790);
    if std::env::var("SHINOBI_FFX_LOG").is_ok() && (on || a.anim.starts_with("a100")) {
        info!("trail check {} {:.2} on {on}", a.anim, a.t);
    }
    // Started when the window opens, stopped (its segments fade out) when it ends.
    let row = combat.param("EquipParamWeapon", if config.player.combat_art > 0 { config.player.combat_art } else { 5000 });
    let sfx = row["traceSfxId0"].as_i64().unwrap_or(-1);
    let blade = match (row["traceDmyIdHead0"].as_i64(), row["traceDmyIdTail0"].as_i64()) {
        (Some(h), Some(t)) => dm.0.get(&(h as i16)).copied().zip(dm.0.get(&(t as i16)).copied()),
        _ => None,
    };
    let live = game_trail.and_then(|e| effects.get_mut(e).ok());
    match (on, live, blade) {
        (true, None, Some((h, t))) if sfx > 0 && lib.has(sfx) => {
            *game_trail = Some(commands.spawn(crate::fxr::FxEffect::new(sfx, Some(crate::fxr::Anchor::Blade(h, t)), true, Transform::IDENTITY)).id());
        }
        (false, Some(mut f), _) => {
            f.stop = true;
            *game_trail = None;
        }
        _ => {}
    }
}

/// SHINOBI_SKILL=<EquipParamWeapon id> (a combat art 5100-7700 or a tool 70000-79200; or an
/// EquipParamGoods id 3000-3999, a quick item) with
/// SHINOBI_SKILL_SHOTS=<dir>: Wolf equips it, uses it 3 s in (art: attack + guard; tool: F; item: R) and
/// screenshots every 3 frames for 1.5 s, then the game exits. For checking the looks.
/// SHINOBI_SKILL_EVERY=<frames> repeats the press.
fn skill_shots(
    mut commands: Commands,
    mut config: ResMut<crate::config::GameConfig>,
    mut pad: ResMut<crate::player::PadInput>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
    wolf: Query<(&crate::actor::Actor, &Transform), With<crate::player::Player>>,
) {
    let (Ok(id), Ok(dir)) = (std::env::var("SHINOBI_SKILL"), std::env::var("SHINOBI_SKILL_SHOTS")) else { return };
    let Ok(id) = id.parse::<i64>() else { return };
    *frame += 1;
    let f = *frame;
    let goods = (3000..4000).contains(&id);
    if f == 2 {
        if goods {
            config.player.items.retain(|(g, _)| *g != id);
            config.player.items.push((id, 9));
            config.player.quick_items = vec![id, 0, 0, 0, 0];
        } else if (70000..80000).contains(&id) {
            config.player.prosthetics = vec![id];
        } else {
            config.player.combat_art = id;
        }
    }
    // SHINOBI_SKILL_CAM="yaw,pitch" [deg] holds the camera there (camera.rs shot_knobs).
    let start = 180;
    // SHINOBI_SKILL_EVERY=<frames>: press again every so many frames (build-ups: the Flame Vent's burn).
    let every = std::env::var("SHINOBI_SKILL_EVERY").ok().and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 0);
    if f == start || every.is_some_and(|n| f > start && (f - start) % n == 0) {
        if goods {
            pad.press(crate::player::Action::UseItem);
        } else if (70000..80000).contains(&id) {
            pad.press(crate::player::Action::Prosthetic);
        } else {
            pad.press(crate::player::Action::CombatArt);
        }
    }
    // SHINOBI_SKILL_HOLD=<frames>: keep the button down that long after the press (the held
    // versions: the axe's spin, the spear's pull, the umbrella's guard ...).
    let hold = std::env::var("SHINOBI_SKILL_HOLD").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    if f >= start && f < start + hold {
        if (70000..80000).contains(&id) {
            pad.prosthetic_held = true;
        } else {
            (pad.attack_held, pad.guard_held) = (true, true);
        }
    }
    // SHINOBI_SKILL_SHOT_RANGE="from,to,step" frames after the press (default 3,90,3).
    let r: Vec<u32> = std::env::var("SHINOBI_SKILL_SHOT_RANGE").unwrap_or("3,90,3".into()).split(',').filter_map(|v| v.parse().ok()).collect();
    let (from, to, step) = if r.len() == 3 { (r[0], r[1], r[2].max(1)) } else { (3, 90, 3) };
    if f >= start + from && f <= start + to && (f - start - from) % step == 0 {
        let path = format!("{dir}/s{:03}.png", f - start);
        if let Ok((a, tf)) = wolf.single() {
            let fwd = tf.rotation * Vec3::Z;
            info!("shot s{:03}: {} {} t {:.2} at {:.2} facing {:.0}", f - start, a.state, a.anim, a.t, tf.translation, fwd.x.atan2(fwd.z).to_degrees());
        }
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
    }
    if f > start + to + 10 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    /// Manual test #4: the Shuriken's a070_400000 SpawnOneShotFFX 300080 (SlotID -1,
    /// IsRestrictToDummyPoly) has an endless node; it ends with its event, as the game's External(0).
    /// SpawnFFX_ByFloor (112) has no such flag and plays out.
    #[test]
    fn restricted_one_shot_effects_end_with_their_event() {
        let path = crate::paths::extracted().join("combat_data.json");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let d: serde_json::Value = serde_json::from_str(&text).unwrap();
        let events: Vec<crate::data::Event> = serde_json::from_value(d["player"]["anims"]["a070_400000"]["events"].clone()).unwrap();
        let fx = events.iter().find(|e| e.kind == 96 && e.arg_i64("FFXID") == Some(300080)).expect("300080 event");
        assert_eq!(fx.arg_i64("SlotID"), Some(-1));
        assert!(super::ends_with_event(fx));
        let floor = crate::data::Event { kind: 112, start: 0.0, end: 0.1, args: serde_json::json!({"DummyPolyID": 5, "FFXID": 0}) };
        assert!(!super::ends_with_event(&floor));
    }

    #[test]
    fn foot_dust_from_foot_sfx_param() {
        let f = super::FootSfx::load();
        if f.rows.is_empty() {
            return;
        }
        // Wolf's row 1: walk 1 (column 0 is the same), run 11, and nothing past the table.
        assert_eq!(f.player_row, Some(1));
        assert_eq!(f.id(1, 0), Some(101001));
        assert_eq!(f.id(1, 1), Some(101001));
        assert_eq!(f.id(1, 11), Some(101011));
        assert_eq!(f.id(1, 5), None);
        assert_eq!(f.id(1, 200), None);
    }
}
