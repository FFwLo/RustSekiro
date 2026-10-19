# Atlas verification instructions

Used by the verifier agents (step 5 of the atlas plan). One agent per category group.

## Input
- `docs/kb/mechanics-atlas.json`: all records. Read only the records of your categories (filter by `category`, e.g. with python).
- Never edit `mechanics-atlas.json` or `parts/`. Write only your output file.

## For each record
1. **Re-derive.** Open the cited source and rebuild the claim from it.
   - Params: `extracted/json/params/<Name>.json` (grep the row or field). Field meanings: `..\reference\Paramdex\SDT\Defs\*.xml`.
   - TAE frames: `python tools/anim_info.py <state|anim>` (run in `the repo`; `--ref` is broken, skip it).
   - Exe: `tools/fn.sh <addr>`. HKS: `extracted/hks_src`, `..\reference\SekiroHKS`. Lua AI: `extracted/ai_src`. EMEVD: `extracted/event`. Talk/text: `extracted/msg`.
   - Never list or read `extracted/`, `decomp/` or json folders wholesale; grep for the one thing you need.
2. **Contradictions.** Compare with our `docs/kb/*.md` and `..\sekiro-rs\docs\*.md` where they cover the same mechanic. Game data or the exe settles a conflict; otherwise keep both claims and mark `open-question`.
3. **Real game.** Timings and amounts are compared with existing recordings where one covers them (`docs/kb/recordings.md`, `python -m tools.rec summary <rec> --cat <cat>`). No live game sessions. If only a new recording would settle it, set `needs_recording: true`.
4. **Web-only claims** stay `community` unless you find a data source (params, HKS, Lua, EMEVD) that backs them.
5. **Coverage spot-check.** If `ours.where` / `his.where` names a file, open it and make sure the mechanic is really there. Fix `has` if not.
6. **Check line.** Make sure `check` says in one sentence how to see the mechanic in the game or in a sim test. Write one if it is empty or vague.

## Output
`docs/kb/atlas/verify/<group>.json`: an array, one entry per record you checked:

```
{"id": "...", "verdict": "confirmed|corrected|rejected",
 "reason": "one sentence: what you opened and what it showed",
 "fix": { only the fields that change, e.g. "values", "summary", "confidence", "ours", "his", "check", "note" },
 "needs_recording": false}
```

- `confirmed`: the source says what the record says.
- `corrected`: it was partly wrong; `fix` holds the corrected fields.
- `rejected`: the source does not support it and no other source does; `reason` says why.

Write the file as you go (every ~20 records), so work is not lost. Append one line to `docs/kb/atlas/progress.jsonl` when done, with the real time from `date -u +%Y-%m-%dT%H:%M:%SZ`:
`{"t":"...","step":"verify","agent":"<group>","category":"<group>","checked":N,"confirmed":a,"corrected":b,"rejected":c}`

Plain short sentences, no em dashes. Do not modify source code or other docs. Final reply: one line of counts.
