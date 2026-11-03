//! Attack hit detection: weapon capsules from TAE attack windows against body capsules.
//!
//! The data trail, per attack:
//!
//! 1. A TAE `AttackBehavior` event on the attacker's clip opens a window with a behaviour judge
//!    id.
//! 2. `BehaviorParam` (NPCs, id `200000000 + NpcParam.behaviorVariationId * 1000 + judge`) or
//!    `BehaviorParam_PC` (the player, `100000000 + EquipParamWeapon.behaviorVariationId * 1000 +
//!    judge`) names the `AtkParam` row (`refType` 0).
//! 3. The AtkParam row's `hitN_DmyPoly1`/`hitN_DmyPoly2`/`hitN_Radius` describe capsules between
//!    two dummy polys. For NPCs the dummies are on the character model (the soldier's `Kodachi`
//!    bone carries 10 and 11); for the player they are on the weapon model (`WP_A_0300` dummies
//!    100 and 120 on the blade), held at the character's dummy 1 on `R_Weapon`.
//! 4. While the window is open the capsule is swept from the previous step's pose to this one
//!    and tested against the defender's vertical body capsule. Each window hits a defender once.
//!
//! The result is a [`HitEvent`]. [`crate::fight::CombatRules`] turns it into HP and posture
//! damage and the reaction codes both scripts read, on the engine rules in [`crate::combat`].

use crate::character::Character;
use crate::combat::knockback::Knockback;
use crate::combat::params::AttackProfile;
use crate::player::IncomingDamage;
use crate::tae::{TaeFrame, TaeKind, TimedEvent};
use glam::{Mat4, Vec3};
use sekiro_formats::anim::Skeleton;
use sekiro_formats::flver::{self, Flver};
use sekiro_formats::param::{RawParam, Row, Table};
use sekiro_formats::paramdef::ParamDef;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// The "bone" of a dummy with no attach bone: the character's own frame.
const ROOT: usize = usize::MAX;

/// Dummy polys of a character (and its held weapon) relative to animated skeleton bones.
#[derive(Debug, Clone, Default)]
pub struct DummyRig {
    /// Reference id -> list of (Havok bone index, offset from that bone, source space).
    dummies: HashMap<i16, Vec<(usize, Mat4)>>,
}

fn node_world(nodes: &[flver::Node]) -> Vec<Mat4> {
    let mut out: Vec<Option<Mat4>> = vec![None; nodes.len()];
    fn resolve(i: usize, nodes: &[flver::Node], out: &mut [Option<Mat4>], depth: usize) -> Mat4 {
        if let Some(m) = out[i] {
            return m;
        }
        let local = nodes[i].local_matrix();
        let p = nodes[i].parent;
        let m = if p >= 0 && (p as usize) < nodes.len() && depth < nodes.len() {
            resolve(p as usize, nodes, out, depth + 1) * local
        } else {
            local
        };
        out[i] = Some(m);
        m
    }
    (0..nodes.len())
        .map(|i| resolve(i, nodes, &mut out, 0))
        .collect()
}

fn parent_world(world: &[Mat4], d: &flver::Dummy) -> Mat4 {
    usize::try_from(d.parent_bone)
        .ok()
        .and_then(|p| world.get(p).copied())
        .unwrap_or(Mat4::IDENTITY)
}

impl DummyRig {
    /// Dummies of a character model, attached to the Havok bones of the same name.
    pub fn from_flver(model: &Flver, skeleton: &Skeleton) -> Self {
        let world = node_world(&model.nodes);
        let mut rig = DummyRig::default();
        for d in &model.dummies {
            let Some(attach) = usize::try_from(d.attach_bone).ok() else {
                // No attach bone: fixed in the character's frame (Wolf's throw dummies and the
                // air kick's 612 to 614 hang off a root-level helper node).
                rig.dummies
                    .entry(d.reference_id)
                    .or_default()
                    .push((ROOT, parent_world(&world, d) * d.frame()));
                continue;
            };
            let Some(node) = model.nodes.get(attach) else {
                continue;
            };
            let Some(bone) = skeleton.bone_index(&node.name) else {
                continue;
            };
            let frame = parent_world(&world, d) * d.frame();
            let offset = world[attach].inverse() * frame;
            rig.dummies
                .entry(d.reference_id)
                .or_default()
                .push((bone, offset));
        }
        rig
    }

