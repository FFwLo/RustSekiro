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
- A behaviour change comes with a sim test (`src/sim_tests/`). A unit test that reads `extracted/` carries
  `#[cfg_attr(not(shinobi_data), ignore = "needs extracted/")]` (build.rs sets the cfg when the data is there);
  sim tests (`src/sim_tests/`) always need the data and CI skips them. Run the full suite with your own data before
  a pull request and paste its result line in the pull request.
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

In short, `tools/row.py` does it in four commands (`B` rows are good first ones):

```
python tools/row.py                            # your claims and pull requests, and rows you can take
python tools/row.py start <ID> --agent <name>  # claim it, make your worktree and test root (--agent only once)
python tools/row.py check                      # in the worktree: the guard and the full suite
python tools/row.py submit "<one line>"        # push to your fork and open the pull request, filled in
python tools/row.py verify <PR>                # check someone else's emu row on your own copy
```

An emu row's submit adds `--emu "<the emu-cases command that made your cases>"`; its numbers count as checked
once another contributor's `row.py verify` re-made them from their own exe and the tests passed (it comments
on the pull request). CI's `claim` check fails a pull request for a row someone else holds.

The claim is a comment on the "Work board claims" issue under your GitHub name; you can also just comment
`claim <ID> <name>` there from the web. Your open pull request holds the claim while it is in review. CI runs the
guard over your diff and checks the claim; it does not build (GitHub has no game data), so your `row.py check`
result in the pull request is the test. The owner reviews every pull request
and is the only one who merges; on the merge the row moves to Done by itself.
