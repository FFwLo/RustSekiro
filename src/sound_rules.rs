//! Which sounds the combat asks for (split from sound.rs, which plays them through FMOD): the
//! queue gameplay code fills, the hit and guard sounds as the exe builds them, the defender's
//! materials, the stateInfos that gate TAE sound events and each step's TAE sound windows.

use bevy::prelude::*;

use crate::actor::{Actor, data_for};
use crate::data::Combat;

/// Where the sound plays: the source map (its area and boss music, the indoor regions) and the
/// floor's reverb (MSB SoundSpaceType). Set by the map (map.rs) or the host.
#[derive(Resource, Clone, Default)]
pub struct SoundPlace {
    pub source_map: String,
    pub sound_space: u8,
    pub space: MapSpace,
}

/// How a Bevy point maps to the source map's MSB space.
#[derive(Clone, Copy, Default)]
pub enum MapSpace {
    /// A generated map: none.
    #[default]
    None,
    /// An exported arena: its export origin and turn (map.rs `arena_to_map`).
    Arena { origin: Vec3, yaw: f32 },
    /// The whole map at its own place (sekiro-rs's absolute space): X mirrored.
    Absolute,
}

impl SoundPlace {
    pub fn to_map(&self, p: Vec3) -> Option<Vec3> {
        match self.space {
            MapSpace::None => None,
            MapSpace::Arena { origin, yaw } => {
                let (s, c) = (-yaw).sin_cos();
                let (x, z) = (-p.x, p.z);
                Some(origin + Vec3::new(x * c + z * s, p.y, -x * s + z * c))
            }
            MapSpace::Absolute => Some(Vec3::new(-p.x, p.y, p.z)),
        }
    }
}

/// Sounds requested by gameplay code (base keys, e.g. "s000003010").
#[derive(Resource, Default)]
pub struct SoundQueue(pub Vec<(String, Vec3)>);

/// A sound the core keeps hold of (an effect node's sound moving with it, a heartbeat cut short).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TrackedSound(pub u64);

pub enum SoundCmd {
    Play(TrackedSound, String, Vec3),
    Move(TrackedSound, Vec3),
    Stop(TrackedSound),
}

/// Held sounds for a host that can move and stop them (this game's fmod.rs inserts it with the
/// FMOD event system and plays the commands each frame); absent, only the `SoundQueue`'s
/// one-shots play.
#[derive(Resource, Default)]
pub struct TrackedSounds {
    next: u64,
    pub cmds: Vec<SoundCmd>,
}

impl TrackedSounds {
    /// Starts event `key` (e.g. "s000003010") at `pos`.
    pub fn play(&mut self, key: &str, pos: Vec3) -> TrackedSound {
        self.next += 1;
        let s = TrackedSound(self.next);
        self.cmds.push(SoundCmd::Play(s, key.to_string(), pos));
        s
    }

    pub fn set_position(&mut self, s: TrackedSound, pos: Vec3) {
        self.cmds.push(SoundCmd::Move(s, pos));
    }

    /// Stops it, letting its fade-out play.
    pub fn stop(&mut self, s: TrackedSound) {
        self.cmds.push(SoundCmd::Stop(s));
    }
}

/// HitEffectSeParam's 9-value column groups in paramdef order; the exe picks a group by the
/// attack's atkMaterial_forSe (FUN_1410c0ea0: group k at row offset k * 0x24).
const HIT_SE_GROUPS: [&str; 15] =
    ["Iron", "Fire", "Wood", "Body", "Eclipse", "Energy", "None", "Dmy1", "Dmy2", "Dmy3", "Maggot", "Wax", "FireFlame", "EclipseGas", "EnergyStrong"];

/// Hit sounds as the exe builds them (FUN_14092ecb0 -> FUN_1410c0cb0): for each of the defender's
/// two materials (NpcParam materialSe1/2, protector defenseMaterial1/2) the HitEffectSeParam row
/// of that id gives, at the attack's group / type (index pow + type * 3 over Slash, Blow, Thrust)
/// / power (S, L, LL), a sound of type 12 = 'z'. E.g. Wolf's sword (group Iron, slash, S) on the
/// General (114, 108): z100000103 + z000000108. Zero / missing rows: silent.
pub fn hit_sounds(combat: &Combat, atk: &crate::data::Attack, def_materials: [i64; 2]) -> Vec<String> {
    lookup(combat, "HitEffectSeParam", atk, def_materials)
}

