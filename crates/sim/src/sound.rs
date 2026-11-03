//! Which sounds a simulation step plays, by the game's sound event names.
//!
//! Two sources, both from the game's data (see `docs/AUDIO.md`):
//!
//! - TAE `PlaySound_*` events (types 128 to 132) of the clips playing, collected by
//!   [`crate::tae`] as (sound type, id) and turned into event keys by [`tae_keys`]: floor
//!   sounds (`x`) pick the event of the floor material under the character, armour sounds
//!   (`b`) the event of the wearer's armour material;
//! - hit, block and deflect sounds from the combat outcome, looked up in `HitEffectSeParam`
//!   and `HitEffectSeJustGuardParam` ([`HitSounds`]). These are `z` sounds.
//!
//! The material rules (`x` = id rounded down to 1000 plus the floor material, `b` = id plus the
//! armour material, the hit-sound rows and columns) were traced in the exe by FwLo's RustSekiro
//! (`docs/kb/sound.md`, MIT).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use sekiro_formats::param::ParamSet;

use crate::character::Character;
use crate::hits::{HitEvent, HitResult};
use crate::tae::sound_name;

/// Floor material when the ground does not say (the flat test floor): HitMtrlParam 1,
/// cobblestone.
pub const DEFAULT_FLOOR_MATERIAL: u32 = 1;

/// Where a cue came from, for logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CueSource {
    /// A TAE `PlaySound` event.
    Tae,
    /// An attack landing.
    Hit,
    /// An attack blocked.
    Guard,
    /// An attack deflected.
    Deflect,
}

/// One sound to play.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundCue {
    /// Sound event keys to try in order, such as `c101001001` or `z999999980`; the first one
    /// the sound system has plays.
    pub keys: Vec<String>,
    /// 0 the player, 1 the enemy.
    pub actor: usize,
    pub position: [f32; 3],
    pub source: CueSource,
}

/// The event keys of a TAE sound of type `kind` and number `id`, to try in order.
///
/// - Floor (`x`, type 8): `c` + (id rounded down to 1000 + `floor` material), so `x` 1000 on
///   tatami (28) is `c000001028`.
/// - Armour (`b`, type 9): `c` + (id + material) for each of the wearer's two armour materials
///   (Wolf's protector `defenseMaterial1/2`, an NPC's `materialSe1/2`); the first one that
///   exists plays.
/// - Everything else: the type letter and the id ([`sound_name`]).
pub fn tae_keys(kind: i32, id: i32, floor: u32, armour: [i32; 2]) -> Vec<String> {
    match kind {
        8 => vec![sound_name(1, id / 1000 * 1000 + floor as i32)],
        9 => armour
            .iter()
            .filter(|&&m| m > 0)
            .map(|&m| sound_name(1, id + m))
            .collect(),
        _ => vec![sound_name(kind, id)],
    }
}

/// Material group names of the `<Group>_<Kind>_<Size>` columns, in `atkMaterial_forSe` order.
const MATERIALS: [&str; 15] = [
    "Iron",
    "Fire",
    "Wood",
    "Body",
    "Eclipse",
    "Energy",
    "None",
    "Dmy1",
    "Dmy2",
    "Dmy3",
    "Maggot",
    "Wax",
    "FireFlame",
    "EclipseGas",
    "EnergyStrong",
];

/// The `<Group>_<Kind>_<Size>` column for an attack: group from `atkMaterial_forSe`, kind from
/// `atkType` (0 slash, 1 blow, 2 thrust), size from `atkPow_forSe` (0 S, 1 L, 2 LL).
fn column(material: u8, kind: u8, pow: u8) -> String {
    let m = MATERIALS.get(material as usize).copied().unwrap_or("Iron");
    let kind = match kind {
        1 => "Blow",
        2 => "Thrust",
        _ => "Slash",
    };
    let size = match pow {
        1 => "L",
        2 => "LL",
        _ => "S",
    };
    format!("{m}_{kind}_{size}")
}

