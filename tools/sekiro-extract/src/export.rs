//! Builds extracted/combat_data.json: the subset of Sekiro data the game
//! reads at startup. Inputs are the outputs of `unpack` and `params`.
//!
//! Layout:
//!   anims:     "a050_300000" -> { duration, rootRate, root: [[x,z,yaw]...], events: [...] }
//!   states:    behavior state name -> anim key (from the CMSG table)
//!   spEffects: id -> selected SpEffectParam fields
//!   attacks:   BehaviorParam id -> selected AtkParam fields
//!   params:    the specific rows the posture/movement code needs

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::{hkx, tae};

struct Character {
    /// e.g. "c0000"
    id: &'static str,
    /// BehaviorParam row = base + variation * 1000 + judgeId.
    behavior_base: i64,
    behavior_param: &'static str,
    atk_param: &'static str,
    /// Weapon motion category for offsetType 13 (Kusabimaru = 50).
    weapon_category: u32,
    /// Prosthetic motion category for offsetType 14 (Shuriken = 70) and its behavior base.
    sub_weapon_category: u32,
    sub_behavior_base: i64,
    /// NpcParam row whose resident SpEffects are exported (NPCs only).
    npc_row: Option<i64>,
}

const PLAYER: Character = Character {
    id: "c0000",
    behavior_base: 100_000_000 + 5000 * 1000,
    behavior_param: "BehaviorParam_PC",
    atk_param: "AtkParam_Pc",
    weapon_category: 50,
    sub_weapon_category: 70,
    sub_behavior_base: 100_000_000 + 7000 * 1000,
    npc_row: None,
};

/// Wolf's default outfit (head, body, arms, legs), as used for the model export.
pub const PLAYER_PROTECTORS: &[i64] = &[100000, 101000, 102000, 103000];

const SAMURAI_GENERAL: Character = Character {
    id: "c1020",
    behavior_base: 200_000_000 + 10200 * 1000,
    behavior_param: "BehaviorParam",
    atk_param: "AtkParam_Npc",
    weapon_category: 0,
    npc_row: Some(10203010),
    sub_weapon_category: 0,
    sub_behavior_base: 0,
};

const OCHIMUSHA: Character = Character {
    id: "c1010",
    behavior_base: 200_000_000 + 10100 * 1000,
    behavior_param: "BehaviorParam",
    atk_param: "AtkParam_Npc",
    weapon_category: 0,
    npc_row: Some(10100000),
    sub_weapon_category: 0,
    sub_behavior_base: 0,
};

const SP_EFFECT_FIELDS: &[&str] = &[
    "name", "behaviorRefId", "stateInfo", "defStaminaAttackRate", "defFlickPower", "staminaAttackRate",
    "staminaRecoverChangeSpeed", "staminaRecoverSpeedRate", "guardDefFlickPowerRate", "flickDamageCutRate",
    "guardStaminaCutRate", "toughnessDamageCutRate", "effectEndurance", "conditionHp", "spCategory",
    "changeHpRate", "changeHpPoint", "changeHpEstusFlaskRate",
];

const BULLET_FIELDS: &[&str] = &[
    "initVellocity", "maxVellocity", "accelInRange", "accelOutRange", "gravityInRange", "gravityOutRange", "dist",
    "life", "hitRadius", "hitRadiusMax", "lockShootLimitAng", "atkId_Bullet",
];

const ATK_FIELDS: &[&str] = &[
    "name", "atkPhys", "atkStam", "atkStamCorrection", "atkSuperArmor", "guardAtkRate", "guardBreakRate",
    "guardStaminaCutRate", "hit0_Radius", "hit1_Radius", "hit0_DmyPoly1", "hit0_DmyPoly2", "atkAttribute",
    "spAttribute", "atkType", "guardAttribute", "staminaPhysicsAttribute", "atkMaterial_forSe", "atkPow_forSe", "defSeMaterial1", "defSeMaterial2", "deflectAction", "deflectedAction", "justDeflectAction",
    "justDeflectedAction", "isDisableParry", "disableGuard_vsGuardAttribute0", "disableJustGuard_vsGuardAttribute0",
    "directAtkStamDamage", "directAtkStamDamage_Attacker", "repelLostStamDamage", "repelLostStamDamage_Attacker",
    "repelVictoryStamDamage_Attacker", "staminaDamageAttackHitParry", "knockbackDist_Guard", "knockbackDist_JustGuard",
    "atkFlickPower", "flickPower", "atkPhysCorrection", "atkStamCorrection", "directAtkStamCorrection",
    "guardAtkRateCorrection", "guardStaminaCutRate", "hitStopTime", "hitStopTime_Defencer", "dmgLevel", "dmgLevel_vsPlayer",
    "throwFlag", "knockbackDist_DirectHit", "atkMagCorrection", "atkFireCorrection", "atkThunCorrection",
    "atkDarkCorrection",
];

