//! Reviewed Mibu and Fountainhead staging, read from the installed MSB and EMEVD.



use super::{ArenaSpec, BossEvents};
use crate::events::EventEntry;

const fn event(id: i64) -> EventEntry {
    EventEntry {
        id,
        from_label: None,
    }
}

/// m15 events 11505800/810/812/814 handle defeat, entry, finishing request and illusion
/// cleanup; 11505849 starts common fog event 20005820. Join the undefeated entry label.
pub const MIBU_EVENTS: BossEvents = BossEvents {
    map: "m15_00_00_00",
    boss_entity: 1_500_800,
    flags: &[11_505_801],
    events: &[
        EventEntry {
            id: 11_505_810,
            from_label: Some(0),
        },
        event(11_505_800),
        event(11_505_812),
        event(11_505_814),
    ],
    fog_events: &[11_505_849],
    lighting: None,
};

/// m25 events 12505990/960 initialise and start the bridge encounter; 12505961/962
/// mark phases, 12505963..965 finish it, 12505970/980..983 control the illusions,
/// 12505950 handles defeat, and 12505999 places both fog assets.
pub const TRUE_MONK_EVENTS: BossEvents = BossEvents {
    map: "m25_00_00_00",
    boss_entity: 2_500_850,
    flags: &[9303, 9304, 9305, 9306, 9308, 9800, 9801, 9802, 12_505_951],
    events: &[
        event(12_505_990),
        EventEntry {
            id: 12_505_960,
            from_label: Some(0),
        },
        event(12_505_961),
        event(12_505_962),
        event(12_505_963),
        event(12_505_964),
        event(12_505_965),
        event(12_505_970),
        event(12_505_980),
        event(12_505_981),
        event(12_505_982),
        event(12_505_983),
        event(12_505_984),
        event(12_505_950),
    ],
    fog_events: &[12_505_999],
    lighting: None,
};

/// m25 event 12505801 starts the Old Dragons at warp point 2502821 and lighting 2;
/// 12505885 transitions to Divine Dragon at point 2502824 and lighting 1.
/// 12505803/837 count the Old Dragons; 12505880 defeats the Divine Dragon.
/// 12505802 starts only common event 20005830, with no 20005820/825 fog wall.
pub const OLD_DRAGONS_EVENTS: BossEvents = BossEvents {
    map: "m25_00_00_00",
    boss_entity: 2_500_810,
    flags: &[9303, 9304, 9305, 9306, 9308, 9309, 9800, 9801, 9802, 9200],
    events: &[
        EventEntry {
            id: 12_505_801,
            from_label: Some(0),
        },
        event(12_505_803),
        event(12_505_837),
        event(12_505_834),
        event(12_505_840),
        event(12_505_841),
        event(12_505_851),
        event(12_505_870),
        event(12_505_871),
        event(12_505_879),
        event(12_505_885),
        event(12_505_850),
        event(12_505_880),
    ],
    fog_events: &[],
    lighting: Some(2),
};

/// The resumed Divine Dragon branch of event 12505885 (Label20), plus its authored
/// tree visibility (12505895/901), opening phase (12505887..890), and defeat (12505880).
pub const DIVINE_DRAGON_EVENTS: BossEvents = BossEvents {
    map: "m25_00_00_00",
    boss_entity: 2_500_800,
    flags: &[
        9303, 9304, 9305, 9306, 9308, 9309, 9800, 9801, 9802, 9318, 9200, 12_505_882,
    ],
    events: &[
        EventEntry {
            id: 12_505_885,
            from_label: Some(20),
        },
        event(12_505_880),
        event(12_505_887),
        event(12_505_888),
        event(12_505_889),
        event(12_505_890),
        event(12_505_891),
        event(12_505_894),
        event(12_505_895),
        event(12_505_901),
        event(12_505_906),
        event(12_505_911),
        event(12_505_916),
        event(12_505_921),
        event(12_505_926),
    ],
    fog_events: &[],
    lighting: Some(1),
};

pub const MIBU: ArenaSpec = ArenaSpec {
    name: "mibu-bridge",
    map: "m15_00_00_00",
    boss_part: "c5000_0000",
    enemy: "c5000:50001000",
    radius: 60.0,
    player_start: Some("c0000_0004"),
    hidden_models: &[],
    // The branch is reachable before the other three ingredients: no mandatory earlier
    // time boss. This chooses the early-story visit, without inventing a required defeat.
    beaten: &[],
};

pub const TRUE_MONK: ArenaSpec = ArenaSpec {
    name: "fountainhead-bridge",
    map: "m25_00_00_00",
    boss_part: "c5000_0000",
    enemy: "c5000:50000000",
    radius: 70.0,
    // Inner bridge player start, inside entry region 2502850 and both fog planes.
    // c0000_0001 is outside the entrance fog and cannot start an already sealed fight.
    player_start: Some("c0000_0002"),
    hidden_models: &[],
    beaten: &[9303, 9304, 9305, 9306, 9800, 9801, 9802],
};

pub const SANCTUARY: ArenaSpec = ArenaSpec {
    name: "fountainhead-sanctuary",
    map: "m25_00_00_00",
    boss_part: "c5300_0025",
    enemy: "c5300:53000000",
    radius: 150.0,
    player_start: Some("region:2502821"),
    hidden_models: &[],
    beaten: &[9303, 9304, 9305, 9306, 9800, 9801, 9802],
};

/// Every placed slot of the phantom, Old Dragon and tree-prop events must run.
pub fn all_slots(map: &str, id: i64) -> bool {
    map == "m25_00_00_00"
        && matches!(
            id,
            12_505_803
                | 12_505_834
                | 12_505_841
                | 12_505_851
                | 12_505_901
                | 12_505_906
                | 12_505_911
                | 12_505_916
                | 12_505_921
                | 12_505_970
                | 12_505_984
        )
}

/// The Dragon opening events use their own completion flags as subsequent waits.
pub fn completion_flag(id: i64) -> bool {
    matches!(id, 12_505_885 | 12_505_888 | 12_505_889 | 12_505_890)
}
