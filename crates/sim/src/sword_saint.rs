//! Sword Saint's two map requests share a slot but belong to consecutive TAE clips.


pub const REMATCH_EVENTS: crate::arena::BossEvents = crate::arena::BossEvents {
    map: "m11_02_54_01",
    boss_entity: 1120890,
    // 2310 selects the rematch's defeat flag 9347 rather than its return-to-hub branch.
    flags: &[8302, 9300, 9303, 9304, 9305, 9306, 9800, 9801, 9802, 2310],
    events: &[
        crate::events::EventEntry {
            id: 11125870,
            from_label: None,
        },
        crate::events::EventEntry {
            id: 11125860,
            from_label: None,
        },
        crate::events::EventEntry {
            id: 11125872,
            from_label: None,
        },
        crate::events::EventEntry {
            id: 11125873,
            from_label: None,
        },
        crate::events::EventEntry {
            id: 11125874,
            from_label: None,
        },
        crate::events::EventEntry {
            id: 11125875,
            from_label: None,
        },
    ],
    fog_events: &[11125871],
    lighting: None,
};

/// Expose the second placed c5400 without changing any existing story arena selection.
pub const REMATCH_ARENA: crate::arena::ArenaSpec = crate::arena::ArenaSpec {
    name: "silvergrass-inner-isshin",
    boss_part: "c5400_0001",
    enemy: "inner-isshin",
    player_start: Some("region:1122800"),
    ..crate::arena_c::SILVERGRASS_ISSHIN
};