    /// Adds a held weapon's dummies: the weapon model's space is the frame of the character's
    /// dummy `holder_dummy` (WepAbsorpPosParam `right_0`), following that dummy's attach bone.
    pub fn add_weapon(
        &mut self,
        model: &Flver,
        skeleton: &Skeleton,
        weapon: &Flver,
        holder_dummy: i16,
    ) -> bool {
        let world = node_world(&model.nodes);
        let Some(h) = model
            .dummies
            .iter()
            .find(|d| d.reference_id == holder_dummy)
        else {
            return false;
        };
        let Some(attach) = usize::try_from(h.attach_bone).ok() else {
            return false;
        };
        let Some(bone) = model
            .nodes
            .get(attach)
            .and_then(|n| skeleton.bone_index(&n.name))
        else {
            return false;
        };
        let holder = world[attach].inverse() * parent_world(&world, h) * h.frame();
        let wworld = node_world(&weapon.nodes);
        for d in &weapon.dummies {
            let frame = parent_world(&wworld, d) * d.frame();
            self.dummies
                .entry(d.reference_id)
                .or_default()
                .push((bone, holder * frame));
        }
        true
    }

    /// World positions of every dummy with `id`, given source-space bone model matrices and the
    /// body placement (`place * mirror`).
    pub fn positions(&self, id: i16, model: &[Mat4], to_world: Mat4) -> Vec<Vec3> {
        self.dummies
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|(b, off)| {
                let m = if *b == ROOT {
                    Some(Mat4::IDENTITY)
                } else {
                    model.get(*b).copied()
                };
                m.map(|m| to_world.transform_point3((m * *off).w_axis.truncate()))
            })
            .collect()
    }

    /// World frames of every dummy with `id`: position, forward (+Z) and up (+Y), same inputs as
    /// [`DummyRig::positions`]. The body placement mirrors X, so the axes are mirrored too.
    pub fn frames(&self, id: i16, model: &[Mat4], to_world: Mat4) -> Vec<[Vec3; 3]> {
        self.dummies
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|(b, off)| {
                let m = if *b == ROOT {
                    Some(Mat4::IDENTITY)
                } else {
                    model.get(*b).copied()
                };
                m.map(|m| {
                    let frame = m * *off;
                    [
                        to_world.transform_point3(frame.w_axis.truncate()),
                        to_world.transform_vector3(frame.z_axis.truncate()),
                        to_world.transform_vector3(frame.y_axis.truncate()),
                    ]
                })
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.dummies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dummies.is_empty()
    }
}

/// One hit capsule of an attack: two dummy polys and a radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitShape {
    pub dummy_a: i16,
    pub dummy_b: i16,
    pub radius: f32,
}

/// One behaviour judge: its AtkParam row (capsules plus the combat fields) and the BehaviorParam
/// fields the rules read. Guard actions are judges too (the TAE `ShieldBlock` flag names them).
#[derive(Debug, Clone, PartialEq)]
pub struct AttackData {
    pub atk_id: i32,
    pub shapes: Vec<HitShape>,
    /// `dmgLevel`: which hit reaction plays (1 small, 2 middle, 3 large, ...).
    pub damage_level: i32,
    pub atk_phys: i32,
    pub atk_stam: i32,
    /// `atk{Phys,Mag,Fire,Thun,Dark}Correction`: percent of the weapon attack power a player
    /// attack uses.
    pub atk_corrections: [i32; 5],
    /// The combat fields of the AtkParam row.
    pub profile: Option<AttackProfile>,
    /// BehaviorParam `category` (picks the attacker's recoil code family).
    pub category: u8,
    /// BehaviorParam `stamina`: for a guard judge, posture added to every guarded hit.
    pub behavior_stamina: i32,
    /// AtkParam `throwFlag`: 0 an ordinary attack, 1 a throw start (its contact grabs instead of
    /// hitting; see [`crate::grab`]), 2 a throw's own damage (`ThrowAttackBehavior` judges).
    pub throw_flag: u8,
    /// AtkParam `throwTypeId`: which of the attacker's ThrowParam rows a throw start begins
    /// ([`crate::deathblow::ThrowTable::npc_row`]).
    pub throw_type_id: i32,
    /// AtkParam `spEffectId0..4` (positive ids): SpEffects a landed hit leaves on the defender
    /// (the Owl's front kick puts 3506000 on Wolf, which his AI observes).
    pub sp_effects: Vec<i32>,
    /// AtkParam `atkType`, the hit's sound and effect family: 0 slash, 1 blow, 2 thrust.
    pub atk_type: u8,
    /// AtkParam `staminaDamageAttackHitParry`: posture lost when Mikiri-countered.
    pub mikiri_posture: i32,
}

