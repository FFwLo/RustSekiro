# Work board

**Goal: RSekiro plays and feels exactly like Sekiro.** Every rule comes from the game's own files or from what
the game's code does, and is checked against your own copy of the game where it can be.

How to use this board is in `AGENTS.md` (pick, claim, finish). The copy on the `agents-work-board` branch is the
only one that counts. Finished rows move to "Done" at the bottom; board history is in `CHANGES.md`.

- **Kinds**: `C` checks one enemy or boss against the exe. `E` checks a system against the exe in the emulator (`tools/emu-cases`,
  `docs/kb/emulator-agent-brief.md`, `docs/kb/emulator-pilot.md`). `P` ports something RSekiro does not do yet
  (the "Not covered" lists in `docs/kb/`). `B` is bugs and cleanup: good first rows.
- **Order**: inside a kind, the top row first. Rows near the top change the feel the most.
- **Area**: `any` = anyone with the setup. `emu` = needs the emulator setup and a snapshot of your own running
  game (`docs/kb/emulator-pilot.md` "Setup", "Snapshot", "Scenes"). `humans` = a decision for the owner; agents do
  not take it.
- **Needs**: rows that must be done first.
- **Who**: `open`, or `github/agent (date)` once the owner copies a claim here. Live claims are comments on the
  "Work board claims" issue; `board.py open` reads both.
- **A row is done** when its pull request is merged into `main`, its tests pass, and its kb note says what was
  checked or ported and what is still not covered.

## C: characters (every enemy and boss)

