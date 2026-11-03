# Coverage: how far Sv1 is proven to match Sekiro

One row per character (every `cNNNN` the extracted data names) and one per system, with the
strongest proof found in the files that Sv1 matches the game there. Everything below the marker
line is generated; this intro is hand-written and kept by the generator.

## Regenerate

    python tools/coverage.py

About 15-30 s. It reads the code and kb from this repo, the extracted data from
`$SHINOBI_ROOT/extracted` (default `<repo>`) and the live recordings
and their replays from `$SHINOBI_REC_DIR` (default `<work>/memscope-data/logs`). It
writes this file with LF line ends. Re-run it after a merge; the counts follow the branch it runs on.

## Proof levels (lowest to highest; a row shows the highest it reached)

- **none**: Sv1 cannot load it (no `enemies/<chr>.json`, not named in `src/`), or no module exists.
- **data loaded**: Sv1 loads its data: the generic enemy loader (`data.rs` `EnemyFile`,
  `encounter_list`, the debug menu's roster) or a module that names it.
- **rules ported**: a sim test names it (characters) or tests cover the module (systems).
- **checked against the exe**: a test compares Sv1 with numbers the exe computed in the emulator
  (`extracted/emu/*.json`, `*_matches_the_exe`, rows of `emulator-pilot.md`).
- **seen side by side**: Sv1's behaviour was compared with the real game: an enemy replay against a
  live recording (`enemies_all.md`, `enemies_pressure.md`, `enemy_ours_c<row>.csv`,
  `pressure/ours_*.csv`), Wolf's HKS-event replay (`replay_all.md`), or still shots of both
  (`extracted/ref_frames`, kb/visuals.md).

## Characters: columns

- **Name**: the game's English name from its health bar (`enemies/npc_names.json`, NPC名 text);
  without a bar, the Paramdex row name in quotes (machine-translated, not shown in the game);
  `?` when nothing names it.
- **Kind**: `boss` / `mini-boss` from the map EMEVD's Display Boss / Miniboss Health Bar
  (2003[11] / 2003[87]) on any of its NpcParam rows. Folding Screen Monkeys' bar sits on a c1000
  dummy, so c1000 is a boss here.
- **Bundles**: game archives in `extracted/chr`: C chrbnd, A anibnd, B behbnd, T texbnd.
- **Model / Anim / HKS**: `model_<chr>.bin`, `anim_<chr>.bin`, `hks_src/<chr>.lua` exported;
  `via cXXXX` = it uses another chr's (roster `animChr`).
- **AI Lua**: its placements' NpcThinkParam battle goals that have game bytecode in
  `script/*.luabnd.d` (what `ai.rs` runs), as found / needed; `(src N)` when fewer have
  decompiled source in `ai_src`.
- **Loaded by Sv1**: `generic` = the roster loader takes it; `named in` = modules (not tests)
  that mention the chr id.
- **Rules checked**: sim tests (`src/sim_tests/`, not `#[ignore]`) whose body names the chr id or
  one of its NpcParam rows. c1020 is also the default enemy of `app()`.
- **Side by side**: comparisons (see the levels) plus weaker evidence that does not raise the
  level: `recorded live` (a recording of the real game names it), `reference clip` (clips.md
  footage names it), `kb/src lines cite a recording` (a line with the chr id and a `rec_` id).
- **Notes**: the boss fights it is cast in (`boss_scripts.json`), its arena
  (`map_boss_arenas.json`; `boss_71001000` is the shared stand-in arena), its battle goal ids,
  files citing its HKS, kb notes naming it.

## Systems: columns

- **Sv1 files**: the modules (line counts). **kb note**: the topic's notes in `docs/kb`.
- **Emulator-checked**: tests that read `extracted/emu`, the pilot rows, and emulator case files
  of that system that no test reads yet (other agents' work in progress).
- **Other tests**: unit tests in the system's files plus sim tests whose name has one of the
  system's keywords (a keyword list in `tools/coverage.py`; a test can count for several systems).
- **Side by side**: replays against recordings, still shots, and how many recordings carry the
  category (data captured, not a comparison by itself).
- **State**: one line written by hand in `tools/coverage.py` from the kb notes.

## Not measurable this way

- How close a match is: a test passing, a replay or a profile existing says nothing about the
  remaining gaps; the replays are counts of events and attack picks, not a tick-by-tick match.
- Feel: input latency, camera feel, animation weight, which no file records.
- Sim tests that use an enemy without naming it, and rules a test checks without naming the
  system (keyword matching both ways). Tests that hit only a chr's shared logic (c9997 HKS,
  common AI goals) do not count for each chr.
- Visual and audio match (models, shaders, effects, sound) beyond the few still shots: there is
  no automatic picture or sound comparison.
- Whether a cited exe function is ported in full (frame-coverage.md: citing is a proxy).
- The replay and pressure outputs live outside the repo and are made by a tool not in this branch
  (`tools/rec/enemies.py` per enemies_all.md); dated 2026-10-22, they predate later changes.
- Unplaced chr ids (texture-only or variant bundles): no data says what they are.

<!-- Generated below by tools/coverage.py: edit the intro above, not the tables. -->

Generated 2026-11-03 from <repo>/extracted and <work>/memscope-data/logs.

## Summary

- Characters: 146 chr ids (102 placed by the maps), 14 bosses, 18 mini-bosses.
- Characters by proof level: none 42, data loaded 56, rules ported 21, checked against the exe 0, seen side by side 27.
- Systems by proof level: none 1, data loaded 0, rules ported 11, checked against the exe 1, seen side by side 8.

## Characters

| chr | Name | Kind | Bundles | Model | Anim | AI Lua | HKS | Loaded by Sv1 | Rules checked (sim tests) | Side by side | Notes | Proof level |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| c0000 | Wolf, the player (src/player.rs) |  | CAB | yes | yes | - | yes | named in actor.rs, anim.rs, combat.rs, data.rs +9 | all 450 sim tests drive Wolf | Wolf event replay: 27 recs, 3974/4376 game events matched; 16 ref_frames shots | its HKS cited in 10 files (actor.rs, combat.rs, docking.rs, ...); not placed by any map (data: anim+chr+hks+model); kb: attacks.md, behaviour-graph.md, docking.md, grapple.md, guard-deflect.md, movement.md, skills.md, visuals.md | seen side by side |
| c1000 | Folding Screen Monkeys | boss | CAB | yes | yes | 2/2 goals | yes | generic (enemies/c1000.json); named in enemy/script.rs | 3: the_infinite_emblems_cheat_refills_them, the_no_posture_cheat_blocks_a_bar_filling_hit, ... | recorded live in 2; reference clip | the health bar sits on this dummy part; in fight: Folding Screen Monkeys; arena boss_12600050; battle Lua 011000,530010; kb: damage.md, visuals.md | rules ported |
| c1001 | "Dummy character for demon Buddha" (param row name) |  | CAB | yes | yes | 2/2 goals | yes | generic (enemies/c1001.json); named in enemy/script.rs | - | - | in fight: Demon of Hatred; battle Lua 011000,105000; kb: damage.md, enemy.md | data loaded |
| c1010 | "Ochimusha Castle One-handed Sword" (param row name) |  | CABT | yes | yes | 6/6 goals | yes | generic (enemies/c1010.json); named in actor.rs, combat.rs, config.rs, data.rs +8 | 44: a_blocked_hit_does_not_wake_a_held_enemy, a_cautious_enemy_listens_to_a_new_sound, ... | AI profile vs 10 recs (enemies_all.md); enemy replay csv in 8 recs; pressure replay rows 10100111,10102230; recorded live in 6; 1 kb/src lines cite a recording | battle Lua 101000,101010,101020,101030 +2; kb: damage.md, enemy.md, guard-deflect.md, map.md, sound.md, visuals.md | seen side by side |
| c1011 | "Tutorial Ochimusha Hidden Prison One-handed Sword Face 1" (param row name) |  | CT | yes | via c1010 | 2/2 goals | via c1010 | generic (enemies/c1011.json) | - | - | battle Lua 101100,101110 | data loaded |
| c1012 | "Hanbei the Undying" (param row name) |  | CT | yes | via c1010 | 1/1 goals | via c1010 | generic (enemies/c1012.json) | - | AI profile vs 2 recs (enemies_all.md); enemy replay csv in 2 recs; pressure replay rows 10120000 | battle Lua 101200; kb: enemy.md | seen side by side |
| c1013 | "Hanbei the Undying" (param row name) |  | CABT | yes | yes | no battle goal | yes | generic (enemies/c1013.json) | - | - | kb: damage.md | data loaded |
| c1019 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1020 | General Naomori Kawarada; General Tenzen Yamauchi; Leader Shigenori Yamauchi; General Kuranosuke Matsumoto | mini-boss | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c1020.json); named in actor.rs, ai.rs, anim.rs, cloth.rs +13 | 33: a_back_step_dodges_a_perilous_thrust, a_broken_enemy_elsewhere_keeps_the_stealth_plunge, ... (+211 more as the default enemy of app()) | AI profile vs 3 recs (enemies_all.md); enemy replay csv in 8 recs; pressure replay rows 10200000,10202000; 11 ref_frames shots; recorded live in 14; reference clip | arena boss_71001000; battle Lua 102000,102010; kb: attacks.md, damage.md, enemy.md, guard-deflect.md, map.md, movement.md, skills.md, sound.md, visuals.md; exe-checked: guard_resolution_matches_the_exe | seen side by side |
| c1021 | Seven Ashina Spears - Shikibu Toshikatsu Yamauchi; Seven Ashina Spears - Shume Masaji Oniwa | mini-boss | CT | yes | via c1020 | 1/1 goals | via c1020 | generic (enemies/c1021.json); named in actor.rs, combat.rs, data.rs, enemy.rs | 2: a_second_form_takes_its_hu_throw_rows, an_enemy_plays_its_own_anim_set | reference clip | arena boss_71001000; battle Lua 102020 | rules ported |
| c1029 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1030 | "Centipede Kodera Close Quarters Battle" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1030.json); named in enemy_bullet.rs | 2: a_fire_pool_burns_wolf_again_after_its_hit_record, a_killing_fire_hit_is_the_fire_death | - | battle Lua 103000; kb: visuals.md | rules ported |
| c1040 | Long-arm Centipede Sen'un; Long-arm Centipede Giraffe | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1040.json); named in cloth.rs | - | reference clip | arena boss_71001000; battle Lua 104000; kb: clips.md, damage.md, visuals.md | data loaded |
| c1050 | Shinobi Hunter Enshin of Misen | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1050.json) | - | recorded live in 1; reference clip | arena boss_71001000; battle Lua 105000; kb: damage.md, visuals.md | data loaded |
| c1060 | "Rotating Spearman Temple" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1060.json) | 5: a_blow_on_a_weak_guard_launches_the_enemy, a_bullet_past_its_range_weakens, ... | - | battle Lua 106000; kb: enemy.md | rules ported |
| c1070 | Juzou the Drunkard; Shigekichi of the Red Guard; Tokujiro the Glutton | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1070.json); named in cloth.rs, combat.rs, prosthetic.rs | 2: an_enemys_own_burst_cool_time_blocks_the_next_firecracker, the_shura_samurais_enchanted_blade_poisons_or_burns | reference clip | arena boss_71001000; battle Lua 107000; kb: damage.md, grapple.md, visuals.md | rules ported |
| c1080 | Shichimen Warrior | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1080.json) | - | reference clip | arena boss_71001000; battle Lua 108000; kb: grapple.md, sound.md, visuals.md | data loaded |
| c1090 | ? |  | A | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1100 | "Gecko valley" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1100.json); named in enemy_bullet.rs | 4: a_dodged_bullet_passes_through_wolf, a_penetrating_bullet_launches_by_its_hit_record, ... | - | battle Lua 110000; kb: visuals.md | rules ported |
| c1110 | "Handmaiden Aunt Honjo Castle Tower 4F" (param row name) |  | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c1110.json); named in data.rs, fmod.rs | 1: a_group_enemy_with_a_foreign_row_still_spawns | - | battle Lua 011000,111000; kb: visuals.md | rules ported |
| c1111 | "Handmaiden Aunt Valley" (param row name) |  | CT | yes | via c1110 | 1/1 goals | via c1110 | generic (enemies/c1111.json) | - | - | battle Lua 111000; kb: visuals.md | data loaded |
| c1120 | "Lookout Castle Castle Mino" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1120.json) | - | - | battle Lua 112000; kb: visuals.md | data loaded |
| c1130 | Armored Warrior | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1130.json); named in prosthetic.rs | - | reference clip | arena boss_71001000; battle Lua 113000 | data loaded |
| c1140 | "Earthen wall soldier valley lower layer" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1140.json); named in data.rs, sound.rs | - | - | battle Lua 114000 | data loaded |
| c1150 | "Inu temple" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1150.json); named in combat.rs, fmod.rs, prosthetic.rs | 4: a_firecracker_breaks_a_dogs_posture, a_maddened_dog_bites_the_soldier, ... | AI profile vs 1 recs (enemies_all.md); enemy replay csv in 2 recs; recorded live in 1 | battle Lua 115000; kb: damage.md | seen side by side |
| c1151 | "Innsmouth dog submerged city" (param row name) |  | CT | yes | via c1150 | 1/1 goals | via c1150 | generic (enemies/c1151.json); named in fmod.rs | - | - | battle Lua 115000 | data loaded |
| c1180 | "Lower man mallet" (param row name) |  | CABT | yes | yes | 3/3 goals | yes | generic (enemies/c1180.json); named in actor.rs, combat.rs, enemy.rs, sound.rs | 15: a_clips_anim_set_overwrites_the_resident_gate, a_respawned_enemy_gets_its_weapons_and_armour_back, ... | enemy replay csv in 3 recs; pressure replay rows 11801100; recorded live in 4 | battle Lua 118000,118020,118040; kb: damage.md, enemy.md, grapple.md, guard-deflect.md, visuals.md; exe-checked: hit_posture_matches_the_exe | seen side by side |
| c1181 | "Younger zombie sick village bell with tie" (param row name) |  | CT | yes | via c1180 | 2/2 goals | via c1180 | generic (enemies/c1181.json) | - | - | battle Lua 118000,118020 | data loaded |
| c1190 | "Valley Enemy Revenge Spirit Normal" (param row name) |  | CABT | yes | yes | 3/3 goals | yes | generic (enemies/c1190.json); named in cloth.rs, data.rs | 6: a_bullet_makes_its_appear_sound_where_it_starts, a_fans_pellets_hit_wolf_once, ... | - | battle Lua 119000,119020,119030; kb: enemy.md, visuals.md | rules ported |
| c1191 | Snake Eyes Shirafuji; Snake Eyes Shirahagi | mini-boss | CT | yes | via c1190 | 1/1 goals | via c1190 | generic (enemies/c1191.json) | 1: a_clips_anim_set_overwrites_the_resident_gate | reference clip | arena boss_71001000; battle Lua 119010; kb: visuals.md | rules ported |
| c1200 | "Sokushinbutsu Temple" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1200.json); named in data.rs | - | - | battle Lua 120000 | data loaded |
| c1210 | "Sokushinbutsu_Centipede Temple" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1210.json); named in gore.rs | - | - | battle Lua 121000 | data loaded |
| c1211 | "Cricket dungeon" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1211.json) | - | - | battle Lua 121100 | data loaded |
| c1212 | ? |  | CAB | - | - | - | yes | - | - | - | not placed by any map (data: chr+hks) | none |
| c1219 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1220 | "Temple foundation Zako Temple fist Hoi only Kasaya" (param row name) |  | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c1220.json); named in combat.rs, data.rs, sound.rs | 1: a_clips_anim_set_overwrites_the_resident_gate | - | battle Lua 122000,122010 | rules ported |
| c1240 | "Shamo chicken_sick village" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1240.json) | - | - | battle Lua 124000 | data loaded |
| c1250 | "Monkey Valley Bare hands No sanding No foot effect" (param row name) |  | CABT | yes | yes | 4/4 goals | yes | generic (enemies/c1250.json); named in combat.rs, enemy.rs, enemy_bullet.rs, prosthetic.rs | 3: a_guarded_hit_without_a_guard_clip_takes_its_damage_reaction, a_spreading_bullet_catches_wolf_beside_its_path, ... | AI profile vs 6 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 12501099; recorded live in 1; 1 kb/src lines cite a recording | in fight: Folding Screen Monkeys; battle Lua 125000,125010,125020,125030; kb: recordings.md | seen side by side |
| c1259 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1260 | "Chigo monkey_monkey" (param row name) |  | CAT | yes | via c1250 | 1/1 goals | via c1250 | generic (enemies/c1260.json); named in enemy.rs, sound.rs | 2: a_clips_anim_set_overwrites_the_resident_gate, a_shuriken_takes_the_light_shoot_rates | pressure replay rows 12600010; recorded live in 1; 1 kb/src lines cite a recording | in fight: Folding Screen Monkeys; battle Lua 126000; kb: recordings.md, visuals.md | seen side by side |
| c1300 | Mist Noble | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1300.json); named in combat.rs, player.rs, status.rs | 4: an_aged_wolf_jumps, an_aged_wolf_plays_the_aged_form, ... | reference clip | arena boss_71001000; battle Lua 130000; kb: skills.md | rules ported |
| c1310 | Okami Leader Shizu | mini-boss | CABT | yes | yes | 4/4 goals | yes | generic (enemies/c1310.json); named in combat.rs | 1: the_sabimaru_poisons_an_okami_warrior_into_her_reaction | reference clip | arena boss_71001000; battle Lua 131000,131010,131020,131030; kb: damage.md, visuals.md | rules ported |
| c1320 | "Piranha treasure carp samurai residence" (param row name) |  | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c1320.json) | - | - | battle Lua 132000,132010; kb: enemy.md, visuals.md | data loaded |
| c1321 | "Pot Noble Harunaga" (param row name) |  | CT | yes | via c1320 | no battle goal | via c1320 | generic (enemies/c1321.json) | - | - | kb: visuals.md | data loaded |
| c1340 | Headless | mini-boss | CT | yes | via c1350 | 1/1 goals | via c1350 | generic (enemies/c1340.json); named in data.rs | - | reference clip | arena boss_71001000; battle Lua 134000; kb: enemy.md, visuals.md | data loaded |
| c1350 | Headless | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1350.json) | - | reference clip | arena boss_71001000; battle Lua 135000; kb: enemy.md, visuals.md | data loaded |
| c1360 | "Rappa Temple Normal Appearance 1" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1360.json); named in actor.rs, combat.rs, enemy.rs | 2: a_grab_pair_is_not_pushed_apart, a_guarded_hit_without_a_guard_clip_takes_its_damage_reaction | - | battle Lua 136000 | rules ported |
| c1361 | "Tokugawa Rappa_Castle" (param row name) |  | CT | yes | via c1360 | 1/1 goals | via c1360 | generic (enemies/c1361.json) | - | - | battle Lua 136000 | data loaded |
| c1370 | Blazing Bull | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1370.json); named in sound.rs | - | reference clip | arena boss_71001000; battle Lua 137000; kb: grapple.md | data loaded |
| c1380 | Sakura Bull of the Palace | mini-boss | CT | yes | via c1370 | 1/1 goals | via c1370 | generic (enemies/c1380.json) | - | reference clip | arena boss_71001000; battle Lua 137000 | data loaded |
| c1400 | Ashina Elite - Jinsuke Saze; Ashina Elite - Ujinari Mizuo | mini-boss | CABT | yes | yes | 3/3 goals | yes | generic (enemies/c1400.json) | - | reference clip | arena boss_71001000; battle Lua 140000,140010,140020 | data loaded |
| c1450 | "Nightjars Revenge Spirit Proximity" (param row name) |  | CAB | yes | yes | 2/2 goals | yes | generic (enemies/c1450.json) | - | enemy replay csv in 2 recs; recorded live in 2 | battle Lua 145000,145010; kb: damage.md, enemy.md, guard-deflect.md, sound.md, visuals.md | seen side by side |
| c1451 | ? |  | CAT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1459 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1460 | "Nightjars (kites) Honjo" (param row name) |  | CAB | yes | yes | 1/1 goals | yes | generic (enemies/c1460.json) | - | - | battle Lua 011000 | data loaded |
| c1470 | Lone Shadow Vilehand; Lone Shadow Masanaga the Spear-Bearer; Lone Shadow Longswordsman | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c1470.json); named in combat.rs | 1: flowing_water_cuts_guarded_posture_damage_by_attack_attribute | reference clip; 1 kb/src lines cite a recording | arena boss_71001000; battle Lua 147000; kb: damage.md, enemy.md, guard-deflect.md | rules ported |
| c1500 | "Villager Zombie _ Bare Hand _ Grab Restraint" (param row name) |  | CABT | yes | yes | 6/6 goals | yes | generic (enemies/c1500.json); named in actor.rs, ai.rs, combat.rs, data.rs +6 | 7: a_zombie_rising_under_flag_96_keeps_1_hp, enemy_grabs_go_through_the_throw_kind_table, ... | - | battle Lua 150000,150010,150020,150030 +2; kb: enemy.md | rules ported |
| c1501 | "Villager Zombie (female) _ kitchen knife" (param row name) |  | CT | yes | via c1500 | 4/4 goals | via c1500 | generic (enemies/c1501.json) | - | - | battle Lua 150000,150010,150020,150030 | data loaded |
| c1510 | "Hallucinatory character _ villager zombie _ kitchen knife" (param row name) |  | CT | yes | via c1500 | 3/3 goals | via c1500 | generic (enemies/c1510.json) | - | AI profile vs 6 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 15100200 | in fight: Lady Butterfly; battle Lua 151020,151030,151040 | seen side by side |
| c1520 | "Prisoner Zombie_Dungeon" (param row name) |  | CT | yes | via c1500 | 1/1 goals | via c1500 | generic (enemies/c1520.json); named in fmod.rs | - | - | battle Lua 150060 | data loaded |
| c1550 | "Bandit_Torches Right side of light gauntlet" (param row name) |  | CABT | yes | yes | 5/5 goals | yes | generic (enemies/c1550.json); named in sound.rs | 4: enemy_bullets_carry_their_effects, reigniting_a_burn_refreshes_it, ... | pressure replay rows 15500200 | battle Lua 155000,155010,155020,155030 +1 | seen side by side |
| c1559 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c1700 | "Tokugawa Samurai Castle_Tokugawa Invasion (for Ochimusha Hostile) Initial Equipment_Sword" (param row name) |  | CABT | yes | yes | 4/4 goals | yes | generic (enemies/c1700.json); named in combat.rs, fxr.rs, sound.rs | 3: a_second_burn_row_replaces_the_first, both_blades_of_a_two_blade_swing_are_deflected, ... | AI profile vs 21 recs (enemies_all.md); enemy replay csv in 7 recs; pressure replay rows 17002010; recorded live in 1 | battle Lua 170000,170010,170020,170030; kb: enemy.md, visuals.md | seen side by side |
| c2021 | ? |  | B | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5000 | Corrupted Monk | boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5000.json); named in cloth.rs, data.rs, enemy.rs | 7: monk_phantoms_come_and_go, monk_phantoms_fade_in_and_out, ... | AI profile vs 2 recs (enemies_all.md); enemy replay csv in 2 recs; pressure replay rows 50000000; recorded live in 2; reference clip | in fight: Corrupted Monk; arena boss_50000000, boss_50001000; battle Lua 500000; kb: damage.md, enemy.md, visuals.md | seen side by side |
| c5001 | "Buddhist monk's bug submerged city" (param row name) |  | CABT | yes | yes | no battle goal | yes | generic (enemies/c5001.json) | - | - | in fight: Corrupted Monk; Guardian Ape | data loaded |
| c5005 | "The Phantom of the Haunting Monk Submerged City" (param row name) |  | CT | yes | via c5000 | 1/1 goals | via c5000 | generic (enemies/c5005.json); named in cloth.rs, data.rs | 2: monk_phantoms_come_and_go, monk_phantoms_fade_in_and_out | AI profile vs 3 recs (enemies_all.md); enemy replay csv in 1 recs | in fight: Corrupted Monk; battle Lua 500005; kb: enemy.md, visuals.md | seen side by side |
| c5010 | "The Great Serpent Castle in the Valley" (param row name) |  | CABT | yes | yes | 3/3 goals | yes | generic (enemies/c5010.json) | - | - | battle Lua 501010,501020,501030; kb: enemy.md, visuals.md | data loaded |
| c5020 | Chained Ogre | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5020.json); named in cloth.rs | 3: the_grappling_hook_attack_throws_the_ogre, the_hook_in_the_ogres_grab_staggers_him, ... | reference clip | arena boss_71001000; battle Lua 502000; kb: clips.md, grapple.md, visuals.md | rules ported |
| c5021 | "Giants Innsmouth Submerged City [Conversation]" (param row name) |  | CT | yes | via c5020 | 1/1 goals | via c5020 | generic (enemies/c5021.json); named in cloth.rs | - | - | battle Lua 502000; kb: visuals.md | data loaded |
| c5040 | "Straw doll (new model)" (param row name) |  | CABT | yes | yes | no battle goal | yes | generic (enemies/c5040.json) | 1: a_level_0_hit_leaves_the_doll_still_and_wolf_recoils | - | kb: sound.md, visuals.md | rules ported |
| c5050 | "Nishikigoi Submerged City" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5050.json); named in cloth.rs | - | - | battle Lua 505000; kb: map.md, visuals.md | data loaded |
| c5060 | Great Shinobi - Owl; Owl (Father) | boss | CAB | yes | yes | 3/3 goals | yes | generic (enemies/c5060.json); named in cloth.rs, combat.rs, data.rs | 4: a_vanish_cut_short_comes_back_visible, the_owl_father_talks_before_the_fight, ... | AI profile vs 2 recs (enemies_all.md); enemy replay csv in 2 recs; pressure replay rows 50601010; 3 ref_frames shots; recorded live in 2; reference clip | in fight: Great Shinobi - Owl; Owl (Father); arena boss_50601010, boss_71001000; battle Lua 506000,506001,506300; kb: enemy.md, grapple.md, sound.md, visuals.md | seen side by side |
| c5061 | ? |  | CABT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5063 | ? |  | AB | - | - | - | yes | - | - | - | not placed by any map (data: chr+hks) | none |
| c5069 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr); kb: visuals.md | none |
| c5070 | "Chief Owl Honjo_When the Ninja Army invades" (param row name) |  | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c5070.json) | - | AI profile vs 1 recs (enemies_all.md); enemy replay csv in 1 recs | in fight: Owl (Father); battle Lua 507000,507300 | seen side by side |
| c5080 | Gyoubu Oniwa | boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5080.json); named in actor.rs, ai.rs, combat.rs, prosthetic.rs | 4: a_thrust_without_a_mikiri_row_costs_no_posture, gyoubus_defeat_waits_for_the_ninsatsu_hit, ... | AI profile vs 3 recs (enemies_all.md); enemy replay csv in 3 recs; pressure replay rows 50800000; recorded live in 3; reference clip; 3 kb/src lines cite a recording | in fight: Gyoubu Oniwa; arena boss_50800000; battle Lua 508000; kb: grapple.md, recordings.md | seen side by side |
| c5089 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5090 | Lady Butterfly | boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5090.json); named in data.rs | 4: a_second_form_takes_its_hu_throw_rows, butterflys_second_body_alone_reaches_her_defeat, ... | AI profile vs 2 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 50900001; recorded live in 1; reference clip; 1 kb/src lines cite a recording | in fight: Lady Butterfly; arena boss_50900000; battle Lua 509000; kb: recordings.md, sound.md, visuals.md | seen side by side |
| c5100 | Guardian Ape | boss | CABT | yes | yes | 3/3 goals | yes | generic (enemies/c5100.json); named in data.rs | 2: encounters_carry_the_health_bar_names, the_guardian_ape_gets_up_headless_after_his_first_todome | AI profile vs 3 recs (enemies_all.md); enemy replay csv in 2 recs; pressure replay rows 51000000; recorded live in 2; reference clip; 1 kb/src lines cite a recording | in fight: Guardian Ape; arena boss_51000000, boss_51000100; battle Lua 510000,510001,510010; kb: enemy.md, grapple.md, recordings.md, visuals.md | seen side by side |
| c5200 | Divine Dragon | boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5200.json); named in data.rs | 6: dragon_probe, every_boss_row_reaches_its_defeat, ... | AI profile vs 1 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 52000000; recorded live in 1; reference clip | in fight: Divine Dragon; Old Dragons of the Tree; arena boss_52000000; battle Lua 520000; kb: enemy.md, grapple.md, visuals.md | seen side by side |
| c5209 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5300 | Old Dragons of the Tree | boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5300.json) | 5: every_boss_row_reaches_its_defeat, the_old_dragons_bar_is_the_scripted_bar_body, ... | recorded live in 1; reference clip | in fight: Old Dragons of the Tree; arena boss_53000000; battle Lua 530000; kb: enemy.md, map.md | rules ported |
| c5310 | "Tentacle_Mermaid Dragon Battle" (param row name) |  | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c5310.json); named in enemy.rs, enemy/grapple.rs, enemy/script.rs | 2: the_divine_dragons_trees_open_grapple_points_when_they_bloom, the_divine_dragons_trees_stand_on_its_dummies | AI profile vs 3 recs (enemies_all.md); enemy replay csv in 1 recs; recorded live in 1 | in fight: Divine Dragon; Old Dragons of the Tree; battle Lua 011000,531000; kb: grapple.md | seen side by side |
| c5320 | "Okiryu (Large)" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5320.json) | - | - | battle Lua 532000; kb: grapple.md | data loaded |
| c5400 | Isshin, the Sword Saint | boss | CAB | yes | yes | 2/2 goals | yes | generic (enemies/c5400.json); named in data.rs | 12: a_second_form_takes_its_hu_throw_rows, a_super_armour_bonus_grows_the_pool, ... | AI profile vs 1 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 54000000; recorded live in 2; reference clip | in fight: Genichiro, Way of Tomoe; Isshin, the Sword Saint; arena boss_54000000; battle Lua 540000,540300; kb: visuals.md | seen side by side |
| c5401 | ? |  | AB | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5403 | ? |  | AB | - | - | - | yes | - | - | - | not placed by any map (data: chr+hks) | none |
| c5408 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5409 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5410 | "Isshin Ashina" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c5410.json) | - | - | battle Lua 011000; kb: visuals.md | data loaded |
| c5420 | ? |  | C | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c5430 | Isshin Ashina | boss | C | yes | via c5400 | 2/2 goals | via c5400 | generic (enemies/c5430.json); named in data.rs | 2: isshin_ashina_follows_emma, isshin_ashinas_volley_fires_from_its_regions | AI profile vs 1 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 54300000; recorded live in 1; reference clip | in fight: Emma, the Gentle Blade; Isshin Ashina; arena boss_71001000; battle Lua 011000,543000; kb: visuals.md | seen side by side |
| c5440 | ? |  | CT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7000 | O'Rin of the Water | mini-boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c7000.json) | - | reference clip | arena boss_71001000; battle Lua 700000 | data loaded |
| c7010 | "Sculptor" (param row name) |  | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c7010.json) | - | - | battle Lua 701000; kb: visuals.md | data loaded |
| c7011 | ? |  | CA | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7019 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7020 | Demon of Hatred | boss | CABT | yes | yes | 1/1 goals | yes | generic (enemies/c7020.json); named in data.rs, enemy.rs | 6: a_big_enemy_reaches_drill_range, a_bullet_burns_once_not_through_its_atk_param_too, ... | AI profile vs 1 recs (enemies_all.md); reactions under pressure (enemies_pressure.md); enemy replay csv in 1 recs; pressure replay rows 70200000; recorded live in 1; reference clip | in fight: Demon of Hatred; arena boss_70200000; battle Lua 702000; kb: enemy.md, grapple.md, map.md, visuals.md | seen side by side |
| c7021 | "Dummy for demon Buddha" (param row name) |  | CAB | yes | yes | 1/1 goals | yes | generic (enemies/c7021.json) | - | - | battle Lua 702100; kb: damage.md | data loaded |
| c7100 | Genichiro Ashina | boss | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c7100.json); named in enemy.rs, enemy_bullet.rs, menu.rs, sound.rs | 8: a_boss_refills_at_its_reactions_hp_reset_frame, a_boss_takes_one_deathblow_per_red_dot, ... | AI profile vs 5 recs (enemies_all.md); enemy replay csv in 5 recs; pressure replay rows 71001000; recorded live in 5; reference clip; 1 kb/src lines cite a recording | in fight: Genichiro Ashina; arena boss_71000000, boss_71001000; battle Lua 710000,710300; kb: damage.md, enemy.md, recordings.md, sound.md, visuals.md | seen side by side |
| c7101 | ? |  | CABT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7102 | ? |  | C | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7103 | ? |  | AB | - | - | - | yes | - | - | - | not placed by any map (data: chr+hks) | none |
| c7109 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7110 | Genichiro, Way of Tomoe | boss | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c7110.json); named in data.rs, debug_menu.rs, enemy_bullet.rs | 14: a_lightning_arrow_shocks_wolf_on_the_ground, castle_genichiros_last_dot_waits_for_the_hit, ... | AI profile vs 3 recs (enemies_all.md); enemy replay csv in 3 recs; pressure replay rows 71101000; recorded live in 1; reference clip | in fight: Genichiro Ashina; Genichiro, Way of Tomoe; arena boss_71001000, boss_71101000; battle Lua 711000,711300; kb: enemy.md, visuals.md | seen side by side |
| c7111 | ? |  | CA | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7200 | "Kuro the Divine Heir" (param row name) |  | CAB | yes | yes | 3/3 goals | yes | generic (enemies/c7200.json) | - | - | battle Lua 011000,101000,720000; kb: visuals.md | data loaded |
| c7201 | ? |  | CAT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7202 | ? |  | CT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7209 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7210 | "Prince childhood" (param row name) |  | CABT | yes | yes | no battle goal | yes | generic (enemies/c7210.json) | - | - | kb: damage.md, visuals.md | data loaded |
| c7300 | "Divine Child of Rejuvenation" (param row name) |  | CAB | yes | yes | 1/1 goals | yes | generic (enemies/c7300.json) | - | - | battle Lua 011000; kb: enemy.md | data loaded |
| c7301 | ? |  | CT | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7309 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7310 | ? |  | C | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7400 | Emma, the Gentle Blade | boss | CABT | yes | yes | 2/2 goals | yes | generic (enemies/c7400.json); named in anim.rs, cloth.rs, enemy.rs, model.rs | 3: emma_killed_from_her_deflect_break_does_not_get_up, every_boss_row_reaches_its_defeat, ... | AI profile vs 1 recs (enemies_all.md); enemy replay csv in 1 recs; pressure replay rows 74000010; recorded live in 1; reference clip | in fight: Emma, the Gentle Blade; arena boss_71001000; battle Lua 011000,740000; kb: enemy.md, visuals.md | seen side by side |
| c7401 | "Isei's daughter for cutscenes" (param row name) |  | CAT | yes | via c7400 | no battle goal | via c7400 | generic (enemies/c7401.json) | - | - | kb: damage.md | data loaded |
| c7409 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7410 | "Fascinated Samurai Hidden Prison [Conversation] (Base c1020)" (param row name) |  | CT | yes | via c1020 | 1/1 goals | via c1020 | generic (enemies/c7410.json); named in combat.rs | 1: a_second_form_takes_its_hu_throw_rows | - | battle Lua 102000 | rules ported |
| c7420 | "Anayama the Peddler" (param row name) |  | CT | yes | via c1550 | 1/1 goals | via c1550 | generic (enemies/c7420.json) | - | - | battle Lua 155010 | data loaded |
| c7430 | "Fujioka the Info Broker" (param row name) |  | CT | yes | via c1550 | 1/1 goals | via c1550 | generic (enemies/c7430.json) | - | - | battle Lua 112000 | data loaded |
| c7439 | ? |  | T | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7440 | "Yaohioka Nun Honjo [Conversation] (Base c1110)" (param row name) |  | CT | yes | via c1110 | 1/1 goals | via c1110 | generic (enemies/c7440.json) | - | - | battle Lua 111000; kb: visuals.md | data loaded |
| c7450 | "Suzubaa Castle [Conversation] (Base c1110)" (param row name) |  | CT | yes | via c1110 | 2/2 goals | via c1110 | generic (enemies/c7450.json) | - | - | battle Lua 011000,111000 | data loaded |
| c7460 | "Suzuba's son Castle [Conversation] (Base c1400)" (param row name) |  | CT | yes | via c1400 | 1/1 goals | via c1400 | generic (enemies/c7460.json) | - | - | battle Lua 140000 | data loaded |
| c7470 | "The kidnapped lower man temple [conversation] (base c1180)" (param row name) |  | CT | yes | via c1180 | 1/1 goals | via c1180 | generic (enemies/c7470.json) | - | - | battle Lua 118020; kb: visuals.md | data loaded |
| c7480 | "Sick Village Kannushi_Before Innsmouth [Conversation] (Base c1400)" (param row name) |  | CT | yes | via c1400 | 1/1 goals | via c1400 | generic (enemies/c7480.json) | - | - | battle Lua 140000 | data loaded |
| c7490 | "Immortal Experiment Doctor_Dungeon [Conversation] (Base c1220)" (param row name) |  | CT | yes | via c1220 | 1/1 goals | via c1220 | generic (enemies/c7490.json) | - | - | battle Lua 122000 | data loaded |
| c7500 | "Basket Wearer Shosuke" (param row name) |  | CT | yes | via c1500 | 2/2 goals | via c1500 | generic (enemies/c7500.json) | 1: a_clips_anim_set_overwrites_the_resident_gate | - | battle Lua 150000,150010 | rules ported |
| c7510 | "Blackhat Badger" (param row name) |  | CT | yes | via c1360 | 1/1 goals | via c1360 | generic (enemies/c7510.json) | - | - | battle Lua 136000 | data loaded |
| c7520 | "Submerged Handmaiden Aunt_Sister [Conversation] (Base c1110)" (param row name) |  | CT | yes | via c1110 | no battle goal | via c1110 | generic (enemies/c7520.json) | - | - | kb: visuals.md | data loaded |
| c7530 | "Shura Handmaiden Aunt_Ninja Army Invasion [Conversation] (Base c1110)" (param row name) |  | CT | yes | via c1110 | no battle goal | via c1110 | generic (enemies/c7530.json); named in fmod.rs | - | - | kb: visuals.md | data loaded |
| c7540 | "Toxic Memorial Mob" (param row name) |  | CT | yes | via c1120 | 1/1 goals | via c1120 | generic (enemies/c7540.json) | - | - | battle Lua 112000; kb: visuals.md | data loaded |
| c7550 | "Shugendo Memorial Mob" (param row name) |  | CT | yes | via c1120 | 1/1 goals | via c1120 | generic (enemies/c7550.json) | - | - | battle Lua 112000 | data loaded |
| c7560 | "Dungeon Memorial Mob" (param row name) |  | CT | yes | via c1120 | 1/1 goals | via c1120 | generic (enemies/c7560.json) | - | - | battle Lua 112000 | data loaded |
| c7580 | ? |  | C | - | - | - | - | - | - | - | not placed by any map (data: chr) | none |
| c7590 | "Exiled Memorial Mob" (param row name) |  | CT | yes | via c1120 | 1/1 goals | via c1120 | generic (enemies/c7590.json) | - | - | battle Lua 112000 | data loaded |
| c7600 | "Crow's Bed Memorial Mob" (param row name) |  | CT | yes | via c1120 | 1/1 goals | via c1120 | generic (enemies/c7600.json) | - | - | battle Lua 112000; kb: visuals.md | data loaded |
| c9000 | ? |  | - | - | - | - | yes | - | - | - | not placed by any map (data: hks) | none |
| c9520 | ? |  | - | - | - | - | yes | - | - | - | not placed by any map (data: hks) | none |
| c9800 | ? |  | - | - | - | - | yes | - | - | - | not placed by any map (data: hks) | none |
| c9820 | ? |  | - | - | - | - | yes | - | - | - | not placed by any map (data: hks) | none |
| c9830 | ? |  | - | - | - | - | yes | - | - | - | not placed by any map (data: hks) | none |
| c9997 | ? |  | - | - | - | - | yes | named in ai.rs, anim.rs, combat.rs, data.rs +5 | 2: a_boss_takes_one_deathblow_per_red_dot, emma_killed_from_her_deflect_break_does_not_get_up | - | shared NPC HKS script, not a character (enemy.rs ports its guard and turn parts); its HKS cited in 5 files (ai.rs, combat.rs, enemy.rs, ...); not placed by any map (data: hks); kb: damage.md, enemy.md, grapple.md, guard-deflect.md, skills.md, sound.md, visuals.md | rules ported |