/// What kind of perilous attack (the red kanji) an AtkParam row is, from its guard flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perilous {
    /// A grab (`throwFlag` 1): step or jump out of reach.
    Grab,
    /// Cannot be blocked or deflected by the katana (guard type 0) and is not a grab: a low
    /// sweep, to be jumped (TAE 954 i-frame type 1, "Sweeps", covers it).
    Sweep,
    /// Cannot be blocked by the katana but can be deflected: a thrust (Mikiri counter).
    Thrust,
}

impl AttackData {
    /// The perilous kind of this attack, if it is one: a grab, or an attack Wolf's katana cannot
    /// block (AtkParam `disableGuard_vsGuardAttribute0`). Rows that deal nothing (the Mikiri
    /// "dummy" hitboxes) are not perilous attacks themselves. The split into sweeps and thrusts
    /// by the deflect flag is ours: in the data every perilous sweep (soldier 240, Owl 120 and
    /// 300) is undeflectable and every perilous thrust (soldier 230, Owl 310) deflectable.
    pub fn perilous(&self) -> Option<Perilous> {
        if self.throw_flag == 1 {
            return Some(Perilous::Grab);
        }
        let p = self.profile.as_ref()?;
        let deals = self.atk_phys > 0 || p.attack_power.iter().any(|v| *v > 0.0);
        if !deals || self.throw_flag != 0 || !p.disable_guard[0] {
            return None;
        }
        Some(if p.disable_just_guard[0] {
            Perilous::Sweep
        } else {
            Perilous::Thrust
        })
    }

    /// Whether TAE 954 i-frame type 1 ("Sweeps", on Wolf's jumps, air kicks and the first
    /// 0.27 s of his steps) lets this attack pass.
    pub fn is_sweep(&self) -> bool {
        self.perilous() == Some(Perilous::Sweep)
    }

    pub(crate) fn from_rows(row: &Row, behavior: &Row) -> Self {
        let mut me = Self::from_row(row);
        me.category = behavior.int("category").unwrap_or(0) as u8;
        me.behavior_stamina = behavior.int("stamina").unwrap_or(0) as i32;
        me
    }

    fn from_row(row: &Row) -> Self {
        let int = |f: &str| row.int(f).unwrap_or(0) as i32;
        let shapes = (0..4)
            .filter_map(|i| {
                let a = int(&format!("hit{i}_DmyPoly1"));
                let b = int(&format!("hit{i}_DmyPoly2"));
                let r = row.f32(&format!("hit{i}_Radius")).unwrap_or(0.0);
                (a > 0 && r > 0.0).then_some(HitShape {
                    dummy_a: a as i16,
                    dummy_b: if b > 0 { b as i16 } else { a as i16 },
                    radius: r,
                })
            })
            .collect();
        Self {
            atk_id: row.id(),
            shapes,
            damage_level: int("dmgLevel"),
            atk_phys: int("atkPhys"),
            atk_stam: int("atkStam"),
            atk_corrections: [
                int("atkPhysCorrection"),
                int("atkMagCorrection"),
                int("atkFireCorrection"),
                int("atkThunCorrection"),
                int("atkDarkCorrection"),
            ],
            profile: AttackProfile::from_row(row).ok(),
            category: 0,
            behavior_stamina: 0,
            throw_flag: int("throwFlag") as u8,
            throw_type_id: int("throwTypeId"),
            sp_effects: (0..5)
                .map(|k| int(&format!("spEffectId{k}")))
                .filter(|&id| id > 0)
                .collect(),
            atk_type: int("atkType") as u8,
            mikiri_posture: int("staminaDamageAttackHitParry"),
        }
    }
}

pub(crate) fn load_table(
    param_dir: &Path,
    defs_dir: &Path,
    file: &str,
    def: &str,
) -> Option<Arc<Table>> {
    crate::shared::load(param_dir.join(format!("{file}.param#{def}")), || {
        let raw =
            RawParam::parse(std::fs::read(param_dir.join(format!("{file}.param"))).ok()?).ok()?;
        let def = ParamDef::load(&defs_dir.join(format!("{def}.xml"))).ok()?;
        Table::new(file, &raw, Arc::new(def), None).ok()
    })
}

