//! `--input FILE`: replays raw keyboard and mouse input into Bevy's input messages, so a run
//! exercises the same path as a player at the keyboard (key state, accumulated mouse motion,
//! cursor grab, camera orbit, stick building) instead of the simulation-level `--script`.
//!
//! One line per segment of rendered frames: `<frames> <keys> [<mouse_dx> <mouse_dy>]`, where
//! keys is a comma list of `W A S D Q E Z X C V Space Shift Esc Enter Up Down Left Right F3 F8
//! Click MMB` (`Click` is the left mouse button, `MMB` the middle one, the game's lock-on; a key
//! named in consecutive segments stays held) or
//! `-`, and the mouse moves by the given counts every frame. `#` starts a comment. Frames count
//! from the first frame the simulation runs. A line `shot <path>` saves a screenshot there when
//! the replay gets to it, so a run can capture screens that pause the simulation (the in-game
//! menu), which `--shots` (counted in simulation steps) cannot.
//!
//! The replay owns the input: real keyboard, mouse button and mouse motion messages are dropped,
//! so someone using the machine meanwhile does not disturb the run. The run starts with the
//! cursor free, as after Esc; a `Click` takes it, as a player clicking into the window does.
//!
//! Every frame is logged to stderr as a `cam` line (frame, focus, grab mode, mouse delta,
//! lock-on, camera yaw and pitch, stick) and every simulation step as a `move` line (see
//! `play.rs`).

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseMotion};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// One segment of replayed input.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub frames: usize,
    /// A screenshot saved where the segment starts (a `shot` line, which has no frames).
    pub shot: Option<std::path::PathBuf>,
    pub keys: Vec<KeyCode>,
    pub buttons: MouseButtons,
    pub mouse: Vec2,
}

/// The mouse buttons a replay names: `Click` the left one, `MMB` the middle one.
const MOUSE: [(&str, MouseButton); 2] =
    [("Click", MouseButton::Left), ("MMB", MouseButton::Middle)];

/// Mouse buttons held in a segment, a bit per entry of [`MOUSE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseButtons(u8);

impl MouseButtons {
    pub fn iter(self) -> impl Iterator<Item = MouseButton> + Clone {
        MOUSE
            .iter()
            .enumerate()
            .filter(move |(i, _)| self.0 & (1 << i) != 0)
            .map(|(_, (_, b))| *b)
    }
}

/// One name in a segment's list.
enum Input {
    Key(KeyCode),
    Mouse(usize),
}

fn input(name: &str) -> Result<Input, String> {
    if let Some(i) = MOUSE.iter().position(|(n, _)| *n == name) {
        return Ok(Input::Mouse(i));
    }
    Ok(Input::Key(match name {
        "W" => KeyCode::KeyW,
        "A" => KeyCode::KeyA,
        "S" => KeyCode::KeyS,
        "D" => KeyCode::KeyD,
        "Q" => KeyCode::KeyQ,
        "C" => KeyCode::KeyC,
        "Z" => KeyCode::KeyZ,
        "X" => KeyCode::KeyX,
        "V" => KeyCode::KeyV,
        "Space" => KeyCode::Space,
        "Shift" => KeyCode::ShiftLeft,
        "E" => KeyCode::KeyE,
        "Esc" => KeyCode::Escape,
        "Enter" => KeyCode::Enter,
        "Up" => KeyCode::ArrowUp,
        "Down" => KeyCode::ArrowDown,
        "Left" => KeyCode::ArrowLeft,
        "Right" => KeyCode::ArrowRight,
        "F8" => KeyCode::F8,
        "F3" => KeyCode::F3,
        other => return Err(format!("unknown key {other:?}")),
    }))
}

pub fn parse(text: &str) -> Result<Vec<Segment>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let err = |e: String| format!("line {}: {e}", n + 1);
        if let Some(path) = line.strip_prefix("shot ") {
            out.push(Segment {
                frames: 0,
                shot: Some(path.trim().into()),
                keys: Vec::new(),
                buttons: MouseButtons::default(),
                mouse: Vec2::ZERO,
            });
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 2 && parts.len() != 4 {
            return Err(err("expected frames keys [dx dy]".into()));
        }
        let frames = parts[0].parse().map_err(|e| err(format!("{e}")))?;
        let mut keys = Vec::new();
        let mut buttons = MouseButtons::default();
        if parts[1] != "-" {
            for name in parts[1].split(',') {
                match input(name).map_err(err)? {
                    Input::Key(k) => keys.push(k),
                    Input::Mouse(i) => buttons.0 |= 1 << i,
                }
            }
        }
        let num = |s: &str| s.parse::<f32>().map_err(|e| err(format!("{e}")));
        let mouse = match parts.get(2..4) {
            Some([x, y]) => Vec2::new(num(x)?, num(y)?),
            _ => Vec2::ZERO,
        };
        out.push(Segment {
            frames,
            shot: None,
            keys,
            buttons,
            mouse,
        });
    }
    Ok(out)
}

