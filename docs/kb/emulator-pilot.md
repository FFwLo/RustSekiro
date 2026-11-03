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
| 4 | Hit-stop mode (+0x2d of `SprjPlayerHitStopModule`, [[chr+0x1ff8]+0x90]) | snapshot read | Wolf and all 89 enemies of the scene: mode 0, WorldChrManDbg +0x11c/+0x11d off, +0x124 0.5. The constructor (FUN_140b9c8b0) zeroes it and no method of the module writes it, so Sv1's mode 0 table is the game's. Not done: a whole-exe search for another writer. |

## Snapshot (2026-11-03)

`emu-cases snapshot <pid> extracted/snapshot` waits until a save is loaded ([WorldChrMan 0x143d7a1e0]+0x88
non-null), waits 15 s, pauses the game, copies every committed readable region (`memory.bin`, about
10 GB, 6,057 regions) and the threads' TEBs (`regions.json`), and resumes it. `--snapshot <dir>` then
loads it: the exe image from the snapshot (globals as they were; import slots still stubs), other
memory a 64 KB window at a time on first touch, GS = the first thread's TEB, `TlsGetValue` read from
that TEB, and the CRT's use-FMA flag (DAT_143f336f4) cleared because Unicorn has no FMA (its SSE2
maths may differ by one ulp). Zero pages made up for stray reads are no longer executable, so a jump
through a null pointer stops the run. The snapshot is game memory: it stays in `extracted/` (ignored).

## What it costs and where it fits

- Small and medium functions with a clear object (pilots 1-2): one generator function and one sim
  test each; the emulator run takes under a second for hundreds of cases.
- Big functions on global state (pilot 3): hand-made fakes do not scale. The way to run them is a
  memory snapshot of the running game (one moment, no play or recording needed) loaded into the
  emulator, so they run on real state.
