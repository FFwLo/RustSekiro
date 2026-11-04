//! RSekiro's param rows against the game's loaded tables (topic params).
//!
//! tools/emu-cases `params` reads every table of the SoloParamRepository (DAT_143d978b0) and
//! ThrowParam (DAT_143d6dae0) out of the Naomori-arena snapshot and writes the rows as numbers
//! (extracted/emu/params_rows.json: per table its field names and rows {id: [values]}). These
//! tests check, field by field, what RSekiro reads: the exported tables (extracted/json/params),
//! and every row Wolf and each exported enemy carries (combat_data.json, enemies/<chr>.json:
//! the params tables, attacks with their hit shapes, SpEffects, bullets, Wolf's behaviours and
//! tools), both the raw rows and the typed fields data.rs loads. Floats must be the same f32.

use crate::data::{Attack, BulletSpec, CharData, EnemyFile, SpEffect};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The game's rows: table -> (field -> column, id -> values).
struct Exe {
    tables: HashMap<String, (HashMap<String, usize>, serde_json::Map<String, Value>)>,
}

impl Exe {
    fn load() -> Option<Exe> {
        let path = crate::paths::emu().join("params_rows.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).unwrap();
        let mut tables = HashMap::new();
        for (name, t) in v["tables"].as_object().unwrap() {
            let fields = t["fields"].as_array().unwrap().iter().enumerate().map(|(i, f)| (f.as_str().unwrap().to_string(), i)).collect();
            tables.insert(name.clone(), (fields, t["rows"].as_object().unwrap().clone()));
        }
        Some(Exe { tables })
    }

    /// The game's row `id` of `table` as {field: value}.
    fn row(&self, table: &str, id: &str) -> Option<HashMap<&str, &Value>> {
        let (fields, rows) = self.tables.get(table)?;
        let vals = rows.get(id)?.as_array().unwrap();
        Some(fields.iter().map(|(f, &i)| (f.as_str(), &vals[i])).collect())
    }
}

/// The same number: integers exactly, floats as the same f32 (0 / false and 1 / true alike).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Value::Bool(x), _) => same(&Value::from(*x as i64), b),
        (_, Value::Bool(y)) => same(a, &Value::from(*y as i64)),
        (Value::Number(x), Value::Number(y)) => match (x.as_i64(), y.as_i64()) {
            (Some(p), Some(q)) => p == q,
            _ => x.as_f64().map(|v| v as f32).map(f32::to_bits) == y.as_f64().map(|v| v as f32).map(f32::to_bits),
        },
        _ => a == b,
    }
}

/// Fields of an exported row that are not the param's own (derived by the export).
const DERIVED: &[&str] = &["id", "name", "hits", "hitSourceType", "atkParamId", "behaviorName", "attack", "bulletId"];

/// Mismatches found, with the first few for the message.
#[derive(Default)]
struct Report {
    rows: usize,
    fields: usize,
    bad: Vec<String>,
}

impl Report {
    fn fail(&mut self, s: String) {
        self.bad.push(s);
    }

    /// Every own field of RSekiro's row `ours` against the game's row `id` of `table`.
    fn row(&mut self, exe: &Exe, who: &str, table: &str, id: &str, ours: &Value) {
        let Some(game) = exe.row(table, id) else {
            return self.fail(format!("{who}: {table} {id} is not in the game's table"));
        };
        self.rows += 1;
        for (k, v) in ours.as_object().unwrap() {
            if DERIVED.contains(&k.as_str()) || v.is_string() || v.is_object() {
                continue;
            }
            match game.get(k.as_str()) {
                None => self.fail(format!("{who}: {table} {id} field {k} is not a field of the game's row")),
                Some(g) if !same(v, g) => self.fail(format!("{who}: {table} {id} {k} = {v}, the game {g}")),
                _ => self.fields += 1,
            }
        }
    }

    /// An exported attack: its AtkParam row and its hit shapes (hit0..15 with DmyPoly1 > 0).
    fn attack(&mut self, exe: &Exe, who: &str, table: &str, a: &Value) {
        let id = a["atkParamId"].to_string();
        self.row(exe, who, table, &id, a);
        let Some(g) = exe.row(table, &id) else { return };
        let n = |k: String| g.get(k.as_str()).copied().cloned().unwrap_or(Value::Null);
        let hits: Vec<Value> = (0..16)
            .filter(|i| n(format!("hit{i}_DmyPoly1")).as_i64().unwrap_or(0) > 0)
            .map(|i| Value::Array(vec![n(format!("hit{i}_DmyPoly1")), n(format!("hit{i}_DmyPoly2")), n(format!("hit{i}_Radius")), n(format!("hit{i}_hitType")), n(format!("hti{i}_Priority"))]))
            .collect();
        if !same(&a["hits"], &Value::Array(hits.clone())) {
            self.fail(format!("{who}: {table} {id} hits {}, the game {}", a["hits"], Value::Array(hits)));
        }
    }