fn lookup(combat: &Combat, table: &str, atk: &crate::data::Attack, def_materials: [i64; 2]) -> Vec<String> {
    let Some(table) = combat.params.get(table) else { return Vec::new() };
    let Some(group) = HIT_SE_GROUPS.get(atk.atk_material_se.max(0) as usize) else { return Vec::new() };
    let kind = match atk.atk_type {
        1 => "Blow",
        2 => "Thrust",
        _ => "Slash",
    };
    let pow = ["S", "L", "LL"][atk.atk_pow_se.clamp(0, 2) as usize];
    def_materials
        .iter()
        .filter(|m| **m >= 0)
        .filter_map(|m| table.get(m.to_string())?.get(format!("{group}_{kind}_{pow}"))?.as_i64())
        .filter(|id| *id > 0)
        .map(|id| format!("z{id:09}"))
        .collect()
}

/// Guard / deflect sounds (the same exe path: FUN_14092f040 for a deflect, HitEffectSeJustGuardParam
/// first and HitEffectSeParam when that gives nothing; a block uses HitEffectSeParam): rows = the
/// attack's defSeMaterial1/2. The General's sword attacks (100 / 139, Iron group): deflect
/// z199999980, block z200000101.
pub fn guard_sounds(combat: &Combat, atk: &crate::data::Attack, deflect: bool) -> Vec<String> {
    let mats = crate::vfx::guard_materials(combat, [atk.def_se_material1, atk.def_se_material2], true);
    if deflect {
        let jg = lookup(combat, "HitEffectSeJustGuardParam", atk, mats);
        if !jg.is_empty() {
            return jg;
        }
    }
    lookup(combat, "HitEffectSeParam", atk, mats)
}

/// The defender's two hit-sound materials: the enemy's NpcParam materialSe1/2, Wolf's body
/// protector defenseMaterial1/2.
pub fn defender_materials(combat: &Combat, def: &crate::actor::Actor) -> [i64; 2] {
    let (r, a, b) = match def.side {
        crate::actor::Side::Enemy => (combat.npc(def), "materialSe1", "materialSe2"),
        crate::actor::Side::Player => (combat.param("EquipParamProtector", 100000), "defenseMaterial1", "defenseMaterial2"),
    };
    [r[a].as_i64().unwrap_or(-1), r[b].as_i64().unwrap_or(-1)]
}