fn load_param(json_dir: &Path, name: &str) -> HashMap<i64, Map<String, Value>> {
    let text = std::fs::read_to_string(json_dir.join(format!("{name}.json")))
        .unwrap_or_else(|_| panic!("{name}.json missing: run `params` first"));
    let v: Value = serde_json::from_str(&text).unwrap();
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].as_i64().unwrap(), r.as_object().unwrap().clone()))
        .collect()
}

fn pick(row: &Map<String, Value>, fields: &[&str]) -> Value {
    Value::Object(fields.iter().filter_map(|f| row.get(*f).map(|v| (f.to_string(), v.clone()))).collect())
}

/// Every TAE in an anibnd, keyed "aXXX_YYYYYY".
fn load_taes(dir: &Path, tmpl: &HashMap<i32, tae::EventTemplate>) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "tae") {
            continue;
        }
        let t = tae::read_tae(&std::fs::read(&p).unwrap(), tmpl);
        // Player TAEs are split per group ("a50.tae" = a050_*); an NPC has one "cXXXX.tae" for a000_*.
        let stem = p.file_stem().unwrap().to_string_lossy();
        let group: u32 = stem.strip_prefix('a').and_then(|n| n.parse().ok()).unwrap_or(0);
        for a in t["anims"].as_array().unwrap() {
            out.insert(format!("a{group:03}_{:06}", a["id"].as_i64().unwrap()), a.clone());
        }
    }
    out
}

/// Finds "<key>.hkx" in any "<chr>_*.anibnd.d" folder, with that folder's compendium.
fn find_hkx(chr_dir: &Path, chr: &str, key: &str, compendiums: &mut HashMap<PathBuf, hkx::Types>) -> Option<(Vec<u8>, hkx::Types)> {
    for e in std::fs::read_dir(chr_dir).ok()?.flatten() {
        let dir = e.path();
        let name = dir.file_name()?.to_string_lossy().to_string();
        if !(name.starts_with(&format!("{chr}_")) || name == format!("{chr}.anibnd.d")) || !name.ends_with(".anibnd.d") {
            continue;
        }
        let file = dir.join(format!("{key}.hkx"));
        if !file.exists() {
            continue;
        }
        let types = compendiums
            .entry(dir.clone())
            .or_insert_with(|| {
                std::fs::read_dir(&dir)
                    .unwrap()
                    .flatten()
                    .find(|f| f.path().extension().is_some_and(|x| x == "compendium"))
                    .map(|f| hkx::read_types(&std::fs::read(f.path()).unwrap()))
                    .unwrap_or_default()
            })
            .clone();
        return Some((std::fs::read(file).ok()?, types));
    }
    None
}

/// Resolves ImportOtherAnim chains to the anim whose events are used.
fn resolve<'a>(taes: &'a HashMap<String, Value>, key: &str) -> Option<(&'a Value, String)> {
    let mut k = key.to_string();
    for _ in 0..8 {
        let a = taes.get(&k)?;
        match a.get("importFrom").and_then(Value::as_i64) {
            // importFrom is group * 1e6 + anim id. Wolf's TAEs are split per group, where it can
            // cross groups (a201_500200 imports 200500200 = a200_500200); an NPC's single TAE
            // keeps its own group.
            Some(src) => {
                let other = format!("a{:03}_{:06}", src / 1_000_000, src % 1_000_000);
                k = if taes.contains_key(&other) { other } else { format!("{}_{:06}", &k[..4], src % 1_000_000) };
            }
            None => return Some((a, k)),
        }
    }
    None
}

