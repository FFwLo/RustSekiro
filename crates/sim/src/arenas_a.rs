//! Reviewed Hirata and Hall of Illusions staging, from the installed EMEVD and MSB.

use std::collections::HashSet;
use std::path::Path;

use crate::arena::{ArenaSpec, BossEvents};
use crate::enemy::EnemySpec;
use crate::events::EventEntry;
use sekiro_formats::{emevd::Emevd, hknp::CollisionMesh, msb};

const fn event(id: i64) -> EventEntry {
    EventEntry {
        id,
        from_label: None,
    }
}

/// m10 event 11005810 starts Butterfly; 11005812 swaps 1000800 to 1000810.
/// 11005800 owns defeat; 11005856/57 own immortality; 11005820/50..81 own illusions.
/// Fog comes from 11005811, not the similarly numbered illusion event 11005820.
pub const BUTTERFLY: BossEvents = BossEvents {
    map: "m10_00_00_00",
    boss_entity: 1_000_800,
    flags: &[11_000_801, 11_005_801],
    events: &[event(11_005_810), event(11_005_812), event(11_005_856)],
    fog_events: &[11_005_811],
    // common event 100's m10 branch selects the fixed past-night environment (variant 0).
    lighting: Some(0),
};

/// Butterfly's second placed character, with its own NpcParam and boss-bar entity.
/// 11005812 retains its native descent/region gate; the illusion event ids are retained.
pub const BUTTERFLY_SECOND: BossEvents = BossEvents {
    boss_entity: 1_000_810,
    events: &[
        event(11_005_812),
        event(11_005_800),
        event(11_005_857),
        event(11_005_820),
        event(11_005_850),
        event(11_005_851),
        event(11_005_855),
        event(11_005_858),
        event(11_005_860),
        event(11_005_870),
        event(11_005_880),
        event(11_005_881),
    ],
    ..BUTTERFLY
};

/// m10 11005910 Label2 starts Father, 11005912 enables the bird after message 20,
/// 11005921 releases immortality, and 11005900 owns defeat.
/// 11005922/23/30/31/35/36 govern the bird and vanish/return state.
pub const FATHER: BossEvents = BossEvents {
    map: "m10_00_00_00",
    boss_entity: 1_000_900,
    flags: &[8304, 11_005_914],
    events: &[
        EventEntry {
            id: 11_005_910,
            from_label: Some(2),
        },
        event(11_005_900),
        event(11_005_912),
        event(11_005_920),
        event(11_005_921),
        event(11_005_922),
        event(11_005_923),
        event(11_005_930),
        event(11_005_931),
        event(11_005_935),
        event(11_005_936),
    ],
    fog_events: &[11_005_911],
    lighting: Some(0),
};

/// Inner Father's real script is m10_00_50_61, whose 11005910 Label2 uses 1000990
/// and name 905063; 11005911 ends its fog on 9346, unlike story Father's 9317.
pub const INNER_FATHER: BossEvents = BossEvents {
    map: "m10_00_50_61",
    boss_entity: 1_000_990,
    ..FATHER
};

/// m20 12005810 owns controller 2000800's bar; 12005800 awaits all four monkeys.
/// 12005820 has four screen slots; 12005825/55/60 own bell, light and room state.
/// The map has no 20005820 or 20005825 fog-wall starts for this fight.
pub const MONKEYS: BossEvents = BossEvents {
    map: "m20_00_00_00",
    boss_entity: 2_000_800,
    flags: &[8400, 9200, 12_000_801, 12_005_801],
    events: &[
        event(12_005_810),
        event(12_005_800),
        event(12_005_813),
        event(12_005_820),
        event(12_005_825),
        event(12_005_826),
        event(12_005_855),
        event(12_005_856),
        event(12_005_857),
        event(12_005_858),
        event(12_005_859),
        event(12_005_860),
        event(12_005_827),
        event(12_005_828),
        event(12_005_835),
        event(12_005_836),
        event(12_005_837),
        event(12_005_838),
        event(12_005_870),
        event(12_005_873),
        event(12_005_875),
        event(12_005_890),
        event(12_005_895),
        event(12_005_900),
    ],
    fog_events: &[],
    lighting: None,
};

