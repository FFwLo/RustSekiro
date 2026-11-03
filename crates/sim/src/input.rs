//! Player input: logical buttons and the movement stick, translated into what the script reads.
//!
//! The script never sees keys. It asks the engine:
//!
//! - `env(1106, arm)`: is the action requested now. The engine latches a press inside the
//!   animation's accept window and reports it inside its execute window (see [`accept_flags`]).
//!   Starting an action calls `act(9101)`, which clears the buffer.
//! - `env(1108, arm)`: how long the button has been held. The script compares it with 0 and, for
//!   sprinting in swamps, with 200, so milliseconds is our working unit (unverified).
//! - Behaviour variables the engine writes every frame: `MoveSpeedLevel` (0 to 2, stick
//!   magnitude), `TurnAngle` and `MoveAngle` (degrees from the character's facing to the stick
//!   direction, positive to the right), and the per-action copies `JumpAngle`, `AttackAngle`,
//!   `JumpStickLevel`, `AttackStickLevel` and so on.
//!
//! Action-arm ids (`ACTION_ARM_*` in the define script): attack 0, prosthetic 1, guard 2, grapple
//! 3, jump 4, step/dodge 5, item 7, crouch 15, combat art 32. Sekiro's dodge button steps on a
//! press and sprints while held, which the script reads as 1106 and 1108 of arm 5.

use crate::tae::{TaeFrame, cancel};
use std::collections::{HashMap, HashSet};

/// Action-arm ids, as the script names them.
pub mod arm {
    pub const ATTACK: i32 = 0;
    pub const PROSTHETIC: i32 = 1;
    pub const GUARD: i32 = 2;
    pub const GRAPPLE: i32 = 3;
    pub const JUMP: i32 = 4;
    pub const DODGE: i32 = 5;
    pub const ITEM: i32 = 7;
    pub const ACTION: i32 = 12;
    pub const CROUCH: i32 = 15;
    pub const SWIM_ACCELERATION: i32 = 28;
    pub const SWIM_UP: i32 = 29;
    pub const SWIM_DOWN: i32 = 30;
    pub const COMBAT_ART: i32 = 32;
}

/// Logical buttons, true while held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Buttons {
    pub attack: bool,
    pub guard: bool,
    pub jump: bool,
    /// Step on press, sprint while held.
    pub dodge: bool,
    pub crouch: bool,
    pub grapple: bool,
    pub action: bool,
    pub prosthetic: bool,
    pub item: bool,
    pub combat_art: bool,
    pub lock_on: bool,
}

impl Buttons {
    fn arms(&self) -> [(i32, bool); 13] {
        [
            (arm::ATTACK, self.attack),
            (arm::GUARD, self.guard),
            (arm::JUMP, self.jump),
            (arm::DODGE, self.dodge),
            (arm::CROUCH, self.crouch),
            (arm::GRAPPLE, self.grapple),
            (arm::ACTION, self.action),
            (arm::PROSTHETIC, self.prosthetic),
            (arm::ITEM, self.item),
            (arm::COMBAT_ART, self.combat_art),
            (arm::SWIM_ACCELERATION, self.dodge),
            (arm::SWIM_UP, self.jump),
            (arm::SWIM_DOWN, self.crouch),
        ]
    }
}

/// One frame of player input.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InputFrame {
    /// Movement stick: x to the right, y forward, magnitude up to 1 (keyboard gives 1).
    pub move_stick: [f32; 2],
    /// Camera yaw in radians: the camera looks along `rotate_y(camera_yaw) * -Z`.
    pub camera_yaw: f32,
    /// Camera pitch in radians, down positive (0 looks level): with `camera_yaw`, the orbit the
    /// hook search looks through (`wire::Wire::search_camera`).
    pub camera_pitch: f32,
    pub buttons: Buttons,
}

