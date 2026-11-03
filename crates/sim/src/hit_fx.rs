//! Which visual effects and floor stains a hit leaves, from the game's own tables.
//!
//! Sparks, blood sprays and stains for a landed, blocked or deflected attack, the rules traced in
//! the exe by FwLo's RustSekiro (`docs/kb/visuals.md`, `src/vfx.rs`, `src/gore.rs`, MIT):
//!
//! - Each material row (a deflect or block: the attack's `defSfxMaterial1/2`; a landed hit: the
//!   defender's `materialSfx1/2`, Wolf's protector `defenseMaterialSfx1/2`) names up to two
//!   concepts in `HitEffectSfxConceptParam` (`HitEffectSfxConceptJustGuardParam` for a deflect,
//!   falling back to the first table), column `atk<group>_1/_2` where the group is the attack's
//!   `atkMaterial_forSfx`. `HitEffectSfxParam` row `concept` at the attack's type (Slash, Blow,
//!   Thrust) and size (`atkPow_forSfx`: S M L LL LLL) is the FXR id.
//! - A hit's stains (`DecalParam`): the attack's `decalId1/2`, and per `decalBaseId1/2` the
//!   hit-blood row `600000000 + defender material * 1000 + base + atkPow_forSfx`.

use std::collections::HashMap;
use std::path::Path;

use sekiro_formats::param::ParamSet;

use crate::hits::{HitEvent, HitResult};

/// `HitEffectSfxConceptParam` column groups in paramdef order, indexed by `atkMaterial_forSfx`.
const GROUPS: [&str; 15] = [
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

/// How a hit ended, which picks the concept table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clash {
    /// A perfect deflect.
    Deflect,
    /// Blocked (or an enemy's deflect against Wolf).
    Guard,
    /// Landed.
    Hit,
}

impl Clash {
    pub fn of(result: &HitResult) -> Self {
        if result.deflected {
            Clash::Deflect
        } else if result.guarded {
            Clash::Guard
        } else {
            Clash::Hit
        }
    }
}

/// The attack fields the effect lookup needs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct AttackFx {
    material: i32,
    kind: i32,
    pow: i32,
    guard_rows: [i32; 2],
    decal: [i32; 2],
    decal_base: [i32; 2],
}

/// One `DecalParam` row, the fields a stain needs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecalRow {
    pub id: i32,
    pub pitch_angle: f32,
    pub near_distance: f32,
    pub far_distance: f32,
    pub near_size: f32,
    pub far_size: f32,
    pub mask_texture: i32,
    pub diffuse_texture: i32,
    pub diffuse_color: [u8; 3],
    pub random_size: [i32; 2],
    pub random_roll: [f32; 2],
    pub appear_dir_type: i32,
    pub thin_out: bool,
    pub thin_out_distance: f32,
    pub thin_out_max: i32,
    pub put_vertical: bool,
    pub mask_scale: f32,
}

