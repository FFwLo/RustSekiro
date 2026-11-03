//! Story time of day. Sekiro's light does not follow a clock: the map's event script picks it
//! from event flags, and the draw params key their light sets by hour.
//!
//! What the game's own scripts say (`event/common.emevd`, read with `emevd_dump`):
//!
//! - Event 100 keeps the sky mesh and environment variant of the castle and the outskirts in
//!   step with the story-time flags 9800 to 9803, which are cumulative: none on is morning
//!   (environment variant 1), 9800 afternoon (2), 9800 and 9801 evening (3), up to 9802 night
//!   (4) and all four dawn (5).
//! - Event 820 raises them from the bosses beaten: 9800 once one of the four time-advancing
//!   bosses (flags 9303 to 9306) is down, 9801 once three are.
//! - `SetLighting` (2003\[81\]) names the same variants 1 to 4 for a cutscene or a phase.
//!
//! Our draw params and probes are keyed by hour (0, 2, 6, 12, 18, 22), so each variant maps to
//! the hour of its authored key (`sky.rs`, `lighting.rs`).

/// The bosses whose defeat advances the story clock, in `common.emevd` event 820.
pub const TIME_BOSS_FLAGS: [i32; 4] = [9303, 9304, 9305, 9306];
/// Defeated time-advancing bosses that raise flag 9800 (afternoon) and flag 9801 (evening).
pub const AFTERNOON_AFTER: usize = 1;
pub const EVENING_AFTER: usize = 3;
/// The cumulative story-time flags of event 100.
pub const TIME_FLAGS: [i32; 4] = [9800, 9801, 9802, 9803];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryTime {
    /// The fixed Hirata memory environment, selected by common event 100 Label2.
    PastNight,
    Morning,
    Afternoon,
    Evening,
    Night,
    Dawn,
}

impl StoryTime {
    /// The hour of the authored light, probe and sky key for this time (0 h is the late-castle
    /// "past night" variant, which no flag state of event 100 selects).
    pub fn hour(self) -> f32 {
        match self {
            Self::PastNight => 0.0,
            Self::Morning => 6.0,
            Self::Afternoon => 12.0,
            Self::Evening => 18.0,
            Self::Night => 22.0,
            Self::Dawn => 2.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::PastNight => "past night",
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
            Self::Dawn => "dawn",
        }
    }

    /// The environment variant `SetLighting` and event 100's `SetAreaEnvmap` use.
    pub fn from_variant(variant: i32) -> Option<Self> {
        Some(match variant {
            0 => Self::PastNight,
            1 => Self::Morning,
            2 => Self::Afternoon,
            3 => Self::Evening,
            4 => Self::Night,
            5 => Self::Dawn,
            _ => return None,
        })
    }

    /// The time the story flags describe, `is_set` answering event flags. Time flags that are
    /// already on win; the bosses beaten raise the first two as event 820 does.
    pub fn from_flags(is_set: impl Fn(i32) -> bool) -> Self {
        let beaten = TIME_BOSS_FLAGS.iter().filter(|&&f| is_set(f)).count();
        let on = |i: usize, after: usize| is_set(TIME_FLAGS[i]) || beaten >= after;
        let (f800, f801) = (on(0, AFTERNOON_AFTER), on(1, EVENING_AFTER));
        match (f800, f801, is_set(TIME_FLAGS[2]), is_set(TIME_FLAGS[3])) {
            (false, ..) => Self::Morning,
            (true, false, ..) => Self::Afternoon,
            (_, true, false, _) => Self::Evening,
            (_, _, true, false) => Self::Night,
            _ => Self::Dawn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock follows the bosses beaten as event 820 does, and the thresholds and flag ranges
    /// above are the ones the installed event script uses (skipped without `cache/raw`).
    #[test]
    fn story_time_follows_bosses_beaten_and_the_event_script() {
        let beaten = |n: usize| StoryTime::from_flags(|f| TIME_BOSS_FLAGS[..n].contains(&f));
        let times = [0, 1, 2, 3, 4].map(beaten);
        assert_eq!(times[0], StoryTime::Morning);
        assert_eq!(times[1..3], [StoryTime::Afternoon; 2]);
        assert_eq!(times[3..], [StoryTime::Evening; 2]);
        assert_eq!(
            StoryTime::from_flags(|f| (9800..=9802).contains(&f)),
            StoryTime::Night
        );
        assert_eq!(StoryTime::from_variant(3).map(StoryTime::hour), Some(18.0));

        use sekiro_formats::emevd::Emevd;
        let path = crate::cache::root().join("raw/event/common.emevd");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let file = Emevd::parse(&bytes).unwrap();
        let ev = file.event(820).unwrap();
        // 3[10](group, first flag, last flag, comparison, count): at least 1, then 3, of 9303-9306.
        let counts: Vec<_> = ev
            .instructions
            .iter()
            .filter(|i| i.code() == (3, 10))
            .map(|i| (i.i32_at(4), i.i32_at(8), i.i32_at(16)))
            .collect();
        assert_eq!(
            counts,
            [
                (Some(9303), Some(9306), Some(1)),
                (Some(9303), Some(9306), Some(3))
            ]
        );
        let sets: Vec<_> = ev
            .instructions
            .iter()
            .filter(|i| i.code() == (2003, 2))
            .filter_map(|i| i.i32_at(0))
            .collect();
        assert_eq!(sets, [9800, 9800, 9801]);
        assert!(
            file.event(100)
                .unwrap()
                .instructions
                .iter()
                .any(|i| i.code() == (3, 1)
                    && i.i32_at(4) == Some(9800)
                    && i.i32_at(8) == Some(9803))
        );
    }
}