fn export_character(
    root: &Path,
    chr: &Character,
    state_names: &[String],
    extra_anims: &[String],
    tmpl: &HashMap<i32, tae::EventTemplate>,
    params: &Path,
) -> Value {
    let chr_dir = root.join("chr");
    let taes = load_taes(&chr_dir.join(format!("{}.anibnd.d", chr.id)), tmpl);
    // The character's own behavior plus the shared NPC behavior (c9997.hkx) shipped in NPC behbnds.
    let mut cmsg = HashMap::new();
    for file in ["c9997".to_string(), chr.id.to_string()] {
        let path = chr_dir.join(format!("{}.behbnd.d/{file}.hkx", chr.id));
        if let Ok(d) = std::fs::read(&path) {
            cmsg.extend(hkx::cmsg_map(&d));
        }
    }

    let mut states = BTreeMap::new();
    let mut keys: BTreeSet<String> = extra_anims.iter().cloned().collect();
    let wanted: Vec<&String> = if state_names.is_empty() { cmsg.keys().collect() } else { state_names.iter().collect() };
    let quiet = state_names.is_empty();
    for s in wanted {
        let Some(&(anim, offset_type)) = cmsg.get(s) else {
            eprintln!("{}: no behavior state {s}", chr.id);
            continue;
        };
        if quiet && anim <= 0 {
            continue;
        }
        let group = match offset_type {
            // 11 = player common, 15 = NPC (offset by the "anime ID offset" SpEffect, 0 here)
            11 | 15 | 0 => 0,
            13 => chr.weapon_category,
            14 if chr.sub_weapon_category > 0 => chr.sub_weapon_category,
            other => {
                if !quiet {
                    eprintln!("{}: state {s} uses offsetType {other}, skipped", chr.id);
                }
                continue;
            }
        };
        // offsetType is not a strict guarantee of the anim group; fall back to the other set.
        let other = if group == 0 { chr.weapon_category } else { 0 };
        let mut key = format!("a{group:03}_{anim:06}");
        if !taes.contains_key(&key) && taes.contains_key(&format!("a{other:03}_{anim:06}")) {
            key = format!("a{other:03}_{anim:06}");
        }
        keys.insert(key.clone());
        states.insert(s.clone(), key);
    }
    if state_names.is_empty() && chr.id != "c0000" {
        keys.extend(taes.keys().cloned());
    }

    let mut compendiums = HashMap::new();
    let mut anims = Map::new();
    let mut clips: Vec<(String, hkx::Clip)> = Vec::new();
    let mut sp_ids = BTreeSet::new();
    let mut judge_ids = BTreeSet::new();
    // Combat-art judges by art anim group (a100..a110 = spAtkcategory), resolved through the
    // art's own behavior variation (EquipParamWeapon 5x00 behaviorVariationId).
    let mut art_judges: BTreeSet<(i64, i64)> = BTreeSet::new();
    // PCBehavior (TAE 307) with GetWeaponData 0: BehaviorParam_PC row = the judge itself
    // (e.g. 901 "Kick_in the air" -> AtkParam_Pc 901; 210/211 sprint body contact).
    let mut pc_judges = BTreeSet::new();
    // Bullet judges (TAE 2) per anim group: prosthetic (a070) anims resolve at the
    // prosthetic's behavior base.
    let mut bullet_judges: BTreeSet<(i64, i64)> = BTreeSet::new();
    for key in &keys {
        let Some((tae, src)) = resolve(&taes, key) else {
            eprintln!("{}: no TAE for {key}", chr.id);
            continue;
        };
        let mut a = Map::new();
        if src != *key {
            a.insert("eventsFrom".into(), json!(src));
        }
        let events: Vec<Value> = tae["events"].as_array().unwrap().clone();
        for e in &events {
            if let Some(id) = e["args"].get("SpEffectID").and_then(Value::as_i64) {
                sp_ids.insert(id);
            }
            if e["type"] == 2 && chr.sub_behavior_base > 0 && key.starts_with(&format!("a{:03}_", chr.sub_weapon_category)) {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    bullet_judges.insert((chr.sub_behavior_base, j));
                }
            }
            if e["type"] == 307 && chr.npc_row.is_none() && e["args"].get("GetWeaponData").and_then(Value::as_i64) == Some(0) {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    pc_judges.insert(j);
                }
            }
            // AttackBehavior (1) and ThrowAttackBehavior (304) both resolve through BehaviorParam.
            if e["type"] == 1 || e["type"] == 304 {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    judge_ids.insert(j);
                    let group = key.get(1..4).and_then(|g| g.parse::<i64>().ok()).unwrap_or(0);
                    if chr.npc_row.is_none() && (100..=110).contains(&group) {
                        art_judges.insert((group, j));
                    }
                }
            }
        }
        // TAE Standard mini header ImportsHKX: the clip is another anim's (e.g. a201_511200 ->
        // its own TAE with a shared clip).
        let hkx_key = taes
            .get(key)
            .and_then(|t| t.get("hkxFrom"))
            .and_then(Value::as_i64)
            .map(|src| format!("{}_{:06}", &key[..4], src % 1_000_000));
        // ImportOtherAnim (TAE mini header 1) brings the HKX along with the events: e.g. the
        // broken-enemy deathblows a201_511200 / 511400 play a201_510200 / 510300's clips.
        let found = find_hkx(&chr_dir, chr.id, key, &mut compendiums)
            .or_else(|| hkx_key.as_deref().and_then(|k| find_hkx(&chr_dir, chr.id, k, &mut compendiums)))
            .or_else(|| if src != *key { find_hkx(&chr_dir, chr.id, &src, &mut compendiums) } else { None });
        if let Some(k) = hkx_key.as_deref().filter(|_| found.is_some()) {
            a.insert("hkxFrom".into(), json!(k));
        }
        if let Some((data, types)) = found {
            if let Some(c) = hkx::clip(&data, &types) {
                clips.push((key.clone(), c));
            }
            let (dur, samples) = hkx::root_motion(&data, &types);
            if let Some(d) = dur {
                a.insert("duration".into(), json!(d));
                if samples.len() > 1 {
                    a.insert("rootRate".into(), json!((samples.len() - 1) as f32 / d));
                    // Havok space -> game space: x is mirrored, forward stays -Z.
                    let root: Vec<Value> = samples.iter().map(|s| json!([-s[0], s[1], s[2], s[3]])).collect();
                    a.insert("root".into(), Value::Array(root));
                }
            }
        }
        a.insert("events".into(), Value::Array(events));
        anims.insert(key.clone(), Value::Object(a));
    }

    // Resident SpEffects of the player's default outfit (EquipParamProtector
    // residentSpEffectId 1-3), e.g. 5221-5223: HP-conditional posture regen.
    if chr.npc_row.is_none() {
        let prot = load_param(params, "EquipParamProtector");
        for id in PLAYER_PROTECTORS {
            if let Some(r) = prot.get(id) {
                for k in ["residentSpEffectId", "residentSpEffectId2", "residentSpEffectId3"] {
                    if let Some(v) = r.get(k).and_then(Value::as_i64).filter(|&v| v > 0) {
                        sp_ids.insert(v);
                    }
                }
            }
        }
    }
    // Area scaling ("Growth Doping": Ashina Outskirts morning 7010 = base, 7011-7014 later).
    if chr.npc_row.is_some() {
        sp_ids.extend(7010..=7014);
    } else {
        // Healing Gourd (goods 3000 -> SpEffect 3000, changeHpEstusFlaskRate -40), applied by the
        // item rather than a TAE event.
        sp_ids.insert(3000);
    }
    // Resident SpEffects (NpcParam spEffectID0..31) for NPCs, e.g. HP-conditional posture regen.
    if let Some(npc_row) = chr.npc_row {
        let npc = load_param(params, "NpcParam");
        if let Some(r) = npc.get(&npc_row) {
            for i in 0..32 {
                if let Some(id) = r.get(&format!("spEffectID{i}")).and_then(Value::as_i64).filter(|&v| v > 0) {
                    sp_ids.insert(id);
                }
            }
        }
    }
    let sp = load_param(params, "SpEffectParam");
    let sp_effects: Map<String, Value> = sp_ids
        .iter()
        .filter_map(|id| sp.get(id).map(|r| (id.to_string(), pick(r, SP_EFFECT_FIELDS))))
        .collect();

    let beh = load_param(params, chr.behavior_param);
    let atk = load_param(params, chr.atk_param);
    let mut attacks = Map::new();
    for j in judge_ids {
        // The weapon's own behavior variation first, then the equipped combat art's
        // (Whirlwind Slash = weapon 5100, behaviorVariationId 5001: +1000).
        let Some(b) = [chr.behavior_base + j, chr.behavior_base + 1000 + j].iter().find_map(|id| beh.get(id)) else { continue };
        if b.get("refType").and_then(Value::as_i64) != Some(0) {
            continue;
        }
        let atk_id = b["refId"].as_i64().unwrap();
        if let Some(a) = atk.get(&atk_id) {
            let mut v = pick(a, ATK_FIELDS);
            v["atkParamId"] = json!(atk_id);
            v["behaviorName"] = b["name"].clone();
            attacks.insert(j.to_string(), v);
        }
    }
    // Combat arts: "a<group>:<judge>" -> the AtkParam of the art's variation.
    if !art_judges.is_empty() {
        let weapons = load_param(params, "EquipParamWeapon");
        // Every art weapon (base 5100-6000, upgraded 6100 / 7000-7700) by its variation as
        // "v<variation>:<judge>"; "a<group>:<judge>" = the group's base art (lowest id).
        let mut arts: Vec<(i64, i64, i64)> = weapons
            .iter()
            .filter(|(id, _)| (5000..8000).contains(*id))
            .filter_map(|(id, w)| Some((*id, w.get("spAtkcategory")?.as_i64()?, w.get("behaviorVariationId")?.as_i64()?)))
            .filter(|&(_, cat, _)| cat > 0)
            .collect();
        arts.sort();
        for (group, j) in &art_judges {
            let mut base_done = false;
            for &(_, _, var) in arts.iter().filter(|(_, cat, _)| cat == group) {
                let id = chr.behavior_base + (var - 5000) * 1000 + j;
                let Some(b) = beh.get(&id) else { continue };
                if b.get("refType").and_then(Value::as_i64) != Some(0) {
                    continue;
                }
                let atk_id = b["refId"].as_i64().unwrap();
                if let Some(a) = atk.get(&atk_id) {
                    let mut v = pick(a, ATK_FIELDS);
                    v["atkParamId"] = json!(atk_id);
                    v["behaviorName"] = b["name"].clone();
                    if !base_done {
                        attacks.insert(format!("a{group}:{j}"), v.clone());
                        base_done = true;
                    }
                    attacks.insert(format!("v{var}:{j}"), v);
                }
            }
        }
    }
    for j in pc_judges {
        let Some(b) = beh.get(&j) else { continue };
        if b.get("refType").and_then(Value::as_i64) != Some(0) {
            continue;
        }
        let atk_id = b["refId"].as_i64().unwrap();
        if let Some(a) = atk.get(&atk_id) {
            let mut v = pick(a, ATK_FIELDS);
            v["atkParamId"] = json!(atk_id);
            v["behaviorName"] = b["name"].clone();
            attacks.insert(format!("pc{j}"), v);
        }
    }
    // Bullets (prosthetic throws): BehaviorParam refType 1 -> Bullet -> atkId_Bullet.
    let bullet_param = load_param(params, "Bullet");
    let mut bullets = Map::new();
    for (base, j) in bullet_judges {
        let Some(b) = beh.get(&(base + j)) else { continue };
        if b.get("refType").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        let Some(bl) = bullet_param.get(&b["refId"].as_i64().unwrap_or(-1)) else { continue };
        let mut v = pick(bl, BULLET_FIELDS);
        v["bulletId"] = b["refId"].clone();
        if let Some(a) = bl.get("atkId_Bullet").and_then(Value::as_i64).and_then(|id| atk.get(&id)) {
            v["attack"] = pick(a, ATK_FIELDS);
        }
        bullets.insert(j.to_string(), v);
    }
    write_anim_bin(root, chr.id, &clips);
    println!("{}: {} states, {} anims, {} spEffects, {} attacks", chr.id, states.len(), anims.len(), sp_effects.len(), attacks.len());
    // Hurtboxes: ragdoll body capsules from chrbnd/<id>.HKX (Havok model space, bind pose).
    let hurtboxes: Vec<Value> = std::fs::read(chr_dir.join(format!("{0}.chrbnd.d/{0}.HKX", chr.id)))
        .map(|d| hkx::ragdoll_capsules(&d))
        .unwrap_or_default()
        .into_iter()
        .map(|c| json!({ "bone": ragdoll_bone(&c.name), "a": c.a, "b": c.b, "r": c.radius }))
        .collect();
    // NPC TAE 700 twists: the shared graph's modifiers bound to this character's properties.
    let mut twists = Map::new();
    if chr.id != "c0000" {
        let (names, props) = std::fs::read(chr_dir.join(format!("{0}.behbnd.d/Characters_{0}.hkx", chr.id))).map(|d| hkx::character_properties(&d)).unwrap_or_default();
        let prop = |n: &str| names.iter().position(|x| x == n).and_then(|i| props.get(i)).copied().unwrap_or(0);
        if let (Ok(d), false) = (std::fs::read(chr_dir.join(format!("{}.behbnd.d/c9997.hkx", chr.id))), props.is_empty()) {
            // The graph picks the quadruped twists only when RefQuadrupedTwistEnable is set, the
            // humanoid ones when RefTwistEnable is (c1020 / c1010: 1 and 0).
            let mods = hkx::twist_modifiers_bound(&d, &props)
                .into_iter()
                .filter(|m| if m.0.contains("Quadruped") { prop("RefQuadrupedTwistEnable") != 0 } else { prop("RefTwistEnable") != 0 })
                .collect();
            twists = twist_json(mods);
        }
    }
    json!({ "states": states, "anims": anims, "spEffects": sp_effects, "attacks": attacks, "hurtboxes": hurtboxes, "bullets": bullets, "twists": twists })
}

