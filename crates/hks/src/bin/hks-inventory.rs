//! `hks-inventory [script_dir] [chunk...]`: lists the engine interface a script set uses.
//!
//! Prints engine globals (read but never defined), and per `env`/`act` id the call count,
//! argument shapes and calling functions. Defaults to the player scripts in
//! `cache/raw/action/script`.

use sekiro_hks::inventory::{self, Arg, CallSite, GlobalUse};
use sekiro_hks::{LoggingHost, PLAYER_SCRIPTS, Value, Vm};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const STDLIB: &[&str] = &[
    "_G",
    "math",
    "table",
    "string",
    "ipairs",
    "pairs",
    "setmetatable",
    "collectgarbage",
    "print",
    "type",
    "tostring",
    "tonumber",
    "next",
    "select",
    "unpack",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(
        args.get(1)
            .map_or("cache/raw/action/script", String::as_str),
    );
    let chunks: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        PLAYER_SCRIPTS.iter().map(|s| s.to_string()).collect()
    };

    let mut vm = Vm::new();
    let mut host = LoggingHost::default();
    let mut calls = Vec::new();
    let mut globals = GlobalUse::default();
    let mut opcodes: BTreeMap<&str, usize> = BTreeMap::new();
    for chunk in &chunks {
        let data = std::fs::read(dir.join(chunk)).expect("read script");
        let file = sekiro_hks::bytecode::parse(&data).expect("parse");
        inventory::scan(&file.main, chunk, &mut calls, &mut globals);
        file.main.walk(&mut |p| {
            for ins in &p.code {
                if let Some(op) = ins.op() {
                    *opcodes.entry(op.name()).or_default() += 1;
                }
            }
        });
        vm.load(&mut host, chunk, &data).expect("load");
    }

    println!("# opcodes");
    for (op, n) in &opcodes {
        println!("{op}	{n}");
    }

    let engine: BTreeSet<&String> = globals
        .read
        .iter()
        .filter(|g| !globals.written.contains(*g) && !STDLIB.contains(&g.as_str()))
        .collect();
    println!("# engine globals (read, never defined)");
    for g in &engine {
        let n = calls
            .iter()
            .filter(|c| c.callee.as_deref() == Some(g.as_str()))
            .count();
        println!("{g}\tcalls={n}");
    }

    println!("\n# calls by callee (engine and stdlib only)");
    let mut by_callee: BTreeMap<String, usize> = BTreeMap::new();
    for c in &calls {
        let name = c.callee.clone().unwrap_or_else(|| "<computed>".into());
        if !globals.written.contains(&name) {
            *by_callee.entry(name).or_default() += 1;
        }
    }
    for (k, v) in &by_callee {
        println!("{k}\t{v}");
    }

    for which in ["env", "act"] {
        println!("\n# {which}");
        report(which, &calls, &vm);
    }

    println!("\n# hkb event and variable names");
    for f in [
        "hkbFireEvent",
        "hkbGetVariable",
        "hkbSetVariable",
        "hkbIsNodeActive",
    ] {
        let mut names: BTreeMap<String, usize> = BTreeMap::new();
        let mut computed = 0;
        for c in calls.iter().filter(|c| c.callee.as_deref() == Some(f)) {
            match c.args.as_ref().and_then(|a| a.first()) {
                Some(Arg::Str(s)) => *names.entry(s.clone()).or_default() += 1,
                _ => computed += 1,
            }
        }
        println!("{f}: {} literal names, {computed} computed", names.len());
    }
    // Script wrappers that forward to the engine with a literal name.
    for f in [
        "FireEvent",
        "FireEventNoReset",
        "SetVariable",
        "IsNodeActive",
    ] {
        let n = calls
            .iter()
            .filter(|c| c.callee.as_deref() == Some(f))
            .count();
        println!("wrapper {f}: {n} call sites");
    }
}

fn describe(arg: &Arg, vm: &Vm) -> String {
    match arg {
        Arg::Number(n) => format!("{n}"),
        Arg::Str(s) => format!("{s:?}"),
        Arg::Bool(b) => format!("{b}"),
        Arg::Nil => "nil".into(),
        Arg::Global(g) => match vm.get_global(g) {
            Value::Number(n) => format!("{g}={n}"),
            Value::Nil => format!("{g}=?"),
            other => format!("{g}:{}", other.type_name()),
        },
        Arg::Computed => "expr".into(),
    }
}

/// The id of an env/act call: a literal, or a global constant resolved through the VM.
fn id_of(c: &CallSite, vm: &Vm) -> String {
    match c.args.as_ref().and_then(|a| a.first()) {
        Some(Arg::Number(n)) => format!("{n}"),
        Some(Arg::Str(s)) => format!("{s:?}"),
        Some(Arg::Global(g)) => match vm.get_global(g) {
            Value::Number(n) => format!("{n}"),
            _ => format!("<{g}>"),
        },
        _ => "<computed>".into(),
    }
}

fn report(which: &str, calls: &[CallSite], vm: &Vm) {
    let mut by_id: BTreeMap<String, Vec<&CallSite>> = BTreeMap::new();
    for c in calls.iter().filter(|c| c.callee.as_deref() == Some(which)) {
        by_id.entry(id_of(c, vm)).or_default().push(c);
    }
    let mut ids: Vec<_> = by_id.into_iter().collect();
    ids.sort_by(|a, b| {
        let num = |s: &str| s.parse::<f64>().unwrap_or(f64::MAX);
        num(&a.0).total_cmp(&num(&b.0)).then(a.0.cmp(&b.0))
    });
    for (id, sites) in ids {
        let mut arities: BTreeSet<String> = BTreeSet::new();
        let mut examples: BTreeSet<String> = BTreeSet::new();
        let mut functions: BTreeSet<String> = BTreeSet::new();
        for s in &sites {
            functions.insert(s.function.clone());
            match &s.args {
                Some(a) => {
                    arities.insert((a.len().saturating_sub(1)).to_string());
                    if a.len() > 1 {
                        let rest: Vec<String> = a[1..].iter().map(|x| describe(x, vm)).collect();
                        examples.insert(rest.join(", "));
                    }
                }
                None => {
                    arities.insert("var".into());
                }
            }
        }
        let distinct = examples.len();
        let ex: Vec<_> = examples.into_iter().take(4).collect();
        let fns: Vec<_> = functions.into_iter().take(4).collect();
        println!(
            "{id}\tcount={}\targs={}\tdistinct={distinct}\te.g. [{}]\tin {}",
            sites.len(),
            arities.into_iter().collect::<Vec<_>>().join("/"),
            ex.join(" | "),
            fns.join(",")
        );
    }
}
