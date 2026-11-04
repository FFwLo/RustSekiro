# Instructions for AI agents (Copilot, etc.)

- Goal: Sekiro-accurate combat. Every value must come from the game data (TAE, HKS, params,
  behaviour graphs) or from what the exe's code does. Never hand-guess a value; mark unknowns as "gap".
- How to contribute (setup, rules, the work board, pull requests): `docs/CONTRIBUTING.md`, then
  `AGENTS.md` on the branch `agents-work-board`.
- Never commit or paste anything from `extracted/` (game files, snapshots, emulator cases), hex dumps,
  disassembly or decompiled code. Write code in your own words; exe function addresses are fine.
  `python tools/guard.py` (the git hooks in `.githooks`) refuses the rest.
- Read the matching `docs/kb/*.md` before changing a system.
- Keep the style of the surrounding code; add a test in `src/sim_tests/` for behaviour changes.
- `cargo build` and the tests that need no game data work without `extracted/`; the rest need your
  own extracted data (tests find it through `SHINOBI_ROOT`).
