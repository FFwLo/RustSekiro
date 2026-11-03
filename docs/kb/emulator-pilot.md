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
| 3 | FUN_14073c260 camera update (22,857 B), FUN_140734790 (3,119 B) | probe | Stop after 7 and 2 blocks: they need the game's global objects (singletons) set up; on empty state they fall into the assertion logger. |

## What it costs and where it fits

- Small and medium functions with a clear object (pilots 1-2): one generator function and one sim
  test each; the emulator run takes under a second for hundreds of cases.
- Big functions on global state (pilot 3): hand-made fakes do not scale. The way to run them is a
  memory snapshot of the running game (one moment, no play or recording needed) loaded into the
  emulator, so they run on real state.
