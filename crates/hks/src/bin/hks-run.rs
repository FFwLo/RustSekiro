//! `hks-run [--dir D] [--frames N] [--state S] [--guard] [--raw]`: loads the player scripts and
//! runs frames against a logging stub host, printing every engine call.
//!
//! One frame is `Update()` followed by `<state>_onUpdate()` (default `StandIdle`), which is
//! what the behaviour graph does for the active state. Unless `--raw` is given, the stub host
//! reports a living, grounded, standing character (HP 100, landed, may stand up). `--guard`
//! additionally reports the guard button as requested.

use sekiro_hks::{LoggingHost, PLAYER_SCRIPTS, Vm};
use std::path::PathBuf;

// Engine ids, see docs/HKS-INTERFACE.md.
const ENV_LANDED: i32 = 201;
const ENV_REALLY_LANDED: i32 = 248;
const ENV_HP: i32 = 1000;
const ENV_ARM_STYLE: i32 = 207;
const ENV_ACTION_REQUEST: i32 = 1106;
const ENV_ACTION_HELD: i32 = 1108;
const ENV_WEAPON_CATEGORY: i32 = 225;
const ENV_ACTION_UNLOCKED: i32 = 3033;
const ENV_CAN_RELEASE_CROUCH: i32 = 3037;
const ACTION_ARM_GUARD: f32 = 2.0;

fn main() {
    let mut dir = sekiro_formats::cache::root().join("raw/action/script");
    let mut frames = 1usize;
    let mut state = "StandIdle".to_owned();
    let mut guard = false;
    let mut raw = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = PathBuf::from(args.next().expect("--dir value")),
            "--frames" => {
                frames = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .expect("--frames N")
            }
            "--state" => state = args.next().expect("--state value"),
            "--guard" => guard = true,
            "--raw" => raw = true,
            other => panic!("unknown argument {other}"),
        }
    }

    let mut vm = Vm::new();
    vm.seed(1);
    let mut host = LoggingHost::default();
    if !raw {
        host.set_env(ENV_HP, None, 100);
        host.set_env(ENV_LANDED, None, 1);
        host.set_env(ENV_REALLY_LANDED, None, 1);
        host.set_env(ENV_CAN_RELEASE_CROUCH, None, 1);
    }
    for chunk in PLAYER_SCRIPTS {
        let data = std::fs::read(dir.join(chunk)).expect("read script");
        vm.load(&mut host, chunk, &data).expect("load");
    }
    println!(
        "loaded {} scripts, {} engine calls during load",
        PLAYER_SCRIPTS.len(),
        host.log.len()
    );
    host.log.clear();

    if guard {
        // Guard requested and held, weapon drawn (not the safe arm style), main weapon
        // (the katana) unlocked and equipped. The
        // unlock type and weapon category ids are script constants, read from the loaded VM.
        let constant = |name: &str| vm.get_global(name).as_number().expect(name);
        host.set_env(ENV_ARM_STYLE, None, constant("ARM_STYLE_ONE_HAND"));
        host.set_env(ENV_ACTION_REQUEST, Some(ACTION_ARM_GUARD), 1);
        host.set_env(ENV_ACTION_HELD, Some(ACTION_ARM_GUARD), 1);
        host.set_env(
            ENV_ACTION_UNLOCKED,
            Some(constant("ACTION_UNLOCK_TYPE_MAIN_WEAPONE")),
            1,
        );
        host.set_env(
            ENV_WEAPON_CATEGORY,
            Some(constant("HAND_RIGHT")),
            constant("WEP_MOTION_CATEGORY_050"),
        );
    }

    let run = |vm: &mut Vm, host: &mut LoggingHost, name: &str| {
        let before = vm.instructions;
        match vm.call_global(host, name, &[]) {
            Ok(_) => println!(
                "== {name}: {} instructions, {} engine calls",
                vm.instructions - before,
                host.log.len()
            ),
            Err(e) => println!("== {name}: error: {e}"),
        }
        for call in host.log.drain(..) {
            println!("   {call}");
        }
    };
    run(&mut vm, &mut host, "Initialize");
    for frame in 0..frames {
        println!("-- frame {frame}");
        run(&mut vm, &mut host, "Update");
        run(&mut vm, &mut host, &format!("{state}_onUpdate"));
    }
}