fn twist_json(mods: Vec<(String, [f32; 4], Vec<(i16, i16, f32, f32, f32, f32)>)>) -> Map<String, Value> {
    let mut twists = Map::new();
    for (name, limits, params) in mods {
        let chains: Vec<Value> = params
            .iter()
            .map(|&(s, e, rate, new_gain, on, off)| json!({ "start": s, "end": e, "rate": rate, "newTargetGain": new_gain, "onGain": on, "offGain": off }))
            .collect();
        twists.insert(name, json!({ "up": limits[0], "down": limits[1], "right": limits[2], "left": limits[3], "chains": chains }));
    }
    twists
}

fn rows(params: &Path, name: &str, ids: &[i64]) -> Value {
    let p = load_param(params, name);
    Value::Object(ids.iter().filter_map(|id| p.get(id).map(|r| (id.to_string(), Value::Object(r.clone())))).collect())
}

/// Ragdoll body name -> animation bone: "Ragdoll_Ctrl_L_Thigh" -> "L_Thigh",
/// "Ragdoll_Spine001/002/003" -> "Spine" / "Spine1" / "Spine2".
fn ragdoll_bone(name: &str) -> String {
    let n = name.trim_start_matches("Ragdoll_Ctrl_").trim_start_matches("Ragdoll_");
    let digits = n.len() - n.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 3 {
        let (base, num) = n.split_at(n.len() - 3);
        let k: u32 = num.parse().unwrap_or(1);
        return if k <= 1 { base.to_string() } else { format!("{base}{}", k - 1) };
    }
    n.to_string()
}