/// Where a right-hand weapon's models sit on the body: body dummy ids from its
/// WepAbsorpPosParam row (EquipParamWeapon `absorpParamId`). Each model's space is its
/// dummy's frame ([`flver::Dummy::frame`]). Facts from FwLo's MIT RustSekiro (`docs/kb/visuals.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponDummies {
    /// `right_0`: the drawn blade (20, in Wolf's right hand).
    pub held: i16,
    /// `right_1`: the scabbard (147, on the `Sheath` bone).
    pub scabbard: i16,
    /// `rightHang_0`: the blade while sheathed (147, inside the scabbard).
    pub sheathed: i16,
}

impl WeaponDummies {
    /// The WepAbsorpPosParam row `id`, if it has a held dummy.
    pub fn from_row(row: &Row) -> Option<Self> {
        let get = |f: &str| row.int(f).ok().and_then(|v| i16::try_from(v).ok());
        Some(Self {
            held: get("right_0").filter(|&d| d > 0)?,
            scabbard: get("right_1")?,
            sheathed: get("rightHang_0")?,
        })
    }
}

/// What a character attacks with: its dummy rig, behaviour variation and body size.
#[derive(Debug, Clone, Default)]
pub struct Combatant {
    pub rig: DummyRig,
    /// `behaviorVariationId` of the NPC or of the player's weapon.
    pub variation: i32,
    /// Body capsule radius and height, metres.
    pub radius: f32,
    pub height: f32,
    /// Radius of the body that pushes other characters away (NpcParam `chrHitRadius`); 0 means
    /// [`Self::radius`]. Only the Demon of Hatred reads it (1.4 against its 2.8 m hurt capsule):
    /// its sweeps reach Wolf only once he stands that close. Other enemies keep the hurt radius,
    /// which the existing Owl and Genichiro regressions are tuned to (inferred from the names).
    pub body_radius: f32,
    /// Attacks by behaviour judge id.
    pub attacks: HashMap<i32, AttackData>,
    /// The player's `PCBehavior` (TAE 307) attacks by judge id: BehaviorParam_PC rows below 1000,
    /// which do not depend on the weapon (the air kick, judge 901, is AtkParam_Pc 901 "kick in
    /// the air"). Empty for NPCs.
    pub pc_attacks: HashMap<i32, AttackData>,
    /// The row the combat rules read: EquipParamWeapon of the player's right-hand weapon, or
    /// the NPC's NpcParam.
    pub param_id: i32,
    /// Bullets by behaviour judge id (BehaviorParam rows with `refType` 1), with their children.
    pub bullets: HashMap<i32, Arc<crate::bullets::BulletDef>>,
    /// Where the player's right-hand weapon sits on the body; `None` for NPCs.
    pub weapon_dummies: Option<WeaponDummies>,
}

impl Combatant {
    /// Furthest active melee capsule from an attack's starting centre, including actual
    /// root travel and the defender's radius. This is a radial envelope, not a cone test.
    /// No value for projectile-only windows or missing shapes.
    pub fn active_reach(
        &self,
        character: &Character,
        start: [f32; 3],
        defender_radius: f32,
    ) -> Option<f32> {
        if character.tae_frame().attacks.is_empty() {
            return None;
        }
        let model = character.bone_source_matrices();
        let world = character.source_to_world();
        let mut reach: Option<f32> = None;
        for event in &character.tae_frame().attacks {
            let TaeKind::Attack { judge, .. } = event.kind else {
                continue;
            };
            let Some(attack) = self.attacks.get(&judge) else {
                continue;
            };
            for shape in &attack.shapes {
                for dummy in [shape.dummy_a, shape.dummy_b] {
                    for p in self.rig.positions(dummy, &model, world) {
                        let r = ((p.x - start[0]).powi(2) + (p.z - start[2]).powi(2)).sqrt()
                            + shape.radius
                            + defender_radius;
                        reach = Some(reach.unwrap_or(0.0).max(r));
                    }
                }
            }
        }
        reach
    }

