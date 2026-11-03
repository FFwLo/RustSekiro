//! Reviewed castle-gate and Shura staging from the installed map EMEVDs.

use crate::arena::{ArenaSpec, BossEvents};
use crate::events::EventEntry;

/// Map SFX regions (POINT_PARAM_ST type 5), read from the player's own MSB.
#[derive(Debug, Clone, Copy)]
pub struct MapEffect {
    pub entity: i32,
    pub effect: i32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

pub fn map_effects(cache: &std::path::Path, map: &str) -> Result<Vec<MapEffect>, String> {
    use sekiro_formats::reader::{Reader, utf16z};
    let path = cache.join(format!("raw/map/MapStudio/{map}.msb"));
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let read = || -> sekiro_formats::reader::Result<Vec<MapEffect>> {
        let mut table = 0x10;
        for _ in 0..32 {
            if table == 0 {
                break;
            }
            let mut r = Reader::at(&bytes, table);
            r.i32()?;
            let count = r.i32()? as usize;
            if !(1..=100_000).contains(&count) {
                break;
            }
            let name = r.offset64()?;
            let mut entries = Vec::new();
            for _ in 1..count {
                entries.push(r.offset64()?);
            }
            table = r.offset64()?;
            if utf16z(&bytes, name)? != "POINT_PARAM_ST" {
                continue;
            }
            let mut effects = Vec::new();
            for entry in entries {
                if Reader::at(&bytes, entry + 8).i32()? != 5 {
                    continue;
                }
                let base = Reader::at(&bytes, entry + 0x50).offset64()?;
                let data = Reader::at(&bytes, entry + 0x58).offset64()?;
                if base == 0 || data == 0 {
                    continue;
                }
                let entity = Reader::at(&bytes, entry + base + 4).i32()?;
                let effect = Reader::at(&bytes, entry + data).i32()?;
                let mut r = Reader::at(&bytes, entry + 0x14);
                let [x, y, z] = r.vec3()?;
                let [rx, ry, rz] = r.vec3()?;
                effects.push(MapEffect {
                    entity,
                    effect,
                    position: [-x, y, z],
                    rotation: [rx, -ry, -rz],
                });
            }
            return Ok(effects);
        }
        Ok(Vec::new())
    };
    read().map_err(|e| e.to_string())
}

const fn event(id: i64) -> EventEntry {
    EventEntry {
        id,
        from_label: None,
    }
}

/// m11_00 events 11105800/810/812/820/830/840/841/842/845.
/// Join 11105810 at instruction 4 after the arena-entry condition; 11100801 skips the intro wait.
/// 11105811 starts three common 20005820 walls, with no 20005825 boundary.
pub const GYOUBU: BossEvents = BossEvents {
    map: "m11_00_00_00",
    boss_entity: 1_100_800,
    flags: &[11_105_803, 11_105_801, 11_100_801],
    events: &[
        event(11_105_800),
        event(11_105_810),
        event(11_105_812),
        event(11_105_820),
        event(11_105_830),
        event(11_105_840),
        event(11_105_841),
        event(11_105_842),
        event(11_105_845),
    ],
    fog_events: &[11_105_811],
    lighting: None,
};

/// m11_00 events 11105900/910/912/920/921/922/924 and camera events 970-972.
/// The retry state 11100901 bypasses the soldier encounter. Off the arena (a flat `--enemy` duel)
/// 11105901 is staged so the fight is on; `Arena::place` removes it, and event 11105910 then holds
/// the Demon's animation disabled until Wolf and he are inside region 1102910 and sets it itself.
/// 11105911 seals the same three assets with start 11105901 and defeat 9313.
pub const DEMON: BossEvents = BossEvents {
    map: "m11_00_00_00",
    boss_entity: 1_100_900,
    flags: &[8302, 9301, 11_100_901, 11_105_901],
    events: &[
        event(11_105_900),
        event(11_105_910),
        event(11_105_912),
        event(11_105_920),
        event(11_105_921),
        event(11_105_922),
        event(11_105_924),
        event(11_105_970),
        event(11_105_971),
        event(11_105_972),
    ],
    fog_events: &[11_105_911],
    lighting: None,
};

/// m11_01 events 11115900/910/930/943: Emma setup Label2, defeat, Isshin hand-over,
/// and the event-controlled tatami/burnt floor. 11115911 keeps the roof fog standing.
pub const EMMA: BossEvents = BossEvents {
    map: "m11_01_00_00",
    boss_entity: 1_110_900,
    flags: &[8300, 8301, 9303, 11_115_722, 71110020, 1270, 1276],
    events: &[
        event(11_115_900),
        EventEntry {
            id: 11_115_910,
            from_label: Some(2),
        },
        event(11_115_930),
        event(11_115_943),
    ],
    fog_events: &[11_115_911],
    lighting: None,
};

/// m11_01 events 11115920/930/932/941/942/943/944.
/// Join 930 at instruction 40 after the cutscene; its SetLighting(4) selects night.
/// Fog event 11115931 replaces Emma's wall: start 11115921, end 11110100.
pub const ISSHIN: BossEvents = BossEvents {
    map: "m11_01_00_00",
    boss_entity: 1_110_920,
    flags: &[
        8300, 8301, 9303, 9315, 11_115_722, 11_115_901, 11_115_921, 1290, 1296,
    ],
    events: &[
        event(11_115_920),
        event(11_115_930),
        event(11_115_932),
        event(11_115_941),
        event(11_115_942),
        event(11_115_943),
        event(11_115_944),
    ],
    fog_events: &[11_115_931],
    lighting: Some(4),
};

pub const GYOUBU_ARENA: ArenaSpec = ArenaSpec {
    name: "gyoubu",
    map: "m11_00_00_00",
    boss_part: "c5080_0000",
    enemy: "c5080:50800000",
    radius: 120.0,
    player_start: Some("c0000_0008"),
    hidden_models: &[],
    beaten: &[],
};
pub const DEMON_ARENA: ArenaSpec = ArenaSpec {
    name: "demon",
    map: "m11_00_00_00",
    boss_part: "c7020_0000",
    enemy: "c7020:70200000",
    radius: 120.0,
    player_start: Some("c0000_0008"),
    hidden_models: &[],
    beaten: &[9303, 9304, 9305, 9306, 9802],
};
pub const SHURA_ARENA: ArenaSpec = ArenaSpec {
    name: "shura",
    map: "m11_01_00_00",
    boss_part: "c7400_0001",
    enemy: "c7400:74000010",
    radius: 60.0,
    player_start: Some("c0000_0010"),
    hidden_models: &[],
    beaten: &[9303, 9304, 9305, 9306],
};

/// Report absent primary and sequential boss models before starting a graphical arena.
pub fn missing_models(cache: &std::path::Path, arena: &ArenaSpec) -> Vec<String> {
    let characters: &[&str] = match arena.name {
        "gyoubu" => &["c5080"],
        "demon" => &["c7020"],
        "shura" => &["c7400", "c5430"],
        _ => &[],
    };
    characters
        .iter()
        .map(|chr| format!("models/{chr}.glb"))
        .filter(|path| !cache.join(path).is_file())
        .collect()
}

/// Reviewed instruction entry points for events with no post-intro label.
/// Emma's 11115930 still waits for her defeat flag before playing the transition.
pub fn entry(id: i64, flags: &std::collections::HashSet<i32>) -> Option<usize> {
    match id {
        11_105_800 => Some(5), // Do not re-disable Gyoubu at fight startup.
        11_105_810 => Some(4),
        11_105_910 => Some(0),
        11_115_930 => Some(if flags.contains(&11_115_921) { 40 } else { 20 }),
        _ => None,
    }
}

/// These flags mean the pre-fight entrance sequence has already completed.
pub fn after_intro(id: i64, pc: usize, flags: &std::collections::HashSet<i32>) -> Option<usize> {
    match (id, pc) {
        (11_105_810, 12) if flags.contains(&11_100_801) => Some(18),
        (11_105_810, 14) if flags.contains(&11_100_801) => Some(18),
        (11_105_910, 27) if flags.contains(&11_105_901) => Some(30),
        _ => None,
    }
}

/// Both Shura setup cutscenes warp Wolf to region 1112812, 0.46 m from player part 0010.
pub fn story_start(
    cache: &std::path::Path,
    spec: &ArenaSpec,
    layout: &mut crate::arena::ArenaLayout,
) -> Result<(), String> {
    if spec.name == "shura" {
        let warp = crate::events::warp_point(cache, 11, 1, 1112812)
            .ok_or("Shura: missing MSB story warp region 1112812")?;
        layout.wolf.position = glam::Vec3::from_array(warp.position);
        layout.wolf.yaw = crate::arena::yaw_toward(layout.wolf.position, layout.boss.position);
        layout.wolf_source = "story warp 1112812 (near c0000_0010)".to_owned();
    }
    Ok(())
}