/// The replay and how far it has got.
#[derive(Resource)]
pub struct InputReplay {
    segments: Vec<Segment>,
    /// Frames replayed so far.
    pub frame: usize,
    /// The segment whose keys and buttons are held now (none before the first and after the
    /// last).
    held: Option<usize>,
    /// Screenshots due and not yet taken: the window takes one a frame.
    shots: Vec<std::path::PathBuf>,
    /// Set by the game once the simulation runs; frames count from there.
    pub running: bool,
}

impl InputReplay {
    pub fn new(segments: Vec<Segment>) -> Self {
        Self {
            segments,
            frame: 0,
            held: None,
            shots: Vec::new(),
            running: false,
        }
    }

    pub fn total_frames(&self) -> usize {
        self.segments.iter().map(|s| s.frames).sum()
    }

    pub fn done(&self) -> bool {
        self.frame >= self.total_frames()
    }

    fn segment_at(&self, frame: usize) -> Option<usize> {
        let mut start = 0;
        for (i, s) in self.segments.iter().enumerate() {
            if frame < start + s.frames {
                return Some(i);
            }
            start += s.frames;
        }
        None
    }

    /// The screenshots due at `frame`.
    fn shots_at(&self, frame: usize) -> impl Iterator<Item = &std::path::PathBuf> {
        self.segments
            .iter()
            .scan(0, |start, s| {
                let at = *start;
                *start += s.frames;
                Some((at, s))
            })
            .filter(move |(at, _)| *at == frame)
            .filter_map(|(_, s)| s.shot.as_ref())
    }
}

/// Drops the real input of this frame and writes the replayed key and button changes and mouse
/// motion instead. Runs before Bevy's input systems, so `ButtonInput` and
/// `AccumulatedMouseMotion` see them this frame.
pub fn inject(
    mut commands: Commands,
    mut replay: ResMut<InputReplay>,
    window: Query<Entity, With<PrimaryWindow>>,
    mut keys: ResMut<Messages<KeyboardInput>>,
    mut buttons: ResMut<Messages<MouseButtonInput>>,
    mut motion: ResMut<Messages<MouseMotion>>,
) {
    keys.clear();
    buttons.clear();
    motion.clear();
    if !replay.running {
        return;
    }
    let Ok(window) = window.single() else {
        return;
    };
    let frame = replay.frame;
    let due: Vec<_> = replay.shots_at(frame).cloned().collect();
    replay.shots.extend(due);
    if !replay.shots.is_empty() {
        let path = replay.shots.remove(0);
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(path));
    }
    let now = replay.segment_at(frame);
    let segment = |i: Option<usize>| i.map(|i| &replay.segments[i]);
    let (was, want) = (segment(replay.held), segment(now));
    let none: &[KeyCode] = &[];
    let was_keys = was.map_or(none, |s| &s.keys).iter().copied();
    let want_keys = want.map_or(none, |s| &s.keys).iter().copied();
    held_diff(was_keys, want_keys, |key_code, state| {
        keys.write(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state,
            text: None,
            repeat: false,
            window,
        });
    });
    let held_buttons = |s: Option<&Segment>| s.map_or(MouseButtons::default(), |s| s.buttons);
    held_diff(
        held_buttons(was).iter(),
        held_buttons(want).iter(),
        |button, state| {
            buttons.write(MouseButtonInput {
                button,
                state,
                window,
            });
        },
    );
    let mouse = want.map_or(Vec2::ZERO, |s| s.mouse);
    replay.held = now;
    if mouse != Vec2::ZERO {
        motion.write(MouseMotion { delta: mouse });
    }
    replay.frame += 1;
}

/// Writes a release for each input held before and not now, then a press for each new one.
fn held_diff<T: Copy + PartialEq>(
    was: impl Iterator<Item = T> + Clone,
    now: impl Iterator<Item = T> + Clone,
    mut write: impl FnMut(T, ButtonState),
) {
    for i in was.clone().filter(|i| !now.clone().any(|n| n == *i)) {
        write(i, ButtonState::Released);
    }
    for i in now.filter(|i| !was.clone().any(|w| w == *i)) {
        write(i, ButtonState::Pressed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys_clicks_and_mouse() {
        let s = parse("# x\n10 W,D,Click,MMB 3 -1\n5 -\n").unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].keys, vec![KeyCode::KeyW, KeyCode::KeyD]);
        assert_eq!(
            s[0].buttons.iter().collect::<Vec<_>>(),
            [MouseButton::Left, MouseButton::Middle]
        );
        assert_eq!(s[1].buttons.iter().count(), 0);
        assert_eq!(s[0].mouse, Vec2::new(3.0, -1.0));
        assert_eq!(s[1].mouse, Vec2::ZERO);
        assert_eq!(parse("3 X").unwrap()[0].keys, vec![KeyCode::KeyX]);
        assert!(parse("3 Unknown").is_err());
        for script in [
            include_str!("../scripts/held_move_kill.txt"),
            include_str!("../scripts/input_camera_movement.txt"),
        ] {
            parse(script).unwrap();
        }
        let s = parse(
            "4 -
shot a.png
2 X
",
        )
        .unwrap();
        let replay = InputReplay::new(s);
        assert_eq!(replay.total_frames(), 6);
        assert_eq!(replay.shots_at(4).count(), 1);
        assert_eq!(replay.shots_at(3).count(), 0);
    }
}