    /// The player with the starting katana (CharaInitParam 10010).
    pub fn player(cache: &Path, skeleton: &Skeleton) -> Self {
        let (pd, dd) = param_dirs(cache);
        let read = |p: &Path| {
            crate::shared::load(p.to_owned(), || {
                std::fs::read(p).ok().and_then(|b| flver::parse(&b).ok())
            })
        };
        let mut c = Combatant {
            radius: 0.4,
            height: 1.7,
            ..Combatant::default()
        };
        let Some(model) = read(&cache.join("raw/chr/c0000.chrbnd.d/c0000.flver")) else {
            return c;
        };
        c.rig = DummyRig::from_flver(&model, skeleton);
        let init = load_table(&pd, &dd, "CharaInitParam", "CharaInitParam");
        let weapons = load_table(&pd, &dd, "EquipParamWeapon", "EquipParamWeapon");
        let wep = init
            .as_ref()
            .and_then(|t| t.find(10010))
            .and_then(|r| r.int("equip_Wep_Right").ok())
            .and_then(|id| weapons.as_ref()?.find(id as i32));
        if let Some(w) = wep {
            c.variation = w.int("behaviorVariationId").unwrap_or(0) as i32;
            c.param_id = w.id();
            let model_id = w.int("equipModelId").unwrap_or(0);
            let name = format!("WP_A_{model_id:04}");
            let path = cache.join(format!(
                "raw/parts/{}.partsbnd.d/{name}.flver",
                name.to_ascii_lowercase()
            ));
            let absorp = load_table(&pd, &dd, "WepAbsorpPosParam", "WepAbsorpPosParam");
            c.weapon_dummies = w
                .int("absorpParamId")
                .ok()
                .and_then(|id| absorp.as_ref()?.find(id as i32))
                .and_then(|r| WeaponDummies::from_row(&r));
            if let (Some(blade), Some(at)) = (read(&path), c.weapon_dummies) {
                c.rig.add_weapon(&model, skeleton, &blade, at.held);
            }
        }
        // Rows missing for the weapon's own variation fall back to its hundred (5001 -> 5000),
        // where the katana's ordinary attacks live (observed in the data, as in DS3).
        let base = c.variation / 100 * 100;
        c.attacks = load_attacks(
            &pd,
            &dd,
            "BehaviorParam_PC",
            "AtkParam_Pc",
            100_000_000,
            base,
        );
        // Only the rows that deal damage or posture: the kicks (900, 901, 903). The others are
        // contact impacts (200 to 222, on every jump and landing: no damage, damage levels 5 and
        // 8), effect probes for the hands and feet (280 to 286) and the deathblow starts (600 on,
        // handled by the deathblow rules); leaving them out is ours.
        c.pc_attacks = load_attacks(&pd, &dd, "BehaviorParam_PC", "AtkParam_Pc", 0, 0)
            .into_iter()
            .filter(|(_, a)| {
                a.atk_phys > 0 || a.profile.as_ref().is_some_and(|p| p.direct_stam > 0)
            })
            .collect();
        if base != c.variation {
            c.attacks.extend(load_attacks(
                &pd,
                &dd,
                "BehaviorParam_PC",
                "AtkParam_Pc",
                100_000_000,
                c.variation,
            ));
        }
        c
    }

    /// An NPC by its NpcParam row (the soldier is 10100000).
    pub fn npc(cache: &Path, chr: &str, npc_param: i32, skeleton: &Skeleton) -> Self {
        let (pd, dd) = param_dirs(cache);
        let mut c = Combatant {
            radius: 0.4,
            height: 1.7,
            ..Combatant::default()
        };
        let model_path = cache.join(format!("raw/chr/{chr}.chrbnd.d/{chr}.flver"));
        if let Some(model) = crate::shared::load(model_path.clone(), || {
            std::fs::read(&model_path)
                .ok()
                .and_then(|b| flver::parse(&b).ok())
        }) {
            c.rig = DummyRig::from_flver(&model, skeleton);
        }
        if let Some(row) = load_table(&pd, &dd, "NpcParam", "NpcParam").and_then(|t| {
            t.find(npc_param).map(|r| {
                (
                    r.int("behaviorVariationId").unwrap_or(0) as i32,
                    r.f32("hitRadius").unwrap_or(0.4),
                    r.f32("hitHeight").unwrap_or(1.7),
                    r.f32("chrHitRadius").unwrap_or(0.0),
                )
            })
        }) {
            let body;
            (c.variation, c.radius, c.height, body) = row;
            if chr == crate::demon::CHR {
                c.body_radius = body;
            }
            c.param_id = npc_param;
        }
        c.attacks = load_attacks(
            &pd,
            &dd,
            "BehaviorParam",
            "AtkParam_Npc",
            200_000_000,
            c.variation,
        );
        if let (Some(b), Some(bullets), Some(a)) = (
            load_table(&pd, &dd, "BehaviorParam", "BehaviorParam"),
            load_table(&pd, &dd, "Bullet", "BulletParam"),
            load_table(&pd, &dd, "AtkParam_Npc", "AtkParam"),
        ) {
            let sp = load_table(&pd, &dd, "SpEffectParam", "SpEffect");
            let effects = |id: i32| {
                let Some(row) = sp.as_ref().and_then(|t| t.find(id)) else {
                    return crate::bullets::EffectKind::default();
                };
                let num = |f: &str| row.f32(f).unwrap_or(0.0);
                crate::bullets::EffectKind {
                    poison: row.int("poizonAttackPower").unwrap_or(0) > 0,
                    timed_state: num("effectEndurance") > 0.0
                        && row.int("stateInfo").unwrap_or(0) != 0,
                }
            };
            c.bullets =
                crate::bullets::load_bullets(&b, &bullets, &a, &effects, 200_000_000, c.variation);
        }
        c
    }
}

