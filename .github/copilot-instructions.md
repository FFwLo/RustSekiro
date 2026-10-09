# Instructions for AI agents (Copilot, etc.)

- Goal: Sekiro-accurate combat. Every value must come from the game data (TAE, HKS, params,
  behaviour graphs) or the exe decompilation. Never hand-guess a value; mark unknowns as "gap".
- Never commit or paste anything from `extracted/` (game files, decompiled exe). It is not in
  this repo; work that needs it must be done on the owner's PC.
- Read `docs/PROGRESS.md` and the matching `docs/kb/*.md` before changing a system.
- Task packets: `docs/HANDOFF_PLAN.md`. Write your result as `docs/reports/<ID>.md`
  (what changed, where, which data proves it, what is still open).
- Keep the style of the surrounding code; add a test in `src/sim_tests.rs` for behaviour changes.
- `cargo check` works without game data; `cargo test` and running the game need `extracted/`.
