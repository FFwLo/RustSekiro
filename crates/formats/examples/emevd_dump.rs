//! Prints EMEVD events with their instructions named from the Sekiro instruction table.
//!
//! ```text
//! cargo run -p sekiro-formats --example emevd_dump -- cache/raw/event/m11_01_00_00.emevd 11115860 11115870
//! cargo run -p sekiro-formats --example emevd_dump -- cache/raw/event/m11_01_00_00.emevd --find 1110850 200051
//! ```
//!
//! With `--find`, lists the events whose instructions or starts mention any of the numbers.

use sekiro_formats::{emedf, emevd::Emevd};

fn words(b: &[u8]) -> Vec<i32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|b| i32::from_le_bytes(*b))
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: emevd_dump <file.emevd> [--find] <numbers...>");
    let rest: Vec<String> = args.collect();
    let find = rest.first().is_some_and(|a| a == "--find");
    let numbers: Vec<i64> = rest
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.parse().expect("number"))
        .collect();
    let data = std::fs::read(&path).expect("read");
    let e = Emevd::parse(&data).expect("parse");

    if find {
        println!(
            "{} events, linked {:?}",
            e.events.len(),
            e.linked_file_names()
        );
        for ev in &e.events {
            let hit = |n: i64| {
                ev.instructions
                    .iter()
                    .any(|i| i.words().any(|w| w as i64 == n))
                    || e.starts_of(ev.id)
                        .iter()
                        .any(|s| words(&s.args).iter().any(|&w| w as i64 == n))
            };
            let found: Vec<i64> = numbers.iter().copied().filter(|&n| hit(n)).collect();
            if !found.is_empty() {
                println!(
                    "event {} ({} instructions): {found:?}",
                    ev.id,
                    ev.instructions.len()
                );
            }
        }
        return;
    }

    for id in numbers {
        let Some(ev) = e.event(id) else {
            println!("event {id}: not in this file");
            continue;
        };
        println!("event {} (rest {:?})", ev.id, ev.rest);
        for s in e.starts_of(id) {
            println!(
                "  started by event {} slot {} with {:?}",
                s.from_event,
                s.slot,
                words(&s.args)
            );
        }
        for (k, ins) in ev.instructions.iter().enumerate() {
            let subs: Vec<String> = ev
                .parameters_of(k)
                .map(|p| {
                    format!(
                        "bytes {}..{} <- X{}",
                        p.target_start,
                        p.target_start + p.byte_count,
                        p.source_start
                    )
                })
                .collect();
            let subs = if subs.is_empty() {
                String::new()
            } else {
                format!("   [{}]", subs.join(", "))
            };
            println!("  {k:3} {}{subs}", emedf::format(ins));
        }
    }
}
