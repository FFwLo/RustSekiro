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
    id: String,
    /// Whose anibnd / behbnd the character uses: NpcParam normalChangeAnimChrId (c1021, the spear
    /// General, plays c1020's; c5430 Isshin Ashina c5400's), else its own id.
    anim_id: String,
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
    /// NPCs: every NpcParam row of the character (resident SpEffects of all of them) and the
    /// behaviorVariationIds they use; attacks / bullets are also keyed "v<variation>:<judge>"
    /// (BehaviorParam 200000000 + variation * 1000 + judge) so any row can be fought.
    npc_rows: Vec<i64>,
    variations: Vec<i64>,
}

fn player() -> Character {
    Character {
        id: "c0000".into(),
        anim_id: "c0000".into(),
        behavior_base: 100_000_000 + 5000 * 1000,
        behavior_param: "BehaviorParam_PC",
        atk_param: "AtkParam_Pc",
        weapon_category: 50,
        sub_weapon_category: 70,
        sub_behavior_base: 100_000_000 + 7000 * 1000,
        npc_row: None,
        npc_rows: Vec::new(),
        variations: Vec::new(),
    }
}

/// Wolf's default outfit (head, body, arms, legs), as used for the model export.
pub const PLAYER_PROTECTORS: &[i64] = &[100000, 101000, 102000, 103000];

/// An enemy: its main NpcParam row picks the behavior variation (NpcParam behaviorVariationId).
fn npc(id: &str, npc_row: i64, npc_rows: &[i64], params: &Path) -> Character {
    let npc = load_param(params, "NpcParam");
    let var = |row: i64| npc.get(&row).and_then(|r| r.get("behaviorVariationId")).and_then(Value::as_i64).unwrap_or(0);
    let mut variations: Vec<i64> = npc_rows.iter().map(|&r| var(r)).filter(|&v| v > 0).collect();
    variations.sort();
    variations.dedup();
    let anim_chr = npc.get(&npc_row).and_then(|r| r.get("normalChangeAnimChrId")).and_then(Value::as_i64).filter(|&c| c > 0);
    Character {
        id: id.into(),
        anim_id: anim_chr.map_or(id.to_string(), |c| format!("c{c:04}")),
        behavior_base: 200_000_000 + var(npc_row) * 1000,
        behavior_param: "BehaviorParam",
        atk_param: "AtkParam_Npc",
        weapon_category: 0,
        npc_row: Some(npc_row),
        sub_weapon_category: 0,
        sub_behavior_base: 0,
        npc_rows: npc_rows.to_vec(),
        variations,
    }
}

const SP_EFFECT_FIELDS: &[&str] = &[
    "name", "behaviorRefId", "stateInfo", "defStaminaAttackRate", "defFlickPower", "staminaAttackRate",
    "staminaRecoverChangeSpeed", "staminaRecoverSpeedRate", "guardDefFlickPowerRate", "flickDamageCutRate",
    "guardStaminaCutRate", "toughnessDamageCutRate", "effectEndurance", "conditionHp", "spCategory",
    // Within one spCategory the lower categoryPriority wins (Paramdex SpEffectParam meta).
    "categoryPriority",
    "changeHpRate", "changeHpPoint", "changeHpEstusFlaskRate",
    // Stealth: how a chr wearing the effect is seen and heard by NPCs (the exe's
    // SprjTargetingSystem, FUN_140bfe8a0 / FUN_140bfc050 / FUN_140bfea80...).
    "sightSearchEnemyCut", "aroundSightPointAddRate", "hearingSearchEnemyRate", "sightCutLimitType",
    "sightSearchLeftAngleCut", "sightSearchRightAngleCut", "sightSearchUpperAngleCut", "sightSearchBottomAngleCut",
    // Status build-up (Paramdex SDT meta: poizonAttackPower = poison, stateInfo 2; registBlood =
    // burn, stateInfo 6), its tick and the effects it turns into.
    "motionInterval", "poizonAttackPower", "registBlood", "replaceSpEffectId", "cycleOccurrenceSpEffectId",
    "vfxId", "vfxId1", "iconId", "registPoizonChangeRate", "registBloodChangeRate",
    // Deathblow recovery skills (105050, 150301-150331): posture refill and the state-change trigger.
    "changeStaminaRate", "changeStaminaPoint", "invocationConditionsStateChange1",
    "invocationConditionsStateChange2", "invocationConditionsStateChange3",
];

/// Every Bullet field a prosthetic tool's bullet (70xxxx-79xxxx) sets.
const BULLET_FIELDS: &[&str] = &[
    "initVellocity", "maxVellocity", "minVellocity", "accelInRange", "accelOutRange", "accelTime", "gravityInRange", "gravityOutRange", "dist",
    "life", "lifeRandomRange", "hitRadius", "hitRadiusMax", "spreadTime", "lockShootLimitAng", "atkId_Bullet",
    "numShoot", "shootInterval", "shootAngle", "shootAngleInterval", "shootAngleXZ", "shootAngleXInterval", "shootAngleXMaxRandom",
    "shootAngleYMaxRandom", "shootYOffsetRange", "targetYOffsetRange", "EmittePosType", "FollowType", "attachEffectType",
    "HitBulletID", "intervalCreateBulletId", "intervalCreateTimeMin", "intervalCreateTimeMax", "intervalCreateWaitTime",
    "isPenetrate", "isPenetrateMap", "isUseSharedHitList", "dmgHitRecordLifeTime", "isCheckWall_byCenterRay", "launchConditionType",
    "spEffectIDForShooter", "spEffectId0", "spEffectId1", "spEffectId2", "spEffectId3", "spEffectId4",
    "isEnableAutoHoming", "homingAngle", "homingBeginDist", "hormingStopRange", "predictionShootObserveTime",
    "isInheritSpeedToChild", "isInheritSfxToChild", "isHitForceMagic", "isHitOtherBulletForceEraseB", "isHitDarkForceMagic", "guardRangeType",
    "atkAttribute", "spAttribute", "staminaPhysicsAttribute", "Material_AttackType", "Material_AttackMaterial_forSe",
    "Material_AttackMaterial_forSfx", "sfxId_Bullet", "sfxId_Hit", "sfxId_Flick", "sfxId_ForceErase", "isAttackSFX", "isSendShootInterrupt",
];

