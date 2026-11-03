//! Reservoir and Sunken Valley staging, reviewed against the installed EMEVDs.


use sekiro_formats::emevd::Instruction;

use crate::arena::{ArenaSpec, BossEvents};
use crate::events::EventEntry;
#[path = "arena_c_regions.rs"]
pub mod regions;

#[derive(Debug, Clone, Default)]
pub struct EventState {
    /// SetCharacterEventTarget, used by the two apes' TARGET_EVENT queries.
    pub targets: std::collections::HashMap<i32, i32>,
    pub bone_warps: std::collections::HashMap<i32, (i32, i16)>,
    /// SetAreaGparamSubID(area, block, sub-id, transition seconds), from 11125872/860.
    pub gparam: Option<(i16, i16, u8, f32)>,
}

impl EventState {
    pub fn instruction(&mut self, ins: &Instruction) -> bool {
        if ins.code() == (2004, 11)
            && let (Some(entity), Some(target)) = (ins.i32_at(0), ins.i32_at(4))
        {
            self.targets.insert(entity, target);
            return true;
        }
        if ins.code() == (2004, 41)
            && ins.args.get(4) == Some(&2)
            && let (Some(entity), Some(destination), Some(dummy)) =
                (ins.i32_at(0), ins.i32_at(8), ins.i32_at(12))
        {
            self.bone_warps.insert(entity, (destination, dummy as i16));
            return true;
        }
        if ins.code() != (2003, 88) {
            return false;
        }
        let (Some(area), Some(block), Some(sub), Some(seconds)) =
            (ins.i32_at(0), ins.i32_at(4), ins.u8_at(8), ins.f32_at(12))
        else {
            return false;
        };
        self.gparam = Some((area as i16, block as i16, sub, seconds));
        true
    }
}

const fn event(id: i64) -> EventEntry {
    EventEntry {
        id,
        from_label: None,
    }
}

/// Opening Genichiro: 11125810 setup, 11125800 outcome, 11125812 boundary immortality.
/// 11125811 places fog asset 1121800 with start 11125810 and end 9300.
/// Common event 100 selects environment 0 / sky group 8907040 under 8306 without time flags.
pub const RESERVOIR_GENICHIRO: BossEvents = BossEvents {
    map: "m11_02_00_00",
    boss_entity: 1_120_800,
    flags: &[8306, 11_125_810, 11_120_810],
    events: &[event(11_125_810), event(11_125_800), event(11_125_812)],
    fog_events: &[11_125_811],
    lighting: Some(0),
};

/// Final Tomoe: 11125840 Label2 joins after cutscene 11020030 at warp region 1122800.
/// 11125830 hands over to story Isshin; 11125841 seals the entrance until that hand-over.
pub const RESERVOIR_TOMOE: BossEvents = BossEvents {
    map: "m11_02_00_00",
    boss_entity: 1_120_830,
    flags: &[
        8302, 9300, 9303, 9304, 9305, 9306, 9800, 9801, 9802, 11_125_831,
    ],
    events: &[
        EventEntry {
            id: 11_125_840,
            from_label: Some(2),
        },
        event(11_125_830),
        event(11_125_842),
    ],
    fog_events: &[11_125_841, 11_125_871],
    lighting: None,
};

/// Story Isshin 1120860, not the 1120890 rematch disabled by 11125370.
/// 11125870 Label1 starts his bar; 11125872 drives the spear/storm phase and model 1127820.
/// 11125860/873/874/875/877 handle finishing messages, immortality and arena boundary.
pub const SWORD_SAINT: BossEvents = BossEvents {
    map: "m11_02_00_00",
    boss_entity: 1_120_860,
    flags: &[
        8302, 9300, 9303, 9304, 9305, 9306, 9311, 9800, 9801, 9802, 11_125_830, 11_120_830,
        11_125_861,
    ],
    events: &[
        EventEntry {
            id: 11_125_870,
            from_label: Some(1),
        },
        event(11_125_860),
        event(11_125_872),
        event(11_125_873),
        event(11_125_874),
        event(11_125_875),
        event(11_125_877),
    ],
    fog_events: &[11_125_871],
    lighting: None,
};