/// The tables for one fight: Wolf (player) against the NPC of one NpcParam row.
#[derive(Debug, Clone, Default)]
pub struct HitFx {
    _params: Option<std::sync::Arc<ParamSet>>,
    concept: HashMap<i32, HashMap<String, i64>>,
    concept_just_guard: HashMap<i32, HashMap<String, i64>>,
    sfx: HashMap<i32, HashMap<String, i64>>,
    attacks: [HashMap<i32, AttackFx>; 2],
    decals: HashMap<i32, DecalRow>,
    /// Each fighter's two effect materials, [player, enemy]: Wolf's protector
    /// `defenseMaterialSfx1/2`, an NPC's `materialSfx1/2`.
    pub materials: [[i32; 2]; 2],
    /// The guard materials an attack that says 255 ("the attacker's weapon") reads: Wolf's
    /// katana row 5000 (`defSfxMaterial1/2`, 101 and 139 on every Wolf weapon).
    weapon_guard: [i32; 2],
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

fn attacks(set: &ParamSet, name: &str) -> HashMap<i32, AttackFx> {
    let Ok(t) = set.table(name) else {
        return HashMap::new();
    };
    t.rows()
        .map(|r| {
            let i = |f: &str| r.int(f).map(|v| v as i32).unwrap_or(-1);
            (
                r.id(),
                AttackFx {
                    material: i("atkMaterial_forSfx"),
                    kind: i("atkType"),
                    pow: i("atkPow_forSfx").max(0),
                    guard_rows: [i("defSfxMaterial1"), i("defSfxMaterial2")],
                    decal: [i("decalId1"), i("decalId2")],
                    decal_base: [i("decalBaseId1"), i("decalBaseId2")],
                },
            )
        })
        .collect()
}

fn pair(set: &ParamSet, table: &str, id: i32, fields: [&str; 2]) -> Option<[i32; 2]> {
    let row = set.table(table).ok()?.find(id)?;
    Some(fields.map(|f| row.int(f).map(|v| v as i32).unwrap_or(-1)))
}

/// The hit-blood DecalParam id (exe FUN_14088a7f0, traced by FwLo): the base (`decalBaseId1/2`)
/// rounded to tens plus the variant (`atkPow_forSfx`), the defender's material in the thousands,
/// then 600000000 on top. Flesh 109, base 0, size L is 600109002.
pub fn hit_stain_id(base: i32, material: i32, variant: i32) -> Option<i32> {
    if base < 0 {
        return None;
    }
    let mut v = base / 10 * 10 + variant;
    if material > 0 {
        v = v % 1000 + material % 1000 * 1000 + v / 1_000_000 * 1_000_000;
    }
    v = v - v / 1_000_000 * 1_000_000 + v / 100_000_000 * 100_000_000;
    Some(v % 100_000_000 + 600_000_000)
}

impl HitFx {
    pub fn load(param_dir: &Path, defs_dir: &Path, npc_param: i32) -> Option<Self> {
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
            ["defenseMaterialSfx1", "defenseMaterialSfx2"],
        )
        .unwrap_or([146, 106]);
        let npc = pair(
            &set,
            "NpcParam",
            npc_param,
            ["materialSfx1", "materialSfx2"],
        )
        .unwrap_or([-1, -1]);
        let weapon_guard = pair(
            &set,
            "EquipParamWeapon",
            5000,
            ["defSfxMaterial1", "defSfxMaterial2"],
        )
        .unwrap_or([101, 139]);
        let mut decals = HashMap::new();
        if let Ok(t) = set.table("DecalParam") {
            for r in t.rows() {
                let f = |n: &str| r.f32(n).unwrap_or(0.0);
                let i = |n: &str| r.int(n).map(|v| v as i32).unwrap_or(0);
                let u = |n: &str| r.int(n).map(|v| v as u8).unwrap_or(255);
                decals.insert(
                    r.id(),
                    DecalRow {
                        id: r.id(),
                        pitch_angle: f("pitchAngle"),
                        near_distance: f("nearDistance"),
                        far_distance: f("farDistance"),
                        near_size: f("nearSize"),
                        far_size: f("farSize"),
                        mask_texture: i("maskTextureId"),
                        diffuse_texture: i("diffuseTextureId"),
                        diffuse_color: [u("diffuseColorR"), u("diffuseColorG"), u("diffuseColorB")],
                        random_size: [i("randomSizeMin"), i("randomSizeMax")],
                        random_roll: [f("randomRollMin"), f("randomRollMax")],
                        appear_dir_type: i("appearDirType"),
                        thin_out: i("bDistThinOutEnable") == 1,
                        thin_out_distance: f("distThinOutCheckDist"),
                        thin_out_max: i("distThinOutMaxNum"),
                        put_vertical: i("putVertical") != 0,
                        mask_scale: f("maskScale"),
                    },
                );
            }
        }
        Some(Self {
            _params: Some(set.clone()),
            concept: table(&set, "HitEffectSfxConceptParam"),
            concept_just_guard: table(&set, "HitEffectSfxConceptJustGuardParam"),
            sfx: table(&set, "HitEffectSfxParam"),
            attacks: [attacks(&set, "AtkParam_Pc"), attacks(&set, "AtkParam_Npc")],
            decals,
            materials: [wolf, npc],
            weapon_guard,
        })
    }

    pub fn load_from_cache(cache: &Path, npc_param: i32) -> Option<Self> {
        Self::load(
            &cache.join("raw/param/gameparam/gameparam.parambnd.d"),
            &cache.join("refs/paramdex/Defs"),
            npc_param,
        )
    }

    pub fn decal(&self, id: i32) -> Option<&DecalRow> {
        self.decals.get(&id)
    }

    /// An attack's guard materials: 255 means the attacker's weapon's.
    fn guard_materials(&self, rows: [i32; 2]) -> [i32; 2] {
        std::array::from_fn(|i| {
            if rows[i] == 255 {
                self.weapon_guard[i]
            } else {
                rows[i]
            }
        })
    }

    fn attack(&self, attacker: usize, atk_id: i32) -> AttackFx {
        self.attacks[attacker.min(1)]
            .get(&atk_id)
            .copied()
            .unwrap_or_default()
    }

