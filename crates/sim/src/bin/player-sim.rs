//! `player-sim [hold|repeat] [--cache DIR] [--calls]`: runs a guard scenario with the real
//! player scripts and behaviour graph and prints state and animation per frame.

use sekiro_sim::PlayerBehavior;
use sekiro_sim::scenario;
use std::path::PathBuf;

fn main() {
    let mut which = "hold".to_owned();
    let mut cache = sekiro_sim::cache::root();
    let mut calls = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--cache" => cache = PathBuf::from(args.next().expect("--cache DIR")),
            "--calls" => calls = true,
            other => which = other.to_owned(),
        }
    }
    let mut player = PlayerBehavior::load_from_cache(&cache).expect("load player");
    let warmup = scenario::settle_to_idle(&mut player, 300);
    println!("settled to idle after {warmup} frames");
    if calls {
        player.call_log = Some(Vec::new());
    }
    let (inputs, combo) = match which.as_str() {
        "repeat" => (scenario::guard_repeat(), true),
        _ => (scenario::guard_hold(), false),
    };
    let reports = scenario::run(&mut player, &inputs, combo);
    for (r, input) in reports.iter().zip(&inputs) {
        let line = format!(
            "{:<45} {:<28}",
            r.state_path.join("/"),
            r.animation.as_deref().unwrap_or("-"),
        );
        println!(
            "{:4} {:<9} {line} t={:5.2}{}{}",
            r.frame,
            format!("{input:?}"),
            r.anim_time,
            if r.events.is_empty() {
                String::new()
            } else {
                format!(" events={:?}", r.events)
            },
            if r.script_errors.is_empty() {
                String::new()
            } else {
                format!(" ERRORS={:?}", r.script_errors)
            },
        );
    }
    if let Some(log) = &player.call_log {
        eprintln!("{} engine calls logged", log.len());
    }
    let unhandled: Vec<_> = player.runtime.unhandled_events.iter().collect();
    if !unhandled.is_empty() {
        println!("unhandled events: {unhandled:?}");
    }
}
