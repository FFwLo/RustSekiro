//! Reviewed Hirata and Hall of Illusions staging, from the installed EMEVD and MSB.


use crate::arena::{ArenaSpec, BossEvents};
use crate::enemy::EnemySpec;
use crate::events::EventEntry;
use sekiro_formats::hknp::CollisionMesh;

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
