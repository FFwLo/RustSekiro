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

**Found** (ported 2026-11-03, below). Both starts (FUN_140b9cd90 attacker, FUN_140b9cfb0 defender, called by
FUN_140b6e5a0 after the guard resolution) return without a hit stop when the hit's five amounts
(the damage struct's floats 0-4, which FUN_140b6e6a0 tests the same way) are all <= 0. Which
hits carry zeros (grabs, pushes, pure posture hits?) is not traced; Sv1 starts on every contact.

**The rest of the pre-update** (FUN_1409f78a0, read, not run apart): the frame counter +0x1fb0,
FUN_1409f7070 (action-flag module +0x5c from chr+0x1a10-0x1a16 bits), the throw module tick
FUN_140be1b50 (clears the frame's throw request, copies the throw id from the SpEffects: no timer),
module +0xc8 FUN_140b5e5f0 (lock/assist bookkeeping from module0 +0xc24 bits), the fall module
reset FUN_140b975f0 when grounded. None holds a timer that shapes the feel.

**Not covered.** The super armour timer and the damage reaction module: run 2026-11-03 below
(they tick in ChrIns vf25's FUN_140a05060, not vf22). The ease-in (+0x12c) and
the debug switch are 0 in the game; the port keeps the ease-in rule but only the 0 case is run.
Which frame a hit's stop starts on (the damage module runs inside the defender's frame, so an
attacker updated earlier in the same frame starts its stop on the next frame) is not compared.

## Super armour, the hit's zero amounts, frame order, umbrella arc, parts guard (2026-11-03, topic damagemod)

**Where the modules tick.** ChrIns::vf22 FUN_140a04850 is not where the damage side ticks: it runs
the event (+0x58), throw (+0x88) and behaviour (+0x28) modules, posture regen (+0x18), the
resist module (+0x20, build-up decay), physics (+0x68), docking, wire and FdpDataEx. The super
armour, toughness and damage modules tick in FUN_140a05060, which ChrIns vf25 (PlayerIns
FUN_140a38c10, EnemyIns FUN_140a16700) calls after the Havok step: throw FUN_140be1b10, the damage
module FUN_140b6a940 (+0x98, SprjEnemyDamageModule on the General: its hit-effect handle list, and
the per-frame set of attacks that hit cleared, +0xc8 = 0), the bullet emitter slot chr+0x10e0
(ChrBulletEmitterSlot) FUN_1407d5c90, the ChrCtrl +0x90 knockback FUN_140be6750,
SprjChrSuperArmorModule FUN_140bdbcb0 (+0x40), CSChrToughnessModule FUN_140be51e0 (+0x48), the
SpEffect tick vf44 (vtable +0x160), the data module FUN_140bd6f90 and the wet module. All take the
frame's FD4Time, the hit-stop clock chr+0x108. No separate invincibility timer is ticked there;
the toughness gauge is 0 on the General (NpcParam toughness 0 on every row) and stays 0.

**Super armour tick** (FUN_140bdbcb0 -> FUN_140bdb690, pool = module +0x18: +0x10 current, +0x14
the full size last tick, +0x18 reserve, +0x1c timer, +0x20 broken, +0x21 broken as of the last
tick, +0x24 bonus). Run (`damagemod_sa`): the General with vfunc 0x2c8 answering 200 (his row has
none), delay vfunc 0x2d8 = 7 s, 9 scenarios, 1,760 ticks at 1/60 and 1/30. The drain carried his
worn saReceiveDamageRate 0.6 (FUN_140bfdd20). Found:
- a break (pool emptied) refills on the second tick after it: the tick reads +0x21, which the
  first tick only sets (Sv1 refilled on the first);
- a running timer (>= 0) counts down and the pool snaps to full on the tick it reaches 0, the
  timer left at about -dt (7 s at 60 fps: the 420th tick, timer -3.7e-5); Sv1 reset it to -0.01;
- an idle timer in (-1, 0) is re-armed to the delay when the full size changed since the last
  tick with no bonus on (a stateInfo-155 bonus that ended): the pool comes back to the smaller
  full size after the delay (Sv1 never touched it);
- the reserve +0x18 is zeroed each tick while (chr+0x50)+0x50 +0x510 < 0 (-1 on the General).

Whole frames (`damagemod_frames`, FUN_140a4b570 on the General, AI off, 4 scenarios x 24 frames:
a 60 drain or a break, hit stop 0.1 or none, 1/60 and 1/30): the timer stands still while the
hit stop holds the clock (7.0 through the hold, then 6.999537, 6.997685, ... as chr+0x108), and
the break's refill still comes on the second frame with the clock at 0.

Fixed: `actor::SuperArmor::tick` is FUN_140bdb690 (fields `was_broken` +0x21, `last_full` +0x14),
`refill` (FUN_140bdb660) no longer touches the timer. Test `super_armour_ticks_match_the_exe`
(sim_tests/emu_damagemod.rs): every tick and hit, and the whole frames with `actor::HitStop` as
the clock.

**The hit's five amounts** (hit record floats 0-4, FUN_14083ddc0 -> the owner's vfunc 0x1d8; an
enemy's is FUN_14083dea0: AtkParam atkPhys / atkMag / atkFire / atkThun / atkDark x the attack
info's rates +0x70, +0xa4-0xb0, plus its flat adds +0x2c, +0x60-0x6c; a player's FUN_140a23f40 ->
FUN_14083e060: the row x the weapon's attack power). Run (`damagemod_amounts`, 154 AtkParam_Npc
rows with the rates 1): each amount is the row's element value. So both hit-stop starts skip
exactly the rows with no element damage (pushes, pure posture hits). Ported:
`Attack::amounts_zero` (NPC: the five atk*; Wolf: the five atk*Correction as well),
`combat::start_hit_stop` returns on it. Test `zero_amount_hits_match_the_exe`. gap: the player
side was not run (FUN_140a23f40 gave 0 on a hand-made attack info: the weapon attack power comes
from fields the attack registration fills); the flat adds are taken as 0.

**Frame order.** The hit manager runs in MoveMapStep's task FUN_140a746f0 (task group 0x4b:
FUN_1409a0dc0 on DAT_143d77ef0 -> FUN_1409956d0 -> FUN_140b06360 -> the defender's damage module
vfunc 0x48 = FUN_140b729e0 -> FUN_140b6a040 -> FUN_140b68ff0 -> FUN_140b6f690 -> FUN_140b6e5a0,
both starts), before the characters' frames FUN_140a70020 (group 0x52) -> FUN_140a4b570. The
shapes it tests are where the last frame's step left them, and the started stop holds both
sides' next step (the modticks frames: a stop started before a frame gives that frame 0). Sv1's
order (Advance, then Resolve starts the stop, the next Advance takes the held clock) is the same.
Read, not run as one task chain. (Remote hits come through PlayerIns vf22 FUN_140b72a80 and the
world pre-update FUN_140a4f940: online only.)

