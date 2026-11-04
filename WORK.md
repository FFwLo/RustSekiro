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
  game (`docs/kb/emulator-pilot.md` "Setup", "Snapshot", "Scenes"). `game` = needs your own game running (side by side, no emulator). `humans` = a decision for the owner; agents do
  not take it.
- **Needs**: rows that must be done first.
- **Who**: `open`, or `github/agent (date)` once the owner copies a claim here. Live claims are comments on the
  "Work board claims" issue; `board.py open` reads both.
- **A row is done** when its pull request is merged into `main`, its tests pass, and its kb note says what was
  checked or ported and what is still not covered.

## C: characters (every enemy and boss)

One row per fighting character: bosses first, then mini-bosses, then the regular enemies. A new character gets
its row from `python tools/board_rows.py` (on `main`), so none is ever left out. A row puts that character side by
side with your own game; no emulator is needed: its behaviour is the game's own data (AI scripts, TAE, params, its
HKS), which RSekiro already runs, and the exe code all enemies share is checked once in the `E` rows.

1. Fight it in the real game and in RSekiro (`F1` menu: pick the fight). Best: record the real fight
   (`docs/kb/recordings.md`) and replay it (`python -m tools.rec replay`); else compare footage frame by frame.
2. Compare its attacks (which ones, timing, reach, damage and posture), its reactions (staggers, deflects, posture
   breaks, deathblow) and its AI choices (distance, order, phase changes).
3. Fix what differs, with a sim test that names it (`src/sim_tests/`), and say in `docs/kb/enemy.md` what was
   compared, with numbers, and what still differs. Split a big one into parts (C1a, C1b).