## Systems

| System | Sv1 files (lines) | kb note | Emulator-checked | Other tests | Side by side | State | Proof level |
|---|---|---|---|---|---|---|---|
| Player movement | player.rs (5397), actor.rs (1264) | movement.md, input-buffer.md | wolf_turn_matches_the_exe; pilot 7 | 102 (5 unit, 97 sim) | Wolf replay vs the game, 16 listed events (W_AddActionInputJump, W_AirKick, W_GroundJumpLandReady, ...): 1608/1789 matched; 8 recordings with category movement | HKS c0000 states hand-ported in player.rs; jump arc and fall control from the exe; turning from the stick checked on 440 emulator frames (2026-11-03). | seen side by side |
| Combat rules (guard, deflect, damage, posture, hit stop) | combat.rs (2643), actor.rs (1264), status.rs (890) | guard-deflect.md, damage.md, attacks.md | hit_hp_matches_the_exe, hit_posture_matches_the_exe, posture_regen_matches_the_exe, super_armor_hit_matches_the_exe; pilot 1, 2, 4, 6; cases without a test yet: damage_survey, hit_stop | 231 (12 unit, 219 sim) | Wolf replay vs the game, 9 listed events (W_DeflectGuardToStand, W_GroundAttackCombo1, W_GroundAttackCombo1Release, ...): 1426/1485 matched; 46 recordings with category combat | Params + decompile; super armour hit, the hit-stop table, HP and posture amounts, posture regen and the guard/deflect outcome checked in the emulator (2026-11-03). | seen side by side |
| Camera | camera.rs (1606) | camera.md | follow_camera_at_walls_matches_the_exe, follow_camera_matches_the_exe, lock_camera_matches_the_exe; pilot 3, 3b; cases without a test yet: taecam_frames | 21 (5 unit, 16 sim) | 4 recordings with category camera | Follow camera checked on 1,626 emulator frames, walls and ground on 2,225 (2026-11-03); lock-on on 1,485; TAE camera under work. | checked against the exe |
| Enemy AI (C++ side) | enemy.rs (3353), stealth.rs (529), duel.rs (159) | enemy.md, frame-coverage.md | cases without a test yet: trace_enemy_frame, trace_enemy_frame_ai | 89 (8 unit, 81 sim) | reactions under pressure for 1 kind (enemies_pressure.md); 40 recordings with category bosses/enemies | Targeting and the native goals reimplemented; the AI frame (656 functions) cited in part only (frame-coverage.md). | seen side by side |
| Enemy AI (Lua goals) | ai.rs (2945), ai/lua50.rs (950) | enemy.md | - | 29 (23 unit, 6 sim) | attack picks by distance vs the game for 22 kinds (enemies_all.md); 40 recordings with category bosses/enemies | The game's own AI bytecode runs unchanged in a Lua 5.0 VM; engine API partly stubbed (ai_stubs). | seen side by side |
| HKS | player.rs (5397), enemy.rs (3353) | behaviour-graph.md | hks_env_matches_the_exe; pilot 5 | 9 (7 unit, 2 sim) | Wolf replay vs the game, all HKS events of 27 recs: 3974/4376 matched; 51 recordings with category combat/movement | No HKS runtime: c0000 rules hand-ported into player.rs, parts of c9997 into enemy.rs; 22 of 82 env ids checked against the exe on idle Wolf (hks_env.rs, 2026-11-03). | seen side by side |
| TAE events | data.rs (2375), anim.rs (871) | attacks.md | - | 13 (7 unit, 6 sim) | - | TAE events ported by id; the exe's per-frame event window rules not checked (frame-coverage.md). | rules ported |
| Behaviour graph | anim.rs (871), actor.rs (1264) | behaviour-graph.md | - | 19 (2 unit, 17 sim) | - | Only loop flags and selector reuse read from c0000.hkx; state machines, layers and blends by hand. | rules ported |
| VFX (FXR) | fxr.rs (2585), ffx.rs (943), vfx.rs (376), gore.rs (335), fx_distort.wgsl (108), fx_multi.wgsl (148), fx_soft.wgsl (42) | fxr.md | - | 35 (16 unit, 19 sim) | 18 recordings with category effects | TAE effects play the game's own FXR files through Sv1's FXR player; ffx.rs is the fallback for ids with no FXR. | rules ported |
| Sound | sound.rs (800), fmod.rs (1748), area_music.rs (265) | sound.md | - | 25 (11 unit, 14 sim) | 16 recordings with category sound | The game's FSB banks decoded and played from TAE / param lookups; area and boss music from the EMEVD tables. | rules ported |
| Maps / collision | map.rs (3068), fog_wall.rs (236), world.rs (117) | map.md | - | 28 (17 unit, 11 sim) | - | Boss arenas and a few map blocks exported with hit collision, fog gates and grapple edges; not the whole world. | rules ported |
| Rendering / shaders / lighting | model.rs (1078), cloth.rs (1528), sky.rs (287), height_fog.rs (240), grading.rs (137), sekiro_material.wgsl (357), map_material.wgsl (760), sky.wgsl (141), height_fog.wgsl (145), grading_lut.wgsl (57) | visuals.md, sky_shader.md | - | 20 (14 unit, 6 sim) | 33 ref_frames stills (real vs Sv1, visuals.md) | Game shaders decoded statically and ported per material family; cloth solver read from the exe; still shots against real frames. | seen side by side |
| Event scripts (EMEVD) | emevd.rs (144), enemy/script.rs (1575), enemy/events.rs (95) | none | - | 32 (2 unit, 30 sim) | 15 recordings with category script | Boss EMEVD events run by an interpreter (enemy/script.rs); the world map scripts are not run. | rules ported |
| Menus / HUD / saves | menu.rs (1063), hud.rs (1372), debug_menu.rs (779), icon.rs (56), icon_dot.rs (28) | none | - | 14 (9 unit, 5 sim) | - | Title / pause / death / won screens, posture and HP bars, debug menu; no game menus, no saves. | rules ported |
| Cutscenes | - | none | - | 0 (0 unit, 0 sim) | - | Not ported. | none |
| NPC quests / dialogue | enemy/talk.rs (166) | none | - | 1 (0 unit, 1 sim) | - | One pre-fight talk (Owl Father, t100121); no quests, no other dialogue. | rules ported |
| Prosthetics | prosthetic.rs (1442), model/tool.rs (436), enemy_bullet.rs (650) | skills.md | - | 72 (2 unit, 70 sim) | Wolf replay vs the game, all HKS events of 1 recs with category skills: 259/308 matched; 5 recordings with category items/skills | Every tool starts and costs its emblems; bullets from the Bullet params. | seen side by side |
| Skills | status.rs (890), prosthetic.rs (1442) | skills.md | - | 35 (3 unit, 32 sim) | Wolf replay vs the game, all HKS events of 1 recs with category skills: 259/308 matched; 4 recordings with category skills | Combat arts and latent skills ported; arts cost emblems and unlock their moves. | seen side by side |
| Grapple | wire.rs (453), enemy/grapple.rs (140) | grapple.md | - | 13 (1 unit, 12 sim) | 8 recordings with category movement | Points, shoot, reel, launch and landing ported (wire.rs, map.rs grapple_target). | rules ported |
| Stealth | stealth.rs (529) | enemy.md | - | 19 (6 unit, 13 sim) | 23 recordings with category enemies | Perception and target states ported from the exe's SprjTargetingSystem (stealth.rs). | rules ported |
| Docking (wall hug, ledge hang) | docking.rs (51) | docking.md | - | 1 (0 unit, 1 sim) | - | docking.rs exists; kb/docking.md (2026-10-11) calls it parked, not built. | rules ported |
