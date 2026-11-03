//! What the AI sees each tick, filled by the simulation from its characters.

use std::collections::HashMap;

/// One character as the AI sees it.
#[derive(Debug, Clone, Default)]
pub struct AiActor {
    pub position: [f32; 3],
    /// World yaw in the simulation's convention (see `crate::body`): yaw 0 faces -Z.
    pub yaw: f32,
    pub hp: f32,
    pub max_hp: f32,
    /// Remaining posture ("stamina" in the data) and its maximum.
    pub posture: f32,
    pub max_posture: f32,
    /// Active SpEffect ids (resident, TAE-driven and timed).
    pub sp_effects: Vec<i32>,
    /// In a guarding state.
    pub guarding: bool,
}

/// The AI's view of the world for one tick.
#[derive(Debug, Clone, Default)]
pub struct AiWorld {
    pub dt: f32,
    pub me: AiActor,
    /// The enemy target (the player), if known.
    pub target: Option<AiActor>,
    /// Character collision radius (NpcParam `hitRadius`), metres.
    pub hit_radius: f32,
    /// Numeric id of the full-body animation playing now, as the action codes use it
    /// (3000 for `a000_003000`), if any.
    pub current_anim: Option<i32>,
    /// Current clip clock, used to distinguish a repeated action from the previous swing.
    pub current_anim_time: f32,
    /// The playing animation has reached a window where the next combo attack is accepted
    /// (the NPC's TAE cancel window for attacks), or has ended.
    pub combo_window: bool,
    /// Where the character spawned (POINT_INITIAL).
    pub home: [f32; 3],
    /// The target's current attack has its parry-timing TAE action flag (63) active (the
    /// engine's parry-timing signal to the AI); its rising edge fires INTERUPT_FindAttack and
    /// INTERUPT_ParryTiming.
    pub parry_timing: bool,
    /// An attack animation is playing on the target, including its wind-up.
    pub target_attacking: bool,
    /// I took damage this step (INTERUPT_Damaged).
    pub damaged: bool,
}

/// The NpcThinkParam row the AI consults through `GetExcelParam` and the think id.
#[derive(Debug, Clone, Default)]
pub struct ThinkParams {
    pub id: i32,
    pub logic_id: i32,
    pub battle_goal_id: i32,
    /// Every field of the row by Paramdex name.
    pub fields: HashMap<String, f64>,
}

impl ThinkParams {
    /// Reads the row `id` from a `NpcThinkParam` table.
    pub fn from_row(row: &sekiro_formats::param::Row) -> sekiro_formats::Result<Self> {
        let mut fields = HashMap::new();
        for (def, value) in row.fields() {
            if let Some(v) = value.as_f64() {
                fields.insert(def.name.clone(), v);
            }
        }
        Ok(Self {
            id: row.id(),
            logic_id: row.i32("logicId")?,
            battle_goal_id: row.i32("battleGoalID")?,
            fields,
        })
    }
}

/// The numeric animation id the AI's action codes use, from a clip name such as `a000_003000`
/// (3000) or `a000_401040` (401040).
pub fn anim_id_from_clip(name: &str) -> Option<i32> {
    name.rsplit('_').next()?.parse().ok()
}