One row per fighting character: bosses first, then mini-bosses, then the regular enemies. A new character gets
its row from `python tools/board_rows.py` (on `main`), so none is ever left out. A row checks that character
against the exe on your own copy: make a scene where it stands (`emu-cases scene`, `docs/kb/emulator-pilot.md`
"Scenes"), then compare its attacks (AtkParam rows, bullets, hit timing), its reactions (HKS hit answers,
posture, staggers) and its AI goals' choices (`docs/kb/aicpp.md`) with RSekiro's. Done when a
`..._matches_the_exe` test names it and `docs/kb/enemy.md` says what is still not covered. Split a big one into
parts (C1a, C1b). Its proof level now is in `docs/kb/coverage.md` and on the work map.

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| C1 | emu | | open | Per-boss check: Genichiro Ashina and Way of Tomoe (c7100, c7110), with a scene in the arena | Split per fight. kb/coverage.md; each boss so far is "seen side by side", none is checked against the exe |
| C2 | emu | | open | Per-boss check: Lady Butterfly (c5090) | as C1 |
| C3 | emu | | open | Per-boss check: Gyoubu Oniwa (c5080) | as C1; horse and arena |
| C4 | emu | | open | Per-boss check: Guardian Ape (c5100), both phases | as C1 |
| C5 | emu | | open | Per-boss check: Corrupted Monk (c5000) | as C1 |
| C6 | emu | | open | Per-boss check: Great Shinobi Owl and Owl (Father) (c5060) | as C1 |
| C7 | emu | | open | Per-boss check: Isshin Ashina and Isshin, the Sword Saint (c5430, c5400) | as C1 |
| C8 | emu | | open | Per-boss check: Demon of Hatred (c7020) | as C1 |
| C9 | emu | | open | Per-boss check: Emma, the Gentle Blade (c7400) | as C1 |
| C10 | emu | | open | Per-boss check: Divine Dragon (c5200) | as C1 |
| C11 | emu | | open | Per-boss check: Folding Screen Monkeys (c1000) and Old Dragons of the Tree (c5300): rules ported only, never seen side by side | as C1 |
| C12 | emu | | open | Per-miniboss check: General Naomori Kawarada; General Tenzen Yamauchi; ... (c1020): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C13 | emu | | open | Per-miniboss check: Seven Ashina Spears - Shikibu Toshikatsu Yamauchi; Seven Ashina Spears - Shume Masaji Oniwa (c1021): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C14 | emu | | open | Per-miniboss check: Long-arm Centipede Sen'un; Long-arm Centipede Giraffe (c1040): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C15 | emu | | open | Per-miniboss check: Shinobi Hunter Enshin of Misen (c1050): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C16 | emu | | open | Per-miniboss check: Juzou the Drunkard; Shigekichi of the Red Guard; ... (c1070): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C17 | emu | | open | Per-miniboss check: Shichimen Warrior (c1080): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C18 | emu | | open | Per-miniboss check: Armored Warrior (c1130): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C19 | emu | | open | Per-miniboss check: Snake Eyes Shirafuji; Snake Eyes Shirahagi (c1191): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C20 | emu | | open | Per-miniboss check: Mist Noble (c1300): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C21 | emu | | open | Per-miniboss check: Okami Leader Shizu (c1310): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C22 | emu | | open | Per-miniboss check: Headless (c1340): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C23 | emu | | open | Per-miniboss check: Headless (c1350): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C24 | emu | | open | Per-miniboss check: Blazing Bull (c1370): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C25 | emu | | open | Per-miniboss check: Sakura Bull of the Palace (c1380): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C26 | emu | | open | Per-miniboss check: Ashina Elite - Jinsuke Saze; Ashina Elite - Ujinari Mizuo (c1400): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C27 | emu | | open | Per-miniboss check: Lone Shadow Vilehand; Lone Shadow Masanaga the Spear-Bearer; ... (c1470): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C28 | emu | | open | Per-miniboss check: Chained Ogre (c5020): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C29 | emu | | open | Per-miniboss check: O'Rin of the Water (c7000): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C30 | emu | | open | Per-enemy check: "Ochimusha Castle One-handed Sword" (c1010): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C31 | emu | | open | Per-enemy check: "Tutorial Ochimusha Hidden Prison One-handed Sword Face 1" (c1011): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C32 | emu | | open | Per-enemy check: "Hanbei the Undying" (c1012): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C33 | emu | | open | Per-enemy check: "Centipede Kodera Close Quarters Battle" (c1030): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C34 | emu | | open | Per-enemy check: "Rotating Spearman Temple" (c1060): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C35 | emu | | open | Per-enemy check: "Gecko valley" (c1100): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C36 | emu | | open | Per-enemy check: "Handmaiden Aunt Honjo Castle Tower 4F" (c1110): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C37 | emu | | open | Per-enemy check: "Handmaiden Aunt Valley" (c1111): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C38 | emu | | open | Per-enemy check: "Lookout Castle Castle Mino" (c1120): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C39 | emu | | open | Per-enemy check: "Earthen wall soldier valley lower layer" (c1140): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C40 | emu | | open | Per-enemy check: "Inu temple" (c1150): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C41 | emu | | open | Per-enemy check: "Innsmouth dog submerged city" (c1151): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C42 | emu | | open | Per-enemy check: "Lower man mallet" (c1180): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C43 | emu | | open | Per-enemy check: "Younger zombie sick village bell with tie" (c1181): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C44 | emu | | open | Per-enemy check: "Valley Enemy Revenge Spirit Normal" (c1190): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C45 | emu | | open | Per-enemy check: "Sokushinbutsu Temple" (c1200): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C46 | emu | | open | Per-enemy check: "Sokushinbutsu_Centipede Temple" (c1210): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C47 | emu | | open | Per-enemy check: "Cricket dungeon" (c1211): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C48 | emu | | open | Per-enemy check: "Temple foundation Zako Temple fist Hoi only Kasaya" (c1220): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C49 | emu | | open | Per-enemy check: "Shamo chicken_sick village" (c1240): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C50 | emu | | open | Per-enemy check: "Monkey Valley Bare hands No sanding No foot effect" (c1250): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C51 | emu | | open | Per-enemy check: "Chigo monkey_monkey" (c1260): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C52 | emu | | open | Per-enemy check: "Piranha treasure carp samurai residence" (c1320): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C53 | emu | | open | Per-enemy check: "Rappa Temple Normal Appearance 1" (c1360): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C54 | emu | | open | Per-enemy check: "Tokugawa Rappa_Castle" (c1361): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C55 | emu | | open | Per-enemy check: "Nightjars Revenge Spirit Proximity" (c1450): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C56 | emu | | open | Per-enemy check: "Nightjars (kites) Honjo" (c1460): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C57 | emu | | open | Per-enemy check: "Villager Zombie _ Bare Hand _ Grab Restraint" (c1500): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now rules ported |
| C58 | emu | | open | Per-enemy check: "Villager Zombie (female) _ kitchen knife" (c1501): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C59 | emu | | open | Per-enemy check: "Hallucinatory character _ villager zombie _ kitchen knife" (c1510): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C60 | emu | | open | Per-enemy check: "Prisoner Zombie_Dungeon" (c1520): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C61 | emu | | open | Per-enemy check: "Bandit_Torches Right side of light gauntlet" (c1550): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C62 | emu | | open | Per-enemy check: "Tokugawa Samurai Castle_Tokugawa Invasion (for Ochimusha Hostile) Initial Equipment_Sword" (c1700): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C63 | emu | | open | Per-enemy check: "The Phantom of the Haunting Monk Submerged City" (c5005): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C64 | emu | | open | Per-enemy check: "The Great Serpent Castle in the Valley" (c5010): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C65 | emu | | open | Per-enemy check: "Nishikigoi Submerged City" (c5050): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C66 | emu | | open | Per-enemy check: "Chief Owl Honjo_When the Ninja Army invades" (c5070): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C67 | emu | | open | Per-enemy check: "Tentacle_Mermaid Dragon Battle" (c5310): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now seen side by side |
| C68 | emu | | open | Per-enemy check: "Okiryu (Large)" (c5320): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |
| C69 | emu | | open | Per-enemy check: "Isshin Ashina" (c5410): its attacks, reactions and AI goals against the exe, with a scene where it stands | the C section's steps; now data loaded |