/// The attack fields the sound lookup needs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AttackSe {
    material: u8,
    kind: u8,
    pow: u8,
    /// Rows for this attack's guard and deflect sounds (`defSeMaterial1/2`).
    guard_rows: [i32; 2],
}

/// Shared immutable lookup, copy-on-write for callers that customize a sound table.
#[derive(Debug, Clone, Default)]
struct Shared<T>(Arc<T>);
impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Clone> std::ops::DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

/// Hit-sound tables and the materials of the two duel characters.
#[derive(Debug, Clone, Default)]
pub struct HitSounds {
    _params: Option<std::sync::Arc<ParamSet>>,
    hit: Shared<HashMap<i32, HashMap<String, i64>>>,
    just_guard: Shared<HashMap<i32, HashMap<String, i64>>>,
    attacks: [Shared<HashMap<i32, AttackSe>>; 2],
    /// Each character's two materials, [player, enemy]: rows of the hit sounds they take and
    /// the offsets of their armour (`b`) sounds. Wolf: his body armour's
    /// `defenseMaterial1/2` (113, 139); an NPC: NpcParam `materialSe1/2` (Owl and the soldier
    /// 114, 108).
    pub materials: [[i32; 2]; 2],
}

fn table(set: &ParamSet, name: &str) -> HashMap<i32, HashMap<String, i64>> {
    let Ok(t) = set.table(name) else {
        return HashMap::new();
    };
    t.rows()
        .map(|r| {
            let fields = r
                .fields()
                .filter_map(|(d, v)| Some((d.name.clone(), v.as_i64()?)))
                .collect();
            (r.id(), fields)
        })
        .collect()
}

fn attacks(set: &ParamSet, name: &str) -> HashMap<i32, AttackSe> {
    let Ok(t) = set.table(name) else {
        return HashMap::new();
    };
    t.rows()
        .map(|r| {
            let u = |f: &str| r.u8(f).unwrap_or(0);
            let i = |f: &str| r.int(f).map(|v| v as i32).unwrap_or(-1);
            (
                r.id(),
                AttackSe {
                    material: u("atkMaterial_forSe"),
                    kind: u("atkType"),
                    pow: u("atkPow_forSe"),
                    guard_rows: [i("defSeMaterial1"), i("defSeMaterial2")],
                },
            )
        })
        .collect()
}

/// Two material fields of a param row, -1 where missing.
fn pair(set: &ParamSet, table: &str, id: i32, fields: [&str; 2]) -> Option<[i32; 2]> {
    let row = set.table(table).ok()?.find(id)?;
    Some(fields.map(|f| row.int(f).map(|v| v as i32).unwrap_or(-1)))
}

impl HitSounds {
    /// Loads the tables for Wolf (CharaInitParam 10010, his body armour's materials) against
    /// the NPC of NpcParam row `npc_param`.
    pub fn load(param_dir: &Path, defs_dir: &Path, npc_param: i32) -> Option<Self> {
        let _timer = crate::perf::Scope::start("audio param load");
        let set = crate::shared::load(param_dir.to_owned(), || {
            ParamSet::load(param_dir, defs_dir).ok()
        })?;
        let armour = set
            .table("CharaInitParam")
            .ok()
            .and_then(|t| t.find(10010))
            .and_then(|r| r.int("equip_Armer").ok())
            .unwrap_or(131000) as i32;
        let wolf = pair(
            &set,
            "EquipParamProtector",
            armour,
            ["defenseMaterial1", "defenseMaterial2"],
        )
        .unwrap_or([113, 139]);
        let npc =
            pair(&set, "NpcParam", npc_param, ["materialSe1", "materialSe2"]).unwrap_or([114, 108]);
        Some(Self {
            _params: Some(set.clone()),
            hit: Shared(crate::shared::load(
                param_dir.join("sound-HitEffectSeParam"),
                || Some(table(&set, "HitEffectSeParam")),
            )?),
            just_guard: Shared(crate::shared::load(
                param_dir.join("sound-HitEffectSeJustGuardParam"),
                || Some(table(&set, "HitEffectSeJustGuardParam")),
            )?),
            attacks: ["AtkParam_Pc", "AtkParam_Npc"].map(|name| {
                Shared(
                    crate::shared::load(param_dir.join(format!("sound-{name}")), || {
                        Some(attacks(&set, name))
                    })
                    .unwrap(),
                )
            }),
            materials: [wolf, npc],
        })
    }