pub const HIRATA_HALL: ArenaSpec = ArenaSpec {
    name: "hirata-hall",
    map: "m10_00_00_00",
    boss_part: "c5090_0000",
    enemy: "c5090:50900000",
    radius: 60.0,
    player_start: Some("c0000_0006"),
    hidden_models: &[],
    beaten: &[],
};
pub const HIRATA_FATHER: ArenaSpec = ArenaSpec {
    name: "hirata-father",
    boss_part: "c5060_0001",
    enemy: "c5060:50601010",
    ..HIRATA_HALL
};
pub const HIRATA_INNER_FATHER: ArenaSpec = ArenaSpec {
    name: "hirata-inner-father",
    boss_part: "c5060_0002",
    ..HIRATA_FATHER
};
pub const SENPOU_SCREENS: ArenaSpec = ArenaSpec {
    name: "senpou-screens",
    map: "m20_00_00_00",
    boss_part: "c1000_0009",
    enemy: "c1000:12600050",
    radius: 160.0,
    // No player part exists in the hidden temple: event 12005210 warps to region 2002200.
    player_start: Some("region:2002200"),
    hidden_models: &[],
    beaten: &[9303],
};

pub fn owns(spec: &ArenaSpec) -> bool {
    [
        HIRATA_HALL.name,
        HIRATA_FATHER.name,
        HIRATA_INNER_FATHER.name,
        SENPOU_SCREENS.name,
    ]
    .contains(&spec.name)
}

/// All required primary/support models are checked before constructing a Bevy session.
pub fn missing_models(cache: &Path, spec: &ArenaSpec) -> Result<Vec<String>, String> {
    if !owns(spec) {
        return Ok(Vec::new());
    }
    let path = cache.join(format!("maps/{}/layout.json", spec.map));
    let json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let boss =
        crate::arena::boss_events(&spec.enemy_spec()).ok_or("arena has no boss event row")?;
    let mut models = HashSet::new();
    for row in json["enemies"].as_array().into_iter().flatten() {
        let entity = row["entity_id"].as_i64().unwrap_or(-1) as i32;
        if (entity == boss.boss_entity || simultaneous(boss.boss_entity, entity))
            && let Some(model) = row["model"].as_str()
        {
            models.insert(format!("models/{model}.glb"));
        }
    }
    let mut missing: Vec<_> = models
        .into_iter()
        .filter(|p| !cache.join(p).is_file())
        .collect();
    missing.sort();
    Ok(missing)
}

/// These MSB actors coexist with the primary boss; enable commands must not replace it.
pub fn simultaneous(boss: i32, entity: i32) -> bool {
    match boss {
        1_000_800 => matches!(entity, 1_000_810 | 1_000_820..=1_000_827),
        1_000_810 => matches!(entity, 1_000_820..=1_000_827),
        1_000_900 => entity == 1_000_901,
        1_000_990 => entity == 1_000_991,
        2_000_800 => matches!(entity, 2_000_801..=2_000_809 | 2_000_811..=2_000_814),
        _ => false,
    }
}

pub fn initially_enabled(entity: i32) -> Option<bool> {
    match entity {
        1_000_810 | 1_000_820..=1_000_827 | 1_000_901 | 1_000_991 => Some(false),
        2_000_801..=2_000_804 => Some(true),
        2_000_805..=2_000_809 | 2_000_811..=2_000_814 => Some(false),
        _ => None,
    }
}

/// The specific props named by the arena's events, rather than every object on the map.
pub fn object_entities(cache: &Path, spec: &ArenaSpec) -> Result<HashSet<i32>, String> {
    if !owns(spec) {
        return Ok(HashSet::new());
    }
    let boss = crate::arena::boss_events(&spec.enemy_spec()).ok_or("arena has no events")?;
    let path = cache.join(format!("raw/event/{}.emevd", boss.map));
    let file = Emevd::parse(&std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| e.to_string())?;
    let mut entities = HashSet::new();
    for entry in boss.events {
        let event = file.event(entry.id).ok_or("missing arena event")?;
        for start in file.starts_of(entry.id) {
            for instruction in event.resolve(&start.args) {
                if instruction.bank == 2005
                    && let Some(entity) = instruction.i32_at(0)
                {
                    entities.insert(entity);
                }
            }
        }
    }
    Ok(entities)
}

