# Emulator comparison pilot (2026-11-03)

`tools/emu-cases` runs functions of the user's own `sekiro.exe` in Unicorn with chosen inputs and
writes the inputs and results to `extracted/emu/<name>.json`; sim tests run Sv1's port on the same
inputs. No game bytes leave the PC; the cases are numbers the functions computed.

## Setup on this PC

- Unicorn crashes on any run under Control Flow Guard (its `longjmp` out of generated code, in
  VCRUNTIME140). The owner turned CFG off for `emu-cases.exe` only
  (`Set-ProcessMitigation -Name emu-cases.exe -Disable CFG`; undo with `-Reset`).
- The tool links the prebuilt `unicorn.dll` of the Python `unicorn` package (build.rs); copy it next
  to `target/release/emu-cases.exe`. The `unicorn-engine` crate needs libclang, which is not here.
- Every import slot becomes a named stub (the file's import table holds name offsets until Windows
  fills it).

Run: `tools/emu-cases/target/release/emu-cases.exe <image/sekiro.exe> extracted/emu [name ...]`;
`probe <addr>` runs a function on zeroed objects and lists what it reads and where it stops.

## Results

| Pilot | Function | Cases | Result |
|---|---|---|---|
| 1 | FUN_140bdb470 super armour hit (488 B) | 1,152 in 0.6 s, 23 blocks | Normal path identical. Fixed: the pool is cut to its full size before the hit (a bonus that ended left extra posture), and a broken pool still drains and restarts its timer. Test `super_armor_hit_matches_the_exe` passes. |
| 2 | FUN_140b9cd90 attacker hit stop (544 B) | 160 in 0.4 s, 29 blocks | Sv1's table is exact for component mode 0/3 with the engine switch off. Found: mode 1 makes type-0 defenders give full hit stop, mode 2 none at all, the switch makes type 0 full, and the type-3 action state is the chr's +0x1ff8 module [0]+0x54 bit 3. Which mode Wolf and enemies carry is not traced. |
| 3 | FUN_140734790 follow camera frame (3,119 B, calls FUN_14073c260 22,857 B) | 2 frames on the snapshot, 6.5 s | On empty state it stopped after 7 blocks (needs the game's singletons). On the 2026-11-03 snapshot it runs to the end: 48 blocks, only locks, TLS and error-code imports hit. Standing still with no input only its timers move (+0x414 = dt, +0x410 counts down); input and motion cases are the next step. |
| 3b | The follow camera over many frames (`camera_frames`, same functions) | 17 scenarios, 1,650 frames, 60 s | From the exe's own state before each frame, test `follow_camera_matches_the_exe` checks Sv1's helpers on 1,626 frames: stick rates and the fast turn ramp, mouse, yaw and pitch read back from the dragged fulcrum, the fulcrum's placement, the orbit point, the position chase, the safe margin, and the control timers (+0x150/+0x154/+0x194, checked once in Python). Fixed: the focus follow is FUN_141155d20, a velocity chase stepped at 1/30 s (`FocusChase`; Sv1 closed a share of the gap per frame, half as fast at the start of a run, with no spring lag); the fulcrum's sideways move is measured on last frame's camera right axis; in the fast turn yaw turns at the full rate with the stick's side only, and the pitch goes to the stick's tilt (`direct_pitch`, CameraParam directPitchAng / directPitchRate, fitted within 3e-6 rad; a full sideways stick levels the camera). Not covered: the wall and ground handling (24 frames where the camera reached the ground, and a wall about 10 m along +X), lock-on, TAE camera events, other frame rates than 60 / 30, which pad axis the exe's stick (pitch, yaw) comes from. |
| 3c | Lock-on camera (`lockcam`: FUN_1409c5f20 hands the target over, then FUN_140734790 -> FUN_14073c260) and the pad (`lockcam_probe`: FUN_1407f0f50, FUN_1409668f0 / FUN_140966960) | 16 scenarios, 1,545 frames (1,485 locked), 190 s; 12 pad frames, 4 stick getter calls | Test `lock_camera_matches_the_exe` checks every locked frame: the lock point (+0x280), pitch / yaw (+0x170 / +0x174 within 1e-5 rad), the fulcrum, orbit point, position chase at the lock rates and the safe margin, and the free yaw after release. Fixed: the lock point is averaged over 0.5 s in the target's frame (`LockPointAverage`), the framing uses the fulcrum's distance (~3.8 m, was camera distance 4.5: steady pitch 0.446 -> 0.411 rad on a target 5 m ahead), yaw chases focus -> P (was Wolf -> target root), no overhead hold (`lock_chase`, `wrap_pi`). The position chase's angle cosine is rounded as the exe's (it stops on nearly parallel vectors whose f32 cosine rounds above 1; was up to 0.4 mm off on 81 frames). Pad: move up / right, mouse, camera yaw = stick x, pitch = -stick y (options 10 / 11 set; cleared they flip), lock switch (x, -y): Sv1 agrees (`pad_mapping_matches_the_exe`). Not covered: walls while locked, TAE camera events while locked, target rotation, the key config's axis binding; the exe's chase has no time factor (at 30 fps Sv1 closes 0.64 a frame, the exe 0.4). |
| 4 | Hit-stop mode (+0x2d of `SprjPlayerHitStopModule`, [[chr+0x1ff8]+0x90]) | snapshot read | Wolf and all 89 enemies of the scene: mode 0, WorldChrManDbg +0x11c/+0x11d off, +0x124 0.5. The constructor (FUN_140b9c8b0) zeroes it and no method of the module writes it, so Sv1's mode 0 table is the game's. Not done: a whole-exe search for another writer. |
| camwall | The follow camera at walls and the ground (FUN_140734790 -> FUN_14073c260, its sphere casts hooked) | 10 scenarios, 2,345 frames, 232 s | The update's four casts (fulcrum, orbit point, obstacle fade, final) recorded with their results; RSekiro's collision block fed the same results matches on 2,225 frames within 1e-4: the fulcrum and orbit point are cut by their own casts, the distance chase starts from the cut camera +0x110 and runs at the _ForHitWall rate for 2 s after a hit (before: only the final cast, off by up to 1.6 m). Test `follow_camera_at_walls_matches_the_exe`; kb/camera.md. Not covered: escape-wall option (off in the user's save), the focus pull-in, the obstacle fade (map objects only; no enemy passes its character test), 30 fps chase. |
| 5 | FUN_140b36410 HKS env dispatcher on Wolf (`hks_env`) | 330 cases (82 ids x the scripts' arguments) + 30 field pokes on the snapshot, under 1 s | Every case ran to the end. Wolf's module bag named by RTTI, the string ids resolved through the exe's name maps (env 3064, act 3041), each documented field moved the answer when poked. Test `hks_env_matches_the_exe`: `src/hks_env.rs` answers 22 ids (1,045 of 1,605 env calls, 65.1 %), all 239 compared cases equal the exe for idle Wolf. Fixed: GetVerticalSpeed on the ground (0 -> -163, one frame of gravity, fitted on one sample); Sv1's SpEffect set gains the equipped weapons' residentSpEffectId. Not covered: 60 ids (gaps, kb/behaviour-graph.md), poses other than idle, the act dispatcher (mapped, not run). |
| guard | Guard and deflect resolution (`guard`): FUN_140b6c880 defender, FUN_140b6e6a0 apply, FUN_140b698d0 attacker, FUN_140842fb0 flick defence | 90 cases, about 7 min | Deflect = guarded and flick defence >= the attack's flick (105010 / 200220 lift 20 -> 40 over a slash's 30; guardAtkRate 100 and Wolf's 90 deathblow starts are blocked in the window); the guard row's guardAttribute picks the disable pair (the umbrella blocks thrusts); arc from guardAngle (Wolf 90, the General 60), ShieldBlock ArgB is the judge. Fixed: deflect needed only a stateInfo 158 SpEffect (the umbrella hold's 107650 deflected 2.9 s), now the flick contest with the guard flag; attribute 1 disables; arcs; no attacker bound on a guard break. Test `guard_resolution_matches_the_exe`; kb/guard-deflect.md. Not covered: parts guard, umbrella arc (not equipped in the snapshot). |
| 6 | HP and posture amounts (`damage`): FUN_140a04850 posture regen with the FUN_140bd4e20 setter, FUN_140840eb0 element sum, FUN_140840ce0 attack / defence curve, FUN_140844070 / FUN_1408439a0 hit posture, vfuncs 0x218 / 0x228 / 0x248 | 4,410 regen frames (6 characters, 21 scenarios), 396 sums, 140 curve points, 90 defences, 3,120 posture cases; about 9 min | Regen fixed: points/s cut to an int (247.95 -> 247), the StaminaControlParam type limits (break anims hold enemy posture at 0, then 30 %; Wolf's type 5 full), carry runs at full posture. HP whole points with a minimum of 1; defence 0 everywhere, curve identity. Unguarded posture matched as it was. Tests `posture_regen_matches_the_exe`, `hit_hp_matches_the_exe`, `hit_posture_matches_the_exe`. Not covered: Wolf's guarded posture (x0.99 with an empty weapon block, no Flowing Water), the sum's flag factors. Unicorn crashes after about a thousand calls on one engine: a fresh one per scenario (kb/damage.md 2026-11-03). |
| 7 | FUN_140a37560 PlayerIns update: pad manipulator FUN_1407f0f50, MoveSpeedLevel FUN_140b4afd0, BehaviorTurn FUN_1407da6d0 / FUN_1407dac80, turn delay FUN_1407e7da0 (`turn`) | 14 scenarios, 440 frames on the snapshot | Test `wolf_turn_matches_the_exe` matches all 440 frames to 1e-4 (MoveSpeedLevel, move vector, yaw request, physics yaw, drawn yaw and its speed). Fixed: free turns from the stick snap whole a frame after the request when no TAE 224 rate is set (Sv1 turned at 720 deg/s the same frame); a 45 deg stick gate (press from rest one frame late, a flick over 45 deg in one frame reads as a let-go); the drawn facing (PlayerCtrl GetRotation = the delay controller) chases at 1.1x the rate, 792 deg/s (`player::Turn`, `actor::ShownYaw`). Confirmed: MoveSpeedLevel = stick length, no dead zone or rescale; locked turning at 720 deg/s (live). Not covered: lock-on manipulator (no target), camera-yaw hold option, walk halving flag, the pad's processing before its floats (kb/movement.md 2026-11-03). |
| taecam | TAE camera events 151 / 153 / 155 / 156 on the camera frame (`taecam`, FUN_140734790 -> FUN_14073c260, singleton DAT_143d5c0d0, CameraSetParam manager DAT_143d59938) | 17 scenarios, 2,160 frames, 250 s | Test `tae_camera_events_match_the_exe` passes (distance, slot blend of all 95 CameraParam fields, FOV, focus height, angle event, look limits and their clamp, focus on the dummy and the after-rate). Fixed: 153 per-frame rates (Sv1 dt * 30), no 0.1 smoothing, EndInterpolationSpeed only for TransitionKeyframe s (Sv1 up to 2.0 m off); 155 eased weight, blend from the blended row at the switch, LockCamParam switched at once, focus height formula; 156 target from Wolf's yaw at the first frame (0.98 rad off when he turns); 151 centre from the dummy's view, limits chase from where they were, exe yaw clamp, after-rate, cast gate. Not covered: the blocked cast, non-full-circle CameraParam rows, SlowEnd, 30 fps. |
| speffect | SpEffect add FUN_1409e9c00 and the chr tick vf44 FUN_140a02a10 (list tick FUN_140a03340 -> FUN_140c005b0 / FUN_140c06b70, HP FUN_140b658b0 -> FUN_140c014f0) on Wolf and the General | 54 scenarios, 32,049 frames at 30 / 60 fps, 204 s | Test `speffect_ticks_match_the_exe` matches 8,730 frames of 16 runs (gauges after each add, HP, the list with time and expired flag), `gourd_heal_matches_the_exe`. Fixed: HP per tick summed then one int (burn on Wolf 9.6 -> 9, poison 20.4 -> 20, the General 111.05 -> 111); interval 0 fires every tick; an entry lives the tick it runs out and its replace comes the next, already ticked (109451 30 -> 31 frames); instant rows fire on their add tick; category >= 10000 refuses a second burn / poison (no build-up, no refresh); build-up x worn def*ResistDamageRate (Hard mode 31 -> 46); gourd heal whole points. Not covered: FUN_140c02000's stateInfo / category matches, the replace's build-up (4011 -> 4012), Wolf's worn rows in `hit_wolf`, regen items and buffs, the buff-duration count, the General's burn (kb/speffect.md). The tick uses `call_quiet` (no per-call block hook): Unicorn segfaulted after about 650 vf44 calls with it. |

## Snapshot (2026-11-03)

`emu-cases snapshot <pid> extracted/snapshot` waits until a save is loaded ([WorldChrMan 0x143d7a1e0]+0x88
non-null), waits 15 s, pauses the game, copies every committed readable region (`memory.bin`, about
10 GB, 6,057 regions) and the threads' TEBs (`regions.json`), and resumes it. `--snapshot <dir>` then
loads it: the exe image from the snapshot (globals as they were; import slots still stubs), other
memory a 64 KB window at a time on first touch, GS = the first thread's TEB, `TlsGetValue` read from
that TEB, and the CRT's use-FMA flag (DAT_143f336f4) cleared because Unicorn has no FMA (its SSE2
maths may differ by one ulp). Zero pages made up for stray reads are no longer executable, so a jump
through a null pointer stops the run. The snapshot is game memory: it stays in `extracted/` (ignored).

## Scenes: snapshots without the owner (2026-11-03)

`emu-cases scene <dir> <idol> [x y z yaw] [ai-off]` finds sekiro.exe, turns the debug flag player
no death on (0x143d7a366 +0), and warps with the game's own idol travel FUN_14068e870(0, idol, 0)
run on a thread of its own (a 30-byte stub). For a spot, the warp's two arrival setters
(FUN_140832a30 position, FUN_140832a10 angle, into [0x143d5c0d0]+0xac0/+0xad0) are rewritten for
the load to store the spot, then restored, as SekiroTool does: writing the field from outside
loses the race (Wolf came out at the idol). It sets disable AI (+0x10) if asked, waits 8 s, checks
Wolf is at the spot (else no snapshot) and copies. Tested 2026-11-03 in General Naomori's arena
(1101951, 201.79 -26.23 23.88): Wolf exactly there, setters back to their own bytes, all 89
enemies hit-stop mode 0. The copy paused the game 35-170 s (disk speed).

Snapshots are working files (owner, 2026-11-03): use them, then delete them (10 GB each). The
Naomori-arena snapshot was deleted after the camera runs; `extracted/emu/camera_frames.json` (numbers
the functions computed) stays, so the test runs without it. New camera scenarios need a new scene
(`emu-cases scene`, about 3 minutes with the game running). Idol ids and boss spots: SekiroTool's `Resources.resx` "Warps" (MIT,
reference/SekiroTool). The same steps for live work without the tool: `tools/memscope/warp.lua`.
The warp's area load moved the game window off the screen once (to 3334,1433); moving it back with
SetWindowPos is enough. A save to load from the title screen still needs screen control (held keys,
about 0.2 s; menu clicks only highlight, a key confirms).

## What it costs and where it fits

- Small and medium functions with a clear object (pilots 1-2): one generator function and one sim
  test each; the emulator run takes under a second for hundreds of cases.
- Big functions on global state (pilot 3): hand-made fakes do not scale. The way to run them is a
  memory snapshot of the running game (one moment, no play or recording needed) loaded into the
  emulator, so they run on real state.
