//! Deathblow data checks against the extracted cache (skipped when `cache/` is missing).

use std::path::PathBuf;

use sekiro_formats::anim::{AnimClip, Skeleton};
use sekiro_sim::deathblow::{DummyFrame, Placement, Situation, ThrowTable, ThrowTimes, select};

fn cache() -> Option<PathBuf> {
    let c = sekiro_sim::cache::root();
    c.join("raw/param").exists().then_some(c)
}

fn clip(cache: &std::path::Path, chr: &str, name: &str) -> AnimClip {
    let bytes = std::fs::read(cache.join(format!("anim/{chr}/{name}.bin"))).unwrap();
    AnimClip::from_bytes(&bytes).unwrap()
}

#[test]
fn soldier_front_deathblow_rows_and_timing() {
    let Some(cache) = cache() else { return };
    let table = ThrowTable::load_from_cache(&cache).unwrap();
    let wolf = Placement::default();
    let soldier = |z: f32| Placement {
        position: [0.0, 0.0, z],
        yaw: std::f32::consts::PI,
    };
    let (start, body) = select(
        &table,
        1010,
        0,
        Situation::PostureBroken,
        &wolf,
        &soldier(-2.4),
    )
    .expect("far front rows");
    assert_eq!((start.id, body.id), (11010000, 11010001));
    assert_eq!(start.attacker_anim(), "a200_500000");
    assert_eq!(body.attacker_anim(), "a200_510000");
    assert_eq!(body.defender_death_anim().as_deref(), Some("a000_012001"));
    assert_eq!(body.atk_sorb_dummy, 267);
    let (near, _) = select(
        &table,
        1010,
        0,
        Situation::PostureBroken,
        &wolf,
        &soldier(-1.0),
    )
    .unwrap();
    assert_eq!(near.id, 11010005);
    // From behind an unaware soldier.
    let unaware = Placement {
        position: [0.0, 0.0, -1.5],
        yaw: 0.0,
    };
    let (back, _) = select(&table, 1010, 0, Situation::Unaware, &wolf, &unaware).unwrap();
    assert_eq!(back.id, 11010020);

    let times = ThrowTimes::load(
        &cache,
        "c0000",
        "a200_500000",
        "a200_510000",
        Some(("c1010", "a000_012000")),
    )
    .unwrap();
    assert!(
        times.death_branch.is_some_and(|b| (b - 1.667).abs() < 0.01),
        "{times:?}"
    );
    assert!((times.grab.0 - 0.2667).abs() < 0.01, "{times:?}");
    assert!((times.damage - 0.9).abs() < 0.01, "{times:?}");

    let data = std::fs::read(cache.join("raw/chr/c0000.chrbnd.d/c0000.flver")).unwrap();
    let flver = sekiro_formats::flver::parse(&data).unwrap();
    let sorb = DummyFrame::from_flver(&flver, 267).unwrap();
    assert!((sorb.position[2] + 1.2).abs() < 1e-3, "{sorb:?}");
    assert!(
        (sorb.yaw.abs() - std::f32::consts::PI).abs() < 1e-3,
        "{sorb:?}"
    );
}

/// The death variant continues the survive variant: its first frame is 12000 at 1.67 s, where
/// 12000 has a one-frame ChrActionFlag 69 event (the branch point the engine switches at).
#[test]
fn death_variant_branches_from_the_survive_clip() {
    let Some(cache) = cache() else { return };
    let skel = Skeleton::from_bytes(&std::fs::read(cache.join("anim/c1010/skeleton.bin")).unwrap())
        .unwrap();
    let live = clip(&cache, "c1010", "a000_012000");
    let dead = clip(&cache, "c1010", "a000_012001");
    // Body bones only: camera and IK helper bones move metres and are not part of the pose.
    let bones: Vec<usize> = [
        "Pelvis", "Spine2", "Head", "R_Hand", "L_Hand", "R_Foot", "L_Foot",
    ]
    .iter()
    .filter_map(|n| skel.bone_index(n))
    .collect();
    let diff = |t_live: f32, t_dead: f32| {
        let a = skel.model_space(&live.sample(t_live, &skel));
        let b = skel.model_space(&dead.sample(t_dead, &skel));
        bones
            .iter()
            .map(|&i| (a[i].w_axis - b[i].w_axis).truncate().length())
            .fold(0.0f32, f32::max)
    };
    let best = (0..=165)
        .map(|f| f as f32 / 30.0)
        .map(|t| (t, diff(t, 0.0)))
        .fold((0.0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a });
    eprintln!(
        "12001 start best matches 12000 at {:.2}s (gap {:.3} m)",
        best.0, best.1
    );
    assert!((best.0 - 1.667).abs() < 0.04 && best.1 < 0.01, "{best:?}");
}

/// How far the defender's own root motion drifts from the attacker's sorb dummy over the body
/// clip (a rough forward-only comparison; it prints, it does not assert). The duel no longer
/// holds the defender on the dummy: both play their own root motion after the absorb, as FwLo's
/// RustSekiro matched against live recordings.
#[test]
fn defender_root_motion_versus_sorb_lock() {
    let Some(cache) = cache() else { return };
    let atk = clip(&cache, "c0000", "a200_510000");
    let live = clip(&cache, "c1010", "a000_012000");
    let def = clip(&cache, "c1010", "a000_012001");
    let (Some(ra), Some(rl), Some(rd)) = (&atk.root_motion, &live.root_motion, &def.root_motion)
    else {
        panic!("root motion missing");
    };
    for t in [0.5f32, 0.9, 1.5, 2.0, 2.6] {
        let a = ra.sample(t);
        let d = if t < 1.667 {
            rl.sample(t)
        } else {
            let b = rl.sample(1.667);
            let e = rd.sample(t - 1.667);
            [b[0] + e[0], b[1] + e[1], b[2] + e[2], b[3] + e[3]]
        };
        // Attacker moves along its -Z; the defender faces it, so the same world motion is
        // +Z in the defender's frame.
        let gap = (-a[2] - d[2]).abs();
        eprintln!(
            "t={t}: attacker z {:.3}, defender z {:.3}, gap {gap:.3}",
            a[2], d[2]
        );
    }
}

/// The running fight reads throw times from the characters' decoded TAE (reading the half-second
/// `tae.json` parse at the deathblow start froze the game); both sources must agree.
#[test]
fn throw_times_from_tae_db_match_the_json_export() {
    let Some(cache) = cache() else { return };
    if !cache.join("anim/c5060/tae.json").is_file() {
        return;
    }
    let wolf = sekiro_sim::tae::TaeDb::load_from_cache(&cache, "c0000");
    for (chr, start, body, def) in [
        ("c1010", "a200_500000", "a200_510000", "a000_012000"),
        ("c5060", "a221_500000", "a221_510000", "a000_012000"),
        ("c5060", "a221_501700", "a221_511700", "a000_013700"),
    ] {
        let enemy = sekiro_sim::tae::TaeDb::load_from_cache(&cache, chr);
        let json = ThrowTimes::load(&cache, "c0000", start, body, Some((chr, def)));
        let db = ThrowTimes::from_tae_db(&wolf, start, body, Some((&enemy, def)));
        assert!(json.is_some(), "{start}");
        assert_eq!(json, db, "{chr} {start} {body} {def}");
    }
}
