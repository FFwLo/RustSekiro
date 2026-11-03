//! Sim tests, part 15: the atlas vfx + lighting pass (docs/kb/mechanics-atlas.json, hit effects,
//! blood, FXR, cloth and status looks; sekiro-rs's lighting for our scenes). Each test names its
//! atlas id.

use super::*;

/// sekiro-rs's draw params for `stem` (his cache, `raw/param/drawparam/<stem>.gparam`) as his
/// MapLighting; None (test skipped) without his cache.
fn sr_lighting(stem: &str, light_set: i32, hour: f32) -> Option<sekiro_rs::lighting::MapLighting> {
    let path = crate::sr_map::cache().join(format!("raw/param/drawparam/{stem}.gparam"));
    let gparam = sekiro_formats::gparam::parse(&std::fs::read(&path).ok()?).ok()?;
    Some(sekiro_rs::lighting::MapLighting { gparam, hour, light_set, applied: None, source: stem.into() })
}

/// Atlas visuals.lightset-fields (and visuals.light-unit-bridge): a lit FXR particle under
/// sekiro-rs's map takes his light set in force through his LightSet: the sun is slot 0's
/// DiffColor0 as the exe uploads it (rgb * rgb * W), the same as our own reader of the same
/// gparam gives, and the scene unit is his exe-traced bridge (pi / Bevy's default exposure).
#[test]
fn atlas_lightset_fields_lit_particles_take_sekiro_rs_light_set() {
    assert!((crate::map::SCENE_TO_BEVY - 3135.61).abs() < 0.01, "the bridge is sekiro-rs's");
    let Ok(text) = std::fs::read_to_string(crate::paths::extracted().join("map_m11_01_00_00.json")) else {
        eprintln!("no m11_01 export: skipped");
        return;
    };
    let j: serde_json::Value = serde_json::from_str(&text).unwrap();
    for (light_set, hour) in [(0, 12.0), (100, 18.0)] {
        let Some(his) = sr_lighting("m11_01_0000", light_set, hour) else {
            eprintln!("no sekiro-rs cache: skipped");
            return;
        };
        let (amb, sun, to_sun) = crate::sr_map::particle_light(&his);
        // Slot 0 straight from his gparam: DiffColor0 squared times W.
        let raw = his.gparam.param("LightSet", "DiffColor0").and_then(|p| p.at(his.gparam.light_row(light_set), hour)).expect("DiffColor0");
        let want = Vec3::new(raw[0] * raw[0], raw[1] * raw[1], raw[2] * raw[2]) * raw[3];
        assert!((sun - want).length() < 1e-3 * want.length().max(1.0), "set {light_set}: sun {sun} vs rgb^2 W {want}");
        // Our own reader of the same draw params (the own map's path) agrees.
        let ours = crate::map::MapLighting {
            drawparam: j["drawparam"].clone(),
            light_set,
            hour,
            applied: false,
            env: Vec::new(),
            arena_yaw: 0.0,
            luts: Vec::new(),
            stem: String::new(),
            sound_space: 0,
            source_map: String::new(),
            map_origin: None,
        };
        let (o_amb, o_sun, o_to) = crate::map::particle_light(&ours);
        assert!((amb - o_amb).length() < 1e-3 * o_amb.length().max(1.0), "set {light_set}: ambient {amb} vs ours {o_amb}");
        assert!((sun - o_sun).length() < 1e-3 * o_sun.length().max(1.0), "set {light_set}: sun {sun} vs ours {o_sun}");
        assert!(to_sun.dot(o_to) > 0.999, "set {light_set}: sun way {to_sun} vs ours {o_to}");
    }
}

/// Is `id` one of the effects the game ships (an f<id>.fxr in one of the eight sfxbnd archives the
/// game has, extracted/sfx/*.ffxbnd.d)?
fn shipped(id: i64) -> bool {
    let Ok(dirs) = std::fs::read_dir(crate::paths::extracted().join("sfx")) else { return false };
    dirs.filter_map(|d| d.ok()).any(|d| d.path().join(format!("f{id:09}.fxr")).is_file())
}