    /// A character's rows (Wolf: `pc`).
    fn character(&mut self, exe: &Exe, who: &str, d: &Value, pc: bool) {
        let atk = if pc { "AtkParam_Pc" } else { "AtkParam_Npc" };
        for a in d["attacks"].as_object().into_iter().flat_map(|m| m.values()) {
            self.attack(exe, who, atk, a);
        }
        for (id, s) in d["spEffects"].as_object().into_iter().flatten() {
            self.row(exe, who, "SpEffectParam", id, s);
        }
        for group in ["bullets", "bulletRows"] {
            for b in d[group].as_object().into_iter().flat_map(|m| m.values()) {
                self.row(exe, who, "Bullet", &b["bulletId"].to_string(), b);
                if b["attack"].is_object() {
                    self.attack(exe, who, atk, &b["attack"]);
                }
            }
        }
        for (id, b) in d["behaviors"].as_object().into_iter().flatten() {
            self.row(exe, who, "BehaviorParam_PC", id, b);
        }
        // Wolf's tools: EquipParamWeapon resourceItemA (emblems), residentSpEffectId, iconId,
        // attackBasePhysics / Fire.
        for p in d["prosthetics"].as_array().into_iter().flatten() {
            let ours = serde_json::json!({
                "resourceItemA": p["emblems"], "residentSpEffectId": p["resident"], "iconId": p["icon"],
                "attackBasePhysics": p["attackBasePhysics"], "attackBaseFire": p["attackBaseFire"],
            });
            self.row(exe, who, "EquipParamWeapon", &p["id"].to_string(), &ours);
        }
        for (table, rows) in d["params"].as_object().into_iter().flatten() {
            for (id, r) in rows.as_object().into_iter().flatten() {
                self.row(exe, who, table, id, r);
            }
        }
    }

    /// The typed fields data.rs loads (serde defaults would hide a field the export lacks).
    fn typed(&mut self, exe: &Exe, who: &str, d: &CharData, pc: bool) {
        let atk = if pc { "AtkParam_Pc" } else { "AtkParam_Npc" };
        let num = |v: f64| Value::from(v);
        let attack: [(&str, fn(&Attack) -> f64); 24] = [
            ("atkPhys", |a| a.atk_phys as f64),
            ("atkMag", |a| a.atk_mag as f64),
            ("atkFire", |a| a.atk_fire as f64),
            ("atkThun", |a| a.atk_thun as f64),
            ("atkDark", |a| a.atk_dark as f64),
            ("atkStam", |a| a.atk_stam as f64),
            ("directAtkStamDamage", |a| a.direct_atk_stam_damage as f64),
            ("staminaPhysicsAttribute", |a| a.stamina_physics_attribute as f64),
            ("atkAttribute", |a| a.atk_attribute as f64),
            ("repelLostStamDamage", |a| a.repel_lost_stam_damage as f64),
            ("isDisableParry", |a| a.is_disable_parry as f64),
            ("hitStopTime", |a| a.hit_stop_time as f64),
            ("hitStopTime_Defencer", |a| a.hit_stop_time_defencer as f64),
            ("dmgLevel", |a| a.dmg_level as f64),
            ("dmgLevel_vsPlayer", |a| a.dmg_level_vs_player as f64),
            ("guardAtkRate", |a| a.guard_atk_rate as f64),
            ("atkType", |a| a.atk_type as f64),
            ("spAttribute", |a| a.sp_attribute as f64),
            ("throwFlag", |a| a.throw_flag as f64),
            ("throwTypeId", |a| a.throw_type_id as f64),
            ("knockbackDist_DirectHit", |a| a.knockback_hit as f64),
            ("knockbackDist_Guard", |a| a.knockback_guard as f64),
            ("deflectAction", |a| a.deflect_action as f64),
            ("staminaDamageAttackHitParry", |a| a.stamina_damage_attack_hit_parry as f64),
        ];
        for a in d.attacks.values() {
            let Some(g) = exe.row(atk, &a.atk_param_id.to_string()) else { continue };
            for (k, get) in &attack {
                if !same(&num(get(a)), g[k]) {
                    self.fail(format!("{who}: typed {atk} {} {k} = {}, the game {}", a.atk_param_id, get(a), g[k]));
                }
            }
        }
        let sp: [(&str, fn(&SpEffect) -> f64); 12] = [
            ("effectEndurance", |s| s.effect_endurance as f64),
            ("stateInfo", |s| s.state_info as f64),
            ("maxHpRate", |s| s.max_hp_rate as f64),
            ("maxStaminaRate", |s| s.max_stamina_rate as f64),
            ("physicsAttackPowerRate", |s| s.physics_attack_power_rate as f64),
            ("staminaAttackRate", |s| s.stamina_attack_rate as f64),
            ("defStaminaAttackRate", |s| s.def_stamina_attack_rate as f64),
            ("changeHpRate", |s| s.change_hp_rate as f64),
            ("slashDamageCutRate", |s| s.damage_cut.slash as f64),
            ("thrustDamageCutRate", |s| s.damage_cut.thrust as f64),
            ("defSlashStaminaDmgRate", |s| s.def_slash_stamina_dmg_rate as f64),
            ("staminaRecoverSpeedRate", |s| s.stamina_recover_speed_rate as f64),
        ];
        for (id, s) in &d.sp_effects {
            let Some(g) = exe.row("SpEffectParam", id) else { continue };
            for (k, get) in &sp {
                if !same(&num(get(s)), g[k]) {
                    self.fail(format!("{who}: typed SpEffectParam {id} {k} = {}, the game {}", get(s), g[k]));
                }
            }
        }
        let bullet: [(&str, fn(&BulletSpec) -> f64); 8] = [
            ("initVellocity", |b| b.init_vellocity as f64),
            ("maxVellocity", |b| b.max_vellocity as f64),
            ("minVellocity", |b| b.min_vellocity as f64),
            ("dist", |b| b.dist as f64),
            ("homingAngle", |b| b.homing_angle as f64),
            ("numShoot", |b| b.num_shoot as f64),
            ("HitBulletID", |b| b.hit_bullet_id as f64),
            ("damageDamp", |b| b.damage_damp as f64),
        ];
        for b in d.bullet_rows.values().chain(d.bullets.values()) {
            let Some(g) = exe.row("Bullet", &b.bullet_id.to_string()) else { continue };
            for (k, get) in &bullet {
                if !same(&num(get(b)), g[k]) {
                    self.fail(format!("{who}: typed Bullet {} {k} = {}, the game {}", b.bullet_id, get(b), g[k]));
                }
            }
        }
    }

