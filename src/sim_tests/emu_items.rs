//! Item use and prosthetic bullets against the user's own sekiro.exe, run in tools/emu-cases
//! `items` on the Naomori-arena snapshot (docs/kb/frame-coverage.md "Item use and prosthetic
//! bullets"): whole chr frames FUN_140a4b570 from the press, the item event FUN_140b57740 ->
//! FUN_1407d5d50 -> the launch FUN_1409e4cd0 -> SpEffect add / bullet, and the count
//! (AdjustItemCount FUN_14079ab70). Skips when extracted/emu/items_*.json is missing.

use super::*;

fn emu_json(name: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(crate::paths::emu().join(format!("{name}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// The first exe frame whose list of added SpEffects holds `id`.
fn exe_added(v: &serde_json::Value, id: i64) -> Option<usize> {
    v["frames"].as_array()?.iter().position(|f| f["sp_added"].as_array().is_some_and(|a| a.iter().any(|x| x.as_i64() == Some(id))))
}

/// The Healing Gourd (goods 3000, refCategory 2 -> SpEffect 3000): pressed on frame 0, the exe
/// plays a000_250000 from frame 1 and its TAE 65 (0.7 s) runs the goods use on frame 42: SpEffect
/// 3000 is added, the HP rises and AdjustItemCount takes the charge, all that frame. Ours: the
/// press, then the frame the HP rises and a charge is gone.
#[test]
pub(crate) fn gourd_use_matches_the_exe() {
    let Some(v) = emu_json("items_gourd") else { return };
    let frames = v["frames"].as_array().unwrap();
    let hp0 = frames[0]["hp"].as_i64().unwrap();
    let exe = exe_added(&v, 3000).expect("the exe added no 3000");
    let exe_hp = frames.iter().position(|f| f["hp"].as_i64() != Some(hp0)).unwrap();
    assert_eq!(exe, exe_hp, "the exe heals on the add's frame");
    let adjust = frames.iter().position(|f| f["events"].as_array().unwrap().iter().any(|e| e["fn"] == "adjust_count" && e["delta"] == -1)).unwrap();
    assert_eq!(adjust, exe, "the exe takes the charge on the add's frame");

    let mut app = app();
    move_enemy_far(&mut app);
    let (charges, hp_start) = {
        let world = app.world_mut();
        let mut q = world.query::<(&mut Actor, &Player)>();
        let (mut a, p) = q.single_mut(world).unwrap();
        // Ours starts at the exe's share of max HP (548 of 1120), so the heal shows.
        a.hp = (a.hp_max * hp0 as f32 / v["hp_max"].as_f64().unwrap() as f32).floor();
        (p.gourd, a.hp)
    };
    app.world_mut().resource_mut::<PadInput>().press(Action::UseItem);
    let mut ours = None;
    for f in 0..frames.len() {
        app.update();
        let world = app.world_mut();
        let mut q = world.query::<(&Actor, &Player)>();
        let (a, p) = q.single(world).unwrap();
        if ours.is_none() && a.hp > hp_start {
            ours = Some(f);
            assert_eq!(p.gourd, charges - 1, "the charge goes with the heal");
        }
    }
    assert_eq!(ours, Some(exe), "the heal's frame (exe {exe})");
}

/// The snapshot's prosthetic, the Loaded Axe 73000, tapped (3 frames): the exe plays a073_400000
/// from frame 1, the release a073_400100 from frame 49, runs its TAE 2 (judge 297, dummy 603; no
/// bullet: the row issues none) at 0.3 s and pays 2 Spirit Emblems (AdjustItemCount on goods 1000)
/// at the TAE 1 judge 102 (0.433 s, frame 74). Ours: the release's first frame and the payment.
#[test]
pub(crate) fn loaded_axe_emblems_match_the_exe() {
    let Some(v) = emu_json("items_prosthetic") else { return };
    let frames = v["frames"].as_array().unwrap();
    let tae = |f: &serde_json::Value, id: i64| f["tae"].as_array().unwrap().iter().any(|a| a.as_i64() == Some(id));
    let exe_release = frames.iter().position(|f| tae(f, 73400100)).unwrap();
    let (exe_pay, exe_cost) = frames
        .iter()
        .enumerate()
        .find_map(|(i, f)| f["events"].as_array().unwrap().iter().find(|e| e["fn"] == "adjust_count").map(|e| (i, -e["delta"].as_i64().unwrap() as u32)))
        .unwrap();
    assert!(!frames.iter().any(|f| f["events"].as_array().unwrap().iter().any(|e| e["fn"] == "bullet")), "the axe issues no bullet");

    let mut app = app();
    app.world_mut().resource_mut::<crate::config::GameConfig>().player.prosthetics = vec![73000];
    move_enemy_far(&mut app);
    let e0 = app.world_mut().query::<&Player>().single(app.world()).unwrap().emblems;
    let (mut release, mut pay) = (None, None);
    for f in 0..frames.len() {
        {
            let mut pad = app.world_mut().resource_mut::<PadInput>();
            pad.prosthetic_held = f < 3;
            if f == 0 {
                pad.press(Action::Prosthetic);
            }
        }
        app.update();
        let (_, anim, ..) = player_state(&mut app);
        let e = app.world_mut().query::<&Player>().single(app.world()).unwrap().emblems;
        if release.is_none() && anim == "a073_400100" {
            release = Some(f);
        }
        if pay.is_none() && e < e0 {
            pay = Some((f, e0 - e));
        }
    }
    let (release, (pay, cost)) = (release.expect("no release"), pay.expect("no payment"));
    // Within the release the payment's frame and amount match (25 frames in, 2 emblems).
    assert_eq!((pay - release, cost), (exe_pay - exe_release, exe_cost), "payment after the release (exe {exe_release} -> {exe_pay} x{exe_cost}, ours {release} -> {pay})");
    // The release's first step and the payment on the exe's frames (49, 74): the exe's HKS asks
    // for a clip after the frame's behaviour step, so its first update is the next frame's; ours
    // reads the press a frame late (player::hks_pad_in) and steps the clip in that frame
    // (tools/emu-cases animstart, kb/frame-coverage.md "One frame's order"). Before, ours led by
    // one (48, 73).
    assert_eq!((release, pay), (exe_release, exe_pay), "release and payment frames");
}
