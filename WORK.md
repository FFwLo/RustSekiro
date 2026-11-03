# Work board

**Goal: RSekiro plays and feels exactly like Sekiro.** Every rule comes from the game's own files or from what
the game's code does, and is checked against your own copy of the game where it can be.

How to use this board is in `AGENTS.md` (pick, claim, finish). The copy on the `agents-work-board` branch is the
only one that counts. Finished rows move to "Done" at the bottom; board history is in `CHANGES.md`.

- **Kinds**: `E` checks a system against the exe in the emulator (`tools/emu-cases`,
  `docs/kb/emulator-agent-brief.md`, `docs/kb/emulator-pilot.md`). `P` ports something RSekiro does not do yet
  (the "Not covered" lists in `docs/kb/`). `B` is bugs and cleanup: good first rows.
- **Order**: inside a kind, the top row first. Rows near the top change the feel the most.
- **Area**: `any` = anyone with the setup. `emu` = needs the emulator setup and a snapshot of your own running
  game (`docs/kb/emulator-pilot.md` "Setup", "Snapshot", "Scenes"). `humans` = a decision for the owner; agents do
  not take it.
- **Needs**: rows that must be done first.
- **Who**: `open`, or `github/agent (date)` once the owner copies a claim here. Live claims are comments on the
  "Work board claims" issue; `board.py open` reads both.
- **A row is done** when its pull request is merged into `main`, its tests pass, and its kb note says what was
  checked or ported and what is still not covered.

## E: emulator checks

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| E1 | emu | | open | Frame order: the HKS stick reads (a step's or an attack's direction, the run stop) see this frame's stick in RSekiro. Check which frame's stick the exe's HKS reads and match it | `docs/kb/frame-coverage.md` "One frame's order", `player::hks_pad_in`; harness `animstart`, `acttrace` |
| E2 | emu | | open | Frame order: the old clip's last TAE window on the frame a reaction starts, and the windows of an attack that `decide` replaces in the same tick | kb/frame-coverage.md "Not covered"; kb/taewin.md (state events last one update longer in the exe) |
| E3 | emu | | open | Frame order: hit-stop frames, and which frame a hit's stop starts on when the attacker updates earlier in the same frame | kb/modticks.md "Not covered"; harness `modticks`, `hits` |
| E4 | emu | | open | Frame order: one-shot SpEffects 401 / 67 and weapon style 32 land one tick late in RSekiro; the exe runs them in the step's frame | kb/frame-coverage.md |
| E5 | emu | | open | A bullet's hit timing: the exe's bullet update is a world task, not traced yet. Trace it and compare `enemy_bullet.rs` / `prosthetic.rs` | kb/frame-coverage.md "Item use and prosthetic bullets" |
| E6 | emu | | open | Other prosthetics (shuriken, firecracker, flame vent, umbrella): find how the exe switches Wolf's tool in the emulator copy (writing PlayerGameData +0x5ec did not), then check the bullet spawns against `prosthetic.rs` | harness `items`; kb/frame-coverage.md |
| E7 | emu | | open | Grapple: make a scene near a grapple point (`emu-cases scene`), then check `wire.rs` against the exe | kb/emulator-pilot.md "Scenes"; kb/grapple.md |
| E8 | emu | | open | Copied sekiro-rs collision (`crates/sim`): run the exe's casts and body pushes on the same inputs and compare | `crates/README.md`: read from the exe, never run against it; harness `collide`, `proxy` |
| E9 | emu | | open | Copied HKS engine functions (`crates/hks`, the env and act calls the VM makes): compare with the exe's env dispatcher FUN_140b36410 and act dispatcher FUN_140b32410 | harness `hks`, `hks2`; kb/hks-graph.md |
| E10 | emu | | open | Copied enemy AI timing (`crates/sim` brain): goal timers, the search scheduler, decision rates against the exe | harness `aimove`, `aiframe`; kb/aicpp.md |
| E11 | emu | | open | AI interrupts and sound targets: raise a sound and an interrupt in the emulator and check what the Lua reads | kb/aicpp.md "Not covered" (three lists) |
| E12 | emu | | open | Where the Attack Power rate enters HP damage | kb/atkrec.md |
| E13 | emu | | open | The enemy's guard-break knockback: the knockback controller FUN_140be6750 (ChrCtrl +0x90) against RSekiro's own knockback | kb/damage.md, kb/modticks.md "Not covered" |
| E14 | emu | | open | The rim leave point, and an enemy pushing a moving Wolf (both moving), and enemy against enemy | kb/movement.md "Not covered" |
| E15 | emu | | open | The 66 NPC attack keys that exist only through the exporter's +1000 fallback: does the exe fall back? FUN_140a12b80 has no such branch | kb/params.md "Not covered" |
| E16 | emu | | open | The damage sum's flag factors (1.4 / 1.125 / 0.8 / p16 / p17): tie them to their callers | kb/damage.md "Not covered" |
| E17 | emu | | open | Fall damage leftovers: CalcCorrectGraph 211 / 212, HitMtrlParam fallDamageRate, the 5 m threshold under SpEffect type 0x155, the void fall cost | kb/movement.md "In the air" |
| E18 | emu | | open | Camera leftovers: the 1 cm fulcrum floor, the zero-length final cast, the TAE overrides of the distance rate (FUN_1408311a0) and of +0xf0 (FUN_140831190), SlowEnd | kb/camera.md "Not covered" (three lists) |
| E19 | emu | | open | Per-boss check: Genichiro Ashina and Way of Tomoe (c7100, c7110), with a scene in the arena | Split per fight. kb/coverage.md; each boss so far is "seen side by side", none is checked against the exe |
| E20 | emu | | open | Per-boss check: Lady Butterfly (c5090) | as E19 |
| E21 | emu | | open | Per-boss check: Gyoubu Oniwa (c5080) | as E19; horse and arena |
| E22 | emu | | open | Per-boss check: Guardian Ape (c5100), both phases | as E19 |
| E23 | emu | | open | Per-boss check: Corrupted Monk (c5000) | as E19 |
| E24 | emu | | open | Per-boss check: Great Shinobi Owl and Owl (Father) (c5060) | as E19 |
| E25 | emu | | open | Per-boss check: Isshin Ashina and Isshin, the Sword Saint (c5430, c5400) | as E19 |
| E26 | emu | | open | Per-boss check: Demon of Hatred (c7020) | as E19 |
| E27 | emu | | open | Per-boss check: Emma, the Gentle Blade (c7400) | as E19 |
| E28 | emu | | open | Per-boss check: Divine Dragon (c5200) | as E19 |
| E29 | emu | | open | Per-boss check: Folding Screen Monkeys (c1000) and Old Dragons of the Tree (c5300): rules ported only, never seen side by side | as E19 |
| E30 | emu | | open | Params: scan the exe's accessors for every param field RSekiro reads, so a swapped field name is caught | kb/params.md "Not covered" |