## E: emulator checks

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| E1 | emu | | open | Frame order: the HKS stick reads (a step's or an attack's direction, the run stop) see this frame's stick in RSekiro. Check which frame's stick the exe's HKS reads and match it | `docs/kb/frame-coverage.md` "One frame's order", `player::hks_pad_in`; harness `animstart`, `acttrace` |
| E2 | emu | | open | Frame order: the old clip's last TAE window on the frame a reaction starts, and the windows of an attack that `decide` replaces in the same tick | kb/frame-coverage.md "Not covered"; kb/taewin.md (state events last one update longer in the exe) |
| E3 | emu | | open | Frame order: hit-stop frames, and which frame a hit's stop starts on when the attacker updates earlier in the same frame | kb/modticks.md "Not covered"; harness `modticks`, `hits` |
| E4 | emu | | open | Frame order: one-shot SpEffects 401 / 67 and weapon style 32 land one tick late in RSekiro; the exe runs them in the step's frame | kb/frame-coverage.md |
| E5 | emu | | open | A bullet's hit timing: the exe's bullet update is a world task, not traced yet. Trace it and compare `enemy_bullet.rs` / `prosthetic.rs` | kb/frame-coverage.md "Item use and prosthetic bullets" |
| E6 | emu | | open | Other prosthetics (shuriken, firecracker, flame vent, umbrella): find how the exe switches Wolf's tool in the emulator copy (writing PlayerGameData +0x5ec did not), then check the bullet spawns against `prosthetic.rs` | harness `items`; kb/frame-coverage.md |
| E7 | emu | | open | Grapple: make a scene near a grapple point (`emu-cases scene`), then check `wire.rs` against the exe | kb/emulator-pilot.md "Scenes"; kb/grapple.md |
| E8 | emu | | open | Copied sekiro-rs collision (`crates/sim`): run the exe's casts and body pushes on the same inputs and compare | `crates/README.md`: read from the exe, never run against it; harness `collide`, `proxy` |
| E9 | emu | | open | Copied HKS engine functions (`crates/hks`, the env and act calls the VM makes): compare with the exe's env dispatcher FUN_140b36410 and act dispatcher FUN_140b32410 | harness `hks`, `hks2`; kb/hks-graph.md |
| E10 | emu | | open | Copied enemy AI timing (`crates/sim` brain): goal timers, the search scheduler, decision rates against the exe | harness `aimove`, `aiframe`; kb/aicpp.md |
| E11 | emu | | open | AI interrupts and sound targets: raise a sound and an interrupt in the emulator and check what the Lua reads | kb/aicpp.md "Not covered" (three lists) |
| E12 | emu | | open | Where the Attack Power rate enters HP damage | kb/atkrec.md |
| E13 | emu | | open | The enemy's guard-break knockback: the knockback controller FUN_140be6750 (ChrCtrl +0x90) against RSekiro's own knockback | kb/damage.md, kb/modticks.md "Not covered" |
| E14 | emu | | open | The rim leave point, and an enemy pushing a moving Wolf (both moving), and enemy against enemy | kb/movement.md "Not covered" |
| E15 | emu | | open | The 66 NPC attack keys that exist only through the exporter's +1000 fallback: does the exe fall back? FUN_140a12b80 has no such branch | kb/params.md "Not covered" |
| E16 | emu | | open | The damage sum's flag factors (1.4 / 1.125 / 0.8 / p16 / p17): tie them to their callers | kb/damage.md "Not covered" |
| E17 | emu | | open | Fall damage leftovers: CalcCorrectGraph 211 / 212, HitMtrlParam fallDamageRate, the 5 m threshold under SpEffect type 0x155, the void fall cost | kb/movement.md "In the air" |
| E18 | emu | | open | Camera leftovers: the 1 cm fulcrum floor, the zero-length final cast, the TAE overrides of the distance rate (FUN_1408311a0) and of +0xf0 (FUN_140831190), SlowEnd | kb/camera.md "Not covered" (three lists) |
| E30 | emu | | open | Params: scan the exe's accessors for every param field RSekiro reads, so a swapped field name is caught | kb/params.md "Not covered" |
| E31 | emu | | open | Deathblow window: how long an enemy stays broken after a posture break before it recovers. RSekiro uses a stand-in, `config.toml` `broken_time` = 4 s. Find what ends the break in the exe (the break anim's length or TAE, an HKS state timer, or a C++ timer), port it per enemy, and remove `broken_time` | `enemy.rs` `on_posture_break`, `combat.rs`; kb/damage.md |
| E32 | emu | | open | Free camera swing after a sideways run: the CameraParam fulcrum gives 68 deg/s at a run and 143 at a sprint, which circled Wolf in RSekiro, so `config.toml` `follow_swing` halves it. Run the exe's free camera (FUN_140734790) with the stick held sideways, match its swing, and remove `follow_swing` | `camera.rs` `drag_fulcrum`; kb/camera.md; the harness of `follow_camera_matches_the_exe` |

## P: port gaps

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| P1 | any | | open | Goods bullets: pellets, Ako's and Gokan's sugars, spiritfall and the throwables are bullets in the exe. RSekiro only has the gourd. Port them from their clip, use frame and Bullet row | kb/frame-coverage.md "Item use and prosthetic bullets" has the table; check with harness `items` |
| P2 | any | | open | State events last one update longer in the exe (it dispatches an event on every update it overlaps): pass prev_t into every `events_at` caller (about 30 sites) | kb/taewin.md "Not covered" |
| P3 | any | | open | The empty gourd | handoff list; kb/frame-coverage.md |
| P4 | any | | open | 24 HKS env ids RSekiro does not answer | kb/behaviour-graph.md |
| P5 | any | | open | FXR EqualDistanceEmitter 301 (particles spaced along the path) and node emitters 1005 | kb/fxr.md "Not covered" |
| P6 | any | | open | SpEffect cycleOccurrenceSpEffectId rows (Ungo's 3421 -> 3422 -> 3423) and the refresh's fire flags | kb/speffect.md "Not covered" |
| P7 | any | | open | The AI search interval stretch (FUN_140614be0 on DAT_143d547d8; intervals up to 0.62 s, RSekiro keeps 0.2-0.4 s) | kb/aicpp.md "Not covered" |
| P8 | any | | open | Genichiro / Tomoe sword lightning: the look in a real fight (the bolt's forked web, the blade charge colour, now orange through the tonemap) against the game; timing already matches the AI (3043 on knockdown, 15 s cooldown) | from the old manual test list; kb/fxr.md |

## B: bugs and cleanup (good first rows)

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| B3 | any | | open | `cargo clippy` warnings: fix one module at a time, no behaviour change | one pull request per module |
| B5 | any | | open | A PowerShell twin of `tools/t.sh` (quiet test run) for contributors without bash | |
| B7 | any | | open | Build the map cache with `cargo run --release -p sr-extract` (crates/README.md; about 50 GB, kept outside git) and run `cargo test -p sekiro-game -p sekiro-sim` with `SEKIRO_CACHE` set: 34 tests read it and were not run after the 2026-11-04 trim. Make them skip without the cache so CI can run them | needs your own game |
| B8 | any | | open | `config.toml` keeps only player and sandbox settings: `buffer`, `regen_delay`, `hp_scales_regen` and `default_ratio_type` already are the exe's behaviour at their defaults, so fix them in code; `player_reach`, `enemy_reach` and `attack_arc` are only a fallback for tests without a model, so move them into the test code; mark `attack_range` and `cooldown_min/max` as the fallback AI's (enemies with no script). No behaviour change | the notes in `config.toml`; `combat.rs` reach fallback, `enemy.rs` fallback combos, `actor.rs` posture regen |

## Done

| ID | Who | Date | Result |
|----|-----|------|--------|
