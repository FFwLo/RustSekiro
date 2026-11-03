# The TAE event window

Which TimeAct events of a clip one update dispatches, and with which flags. Port:
`src/tae_window.rs`; test `tae_window_matches_the_exe` (`src/sim_tests/emu_taewin.rs`); cases from
`tools/emu-cases` `taewin` (`extracted/emu/taewin_cases.json`).

## 2026-11-03: checked against the exe

**The path.** FUN_140b5c730 (named in frame-coverage.md as the dispatcher) is not the window
rule: it replays the chr module's queue of TAE windows after the multithreaded Havok step. The
chain is:

- Havok's CustomManualSelectorGenerator FUN_1411e56e0 keeps the selected clip's anim id (+0xec),
  its local time before the step (+0xe0, set in preUpdate FUN_1411e55e0), after it (+0xe4) and
  its length (+0xe8), and per update calls FUN_1411e8f30 (profiler "TtTAE") -> the callback
  registered at [0x143d928a8] = FUN_140b5c920 (anim id, prev, cur, length, chr).
- FUN_140b5c920 -> FUN_140b5c580 on the chr's module [[chr+0x1ff8]+0x10]: looks the anim up
  (FUN_14098ed70 on module +0x10), writes the id to +0xf8, then with the multithreaded-step flag
  ([0x143d92828]+0x58, FUN_141043150) clear runs FUN_140654b70 (handlers through callback
  0x140654a8c); with it set, FUN_140b58420 (callback 0x140b4fd9c, the in-step handlers) and a
  record (id, prev, cur, length, frame) in a ring of 10 at +0x20 (stride 0x14, head +0xe8, tail
  +0xec) that FUN_140b5c730 replays after the step through FUN_140b58530 (callback 0x140b4fda4).
- FUN_140654b70 / FUN_140b58420 / FUN_140b58530 are the same window maths; FUN_1425e5640 walks
  the anim's event groups (+0x08 groups of 0x20: u16 count, +0x08 u32 offsets, +0x18 base; +0x24
  group count) and hands every overlapping event to the callback in a record: +0x00 type, +0x10 /
  +0x14 the window, +0x18 entered, +0x19 ended, +0x20 the event.

**The rule** (`tae_window::windows`, `hit`):

- No dispatch when prev and cur are both 0. Window (a, b]: a = -1 when prev is 0 (the clip's
  first update), else prev + 5e-7 (prev if negative); b = cur + 5e-7 (cur if not positive).
- A wrap (b < a) asks (a, b + len + 1), then (a - len - 1, b): both ends of the loop in one
  update; an event spanning the wrap comes twice.
- An event [s, e] is dispatched when s <= b and a < e. Entered (+0x18) unless a == b or s <= a
  (an event at 0 with a at 0 enters); ended (+0x19) unless a == b or b < e.

So: every update a state event overlaps dispatches it, the update its end falls in too (with
"ended"); an event shorter than the step, or of zero length, is dispatched once with both flags;
the 5e-7 edge lets a start a rounding error past the accumulated f32 time count now; the first
update of a clip enters every event up to the time reached; a hit-stop update (prev == cur > 0)
dispatches the overlapping events but enters none, one that stays at 0 dispatches nothing. The
handlers (FUN_140b51b70 and its siblings) gate their one-shots on +0x18 and read +0x14 for
progress.

**Run** (Wolf's module on the 2026-11-03 Naomori-arena snapshot, his current anim's entry pointed
at a synthetic list of 13 events in two groups; callbacks replaced by `ret` and recorded): 30 /
60 / 20 / 144 fps runs, a phase off the grid, speed x1 -> x0.5 -> x3 -> x1, hit stop (dt 0 at the
start and mid-event), a one-shot clip held at its end, nine first updates of a new clip, 148
windows on and 1e-7 .. 6e-7 s around every edge, loop wraps, 450 random windows; plus Wolf's own
a000_790010 (his anim in the snapshot), a050_300000, a110_316200, a073_401000 at 30 fps looping
(339 events). The multithreaded path (in-step + queue replay through FUN_140b5c730) was run on
the looping 30 fps and hit-stop runs, the first updates and the wraps: the same dispatches.

**Matched:** all 2,146 updates (1,029 synthetic, 1,117 on Wolf's anims), 15,069 dispatches:
event, window bits, entered and ended equal to `tae_window::dispatch`; `starts` equals "entered"
on every update that does not wrap.

**Fixed** (before -> after, on the same updates): RSekiro's one-shot rule `(start > prev_t ||
(prev_t == 0 && start == 0)) && start <= t` differed from "entered" on 127 event-updates:
- 68 on Wolf's own anims at 30 fps (34 events): the f32 clock lands a hair under a 30 fps grid
  time (0.8999998 for 0.9), so RSekiro entered the event one update (33 ms) late; the exe's 5e-7
  edge enters it on time. Same at 60 fps (11), in the speed / once runs (6) and on the edges (18).
- 24 hit-stop updates held at 0 (prev_t == t == 0): RSekiro fired every event at 0 again on each
  held update; the exe dispatches nothing until the clip moves.
Now one rule, `tae_window::starts`, in: player.rs `one_shot_starts` (TAE 401 / 67 recoveries,
SpEffects) and the 32 weapon style / 65 gourd consume checks; data.rs `velocity_change` (920),
`velocity_scale` (922), `perilous_warning`; model.rs draw mask 233; enemy/events.rs messages;
enemy_bullet.rs and prosthetic.rs bullets; gore.rs 138; camera.rs 144-147 shakes; combat.rs
attack AI sounds and 304 lands (these also missed an event at 0 on a clip's first update);
enemy.rs deathblow hit, `instant_hp_rate`, the hearing check; sound.rs `sound_windows` (TAE
sounds and, through it, ffx.rs effects) now asks the exe's windows, including both ends of a
locomotion loop's wrap, and the 0.05 s cap on "an event at 0 plays on the first step" is gone.

**Not covered / gaps:**
- State events (`Event::in_time`, `CharData::events_at` and everything built on it: ChrActionFlag,
  SpEffects at t, turn speed, ...) are a point test start <= t < end. The exe dispatches an event
  on every update it overlaps, so also on the update its end falls in: 1,552 event-updates here
  where the exe still dispatches (with "ended") and RSekiro does not; a state event lasts one
  update longer in the game. Fixing it means passing prev_t into every `events_at` caller (about
  30 sites); not done. Short events matter little: only one event in combat_data is shorter than
  1/60 s (a zero-length 128), the TAE grid is 1/30 s.
- Which handlers act on "ended" (+0x19) or on non-entered dispatches was not traced per type
  (FUN_140b51b70 gates most one-shots on +0x18; one case reads +0x19).
- The selector level (FUN_1411e56e0) was read, not run: on an animation change FUN_1411e6de0
  resets prev and cur to 0 and only the newly selected clip is dispatched; the clip blended out
  gets no last window, and a clip started at a StartTime offset enters every event before it
  (RSekiro's `continue_state` sets prev_t = t, so it enters none). Whether the new clip already
  advances in the update it is selected was not checked.
- The queue of 10 (FUN_140b5c580 / FUN_140b5c730) overflows silently past 10 windows a frame;
  its +0xf0 / frame-stamp reuse was not followed.
