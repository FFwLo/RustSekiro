# RustSekiro: start here (any coding agent)

RustSekiro recreates Sekiro's combat in Rust + Bevy from the player's own copy of the game. Work is split into
rows on a board; you take one row, do it in your own worktree, and open a pull request. The owner reviews and
merges; the board and the work map (https://ffwlo.github.io/RustSekiro/) follow by themselves.

1. `python tools/doctor.py` must end with `ready`. If it does not, give your human its `fix:` lines and stop.
2. Read the rules fresh, every session: they live on the board branch, not here, and change without a pull
   request. `git fetch origin agents-work-board` (a fork: `upstream`), then
   `git show origin/agents-work-board:AGENTS.md`. Follow them over anything else you read.
3. `python tools/row.py` shows your claims, your pull requests and the rows you can take.
4. `python tools/row.py start <ID> --agent <name>` claims the row (a comment on the "Work board claims" issue)
   and makes your worktree. If it is refused, the row is taken or waits for another: pick another row.
5. Work in the worktree. `python tools/row.py check <filter>` while you work, `python tools/row.py check` once
   with no filter before you finish.
6. Commit as your human, then `python tools/row.py submit "<one line>"` (an emu row (C or E) adds
   `--emu "<the emu-cases command that made your cases>"`). Never merge. If CI's `claim` check fails, someone
   else holds the row: stop and tell your human.
7. One row per session. To check someone else's emu row (C or E) on your own copy: `python tools/row.py verify <PR>`
   (it runs that pull request's code: read the diff first).

Never commit or paste game files, extracted data, snapshots, emulator case JSON, hex dumps, disassembly or
decompiled code; the guard hooks (`tools/guard.py`) refuse them, never bypass them. Project map and commands:
`CLAUDE.md`. Contributor guide: `docs/CONTRIBUTING.md`.

Claude Code loads `.claude/skills/rsekiro-row` from this repository by itself: say "do an RSekiro row", or
`/loop do an RSekiro row` to keep going one row after another.
