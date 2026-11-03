# Contributing

RSekiro rebuilds Sekiro's combat in Rust and Bevy from the game's own files and from what the game's code does.
It runs on your own copy of the game. Help is welcome from people and from their AI agents; both follow the same
steps.

## Set up

1. Own Sekiro on Steam (patch 1.06). Install Rust (https://rustup.rs), Python 3.10+, Git and the GitHub CLI
   (`gh auth login`). Node.js is optional (the game's effect files).
2. Fork https://github.com/FFwLo/RustSekiro and clone your fork. Add the project as `upstream`:
   `git remote add upstream https://github.com/FFwLo/RustSekiro.git`, then `git fetch upstream`.
3. Turn on the guard hooks: `git config core.hooksPath .githooks`.
4. Extract the data from your game into `extracted/` (git-ignored; it takes a while):
   `powershell -ExecutionPolicy Bypass -File tools\extract.ps1` (add `-Sekiro "<install folder>"` if the game is
   not in the default Steam folder).
5. `python tools/doctor.py` must end with `ready`. Each missing item prints its fix.

Rows of kind `E` also need the emulator (`tools/emu-cases`): see `docs/kb/emulator-pilot.md` "Setup".

## Rules

- No game data in the repository, issues or pull requests: no game files, no `extracted/` output, no memory
  snapshots, no emulator case JSON. Everyone extracts from their own copy.
- No decompiled code, disassembly or hex dumps. Read decompiler output on your own screen, then write the code in
  your own words. Exe function addresses (`FUN_140bdb470`) and field offsets are fine, and welcome in comments.
- Every number comes from the game's files or the game's code. Mark what is not known or not checked as `gap:`.
- A behaviour change comes with a sim test (`src/sim_tests/`). A test that needs `extracted/` should skip itself when
  the data is missing (most do not yet: board row B1). Run the full suite with your own data before a pull request.
- Commit as yourself. Plain short sentences in code comments and notes.

The guard (`tools/guard.py`, run by the hooks and by CI) refuses game file types and folders, binary files, emulator
cases, hex dumps, disassembly and decompiled C. Never bypass it. If it flags something that is really yours, say so
in the pull request.

## Pick a row and open a pull request

The work board is on the branch `agents-work-board`. Its `AGENTS.md` has every step:

```
git show upstream/agents-work-board:AGENTS.md
git show upstream/agents-work-board:board.py | python - open
```

In short: pick an open row (`B` rows are good first ones), claim it with
`git show upstream/agents-work-board:board.py | python - claim <ID> <your agent or nickname>` (a comment on the
"Work board claims" issue), work on a branch `work-<ID>-<slug>-<github>-<agent>`, run the tests, and open a pull
request into `main` from your fork. CI builds the code and the tests (it has no game data to run them) and runs the guard over your diff. The
owner reviews every pull request and is the only one who merges.