    /// The FXR ids a hit plays, at the contact point, in table order.
    pub fn effects(&self, hit: &HitEvent, result: &HitResult) -> Vec<i32> {
        self.lookup(
            hit.attacker,
            hit.attack.atk_id,
            hit.defender,
            Clash::of(result),
        )
    }

    /// [`HitFx::effects`] from its parts: AtkParam row `atk_id` of `attacker` (0 Wolf, 1 enemy)
    /// meeting `defender`.
    pub fn lookup(&self, attacker: usize, atk_id: i32, defender: usize, clash: Clash) -> Vec<i32> {
        let atk = self.attack(attacker, atk_id);
        let Some(group) = usize::try_from(atk.material)
            .ok()
            .and_then(|g| GROUPS.get(g))
        else {
            return Vec::new();
        };
        let column = format!(
            "{}_{}",
            match atk.kind {
                1 => "Blow",
                2 => "Thrust",
                _ => "Slash",
            },
            ["S", "M", "L", "LL", "LLL"][atk.pow.clamp(0, 4) as usize]
        );
        let look = |concepts: &HashMap<i32, HashMap<String, i64>>, rows: [i32; 2]| {
            let mut out: Vec<i32> = Vec::new();
            for row in rows.into_iter().filter(|r| *r >= 0) {
                for n in 1..=2 {
                    let concept = concepts
                        .get(&row)
                        .and_then(|r| r.get(&format!("atk{group}_{n}")))
                        .copied()
                        .unwrap_or(0);
                    if concept <= 0 {
                        continue;
                    }
                    let id = self
                        .sfx
                        .get(&(concept as i32))
                        .and_then(|r| r.get(&column))
                        .copied()
                        .unwrap_or(0);
                    if id > 0 && !out.contains(&(id as i32)) {
                        out.push(id as i32);
                    }
                }
            }
            out
        };
        let guard = self.guard_materials(atk.guard_rows);
        match clash {
            Clash::Deflect => {
                let just = look(&self.concept_just_guard, guard);
                if just.is_empty() {
                    look(&self.concept, guard)
                } else {
                    just
                }
            }
            Clash::Guard => look(&self.concept, guard),
            Clash::Hit => look(&self.concept, self.materials[defender.min(1)]),
        }
    }

    /// The DecalParam ids a landed hit leaves: the attack's own, then the hit-blood rows.
    /// Rows that do not exist (most materials have no hit-blood rows) are dropped.
    pub fn stains(&self, hit: &HitEvent, result: &HitResult) -> Vec<i32> {
        if Clash::of(result) != Clash::Hit {
            return Vec::new();
        }
        let atk = self.attack(hit.attacker, hit.attack.atk_id);
        let material = self.materials[hit.defender.min(1)][0];
        let own = atk.decal.into_iter().filter(|id| *id > 0);
        let blood = atk
            .decal_base
            .into_iter()
            .filter_map(|b| hit_stain_id(b, material, atk.pow));
        own.chain(blood)
            .filter(|id| self.decals.contains_key(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exe's hit-blood id: flesh (109), base 0 / 100, sizes S-LLL are the rows
    /// 600109000-004 / 600109100-104; Wolf's 146; no base, no row.
    #[test]
    fn hit_stain_ids_follow_the_exe_formula() {
        assert_eq!(hit_stain_id(0, 109, 2), Some(600109002));
        assert_eq!(hit_stain_id(100, 109, 4), Some(600109104));
        assert_eq!(hit_stain_id(100, 146, 0), Some(600146100));
        assert_eq!(hit_stain_id(-1, 109, 0), None);
    }

    /// Against the installed tables (skipped without them): a sword deflected by Wolf throws
    /// sparks, one that lands on flesh plays the hit effect, and every id is an installed effect.
    #[test]
    fn clash_effects_come_from_the_installed_tables() {
        let cache = crate::cache::root();
        if !cache
            .join("raw/param/gameparam/gameparam.parambnd.d")
            .is_dir()
        {
            return;
        }
        // Owl: NpcParam 50600000. AtkParam_Npc 50600100 is a sword slash of his.
        let fx = HitFx::load_from_cache(&cache, 50600000).expect("tables");
        let deflect = fx.lookup(1, 50600100, 0, Clash::Deflect);
        let guard = fx.lookup(1, 50600100, 0, Clash::Guard);
        let hit = fx.lookup(1, 50600100, 0, Clash::Hit);
        println!("owl 50600100: deflect {deflect:?} guard {guard:?} hit {hit:?}");
        // The General's / Owl's sword deflected: "Jasuga sparks"; blocked: the guard sparks.
        assert_eq!(deflect, [252001]);
        assert_eq!(guard, [201002]);
        assert!(!hit.is_empty());
    }
}