fn all_rows(params: &Path, name: &str) -> Value {
    let p = load_param(params, name);
    Value::Object(p.iter().map(|(id, r)| (id.to_string(), Value::Object(r.clone()))).collect())
}

pub fn export(root: &Path, list_file: &Path, defs: &Path) {
    let tmpl = tae::load_template(&defs.join("TAE.Template.SDT.xml"));
    let params = root.join("json/params");
    let list = std::fs::read_to_string(list_file).expect("export list");
    let (mut states, mut extra) = (Vec::new(), Vec::new());
    for line in list.lines().map(|l| l.split('#').next().unwrap().trim()).filter(|l| !l.is_empty()) {
        if line.starts_with('a') && line.len() == 11 && line.as_bytes()[4] == b'_' {
            extra.push(line.to_string());
        } else {
            states.push(line.to_string());
        }
    }
    let player = export_character(root, &PLAYER, &states, &extra, &tmpl, &params);
    let enemy = export_character(root, &SAMURAI_GENERAL, &[], &[], &tmpl, &params);
    // Further enemies (select with config enemy.chr); "enemy" stays the Samurai General.
    let mut enemies = Map::new();
    enemies.insert(SAMURAI_GENERAL.id.to_string(), enemy.clone());
    if root.join("chr/c1010.anibnd.d").exists() {
        enemies.insert(OCHIMUSHA.id.to_string(), export_character(root, &OCHIMUSHA, &[], &[], &tmpl, &params));
    }
    // Camera shakes (TAE RumbleCam events): other/default.rumblebnd camera_NNN.hkx.
    let mut rumble = Map::new();
    if let Ok(rd) = std::fs::read_dir(root.join("other/default.rumblebnd.d")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_prefix("camera_").and_then(|n| n.strip_suffix(".hkx")).and_then(|n| n.parse::<i64>().ok()) else { continue };
            if let Some((duration, frames)) = std::fs::read(e.path()).ok().and_then(|d| hkx::rumble_cam(&d)) {
                rumble.insert(id.to_string(), json!({ "duration": duration, "frames": frames }));
            }
        }
    }
    // TAE 700 upper-body twist modifiers (Wolf's behaviour graph).
    let twists = std::fs::read(root.join("chr/c0000.behbnd.d/c0000.hkx")).map(|d| twist_json(hkx::twist_modifiers(&d))).unwrap_or_default();
    let out = json!({
        "rumble": rumble,
        "twists": twists,
        "source": "Generated from the local Sekiro install by sekiro-extract. Do not distribute.",
        "taeFps": 30,
        "player": player,
        "enemy": enemy,
        "enemies": enemies,
        "params": {
            "StaminaControlParam": rows(&params, "StaminaControlParam", &[0, 1000000, 1000100, 1000200]),
            "CalcCorrectGraph": rows(&params, "CalcCorrectGraph", &[50, 51, 500, 501, 504]),
            // Samurai General variants (outfit / haori by modelDispMask) and the Ochimusha.
            "NpcParam": rows(&params, "NpcParam", &[10203010, 10200000, 10200010, 10202000, 10202070, 10219000, 10100000]),
            "GameSystemParam": rows(&params, "GameSystemParam", &[0]),
            // Combat arts: base (5100-6000) and upgraded (6100, 7000-7700) virtual weapons; their
            // resident SpEffects carry the SP_EF_REF_WEP_SP_ATK_UNLOCK_* refs (280-287).
            "EquipParamWeapon": rows(&params, "EquipParamWeapon", &[5000, 5100, 5200, 5300, 5400, 5500, 5600, 5700, 5800, 5900, 6000, 6100, 7000, 7100, 7200, 7300, 7400, 7500, 7600, 7700, 70000]),
            // Where the sword parts hang: EquipParamWeapon 5000 absorpParamId -> right_0 (blade, body
            // dummy 20) / right_1 (scabbard, 147); right_2 149 is the TAE 715 override target.
            "WepAbsorpPosParam": rows(&params, "WepAbsorpPosParam", &[5000]),
            // Latent skills that change posture damage when guarding (SkillParam 280 "Flowing Water" ->
            // SpEffect 150420 deflect / 150421 guard: def<Attr>StaminaDmgRate).
            "SkillParam": rows(&params, "SkillParam", &[280]),
            // Hit sounds: row = the defender's material (NpcParam materialSe1/2, protector
            // defenseMaterial1/2), column = the attack's atkMaterial_forSe group / type / power.
            "HitEffectSeParam": all_rows(&params, "HitEffectSeParam"),
            "HitEffectSeJustGuardParam": all_rows(&params, "HitEffectSeJustGuardParam"),
            "SpEffectParam": rows(&params, "SpEffectParam", &[127000, 150420, 150421, 140000, 140100, 140101, 140200, 140201, 140300, 140400, 140500, 140501, 140600, 140601, 140700, 140701, 140800, 140801, 140900, 140901, 141000, 100286]),
            "EquipParamProtector": rows(&params, "EquipParamProtector", PLAYER_PROTECTORS),
            "LockCamParam": all_rows(&params, "LockCamParam"),
            "CameraParam": all_rows(&params, "CameraParam"),
            "CameraSetParam": all_rows(&params, "CameraSetParam"),
            "KnockBackParam": all_rows(&params, "KnockBackParam"),
            "ThrowParam": rows(&params, "ThrowParam", &{
                // PC -> c1010 (11010xxx) / c1020 (11020xxx) suffixes: 0000/0001 front, 0005/0006 front near,
                // 0010-0012 deflect break, 0110/0111 behind, 0120-0122 mikiri break, 0150/0151 plunge,
                // 0160/0161 kick-down, 0190 mikiri, 0200/0201 kick-jump over; 21020000 the General's grab.
                let mut ids = vec![21020000];
                for base in [11010000, 11020000] {
                    for s in [0, 1, 5, 6, 10, 11, 12, 20, 21, 30, 31, 110, 111, 120, 121, 122, 150, 151, 160, 161, 190, 200, 201] {
                        ids.push(base + s);
                    }
                }
                ids
            }),
            "ChrPhysicsVelocityChangeParam": all_rows(&params, "ChrPhysicsVelocityChangeParam"),
        },
    });
    let path = root.join("combat_data.json");
    std::fs::write(&path, serde_json::to_string(&out).unwrap()).unwrap();
    println!("wrote {} ({} KB)", path.display(), std::fs::metadata(&path).unwrap().len() / 1024);
}