/// Atlas combat.hit-effects (and visuals.fxr-format-player): the clash effects come from the
/// HitEffectSfx tables by material (Wolf's sword on bare flesh 109 and on the General's armour 114
/// / 108 differ; a deflect takes the JustGuard table), and every effect those tables, Wolf's
/// SpawnOneShotFFX / SpawnFFX_Blade events and the blood FFXs name is either extracted for fxr.rs
/// or not shipped by the game at all, so nothing needs a hand-made stand-in (vfx.rs, ffx.rs and
/// gore.rs have none any more).
#[test]
fn atlas_hit_effects_come_from_the_tables_and_the_games_own_fxr() {
    let app = app();
    let combat = app.world().resource::<Combat>();
    use crate::vfx::{Kind, hit_sfx};
    let sword = crate::data::Attack { atk_material_sfx: 0, atk_pow_sfx: 1, atk_type: 0, def_sfx_material1: 255, def_sfx_material2: 139, ..Default::default() };
    let flesh = hit_sfx(combat, &sword, Kind::Hit, [109, -1]);
    let armour = hit_sfx(combat, &sword, Kind::Hit, [114, 108]);
    assert!(!flesh.is_empty() && !armour.is_empty(), "flesh {flesh:?} armour {armour:?}");
    assert_ne!(flesh, armour, "the material picks the effect");
    let general = crate::data::Attack { atk_material_sfx: 0, atk_pow_sfx: 2, atk_type: 0, def_sfx_material1: 100, def_sfx_material2: 139, ..Default::default() };
    assert_ne!(hit_sfx(combat, &general, Kind::Deflect, [-1, -1]), hit_sfx(combat, &general, Kind::Guard, [-1, -1]), "a deflect shows the just-guard sparks");
    if !crate::paths::extracted().join("sfx").is_dir() {
        eprintln!("no extracted/sfx: archive check skipped");
        return;
    }
    let mut ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for row in combat.params["HitEffectSfxParam"].as_object().into_iter().flat_map(|o| o.values()) {
        for (k, v) in row.as_object().into_iter().flatten() {
            if ["Slash", "Blow", "Thrust"].iter().any(|t| k.starts_with(t)) {
                ids.extend(v.as_i64().filter(|v| *v > 0));
            }
        }
    }
    for anim in combat.player.anims.values() {
        ids.extend(anim.events.iter().filter(|e| matches!(e.kind, 96 | 118)).filter_map(|e| e.arg_i64("FFXID")).filter(|v| *v > 0));
    }
    ids.extend([220502, 220503, 220505, 220506, 401000]);
    let missing: Vec<i64> = ids.iter().copied().filter(|id| shipped(*id) && !crate::paths::extracted().join(format!("fxr/{id}.json")).is_file()).collect();
    assert!(missing.is_empty(), "shipped effects not extracted (tools/fxr_extract.py): {missing:?}");
    assert!(!shipped(206000) && !shipped(254000), "these HitEffectSfxParam ids are not in the game's archives");
}

/// Our side's map lighting for the hour / light set tests (no draw params needed).
fn our_lighting(light_set: i32, hour: f32) -> crate::map::MapLighting {
    crate::map::MapLighting {
        drawparam: serde_json::Value::Null,
        light_set,
        hour,
        applied: false,
        env: Vec::new(),
        arena_yaw: 0.0,
        luts: Vec::new(),
        stem: String::new(),
        sound_space: 0,
        source_map: String::new(),
        map_origin: None,
    }
}

/// Atlas visuals.time-of-day-variants and map.story-clock: under sekiro-rs's lighting the hour
/// our side asks for (SHINOBI_HOUR, the `[` / `]` keys, the debug menu's hour and day cycle)
/// reaches his MapLighting, which applies that time's light set again (applied cleared); and
/// visuals.lightset-per-collision on our own scene (the sandbox): his light set is the one our
/// export took from the collision under the source arena's fight spot.
#[test]
fn atlas_time_of_day_and_scene_light_set_reach_sekiro_rs_lighting() {
    let Some(his) = sr_lighting("m11_01_0000", 0, 0.0) else {
        eprintln!("no sekiro-rs cache: skipped");
        return;
    };
    let mut app = App::new();
    app.insert_resource(his).insert_resource(our_lighting(100, 18.0)).add_systems(Update, (crate::sr_map::sync_hour, crate::sr_map::scene_light_set));
    app.world_mut().resource_mut::<sekiro_rs::lighting::MapLighting>().applied = Some(0);
    app.update();
    let his = app.world().resource::<sekiro_rs::lighting::MapLighting>();
    assert_eq!((his.hour, his.light_set, his.applied), (18.0, 100, None), "the hour and the scene's light set go over and re-apply");
    assert!(app.world().resource::<crate::map::MapLighting>().applied, "taken once");
    // A later change (the `]` key: applied cleared) goes over again; an unchanged one does not.
    app.world_mut().resource_mut::<sekiro_rs::lighting::MapLighting>().applied = Some(100);
    app.update();
    assert_eq!(app.world().resource::<sekiro_rs::lighting::MapLighting>().applied, Some(100));
    {
        let mut ours = app.world_mut().resource_mut::<crate::map::MapLighting>();
        ours.hour = 19.0;
        ours.applied = false;
    }
    app.update();
    let his = app.world().resource::<sekiro_rs::lighting::MapLighting>();
    assert_eq!((his.hour, his.applied), (19.0, None));
}
