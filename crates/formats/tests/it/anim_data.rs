//! Checks the HKX and TAE readers against the player's own unpacked game data in `cache/raw`.
//! Expected values come from the earlier sekiro-deflection project (docs/ANIMATION-DECODER-AUDIT.md,
//! LOCOMOTION-AUDIT.md, BASELINE-COMBAT-FIXTURE.md). Skips when `cache/` has not been extracted.

use std::path::PathBuf;

use glam::Quat;
use sekiro_formats::anim::{AnimClip, BlendHint, Skeleton};
use sekiro_formats::hkx::{self, Compendium, DecodeStats, TagFile, tagfile};
use sekiro_formats::tae::{self, FieldValue, Template};

fn chr() -> PathBuf {
    sekiro_formats::cache::root().join("raw/chr")
}

fn read(rel: &str) -> Option<Vec<u8>> {
    let p = chr().join(rel);
    match std::fs::read(&p) {
        Ok(d) => Some(d),
        Err(_) => {
            eprintln!("skipping: {} not extracted", p.display());
            None
        }
    }
}

/// Parses `dir/file`, using the folder's compendium when the file references one.
fn tagfile_in(dir: &str, file: &str) -> Option<TagFile> {
    let data = read(&format!("{dir}/{file}"))?;
    let comp = match tagfile::compendium_ref(&data).unwrap() {
        Some(_) => {
            let path = std::fs::read_dir(chr().join(dir))
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.extension().is_some_and(|e| e == "compendium"))
                .expect("compendium");
            Some(Compendium::parse(&std::fs::read(path).unwrap()).unwrap())
        }
        None => None,
    };
    Some(TagFile::parse(&data, comp.as_ref()).unwrap())
}

fn skeleton(dir: &str, file: &str) -> Option<Skeleton> {
    Some(hkx::skeleton_from_tagfile(&tagfile_in(dir, file)?).unwrap())
}

fn clip(dir: &str, name: &str, bones: usize) -> Option<AnimClip> {
    let f = tagfile_in(dir, &format!("{name}.hkx"))?;
    let mut stats = DecodeStats::default();
    Some(hkx::clip_from_tagfile(&f, name, bones, &mut stats).unwrap())
}

#[test]
#[ignore = "debug dump: HKX=<dir>/<file> cargo test -p sekiro-formats --test it -- anim_data::dump --ignored --nocapture"]
fn dump() {
    let Ok(path) = std::env::var("HKX") else {
        return;
    };
    let (dir, file) = path.rsplit_once('/').unwrap();
    let f = tagfile_in(dir, file).unwrap();
    println!("{f:?}");
    for o in f.objects() {
        println!("{}", o.describe(0));
    }
}