/// stateInfos of the SpEffects on an actor now: its anim's TAE SpEffects, plus (Wolf) the resident
/// SpEffects of his equipment - the equipped prosthetic tool level (e.g. 70000: 127000 "Shuriken
/// LV1", stateInfo 905; 71000 Firecracker: 911) and the combat art - and 914 (SpEffect 106000
/// "No enchantment": Wolf's sword carries no buff; gap: buff items are not modelled). Gated TAE
/// events (PlaySound / SpawnFFX StateInfo) run only when one matches.
pub fn active_state_infos(combat: &Combat, a: &Actor, config: Option<&crate::config::GameConfig>, tool: Option<i64>) -> Vec<i64> {
    const NO_ENCHANTMENT: i64 = 914;
    let d = data_for(combat, &a);
    let mut v: Vec<i64> = if a.anim.is_empty() { Vec::new() } else { d.sp_effects_at(&a.anim, a.t).iter().map(|(_, s)| s.state_info).collect() };
    // The live resident SpEffects too, every side (round 70: enemies' NpcParam residents such as
    // the anim-set rows 200030-200034 -> stateInfo 270-274, or 3107040 -> 950 / 951, gate
    // thousands of PlaySound / SpawnFFX events - c1180, c1220, c1260, c1700, ... - and only
    // Wolf's were counted, so they were silent).
    // A TAE SpEffect of the same spCategory overwrites the resident (the anim-set rows 200030-34
    // are all category 100: a clip's 200031 replaces the resident 200030, HANDOFF ~367): its
    // stateInfo alone gates, not both sets.
    let tae_cats: Vec<i64> = if a.anim.is_empty() { Vec::new() } else { d.sp_effects_at(&a.anim, a.t).iter().filter_map(|(id, _)| crate::data::sp_cycle(*id)).map(|c| c.category).filter(|c| *c > 0).collect() };
    for &id in &a.resident {
        if crate::data::sp_cycle(id).is_some_and(|c| c.category > 0 && tae_cats.contains(&c.category)) {
            continue;
        }
        let si = d.sp_effects.get(&id.to_string()).map(|s| s.state_info).or_else(|| crate::data::sp_cycle(id).map(|c| c.state_info));
        if let Some(si) = si.filter(|s| *s > 0 && !v.contains(s)) {
            v.push(si);
        }
    }
    if a.side == crate::actor::Side::Player {
        v.push(NO_ENCHANTMENT);
        let mut weapons: Vec<i64> = tool.into_iter().collect();
        if let Some(c) = config {
            weapons.push(c.player.combat_art);
        }
        for w in weapons {
            let row = combat.param("EquipParamWeapon", w);
            let mut ids: Vec<i64> = ["residentSpEffectId", "residentSpEffectId1", "residentSpEffectId2"].iter().filter_map(|k| row[*k].as_i64()).collect();
            // The tools' rows are not in params.EquipParamWeapon: their Prosthetic.resident (the
            // Flame Vent's 127200 "Ignition LV1" -> 915 turns on its flame FFX 300161 / 300164).
            if let Some(t) = combat.player.prosthetics.iter().find(|t| t.id == w) {
                ids.push(t.resident);
            }
            // Sen Throw (resident 127022, stateInfo 908): common.emevd (~193180-193570) puts one of
            // 127060-127063 "sen held LV0-3" (stateInfo 999-996) on by the sen held; its throws'
            // judges are gated by them (round 68: none was on, so 70400 threw nothing).
            if ids.contains(&127022) {
                let sen = config.map_or(500, |c| c.player.sen);
                v.push(match sen {
                    0 => 999,
                    1..=249 => 998,
                    250..=499 => 997,
                    _ => 996,
                });
            }
            for id in ids.into_iter().filter(|id| *id > 0) {
                let si = d.sp_effects.get(&id.to_string()).map(|s| s.state_info).or_else(|| combat.param("SpEffectParam", id)["stateInfo"].as_i64());
                if let Some(si) = si.filter(|s| *s > 0) {
                    if !v.contains(&si) {
                        v.push(si);
                    }
                }
            }
        }
    }
    v
}


#[allow(clippy::too_many_arguments)]
/// The clip whose TAE sounds play this step and the time windows (lo, hi] in it: the TAE anim
/// from prev_t to t, or - Wolf's procedural walk / run / crouch-walk (Locomotion, no anim; their
/// footsteps c00000100x / 200x and armour rustle are the clips' PlaySound events) - the shown
/// locomotion clip (`locomotion_clip_at`), wrapping at the loop's end. A step that changes
/// locomotion clip (the move start into the loop) starts the new clip at 0, as does the first
/// step of Locomotion (round 55: before, its window was (0, dt] and a start clip's event at 0,
/// e.g. a000_005100's PlaySound 2100, never played). gap: the rest of the clip left behind is
/// not played, and a turn or a walk / run switch keeps the clip phase (locomotion_clip_at
/// keys on the current move_dir / move_index). Also the clip time (the once-per-step key).
pub(crate) fn sound_windows(a: &Actor, d: &crate::data::CharData) -> Option<(String, Vec<(f32, f32)>, f32)> {
    const AT_ZERO: f32 = -1e-6;
    if !a.anim.is_empty() {
        // An event at 0 fires in the anim's first step.
        let lo = if a.prev_t == 0.0 && a.t > 0.0 && a.t < 0.05 { AT_ZERO } else { a.prev_t };
        return Some((a.anim.clone(), vec![(lo, a.t)], a.t));
    }
    if a.state != "Locomotion" {
        return None;
    }
    let dt = a.t - a.prev_t;
    let (key, t, looping) = a.locomotion_clip_at(a.t);
    let len = d.length(&key);
    let now = if looping && len > 0.0 { t.rem_euclid(len) } else { t };
    if dt <= 0.0 {
        return Some((key, Vec::new(), now));
    }
    if a.prev_t == 0.0 || a.locomotion_clip_at(a.prev_t).0 != key {
        return Some((key, vec![(AT_ZERO, now)], now));
    }
    let before = now - dt;
    let windows = if before >= 0.0 {
        vec![(before, now)]
    } else if looping && len > 0.0 {
        vec![(before + len, len), (AT_ZERO, now)]
    } else {
        vec![(AT_ZERO, now)]
    };
    Some((key, windows, now))
}
