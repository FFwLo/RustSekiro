# Atlas-driven work passes

Instructions for an agent that improves one area of RustSekiro from the Sekiro Mechanics Atlas.
Areas run one at a time, in this order: movement, combat (combat + armor + deathblows), vfx (hit and
effect visuals: sparks, blood, FXR/FFX, plus sekiro-rs's lighting for our own scenes), bosses.

## Rules
- Repo: `the repo`, branch merged-test, crate/exe `sv1` (Bevy 0.20).
  Read `CLAUDE.md` first (where things are, how to query data cheaply).
- Values only from Sekiro sources (the atlas record's sources, params, TAE, HKS, exe). Never invent a
  number.
- sekiro-rs (`..\sekiro-rs`, AKJama's build, used here as a library) may be used where
  the atlas record says `his.has` is `yes` and ours is not: read his code at `his.where`, then call it
  through the library or port it into RustSekiro's style. Check his numbers against the record's game
  sources; where they differ, the game data wins and the difference goes in the report. Note each
  ported piece in `CREDITS.md`. This covers his combat rules (hit order, block vs deflect, posture by
  attribute, elements through guard, Mikiri hitbox, enemy guard reductions, armor parts), Attack
  Power, grapple markers, the three movement details, his boss fights (phases, attack order, special
  rules, Owl's poison floor, the Headless Ape terror; they must run on our enemy AI, not his actor
  system) and his lighting (light sets by standing collision, time of day and story lighting, fog,
  exposure, bloom, shadows, map shaders: used for our own scenes too, through the library where it
  can be). Still never edit his repo.
- Before using any sekiro-rs piece, check that it comes from the real game: every number, curve,
  timing and rule in it must trace to Sekiro's own data (params, TAE, HKS, AI Lua, EMEVD, GPARAM,
  BTL/MSB, the exe) or to a recording of the real game. Open his source and his doc for that piece,
  find where each value comes from, and confirm it against our extracted data. Use only what traces.
  Hand-tuned constants, guessed logic or look-alike tweaks (for example a fudge factor on light
  intensity, a made-up phase threshold) are not taken: replace them with the game's value if one
  exists, or leave that part out. The area report lists, per piece taken: what traced to which game
  source, and what was left out because it did not trace.
- Do not edit `..\sekiro-rs` (beyond what is already there), `shinobi-combat` (Sv1),
  `extracted/` or `config.toml`. No git commit, no push.
- Lighting: only by using sekiro-rs's lighting (above); no hand-tuned lighting of our own.
- Build: `taskkill //IM sv1.exe //F` first, then `CARGO_TARGET_DIR=target/bevy20 cargo build --release --bin sv1`.
  Tests: `CARGO_TARGET_DIR=target/bevy20 cargo test --profile quick --lib <filter>` (only the
  covering tests per change; no full suite per change). The full suite runs once at the end of the
  whole area.
- Small batches: one mechanic (or a few tightly linked ones) per batch. Build and run its tests after
  each batch. A batch that breaks a test and cannot be fixed properly is reverted and noted.
- Gameplay-feel changes (timings, speeds, windows) go in when the data says so, and are listed for the
  user's playtest in the report.
- Cleanup rule: when you touch code, remove dead code and needless complexity you find there and use a
  simpler way when one exists. Mirror a fix to similar code elsewhere in RustSekiro and say so.
- Never quote the user in repo files. Plain short sentences, no em dashes.

## Steps for an area
1. Pick records: with python, filter `docs/kb/mechanics-atlas.json` by the area's categories. Work list =
   records whose `ours.has` is `no` or `partial`, plus records with `verdict: corrected` whose values may
   differ from our code. Order by the `needs` links (build what others need first) and by gameplay
   weight. Skip records that need a recording (`needs_recording`) unless the data alone settles them;
   list those for the user.
2. For each record: read the record (values, how, check, sources), find our code (`ours.where`, grep),
   compare, then implement or correct it.
3. Test: add or extend a sim test (`src/sim_tests*.rs`, see how the existing ones drive Wolf and
   enemies) whose name or comment names the atlas id, checking the record's `check` sentence with the
   record's values. Run it.
4. Auto playtest at the end of each area: launch the built game for a short scripted run using the
   existing check hooks (`SHINOBI_*` env variables, see `src/menu.rs` header and CLAUDE.md), read the
   log for errors and panics, and take a few screenshots with the `screenshot-judge` agent if the change
   is visible. Kill the game after.
5. Update the atlas: set `ours` to `{"has":"yes|partial","where":"file"}` for what you built, then run
   `python tools/atlas_md.py`. Add one dated line to `docs/PROGRESS.md` for the area (what changed, test
   names, what is left).
6. Final reply (short): records done / partly done / skipped with reasons, tests added and their
   results, gameplay changes for the user to playtest, anything that needs a recording.