/// 12005210 selects the illusion sky and room state before cutscene 20000000's warp.
pub fn seed_scene(
    cache: &Path,
    boss: &BossEvents,
    scripts: &mut crate::events::EventScripts,
) -> Result<(), String> {
    if boss.boss_entity != MONKEYS.boss_entity {
        return Ok(());
    }
    let path = cache.join("raw/event/m20_00_00_00.emevd");
    let file = Emevd::parse(&std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| e.to_string())?;
    let event = file
        .event(12_005_210)
        .ok_or("missing illusion temple entrance event 12005210")?;
    for ins in event.instructions.iter().skip(11).take(15) {
        if matches!(ins.code(), (2012, 1) | (2005, 3)) {
            let entity = ins
                .i32_at(0)
                .ok_or("short illusion temple scene instruction")?;
            let on = ins
                .u8_at(4)
                .ok_or("short illusion temple scene instruction")?
                != 0;
            scripts.cast_scene.enabled.insert(entity, on);
        }
    }
    Ok(())
}

pub fn sky_override(boss_entity: i32) -> Option<&'static str> {
    (boss_entity == MONKEYS.boss_entity).then_some("m900085")
}

/// The shared NpcParam must not erase Inner Father's distinct MSB think identity.
pub fn select_placed_think(spec: &ArenaSpec, enemy: &mut EnemySpec) {
    if spec.name == HIRATA_INNER_FATHER.name
        && enemy.chr == "c5060"
        && enemy.npc_param == 50_601_010
    {
        enemy.think = 50_630_000;
    }
}

pub fn arena_enemy(spec: &ArenaSpec) -> Option<EnemySpec> {
    if ![
        HIRATA_HALL.name,
        HIRATA_FATHER.name,
        HIRATA_INNER_FATHER.name,
        SENPOU_SCREENS.name,
    ]
    .contains(&spec.name)
    {
        return None;
    }
    let mut enemy = EnemySpec::by_name(spec.enemy).expect("arena enemy is extracted");
    select_placed_think(spec, &mut enemy);
    Some(enemy)
}

pub fn identity_events(enemy: &EnemySpec) -> Option<&'static BossEvents> {
    (enemy.chr == "c5060" && enemy.npc_param == 50_601_010 && enemy.think == 50_630_000)
        .then_some(&INNER_FATHER)
}

pub fn retired_butterfly(boss: i32, entity: i32) -> bool {
    boss == BUTTERFLY_SECOND.boss_entity && entity == BUTTERFLY.boss_entity
}

/// Entry instructions where the original event has no label at the post-intro boundary.
/// Verify the command at the offset before joining, so changed extracts fail explicitly.
pub fn join(map: &str, id: i64, _entries: &[EventEntry]) -> Option<(usize, (i32, i32))> {
    match (map, id) {
        ("m10_00_00_00", 11_005_810) => Some((15, (2008, 4))),
        ("m10_00_00_00", 11_005_800) => Some((21, (4, 5))),
        ("m20_00_00_00", 12_005_810) => Some((29, (2003, 2))),
        _ => None,
    }
}

/// Expand each instance of parameterized arena events, including every illusion/screen slot.
pub fn all_slots(map: &str, id: i64) -> bool {
    (map == "m10_00_00_00" && [11_005_820, 11_005_858, 11_005_860, 11_005_870].contains(&id))
        || (map == "m20_00_00_00" && [12_005_820, 12_005_890, 12_005_895, 12_005_900].contains(&id))
}