    /// [`HitSounds::load`] from the extracted params in `cache`.
    pub fn load_from_cache(cache: &Path, npc_param: i32) -> Option<Self> {
        Self::load(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
            npc_param,
        )
    }

    /// The sounds of one hit: one cue per sound the tables name.
    pub fn cues(&self, hit: &HitEvent, result: &HitResult) -> Vec<SoundCue> {
        let source = if result.deflected {
            CueSource::Deflect
        } else if result.guarded {
            CueSource::Guard
        } else {
            CueSource::Hit
        };
        self.lookup(
            hit.attacker,
            hit.attack.atk_id,
            hit.defender,
            result.deflected,
            result.guarded,
        )
        .into_iter()
        .map(|name| SoundCue {
            keys: vec![name],
            actor: hit.defender,
            position: hit.position,
            source,
        })
        .collect()
    }

    /// The sound event names for an attack (AtkParam row `atk_id` of `attacker`: 0 player,
    /// 1 enemy) meeting `defender`, deflected, blocked or landing.
    ///
    /// The exe's rules (traced by FwLo): a landed hit reads `HitEffectSeParam` at each of the
    /// defender's two materials; a block reads it at the attack's own `defSeMaterial1/2`; a
    /// deflect reads `HitEffectSeJustGuardParam` at those rows, and `HitEffectSeParam` when that
    /// names nothing. The column is the attack's group, kind and size; the values are `z` ids.
    pub fn lookup(
        &self,
        attacker: usize,
        atk_id: i32,
        defender: usize,
        deflected: bool,
        guarded: bool,
    ) -> Vec<String> {
        let atk = self.attacks[attacker.min(1)]
            .get(&atk_id)
            .copied()
            .unwrap_or_default();
        let col = column(atk.material, atk.kind, atk.pow);
        let read = |table: &HashMap<i32, HashMap<String, i64>>, rows: [i32; 2]| -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            for row in rows.into_iter().filter(|&r| r >= 0) {
                if let Some(&id) = table.get(&row).and_then(|r| r.get(&col))
                    && id > 0
                {
                    let name = sound_name(12, id as i32);
                    if !out.contains(&name) {
                        out.push(name);
                    }
                }
            }
            out
        };
        if deflected {
            let jg = read(&self.just_guard, atk.guard_rows);
            if !jg.is_empty() {
                return jg;
            }
            read(&self.hit, atk.guard_rows)
        } else if guarded {
            read(&self.hit, atk.guard_rows)
        } else {
            read(&self.hit, self.materials[defender.min(1)])
        }
    }
}