fn param_dirs(cache: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    (
        cache.join("raw/param/gameparam/gameparam.parambnd.d"),
        cache.join("refs/paramdex/Defs"),
    )
}

fn load_attacks(
    pd: &Path,
    dd: &Path,
    behavior: &str,
    atk: &str,
    base: i32,
    variation: i32,
) -> HashMap<i32, AttackData> {
    let mut out = HashMap::new();
    let (Some(b), Some(a)) = (
        load_table(pd, dd, behavior, "BehaviorParam"),
        load_table(pd, dd, atk, "AtkParam"),
    ) else {
        return out;
    };
    let first = base + variation * 1000;
    for row in b.rows() {
        let id = row.id();
        if !(first..first + 1000).contains(&id) || row.int("refType").unwrap_or(-1) != 0 {
            continue;
        }
        if let Some(r) = a.find(row.int("refId").unwrap_or(-1) as i32) {
            out.insert(id - first, AttackData::from_rows(&r, &row));
        }
    }
    out
}

/// One contact between an attack and a defender.
#[derive(Debug, Clone, PartialEq)]
pub struct HitEvent {
    /// Kernel-local 0/1 during resolution; population reports use 0 for Wolf and foe index + 1.
    pub attacker: usize,
    /// Same namespace as attacker. Aggregated reports must not be resolved by a duel again.
    pub defender: usize,
    pub judge: i32,
    pub attack: AttackData,
    /// Contact point, world space.
    pub position: [f32; 3],
    /// Horizontal direction from attacker to defender, unit length.
    pub direction: [f32; 2],
}

/// The outcome of a hit for both sides' scripts.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HitResult {
    pub defender: IncomingDamage,
    pub attacker: Option<IncomingDamage>,
    pub hp_damage: i32,
    /// Posture taken by the defender.
    pub posture_damage: i32,
    /// Posture sent back to the attacker (deflects and blocks).
    pub attacker_posture_damage: i32,
    pub deflected: bool,
    pub guarded: bool,
    /// The defender (direct hit, or a block that broke its guard) or the attacker (deflected or
    /// blocked) had its posture broken.
    pub posture_broke: bool,
    /// The push the defender takes (see [`crate::combat::knockback`]).
    pub knockback: Option<Knockback>,
    /// Hit stop in seconds for the attacker and the defender.
    pub hit_stop: [f32; 2],
}

/// ChrActionFlag type for "guarding" (`ShieldBlock`). Its ArgB is the behaviour judge of the
/// guard action (90 for Wolf's sword guard, 900 for the soldier's), whose AtkParam row is the
/// guard row the rules read.
pub const FLAG_GUARD: i32 = 3;

/// Closest distance between segments `p1-q1` and `p2-q2`.
pub fn segment_distance(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t);
    if a <= 1e-9 && e <= 1e-9 {
        return r.length();
    }
    if a <= 1e-9 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-9 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > 1e-9 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    ((p1 + d1 * s) - (p2 + d2 * t)).length()
}