impl InputFrame {
    /// A script row ([`crate::demo::parse`]) played from the camera at `camera_yaw` and
    /// `camera_pitch`: the row's yaw and pitch columns are offsets from that camera, so its stick
    /// is camera-relative like a player's. A forward stick then pushes along world yaw
    /// `camera_yaw + column` ([`InputState::stick_world_yaw`]; yaw 0 faces -Z, positive turns
    /// left, see [`crate::arena::forward`]).
    pub fn from_camera(self, camera_yaw: f32, camera_pitch: f32) -> Self {
        Self {
            camera_yaw: camera_yaw + self.camera_yaw,
            camera_pitch: camera_pitch + self.camera_pitch,
            ..self
        }
    }

    /// A forward stick of `level` pushing along world yaw `yaw`, whatever the camera (the
    /// `--script-route` steering). The same push as a script row with stick `0 level` and yaw
    /// column `yaw - camera` played [`Self::from_camera`] `camera`.
    pub fn toward_world_yaw(yaw: f32, level: f32) -> Self {
        Self {
            move_stick: [0.0, level],
            camera_yaw: yaw,
            ..Self::default()
        }
    }
}

/// Script extensions for docking's action button and the water movement aliases.
pub fn parse_action_script(text: &str) -> Result<Vec<InputFrame>, String> {
    let mut frames = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut fields: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        let mut action = false;
        if let Some(field) = fields.get_mut(3) {
            *field = field
                .split(',')
                .map(|b| match b {
                    "dive" => "crouch",
                    "surface" => "jump",
                    "swim-boost" => "dodge",
                    b => b,
                })
                .collect::<Vec<_>>()
                .join(",");
        }
        if let Some(buttons) = fields.get_mut(3) {
            let kept: Vec<&str> = buttons
                .split(',')
                .filter(|b| {
                    if *b == "action" {
                        action = true;
                        false
                    } else {
                        true
                    }
                })
                .collect();
            *buttons = if kept.is_empty() {
                "-".into()
            } else {
                kept.join(",")
            };
        }
        let mut segment = crate::demo::parse(&fields.join(" "))?;
        for frame in &mut segment {
            frame.buttons.action = action;
        }
        frames.extend(segment);
    }
    Ok(frames)
}

/// `ChrActionFlag` that accepts a press of every action (`AttackAction_Complex`).
pub const ACCEPT_ALL: i32 = 87;

/// The engine's action-request buffer works in two steps, each driven by `ChrActionFlag`
/// events of the current animations:
///
/// - Accept: a press is latched only while one of the action's accept flags is active. A press
///   outside them is dropped, so mashing before the window does nothing.
/// - Execute: a latched press is reported to the script (`env(1106)`) only while one of its
///   execute flags is active. It waits for that with no timer; `act(9101)` clears it.
///
/// The flag-to-action table is the exe's `ChrActionFlag` handler, traced by FwLo's RustSekiro
/// (`docs/kb/input-buffer.md`). Animations without any action flags (idle, locomotion loops)
/// accept and execute everything.
pub fn accept_flags(action: i32) -> Option<&'static [i32]> {
    Some(match action {
        arm::ATTACK => &[1],
        arm::PROSTHETIC => &[1, 136],
        arm::GUARD => &[9, 150],
        arm::COMBAT_ART => &[1, 9],
        arm::JUMP => &[25, 151],
        arm::DODGE => &[25],
        arm::ITEM => &[30],
        _ => return None,
    })
}

/// Execute flags of `action`; see [`accept_flags`].
pub fn execute_flags(action: i32) -> &'static [i32] {
    match action {
        arm::ATTACK => &[4, cancel::ATTACK],
        arm::PROSTHETIC => &[116, cancel::PROSTHETIC],
        arm::GUARD => &[16, cancel::GUARD],
        arm::COMBAT_ART => &[cancel::COMBAT_ART],
        arm::JUMP => &[cancel::JUMP],
        arm::DODGE => &[cancel::DODGE],
        arm::ITEM => &[cancel::ITEM],
        arm::ACTION => &[],
        arm::GRAPPLE => &[cancel::MOVE, cancel::JUMP],
        _ => &[cancel::MOVE],
    }
}