/// The TAE sound cues that started on `ch` this step; `armour` is its two armour materials
/// ([`HitSounds::materials`]). Floor sounds use the material of the ground under its feet.
pub fn tae_cues(actor: usize, ch: &Character, armour: [i32; 2]) -> Vec<SoundCue> {
    let sounds = &ch.tae_frame().sounds;
    if sounds.is_empty() {
        return Vec::new();
    }
    let [x, y, z] = ch.body.position;
    // The HitMtrlParam row, without the material's special setting: a rideable tree trunk (808)
    // sounds as wood (8).
    let floor = ch
        .ground
        .material(x, z, y + 0.5)
        .and_then(crate::body::hit_mtrl_row)
        .unwrap_or(DEFAULT_FLOOR_MATERIAL);
    sounds
        .iter()
        .map(|&(kind, id)| SoundCue {
            keys: tae_keys(kind, id, floor, armour),
            actor,
            position: ch.body.position,
            source: CueSource::Tae,
        })
        .filter(|c| !c.keys.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_tables_share_without_sharing_mutations() {
        let cache = crate::cache::root();
        if !cache
            .join("raw/param/gameparam/gameparam.parambnd.d")
            .is_dir()
        {
            return;
        }
        let mut a = HitSounds::load_from_cache(&cache, 10100000).unwrap();
        let b = HitSounds::load_from_cache(&cache, 10109009).unwrap();
        assert!(Arc::ptr_eq(&a.hit.0, &b.hit.0));
        assert!(Arc::ptr_eq(&a.attacks[1].0, &b.attacks[1].0));
        let original = b.hit.get(&114).cloned();
        a.hit.insert(114, HashMap::new());
        assert_eq!(b.hit.get(&114).cloned(), original);
    }

    #[test]
    fn floor_sounds_pick_the_floor_material() {
        // Wolf's footstep x 1000 on tatami (28), cobblestone (1) and tile (16).
        assert_eq!(tae_keys(8, 1000, 28, [113, 139]), ["c000001028"]);
        assert_eq!(tae_keys(8, 1003, 1, [113, 139]), ["c000001001"]);
        assert_eq!(tae_keys(8, 2000, 16, [-1, -1]), ["c000002016"]);
    }

    #[test]
    fn armour_sounds_try_both_materials() {
        assert_eq!(
            tae_keys(9, 1000, 1, [113, 139]),
            ["c000001113", "c000001139"]
        );
        assert_eq!(tae_keys(9, 0, 1, [114, -1]), ["c000000114"]);
        assert!(tae_keys(9, 0, 1, [-1, 0]).is_empty());
    }

    #[test]
    fn other_sounds_keep_their_letter() {
        assert_eq!(tae_keys(1, 4010, 1, [113, 139]), ["c000004010"]);
        assert_eq!(tae_keys(1, 506001001, 1, [114, 108]), ["c506001001"]);
        assert_eq!(tae_keys(5, 404000, 1, [0, 0]), ["s000404000"]);
        assert_eq!(sound_name(12, 999999980), "z999999980");
    }

    #[test]
    fn columns_follow_type_and_power() {
        assert_eq!(column(0, 0, 0), "Iron_Slash_S");
        assert_eq!(column(3, 1, 1), "Body_Blow_L");
        assert_eq!(column(0, 2, 2), "Iron_Thrust_LL");
        assert_eq!(column(99, 7, 9), "Iron_Slash_S");
    }

    fn sounds() -> HitSounds {
        let mut s = HitSounds {
            materials: [[113, 139], [114, 108]],
            ..Default::default()
        };
        let row = |pairs: &[(&str, i64)]| -> HashMap<String, i64> {
            pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
        };
        s.hit.insert(100, row(&[("Iron_Slash_S", 200000101)]));
        s.hit.insert(114, row(&[("Iron_Slash_S", 100000103)]));
        s.hit.insert(108, row(&[("Iron_Slash_S", 108)]));
        s.hit.insert(113, row(&[("Iron_Slash_S", 0)]));
        s.just_guard
            .insert(100, row(&[("Iron_Slash_S", 199999980)]));
        s.attacks[0].insert(
            1,
            AttackSe {
                guard_rows: [100, -1],
                ..Default::default()
            },
        );
        s.attacks[1].insert(
            2,
            AttackSe {
                guard_rows: [139, -1],
                ..Default::default()
            },
        );
        s
    }

    #[test]
    fn hits_read_both_defender_materials() {
        assert_eq!(
            sounds().lookup(0, 1, 1, false, false),
            ["z100000103", "z000000108"]
        );
        // A zero entry is silent.
        assert!(sounds().lookup(1, 2, 0, false, false).is_empty());
    }

    #[test]
    fn guards_and_deflects_read_the_attack_rows() {
        let s = sounds();
        assert_eq!(s.lookup(0, 1, 1, false, true), ["z200000101"]);
        assert_eq!(s.lookup(0, 1, 1, true, false), ["z199999980"]);
        // Nothing in the deflect table: falls back to the block table, here also empty.
        assert!(s.lookup(1, 2, 0, true, false).is_empty());
    }
}
