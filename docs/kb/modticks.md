# Per-character pre-update and hit stop (topic modticks)

## Hit stop against the exe (2026-11-03)

`FUN_140a4b570(WorldChrMan, dt, chr)` writes the raw dt to chr+0x108, then the ChrIns pre-update
`FUN_1409f78a0` ticks SprjChrHitStopModule ([chr+0x1ff8]+0x90) with it, `FUN_140b9d1b0`, and
writes the module's result +0x18 back to chr+0x108: the character's clock for the rest of the
frame. Only the Havok world step and the fall take the raw dt.

The module (constructor FUN_140b9c8b0; start FUN_140b9dad0, rcx module, xmm1 seconds):
- +0x10 timer, +0x14 length, +0x18 scaled dt, +0x1c raw dt, +0x20 state, +0x2d mode (0 in the
  game, pilot 4), +0x31 forced (ChrActionFlag 41), +0x34 the time the last start computed.
- Start: +0x14 = seconds, +0x10 = the ease-in WorldChrManDbg +0x12c, +0x20 = 1. A start replaces
  a running hit stop.
- Tick: the timer counts down when > 0. State 1 eases in (dt x (timer / ease-in)^2), 2 holds
  (dt 0) for WorldChrManDbg +0x130, 3 eases out (dt x (1 - timer / length)^2), length = +0x14
  (or +0x134 = 0.04 with the debug switch +0x11d). A state whose time is 0 hands over at once
  (1 -> 2 with the timer = hold - dt); a running state that ends sets the next state's full time
  and returns this frame's own rule (the frame that ends the hold advances nothing).
- Snapshot WorldChrManDbg: +0x12c 0 (no ease-in), +0x130 0.06, +0x134 0.04, +0x11d 0, as
  FUN_140a51800 sets them.

So a hit stop of length L at 60 fps: frames 1-4 advance 0 (the hold 0.06 s, plus the frame that
ends it), then dt x (k dt / L)^2 for k = 1.. until the timer reaches 0; at 30 fps two frames hold.
For L = 0.1 at 60 fps the clock moves 0, 0, 0, 0, 0.00046, 0.00185, 0.00417, 0.00741, 0.01157,
0.016666666 (the timer is 3.7e-9, not yet 0), then whole frames.

**Run** (tools/emu-cases `modticks`, the 2026-11-03 Naomori-arena snapshot):
- `modticks_unit`: FUN_140b9dad0 then FUN_140b9d1b0 on Wolf's module, 7 lengths (0.03-0.5 s) and
  4 restarts (in the hold and the ease), dt 1/60 and 1/30: 22 scenarios, 273 frames, 10 s.
- `modticks_frames`: whole frames FUN_140a4b570 for Wolf and the General, hit stop 0.1 or none,
  dt 1/60 and 1/30: 8 scenarios, 54 frames, 35 s. FUN_140fff000 (the Havok behaviour character's
  step, set by the behaviour module FUN_140b45440 as clock x speed FUN_140b3b5a0 x +0xd00 x +0x960)
  hooked: on every frame the animation step equals chr+0x108, 0 in the hold, so the clip, TAE
  window and root motion all run on the hit-stop clock, Wolf and enemies alike.
- `modticks_defender`: the defender's start FUN_140b9cfb0 on Wolf (FUN_140b9d8c0 type 1, a
  player) and the General (type 0: never stopped as defender), 16 cases.

**Result.** RSekiro's `actor::HitStop` (hold then ease) gave the same clock except at the
epsilon it used (a timer under 1e-5 ended the hold or the ease a frame early; the exe runs the
frame with the timer at 3.7e-9 and returns the eased dt). Now a port of the module's states
(1 ease in, 2 hold, 3 ease out; the timer, length and constants as the exe), equal to the exe on
all 273 module frames within 1e-7 (state, timer, length, scaled dt) and on the 54 whole frames
(chr+0x108 and the behaviour step). Tests `hit_stop_module_matches_the_exe`,
`hit_stop_frames_match_the_exe`, `hit_stop_defender_start_matches_the_exe` (sim_tests/emu_modticks.rs).

**Found, not ported.** Both starts (FUN_140b9cd90 attacker, FUN_140b9cfb0 defender, called by
FUN_140b6e5a0 after the guard resolution) return without a hit stop when the hit's five amounts
(the damage struct's floats 0-4, which FUN_140b6e6a0 tests the same way) are all <= 0. Which
hits carry zeros (grabs, pushes, pure posture hits?) is not traced; Sv1 starts on every contact.

**The rest of the pre-update** (FUN_1409f78a0, read, not run apart): the frame counter +0x1fb0,
FUN_1409f7070 (action-flag module +0x5c from chr+0x1a10-0x1a16 bits), the throw module tick
FUN_140be1b50 (clears the frame's throw request, copies the throw id from the SpEffects: no timer),
module +0xc8 FUN_140b5e5f0 (lock/assist bookkeeping from module0 +0xc24 bits), the fall module
reset FUN_140b975f0 when grounded. None holds a timer that shapes the feel.

**Not covered.** The super armour and invincibility timers and the damage reaction module run in
ChrIns::vf22 (FUN_140a04850), not in this pre-update; not checked here. The ease-in (+0x12c) and
the debug switch are 0 in the game; the port keeps the ease-in rule but only the 0 case is run.
Which frame a hit's stop starts on (the damage module runs inside the defender's frame, so an
attacker updated earlier in the same frame starts its stop on the next frame) is not compared.