Its proof level now is in `docs/kb/coverage.md` and on the work map; a done row reaches "seen side by side". If a
character turns out to run exe code of its own, add an `E` row for that part.

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| C1 | game | | open | Per-boss check: Genichiro Ashina and Way of Tomoe (c7100, c7110), side by side with your own game | Split per fight (C1a, C1b); the C section's steps |
| C2 | game | | open | Per-boss check: Lady Butterfly (c5090) | as C1 |
| C3 | game | | open | Per-boss check: Gyoubu Oniwa (c5080) | as C1; horse and arena |
| C4 | game | | open | Per-boss check: Guardian Ape (c5100), both phases | as C1 |
| C5 | game | | open | Per-boss check: Corrupted Monk (c5000) | as C1 |
| C6 | game | | open | Per-boss check: Great Shinobi Owl and Owl (Father) (c5060) | as C1 |
| C7 | game | | open | Per-boss check: Isshin Ashina and Isshin, the Sword Saint (c5430, c5400) | as C1 |
| C8 | game | | open | Per-boss check: Demon of Hatred (c7020) | as C1 |
| C9 | game | | open | Per-boss check: Emma, the Gentle Blade (c7400) | as C1 |
| C10 | game | | open | Per-boss check: Divine Dragon (c5200) | as C1 |
| C11 | game | | open | Per-boss check: Folding Screen Monkeys (c1000) and Old Dragons of the Tree (c5300): rules ported only, never seen side by side | as C1 |
| C12 | game | | open | Per-miniboss check: General Naomori Kawarada; General Tenzen Yamauchi; ... (c1020) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C13 | game | | open | Per-miniboss check: Seven Ashina Spears - Shikibu Toshikatsu Yamauchi; Seven Ashina Spears - Shume Masaji Oniwa (c1021) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C14 | game | | open | Per-miniboss check: Long-arm Centipede Sen'un; Long-arm Centipede Giraffe (c1040) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C15 | game | | open | Per-miniboss check: Shinobi Hunter Enshin of Misen (c1050) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C16 | game | | open | Per-miniboss check: Juzou the Drunkard; Shigekichi of the Red Guard; ... (c1070) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C17 | game | | open | Per-miniboss check: Shichimen Warrior (c1080) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C18 | game | | open | Per-miniboss check: Armored Warrior (c1130) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C19 | game | | open | Per-miniboss check: Snake Eyes Shirafuji; Snake Eyes Shirahagi (c1191) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C20 | game | | open | Per-miniboss check: Mist Noble (c1300) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C21 | game | | open | Per-miniboss check: Okami Leader Shizu (c1310) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C22 | game | | open | Per-miniboss check: Headless (c1340) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C23 | game | | open | Per-miniboss check: Headless (c1350) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C24 | game | | open | Per-miniboss check: Blazing Bull (c1370) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C25 | game | | open | Per-miniboss check: Sakura Bull of the Palace (c1380) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C26 | game | | open | Per-miniboss check: Ashina Elite - Jinsuke Saze; Ashina Elite - Ujinari Mizuo (c1400) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C27 | game | | open | Per-miniboss check: Lone Shadow Vilehand; Lone Shadow Masanaga the Spear-Bearer; ... (c1470) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C28 | game | | open | Per-miniboss check: Chained Ogre (c5020) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C29 | game | | open | Per-miniboss check: O'Rin of the Water (c7000) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C30 | game | | open | Per-enemy check: "Ochimusha Castle One-handed Sword" (c1010) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C31 | game | | open | Per-enemy check: "Tutorial Ochimusha Hidden Prison One-handed Sword Face 1" (c1011) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C32 | game | | open | Per-enemy check: "Hanbei the Undying" (c1012) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C33 | game | | open | Per-enemy check: "Centipede Kodera Close Quarters Battle" (c1030) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C34 | game | | open | Per-enemy check: "Rotating Spearman Temple" (c1060) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C35 | game | | open | Per-enemy check: "Gecko valley" (c1100) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C36 | game | | open | Per-enemy check: "Handmaiden Aunt Honjo Castle Tower 4F" (c1110) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C37 | game | | open | Per-enemy check: "Handmaiden Aunt Valley" (c1111) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C38 | game | | open | Per-enemy check: "Lookout Castle Castle Mino" (c1120) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C39 | game | | open | Per-enemy check: "Earthen wall soldier valley lower layer" (c1140) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C40 | game | | open | Per-enemy check: "Inu temple" (c1150) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C41 | game | | open | Per-enemy check: "Innsmouth dog submerged city" (c1151) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C42 | game | | open | Per-enemy check: "Lower man mallet" (c1180) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C43 | game | | open | Per-enemy check: "Younger zombie sick village bell with tie" (c1181) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C44 | game | | open | Per-enemy check: "Valley Enemy Revenge Spirit Normal" (c1190) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C45 | game | | open | Per-enemy check: "Sokushinbutsu Temple" (c1200) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C46 | game | | open | Per-enemy check: "Sokushinbutsu_Centipede Temple" (c1210) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C47 | game | | open | Per-enemy check: "Cricket dungeon" (c1211) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C48 | game | | open | Per-enemy check: "Temple foundation Zako Temple fist Hoi only Kasaya" (c1220) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C49 | game | | open | Per-enemy check: "Shamo chicken_sick village" (c1240) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C50 | game | | open | Per-enemy check: "Monkey Valley Bare hands No sanding No foot effect" (c1250) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C51 | game | | open | Per-enemy check: "Chigo monkey_monkey" (c1260) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C52 | game | | open | Per-enemy check: "Piranha treasure carp samurai residence" (c1320) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C53 | game | | open | Per-enemy check: "Rappa Temple Normal Appearance 1" (c1360) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C54 | game | | open | Per-enemy check: "Tokugawa Rappa_Castle" (c1361) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C55 | game | | open | Per-enemy check: "Nightjars Revenge Spirit Proximity" (c1450) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C56 | game | | open | Per-enemy check: "Nightjars (kites) Honjo" (c1460) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C57 | game | | open | Per-enemy check: "Villager Zombie _ Bare Hand _ Grab Restraint" (c1500) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now rules ported |
| C58 | game | | open | Per-enemy check: "Villager Zombie (female) _ kitchen knife" (c1501) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C59 | game | | open | Per-enemy check: "Hallucinatory character _ villager zombie _ kitchen knife" (c1510) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C60 | game | | open | Per-enemy check: "Prisoner Zombie_Dungeon" (c1520) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C61 | game | | open | Per-enemy check: "Bandit_Torches Right side of light gauntlet" (c1550) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C62 | game | | open | Per-enemy check: "Tokugawa Samurai Castle_Tokugawa Invasion (for Ochimusha Hostile) Initial Equipment_Sword" (c1700) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C63 | game | | open | Per-enemy check: "The Phantom of the Haunting Monk Submerged City" (c5005) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C64 | game | | open | Per-enemy check: "The Great Serpent Castle in the Valley" (c5010) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C65 | game | | open | Per-enemy check: "Nishikigoi Submerged City" (c5050) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C66 | game | | open | Per-enemy check: "Chief Owl Honjo_When the Ninja Army invades" (c5070) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C67 | game | | open | Per-enemy check: "Tentacle_Mermaid Dragon Battle" (c5310) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now seen side by side |
| C68 | game | | open | Per-enemy check: "Okiryu (Large)" (c5320) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |
| C69 | game | | open | Per-enemy check: "Isshin Ashina" (c5410) side by side with your own game: its attacks, reactions and AI choices | the C section's steps; now data loaded |