    fn assert_clean(&self, what: &str) {
        assert!(self.bad.is_empty(), "{what}: {} mismatches, e.g.\n{}", self.bad.len(), self.bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
    }
}

/// Every exported table (extracted/json/params, what data.rs, ai.rs, status.rs ... read
/// directly) against the game's: the same row ids and every field the same. Ids a table holds
/// twice (CutsceneParam, ObjectParam, TalkParam ...: the game's sorted lookup finds either)
/// are skipped. 139 tables in the game's repository plus ThrowParam; the four Lod tables the
/// export has are not loaded there.
#[test]
fn param_tables_match_the_exe() {
    let Some(exe) = Exe::load() else {
        eprintln!("skipped: extracted/emu/params_rows.json missing (tools/emu-cases params)");
        return;
    };
    let mut r = Report::default();
    let mut tables = 0;
    for (name, (_, game_rows)) in &exe.tables {
        let path = crate::paths::extracted().join(format!("json/params/{name}.json"));
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        tables += 1;
        let v: Value = serde_json::from_str(&text).unwrap();
        let rows = v["rows"].as_array().unwrap();
        let mut seen: HashMap<String, usize> = HashMap::new();
        for row in rows {
            *seen.entry(row["id"].to_string()).or_default() += 1;
        }
        let ours: HashSet<&String> = seen.keys().collect();
        for id in game_rows.keys() {
            if !ours.contains(id) {
                r.fail(format!("json/params/{name}: row {id} missing"));
            }
        }
        for row in rows {
            let id = row["id"].to_string();
            if seen[&id] > 1 {
                continue;
            }
            r.row(&exe, "json/params", name, &id, row);
        }
    }
    eprintln!("param tables: {tables} tables, {} rows, {} fields the same as the game's", r.rows, r.fields);
    assert!(tables >= 130, "only {tables} exported tables found");
    r.assert_clean("json/params vs the game");
}

/// Wolf's and every exported enemy's rows as RSekiro loads them (combat_data.json's player and
/// params, EnemyFile::load of each extracted/enemies/c*.json: the 14 bosses, 18 mini-bosses and
/// the common enemies alike) against the game's, raw and typed; and each attack key's
/// BehaviorParam row as the exe builds its id (FUN_140a12b80: 200000000 + NpcParam
/// behaviorVariationId * 1000 + judge for an NPC; Wolf's BehaviorParam_PC 100000000 +
/// variation * 1000 + judge) must point at the attack's AtkParam row.
#[test]
fn character_rows_match_the_exe() {
    let Some(exe) = Exe::load() else {
        eprintln!("skipped: extracted/emu/params_rows.json missing (tools/emu-cases params)");
        return;
    };
    let root = crate::paths::extracted();
    let Ok(text) = std::fs::read_to_string(root.join("combat_data.json")) else { return };
    let cd: Value = serde_json::from_str(&text).unwrap();
    let mut r = Report::default();
    r.character(&exe, "c0000", &cd["player"], true);
    let wolf: CharData = serde_json::from_value(cd["player"].clone()).unwrap();
    r.typed(&exe, "c0000", &wolf, true);
    r.character(&exe, "combat_data", &serde_json::json!({ "params": cd["params"] }), false);
    let behavior = |pc: bool, id: i64| exe.row(if pc { "BehaviorParam_PC" } else { "BehaviorParam" }, &id.to_string());
    let mut keys = 0;
    let mut chrs = Vec::new();
    for e in std::fs::read_dir(root.join("enemies")).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.len() == 10 && name.starts_with('c') && name.ends_with(".json") {
            chrs.push(name[..5].to_string());
        }
    }
    chrs.sort();
    for chr in &chrs {
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(root.join(format!("enemies/{chr}.json"))).unwrap()).unwrap();
        let before = r.rows;
        r.character(&exe, chr, &raw, false);
        let file = EnemyFile::load(chr).unwrap_or_else(|| panic!("{chr}: EnemyFile::load failed"));
        r.typed(&exe, chr, &file.data, false);
        // The attack keys: "v<variation>:<judge>" and the bare judge of the main row's
        // variation. FUN_140a12b80 has no fallback (the export's base + 1000 + judge one gave 66
        // bare keys the exe never resolves; `npc_attack_keys_resolve_like_the_exe`).
        let main = file.roster["npc"].as_i64().unwrap_or(0);
        let var = exe.row("NpcParam", &main.to_string()).and_then(|g| g["behaviorVariationId"].as_i64()).unwrap_or(0);
        for (k, a) in &file.data.attacks {
            let (v, j) = match k.strip_prefix('v').and_then(|s| s.split_once(':')) {
                Some((v, j)) => (v.parse::<i64>().unwrap(), j.parse::<i64>().unwrap()),
                None => match k.parse::<i64>() {
                    Ok(j) => (var, j),
                    Err(_) => continue,
                },
            };
            keys += 1;
            match behavior(false, 200_000_000 + v * 1000 + j) {
                Some(b) if b["refType"].as_i64() == Some(0) => {
                    if b["refId"].as_i64() != Some(a.atk_param_id) {
                        r.fail(format!("{chr}: attack {k} is AtkParam {}, the exe's BehaviorParam row refers to {}", a.atk_param_id, b["refId"]));
                    }
                }
                _ => r.fail(format!("{chr}: attack {k} has no refType-0 BehaviorParam row {}", 200_000_000 + v * 1000 + j)),
            }
        }
        eprintln!("{chr}: {} rows checked", r.rows - before);
    }
    for (k, a) in &wolf.attacks {
        let id = if let Some(j) = k.strip_prefix("pc") {
            j.parse::<i64>().ok()
        } else if let Some((v, j)) = k.strip_prefix('v').and_then(|s| s.split_once(':')) {
            Some(100_000_000 + v.parse::<i64>().unwrap() * 1000 + j.parse::<i64>().unwrap())
        } else {
            None
        };
        let Some(id) = id else { continue };
        keys += 1;
        match behavior(true, id) {
            Some(b) if b["refType"].as_i64() == Some(0) && b["refId"].as_i64() == Some(a.atk_param_id) => {}
            b => r.fail(format!("c0000: attack {k} is AtkParam {}, BehaviorParam_PC {id} is {:?}", a.atk_param_id, b.map(|b| (b["refType"].clone(), b["refId"].clone())))),
        }
    }
    eprintln!(
        "characters: Wolf + {} enemies, {} rows / {} fields the same as the game's; {keys} attack keys",
        chrs.len(),
        r.rows,
        r.fields
    );
    assert!(chrs.len() > 30, "only {} enemy files", chrs.len());
    r.assert_clean("character rows vs the game");
}