const ATK_FIELDS: &[&str] = &[
    "name", "atkPhys", "atkStam", "atkStamCorrection", "atkSuperArmor", "guardAtkRate", "guardBreakRate",
    "guardStaminaCutRate", "hit0_Radius", "hit1_Radius", "hit0_DmyPoly1", "hit0_DmyPoly2", "atkAttribute",
    "spAttribute", "atkType", "guardAttribute", "staminaPhysicsAttribute", "atkMaterial_forSe", "atkPow_forSe", "defSeMaterial1", "defSeMaterial2",
    // Hit sparks (HitEffectSfxParam via the Concept / ConceptJustGuard tables), like the *_forSe fields.
    "atkMaterial_forSfx", "atkPow_forSfx", "defSfxMaterial1", "defSfxMaterial2", "deflectAction", "deflectedAction", "justDeflectAction",
    "justDeflectedAction", "isDisableParry", "disableGuard_vsGuardAttribute0", "disableJustGuard_vsGuardAttribute0",
    "directAtkStamDamage", "directAtkStamDamage_Attacker", "repelLostStamDamage", "repelLostStamDamage_Attacker",
    "repelVictoryStamDamage_Attacker", "staminaDamageAttackHitParry", "knockbackDist_Guard", "knockbackDist_JustGuard",
    "atkFlickPower", "flickPower", "atkPhysCorrection", "atkStamCorrection", "directAtkStamCorrection",
    "guardAtkRateCorrection", "guardStaminaCutRate", "hitStopTime", "hitStopTime_Defencer", "dmgLevel", "dmgLevel_vsPlayer",
    "throwFlag", "knockbackDist_DirectHit", "atkMagCorrection", "atkFireCorrection", "atkThunCorrection",
    "atkDarkCorrection", "spEffectId0", "spEffectId1", "spEffectId2", "spEffectId3", "spEffectId4", "opposeTarget", "friendlyTarget", "atkMag", "atkFire", "atkThun", "atkDark", "throwTypeId",
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
        // Player TAEs are split per group ("a50.tae" = a050_*); an NPC has one "cXXXX.tae" whose
        // ids carry the group: 100003001 = a100_003001 (the "Anime ID offset" sets, SpEffect
        // 200030-200034 stateInfo 270-274 -> a000-a400).
        let stem = p.file_stem().unwrap().to_string_lossy();
        let group: i64 = stem.strip_prefix('a').and_then(|n| n.parse().ok()).unwrap_or(0);
        for a in t["anims"].as_array().unwrap() {
            let id = a["id"].as_i64().unwrap();
            let (g, n) = if id >= 1_000_000 { (id / 1_000_000, id % 1_000_000) } else { (group, id) };
            out.insert(format!("a{g:03}_{n:06}"), a.clone());
        }
    }
    out
}

