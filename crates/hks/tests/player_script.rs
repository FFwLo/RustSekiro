//! Runs the installed player scripts in the VM. Skips when `cache/` has not been extracted.

use sekiro_hks::{CommandId, Host, LoggingHost, PLAYER_SCRIPTS, Value, Vm};
use std::path::{Path, PathBuf};

fn script_dir() -> Option<PathBuf> {
    let dir = sekiro_formats::cache::root().join("raw/action/script");
    if dir.join("c0000.hks").exists() {
        Some(dir)
    } else {
        eprintln!("skipping: {} not extracted", dir.display());
        None
    }
}

fn load_player(dir: &Path, host: &mut dyn Host) -> Vm {
    let mut vm = Vm::new();
    vm.seed(1);
    for chunk in PLAYER_SCRIPTS {
        let data = std::fs::read(dir.join(chunk)).unwrap();
        vm.load(host, chunk, &data).unwrap();
    }
    vm.call_global(host, "Initialize", &[]).unwrap();
    vm
}

fn fired_events(host: &LoggingHost) -> Vec<String> {
    host.log
        .iter()
        .filter(|c| c.function == "hkbFireEvent")
        .map(|c| c.args[0].to_display())
        .collect()
}

/// A living, grounded character with the katana drawn.
fn standing_host(vm_constants: &Vm) -> LoggingHost {
    let constant = |name: &str| vm_constants.get_global(name).as_number().expect(name);
    let mut host = LoggingHost::default();
    host.set_env(1000, None, 100); // HP
    host.set_env(201, None, 1); // landed
    host.set_env(248, None, 1); // really landed
    host.set_env(3037, None, 1); // may leave crouch
    host.set_env(207, None, constant("ARM_STYLE_ONE_HAND"));
    host.set_env(3033, Some(constant("ACTION_UNLOCK_TYPE_MAIN_WEAPONE")), 1);
    host.set_env(
        225,
        Some(constant("HAND_RIGHT")),
        constant("WEP_MOTION_CATEGORY_050"),
    );
    host
}

#[test]
fn idle_frame_runs_and_guard_press_starts_deflect() {
    let Some(dir) = script_dir() else { return };
    let mut boot = LoggingHost::default();
    let mut vm = load_player(&dir, &mut boot);

    let mut host = standing_host(&vm);
    vm.call_global(&mut host, "Update", &[]).unwrap();
    vm.call_global(&mut host, "StandIdle_onUpdate", &[])
        .unwrap();
    assert!(
        !fired_events(&host).iter().any(|e| e.contains("Guard")),
        "{:?}",
        fired_events(&host)
    );

    let guard = vm.get_global("ACTION_ARM_GUARD").as_number().unwrap();
    host.log.clear();
    host.set_env(1106, Some(guard), 1); // guard requested this frame
    host.set_env(1108, Some(guard), 1); // guard held
    vm.call_global(&mut host, "Update", &[]).unwrap();
    vm.call_global(&mut host, "StandIdle_onUpdate", &[])
        .unwrap();
    assert_eq!(
        fired_events(&host).last().map(String::as_str),
        Some("W_StandToDeflectGuard")
    );
}

/// A host that answers every query with a pseudo-random small integer, to push the scripts
/// down many different branches.
struct NoisyHost {
    state: u64,
    calls: usize,
}

impl NoisyHost {
    fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state % 3) as f32
    }
}

impl Host for NoisyHost {
    fn env(&mut self, _id: CommandId<'_>, _args: &[Value]) -> Value {
        self.calls += 1;
        Value::Number(self.next())
    }

    fn act(&mut self, _id: CommandId<'_>, _args: &[Value]) -> Value {
        self.calls += 1;
        Value::Nil
    }

    fn call(&mut self, name: &str, _args: &[Value]) -> Vec<Value> {
        self.calls += 1;
        match name {
            "hkbGetVariable" => vec![Value::Number(self.next())],
            "hkbIsNodeActive" => vec![Value::Bool(self.next() > 1.0)],
            _ => Vec::new(),
        }
    }
}

/// Every behaviour-state callback runs without a VM error under varied engine answers.
///
/// Two callbacks fail in the shipped scripts themselves: their state constant is either never
/// defined or missing from the state parameter table, so `Control` indexes nil. The game would
/// raise the same script error if those states were ever entered.
#[test]
fn every_state_callback_runs() {
    let Some(dir) = script_dir() else { return };
    let mut host = NoisyHost {
        state: 0x9e37_79b9_7f4a_7c15,
        calls: 0,
    };
    let mut vm = load_player(&dir, &mut host);
    let names: Vec<String> = {
        let globals = vm.globals().borrow();
        let mut out = Vec::new();
        let mut key = Value::Nil;
        while let Some((k, v)) = globals.next(&key).unwrap() {
            if let (Some(name), Value::Function(_)) = (k.as_str(), &v)
                && (name.ends_with("_onUpdate")
                    || name.ends_with("_onActivate")
                    || name.ends_with("_onDeactivate"))
            {
                out.push(name.to_owned());
            }
            key = k;
        }
        out
    };
    assert!(names.len() > 3000, "{} callbacks", names.len());
    const SCRIPT_BUGS: [&str; 2] = ["StandBreakKick_onUpdate", "SwimDeflectBreak_onUpdate"];
    let mut failures = Vec::new();
    for round in 0..3 {
        for name in &names {
            if let Err(e) = vm.call_global(&mut host, name, &[])
                && !SCRIPT_BUGS.contains(&name.as_str())
            {
                failures.push(format!("round {round} {name}: {e}"));
            }
        }
    }
    for name in SCRIPT_BUGS {
        assert!(
            vm.call_global(&mut host, name, &[]).is_err(),
            "{name} now succeeds"
        );
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(host.calls > 100_000, "{} engine calls", host.calls);
    eprintln!(
        "{} callbacks x3, {} engine calls, {} instructions",
        names.len(),
        host.calls,
        vm.instructions
    );
}

/// The main chunk of every installed script loads (defines its globals) without error.
#[test]
fn every_script_main_chunk_loads() {
    let Some(dir) = script_dir() else { return };
    let mut host = LoggingHost::default();
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("hks") {
            continue;
        }
        let mut vm = Vm::new();
        let data = std::fs::read(&path).unwrap();
        vm.load(
            &mut host,
            &path.file_name().unwrap().to_string_lossy(),
            &data,
        )
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        count += 1;
    }
    assert_eq!(count, 81);
}