/// Every judge an enemy file uses (its attack keys and its anims' AttackBehavior events) in its
/// main NpcParam variation and each "v<variation>:" one, as RSekiro resolves it after
/// `select_variation`, against the exe's (tools/emu-cases `params_keys`: FUN_140a12b80 on the
/// General with that variation, then the attack info FUN_14084fc50's AtkParam id, -1 = none).
/// The exe has no fallback for a missing row.
#[test]
fn npc_attack_keys_resolve_like_the_exe() {
    let path = crate::paths::emu().join("params_keys.json");
    let (Ok(text), Some(exe)) = (std::fs::read_to_string(&path), Exe::load()) else {
        eprintln!("skipped: extracted/emu/params_keys.json or params_rows.json missing (tools/emu-cases params_keys / params)");
        return;
    };
    let v: Value = serde_json::from_str(&text).unwrap();
    let game: HashMap<(i64, i64), (i64, i64)> = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| ((c["variation"].as_i64().unwrap(), c["judge"].as_i64().unwrap()), (c["row"].as_i64().unwrap(), c["atk"].as_i64().unwrap())))
        .collect();
    // The id is the plain sum (no SpEffect shift on the snapshot's General), -1 for a judge
    // outside 0..1000 (an unsigned compare: -1 too).
    for (&(var, j), &(row, _)) in &game {
        assert_eq!(row, if (0..1000).contains(&j) { 200_000_000 + var * 1000 + j } else { -1 }, "FUN_140a12b80 ({var}, {j})");
    }
    let root = crate::paths::extracted();
    let mut chrs: Vec<String> = std::fs::read_dir(root.join("enemies"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.len() == 10 && n.starts_with('c') && n.ends_with(".json"))
        .map(|n| n[..5].to_string())
        .collect();
    chrs.sort();
    let mut r = Report::default();
    let (mut checked, mut attacks, mut missing) = (0, 0, 0);
    for chr in &chrs {
        let file = EnemyFile::load(chr).unwrap();
        let main = file.roster["npc"].as_i64().unwrap_or(0);
        let main_var = exe.row("NpcParam", &main.to_string()).and_then(|g| g["behaviorVariationId"].as_i64()).unwrap_or(0);
        let mut vars = vec![main_var];
        let mut judges: HashSet<i64> = HashSet::new();
        for k in file.data.attacks.keys() {
            if let Ok(j) = k.parse::<i64>() {
                judges.insert(j);
            } else if let Some((v, j)) = k.strip_prefix('v').and_then(|s| s.split_once(':')) {
                vars.push(v.parse().unwrap());
                judges.insert(j.parse().unwrap());
            }
        }
        for a in file.data.anims.values() {
            judges.extend(a.events.iter().filter(|e| e.kind == 1).filter_map(|e| e.arg_i64("BehaviorJudgeID")));
        }
        vars.sort();
        vars.dedup();
        for &var in &vars {
            let mut d = CharData { attacks: file.data.attacks.clone(), bullets: file.data.bullets.clone(), ..Default::default() };
            crate::data::select_variation(&mut d, var);
            // Judges from 1000 are ThrowAttackBehavior's (TAE 304: c1020's 1010-1090, c5000's
            // 1000-1080), whose id is not built by FUN_140a12b80 (it returns -1 for them).
            // gap: the throw handler's id rule is not traced; those keys stay base + judge.
            for &j in judges.iter().filter(|&&j| (0..1000).contains(&j)) {
                let Some(&(_, atk)) = game.get(&(var, j)) else {
                    missing += 1;
                    continue;
                };
                checked += 1;
                // The export keeps a row whose AtkParam_Npc row exists (the exe's info holds the
                // id either way; its hit entries need the row).
                let want = (atk != -1 && exe.row("AtkParam_Npc", &atk.to_string()).is_some()).then_some(atk);
                let ours = d.attacks.get(&j.to_string()).map(|a| a.atk_param_id);
                attacks += want.is_some() as usize;
                if ours != want {
                    r.fail(format!("{chr} variation {var} judge {j}: RSekiro {ours:?}, the exe {want:?} (AtkParam {atk})"));
                }
            }
        }
    }
    eprintln!("npc attack keys: {} enemies, {checked} (variation, judge) checked, {attacks} attacks, {missing} not run", chrs.len());
    assert!(checked > 1000, "only {checked} keys checked");
    assert_eq!(missing, 0, "rerun tools/emu-cases params_keys");
    r.assert_clean("NPC attack keys vs the exe");
}

