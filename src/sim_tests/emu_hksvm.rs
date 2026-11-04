//! The copied HKS VM's engine side (row E9): sekiro-rs's VM (crates/hks) routes the scripts'
//! `env(id, ...)` / `act(id, ...)` to RSekiro's `Host` through `hks::VmHost`, which turns string
//! ids into numbers, the first argument into the answer's key and the answer into the number the
//! script sees. Checked against the exe's own: its name maps (FUN_140b362d0 / FUN_140b34ca0,
//! recorded in `hks_env.json`), its env dispatcher FUN_140b36410's answers (`hks_env.json`,
//! `hks2_env.json`) and its act dispatcher FUN_140b32410's ids (`hks2_act.json`).

use super::*;
use sekiro_hks::{CommandId, Host as _, Value as V};

fn load(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// The exe's map lookup: the name cut to 31 UTF-16 units (the map's keys are), else 9999.
fn exe_id(map: &serde_json::Value, name: &str) -> i64 {
    let cut = String::from_utf16_lossy(&name.encode_utf16().take(31).collect::<Vec<_>>());
    map[&cut].as_i64().unwrap_or(crate::hks::UNKNOWN_NAME)
}

/// A case's id as the script passes it: `"name"` or a number.
fn command(text: &str) -> (Option<String>, i32) {
    match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(name) => (Some(name.to_string()), 0),
        None => (None, text.parse().unwrap()),
    }
}

/// Every string id any shipped script (all 81 compiled `action/script/*.hks`) passes to `env` or
/// `act` resolves in the copied VM's host as the exe's name maps resolve it.
#[test]
pub(crate) fn copied_vm_resolves_named_ids_like_the_exe() {
    let Some(v) = load("hks_env.json") else { return };
    let dir = crate::paths::extracted().join("action/script");
    let Ok(files) = std::fs::read_dir(&dir) else { return };
    let mut calls = Vec::new();
    let mut globals = sekiro_hks::inventory::GlobalUse::default();
    let mut chunks = 0;
    for f in files.flatten() {
        let path = f.path();
        if path.extension().is_none_or(|e| e != "hks") {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let file = sekiro_hks::bytecode::parse(&data).unwrap();
        sekiro_hks::inventory::scan(&file.main, &path.file_name().unwrap().to_string_lossy(), &mut calls, &mut globals);
        chunks += 1;
    }
    let mut seen = std::collections::BTreeMap::new();
    for c in &calls {
        let (map, ours) = match c.callee.as_deref() {
            Some("env") => (&v["env_names"], crate::hks::NAMED_ENV),
            Some("act") => (&v["act_names"], crate::hks::NAMED_ACT),
            _ => continue,
        };
        let Some(sekiro_hks::inventory::Arg::Str(name)) = c.args.as_ref().and_then(|a| a.first()) else { continue };
        let entry = seen.entry((c.callee.clone().unwrap(), name.clone())).or_insert((exe_id(map, name), crate::hks::named_id(ours, name), 0, c.chunk.clone()));
        entry.2 += 1;
    }
    for ((f, name), (exe, ours, sites, chunk)) in &seen {
        println!("{f}(\"{name}\"): exe {exe}, copied VM {ours} ({sites} sites, {chunk})");
    }
    assert!(chunks >= 81, "{chunks} scripts");
    assert_eq!(seen.len(), 3, "{seen:?}");
    for ((f, name), (exe, ours, ..)) in &seen {
        assert_eq!(exe, ours, "{f}(\"{name}\")");
    }
}

/// The exe's env answers, put into RSekiro's `Host` under the id and first argument the case
/// asked, come out of the copied VM's `env` as the script would see them: every case of
/// `hks_env.json` (idle Wolf, 82 ids) and `hks2_env.json` (223 poses), with its string id as the
/// script writes it. And an answer RSekiro holds as a fraction comes out as the exe's integer.
#[test]
pub(crate) fn copied_vm_passes_the_exes_env_answers() {
    let (Some(a), Some(b)) = (load("hks_env.json"), load("hks2_env.json")) else { return };
    let mut cases: Vec<(String, i64, Vec<f64>, i64)> = Vec::new();
    for e in a["env"].as_array().unwrap() {
        for c in e["cases"].as_array().unwrap() {
            let args = c["args"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            cases.push((e["id_text"].as_str().unwrap().to_string(), e["id"].as_i64().unwrap(), args, c["ret"].as_i64().unwrap()));
        }
    }
    for c in b["cases"].as_array().unwrap() {
        if c["stopped"] == true {
            continue;
        }
        let args = c["args"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        cases.push((c["id"].to_string(), c["id"].as_i64().unwrap(), args, c["ret"].as_i64().unwrap()));
    }
    let mut n = 0;
    for (text, id, args, ret) in &cases {
        let mut host = crate::hks::Host::default();
        host.env.insert((*id, args.first().map(|&x| x as i64)), *ret as f64);
        let values: Vec<V> = args.iter().map(|&x| V::Number(x as f32)).collect();
        let (name, number) = command(text);
        let cid = match &name {
            Some(n) => CommandId::Name(n),
            None => CommandId::Number(number),
        };
        let got = crate::hks::VmHost(&mut host).env(cid, &values);
        assert_eq!(got.as_number(), Some(*ret as f32), "env({text}, {args:?}): exe {ret}");
        assert!(host.unanswered.is_empty(), "env({text}): resolved elsewhere than {id}: {:?}", host.unanswered);
        n += 1;
    }
    // DT (333): the exe's 16 at 1/60 s (hks_env.json), where hks_wolf.rs holds 1000 / 60.
    let dt = a["env"].as_array().unwrap().iter().find(|e| e["id"] == 333).unwrap()["cases"][0]["ret"].as_i64().unwrap();
    let mut host = crate::hks::Host::default();
    host.env.insert((333, None), 1000.0 / 60.0);
    assert_eq!(crate::hks::VmHost(&mut host).env(CommandId::Number(333), &[]).as_number(), Some(dt as f32));
    println!("copied VM env: {n} exe answers passed through");
    assert!(n > 500, "{n}");
}

/// The act ids the copied VM hands RSekiro's `Host` for every case the exe's act dispatcher ran
/// (`hks2_act.json`, 129 calls over 44 ids, the string id among them) are the exe's.
#[test]
pub(crate) fn copied_vm_routes_the_exes_act_ids() {
    let Some(v) = load("hks2_act.json") else { return };
    let mut n = 0;
    for c in v["cases"].as_array().unwrap() {
        // 148 (SetHavokVariable), 9000 (debug print), 9103 were not run.
        if c["args"].is_null() {
            continue;
        }
        let id = c["id"].as_i64().unwrap();
        let text = c["id_text"].as_str().unwrap();
        let args: Vec<V> = c["args"].as_array().unwrap().iter().map(|x| V::Number(x.as_f64().unwrap() as f32)).collect();
        let (name, number) = command(text);
        let cid = match &name {
            Some(n) => CommandId::Name(n),
            None => CommandId::Number(number),
        };
        let mut host = crate::hks::Host::default();
        crate::hks::VmHost(&mut host).act(cid, &args);
        assert_eq!(host.acts, vec![id], "act({text})");
        n += 1;
    }
    assert!(n >= 129, "{n}");
}
