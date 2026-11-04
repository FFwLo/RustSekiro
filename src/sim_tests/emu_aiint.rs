//! The interrupts the enemy Lua AI reads and its sound target, against the exe (topic aiint).

use std::collections::HashMap;

use bevy::prelude::*;

use crate::data::Combat;
use crate::enemy::{ai_sound_heard, AiSounds};
use crate::stealth::{Heard, Targeting};
use crate::enemy::Enemy;
use crate::Actor;

fn run(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("aiint_{name}.json"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    Some(v["scenarios"][0].clone())
}

fn v3(v: &serde_json::Value) -> Vec3 {
    Vec3::new(v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32, v[2].as_f64().unwrap() as f32)
}

/// The interrupt numbers in a frame's masks (the goal tree's dispatch, FUN_1405ada60).
fn ints(fr: &serde_json::Value) -> Vec<u32> {
    let mut out = Vec::new();
    for m in fr["masks"].as_array().unwrap() {
        let m = m.as_u64().unwrap();
        out.extend((0..64).filter(|b| m >> b & 1 == 1));
    }
    out
}

/// The exe's sound target in a frame: the target list's kind 3 node, its slot at +0x70
/// (serial u16 +0x78, id +0x94, rank +0x98).
fn held(fr: &serde_json::Value) -> Option<(u64, i64, i64)> {
    let n = fr["list"].as_array().unwrap().iter().find(|n| n["kind"] == 3)?;
    let w = |i: usize| n["w"][i].as_u64().unwrap();
    Some((w(0x1e) & 0xffff, w(0x25) as i32 as i64, w(0x26) as i32 as i64))
}

/// The General's sound target frame by frame (tools/emu-cases `aiint_snd` / `aiint_snd_two`:
/// whole world frames on the snapshot, AI on, Wolf unseen on his ledge; AI sounds raised near
/// the General as Wolf's TAE CreateAISound raises them, FUN_14064a5f0, the records aged after
/// each frame by FUN_140611a00). Replayed through `AiSounds::emit` (the records: a sound raised
/// again within its life is refreshed, after it a new record), the listener check
/// `ai_sound_heard` at the General's spot and `Targeting::listen` (the pick: higher rank, then
/// the newer record): per frame the held sound's id and rank, the frames that raise
/// INTERUPT_ChangeSoundTarget (46), the live records' ids and which held records are one
/// record (the serials, matched one to one) must be the exe's. Skipped when the JSON is missing.
#[test]
fn sound_targets_match_the_exe() {
    let runs: Vec<_> = ["snd", "snd_two"].iter().filter_map(|n| run(n)).collect();
    if runs.is_empty() {
        return;
    }
    let mut app = super::app();
    app.update();
    let world = app.world_mut();
    let (me, la) = {
        let mut q = world.query_filtered::<(Entity, &Actor), With<Enemy>>();
        let (e, a) = q.iter(world).next().unwrap();
        (e, a.clone())
    };
    let combat = world.resource::<Combat>();
    let team = crate::combat::team_of(combat, &la);
    let mut checked = (0, 0);
    for sc in &runs {
        let think = combat.param("NpcThinkParam", sc["think"].as_i64().unwrap()).clone();
        let dt = sc["dt"].as_f64().unwrap() as f32;
        let frames = sc["frames"].as_array().unwrap();
        let sounds: Vec<(usize, i64, f32)> = sc["sounds"].as_array().unwrap().iter().map(|s| (s[0].as_u64().unwrap() as usize, s[1].as_i64().unwrap(), s[2].as_f64().unwrap() as f32)).collect();
        let mut recs = AiSounds::default();
        let mut t = Targeting::new(1);
        let mut serials: HashMap<u64, i64> = HashMap::new();
        for (f, fr) in frames.iter().enumerate() {
            // Raised before the frame, in front of the General as he stood after the last one.
            for &(_, id, d) in sounds.iter().filter(|s| s.0 == f) {
                let prev = &frames[f - 1];
                let (g, yaw) = (v3(&prev["g"]), prev["gyaw"].as_f64().unwrap() as f32);
                recs.emit(combat, id, g - d * Vec3::new(yaw.sin(), 0.0, yaw.cos()), None, None);
            }
            // Our X is the game's mirrored (map.rs); the distances are the same either way.
            let at = v3(&fr["g"]);
            t.sound_new = false;
            let heard: Vec<Heard> = recs
                .0
                .iter()
                .filter_map(|s| ai_sound_heard(combat, s, me, team, &la, at, &think, &[]).map(|(id, rank)| Heard { pos: s.pos, id, rank, serial: s.serial }))
                .collect();
            t.listen(&think, &heard);
            let exe = held(fr);
            assert_eq!(t.sound.map(|s| (s.id, s.rank)), exe.map(|h| (h.1, h.2)), "{} f{f}: held", sc["name"]);
            if let (Some(h), Some(s)) = (exe, t.sound) {
                let mine = *serials.entry(h.0).or_insert(s.serial);
                assert_eq!(mine, s.serial, "{} f{f}: the exe's record {} is another of ours", sc["name"], h.0);
            }
            assert_eq!(t.sound_new, ints(fr).contains(&46), "{} f{f}: ChangeSoundTarget (46)", sc["name"]);
            // The live records (one area's copy: the exe keeps one per map area).
            let area = fr["records"].as_array().unwrap().first().map(|r| r["area"].clone());
            let mut want: Vec<i64> = fr["records"].as_array().unwrap().iter().filter(|r| Some(&r["area"]) == area.as_ref()).map(|r| r["id"].as_i64().unwrap()).collect();
            // Aged after the frame (FUN_140611a00): the exe's list is read after it.
            recs.0.retain_mut(|s| {
                s.life -= dt;
                s.life > 1e-4
            });
            let mut got: Vec<i64> = recs.0.iter().map(|s| s.id).collect();
            want.sort();
            got.sort();
            assert_eq!(got, want, "{} f{f}: live records", sc["name"]);
            checked.0 += 1;
            checked.1 += t.sound_new as usize;
        }
    }
    assert!(checked.0 > 150 && checked.1 >= 5, "{checked:?}");
}

/// INTERUPT_LoseSightTarget (31) against the exe (tools/emu-cases `aiint_lose`: the General in
/// BATTLE with Wolf 6 m in front, Wolf moved behind him at f60 and in front again at f110): the
/// exe raises 31 exactly on the frames its normal target's seen flag (node +0x40) goes 1 -> 0
/// (the sight search that misses), never when it comes back; `ai::lose_sight` on the same flags
/// must give the same frames. Skipped when the JSON is missing.
#[test]
fn lose_sight_interrupt_matches_the_exe() {
    let Some(sc) = run("lose") else { return };
    let mut was = false;
    let (mut exe, mut ours) = (Vec::new(), Vec::new());
    for (f, fr) in sc["frames"].as_array().unwrap().iter().enumerate() {
        let seen = fr["list"].as_array().unwrap().iter().find(|n| n["kind"] == 1).is_some_and(|n| n["w"][0x10].as_u64().unwrap() & 0xff == 1);
        if ints(fr).contains(&31) {
            exe.push(f);
        }
        if crate::ai::lose_sight(&mut was, seen) {
            ours.push(f);
        }
    }
    assert_eq!(ours, exe);
    assert!(exe.len() >= 2, "{exe:?}");
}
