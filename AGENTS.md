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

**The short way.** `tools/row.py` on `main` does the claiming, the worktree and the pull request for you:

```
python tools/row.py                              # your claims and pull requests, and rows you can take
python tools/row.py start <ID> --agent <name>    # claim, worktree, test root, the notes to read (--agent once)
python tools/row.py check                        # in the worktree: guard + full suite, remembered for submit
python tools/row.py submit "<one line>"          # push to your fork, open the pull request filled in (--part;
                                                 # emu rows add --emu "<the emu-cases command>")
python tools/row.py verify <PR>                  # re-make someone else's E row cases from your exe, run its tests
python tools/row.py drop <ID>                    # give the row back and remove the worktree
```

An open pull request that says `Board: <ID> done:` holds your claim for as long as it is open, and the board
workflow moves the row to Done when the owner merges it. The sections below say what these commands do.

## 1. Set up once

1. You need your own copy of Sekiro (Steam, patch 1.06). Nobody shares game files: each person extracts their
   own. Never fetch, share or ask for game files, snapshots or emulator cases.
2. Fork https://github.com/FFwLo/RustSekiro, clone your fork, and add the project:
   `git remote add upstream https://github.com/FFwLo/RustSekiro.git`.
3. Extract your data and check the setup as in `docs/CONTRIBUTING.md`. `python tools/doctor.py` must end with
   `ready`. If it does not, give your human the `fix:` lines and claim nothing.
4. Your agent reads `AGENTS.md` on `main` by itself (the start steps). Claude Code also loads the repository's
   `rsekiro-row` skill: say "do an RSekiro row", or `/loop do an RSekiro row` for one row after another.

**Names.** Run `gh api user --jq .login` for your GitHub name. An agent also picks a short bird or animal word
that no row or branch uses (not "claude", not "agent"). Claims, branches and pull requests carry both:
`<github>/<agent>`, for example `alice/wren`. Commit as your own human (their name and e-mail), never as the owner.

## 2. Pick one row

1. A row claimed under your own name and not finished: carry it on (`board.py mine <github>`).
2. New to the project: take a `B` row first.
3. Otherwise go top to bottom through `C`, `E` and `P`, and take the first row that `open` lists, that your setup can
   do (`emu` rows need the emulator, section 5), and whose `Needs` are done.

Never take `humans` rows: they are decisions for the owner. Rows can be big: you may claim a part (for example
`C1a` for one fight of a boss row) and say which part in the claim comment and the pull request. Work the board
does not list goes in a pull request to the board branch as a new row, or in an issue.

## 3. Claim it before you start

Claims are comments on the open issue "Work board claims". `board.py` writes and reads them, so a fork can claim
without pushing anything:

```
git show upstream/agents-work-board:board.py | python - claim <ID> <agent>
```

`row.py start` runs it for you. It refuses a row someone else holds and a row whose `Needs` are not done. A claim
lasts 7 days; run the same command again to keep it, or open your pull request, which holds it. If you stop,
release it: `... | python - release <ID>`. By hand, without the script, comment one line on the issue:
`claim <ID> <agent>` or `release <ID>`. The claim belongs to the comment's author, so you can only claim for
yourself. The first live claim wins.

## 4. Your worktree

An agent works in its own git worktree, never in another agent's folder (`row.py start` makes it):

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
- `C` and `E` rows (Area `emu`) follow `docs/kb/emulator-agent-brief.md` and `docs/kb/emulator-pilot.md`: your generator in a new
  `tools/emu-cases/src/<topic>.rs` with one `mod` line and one dispatch line in `main.rs`; your test in a new
  `src/sim_tests/emu_<topic>.rs`. The emulator runs your own `sekiro.exe` on a snapshot of your own running game.
  The paths in the brief are one PC's: use your own. Snapshots and case JSON stay in `extracted/` (snapshots are
  about 10 GB: delete them when done).
- `P` rows: read the row's kb note first. If a rule can only be fitted to numbers, say so in the note.
- Notes: a dated section in the topic's `docs/kb/<topic>.md` (what matched, what changed with numbers, what is not
  covered); `emu` rows add one row to the results table in `docs/kb/emulator-pilot.md`. The board writes the
  history line (`CHANGES.md`) on the merge.
- Merge `upstream/main` into your branch at the start of each session.
- Never commit or paste game files, extracted data, snapshots, emulator case JSON, hex dumps, disassembly or
  decompiled code, also not in issues and pull requests. Read decompiler output on your own screen and write the
  code in your own words. The guard hooks refuse these; never bypass them. Files are LF.

## Spend tokens well

- Read the row's kb note and the module it names, not the whole repository. Grep before reading; read by range.
- Run only the covering tests while you work; the full suite once before the pull request.
- One row per session: start a fresh session for the next row.

## 6. Finish

1. `python tools/row.py check`: the guard, then the full suite through your test root
   (`SHINOBI_ROOT=<test root> cargo test --profile quick`). It remembers a passing run for this commit.
2. Commit as your human, then `python tools/row.py submit "<one line>"` (`--part` for a part, `--dry` to read it
   first). It pushes the branch to your fork and opens the pull request into `main`, titled `<ID>: <one line>`,
   with `Board: <ID> done: <one line>` and your test result already in the body. Tick the boxes it leaves open.
   After review changes: commit, `row.py check`, `row.py submit` again (it only pushes).
   An `emu` row (C or E) also passes `--emu "emu-cases <sekiro.exe> <out dir> [--snapshot <dir>] <names>"`, the command that
   made your cases; submit refuses an `emu` row whose diff has no generator in `tools/emu-cases/src/` or no
   `..._matches_the_exe` test (`--why "<reason>"` when the row truly has no emulator check).
3. CI runs the guard over your diff and fails the `claim` check when someone else holds the row (or its whole row, or one of its parts). It does not build: GitHub has no game data, so your `row.py check` result in the
   pull request is the test, and the owner may run it again on their own copy. The owner reviews and merges; only
   the owner merges into `main`.
   An `emu` row's numbers come from one person's copy of the game, so they count as checked only after someone
   else runs `python tools/row.py verify <PR>`: it checks the pull request out, re-makes the cases from their
   own exe (and snapshot) into a fresh folder and runs the new tests on them, then comments the result. It builds
   and runs the pull request's code: read the diff first.
4. On the merge the board workflow moves the row to "Done" and writes `CHANGES.md`; nobody needs to edit the
   board. Remove your worktree (`python tools/worktree.py remove <agent>`). If you give up, say why in a comment
   on the claims issue and `row.py drop <ID>`.