/// Converts decoded clips in cache/anim to JSON for cross-checking with the Python decoder:
/// `BINS=c0000/a000_000200,... OUT=<dir> cargo test -p sekiro-formats --test it -- anim_data::bin_to_json --ignored`.
#[test]
#[ignore = "debug export"]
fn bin_to_json() {
    let (Ok(out), Ok(bins)) = (std::env::var("OUT"), std::env::var("BINS")) else {
        return;
    };
    let out = PathBuf::from(out);
    for rel in bins.split(',') {
        let p = chr().join("../../anim").join(format!("{rel}.bin"));
        let clip = AnimClip::from_bytes(&std::fs::read(&p).unwrap()).unwrap();
        std::fs::write(
            out.join(format!("{}.json", rel.replace('/', "_"))),
            serde_json::to_string(&clip).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn wolf_skeleton() {
    let Some(s) = skeleton("c0000.anibnd.d", "skeleton.hkx") else {
        return;
    };
    assert_eq!(s.name, "c0000");
    assert_eq!(s.bones.len(), 146);
    assert_eq!(s.bones.iter().filter(|b| b.parent.is_none()).count(), 1);
    assert!(s.bone_index("R_Weapon").is_some());
    // Reference pose stands upright in source units: head well above the feet.
    let m = s.model_space(&s.reference_pose());
    let y = |n: &str| m[s.bone_index(n).unwrap()].w_axis.y;
    assert!(
        y("Head") > 1.4 && y("L_Foot") < 0.2,
        "{} {}",
        y("Head"),
        y("L_Foot")
    );
}

#[test]
fn soldier_skeleton() {
    let Some(s) = skeleton("c1010.anibnd.d", "skeleton.HKX") else {
        return;
    };
    assert_eq!(s.bones.len(), 127);
    // The serialized form round-trips.
    assert_eq!(Skeleton::from_bytes(&s.to_bytes().unwrap()).unwrap(), s);
}

#[test]
fn guard_start_clip() {
    let Some(c) = clip("c0000_a05x.anibnd.d", "a050_203000", 146) else {
        return;
    };
    assert_eq!(c.frame_count, 16);
    assert!((c.duration - 0.5).abs() < 1e-6);
    assert!((c.fps() - 30.0).abs() < 1e-3);
    assert_eq!(c.tracks.len(), 112);
    assert_eq!(c.blend, BlendHint::Normal);
    assert!(c.root_motion.is_none());
}

#[test]
fn soldier_attack_clip() {
    let Some(c) = clip("c1010.anibnd.d", "a000_003000", 127) else {
        return;
    };
    assert_eq!(c.frame_count, 71);
    assert!((c.duration - 2.333_333_3).abs() < 1e-5);
    let rm = c.root_motion.as_ref().expect("attack carries root motion");
    assert_eq!(rm.samples.len(), 71);
}

#[test]
fn wolf_idle_and_locomotion() {
    let Some(idle) = clip("c0000_a000_lo.anibnd.d", "a000_000000", 146) else {
        return;
    };
    assert_eq!(idle.frame_count, 101);
    assert_eq!(idle.tracks.len(), 114);
    assert!(idle.root_motion.is_none());

    // The walk and run loops that failed in the earlier project decode, with the authored
    // root motion (source -Z is forward).
    let walk = clip("c0000_a000_lo.anibnd.d", "a000_000200", 146).unwrap();
    assert_eq!(walk.frame_count, 81);
    let rm = walk.root_motion.as_ref().unwrap();
    assert_eq!(rm.samples.len(), 81);
    assert!((rm.total()[2] - -4.306_362_6).abs() < 1e-4);
    assert_eq!(rm.up, [0.0, 1.0, 0.0]);
    let run = clip("c0000_a000_lo.anibnd.d", "a000_000500", 146).unwrap();
    assert_eq!(run.frame_count, 41);
    assert!((run.root_motion.unwrap().total()[2] - -7.4).abs() < 1e-4);

    // The earlier failure was its own range check (|translation| <= 5) tripping on the camera
    // dummy bone, which legitimately moves several units. Body bones stay small.
    let s = skeleton("c0000.anibnd.d", "skeleton.hkx").unwrap();
    let cam = s.bone_index("z_dummy_camera").unwrap() as u16;
    let max = |only_cam: bool| {
        walk.tracks
            .iter()
            .filter(|t| (t.bone == cam) == only_cam)
            .flat_map(|t| t.translation.iter().flatten())
            .fold(0.0f32, |m, v| m.max(v.abs()))
    };
    assert!(max(true) > 5.0);
    assert!(max(false) < 5.0);

    // Sampling between frames is continuous.
    let a = walk.sample(1.0, &s);
    let b = walk.sample(1.0 + 1.0 / 300.0, &s);
    for (x, y) in a.iter().zip(&b) {
        assert!(Quat::from_array(x.rotation).angle_between(Quat::from_array(y.rotation)) < 0.05);
    }
}

/// Clips longer than one block (255 frames) stay continuous across block boundaries.
#[test]
fn multi_block_clip_is_continuous() {
    let Some(c) = clip("c0000_c5010.anibnd.d", "a212_510310", 146) else {
        return;
    };
    assert!(c.frame_count > 1000);
    let mut boundary = 0.0f32;
    let mut inside = 0.0f32;
    for t in c.tracks.iter().filter(|t| t.rotation.len() > 1) {
        for f in 1..t.rotation.len() {
            let step =
                Quat::from_array(t.rotation[f - 1]).angle_between(Quat::from_array(t.rotation[f]));
            if f % 255 == 0 {
                boundary = boundary.max(step);
            } else {
                inside = inside.max(step);
            }
        }
    }
    assert!(
        boundary <= inside,
        "boundary step {boundary} vs inside {inside}"
    );
}

fn template() -> Option<Template> {
    let p = chr().join("../../refs/TAE.Template.SDT.xml");
    match std::fs::read_to_string(&p) {
        Ok(t) => Some(Template::parse_xml(&t).unwrap()),
        Err(_) => {
            eprintln!("skipping: {} missing", p.display());
            None
        }
    }
}

#[test]
fn soldier_attack_tae() {
    let (Some(data), Some(t)) = (read("c1010.anibnd.d/c1010.tae"), template()) else {
        return;
    };
    let tae = tae::parse(&data).unwrap();
    assert_eq!(tae.id, 201010);
    let anim = tae.animations.iter().find(|a| a.id == 3000).unwrap();
    let attack = anim.events.iter().find(|e| e.event_type == 1).unwrap();
    assert!((attack.start - 0.666_666_7).abs() < 1e-5);
    assert!((attack.end - 0.8).abs() < 1e-5);
    let d = t.decode(attack).unwrap();
    assert_eq!(d.name, "AttackBehavior");
    assert!(d.complete);
    assert_eq!(d.field("BehaviorJudgeID"), Some(&FieldValue::Int(100)));
}

#[test]
fn wolf_guard_tae_imports() {
    let Some(data) = read("c0000.anibnd.d/a50.tae") else {
        return;
    };
    let tae = tae::parse(&data).unwrap();
    assert_eq!(tae::category_from_file_name("a50.tae"), 50);
    let guard = tae.animations.iter().find(|a| a.id == 203000).unwrap();
    assert_eq!(guard.hkx_source(), 203000);
    // The fourth guard variant plays 203005's motion with its own events.
    let v4 = tae.animations.iter().find(|a| a.id == 203007).unwrap();
    assert_eq!(v4.hkx_source(), 50_203_005);
    assert_eq!(
        tae::anim_name(tae::full_id(50, v4.hkx_source())),
        "a050_203005"
    );
}

/// Every Sekiro TAE in the two milestone characters parses and every event type the template
/// knows decodes without padding mismatches.
#[test]
fn all_tae_events_decode() {
    let Some(t) = template() else {
        return;
    };
    let mut files = 0;
    for dir in ["c0000.anibnd.d", "c1010.anibnd.d"] {
        let Ok(entries) = std::fs::read_dir(chr().join(dir)) else {
            eprintln!("skipping: {dir} not extracted");
            return;
        };
        for p in entries.map(|e| e.unwrap().path()) {
            if p.extension().is_none_or(|e| e != "tae") {
                continue;
            }
            files += 1;
            let tae = tae::parse(&std::fs::read(&p).unwrap()).unwrap();
            for a in &tae.animations {
                for e in &a.events {
                    if let Some(d) = t.decode(e) {
                        assert!(
                            d.assert_mismatches.is_empty(),
                            "{} anim {} event {}: {:?}",
                            p.display(),
                            a.id,
                            e.event_type,
                            d.assert_mismatches
                        );
                    }
                }
            }
        }
    }
    assert!(files > 60, "{files}");
}

/// The Havok reference pose and the FLVER bind pose agree by bone name for all body bones, so a
/// sampled Havok pose can drive the exported model's joints (matched by name, never by index).
#[test]
fn havok_reference_pose_matches_flver_nodes() {
    for (chr_id, skel_dir, skel_file) in [
        ("c0000", "c0000.anibnd.d", "skeleton.hkx"),
        ("c1010", "c1010.anibnd.d", "skeleton.HKX"),
    ] {
        let (Some(s), Some(fl)) = (
            skeleton(skel_dir, skel_file),
            read(&format!("{chr_id}.chrbnd.d/{chr_id}.flver")),
        ) else {
            return;
        };
        let flver = sekiro_formats::flver::parse(&fl).unwrap();
        let mut world = vec![glam::Mat4::IDENTITY; flver.nodes.len()];
        for _ in 0..flver.nodes.len() {
            for (i, n) in flver.nodes.iter().enumerate() {
                let local = n.local_matrix();
                world[i] = match usize::try_from(n.parent) {
                    Ok(p) if p < world.len() => world[p] * local,
                    _ => local,
                };
            }
        }
        let hk = s.model_space(&s.reference_pose());
        let mut matched = 0;
        let mut worst = (0.0f32, String::new());
        for (i, b) in s.bones.iter().enumerate() {
            let Some(j) = flver.nodes.iter().position(|n| n.name == b.name) else {
                continue;
            };
            matched += 1;
            let d = hk[i].w_axis.truncate().distance(world[j].w_axis.truncate());
            if d > 1e-3 {
                let hk_parent = b.parent.map(|p| s.bones[p as usize].name.as_str());
                let fl_parent = usize::try_from(flver.nodes[j].parent)
                    .ok()
                    .and_then(|p| flver.nodes.get(p))
                    .map(|n| n.name.as_str());
                eprintln!(
                    "  {chr_id} {} differs by {d} (parents: havok {hk_parent:?}, flver {fl_parent:?})",
                    b.name
                );
            }
            // IK foot targets are parented differently in the FLVER, and the soldier's `[omit]`
            // helper bones have a different FLVER bind pose; drive those through model space.
            if b.name.contains("_Foot_Target") || b.name.ends_with("[omit]") {
                continue;
            }
            if d > worst.0 {
                worst = (d, b.name.clone());
            }
        }
        eprintln!(
            "{chr_id}: {matched}/{} matched, worst {worst:?}",
            s.bones.len()
        );
        assert!(
            matched * 10 >= s.bones.len() * 9,
            "{chr_id}: only {matched} names match"
        );
        assert!(worst.0 < 1e-3, "{chr_id}: {worst:?}");
    }
}
