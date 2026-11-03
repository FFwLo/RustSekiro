## What changed

<!-- One or two plain sentences. -->

## Board

Board: <ID> done: <one line>  <!-- or "Board: <ID> part: ...", or "Board: none" -->

## How it was checked

- [ ] `cargo test --profile quick` passes with my own `extracted/` (SHINOBI_ROOT=<test root>)
- [ ] New behaviour has a sim test; tests that need game data skip without it
- [ ] For an E row: the emulator run (functions, case count) and the kb / emulator-pilot.md notes
- [ ] The kb note says what is still not covered (`gap:`)

## Rules

- [ ] No game files, extracted data, snapshots, emulator case JSON, hex dumps, disassembly or decompiled code
- [ ] The code and notes are in my own words; exe function addresses and offsets only
- [ ] `python tools/guard.py push` passes