/// Whether a press of `action` is latched under the current flags.
pub fn accepts(action: i32, tae: &TaeFrame) -> bool {
    if !tae.restricted || tae.flag(ACCEPT_ALL) {
        return true;
    }
    match accept_flags(action) {
        Some(flags) => flags.iter().any(|&f| tae.flag(f)),
        None => true,
    }
}

/// Press buffering and hold times across frames.
#[derive(Debug, Default)]
pub struct InputState {
    prev: Buttons,
    /// Latched presses waiting for their execute window.
    pending: HashSet<i32>,
    /// Milliseconds each button has been held.
    held_ms: HashMap<i32, f32>,
    pub frame: InputFrame,
}

impl InputState {
    /// Takes this frame's input. `tae` holds the action flags the last animation update set,
    /// which is what the engine's request module sees when it reads the buttons.
    pub fn update(&mut self, frame: &InputFrame, dt: f32, tae: &TaeFrame) {
        let prev = self.prev.arms();
        for (i, (action, down)) in frame.buttons.arms().into_iter().enumerate() {
            if down && !prev[i].1 && accepts(action, tae) {
                self.pending.insert(action);
            }
            if down {
                *self.held_ms.entry(action).or_insert(0.0) += dt * 1000.0;
            } else {
                self.held_ms.remove(&action);
            }
        }
        self.prev = frame.buttons;
        self.frame = *frame;
    }

    /// `act(9101)`: the script started an action; buffered presses are spent.
    pub fn reset_requests(&mut self) {
        self.pending.clear();
    }

    /// `env(1106, action)` given the current TAE windows. A buffered guard fires only while the
    /// button is still held (a tap let go before its window is dropped, as in the game); other
    /// presses survive being let go.
    pub fn requested(&self, action: i32, tae: &TaeFrame) -> bool {
        if !self.pending.contains(&action) {
            return false;
        }
        if action == arm::GUARD && !self.frame.buttons.guard {
            return false;
        }
        // Docking and swim arms have no traced execute mapping. HKS owns their state gates.
        matches!(
            action,
            arm::ACTION | arm::SWIM_ACCELERATION | arm::SWIM_UP | arm::SWIM_DOWN
        ) || !tae.restricted
            || execute_flags(action).iter().any(|&f| tae.flag(f))
    }

    pub fn pending_actions(&self) -> impl Iterator<Item = i32> + '_ {
        self.pending.iter().copied()
    }

    /// `env(1108, action)` in milliseconds.
    pub fn held_ms(&self, action: i32) -> f32 {
        self.held_ms.get(&action).copied().unwrap_or(0.0)
    }

    pub fn held(&self) -> impl Iterator<Item = (i32, f32)> + '_ {
        self.held_ms.iter().map(|(a, t)| (*a, *t))
    }

    /// Stick magnitude, clamped to 1.
    pub fn stick_level(&self) -> f32 {
        let [x, y] = self.frame.move_stick;
        (x * x + y * y).sqrt().min(1.0)
    }

    /// World-space yaw of the stick direction (same convention as the camera), if pushed.
    pub fn stick_world_yaw(&self) -> Option<f32> {
        let [x, y] = self.frame.move_stick;
        if x * x + y * y < 0.01 {
            return None;
        }
        // Stick forward is the camera forward; stick right turns clockwise seen from above.
        Some(self.frame.camera_yaw + (-x).atan2(y))
    }
}

