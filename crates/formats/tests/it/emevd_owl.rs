//! Reads the invaded Ashina Castle's event script (`event/m11_01_00_00.emevd`) and checks the
//! Great Shinobi Owl's phase-2 event (see `docs/OWL.md`, "Phase 2"). Skips when `cache/` lacks
//! it (`sekiro-extract unpack`).

use sekiro_formats::emedf::{self, Arg};
use sekiro_formats::emevd::{Emevd, RestBehavior};

fn load() -> Option<Emevd> {
    let path = sekiro_formats::cache::root().join("raw/event/m11_01_00_00.emevd");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: {} is not extracted", path.display());
        return None;
    };
    Some(Emevd::parse(&bytes).unwrap())
}

fn named(e: &Emevd, event: i64) -> Vec<(String, Vec<i64>)> {
    e.event(event)
        .unwrap()
        .instructions
        .iter()
        .map(|i| match emedf::decode(i) {
            Some((d, args)) => (
                d.name.to_owned(),
                args.iter().map(|(_, a)| Arg::as_i64(*a)).collect(),
            ),
            None => (format!("{}[{}]", i.bank, i.id), Vec::new()),
        })
        .collect()
}

#[test]
fn owl_phase_two_event_reads_back() {
    let Some(e) = load() else { return };
    assert_eq!(e.events.len(), 162);
    assert!(e.linked_file_names()[0].ends_with("common_func.emevd"));
    let phase = named(&e, 11_115_870);
    assert_eq!(phase.len(), 18);
    let has = |name: &str, args: &[i64]| {
        phase
            .iter()
            .any(|(n, a)| n == name && a.len() >= args.len() && a[..args.len()] == *args)
    };
    // Waits for message 20 on him (his first get-up), then: boss bar off, HKS command 506000,
    // SpEffect 200051, and the bar back on after the heat-up.
    assert!(has("IfCharacterHasEventMessage", &[0, 1_110_850, 20]));
    assert!(has("DisplayBossHealthBar", &[0, 1_110_850, 0, 905_060]));
    assert!(has("EzstateInstructionRequest", &[1_110_850, 506_000, 1]));
    assert!(has("SetSpEffect", &[1_110_850, 200_051]));
    assert!(has(
        "IfCharacterHasSpeffect",
        &[-1, 1_110_850, 3_506_080, 0]
    ));
    assert!(has("DisplayBossHealthBar", &[1, 1_110_850, 0, 905_060]));
    assert_eq!(e.event(11_115_870).unwrap().rest, RestBehavior::Default);

    // The fight's set-up event is started with the fighting Owl (X0) and his conversation copy
    // (X4); its boss bar takes the name from X0.
    let starts = e.starts_of(11_115_860);
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].arg_i32(0), Some(1_110_850));
    assert_eq!(starts[0].arg_i32(1), Some(1_110_722));
    let ev = e.event(11_115_860).unwrap();
    let resolved = ev.resolve(&starts[0].args);
    let bar = resolved.iter().find(|i| i.code() == (2003, 11)).unwrap();
    assert_eq!(bar.i32_at(4), Some(1_110_850));
    assert_eq!(bar.i32_at(12), Some(905_060));
}
