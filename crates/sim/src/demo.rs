//! Scripted input: a tiny text format for repeatable play sessions, and a built-in demo.
//!
//! One line per segment: `<frames> <stick_x> <stick_y> <buttons> [yaw_degrees
//! [pitch_degrees]]`, the pitch down positive.
//! Buttons are a comma list of `attack, guard, jump, dodge, crouch, grapple, prosthetic, item,
//! combat_art, lock_on, action` (`action` kneels at an idol or picks up), or `-` for none.
//! `#` starts a comment.
//!
//! The stick is camera-relative, like a player's: `stick_y` 1 is away from the camera, `stick_x`
//! 1 to its right. The yaw and pitch columns are offsets that turn the camera the row is played
//! from (`InputFrame::from_camera`), not world angles. In `--play` that camera is the orbit,
//! which starts at the spawn yaw (`--spawn x,y,z,yaw,pitch`, or the map start's facing) and
//! then moves only with the mouse, a pad's right stick, lock-on, a respawn or a cinematic, so a
//! forward stick runs along world yaw `spawn + column` (yaw 0 faces -Z, positive turns left, see
//! [`crate::arena::forward`]). To aim along a world direction `(dx, dz)`, write the column
//! `atan2(-dx, -dz)` in degrees minus the spawn yaw. A `0 -1` stick runs back toward the camera.

use crate::input::{Buttons, InputFrame};

/// Run, sprint, jump, a three-hit combo, a held guard, a step and a crouch, at 60 steps/s.
pub const DEMO: &str = "\
# idle
30 0 0 -
# run forward, then turn right while running
60 0 1 -
40 1 0 -
# sprint: hold dodge while running
1 0 1 dodge
50 0 1 dodge
20 0 0 -
# jump forward and land
1 0 1 jump
80 0 0 -
# three attacks: press, release, press again in the combo window (a press before the next
# slash's accept window would be dropped, as in the game)
1 0 0 attack
25 0 0 -
1 0 0 attack
40 0 0 -
1 0 0 attack
70 0 0 -
# hold guard, then release
1 0 0 guard
60 0 0 guard
50 0 0 -
# step dodge to the left (holding the direction, as a player would)
1 -1 0 dodge
25 -1 0 -
40 0 0 -
# crouch toggle
1 0 0 crouch
40 0 0 -
1 0 0 crouch
40 0 0 -
";

fn parse_buttons(s: &str) -> Result<Buttons, String> {
    let mut b = Buttons::default();
    if s == "-" {
        return Ok(b);
    }
    for name in s.split(',') {
        match name.trim() {
            "attack" => b.attack = true,
            "guard" => b.guard = true,
            "jump" => b.jump = true,
            "dodge" => b.dodge = true,
            "crouch" => b.crouch = true,
            "grapple" => b.grapple = true,
            "prosthetic" => b.prosthetic = true,
            "item" => b.item = true,
            "combat_art" => b.combat_art = true,
            "lock_on" => b.lock_on = true,
            "action" => b.action = true,
            other => return Err(format!("unknown button {other:?}")),
        }
    }
    Ok(b)
}

/// Expands a script into one [`InputFrame`] per step.
pub fn parse(text: &str) -> Result<Vec<InputFrame>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(format!("line {}: expected frames x y buttons", n + 1));
        }
        let num = |s: &str| s.parse::<f32>().map_err(|e| format!("line {}: {e}", n + 1));
        let frames = parts[0]
            .parse::<usize>()
            .map_err(|e| format!("line {}: {e}", n + 1))?;
        let degrees = |i: usize| -> Result<f32, String> {
            Ok(parts
                .get(i)
                .map(|s| num(s))
                .transpose()?
                .unwrap_or(0.0)
                .to_radians())
        };
        let frame = InputFrame {
            move_stick: [num(parts[1])?, num(parts[2])?],
            camera_yaw: degrees(4)?,
            camera_pitch: degrees(5)?,
            buttons: parse_buttons(parts[3]).map_err(|e| format!("line {}: {e}", n + 1))?,
        };
        out.extend(std::iter::repeat_n(frame, frames));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_parses() {
        let frames = super::parse(super::DEMO).unwrap();
        assert!(frames.len() > 600);
        assert!(frames[30].move_stick[1] == 1.0);
        assert!(super::parse("3 0 0 kick").is_err());
        assert!(super::parse("1 0 0 action").unwrap()[0].buttons.action);
        let aimed = super::parse("1 0 0 - 90 30").unwrap()[0];
        assert!((aimed.camera_yaw - 90f32.to_radians()).abs() < 1e-6);
        assert!((aimed.camera_pitch - 30f32.to_radians()).abs() < 1e-6);
        let aimed = super::parse("1 0 0 - 90 30").unwrap()[0];
        assert!((aimed.camera_yaw - 90f32.to_radians()).abs() < 1e-6);
        assert!((aimed.camera_pitch - 30f32.to_radians()).abs() < 1e-6);
    }
}