/// Keep an unavailable action-button/AI-status condition from firing its body.
/// These set-piece interactions await their engine hosts, not a made-up success response.
pub fn unavailable_condition(event: i64, code: (i32, i32)) -> bool {
    ((11_005_800..=11_005_881).contains(&event) || (12_005_800..=12_005_900).contains(&event))
        && matches!(code, (4, 9) | (5, 2) | (13, 1) | (3, 3) | (3, 1))
}

/// EMEVD 4[2] stores group/entity/comparison/ratio at byte offsets 0/4/8/12.
pub fn hp_ratio_condition(
    ins: &sekiro_formats::emevd::Instruction,
    host: &dyn crate::events::EventHost,
) -> bool {
    let Some(entity) = ins.i32_at(4) else {
        return false;
    };
    let Some((hp, max)) = host.hp(entity).zip(host.max_hp(entity)) else {
        return false;
    };
    let Some(value) = ins.f32_at(12) else {
        return false;
    };
    if max <= 0 {
        return false;
    }
    let ratio = hp as f32 / max as f32;
    match ins.u8_at(8) {
        Some(0) => ratio == value,
        Some(1) => ratio != value,
        Some(2) => ratio > value,
        Some(3) => ratio < value,
        Some(4) => ratio >= value,
        Some(5) => ratio <= value,
        _ => false,
    }
}

/// Initial display/collision alternatives from m10 constructor's common 20004114/15 calls.
/// Read the raw MSB because layout.json omits piece entities and entity groups.
pub fn hidden_parts(cache: &Path, spec: &ArenaSpec) -> Result<HashSet<String>, String> {
    if spec.map != "m10_00_00_00" {
        return Ok(HashSet::new());
    }
    let flags = if spec.name == HIRATA_HALL.name {
        BUTTERFLY.flags
    } else {
        FATHER.flags
    };
    let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let file = Emevd::parse(&read(&cache.join("raw/event/m10_00_00_00.emevd"))?)
        .map_err(|e| e.to_string())?;
    let map = msb::parse(&read(&cache.join("raw/map/mapstudio/m10_00_00_00.msb"))?)
        .map_err(|e| e.to_string())?;
    let mut inactive = HashSet::new();
    for start in file
        .starts()
        .iter()
        .filter(|s| s.common && s.from_event == 0)
    {
        if start.event == 20_004_115 {
            let a = start.arg_i32(0).ok_or("short 20004115 start")?;
            let b = start.arg_i32(1).ok_or("short 20004115 start")?;
            let flag = start.arg_i32(2).ok_or("short 20004115 start")?;
            inactive.insert(if flags.contains(&flag) { a } else { b });
        } else if start.event == 20_004_114 {
            let active = (0..4).any(|n| {
                let flag = start.arg_i32(n + 1).unwrap_or(0);
                flag != 0 && flags.contains(&flag) == (start.args.get(20 + n) == Some(&1))
            });
            if !active {
                inactive.insert(start.arg_i32(0).ok_or("short 20004114 start")?);
            }
        }
    }
    Ok(map
        .parts
        .iter()
        .filter(|p| {
            inactive.contains(&p.entity_id)
                || p.entity_group_ids.iter().any(|id| inactive.contains(id))
        })
        .map(|p| p.name.clone())
        .collect())
}

/// Zeroes the triangles of the collision parts `hidden` names; returns how many parts it removed.
pub fn remove_hidden_collision(
    mesh: &mut CollisionMesh,
    layout: &serde_json::Value,
    hidden: impl Fn(&str) -> bool,
) -> usize {
    let ranges: Vec<_> = layout["collisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["name"].as_str().is_some_and(&hidden))
        .filter_map(|r| {
            Some((
                r["triangles"][0].as_u64()? as usize,
                r["triangles"][1].as_u64()? as usize,
            ))
        })
        .collect();
    let removed = ranges.len();
    // Keep MSB triangle ranges and material indices valid for fog and draw-group selection.
    for (first, count) in ranges {
        let end = (first + count).min(mesh.indices.len() / 3);
        if first < end {
            for triangle in mesh.indices[first * 3..end * 3].as_chunks_mut::<3>().0 {
                *triangle = [0; 3];
            }
        }
    }
    removed
}
