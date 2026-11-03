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
    let mut fallback_only = Vec::new();
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
        // variation. A bare key whose exe row is missing was taken by the export's art-style
        // +1000 fallback (BehaviorParam ... + 1000 + judge), which FUN_140a12b80 does not do:
        // counted, not failed (kb/params.md).
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
                _ if !k.starts_with('v') => fallback_only.push(format!("{chr}:{k}")),
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
        "characters: Wolf + {} enemies, {} rows / {} fields the same as the game's; {keys} attack keys, {} bare NPC keys only through the export's +1000 fallback: {}",
        chrs.len(),
        r.rows,
        r.fields,
        fallback_only.len(),
        fallback_only.iter().take(12).cloned().collect::<Vec<_>>().join(", ")
    );
    assert!(chrs.len() > 30, "only {} enemy files", chrs.len());
    r.assert_clean("character rows vs the game");
}