## E: emulator checks

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| E6 | emu | | open | Other prosthetics (shuriken, firecracker, flame vent, umbrella): find how the exe switches Wolf's tool in the emulator copy (writing PlayerGameData +0x5ec did not), then check the bullet spawns against `prosthetic.rs` | harness `items`; kb/frame-coverage.md |
| E15 | emu | | open | The 66 NPC attack keys that exist only through the exporter's +1000 fallback: does the exe fall back? FUN_140a12b80 has no such branch | kb/params.md "Not covered" |
| E30 | emu | | open | Params: scan the exe's accessors for every param field RSekiro reads, so a swapped field name is caught | kb/params.md "Not covered" |

## P: port gaps

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| P1 | any | | open | Goods bullets: pellets, Ako's and Gokan's sugars, spiritfall and the throwables are bullets in the exe. RSekiro only has the gourd. Port them from their clip, use frame and Bullet row | kb/frame-coverage.md "Item use and prosthetic bullets" has the table; check with harness `items` |
| P2 | any | | open | State events last one update longer in the exe (it dispatches an event on every update it overlaps): pass prev_t into every `events_at` caller (about 30 sites) | kb/taewin.md "Not covered" |
| P3 | any | | open | The empty gourd | handoff list; kb/frame-coverage.md |
| P4 | any | | open | 24 HKS env ids RSekiro does not answer | kb/behaviour-graph.md |
| P5 | any | | open | FXR EqualDistanceEmitter 301 (particles spaced along the path) and node emitters 1005 | kb/fxr.md "Not covered"; system: VFX |
| P6 | any | | open | SpEffect cycleOccurrenceSpEffectId rows (Ungo's 3421 -> 3422 -> 3423) and the refresh's fire flags | kb/speffect.md "Not covered" |
| P7 | any | | open | The AI search interval stretch (FUN_140614be0 on DAT_143d547d8; intervals up to 0.62 s, RSekiro keeps 0.2-0.4 s) | kb/aicpp.md "Not covered" |
| P8 | any | | open | Genichiro / Tomoe sword lightning: the look in a real fight (the bolt's forked web, the blade charge colour, now orange through the tonemap) against the game; timing already matches the AI (3043 on knockdown, 15 s cooldown) | from the old manual test list; kb/fxr.md |
| P9 | any | | open | Cutscenes: none are ported. Play the game's realtime cutscenes (remo) and movies where the event scripts call them; start with one boss intro | system: Cutscenes; start docs/kb/cutscenes.md |
| P10 | any | | open | Event scripts on the open maps: only boss events run today (`enemy/script.rs`). Run a map's own events (doors, ambushes, flags) through the same interpreter, one map per part (P10a, P10b) | system: Event scripts; the `gap:` lines in src/enemy/script.rs |
| P11 | any | | open | Menus / HUD / saves: the game's own menus (equipment, prosthetic tools, skill tree, items, options) and saving and loading progress; today's menus are sized and timed by eye | system: Menus / HUD / saves; the `gap:` lines in src/menu.rs, src/hud.rs |
| P12 | any | | open | Docking (wall hug, ledge hang): finish what kb/docking.md parked: the exe's search box, the air hang reach (env 3045), the edge end check | system: Docking; kb/docking.md, the `gap:` lines in src/docking.rs |
| P13 | any | | open | Behaviour graph: Wolf's state machines, layers and blends are built by hand; run them from c0000.hkx (`src/hkb_runtime.rs` is the start) | system: Behaviour graph; kb/behaviour-graph.md "Plan" |
| P14 | any | | open | Sound leftovers: sounds play at the actor root, not the TAE event's DummyPoly; no map reverb; the floor and armour sound sets; occlusion | system: Sound; kb/sound.md gaps |
| P15 | any | | open | NPC quests / dialogue: only the talk itself is ported (`enemy/talk.rs`); the quest steps from the NPCs' talk scripts and flags, lip sync | system: NPC quests / dialogue |
| P16 | any | | open | Skills leftovers: the item-side gaps (kb/skills.md, 2026-10-22) and the status gaps (posture scaling, the protectors' resists) | system: Skills; the `gap:` lines in src/status.rs |
| P17 | any | | open | Rendering / shaders / lighting leftovers: the height fog's local lights, shadowed variant and slice count; cloth transition sets; the castle's fire sky | system: Rendering / shaders / lighting; the `gap:` lines in src/height_fog.rs, src/cloth.rs, src/sky.rs |
| P18 | any | | open | Player movement leftovers: the 62 `gap:` lines in src/player.rs (plunges, the objects' own collision, ...), one cluster per part (P18a, P18b) | system: Player movement; kb/movement.md |
| P19 | any | | open | One extractor: fold `crates/extract` (sr-extract, the world cache) into `tools/sekiro-extract`. Today both unpack the install (sr-extract all of it into cache/raw, about 32 GB; sekiro-extract its own parts into extracted/); make one unpack serve both and one binary run `tools/extract.ps1 -World`. They already share the keys and param defs (2026-11-04) | crates/README.md; tools/extract.ps1 |

## B: bugs and cleanup (good first rows)

| ID | Area | Needs | Who | Task | Notes |
|----|------|-------|-----|------|-------|
| B3 | any | | open | `cargo clippy` warnings: fix one module at a time, no behaviour change | one pull request per module |
| B7 | any | | open | Build the map cache with `cargo run --release -p sr-extract` (crates/README.md; about 50 GB, kept outside git) and run `cargo test -p sekiro-game -p sekiro-sim` with `SEKIRO_CACHE` set: 34 tests read it and were not run after the 2026-11-04 trim. Make them skip cleanly without the cache, so a plain `cargo test` passes for everyone | needs your own game |
| B8 | any | | open | `config.toml` keeps only player and sandbox settings: `buffer`, `regen_delay`, `hp_scales_regen` and `default_ratio_type` already are the exe's behaviour at their defaults, so fix them in code; `player_reach`, `enemy_reach` and `attack_arc` are only a fallback for tests without a model, so move them into the test code; mark `attack_range` and `cooldown_min/max` as the fallback AI's (enemies with no script). No behaviour change | the notes in `config.toml`; `combat.rs` reach fallback, `enemy.rs` fallback combos, `actor.rs` posture regen |

## Done

| ID | Who | Date | Result |
|----|-----|------|--------|
| B5 | FFwLo/heron | 2026-11-04 | tools/t.ps1, a PowerShell twin of t.sh for contributors without bash (#4) |
| E8 | FFwLo/claude | 2026-11-04 | crates/sim collision vs the exe's casts and pushes (radius 0.01, keep 0.05, support/ceiling contacts), emu_srcollide tests |
| E9 | FFwLo/claude | 2026-11-04 | crates/hks env and act calls vs FUN_140b36410 / FUN_140b32410, emu_hksvm tests |
| E34 | FFwLo/claude | 2026-11-04 | map_world back_read matches WorldBackRead frame by frame (46 scenarios), 83256bd |
| E10 | FFwLo/claude | 2026-11-04 | goal life f32 countdown, REGISTER_GOAL_UPDATE_TIME timer, aimove hook reads xmm2 (922573e); search ring earlier. Left: goal order frame by frame vs our Lua run |
| E1 | FFwLo/claude | 2026-11-04 | the HKS reads the stick of its buttons' frame (hksstick, 44 frames match), 7747a65 |
| E5 | FFwLo/claude | 2026-11-04 | bullet task runs first in MoveMapStep, first update only launches; shuriken hits/chains match (bullets), 517aabb |
| E31 | FFwLo/claude | 2026-11-04 | deathblow window = break anim's TAE 932 span (151 frames), broken_time stand-in removed; 20 chrs without a break anim keep 4 s (gap), f171729 |
| E13 | FFwLo/claude | 2026-11-04 | guard/posture break: KnockBackParam guardBrake 0/0 = no push (FUN_140be6750 is a position/facing controller, not knockback), 0972b2f |
| E11 | FFwLo/claude | 2026-11-04 | sound target pick, 46 on serial change, 31 LoseSightTarget, record refresh (aiint), 25b1b6b |
| E17 | FFwLo/claude | 2026-11-04 | fall landing/return rules from the exe: 1 HP floor, lethal = 0.4 maxHP or death, aged 5 m lethal, floor rates, void 448 (air_falldmg) |
| E7 | FFwLo/claude | 2026-11-04 | grapple: least-speed arc to the landing spot, braking 10, mean-velocity step, arrival by flight time; every flight frame checked (wire) |
| E2 | FFwLo/claude | 2026-11-04 | old clip's TAE window and attack records live into the next hit step (framewin, Actor held) |
| E3 | FFwLo/claude | 2026-11-04 | both hit-stops start in the hit step on the hit frame, whoever updates first; Sv1 already matched (hits_stop_*) |
| E4 | FFwLo/claude | 2026-11-04 | 67 / 401 / style 32 land in the step's frame (frameorder, player::tae_one_shots) |
| E33 | FFwLo/claude | 2026-11-04 | geometry bits, observer light remap, nose marks (aistealth); gap: Wolf's light index needs MSB MapVisibility per collision |
| E12 | FFwLo/claude | 2026-11-04 | AP rate enters via the hit entry's int16 percent (hprate) |
| E16 | FFwLo/claude | 2026-11-04 | x1.4 = defender flag 41 non-arrow; 1.125 / 0.8 / p16 dead; p17 stays 1 |
| E18 | FFwLo/claude | 2026-11-04 | 1 cm floor matched, TAE 152 AttachCameraToNode ported, SlowEnd copies only (camleft) |
| E32 | FFwLo/claude | 2026-11-04 | free camera swing is the full fulcrum swing; follow_swing stand-in removed (camswing) |
| E14 | FFwLo/claude | 2026-11-04 | chr push: stepping side lifted 0.3 m and stops at 0.950 m, other never shoved; humanoids pass, dogs block (pushmove); rim leave by the two-sphere ground check (collide_rimleave) |
