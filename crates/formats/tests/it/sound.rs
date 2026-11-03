//! FSB5 and FEV readers against the player's own sound banks. Skips without `cache/raw/sound`.

use std::path::PathBuf;

use sekiro_formats::fev::{Fev, sound_stem};
use sekiro_formats::fsb::{CODEC_VORBIS, Fsb, is_fsb5, vorbis_packets};

fn sound_dir() -> Option<PathBuf> {
    let d = sekiro_formats::cache::root().join("raw/sound");
    d.is_dir().then_some(d)
}

#[test]
fn soldier_bank_samples() {
    let Some(dir) = sound_dir() else { return };
    let data = std::fs::read(dir.join("c1010.fsb")).unwrap();
    let fsb = Fsb::parse(&data).unwrap();
    assert_eq!(fsb.codec, CODEC_VORBIS);
    assert_eq!(fsb.samples.len(), 308);
    let s = fsb.samples.iter().find(|s| s.name == "c101001001").unwrap();
    assert_eq!((s.rate, s.channels), (44100, 1));
    assert!(s.vorbis_crc.is_some());
    // About 0.34 s, as the event project lists.
    assert!((14_000..16_000).contains(&s.frames), "{}", s.frames);
    let packets = vorbis_packets(&data[s.data.clone()]);
    assert!(packets.len() > 10);
}

#[test]
fn every_plain_bank_parses_and_the_rest_are_encrypted() {
    let Some(dir) = sound_dir() else { return };
    let (mut plain, mut encrypted) = (0, 0);
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().extension().is_none_or(|x| x != "fsb") {
            continue;
        }
        let data = std::fs::read(e.path()).unwrap();
        if is_fsb5(&data) {
            Fsb::parse(&data).unwrap();
            plain += 1;
        } else {
            encrypted += 1;
        }
    }
    assert_eq!((plain, encrypted), (112, 62));
}

#[test]
fn soldier_events_map_to_waves() {
    let Some(dir) = sound_dir() else { return };
    let fev = Fev::parse(&std::fs::read(dir.join("c1010.fev")).unwrap()).unwrap();
    assert!(fev.event_names().any(|e| e == "c101001001"));
    let waves = fev.event_waves("c101001001");
    let names: Vec<&str> = waves.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["c101001001", "c101001001b", "c101001001c"]);
    assert_eq!(waves[0].length_ms, 340);
    assert_eq!(sound_stem("c101001001b"), "c101001001");
    assert!(fev.sound_defs.len() >= 70);
}