**Umbrella arc** (`damagemod_guard`: Wolf's vfunc 0x338 answering EquipParamWeapon 76000 in the
emulator's copy, judge 100 -> guard row 107600100, the General's slash at 0-180 deg; 21 cases):
vfunc 0x258 gives 180 and the attack is guarded at every angle up to 179 deg but not from
straight behind (180): the test is facing . attack dir < cos(angle + 180), strict (cos 360 = 1).
With the katana (0) the front half, strict at 90. Fixed: `combat::in_guard_arc` is that test
(Sv1 guarded all round with the umbrella, and at exactly the edge). Test
`umbrella_arc_and_parts_guard_match_the_exe`.

**Parts guard** (FUN_140b6c880: the hit's body part +0x160 -> FUN_1407dceb0(ChrCtrl) -> guard part
1-8 -> NpcParam partsAtkParamId<n> (+0x1e4, FUN_1410c66a0); guarded by the part when that row's
guardBreakRate (+0x64) is above the attack's guardAtkRate (+0x62): +0x1ce = 1 and the part row is
the guard row). Run on the General for body parts -1..30, his parts rows as they are (-1) and set
to 10200900 in the copy (33 cases): FUN_1407dceb0 maps every part to 0, +0x1ce never set. In the
data no NpcParam row names a parts row but 95211000 / 95231000 (AtkParam_Npc 0, guardBreakRate
0), so the parts guard never fires in Sekiro: nothing to port; the test checks the data stays so.

**Not covered.** The ChrCtrl +0x90 adjuster FUN_140be6750 (not the knockback after all: kb/damage.md
"Guard-break knockback", 2026-11-04), the damage module's hit-effect list, the stateInfo-155 bonus and the
stateInfo-331 instant refill in the emulator (no SpEffect added; Sv1 follows the code), Wolf's
amounts, the hit task run as a whole.