/// Guardian Ape: 11705810 setup, 11705800 victory, 11705820-27 phase/death messages,
/// 11705835 false victory, 11705833 phase breath effects.
/// 11705811 places fog 1701800 using start 11705801 and end 9304.
pub const GUARDIAN_APE: BossEvents = BossEvents {
    map: "m17_00_00_00",
    boss_entity: 1_700_800,
    flags: &[9303, 11_705_801, 11_700_801],
    events: &[
        event(11_705_810),
        event(11_705_800),
        event(11_705_820),
        event(11_705_821),
        event(11_705_822),
        event(11_705_823),
        event(11_705_824),
        event(11_705_825),
        event(11_705_826),
        event(11_705_827),
        event(11_705_833),
        event(11_705_835),
    ],
    fog_events: &[11_705_811],
    lighting: None,
};

/// Headless Ape: 11705860 setup, 11705850 victory, 11705870-73 finishing messages,
/// 11705880 companion appearance and 11705884 companion dismissal.
/// 11705861 places 1701850/851 with start 11705851 and end 9307.
/// 8303 enables the burrow encounter; 9314 must stay off until immortality is severed.
pub const HEADLESS_APE: BossEvents = BossEvents {
    map: "m17_00_00_00",
    boss_entity: 1_700_850,
    flags: &[8303, 9303, 9304, 11_700_851, 11_705_851],
    events: &[
        event(11_705_860),
        event(11_705_850),
        event(11_705_870),
        event(11_705_871),
        event(11_705_872),
        event(11_705_873),
        event(11_705_880),
        event(11_705_884),
    ],
    fog_events: &[11_705_861],
    lighting: None,
};

pub const SILVERGRASS: ArenaSpec = ArenaSpec {
    name: "silvergrass",
    map: "m11_02_00_00",
    boss_part: "c7110_0000",
    enemy: "c7110:71101000",
    radius: 100.0,
    player_start: None,
    hidden_models: &[],
    beaten: &[9303, 9304, 9305, 9306, 9802],
};
pub const SILVERGRASS_GENICHIRO: ArenaSpec = ArenaSpec {
    name: "silvergrass-genichiro",
    map: "m11_02_00_00",
    boss_part: "c7100_0000",
    enemy: "c7100:71000000",
    radius: 100.0,
    player_start: None,
    hidden_models: &[],
    beaten: &[],
};
pub const SILVERGRASS_ISSHIN: ArenaSpec = ArenaSpec {
    name: "silvergrass-isshin",
    map: "m11_02_00_00",
    boss_part: "c5400_0000",
    enemy: "c5400:54000000",
    radius: 100.0,
    player_start: None,
    hidden_models: &[],
    beaten: &[9303, 9304, 9305, 9306, 9802],
};
pub const WATERING_HOLE: ArenaSpec = ArenaSpec {
    name: "watering-hole",
    map: "m17_00_00_00",
    boss_part: "c5100_0000",
    enemy: "c5100:51000000",
    radius: 110.0,
    player_start: Some("c0000_0009"),
    hidden_models: &[],
    beaten: &[9303],
};
pub const APES_BURROW: ArenaSpec = ArenaSpec {
    name: "apes-burrow",
    map: "m17_00_00_00",
    boss_part: "c5100_0001",
    enemy: "c5100:51000100",
    radius: 80.0,
    player_start: Some("c0000_0007"),
    hidden_models: &[],
    beaten: &[9303, 9304],
};

/// Events without a post-intro label join at reviewed instruction boundaries.
/// Keep the real setup commands before the proximity wait, then the real fight suffix.
pub fn join_intro(id: i64, code: &mut Vec<Instruction>) -> Result<(), String> {
    let ranges: &[std::ops::Range<usize>] = match id {
        11_125_810 => &[4..5, 81..93],
        11_705_810 => &[10..17, 22..34],
        11_705_860 => &[42..49, 53..65],
        _ => return Ok(()),
    };
    let mut joined = Vec::new();
    for range in ranges {
        joined.extend_from_slice(
            code.get(range.clone())
                .ok_or_else(|| format!("event {id}: missing reviewed post-intro instructions"))?,
        );
    }
    *code = joined;
    Ok(())
}
