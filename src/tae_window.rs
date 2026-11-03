//! The TAE event window: which events of a clip one update dispatches, and with which flags.
//!
//! The exe: Havok's CustomManualSelectorGenerator (FUN_1411e56e0) calls the registered TAE
//! callback once per update of its selected clip with (anim id, the clip's local time before the
//! step (+0xe0, set in preUpdate FUN_1411e55e0), after it (+0xe4), the clip length (+0xe8));
//! FUN_1411e8f30 -> FUN_140b5c920 -> FUN_140b5c580 on the chr's module [[chr+0x1ff8]+0x10] looks
//! the anim up and runs FUN_140654b70 (or, in the multithreaded step, FUN_140b58420 now and
//! FUN_140b58530 when FUN_140b5c730 replays the module's queue of 10 after the step: the same
//! windows). Those turn (prev, cur, length) into one or two query windows (`windows`) and
//! FUN_1425e5640 walks the anim's event groups; every event that overlaps a window goes to the
//! handlers with an "entered" byte (record +0x18: the one-shot handlers act on it) and an "ended"
//! byte (+0x19). Checked against the exe in `sim_tests/emu_taewin.rs` (tools/emu-cases taewin).
//!
//! What follows for the sim: an event is dispatched on every update whose window it overlaps,
//! so also on the update its end falls in (with "ended"); an event shorter than the step (or of
//! zero length) is dispatched once with both flags; the window's edges sit 5e-7 s after the clip
//! times, so a start a rounding error past the accumulated time still counts this update; on a
//! clip's first update (prev 0) every event up to the time reached is "entered", events at 0
//! among them; an update that does not move the clip (hit stop) dispatches but enters nothing,
//! and one that stays at 0 dispatches nothing; a looping clip's wrap queries both ends.
//! State events (ChrActionFlags, TAE SpEffects, turn speed, attack windows, ...) follow the same
//! windows through `At::Step` (`Actor::step`), not a point test at t.
//!
//! The selector (checked in `sim_tests/emu_taestate.rs`, tools/emu-cases taestate): activate and
//! a change of selection run FUN_1411e6de0, which zeroes the clip's TAE times, so the new clip's
//! first update is (0, its time) and enters every event up to it; `Actor::play` restarts at prev_t
//! 0 the same way. gap: a clip started at a StartTime offset (`Actor::start_at`) enters nothing
//! before the offset (as if a preUpdate came between, see there). The outgoing clip of
//! a transition is not dispatched at all (FUN_1411ea620 skips its update), so it gets no last
//! window, as RSekiro dispatches only `Actor::anim`.

/// FUN_140654b70's edge offset: both window ends sit this far after the clip times.
pub const EDGE: f32 = 5e-7;

/// The query windows (a, b] of one clip update from local time `prev` to `cur` of a clip of
/// `len` s (FUN_140654b70): none when both are 0; a = -1 when prev is 0 (the clip's first
/// update), else prev + 5e-7 (prev if negative); b = cur + 5e-7 (cur if not positive). A wrap
/// (b < a) asks (a, b + len + 1) then (a - len - 1, b).
pub fn windows(prev: f32, cur: f32, len: f32) -> impl Iterator<Item = (f32, f32)> {
    let mut w = [(0.0, 0.0); 2];
    let n = if prev == 0.0 && cur == 0.0 {
        0
    } else {
        let a = if prev == 0.0 {
            -1.0
        } else if prev > 0.0 {
            prev + EDGE
        } else {
            prev
        };
        let b = if cur > 0.0 { cur + EDGE } else { cur };
        if b < a {
            w = [(a, b + len + 1.0), (a - len - 1.0, b)];
            2
        } else {
            w[0] = (a, b);
            1
        }
    };
    w.into_iter().take(n)
}

/// One dispatch of an event: the record's "entered" (+0x18) and "ended" (+0x19) bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub entered: bool,
    pub ended: bool,
}

/// FUN_1425e5640's test of an event [start, end] against a window (a, b]: dispatched when
/// start <= b and a < end; entered unless the window is empty (a == b) or the event began at or
/// before a (an event at 0 with a at 0 still enters); ended when b reaches the end.
pub fn hit(start: f32, end: f32, a: f32, b: f32) -> Option<Hit> {
    (start <= b && a < end).then(|| Hit { entered: !(a == b || (start <= a && (start != 0.0 || a != 0.0))), ended: !(a == b || b < end) })
}

/// Every dispatch of one clip update, in the exe's order (window by window, events in list
/// order): (event index, window, flags). A long event can come twice on a loop's wrap.
#[allow(dead_code)]
pub fn dispatch(events: &[(f32, f32)], prev: f32, cur: f32, len: f32) -> Vec<(usize, (f32, f32), Hit)> {
    let mut out = Vec::new();
    for (a, b) in windows(prev, cur, len) {
        for (i, &(s, e)) in events.iter().enumerate() {
            if let Some(h) = hit(s, e, a, b) {
                out.push((i, (a, b), h));
            }
        }
    }
    out
}

/// Where a state event is asked about: one update of the clip (`Actor::step`: the clip times
/// before and after it) or a bare time (a scan over a clip, an additive layer without a step).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum At {
    /// The update prev -> t: the events the exe dispatches in it (`dispatched`).
    Step(f32, f32),
    /// A time: start <= t < end.
    Point(f32),
}

impl From<f32> for At {
    fn from(t: f32) -> At {
        At::Point(t)
    }
}

impl At {
    /// Is the event [start, end] dispatched here? For a step: by the exe's windows (FUN_140654b70
    /// -> FUN_1425e5640, `hit`), so on every update it overlaps, the one its end falls in too
    /// (the handlers set their state on "ended" dispatches as on any other: FUN_140b51b70's
    /// ChrActionFlags OR their bits in whatever the record's +0x19), a start 5e-7 s past the
    /// accumulated time already, every event up to t on a clip's first update (prev 0). A step
    /// going back (a clock set back by hand; RSekiro's clips never wrap), (0, 0) and a point:
    /// start <= t < end. gap: the exe dispatches nothing on an update that stays at 0 (hit stop
    /// at a clip's start); RSekiro's (0, 0) is mostly a clip just played and not stepped yet,
    /// read before the step in the same tick (the exe's HKS change is followed by the clip's
    /// first window in the same frame), so it keeps the events at 0.
    pub fn dispatched(self, start: f32, end: f32) -> bool {
        match self {
            At::Step(prev, t) if t >= prev && t > 0.0 => windows(prev, t, 0.0).any(|(a, b)| hit(start, end, a, b).is_some()),
            At::Step(_, t) | At::Point(t) => start <= t && t < end,
        }
    }
}

/// Does an event starting at `start` enter in the update from `prev` to `cur` (the one-shot
/// rule, end >= start)? RSekiro restarts a looping clip through `Actor::play` (prev_t 0), so its
/// clock never wraps: an update going back enters nothing.
pub fn starts(start: f32, prev: f32, cur: f32) -> bool {
    cur >= prev && windows(prev, cur, 0.0).any(|(a, b)| hit(start, f32::INFINITY, a, b).is_some_and(|h| h.entered))
}