/// Wolf's right-hand judge with an art equipped (the snapshot's, read off the exe's own rows:
/// 5008) as the exe builds it (PlayerIns vfunc 0x1c0, FUN_140a24560; atkrec_judges): the art's
/// row 100000000 + variation * 1000 + judge, else variation / 100 * 100's (5000); and RSekiro's
/// anims of that art resolving the same AtkParam rows (`attack_windows`).
#[test]
fn wolf_art_judges_resolve_like_the_exe() {
    let path = crate::paths::emu().join("atkrec_judges.json");
    let (Ok(text), Some(exe)) = (std::fs::read_to_string(&path), Exe::load()) else {
        eprintln!("skipped: extracted/emu/atkrec_judges.json or params_rows.json missing");
        return;
    };
    let Ok(cd) = std::fs::read_to_string(crate::paths::extracted().join("combat_data.json")) else { return };
    let cd: Value = serde_json::from_str(&cd).unwrap();
    let mut wolf: CharData = serde_json::from_value(cd["player"].clone()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    let rows: HashMap<i64, i64> = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["chr"] == "wolf" && c["via"] == "0x1c0" && c["hand"].as_i64() == Some(-1))
        .map(|c| (c["judge"].as_i64().unwrap(), c["row"].as_i64().unwrap()))
        .collect();
    let pc_row = |id: i64| exe.row("BehaviorParam_PC", &id.to_string());
    let art = (rows[&200] - 100_000_000) / 1000;
    assert!((5001..5100).contains(&art), "the snapshot's right hand is not an art ({art})");
    let mut r = Report::default();
    for (&j, &row) in &rows {
        let own = 100_000_000 + art * 1000 + j;
        let want = if pc_row(own).is_some() { own } else { 100_000_000 + art / 100 * 100 * 1000 + j };
        if row != want {
            r.fail(format!("judge {j}: exe row {row}, rule {want}"));
        }
    }
    // The art's anims (a<spAtkcategory>): every AttackBehavior judge's AtkParam row is the exe
    // row's refId.
    wolf.art_variation = Some(art);
    let group = (5000..8000)
        .find_map(|id| {
            let w = exe.row("EquipParamWeapon", &id.to_string())?;
            (w["behaviorVariationId"].as_i64() == Some(art)).then(|| w["spAtkcategory"].as_i64()).flatten()
        })
        .expect("the art's EquipParamWeapon row");
    let mut keys: Vec<String> = wolf.anims.keys().filter(|k| k.starts_with(&format!("a{group}_"))).cloned().collect();
    keys.sort();
    let mut windows = 0;
    for key in &keys {
        for (e, atk, j) in wolf.attack_windows(key) {
            if e.kind != 1 {
                continue;
            }
            windows += 1;
            let row = rows[&j];
            let want = pc_row(row).filter(|b| b["refType"].as_i64() == Some(0)).and_then(|b| b["refId"].as_i64());
            if want != Some(atk.atk_param_id) {
                r.fail(format!("{key} judge {j}: RSekiro AtkParam {}, the exe's row {row} -> {want:?}", atk.atk_param_id));
            }
        }
    }
    eprintln!("wolf art judges: art {art} (a{group}), {} judges, {} anims, {windows} windows", rows.len(), keys.len());
    assert!(windows > 0, "no a{group} attack windows");
    r.assert_clean("Wolf's art judges vs the exe");
}

