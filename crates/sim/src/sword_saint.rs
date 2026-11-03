//! Sword Saint's two map requests share a slot but belong to consecutive TAE clips.

use crate::{character::Character, enemy::EnemySpec};

/// The Inner rematch changes event scripts while using the same installed Reservoir MSB.
pub fn world_map(map: &str) -> &str {
    if map == "m11_02_54_01" {
        "m11_02_00_00"
    } else {
        map
    }
}

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

/// The execution state owns death while its TAE and the map continuation are running.
/// Actual combat HP stays zero; this only stops HKS taking its ordinary HP-zero branch.
pub fn execution_owns_death(spec: &EnemySpec, character: &Character, marks: i32) -> bool {
    spec.chr == "c5400" && marks == 0 && character.tae_runtime.event_message != Some(30)
}

/// ThrowKind 250000 is the separate Mortal Blade execution after the last combat mark.
/// The kneeling loop supplies SpEffect 220400; the row supplies its actual search range.
pub fn finishing_rows(
    spec: &EnemySpec,
    character: &Character,
    table: &crate::deathblow::ThrowTable,
    attacker: &crate::deathblow::Placement,
    defender: &crate::deathblow::Placement,
) -> Option<(i32, i32)> {
    if spec.chr != "c5400" || !character.tae_frame().sp_effects.contains(&220400) {
        return None;
    }
    let row = table
        .rows
        .iter()
        .find(|r| r.atk_chr == 0 && r.def_chr == 5400 && r.throw_kind == 250000)?;
    row.in_range(attacker, defender).then_some((row.id, row.id))
}

#[derive(Debug, Clone, Default)]
pub struct FinishRequests {
    followup: Option<(u8, i32)>,
}

impl FinishRequests {
    /// Event 11125875's death-idle request must not replace 11125874's execution request.
    /// Message 20 releases the pair; the kneeling clip's action flag 152 releases its loop.
    pub fn receive(
        &mut self,
        spec: &EnemySpec,
        pending: &mut Option<(u8, i32)>,
        request: (u8, i32),
    ) {
        if spec.chr == "c5400" && request.1 == 21000 {
            self.followup = Some(request);
        } else {
            *pending = Some(request);
        }
    }

    pub fn release(&mut self, character: &mut Character, pending: &mut Option<(u8, i32)>) {
        if pending.is_none()
            && character.state() == "Event20200"
            && character.tae_frame().flag(152)
            && let Some((_, command)) = self.followup.take()
        {
            // This request is not polled by Event20200's HKS update, unlike ThrowDef13700.
            // Reconstruct the engine's queued event hand-off at its authored cancel window.
            character
                .behavior
                .runtime
                .fire_event(&format!("W_Event{command}"));
        }
    }
}

/// The final Tomoe row has different Lua from the castle's externally scripted plan table.
pub fn keep_lua(spec: &EnemySpec, brain: &str, log: bool) -> bool {
    matches!(
        (spec.chr.as_str(), spec.npc_param),
        ("c7110", 71101000) | ("c7100", 71000000)
    ) && brain == "scripted"
        && !log
}