/// Signed smallest angle `to - from`, radians in (-pi, pi].
pub fn angle_diff(to: f32, from: f32) -> f32 {
    let mut d = (to - from) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d <= -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy(flags: &[i32]) -> TaeFrame {
        TaeFrame {
            restricted: true,
            flags: flags.iter().copied().collect(),
            ..TaeFrame::default()
        }
    }

    fn press(s: &mut InputState, buttons: Buttons, tae: &TaeFrame) {
        let f = InputFrame {
            buttons,
            ..InputFrame::default()
        };
        s.update(&f, 1.0 / 30.0, tae);
    }

    #[test]
    fn press_waits_for_execute_window_without_timer_until_reset() {
        let mut s = InputState::default();
        let attack = Buttons {
            attack: true,
            ..Buttons::default()
        };
        press(&mut s, attack, &busy(&[1]));
        // Let go and wait well past any short buffer: the latched press stays.
        for _ in 0..60 {
            press(&mut s, Buttons::default(), &busy(&[1]));
        }
        assert!(!s.requested(arm::ATTACK, &busy(&[1])));
        assert!(s.requested(arm::ATTACK, &busy(&[cancel::ATTACK])));
        s.reset_requests();
        assert!(!s.requested(arm::ATTACK, &TaeFrame::default()));
    }

    #[test]
    fn press_outside_accept_window_is_dropped() {
        let mut s = InputState::default();
        let attack = Buttons {
            attack: true,
            ..Buttons::default()
        };
        // Mashing before the accept flag does nothing.
        press(&mut s, attack, &busy(&[]));
        press(&mut s, Buttons::default(), &busy(&[1]));
        assert!(!s.requested(arm::ATTACK, &busy(&[cancel::ATTACK])));
        // The all-actions accept flag and free animations latch anything.
        press(&mut s, attack, &busy(&[ACCEPT_ALL]));
        assert!(s.requested(arm::ATTACK, &busy(&[cancel::ATTACK])));
        assert!(s.held_ms(arm::ATTACK) > 30.0);
    }

    #[test]
    fn buffered_guard_needs_the_button_held() {
        let mut s = InputState::default();
        let guard = Buttons {
            guard: true,
            ..Buttons::default()
        };
        press(&mut s, guard, &busy(&[9]));
        assert!(s.requested(arm::GUARD, &busy(&[cancel::GUARD])));
        press(&mut s, Buttons::default(), &busy(&[9]));
        assert!(!s.requested(arm::GUARD, &busy(&[cancel::GUARD])));
    }

    #[test]
    fn stick_yaw_follows_camera() {
        let mut s = InputState::default();
        s.update(
            &InputFrame {
                move_stick: [0.0, 1.0],
                camera_yaw: 0.5,
                ..InputFrame::default()
            },
            0.0,
            &TaeFrame::default(),
        );
        assert!((s.stick_world_yaw().unwrap() - 0.5).abs() < 1e-6);
        s.update(
            &InputFrame {
                move_stick: [1.0, 0.0],
                camera_yaw: 0.0,
                ..InputFrame::default()
            },
            0.0,
            &TaeFrame::default(),
        );
        // Right is a clockwise (negative) yaw.
        assert!((s.stick_world_yaw().unwrap() + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((angle_diff(3.0, -3.0) - (6.0 - std::f32::consts::TAU)).abs() < 1e-6);
    }

    /// A script row's yaw column turns the camera it is played from (#215): forward pushes along
    /// camera plus column, back along the opposite, and a route frame toward that world yaw is
    /// the same push.
    #[test]
    fn script_rows_push_along_camera_plus_yaw_column_like_a_route() {
        let world = |frame: InputFrame| {
            let mut s = InputState::default();
            s.update(&frame, 0.0, &TaeFrame::default());
            s.stick_world_yaw().unwrap()
        };
        let camera = (-158f32).to_radians();
        for (row, column, back) in [
            ("1 0 1 - 0", 0.0, false),
            ("1 0 1 - 90", 90.0, false),
            ("1 0 1 - -135 10", -135.0, false),
            ("1 0 -1 - 0", 0.0, true),
        ] {
            let parsed = parse_action_script(row).unwrap()[0];
            let played = parsed.from_camera(camera, 0.25);
            let turn = if back { std::f32::consts::PI } else { 0.0 };
            let want = camera + f32::to_radians(column) + turn;
            assert!(angle_diff(world(played), want).abs() < 1e-5, "{row}");
            let route = InputFrame::toward_world_yaw(want, 1.0);
            assert!(
                angle_diff(world(route), world(played)).abs() < 1e-5,
                "{row}"
            );
        }
        let pitched = parse_action_script("1 0 0 - 0 10").unwrap()[0].from_camera(0.0, 0.25);
        assert!((pitched.camera_pitch - (0.25 + 10f32.to_radians())).abs() < 1e-6);
    }
}
