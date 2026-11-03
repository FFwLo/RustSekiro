---
name: rsekiro-row
description: Do one row of the RSekiro work board end to end (RustSekiro, a Rust/Bevy recreation of Sekiro that runs on your own copy of the game) exactly as its AGENTS.md says - check the setup with tools/doctor.py, read AGENTS.md and the board fresh, carry on or pick a row, claim it, work in your own worktree, run the tests, open a pull request into main, and report. Use it whenever the user says do an RSekiro row, next RSekiro row, carry on the RustSekiro board, or runs /loop with it.
---

# One RSekiro row

Every step, command and rule is in `AGENTS.md` on the board branch. This skill holds none of them, so the two can
never disagree. Read it fresh on every run and follow it from top to bottom:

```
git fetch -q upstream          # the owner's own checkout: origin, and read origin/ below
git show upstream/agents-work-board:AGENTS.md
```

Then, in its order:

1. `python -I tools/doctor.py`. If it does not end with `ready`, stop and give your human the `fix:` lines.
2. Your names: `gh api user --jq .login`, and a short bird or animal word of your own that no row or branch uses.
3. Carry on your own claimed row, or pick one: `git show upstream/agents-work-board:board.py | python - open`.
4. Claim it: `... | python - claim <ID> <agent>`. If it is refused, pick another row.
5. `python tools/worktree.py add <agent> <ID> <slug>` and work there. Read the row's kb note and the module it
   names first. Use the SHINOBI_ROOT line `add` printed for the tests.
6. Run the covering tests while you work, the full suite once at the end. The guard hooks must pass.
7. Push the branch to your fork and open the pull request into `main` with `Board: <ID> done: <one line>` in the
   body; that line marks the row done for the owner. Do not merge it: the owner reviews and merges.
8. Only when your human is the owner: after the merge, in the board worktree (`python tools/worktree.py board`)
   run `python board.py done <ID> "<one line, PR>" <github>/<agent>`, add a line to `CHANGES.md`, commit, push.

Never commit or paste game files, extracted data, snapshots, emulator case JSON, disassembly or decompiled code.

At the end tell your human: the pull request link, what was checked or ported (with numbers), what is still not
covered, and the full test result.
