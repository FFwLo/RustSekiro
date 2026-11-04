## What changed

<!-- One or two plain sentences. -->

## Board

Board: <ID> done: <one line>  <!-- or "Board: <ID> part: ...", or "Board: none" -->

## How it was checked

- [ ] The full suite passes with my own `extracted/` (SHINOBI_ROOT=<test root>): paste its `test result:` line here
- [ ] New behaviour has a sim test; a unit test that reads game data carries `cfg_attr(not(shinobi_data), ignore)`
- [ ] For an emu row (C or E): the emulator run (functions, case count) and the kb / emulator-pilot.md notes, and an
      "Emulator: `emu-cases ...`" line (the command that made the cases) so someone else can `row.py verify` it
- [ ] The kb note says what is still not covered (`gap:`)

## Rules

- [ ] No game files, extracted data, snapshots, emulator case JSON, hex dumps, disassembly or decompiled code
- [ ] The code and notes are in my own words; exe function addresses and offsets only
- [ ] `python tools/guard.py push` passes
