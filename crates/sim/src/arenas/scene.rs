//! Presentation state from arena EMEVD instructions, independent of combat AI.

use std::collections::HashMap;

use sekiro_formats::{emedf::Arg, emevd::Instruction};

#[derive(Debug, Clone, Default)]
pub struct SceneState {
    /// Collision enablement is separate from rendered map-part enablement.
    pub collisions_enabled: HashMap<i32, bool>,
    pub enabled: HashMap<i32, bool>,
    /// Recency of active map-part commands, used when an event overrides the common sky.
    pub enabled_order: Vec<i32>,
    pub animations: HashMap<i32, (i32, bool)>,
    pub animation_enabled: HashMap<i32, bool>,
    pub masks: HashMap<i32, HashMap<u8, bool>>,
    pub effects: HashMap<i32, bool>,
    /// Map-authored effect state retained for a character before it occupies a combat slot.
    pub character_effects: HashMap<(i32, i32), bool>,
    pub lighting: Option<i32>,
    pub environment: Option<u8>,
    /// Area/block overrides are independent and survive until the event state restarts.
    pub gparam_sub_ids: HashMap<(i16, i16), (u8, f32)>,
    /// Region warps for supporting actors; bone-relative attachments need the actor rig.
    pub warps: HashMap<i32, i32>,
    pub backread_enabled: HashMap<i32, bool>,
    /// Animated objects follow a character's authored dummy instead of their MSB origin.
    pub attachments: HashMap<i32, (i32, i16)>,
}

impl SceneState {
    /// Returns true only for instructions fully consumed by the presentation state.
    /// Character enables are also retained here, but still reach the combat interpreter.
    pub fn instruction(&mut self, ins: &Instruction, args: &[Arg]) -> bool {
        let i = |n: usize| args.get(n).map_or(0, |v| v.as_i64());
        match ins.code() {
            (2011, 1) => {
                self.collisions_enabled
                    .insert(ins.i32_at(0).unwrap_or(0), ins.u8_at(4) != Some(0));
                true
            }
            (2003, 80) => {
                self.environment = ins.u8_at(0);
                true
            }
            (2003, 88) => {
                if let (Some(area), Some(block), Some(sub), Some(seconds)) =
                    (ins.i32_at(0), ins.i32_at(4), ins.u8_at(8), ins.f32_at(12))
                {
                    self.gparam_sub_ids
                        .insert((area as i16, block as i16), (sub, seconds));
                }
                true
            }
            (2003, 58) => {
                self.attachments.insert(
                    ins.i32_at(8).unwrap_or(0),
                    (
                        ins.i32_at(0).unwrap_or(0),
                        ins.i32_at(4).unwrap_or(0) as i16,
                    ),
                );
                true
            }
            (2004, 8) | (2004, 21) => {
                self.character_effects
                    .insert((i(0) as i32, i(1) as i32), ins.id == 8);
                false
            }
            (2004, 29) => {
                self.backread_enabled.insert(i(0) as i32, i(1) == 0);
                true
            }
            (2004, 41) if i(1) == 1 => {
                self.warps.insert(i(0) as i32, i(2) as i32);
                true
            }
            (2004, 42) if ins.i32_at(4) == Some(1) => {
                self.warps
                    .insert(ins.i32_at(0).unwrap_or(0), ins.i32_at(8).unwrap_or(0));
                true
            }
            (2004, 5) => {
                self.enabled.insert(i(0) as i32, i(1) != 0);
                false
            }
            (2005, 3) | (2012, 1) => {
                self.enabled.insert(i(0) as i32, i(1) != 0);
                if ins.code() == (2012, 1) {
                    self.enabled_order.retain(|&entity| entity != i(0) as i32);
                    if i(1) != 0 {
                        self.enabled_order.push(i(0) as i32);
                    }
                }
                true
            }
            (2003, 18) => {
                self.animations
                    .insert(i(0) as i32, (i(1) as i32, i(2) != 0));
                true
            }
            (2004, 39) => {
                self.animation_enabled.insert(i(0) as i32, i(1) != 0);
                true
            }
            (2004, 26) => {
                self.masks
                    .entry(i(0) as i32)
                    .or_default()
                    .insert(i(1) as u8, i(2) != 0);
                true
            }
            (2006, 1) => {
                self.effects.insert(i(0) as i32, false);
                true
            }
            (2006, 2) => {
                self.effects.insert(i(0) as i32, true);
                true
            }
            (2003, 81) => {
                self.lighting = Some(i(0) as i32);
                true
            }
            _ => false,
        }
    }
}