/// Finds "<key>.hkx" in any "<chr>_*.anibnd.d" folder, with that folder's compendium.
/// Also returns the binder folder name ("c0000_c1040.anibnd.d": Wolf's clips against one enemy).
fn find_hkx(chr_dir: &Path, chr: &str, key: &str, compendiums: &mut HashMap<PathBuf, hkx::Types>) -> Option<(Vec<u8>, hkx::Types, String)> {
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
        return Some((std::fs::read(file).ok()?, types, name));
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
    let chr_id = chr.anim_id.as_str();
    let taes = load_taes(&chr_dir.join(format!("{chr_id}.anibnd.d")), tmpl);
    // The character's own behavior plus the shared NPC behavior (c9997.hkx) shipped in NPC behbnds.
    let mut cmsg = HashMap::new();
    for file in ["c9997".to_string(), chr.anim_id.clone()] {
        let path = chr_dir.join(format!("{chr_id}.behbnd.d/{file}.hkx"));
        if let Ok(d) = std::fs::read(&path) {
            cmsg.extend(hkx::cmsg_map(&d));
        }
    }

    let mut states = BTreeMap::new();
    let mut keys: BTreeSet<String> = extra_anims.iter().cloned().collect();
    // Prosthetic tools: every offsetType 14 state (W_GroundSubAttack*, W_SubAttackJump*, ...) and
    // every anim of the ten tool groups a070..a079 (wepmotionCategory 70..79); the game swaps the
    // group for the equipped tool, like the combat arts' a100..a110.
    let mut state_names = state_names.to_vec();
    if chr.sub_weapon_category > 0 && !state_names.is_empty() {
        // ...and the combat arts' no-Spirit-Emblem versions (offsetType 13 "*NoResource":
        // GroundSpecialAttackCombo1NoResource 316001 -> a103_316001 / a104_316001).
        let mut sub: Vec<String> = cmsg
            .iter()
            .filter(|(k, (anim, ot))| *anim > 0 && (*ot == 14 || (*ot == 13 && k.ends_with("NoResource"))))
            .map(|(k, _)| k.clone())
            .collect();
        sub.sort();
        for k in sub {
            if !state_names.contains(&k) {
                state_names.push(k);
            }
        }
        keys.extend(taes.keys().filter(|k| k.get(1..4).and_then(|g| g.parse::<u32>().ok()).is_some_and(|g| (70..=79).contains(&g))).cloned());
    }
    let state_names = &state_names[..];
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
    // (output file, anim key, clip): output "" = anim_<chr>.bin.
    let mut clips: Vec<(String, String, hkx::Clip)> = Vec::new();
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
    let mut npc_bullet_judges: BTreeSet<i64> = BTreeSet::new();
    // Prosthetic tool melee (TAE 1 in a07x: the Axe, Sabimaru, Spear) per tool variation.
    let mut tool_judges: BTreeSet<(i64, i64)> = BTreeSet::new();
    // (weapon id, wepmotionCategory, behaviorVariationId) of every prosthetic tool level.
    let weapons = load_param(params, "EquipParamWeapon");
    let mut tools: Vec<(i64, i64, i64)> = weapons
        .iter()
        .filter(|(id, _)| (70000..80000).contains(*id))
        .filter_map(|(id, w)| Some((*id, w.get("wepmotionCategory")?.as_i64()?, w.get("behaviorVariationId")?.as_i64()?)))
        .filter(|&(_, g, _)| (70..=79).contains(&g))
        .collect();
    tools.sort();
    if chr.sub_behavior_base == 0 {
        tools.clear();
    }
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
            let tool_group = key.get(1..4).and_then(|g| g.parse::<i64>().ok()).filter(|g| chr.sub_behavior_base > 0 && (70..=79).contains(g));
            if let Some(g) = tool_group {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    for &(_, _, var) in tools.iter().filter(|t| t.1 == g) {
                        if e["type"] == 2 {
                            bullet_judges.insert((var, j));
                        } else if e["type"] == 1 {
                            tool_judges.insert((var, j));
                        }
                    }
                }
            }
            // Wolf's sword bullets (TAE 2 / 4 BulletBehavior(_Midair) outside the tool groups: the
            // Lightning Reversal's bolt, a050_308900 judge 184 -> BehaviorParam_PC 105000184 ->
            // Bullet 500184), keyed "v5000:<judge>" at the sword's variation.
            if tool_group.is_none() && chr.npc_row.is_none() && (e["type"] == 2 || e["type"] == 4) {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    bullet_judges.insert(((chr.behavior_base - 100_000_000) / 1000, j));
                }
            }
            if e["type"] == 307 && chr.npc_row.is_none() && e["args"].get("GetWeaponData").and_then(Value::as_i64) == Some(0) {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    pc_judges.insert(j);
                }
            }
            // NPC BulletBehavior (TAE 2): arrows, guns, shuriken, fire breath -> BehaviorParam refType 1.
            if e["type"] == 2 && chr.npc_row.is_some() {
                if let Some(j) = e["args"].get("BehaviorJudgeID").and_then(Value::as_i64) {
                    npc_bullet_judges.insert(j);
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
            .map(|src| if chr.npc_row.is_some() { format!("a{:03}_{:06}", src / 1_000_000, src % 1_000_000) } else { format!("{}_{:06}", &key[..4], src % 1_000_000) });
        // ImportOtherAnim (TAE mini header 1) brings the HKX along with the events: e.g. the
        // broken-enemy deathblows a201_511200 / 511400 play a201_510200 / 510300's clips.
        let found = find_hkx(&chr_dir, chr_id, key, &mut compendiums)
            .or_else(|| hkx_key.as_deref().and_then(|k| find_hkx(&chr_dir, chr_id, k, &mut compendiums)))
            .or_else(|| if src != *key { find_hkx(&chr_dir, chr_id, &src, &mut compendiums) } else { None });
        if let Some(k) = hkx_key.as_deref().filter(|_| found.is_some()) {
            a.insert("hkxFrom".into(), json!(k));
        }
        if let Some((data, types, folder)) = found {
            if let Some(c) = hkx::clip(&data, &types) {
                // Wolf's clips against one enemy (chr/c0000_c<chr>.anibnd, loaded by the exe per
                // enemy) go to anim_c0000_c<chr>.bin; the General's stay in the main file.
                let split = folder.strip_prefix("c0000_c").and_then(|f| f.strip_suffix(".anibnd.d")).filter(|n| *n != "1020").map(|n| format!("c0000_c{n}"));
                clips.push((split.unwrap_or_default(), key.clone(), c));
            }
            let (dur, samples) = hkx::root_motion(&data, &types);
            if let Some(d) = dur {
                a.insert("duration".into(), json!(d));
                if samples.len() > 1 {
                    a.insert("rootRate".into(), json!((samples.len() - 1) as f32 / d));
                    // Havok space -> game space: x is mirrored, forward stays -Z.
                    // Game space mirrors X, which also mirrors turns: yaw -> -yaw (live: the game's
                    // physics yaw y maps to ours as pi - y). Kept raw, root turns went the wrong way;
                    // where the skeleton counter-turns (c1020 ThrowDefDeath 13411 / 13511 / 12311:
                    // body -180, root +180 in the game's space, net ~0) the two added up to a 360 spin.
                    let root: Vec<Value> = samples.iter().map(|s| json!([-s[0], s[1], s[2], -s[3]])).collect();
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
    for npc_row in chr.npc_row.iter().chain(chr.npc_rows.iter()) {
        let npc = load_param(params, "NpcParam");
        if let Some(r) = npc.get(npc_row) {
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
    for &j in &judge_ids {
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
    // NPCs: the same judges in every behavior variation the character's rows use.
    for &var in &chr.variations {
        for &j in &judge_ids {
            let Some(b) = beh.get(&(200_000_000 + var * 1000 + j)) else { continue };
            if b.get("refType").and_then(Value::as_i64) != Some(0) {
                continue;
            }
            let atk_id = b["refId"].as_i64().unwrap();
            if let Some(a) = atk.get(&atk_id) {
                let mut v = pick(a, ATK_FIELDS);
                v["atkParamId"] = json!(atk_id);
                v["behaviorName"] = b["name"].clone();
                attacks.insert(format!("v{var}:{j}"), v);
            }
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
    // Bullets (prosthetic throws): BehaviorParam refType 1 -> Bullet -> atkId_Bullet, keyed
    // "v<variation>:<judge>" (BehaviorParam_PC 100000000 + variation * 1000 + judge); the Shuriken
    // LV1's (variation 7000) also by the bare judge. Each bullet's HitBulletID chain (the
    // firecracker's sparks, the flame vent's fire) is kept in "bulletRows" by Bullet id.
    let bullet_param = load_param(params, "Bullet");
    let mut bullets = Map::new();
    let mut bullet_rows: Map<String, Value> = Map::new();
    let mut extra_sp: BTreeSet<i64> = BTreeSet::new();
    // (BehaviorParam row, keys): the player's tool bullets per tool variation, an NPC's per
    // behavior variation (bare judge = the main row's variation).
    let mut bullet_sources: Vec<(i64, Vec<String>)> = bullet_judges
        .iter()
        .map(|&(var, j)| (100_000_000 + var * 1000 + j, if var == 7000 { vec![j.to_string(), format!("v{var}:{j}")] } else { vec![format!("v{var}:{j}")] }))
        .collect();
    for &j in &npc_bullet_judges {
        bullet_sources.push((chr.behavior_base + j, vec![j.to_string()]));
        for &var in &chr.variations {
            bullet_sources.push((200_000_000 + var * 1000 + j, vec![format!("v{var}:{j}")]));
        }
    }
    for (row, keys) in &bullet_sources {
        let Some(b) = beh.get(row) else { continue };
        if b.get("refType").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        let first = b["refId"].as_i64().unwrap_or(-1);
        let mut next = Some(first);
        let mut pending: Vec<i64> = Vec::new();
        while let Some(bid) = next.take().filter(|id| *id > 0 && !bullet_rows.contains_key(&id.to_string())) {
            let Some(bl) = bullet_param.get(&bid) else { break };
            let mut v = pick(bl, BULLET_FIELDS);
            v["bulletId"] = json!(bid);
            if let Some(aid) = bl.get("atkId_Bullet").and_then(Value::as_i64) {
                if let Some(a) = atk.get(&aid) {
                    let mut av = pick(a, ATK_FIELDS);
                    av["atkParamId"] = json!(aid);
                    for k in ["spEffectId0", "spEffectId1", "spEffectId2", "spEffectId3", "spEffectId4"] {
                        if let Some(sid) = a.get(k).and_then(Value::as_i64).filter(|v| *v > 0) {
                            av[k] = json!(sid);
                            extra_sp.insert(sid);
                        }
                    }
                    v["attack"] = av;
                }
            }
            for k in ["spEffectIDForShooter", "spEffectId0", "spEffectId1", "spEffectId2", "spEffectId3", "spEffectId4"] {
                if let Some(sid) = bl.get(k).and_then(Value::as_i64).filter(|v| *v > 0) {
                    extra_sp.insert(sid);
                }
            }
            next = bl.get("HitBulletID").and_then(Value::as_i64).filter(|v| *v > 0);
            let interval = bl.get("intervalCreateBulletId").and_then(Value::as_i64).filter(|v| *v > 0);
            bullet_rows.insert(bid.to_string(), v);
            if next.is_none() || next.is_some_and(|n| bullet_rows.contains_key(&n.to_string())) {
                next = interval;
            } else if let Some(i) = interval.filter(|i| !bullet_rows.contains_key(&i.to_string())) {
                pending.push(i);
            }
            if next.is_none() {
                next = pending.pop();
            }
        }
        let Some(v) = bullet_rows.get(&first.to_string()).cloned() else { continue };
        for k in keys {
            bullets.insert(k.clone(), v.clone());
        }
    }
    for (var, j) in &tool_judges {
        let Some(b) = beh.get(&(100_000_000 + var * 1000 + j)) else { continue };
        if b.get("refType").and_then(Value::as_i64) != Some(0) {
            continue;
        }
        let atk_id = b["refId"].as_i64().unwrap();
        if let Some(a) = atk.get(&atk_id) {
            let mut v = pick(a, ATK_FIELDS);
            v["atkParamId"] = json!(atk_id);
            v["behaviorName"] = b["name"].clone();
            attacks.insert(format!("v{var}:{j}"), v);
        }
    }
    // The tools themselves: every level, its anim group, variation, Spirit Emblem cost
    // (resourceItemA), resident SpEffect, menu icon and model.
    let prosthetics: Vec<Value> = tools
        .iter()
        .map(|&(id, g, var)| {
            let w = &weapons[&id];
            let resident = w.get("residentSpEffectId").and_then(Value::as_i64).unwrap_or(-1);
            if resident > 0 {
                extra_sp.insert(resident);
            }
            json!({
                "id": id, "group": g, "variation": var, "name": w.get("name").cloned().unwrap_or_default(),
                "emblems": w.get("resourceItemA").cloned().unwrap_or(json!(0)), "resident": resident,
                "icon": w.get("iconId").cloned().unwrap_or(json!(-1)), "model": w.get("equipModelId").cloned().unwrap_or(json!(-1)),
                "attackBasePhysics": w.get("attackBasePhysics").cloned().unwrap_or(json!(0)),
                "attackBaseFire": w.get("attackBaseFire").cloned().unwrap_or(json!(0)),
            })
        })
        .collect();
    // Wolf's art and prosthetic behaviours (BehaviorParam_PC 105000000-107999999): wepCost (1 = the
    // behaviour costs the tool's Spirit Emblems) and the ref; refType 2 rows are SpEffects TAE 940
    // BehaviorParam_AddSpEffect puts on Wolf (Divine Abduction 107700: ref 309 USED_TEKIMAWASHI).
    let mut behaviors = Map::new();
    if chr.npc_row.is_none() {
        for (id, b) in beh.iter().filter(|(id, _)| (105_000_000..=107_999_999).contains(*id)) {
            let ref_type = b.get("refType").and_then(Value::as_i64).unwrap_or(-1);
            let ref_id = b.get("refId").and_then(Value::as_i64).unwrap_or(-1);
            behaviors.insert(id.to_string(), json!({ "wepCost": b.get("wepCost").cloned().unwrap_or(json!(0)), "refType": ref_type, "refId": ref_id }));
            if ref_type == 2 && ref_id > 0 {
                extra_sp.insert(ref_id);
            }
        }
        // Divine Abduction follow-ups and tool-state SpEffects the HKS / TAE check by id.
        extra_sp.extend([107715, 107716, 107717, 107718, 100260, 100277, 100290]);
        // Given by enemy bullets / held by the engine: the storm jump openings 106100 / 106101
        // (refs 110003 / 110004, the Divine Dragon's updraft bullets 52000660 / 53100640) and the
        // Lightning Reversal charges 9490 / 9495.
        extra_sp.extend([106100, 106101, 9490, 9495]);
    }
    // Every exported attack's on-hit SpEffects (Sabimaru AtkParam_Pc 7500100 -> 9004 poison).
    for a in attacks.values().chain(bullet_rows.values().filter_map(|b| b.get("attack"))) {
        let Some(row) = a.get("atkParamId").and_then(Value::as_i64).and_then(|id| atk.get(&id)) else { continue };
        for k in ["spEffectId0", "spEffectId1", "spEffectId2", "spEffectId3", "spEffectId4"] {
            if let Some(sid) = row.get(k).and_then(Value::as_i64).filter(|v| *v > 0) {
                extra_sp.insert(sid);
            }
        }
    }
    // ...and what those turn into (replaceSpEffectId) or tick (cycleOccurrenceSpEffectId):
    // 9004 -> 9045 poison damage.
    let mut todo: Vec<i64> = extra_sp.iter().copied().chain(sp_effects.keys().filter_map(|k| k.parse().ok())).collect();
    while let Some(id) = todo.pop() {
        let Some(r) = sp.get(&id) else { continue };
        for k in ["replaceSpEffectId", "cycleOccurrenceSpEffectId"] {
            if let Some(n) = r.get(k).and_then(Value::as_i64).filter(|v| *v > 0) {
                if extra_sp.insert(n) {
                    todo.push(n);
                }
            }
        }
    }
    let mut sp_effects = sp_effects;
    for id in extra_sp {
        if let Some(r) = sp.get(&id) {
            sp_effects.insert(id.to_string(), pick(r, SP_EFFECT_FIELDS));
        }
    }
    let mut files: BTreeMap<String, Vec<(String, hkx::Clip)>> = BTreeMap::new();
    for (file, key, c) in clips {
        files.entry(if file.is_empty() { chr_id.to_string() } else { file }).or_default().push((key, c));
    }
    for (file, clips) in &files {
        write_anim_bin(root, chr_id, file, clips);
    }
    println!("{}: {} states, {} anims, {} spEffects, {} attacks", chr.id, states.len(), anims.len(), sp_effects.len(), attacks.len());
    // Hurtboxes: ragdoll body capsules from chrbnd/<id>.HKX (Havok model space, bind pose).
    let hurtboxes: Vec<Value> = std::fs::read(chr_dir.join(format!("{0}.chrbnd.d/{0}.HKX", chr.id)))
        // A ragdoll with shape types the reader lacks (c7021: hknpCapsuleShape outside the
        // compendium) is skipped rather than ending the whole export.
        .ok()
        .and_then(|d| std::panic::catch_unwind(|| hkx::ragdoll_capsules(&d)).map_err(|_| eprintln!("{}: ragdoll not readable, no hurtboxes", chr.id)).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|c| json!({ "bone": ragdoll_bone(&c.name), "a": c.a, "b": c.b, "r": c.radius }))
        .collect();
    // Their NPC part groups (HKS env(1120) GetPartGroup -> W_PartBlend_Add0N, the Divine
    // Dragon's 1 right arm / 2 head / 3 body / 4-5 neck): chrbnd/<id>.hkxpwv, one 16-byte entry
    // per ragdoll body after the bone map (4 bytes each) and the anim bones (8): RagdollParam id
    // (i32), then the part group (u8) (SoulsFormats HKXPWV RagdollBoneEntry, whose DS3 layout
    // reads an i16 there; Sekiro's groups 0-5 sit in the first byte, matching the PartBlend anims).
    let mut hurtboxes = hurtboxes;
    if let Ok(d) = std::fs::read(chr_dir.join(format!("{0}.chrbnd.d/{0}.hkxpwv", chr.id))) {
        let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
        if let (Some(maps), Some(anims), Some(rags)) = (u16_at(6), u16_at(8), u16_at(10)) {
            let base = 0x20 + maps * 4 + anims * 8;
            if rags == hurtboxes.len() && d.len() == base + rags * 16 {
                for (i, h) in hurtboxes.iter_mut().enumerate() {
                    let part = d[base + i * 16 + 4];
                    if part > 0 {
                        h["part"] = json!(part);
                    }
                }
            }
        }
    }
    // NPC TAE 700 twists: the shared graph's modifiers bound to this character's properties.
    let mut twists = Map::new();
    if chr.id != "c0000" {
        let (names, props) = std::fs::read(chr_dir.join(format!("{0}.behbnd.d/Characters_{0}.hkx", chr_id))).map(|d| hkx::character_properties(&d)).unwrap_or_default();
        let prop = |n: &str| names.iter().position(|x| x == n).and_then(|i| props.get(i)).copied().unwrap_or(0);
        if let (Ok(d), false) = (std::fs::read(chr_dir.join(format!("{chr_id}.behbnd.d/c9997.hkx"))), props.is_empty()) {
            // The graph picks the quadruped twists only when RefQuadrupedTwistEnable is set, the
            // humanoid ones when RefTwistEnable is (c1020 / c1010: 1 and 0).
            let mods = hkx::twist_modifiers_bound(&d, &props)
                .into_iter()
                .filter(|m| if m.0.contains("Quadruped") { prop("RefQuadrupedTwistEnable") != 0 } else { prop("RefTwistEnable") != 0 })
                .collect();
            twists = twist_json(mods);
        }
    }
    json!({ "states": states, "anims": anims, "spEffects": sp_effects, "attacks": attacks, "hurtboxes": hurtboxes, "bullets": bullets, "bulletRows": bullet_rows, "prosthetics": prosthetics, "twists": twists, "behaviors": behaviors })
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
    // Wolf's side of every deathblow / throw (ThrowParam rows with AtkChrId 0): group
    // atkAnimOffset (a200 generic, a201 the General, a215 the large Centipede, a242 Isshin...),
    // clips in chr/c0000_c<chr>.anibnd (the exe loads "chranibnd:/c0000_c%04d.anibnd" per enemy).
    let throw_rows = load_param(&params, "ThrowParam");
    let mut throw_anims: BTreeSet<String> = throw_rows
        .values()
        .filter(|r| r.get("AtkChrId").and_then(Value::as_i64) == Some(0))
        .filter_map(|r| Some((r.get("atkAnimOffset")?.as_i64()?, r.get("atkAnimId")?.as_i64()?)))
        .filter(|&(g, a)| g > 0 && a > 0)
        .map(|(g, a)| format!("a{g:03}_{a:06}"))
        .collect();
    // ... and Wolf's side of every enemy grab (DefChrId 0: defAnimOffset group, e.g. the General's
    // 21020000 -> a210_600000).
    throw_anims.extend(
        throw_rows
            .values()
            .filter(|r| r.get("DefChrId").and_then(Value::as_i64) == Some(0) && r.get("AtkChrId").and_then(Value::as_i64).is_some_and(|c| c > 0))
            .filter_map(|r| Some((r.get("defAnimOffset")?.as_i64()?, r.get("defAnimId")?.as_i64()?)))
            .filter(|&(g, a)| g > 0 && a > 0)
            .map(|(g, a)| format!("a{g:03}_{a:06}")),
    );
    throw_anims.retain(|k| !extra.contains(k));
    extra.extend(throw_anims);
    let player = export_character(root, &player(), &states, &extra, &tmpl, &params);
    let general = npc("c1020", 10203010, &[10203010], &params);
    let enemy = export_character(root, &general, &[], &[], &tmpl, &params);
    // Further enemies (select with config enemy.chr); "enemy" stays the Samurai General.
    let mut enemies = Map::new();
    enemies.insert(general.id.clone(), enemy.clone());
    if root.join("chr/c1010.anibnd.d").exists() {
        let ochimusha = npc("c1010", 10100000, &[10100000], &params);
        enemies.insert(ochimusha.id.clone(), export_character(root, &ochimusha, &[], &[], &tmpl, &params));
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
    let decals = export_decals(root, &params, &[&player, &Value::Object(enemies.clone())]);
    // Official English names (msg/engus/item.msgbnd 武器名 = weapon names: the combat arts 5100-7700,
    // the prosthetic tools 70000-79200 and the skill entries SkillParam.virtualWeaponId point at).
    let weapon_names: Map<String, Value> = std::fs::read(root.join("msg/engus/item.msgbnd.d/武器名.fmg"))
        .map(|d| crate::fmg::read(&d))
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, _)| (5000..8000).contains(id) || (70000..80000).contains(id) || (200000..800000).contains(id))
        .map(|(id, n)| (id.to_string(), json!(n)))
        .collect();
    // Every skill (SkillParam): its tree page, the art / tool it unlocks and its SpEffects.
    let skills = all_rows(&params, "SkillParam");
    let skill_sp: Vec<i64> = skills
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
        .flat_map(|r| ["spEffect1", "spEffect2", "spEffect3"].map(|k| r[k].as_i64().unwrap_or(-1)))
        .filter(|v| *v > 0)
        .collect();
    // ...plus their accumuOverFireId chains (Knowledge of Medicine 150200 -> 150201 -> ... -> 150204).
    let mut skill_sp = skill_sp;
    {
        let sp = load_param(&params, "SpEffectParam");
        let mut i = 0;
        while i < skill_sp.len() {
            let next = sp.get(&skill_sp[i]).and_then(|r| r.get("accumuOverFireId")).and_then(Value::as_i64).filter(|v| *v > 0);
            if let Some(n) = next.filter(|n| !skill_sp.contains(n)) {
                skill_sp.push(n);
            }
            i += 1;
        }
    }
    // HUD item icons (MENU_ItemIcon_<iconId:05>) of the combat arts (5000-7999) and prosthetic
    // tools (70000-79999) for the equipped-item slots.
    let icons: BTreeSet<String> = load_param(&params, "EquipParamWeapon")
        .iter()
        .filter(|(id, _)| (5000..8000).contains(*id) || (70000..80000).contains(*id))
        .filter_map(|(_, r)| r.get("iconId").and_then(Value::as_i64).filter(|i| *i > 0))
        .map(|i| format!("MENU_ItemIcon_{i:05}"))
        .collect();
    export_hud(root, &icons);
    // The combat arts' "art has emblems" gates: resident R -> R / 100 * 100 + 10 when that row
    // exists (140310 stateInfo 995, 140410 990, 140510 994, 140610 993; TAE events are gated by
    // them). gap: the exe applies them; the link to the resident is inferred from ids and names.
    let art_gates: Vec<i64> = {
        let sp = load_param(&params, "SpEffectParam");
        let mut v: Vec<i64> = load_param(&params, "EquipParamWeapon")
            .iter()
            .filter(|(id, _)| (5000..8000).contains(*id))
            .filter_map(|(_, r)| r.get("residentSpEffectId").and_then(Value::as_i64).filter(|r| *r > 0))
            .map(|r| r / 100 * 100 + 10)
            .filter(|g| sp.contains_key(g))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    // The virtual weapons the skills point at (SkillParam.virtualWeaponId: 200000-200600,
    // 301000-301300): their actionUnlockParamId -> ActionUnlockParam action<n>, which the exe's
    // env(3033, ACTION_UNLOCK_TYPE_n) counts (FUN_1407a7fe0).
    let mut weapon_ids = vec![5000, 5100, 5200, 5300, 5400, 5500, 5600, 5700, 5800, 5900, 6000, 6100, 7000, 7100, 7200, 7300, 7400, 7500, 7600, 7700, 70000];
    weapon_ids.extend(skills.as_object().into_iter().flat_map(|m| m.values()).filter_map(|r| r["virtualWeaponId"].as_i64()).filter(|v| *v > 0));
    weapon_ids.sort();
    weapon_ids.dedup();
    let out = json!({
        "names": { "weapon": weapon_names },
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
            // ...and the skills' virtual weapons (above).
            "EquipParamWeapon": rows(&params, "EquipParamWeapon", &weapon_ids),
            "ActionUnlockParam": all_rows(&params, "ActionUnlockParam"),
            // Where the sword parts hang: EquipParamWeapon 5000 absorpParamId -> right_0 (blade, body
            // dummy 20) / right_1 (scabbard, 147); right_2 149 is the TAE 715 override target.
            "WepAbsorpPosParam": rows(&params, "WepAbsorpPosParam", &[5000]),
            // Latent skills that change posture damage when guarding (SkillParam 280 "Flowing Water" ->
            // SpEffect 150420 deflect / 150421 guard: def<Attr>StaminaDmgRate).
            "SkillParam": skills,
            // Hit sounds: row = the defender's material (NpcParam materialSe1/2, protector
            // defenseMaterial1/2), column = the attack's atkMaterial_forSe group / type / power.
            "HitEffectSeParam": all_rows(&params, "HitEffectSeParam"),
            "HitEffectSeJustGuardParam": all_rows(&params, "HitEffectSeJustGuardParam"),
            // Hit / guard / deflect sparks: Concept(JustGuard) row = the defender's material
            // (NpcParam materialSfx1/2, protector defenseMaterialSfx1/2), its column the attack's
            // atkMaterial_forSfx -> HitEffectSfxParam row -> FXR per attack type and power.
            "HitEffectSfxParam": all_rows(&params, "HitEffectSfxParam"),
            "HitEffectSfxConceptParam": all_rows(&params, "HitEffectSfxConceptParam"),
            "HitEffectSfxConceptJustGuardParam": all_rows(&params, "HitEffectSfxConceptJustGuardParam"),
            "SpEffectParam": rows(&params, "SpEffectParam", &[skill_sp.as_slice(), &[127000, 150420, 150421, 140000, 140100, 140101, 140200, 140201, 140300, 140400, 140500, 140501, 140600, 140601, 140700, 140701, 140800, 140801, 140900, 140901, 141000, 100286], art_gates.as_slice()].concat()),
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
            // Stealth: the enemies' sight / hearing / forget rules and the AI sounds Wolf's TAE makes.
            "NpcThinkParam": rows(&params, "NpcThinkParam", &[10100000, 10200000]),
            "AiSoundParam": all_rows(&params, "AiSoundParam"),
            // Blood stains etc.: the rows the exported anims' TAE 137-139 DecalParamID events use.
            "DecalParam": decals,
        },
    });
    let path = root.join("combat_data.json");
    std::fs::write(&path, serde_json::to_string(&out).unwrap()).unwrap();
    println!("wrote {} ({} KB)", path.display(), std::fs::metadata(&path).unwrap().len() / 1024);
}

/// HUD sprites the game draws itself, cut from the menu atlases (menu/hi/01_common.tpf, sprite rects
/// from menu/hi/01_common.sblytbnd <atlas>.layout) to extracted/hud/<name>.png: the deathblow mark
/// (MENU_ninsatu_01 / _02 in SB_FE).
fn export_hud(root: &Path, icons: &BTreeSet<String>) {
    const WANTED: [&str; 2] = ["MENU_ninsatu_01", "MENU_ninsatu_02"];
    let Ok(tpf) = std::fs::read(root.join("menu/hi/01_common.tpf")) else {
        eprintln!("no menu/hi/01_common.tpf (sekiro-extract unpack <dir> extracted 'menu/hi/01_common'): HUD sprites skipped");
        return;
    };
    let tex: HashMap<String, Vec<u8>> = crate::flver::tpf(&tpf).into_iter().collect();
    let _ = std::fs::create_dir_all(root.join("hud"));
    let Ok(dir) = std::fs::read_dir(root.join("menu/hi/01_common.sblytbnd.d")) else { return };
    let attr = |line: &str, k: &str| -> Option<String> {
        let i = line.find(&format!(" {k}=\""))? + k.len() + 3;
        Some(line[i..].split('"').next()?.to_string())
    };
    for f in dir.flatten() {
        let Ok(text) = std::fs::read_to_string(f.path()) else { continue };
        let Some(atlas) = text.lines().find_map(|l| attr(l, "imagePath")).map(|p| p.trim_end_matches(".png").to_lowercase()) else { continue };
        let mut decoded = None;
        for line in text.lines() {
            let Some(name) = attr(line, "name").map(|n| n.trim_end_matches(".png").to_string()) else { continue };
            if !WANTED.contains(&name.as_str()) && !icons.contains(&name) {
                continue;
            }
            let Some(dds) = tex.get(&atlas) else { continue };
            let (w, _, px) = decoded.get_or_insert_with(|| crate::decode_dds(dds));
            let n = |k: &str| attr(line, k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let (x, y, sw, sh) = (n("x"), n("y"), n("width"), n("height"));
            let mut rgba = Vec::with_capacity(sw * sh * 4);
            for yy in y..y + sh {
                for xx in x..x + sw {
                    let p = px[yy * *w + xx];
                    rgba.extend_from_slice(&[(p >> 16 & 0xff) as u8, (p >> 8 & 0xff) as u8, (p & 0xff) as u8, (p >> 24) as u8]);
                }
            }
            crate::write_png(&root.join(format!("hud/{name}.png")), sw, sh, &rgba);
        }
    }
}

/// The DecalParam rows the exported anims' TAE 137 / 138 / 139 events name (blood pools under a
/// deathblow: 710011), and per row extracted/decal/<id>.png from other/decaltex.tpf: RGB the
/// diffuse texture dp<diffuseTextureId>_a, alpha the mask dp<maskTextureId>_m (its red channel,
/// the splat shape). The colour tint (diffuseColorR/G/B) stays in the row for the game to apply.
fn export_decals(root: &Path, params: &Path, chars: &[&Value]) -> Value {
    fn collect(v: &Value, ids: &mut BTreeSet<i64>) {
        match v {
            Value::Object(m) => {
                if matches!(m.get("type").and_then(Value::as_i64), Some(137..=139)) {
                    if let Some(id) = m.get("args").and_then(|a| a.get("DecalParamID")).and_then(Value::as_i64) {
                        ids.insert(id);
                    }
                }
                m.values().for_each(|c| collect(c, ids));
            }
            Value::Array(a) => a.iter().for_each(|c| collect(c, ids)),
            _ => {}
        }
    }
    let mut ids = BTreeSet::new();
    chars.iter().for_each(|c| collect(c, &mut ids));
    let ids: Vec<i64> = ids.into_iter().collect();
    let rows = rows(params, "DecalParam", &ids);
    let Ok(tpf) = std::fs::read(root.join("other/decaltex.tpf")) else {
        eprintln!("no other/decaltex.tpf (sekiro-extract unpack <dir> extracted decaltex): decal textures skipped");
        return rows;
    };
    let tex: HashMap<String, Vec<u8>> = crate::flver::tpf(&tpf).into_iter().collect();
    let _ = std::fs::create_dir_all(root.join("decal"));
    for (id, row) in rows.as_object().into_iter().flatten() {
        let get = |k: &str| row[k].as_i64().filter(|v| *v >= 0).and_then(|v| tex.get(&format!("dp{v:09}_{}", if k == "maskTextureId" { "m" } else { "a" })));
        let (Some(diffuse), Some(mask)) = (get("diffuseTextureId"), get("maskTextureId")) else { continue };
        let ((w, h, d), (mw, mh, m)) = (crate::decode_dds(diffuse), crate::decode_dds(mask));
        if (w, h) != (mw, mh) {
            eprintln!("decal {id}: diffuse {w}x{h} / mask {mw}x{mh} differ, skipped");
            continue;
        }
        let rgba: Vec<u8> = d.iter().zip(&m).flat_map(|(p, q)| [(p >> 16 & 0xff) as u8, (p >> 8 & 0xff) as u8, (p & 0xff) as u8, (q >> 16 & 0xff) as u8]).collect();
        crate::write_png(&root.join(format!("decal/{id}.png")), w, h, &rgba);
    }
    rows
}

/// extracted/anim_<chr>.bin, little endian:
///   "SHAN" u32 version=1
///   u32 bones; per bone: u16 name_len, name, i16 parent, f32 t[3] r[4] s[3]
///   u32 clips; per clip: u16 key_len, key, f32 frame_duration, u32 frames, u32 tracks,
///     i16 track_to_bone[tracks], then frames*tracks*(f32 t[3] r[4] s[3])
/// `chr` names the skeleton's anibnd, `file` the output (anim_<file>.bin).
fn write_anim_bin(root: &Path, chr: &str, file: &str, clips: &[(String, hkx::Clip)]) {
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
    let path = root.join(format!("anim_{file}.bin"));
    std::fs::write(&path, &out).unwrap();
    println!("{chr}: {} bones, {} clips -> {} ({} KB)", bones.len(), clips.len(), path.display(), out.len() / 1024);
}

/// Every enemy the game places (map/mapstudio/*.msb enemy parts, PartType 2 / 10 dummy enemies):
/// chr id -> (NpcParam row, NpcThinkParam row) -> (placements, maps).
fn roster(root: &Path) -> BTreeMap<String, BTreeMap<(i64, i64), (usize, BTreeSet<String>)>> {
    let mut out: BTreeMap<String, BTreeMap<(i64, i64), (usize, BTreeSet<String>)>> = BTreeMap::new();
    let dir = root.join("map/mapstudio");
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "msb") {
            continue;
        }
        let map = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let Ok(d) = std::fs::read(&p) else { continue };
        for part in crate::msb::read(&d).parts {
            if !(part.kind == 2 || part.kind == 10) || !part.model.starts_with('c') || part.model == "c0000" || part.npc <= 0 {
                continue;
            }
            let slot = out.entry(part.model.clone()).or_default().entry((part.npc as i64, part.think as i64)).or_default();
            slot.0 += 1;
            slot.1.insert(map.clone());
        }
    }
    out
}

/// `sekiro-extract npcs <extracted> [chr ...]`: every placed enemy (or the listed ones) ->
/// extracted/enemies/<chr>.json (the same character data as combat_data.json's "enemy", plus that
/// character's param rows) and anim_<chr>.bin; extracted/enemies/roster.json lists them all.
///   NpcParam rows: the character's own (id / 10000 = chr number) and every placed row;
///   NpcThinkParam: the placed rows and the character's own; ThrowParam: rows whose
///   AtkChrId / DefChrId is the chr number (PC deathblows 10000000 + chr * 1000 + suffix,
///   the chr's grabs 20000000 + chr * 1000).
pub fn export_npcs(root: &Path, defs: &Path, only: &[String]) {
    let tmpl = tae::load_template(&defs.join("TAE.Template.SDT.xml"));
    let params = root.join("json/params");
    let npc_p = load_param(&params, "NpcParam");
    let think_p = load_param(&params, "NpcThinkParam");
    let throw_p = load_param(&params, "ThrowParam");
    let names: BTreeMap<i64, String> = std::fs::read(root.join("msg/engus/item.msgbnd.d/NPC名.fmg")).map(|d| crate::fmg::read(&d)).unwrap_or_default();
    let out_dir = root.join("enemies");
    let _ = std::fs::create_dir_all(&out_dir);
    let mut list = Vec::new();
    let mut jobs = Vec::new();
    for (chr, rows) in roster(root) {
        let num: i64 = chr[1..].parse().unwrap_or(-1);
        // Main row: the most placed one (ties: the lowest id).
        let mut placed: Vec<(&(i64, i64), &(usize, BTreeSet<String>))> = rows.iter().collect();
        placed.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
        let ((main_npc, main_think), _) = placed[0];
        let anim_chr = npc_p.get(main_npc).and_then(|r| r.get("normalChangeAnimChrId")).and_then(Value::as_i64).filter(|&c| c > 0).map_or(chr.clone(), |c| format!("c{c:04}"));
        let has_files = root.join(format!("chr/{chr}.chrbnd.d")).is_dir() && ["anibnd", "behbnd"].iter().all(|k| root.join(format!("chr/{anim_chr}.{k}.d")).is_dir());
        // The official name (NPC名.fmg by nameId: bosses and named NPCs), else the param row's
        // Paramdex name (English part before " -- ").
        let npc_name = |row: i64| {
            let r = npc_p.get(&row);
            r.and_then(|r| r.get("nameId")).and_then(Value::as_i64).and_then(|n| names.get(&n)).cloned().unwrap_or_else(|| {
                r.and_then(|r| r.get("name")).and_then(Value::as_str).map(|n| n.split(" -- ").next().unwrap_or("").trim().replace("&#39;", "'")).unwrap_or_default()
            })
        };
        let mut npc_rows: Vec<i64> = npc_p.keys().copied().filter(|id| id / 10000 == num).collect();
        npc_rows.extend(rows.keys().map(|k| k.0).filter(|id| npc_p.contains_key(id)));
        npc_rows.sort();
        npc_rows.dedup();
        let mut think_rows: Vec<i64> = think_p.keys().copied().filter(|id| id / 10000 == num).collect();
        think_rows.extend(rows.keys().map(|k| k.1).filter(|id| think_p.contains_key(id)));
        think_rows.sort();
        think_rows.dedup();
        let deathblows = rows.keys().filter_map(|k| npc_p.get(&k.0)?.get("ninsatuNum")?.as_i64()).max().unwrap_or(0);
        let placements: Vec<Value> = placed
            .iter()
            .map(|((npc, think), (count, maps))| {
                let r = npc_p.get(npc);
                let f = |k: &str| r.and_then(|r| r.get(k)).cloned().unwrap_or(Value::Null);
                json!({ "npc": npc, "think": think, "count": count, "maps": maps, "name": npc_name(*npc),
                        "hp": f("hp"), "posture": f("stamina"), "deathblows": f("ninsatuNum"), "variation": f("behaviorVariationId") })
            })
            .collect();
        let entry = json!({ "chr": chr, "name": npc_name(*main_npc), "npc": main_npc, "think": main_think, "deathblows": deathblows,
                            "animChr": anim_chr, "texChr": npc_p.get(main_npc).and_then(|r| r.get("normalChangeTexChrId")).cloned().unwrap_or(json!(-1)),
                            "exported": has_files, "placements": placements });
        list.push(entry.clone());
        if !has_files || (!only.is_empty() && !only.contains(&chr)) {
            continue;
        }
        jobs.push((chr.clone(), *main_npc, npc_rows, think_rows, entry, num, npc_name(*main_npc)));
    }
    let path = out_dir.join("roster.json");
    std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap()).unwrap();
    println!("roster: {} characters -> {}", list.len(), path.display());
    for (chr, main_npc, npc_rows, think_rows, entry, num, name) in jobs {
        let c = npc(&chr, main_npc, &npc_rows, &params);
        let mut data = export_character(root, &c, &[], &[], &tmpl, &params);
        let pick_rows = |p: &HashMap<i64, Map<String, Value>>, ids: &[i64]| -> Value {
            Value::Object(ids.iter().filter_map(|id| p.get(id).map(|r| (id.to_string(), Value::Object(r.clone())))).collect())
        };
        let throws: Vec<i64> = throw_p
            .iter()
            .filter(|(_, r)| [r.get("AtkChrId"), r.get("DefChrId")].iter().any(|v| v.and_then(|v| v.as_i64()) == Some(num)))
            .map(|(id, _)| *id)
            .collect();
        data["params"] = json!({
            "NpcParam": pick_rows(&npc_p, &npc_rows),
            "NpcThinkParam": pick_rows(&think_p, &think_rows),
            "ThrowParam": pick_rows(&throw_p, &throws),
        });
        data["roster"] = entry;
        let path = out_dir.join(format!("{chr}.json"));
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        println!("{chr}: {name} -> {} KB", std::fs::metadata(&path).map(|m| m.len() / 1024).unwrap_or(0));
    }
}