/// Whether any attack can land on a character this step. TAE 950 (`IFrames_MistRaven`) and 951
/// (`IFrames_DuringAction`) make it fully invulnerable: they set bits 1 and 0 of the action
/// module's flag word +0x70 (traced by FwLo's RustSekiro, `docs/kb/damage.md`); ChrActionFlag 94
/// sets bit 0 as well (the Owl's warps, [`crate::tae::cancel::WARP_INVULNERABLE`]). An attack that
/// passes through an invulnerable character is not spent: the window can still hit it once the
/// invulnerability ends, as FwLo's notes say for dodges.
///
/// Of the typed i-frames of TAE 954, only grab (7, [`can_be_grabbed`]) and sweep (1,
/// [`IFRAME_SWEEP`]) are applied; general (2) and thrust (6) are not yet: which attacks each
/// type ignores is not traced.
pub fn can_be_hit(frame: &TaeFrame) -> bool {
    !frame.invulnerable
}

/// TAE 954 `IFrameType` 1 ("Sweeps" in the DSAnimStudio template): on Wolf's jumps for most of
/// their air time, his air kicks, and the first 0.27 s of his steps. A sweep
/// ([`AttackData::is_sweep`]) passes through it unspent, as FwLo's RustSekiro does for the attacks
/// that raise a sweep warning (`docs/kb/guard-deflect.md`).
pub const IFRAME_SWEEP: i32 = 1;

/// TAE 954 `IFrameType` 7: Wolf's steps carry it (with type 2, general) for their first 0.27 s;
/// FwLo's RustSekiro lists it as the grab type (`docs/kb/guard-deflect.md`).
pub const IFRAME_GRAB: i32 = 7;

/// Whether a throw start (AtkParam `throwFlag` 1) can catch a character this step: not while it
/// is invulnerable, airborne (a jump gets out of a grab, as in FwLo's RustSekiro), or in a step's
/// grab i-frames. As with [`can_be_hit`], a grab that passes through is not spent.
pub fn can_be_grabbed(frame: &TaeFrame, grounded: bool) -> bool {
    can_be_hit(frame) && grounded && !frame.iframes.contains(&IFRAME_GRAB)
}

/// Tracks attack windows across steps and finds contacts.
#[derive(Debug, Default)]
pub struct HitDetector {
    /// Previous step's capsule ends per (attacker, judge, shape index).
    last: HashMap<(usize, i32, usize), (Vec3, Vec3)>,
    /// (attacker, judge, window start, defender) already hit in the current window.
    done: HashSet<(usize, i32, u32, usize)>,
}

