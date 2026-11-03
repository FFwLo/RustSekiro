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
| 4 | Hit-stop mode (+0x2d of `SprjPlayerHitStopModule`, [[chr+0x1ff8]+0x90]) | snapshot read | Wolf and all 89 enemies of the scene: mode 0, WorldChrManDbg +0x11c/+0x11d off, +0x124 0.5. The constructor (FUN_140b9c8b0) zeroes it and no method of the module writes it, so Sv1's mode 0 table is the game's. Not done: a whole-exe search for another writer. |
| camwall | The follow camera at walls and the ground (FUN_140734790 -> FUN_14073c260, its sphere casts hooked) | 10 scenarios, 2,345 frames, 232 s | The update's four casts (fulcrum, orbit point, obstacle fade, final) recorded with their results; RSekiro's collision block fed the same results matches on 2,225 frames within 1e-4: the fulcrum and orbit point are cut by their own casts, the distance chase starts from the cut camera +0x110 and runs at the _ForHitWall rate for 2 s after a hit (before: only the final cast, off by up to 1.6 m). Test `follow_camera_at_walls_matches_the_exe`; kb/camera.md. Not covered: escape-wall option (off in the user's save), the focus pull-in, the obstacle fade (map objects only; no enemy passes its character test), 30 fps chase. |
| 5 | FUN_140b36410 HKS env dispatcher on Wolf (`hks_env`) | 330 cases (82 ids x the scripts' arguments) + 30 field pokes on the snapshot, under 1 s | Every case ran to the end. Wolf's module bag named by RTTI, the string ids resolved through the exe's name maps (env 3064, act 3041), each documented field moved the answer when poked. Test `hks_env_matches_the_exe`: `src/hks_env.rs` answers 22 ids (1,045 of 1,605 env calls, 65.1 %), all 239 compared cases equal the exe for idle Wolf. Fixed: GetVerticalSpeed on the ground (0 -> -163, one frame of gravity, fitted on one sample); Sv1's SpEffect set gains the equipped weapons' residentSpEffectId. Not covered: 60 ids (gaps, kb/behaviour-graph.md), poses other than idle, the act dispatcher (mapped, not run). |

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