/// (table, Paramdex field, exe function) for every field RSekiro's code names next to the exe
/// function it cites, where the emulator saw that function read it: the field's row offset in
/// the paramdef RSekiro reads by is the one the cited code loads, so the name is not swapped
/// with a neighbour (the ThrowParam sight-cast one was). From the param read watch
/// (tools/emu-cases EMU_PARAM_READS over atkrec_rows, guard, speffect, speffect2, hits, bullets,
/// items, aiint, lockcam, air_falldmg, wire, damage, knockback, brkwin; kb/params.md E30).
const CITED_READS: &[(&str, &str, u64)] = &[
    ("AtkParam_Npc", "AppearAiSoundId", 0x1409a1290),
    ("AtkParam_Npc", "atkDark", 0x14083dea0),
    ("AtkParam_Npc", "atkFire", 0x14083dea0),
    ("AtkParam_Npc", "atkMag", 0x14083dea0),
    ("AtkParam_Npc", "atkPhys", 0x14083dea0),
    ("AtkParam_Npc", "atkStam", 0x140847d50),
    ("AtkParam_Npc", "atkThun", 0x14083dea0),
    ("AtkParam_Npc", "defSeMaterial1", 0x140ba8840),
    ("AtkParam_Npc", "defSfxMaterial1", 0x140ba8a50),
    ("AtkParam_Npc", "directAtkStamDamage", 0x140847d50),
    ("AtkParam_Npc", "directAtkStamDamage_Attacker", 0x140842790),
    ("AtkParam_Npc", "guardBreakRate", 0x14084cd80),
    ("AtkParam_Npc", "guardCutCancelRate", 0x140a12d60),
    ("AtkParam_Npc", "hitSourceType", 0x14084fc50),
    ("AtkParam_Npc", "hitStopTime", 0x140b9cd90),
    ("AtkParam_Npc", "hitStopTime_Defencer", 0x140b9cfb0),
    ("AtkParam_Npc", "isAllDirGuard", 0x140b6c880),
    ("AtkParam_Npc", "isGhostAtk", 0x140b6c880),
    ("AtkParam_Npc", "justDeflectedAction", 0x140b698d0),
    ("AtkParam_Npc", "repelLostStamDamage", 0x140847d50),
    ("AtkParam_Npc", "repelLostStamDamage_Attacker", 0x140842790),
    ("AtkParam_Npc", "repelVictoryStamDamage_Attacker", 0x140842790),
    ("AtkParam_Npc", "trackType", 0x140b68a50),
    ("AtkParam_Pc", "AppearAiSoundId", 0x1409a1290),
    ("AtkParam_Pc", "atkDark", 0x14083e060),
    ("AtkParam_Pc", "atkFire", 0x14083e060),
    ("AtkParam_Pc", "atkMag", 0x14083e060),
    ("AtkParam_Pc", "atkPhys", 0x14083e060),
    ("AtkParam_Pc", "atkSuperArmorCorrection", 0x140848440),
    ("AtkParam_Pc", "atkThun", 0x14083e060),
    ("AtkParam_Pc", "defSeMaterial1", 0x140ba8840),
    ("AtkParam_Pc", "defSfxMaterial1", 0x140ba8a50),
    ("AtkParam_Pc", "directAtkStamDamage_Attacker", 0x140842790),
    ("AtkParam_Pc", "guardAtkRate", 0x14084c7b0),
    ("AtkParam_Pc", "guardAtkRateCorrection", 0x14084c7b0),
    ("AtkParam_Pc", "guardBreakCorrection", 0x14084ce20),
    ("AtkParam_Pc", "guardCutCancelRate", 0x140a24ba0),
    ("AtkParam_Pc", "hitSourceType", 0x14084fc50),
    ("AtkParam_Pc", "hitStopTime", 0x140b9cd90),
    ("AtkParam_Pc", "hitStopTime_Defencer", 0x140b9cfb0),
    ("AtkParam_Pc", "isAllDirGuard", 0x140b6c880),
    ("AtkParam_Pc", "isGhostAtk", 0x140b6c880),
    ("AtkParam_Pc", "justDeflectedAction", 0x140b698d0),
    ("AtkParam_Pc", "repelLostStamDamage_Attacker", 0x140842790),
    ("AtkParam_Pc", "repelVictoryStamDamage_Attacker", 0x140842790),
    ("BehaviorParam", "category", 0x140b698d0),
    ("BehaviorParam_PC", "category", 0x140b698d0),
    ("Bullet", "accelInRange", 0x140b14650),
    ("Bullet", "gravityInRange", 0x140b14650),
    ("Bullet", "homingAngle", 0x140b11270),
    ("Bullet", "homingBeginDist", 0x140b11270),
    ("Bullet", "hormingStopRange", 0x140b11270),
    ("Bullet", "maxVellocity", 0x140b14650),
    ("Bullet", "minVellocity", 0x140b14650),
    ("Bullet", "spEffectId0", 0x1410b7910),
    ("EquipParamWeapon", "attackBaseRepel", 0x14084c7b0),
    ("EquipParamWeapon", "behaviorVariationId", 0x140a24560),
    ("EquipParamWeapon", "defSeMaterial1", 0x140ba8840),
    ("EquipParamWeapon", "defSfxMaterial1", 0x140ba8a50),
    ("EquipParamWeapon", "guardBaseRepel", 0x14084ce20),
    ("EquipParamWeapon", "guardCutCancelRate", 0x140a24ba0),
    ("EquipParamWeapon", "overStrength", 0x14084c7b0),
    ("EquipParamWeapon", "overStrength", 0x14084ce20),
    ("EquipParamWeapon", "resourceItemA", 0x140a26010),
    ("EquipParamWeapon", "saWeaponDamage", 0x140848440),
    ("EquipParamWeapon", "spAtkcategory", 0x1409f1830),
    ("HitMtrlParam", "disableFallDamage", 0x140ba8cb0),
    ("HitMtrlParam", "fallDamageRate", 0x1408433f0),
    ("HitMtrlParam", "fallDeathDist", 0x14084e5d0),
    ("LockCamParam", "bulletAngRange", 0x140b2e0e0),
    ("LockCamParam", "bulletMaxRadius", 0x140b2e0e0),
    ("LockCamParam", "camDistTarget", 0x14073c260),
    ("LockCamParam", "camFovY", 0x14073c260),
    ("LockCamParam", "chrOrgOffset_Y", 0x14073c260),
    ("LockCamParam", "lockRotXShiftRatio", 0x14073c260),
    ("LockCamParam", "rotRangeMinX", 0x140742230),
    ("NpcParam", "hitStopType", 0x140b9d850),
    ("NpcParam", "maxDebtStamina", 0x140bd4e20),
    ("NpcParam", "staminaRecoverBaseVel", 0x140a13a70),
    ("NpcParam", "superArmorDurability", 0x140a12de0),
    ("NpcParam", "superArmorRecoverCorrection", 0x140a12e90),
    ("NpcThinkParam", "ear_dist", 0x140610a30),
    ("NpcThinkParam", "ear_soundcut_dist", 0x140610b30),
    ("NpcThinkParam", "nose_dist", 0x14061c940),
    ("ReinforceParamWeapon", "saWeaponAtkRate", 0x140848440),
    ("SpEffectParam", "aroundSightPointAddRate", 0x140bfc050),
    ("SpEffectParam", "categoryPriority", 0x140c02000),
    ("SpEffectParam", "categoryPriority", 0x140c05a80),
    ("SpEffectParam", "categoryPriority", 0x140c05c10),
    ("SpEffectParam", "categoryPriority", 0x140c064c0),
    ("SpEffectParam", "changeHpEstusFlaskCorrectRate", 0x140c00ce0),
    ("SpEffectParam", "changeStaminaRate", 0x140bfe2c0),
    ("SpEffectParam", "disableBlood", 0x140c05f20),
    ("SpEffectParam", "disablePoison", 0x140c05f20),
    ("SpEffectParam", "effectEndurance", 0x140c03af0),
    ("SpEffectParam", "effectEndurance", 0x140c06860),
    ("SpEffectParam", "guardDefFlickPowerRate", 0x140bfccc0),
    ("SpEffectParam", "hearingSearchEnemyCut", 0x140bfe590),
    ("SpEffectParam", "hpRecoverRate", 0x140c00ce0),
    ("SpEffectParam", "invocationConditionsStateChange1", 0x140c060d0),
    ("SpEffectParam", "isWaitModeDelete", 0x140c04090),
    ("SpEffectParam", "lifeReductionType", 0x140c00de0),
    ("SpEffectParam", "motionInterval", 0x140c06c60),
    ("SpEffectParam", "replaceSpEffectId", 0x140c005b0),
    ("SpEffectParam", "sightSearchEnemyCut", 0x140bfe8a0),
    ("SpEffectParam", "spCategory", 0x140c02000),
    ("SpEffectParam", "spCategory", 0x140c02280),
    ("SpEffectParam", "spCategory", 0x140c05a80),
    ("SpEffectParam", "spCategory", 0x140c05c10),
    ("SpEffectParam", "spCategory", 0x140c064c0),
    ("SpEffectParam", "staminaRecoverChangeSpeed", 0x140bfbe40),
    ("SpEffectParam", "stateInfo", 0x140bfc170),
    ("SpEffectParam", "stateInfo", 0x140bfc490),
    ("SpEffectParam", "stateInfo", 0x140bfccc0),
    ("SpEffectParam", "stateInfo", 0x140c00de0),
    ("SpEffectParam", "stateInfo", 0x140c02000),
    ("SpEffectParam", "stateInfo", 0x140c02280),
    ("SpEffectParam", "stateInfo", 0x140c05a10),
    ("SpEffectParam", "stateInfo", 0x140c064c0),
    ("SpEffectParam", "stateInfo", 0x140c06860),
    ("SpEffectParam", "wepParamChange", 0x140c06240),
    ("ThrowKindParam", "doesCheckNormalFallOrbit", 0x140be3ba0),
    ("ThrowKindParam", "throwKindCategory", 0x140be3810),
    ("ThrowParam", "DiffAngMin", 0x140be0270),
    ("ThrowParam", "diffAngMyToDef", 0x140be0270),
    ("ThrowParam", "isSkipSphereCast", 0x140be3da0),
    ("ThrowParam", "judgeRangeBasePosDmyId1", 0x140be3290),
    ("ThrowParam", "lowerYRange", 0x140be0270),
    ("ThrowParam", "sphereCastRadiusRateTop", 0x140be3da0),
    ("ThrowParam", "upperYRange", 0x140be0270),
];