/// extracted/anim_<chr>.bin, little endian:
///   "SHAN" u32 version=1
///   u32 bones; per bone: u16 name_len, name, i16 parent, f32 t[3] r[4] s[3]
///   u32 clips; per clip: u16 key_len, key, f32 frame_duration, u32 frames, u32 tracks,
///     i16 track_to_bone[tracks], then frames*tracks*(f32 t[3] r[4] s[3])
fn write_anim_bin(root: &Path, chr: &str, clips: &[(String, hkx::Clip)]) {
    let anibnd = root.join("chr").join(format!("{chr}.anibnd.d"));
    let skel_path = ["skeleton.hkx", "Skeleton.HKX", "Skeleton.hkx"].iter().map(|n| anibnd.join(n)).find(|p| p.exists());
    let Some(skel_path) = skel_path else {
        eprintln!("{chr}: no skeleton.hkx");
        return;
    };
    let skel = std::fs::read(skel_path).unwrap();
    let mut bones = hkx::skeleton(&skel, &hkx::Types::default());
    if bones.is_empty() {
        // NPC skeletons carry only a type-compendium reference.
        if let Some(c) = std::fs::read_dir(&anibnd).unwrap().flatten().find(|f| f.path().extension().is_some_and(|x| x == "compendium")) {
            bones = hkx::skeleton(&skel, &hkx::read_types(&std::fs::read(c.path()).unwrap()));
        }
    }
    let mut out: Vec<u8> = Vec::new();
    let qs = |out: &mut Vec<u8>, q: &crate::spline::Qs| {
        for v in q.t.iter().chain(q.r.iter()).chain(q.s.iter()) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    };
    out.extend_from_slice(b"SHAN");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(bones.len() as u32).to_le_bytes());
    for b in &bones {
        out.extend_from_slice(&(b.name.len() as u16).to_le_bytes());
        out.extend_from_slice(b.name.as_bytes());
        out.extend_from_slice(&b.parent.to_le_bytes());
        qs(&mut out, &b.pose);
    }
    out.extend_from_slice(&(clips.len() as u32).to_le_bytes());
    for (key, c) in clips {
        out.extend_from_slice(&(key.len() as u16).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&c.frame_duration.to_le_bytes());
        out.extend_from_slice(&(c.frames as u32).to_le_bytes());
        out.extend_from_slice(&(c.track_to_bone.len() as u32).to_le_bytes());
        for t in &c.track_to_bone {
            out.extend_from_slice(&t.to_le_bytes());
        }
        for frame in &c.samples {
            for (i, q) in frame.iter().enumerate() {
                if i < c.track_to_bone.len() {
                    qs(&mut out, q);
                }
            }
        }
    }
    let path = root.join(format!("anim_{chr}.bin"));
    std::fs::write(&path, &out).unwrap();
    println!("{chr}: {} bones, {} clips -> {} ({} KB)", bones.len(), clips.len(), path.display(), out.len() / 1024);
}
