# Instructions for contributors and their AI agents

The board lives on the `agents-work-board` branch, not on `main`. The copies here are the only ones that count.
Everyone works from a fork, with the project as the remote `upstream` (the owner's own checkout says `origin`
where this file says `upstream`; the tools switch by themselves). Always read the board fresh:

```
git fetch -q upstream
git show upstream/agents-work-board:board.py | python - open        # open rows that nobody has claimed
git show upstream/agents-work-board:board.py | python - row <ID>    # one row in full, and its claim
git show upstream/agents-work-board:board.py | python - claims      # every live claim
```

`WORK.md` holds the rows (finished ones under "Done"), `CHANGES.md` the board's history. People and agents follow
the same steps.

## 1. Set up once

1. You need your own copy of Sekiro (Steam, patch 1.06). Nobody shares game files: each person extracts their
   own. Never fetch, share or ask for game files, snapshots or emulator cases.
2. Fork https://github.com/FFwLo/RustSekiro, clone your fork, and add the project:
   `git remote add upstream https://github.com/FFwLo/RustSekiro.git`.
3. Extract your data and check the setup as in `docs/CONTRIBUTING.md`. `python tools/doctor.py` must end with
   `ready`. If it does not, give your human the `fix:` lines and claim nothing.
4. Claude Code users can copy `tools/claude-skills/rsekiro-row` into `~/.claude/skills/` and say "do an RSekiro
   row".

**Names.** Run `gh api user --jq .login` for your GitHub name. An agent also picks a short bird or animal word
that no row or branch uses (not "claude", not "agent"). Claims, branches and pull requests carry both:
`<github>/<agent>`, for example `alice/wren`. Commit as your own human (their name and e-mail), never as the owner.

## 2. Pick one row

1. A row claimed under your own name and not finished: carry it on (`board.py mine <github>`).
2. New to the project: take a `B` row first.
3. Otherwise go top to bottom through `E` and `P`, and take the first row that `open` lists, that your setup can
   do (`emu` rows need the emulator, section 5), and whose `Needs` are done.

Never take `humans` rows: they are decisions for the owner. Rows can be big: you may claim a part (for example
`E19a` for one fight of a boss row) and say which part in the claim comment and the pull request. Work the board
does not list goes in a pull request to the board branch as a new row, or in an issue.

## 3. Claim it before you start

Claims are comments on the open issue "Work board claims". `board.py` writes and reads them, so a fork can claim
without pushing anything:

```
git show upstream/agents-work-board:board.py | python - claim <ID> <agent>
```

It refuses a row someone else holds. A claim lasts 7 days; run the same command again to keep it. If you stop,
release it: `... | python - release <ID>`. By hand, without the script, comment one line on the issue:
`claim <ID> <agent>` or `release <ID>`. The claim belongs to the comment's author, so you can only claim for
yourself. The first live claim wins.

## 4. Your worktree

An agent works in its own git worktree, never in another agent's folder:

```
python tools/worktree.py add <agent> <row ID> <slug>   # ~/rsekiro-<agent> on work-<ID>-<slug>-<github>-<agent>
python tools/worktree.py sync <agent>                   # after config.toml changes on your branch
python tools/worktree.py remove <agent>                 # after your pull request is merged
```

`extracted/` is git-ignored and stays in your main checkout. `add` makes a test root next to the worktree with the
branch's `config.toml` and a link to that `extracted/`, and prints the line to run the tests with
(`SHINOBI_ROOT=<test root> cargo test --profile quick <filter>`). Never copy or link `extracted/` into the worktree
itself. Branch names say what they are: `work-<ID>-<slug>-<github>-<agent>`, never `patch-1` or `fix`.

## 5. While you work

- Port what the game's code does. Code comments name the exe function (`FUN_1409e9c00`) and the offsets it reads;
  write `gap:` for what is not ported or not checked. Addresses and offsets are fine; the code is yours.
- Every behaviour change gets a sim test (`src/sim_tests/`). A test that needs `extracted/` skips itself when the
  data is missing, like `super_armor_hit_matches_the_exe`, so CI passes without game data.
- `E` rows follow `docs/kb/emulator-agent-brief.md` and `docs/kb/emulator-pilot.md`: your generator in a new
  `tools/emu-cases/src/<topic>.rs` with one `mod` line and one dispatch line in `main.rs`; your test in a new
  `src/sim_tests/emu_<topic>.rs`. The emulator runs your own `sekiro.exe` on a snapshot of your own running game.
  The paths in the brief are one PC's: use your own. Snapshots and case JSON stay in `extracted/` (snapshots are
  about 10 GB: delete them when done).
- `P` rows: read the row's kb note first. If a rule can only be fitted to numbers, say so in the note.
- Notes: a dated section in the topic's `docs/kb/<topic>.md` (what matched, what changed with numbers, what is not
  covered); `E` rows add one row to the results table in `docs/kb/emulator-pilot.md`; one line at the top of
  "## Recent" in `docs/PROGRESS.md`.
- Merge `upstream/main` into your branch at the start of each session.
- Never commit or paste game files, extracted data, snapshots, emulator case JSON, hex dumps, disassembly or
  decompiled code, also not in issues and pull requests. Read decompiler output on your own screen and write the
  code in your own words. The guard hooks refuse these; never bypass them. Files are LF.

## Spend tokens well

- Read the row's kb note and the module it names, not the whole repository. Grep before reading; read by range.
- Run only the covering tests while you work; the full suite once before the pull request.
- One row per session: start a fresh session for the next row.

## 6. Finish

1. The full suite passes: `SHINOBI_ROOT=<test root> cargo test --profile quick`. `python tools/guard.py push`
   passes (the pre-push hook runs it).
2. Commit as your human, push the branch to your fork, and open a pull request into `main`:
   `gh pr create --repo FFwLo/RustSekiro --base main --head <github>:<branch> --title "<ID>: <what changed>"`.
   Fill in the template. Put `Board: <ID> done: <one line>` in the body (or `Board: <ID> part: ...` for a part).
3. CI builds, runs the tests that need no game data, and runs the guard over your diff. The owner reviews and
   merges; only the owner merges into `main`. Answer review comments on the same branch.
4. After the merge the owner moves the row to "Done" (`board.py done`) and adds a line to `CHANGES.md`. Remove
   your worktree. If you give up, say why in a comment on the claims issue and release the claim.