/// Every `CITED_READS` claim against extracted/emu/params_reads.json: some instruction of that
/// function read a byte of that field (the read's row offset decoded with the same Paramdex
/// layout the export uses).
#[test]
fn cited_param_fields_are_read_by_the_cited_exe_code() {
    let Ok(text) = std::fs::read_to_string(crate::paths::emu().join("params_reads.json")) else {
        eprintln!("skipped: extracted/emu/params_reads.json missing (EMU_PARAM_READS, then emu-cases params_reads)");
        return;
    };
    let v: Value = serde_json::from_str(&text).unwrap();
    let mut seen: HashSet<(String, String, u64)> = HashSet::new();
    for r in v["reads"].as_array().unwrap() {
        let Some(func) = r[5].as_u64() else { continue };
        for f in r[3].as_array().unwrap() {
            seen.insert((r[1].as_str().unwrap().to_string(), f.as_str().unwrap().to_string(), func));
        }
    }
    assert!(!seen.is_empty(), "params_reads.json has no functions (set DSEKIRO_FUNCTIONS for params_reads)");
    let missing: Vec<String> = CITED_READS
        .iter()
        .filter(|(t, f, func)| !seen.contains(&(t.to_string(), f.to_string(), *func)))
        .map(|(t, f, func)| format!("{t}.{f} by FUN_{func:x}"))
        .collect();
    eprintln!("cited param reads: {} claims, {} reads recorded", CITED_READS.len(), seen.len());
    assert!(missing.is_empty(), "{} cited fields not read by their function: {}", missing.len(), missing.join(", "));
}
