---
name: rsekiro-row
description: Do one row of the RSekiro work board end to end (RustSekiro, a Rust/Bevy recreation of Sekiro that runs on your own copy of the game) exactly as its AGENTS.md says - check the setup with tools/doctor.py, read AGENTS.md and the board fresh, carry on or pick a row, claim it, work in your own worktree, run the tests, open a pull request into main, and report. Use it whenever the user says do an RSekiro row, next RSekiro row, carry on the RustSekiro board, or runs /loop with it.
---

# One RSekiro row

Every rule is in `AGENTS.md` on the board branch; this skill holds none of them, so the two can never disagree.
Read it fresh on every run and follow it:

```
git fetch -q upstream          # the owner's own checkout: origin, and read origin/ below
git show upstream/agents-work-board:AGENTS.md
```

Then:

1. `python -I tools/doctor.py`. If it does not end with `ready`, stop and give your human the `fix:` lines.
2. `python tools/row.py`: your claims and open pull requests first (carry those on), then the rows you can take.
3. `python tools/row.py start <ID> --agent <name>` (a short bird or animal word no row or branch uses; `--agent`
   only the first time). If it is refused, pick another row. Work in the worktree it prints; read the notes it
   lists and the module the row names first.
4. Run the covering tests while you work (`python tools/row.py check <filter>`), then `python tools/row.py check`
   once with no filter. The guard must pass.
5. Commit as your human, then `python tools/row.py submit "<one line>"` (`--part` for a part of a row; an emu row
   adds `--emu "<the emu-cases command that made the cases>"`). Tick the boxes it leaves open. If CI's `claim`
   check fails, someone else holds the row: stop and say so. Do not merge: the owner reviews and merges, and the board follows by itself.

Never commit or paste game files, extracted data, snapshots, emulator case JSON, disassembly or decompiled code.

At the end tell your human: the pull request link, what was checked or ported (with numbers), what is still not
covered, and the full test result.