## P: port gaps

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| P1 | any | | open | Goods bullets: pellets, Ako's and Gokan's sugars, spiritfall and the throwables are bullets in the exe. RSekiro only has the gourd. Port them from their clip, use frame and Bullet row | kb/frame-coverage.md "Item use and prosthetic bullets" has the table; check with harness `items` |
| P2 | any | | open | State events last one update longer in the exe (it dispatches an event on every update it overlaps): pass prev_t into every `events_at` caller (about 30 sites) | kb/taewin.md "Not covered" |
| P3 | any | | open | The empty gourd | handoff list; kb/frame-coverage.md |
| P4 | any | | open | 24 HKS env ids RSekiro does not answer | kb/behaviour-graph.md |
| P5 | any | | open | FXR EqualDistanceEmitter 301 (particles spaced along the path) and node emitters 1005 | kb/fxr.md "Not covered" |
| P6 | any | | open | SpEffect cycleOccurrenceSpEffectId rows (Ungo's 3421 -> 3422 -> 3423) and the refresh's fire flags | kb/speffect.md "Not covered" |
| P7 | any | | open | The AI search interval stretch (FUN_140614be0 on DAT_143d547d8; intervals up to 0.62 s, RSekiro keeps 0.2-0.4 s) | kb/aicpp.md "Not covered" |

## B: bugs and cleanup (good first rows)

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| B3 | any | | open | `cargo clippy` warnings: fix one module at a time, no behaviour change | one pull request per module |
| B5 | any | | open | A PowerShell twin of `tools/t.sh` (quiet test run) for contributors without bash | |
| B7 | any | | open | Build the map cache with `cargo run --release -p sr-extract` (crates/README.md; about 50 GB, kept outside git) and run `cargo test -p sekiro-game -p sekiro-sim` with `SEKIRO_CACHE` set: 34 tests read it and were not run after the 2026-11-04 trim. Make them skip without the cache so CI can run them | needs your own game |

## Done

| ID | Who | Date | Result |
|----|-----|------|--------|