impl HitDetector {
    /// Checks every open attack window of every character against every other character.
    pub fn detect(&mut self, chars: &[&Character], fighters: &[&Combatant]) -> Vec<HitEvent> {
        let mut hits = Vec::new();
        let mut live_windows = HashSet::new();
        let mut live_shapes = HashSet::new();
        for (ai, attacker) in chars.iter().enumerate() {
            let windows: Vec<TimedEvent> = attacker
                .tae_frame()
                .attacks
                .iter()
                .filter(|e| matches!(e.kind, TaeKind::Attack { .. } | TaeKind::PcBehavior { .. }))
                .cloned()
                .collect();
            if windows.is_empty() {
                continue;
            }
            let model = attacker.bone_source_matrices();
            let to_world = attacker.source_to_world();
            for w in windows {
                let (judge, table) = match w.kind {
                    TaeKind::Attack { judge, .. } => (judge, &fighters[ai].attacks),
                    TaeKind::PcBehavior { judge } => (judge, &fighters[ai].pc_attacks),
                    _ => continue,
                };
                let Some(attack) = table.get(&judge) else {
                    continue;
                };
                let window_key = (ai, judge, w.start.to_bits());
                live_windows.insert(window_key);
                for (si, shape) in attack.shapes.iter().enumerate() {
                    let rig = &fighters[ai].rig;
                    let (Some(a), Some(b)) = (
                        rig.positions(shape.dummy_a, &model, to_world)
                            .first()
                            .copied(),
                        rig.positions(shape.dummy_b, &model, to_world)
                            .first()
                            .copied(),
                    ) else {
                        continue;
                    };
                    let key = (ai, judge, si);
                    live_shapes.insert(key);
                    let (pa, pb) = self.last.insert(key, (a, b)).unwrap_or((a, b));
                    for (di, defender) in chars.iter().enumerate() {
                        let reachable = if attack.throw_flag == 1 {
                            can_be_grabbed(defender.tae_frame(), defender.body.grounded)
                        } else {
                            can_be_hit(defender.tae_frame())
                                && !(attack.is_sweep()
                                    && defender.tae_frame().iframes.contains(&IFRAME_SWEEP))
                        };
                        if di == ai
                            || !reachable
                            || (crate::perilous::counter_dummy(attack)
                                && !defender.tae_frame().flag(crate::perilous::FLAG_MIKIRI))
                            || self.done.contains(&(ai, judge, w.start.to_bits(), di))
                        {
                            continue;
                        }
                        let f = fighters[di];
                        let base = Vec3::from_array(defender.body.position);
                        let axis = (
                            base + Vec3::Y * f.radius,
                            base + Vec3::Y * (f.height - f.radius).max(f.radius),
                        );
                        // Sweep: the blade now, before, and the two arcs its ends traced.
                        let d = segment_distance(a, b, axis.0, axis.1)
                            .min(segment_distance(pa, pb, axis.0, axis.1))
                            .min(segment_distance(pa, a, axis.0, axis.1))
                            .min(segment_distance(pb, b, axis.0, axis.1));
                        if d <= shape.radius + f.radius {
                            self.done.insert((ai, judge, w.start.to_bits(), di));
                            let dir = Vec3::new(
                                base.x - attacker.body.position[0],
                                0.0,
                                base.z - attacker.body.position[2],
                            )
                            .normalize_or(Vec3::Z);
                            let mid = (a + b) * 0.5;
                            hits.push(HitEvent {
                                attacker: ai,
                                defender: di,
                                judge,
                                attack: attack.clone(),
                                position: mid.to_array(),
                                direction: [dir.x, dir.z],
                            });
                        }
                    }
                }
            }
        }
        self.last.retain(|k, _| live_shapes.contains(k));
        self.done
            .retain(|(a, j, s, _)| live_windows.contains(&(*a, *j, *s)));
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #103: standing idle with the sword drawn, the katana points forward and a little
    /// outward from the right hand and curves up, as in the real game. The blade sits on
    /// WepAbsorpPosParam `right_0` (dummy 20) with the dummy's frame as its model space; it used
    /// to hang on dummy 1 with its -Y along the dummy's forward, which pointed the same way but
    /// rolled the blade a quarter turn, laying the curve flat out to the side.
    #[test]
    fn idle_katana_points_forward_and_curves_up() {
        let cache = crate::cache::root();
        let (Ok(skeleton), Ok(clip)) = (
            std::fs::read(cache.join("anim/c0000/skeleton.bin")),
            std::fs::read(cache.join("anim/c0000/a000_000000.bin")),
        ) else {
            eprintln!("skipping: c0000 animations are not extracted");
            return;
        };
        let skeleton = Skeleton::from_bytes(&skeleton).unwrap();
        let clip = sekiro_formats::anim::AnimClip::from_bytes(&clip).unwrap();
        let wolf = Combatant::player(&cache, &skeleton);
        let Some(at) = wolf.weapon_dummies else {
            eprintln!("skipping: params are not extracted");
            return;
        };
        assert_eq!((at.held, at.scabbard, at.sheathed), (20, 147, 147));
        // Source space, character local: forward is -Z, Wolf's right is -X.
        let model = skeleton.model_space(&clip.sample(0.0, &skeleton));
        // Katana dummies 120 (at the guard), 110 (mid blade) and 100 (near the tip) lie along
        // its -Y; the tip sits 5 cm off the straight line, toward the side the blade curves to.
        let at = |id: i16| wolf.rig.positions(id, &model, Mat4::IDENTITY)[0];
        let (guard, mid, tip) = (at(120), at(110), at(100));
        let blade = (tip - guard).normalize();
        let forward = blade.dot(Vec3::NEG_Z).acos().to_degrees();
        assert!(
            forward < 35.0,
            "blade {blade:.2} is {forward:.0} deg off forward"
        );
        assert!(
            blade.x < -0.2,
            "blade {blade:.2} does not lean out to the right"
        );
        let straight = (mid - guard).normalize();
        let curve = ((tip - guard) - straight * (tip - guard).dot(straight)).normalize();
        assert!(curve.y > 0.7, "the blade curves toward {curve:.2}, not up");
    }

    #[test]
    fn segment_distances() {
        let d = segment_distance(
            Vec3::new(-1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 0.5),
            Vec3::new(0.0, 2.0, 0.5),
        );
        assert!((d - 0.5).abs() < 1e-5);
        let parallel = segment_distance(Vec3::ZERO, Vec3::Y, Vec3::X, Vec3::X + Vec3::Y);
        assert!((parallel - 1.0).abs() < 1e-5);
    }
}
