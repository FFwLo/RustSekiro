# Boss and mini-boss handover (2026-10-12)

The user gave every boss and mini-boss row in `extracted/enemies/npc_names.json` to an outside
contributor ("exclude these bosses name list From the work because i gave it to someone").
This sheet says where each of those 57 fights stands in Sv1 today. The Claude sessions keep
the regular enemies (also when they share a chr with a boss) and the shared engine; see
docs/HANDOFF.md section 0.1.

Written from a read-only sweep of the docs, data and code (eight readers, one per group, and a
completeness check); every claim cites file:line as of commit 575e910. "Status" is the
reader's verdict; "untested" means no test or harness runs that NpcParam row.

## How fights are set up

- **Boss-script fights** (20 rows, `tools/boss_scripts.py`: a 2003[11] boss bar in the map's
  events) run their own EMEVD events (`src/enemy/script.rs`) and have an arena in
  `extracted/map_boss_arenas.json` (own, shared or moved; `tools/boss_arenas.py`).
- **Mini-boss rows** (2003[87] mini-boss bar, common event 20005330) have no script and no
  arena: they fight on the config `[world] map` (the gate) or the flat floor. `all_enemies_full`
  only runs each chr's roster default row and the boss-script rows, so most of them are untested.

## Summary

| Row | Name | chr | Game map | Status | Setup |
|---|---|---|---|---|---|
| 10200000 | General Naomori Kawarada | c1020 | m11_00_00_00 | partial (untested) | Plain duel. |
| 10200010 | General Tenzen Yamauchi | c1020 | m11_00_00_00 | partial | Same as 10200000. |
| 10201000 | Leader Shigenori Yamauchi | c1020 | m11_02_00_00 | partial (untested) | No arena (no map_boss_arenas.json key), so the fight falls back to config [world] map (map.rs:94-106,1249). |
| 10202000 | General Kuranosuke Matsumoto | c1020 | m11_01_00_00 | partial (untested) | No arena key, so the fight falls back to config [world] map (map.rs:94-106,1249). |
| 10210000 | Seven Ashina Spears - Shikibu Toshikatsu Yamauchi | c1021, c1020 | m11_02_00_00 | partial | No arena key, so the fight falls back to config [world] map (map.rs:94-106,1249). |
| 10211000 | Seven Ashina Spears - Shume Masaji Oniwa | c1021, c1020 | m11_02_00_00 | partial (untested) | Same as 10210000: no arena (fallback to config map, map.rs:94-106,1249), no boss script, no phase events, no cast. |
| 10400000 | Long-arm Centipede Sen'un | c1040 | m20_00_00_00 | partial | Arena: none. |
| 10401000 | Long-arm Centipede Giraffe | c1040 | m17_00_00_00 | partial (untested) | Same as 10400000. |
| 10500001 | Shinobi Hunter Enshin of Misen | c1050 | m10_00_00_00 | partial | Arena: none. |
| 11300000 | Armored Warrior | c1130 | m20_00_00_00 | partial | Arena: none. |
| 11910000 | Snake Eyes Shirafuji | c1191, c1190 | m17_00_00_00 | partial (untested) | Arena: none. |
| 11911000 | Snake Eyes Shirahagi | c1191, c1190 | m17_00_00_00 | partial (untested) | Same as 11910000: no arena (config map), no boss script, no phase events, no extra cast, no music. |
| 10700000 | Juzou the Drunkard | c1070 | m10_00_00_00 | partial | Plain one-enemy duel. |
| 10701000 | Shigekichi of the Red Guard | c1070 | m11_00_00_00 | unknown | Same plain duel as 10700000: config map m11_01_00_00, no own or shared arena (map.rs:120-129,94-105), no boss script, no phase events, no cast, no boss music (a |
| 10703000 | Tokujiro the Glutton | c1070 | m15_00_00_00 | unknown | Same plain duel on config map m11_01_00_00. |
| 10704000 | Juzou the Drunkard (c1070, second Hirata visit) | c1070 | m10_00_00_00 | unknown | The same boss as 10700000 in the 'Truth' version of Hirata Estate. |
| 10800000 | Shichimen Warrior (Abandoned Dungeon) | c1080 | m13_00_00_00 | partial | Plain one-enemy duel on config map m11_01_00_00. |
| 10801000 | Shichimen Warrior (Fountainhead Palace) | c1080 | m25_00_00_00 | unknown | Same plain duel on config map m11_01_00_00. |
| 10802000 | Shichimen Warrior (Sunken Valley) | c1080 | m17_00_00_00 | unknown | Same plain duel on config map m11_01_00_00. |
| 13001000 | Mist Noble | c1300 | m15_00_00_00 | unknown | No own fight setup. |
| 13100310 | Okami Leader Shizu | c1310 | m25_00_00_00 | unknown | No boss script, no arena and no phase events, for the same reason as the Mist Noble: her bar is 2003[87] [1,2500406,0,911310] in m25 event 12505215, and boss_sc |
| 13400000 | Headless (underwater, Ashina Castle) | c1340, c1350 | m11_01_00_00 | partial | This is the c1340 default row (roster.json:4990), so the tests run it. |
| 13401000 | Headless (underwater, Fountainhead Palace) | c1340, c1350 | m25_00_00_00 | unknown | No boss script, no arena and no phase events (bar 2003[87] [1,2500550,0,911350] in common event 20005330). |
| 13500000 | Headless (Ashina Outskirts) | c1350 | m11_00_00_00 | partial | This is the c1350 default row (roster.json:5041), the one the tests run. |
| 13501000 | Headless (Sunken Valley) | c1350 | m17_00_00_00 | unknown | Same as 13500000: no boss script, no arena, no phase events, no extra cast (bar in common event 20005330; common events 20005331, 20005340 and 20005430-20005433 |
| 13502000 | Headless (Mibu Village) | c1350 | m15_00_00_00 | unknown | No boss script, no arena, no phase events. |
| 13700000 | Blazing Bull | c1370 | m11_01_00_00 | partial | Arena: none of its own. |
| 13800000 | Sakura Bull of the Palace | c1380 | m25_00_00_00 | partial | Arena: none of its own (no key in map_boss_arenas.json:1-29). |
| 14000200 | Ashina Elite - Jinsuke Saze | c1400 | m11_01_00_00 | unknown | Arena: none of its own (no key in map_boss_arenas.json). |
| 14004200 | Ashina Elite - Ujinari Mizuo | c1400 | m11_01_00_00 | unknown | Set up the same way as Jinsuke Saze 14000200: no arena of its own (it fights on the config's map, map.rs:103/1249), no boss script, no phase events, no cast. |
| 14701005 | Lone Shadow Vilehand | c1470 | m11_01_00_00 | unknown | Arena: none of its own. |
| 14701006 | Lone Shadow Masanaga the Spear-Bearer (Ashina Castle row) | c1470 | m11_01_00_00 | unknown | The same fight as 14703002 (both rows carry the bar name 'Lone Shadow Masanaga the Spear-Bearer', npc_names.json:46-47). |
| 14703002 | Lone Shadow Masanaga the Spear-Bearer (Hirata Estate row) | c1470 | m10_00_00_00 | unknown | Same boss as 14701006; the picker shows this row with '(Hirata Estate)' (data.rs:1470-1477). |
| 14704000 | Lone Shadow Longswordsman | c1470 | m11_02_00_00 | unknown | Arena: none of its own (no key in map_boss_arenas.json). |
| 12600050 | Folding Screen Monkeys | c1000, c1260, c1250 | m20 | broken | Own arena boss_12600050 (map_boss_arenas.json:2; OWN_ROWS tools/boss_arenas.py:67-69). |
| 50000000 | Corrupted Monk (Fountainhead Palace, with the Mibu phantoms) | c5000, c5005, c5001 | m25 | works | Own arena boss_50000000 (map_boss_arenas.json:3; OWN_ROWS tools/boss_arenas.py:59), map m25 (map_boss_50000000.json:1719). |
| 50001000 | Corrupted Monk (Mibu Village, the true Monk on the bridge) | c5000 | m15 | works | Own arena boss_50001000 (map_boss_arenas.json:4; OWN_ROWS tools/boss_arenas.py:59), map m15 (map_boss_50001000.json:1409). |
| 70000000 | O'Rin of the Water | c7000 | m15 | partial | No boss script (no 70000000 key in boss_scripts.json:1). |
| 50200010 | Chained Ogre (Ashina Castle, the 'Ninja Army Invasion (Indis | c5020 | m11_01 | partial | No boss script (no key in boss_scripts.json:1; exporter rule tools/boss_scripts.py:345) and no arena entry (map_boss_arenas.json has none), so the fight runs on |
| 50202000 | Chained Ogre (Ashina Outskirts, the 'Castle_VP (Red Eye)' pl | c5020 | m11_00 | unknown | No boss script (no key in boss_scripts.json:1; tools/boss_scripts.py:345) and no arena entry (map_boss_arenas.json), so it would run on the config map m11_01_00 |
| 50800000 | Gyoubu Oniwa | c5080 | m11_00 | works | Shared arena boss_54000000, Isshin's flower field m11_02 (map_boss_arenas.json:7; tools/boss_arenas.py:109-110; map_boss_54000000.json:5, :3330). |
| 70200000 | Demon of Hatred | c7020, c1001 | m11_00 | works | Shared arena boss_54000000, Isshin's flower field m11_02 (map_boss_arenas.json:16; tools/boss_arenas.py:109-110), with the stranger-arena rules (src/enemy.rs:87 |
| 50600000 | Great Shinobi - Owl (npc_names.json:53) | c5060 | m11_01_00_00 | works | Arena: MOVED onto Genichiro's castle top boss_71001000 (tools/boss_arenas.py:71-79, map_boss_arenas.json:5 and 22-23), because his own spot put the start outsid |
| 50601010 | Owl (Father) (npc_names.json:54) | c5060, c5070 | m10_00_00_00 | works | Arena: the shared flower field boss_54000000, because the row is not in OWN_ROWS (boss_arenas.py:58-70, 108-111; map_boss_arenas.json:6). |
| 50900000 | Lady Butterfly (npc_names.json:56) | c5090 | m10_00_00_00 | works | Arena: own, boss_50900000 (OWN_ROWS boss_arenas.py:60; map_boss_arenas.json:8). |
| 50900001 | Lady Butterfly, second body (npc_names.json:57) | c5090 | m10_00_00_00 | partial | Arena: shares boss_50900000 (map_boss_arenas.json:9; the sharing rule is boss_arenas.py:112-122). |
| 51000000 | Guardian Ape (npc_names.json:58) | c5100, c5001 | m17_00_00_00 | partial | Arena: own, boss_51000000, the valley's pond and grotto (OWN_ROWS boss_arenas.py:61; map_boss_arenas.json:10). |
| 51000100 | Guardian Ape, headless fight (npc_names.json:59) | c5100, c5001 | m17_00_00_00 | works | Arena: own, boss_51000100, exported turned 180 degrees (MAP_CENTRE_YAW, boss_arenas.py:40-42 and 53; HANDOFF.md:436-440; map_boss_arenas.json:11). |
| 52000000 | Divine Dragon (npc_names.json:60) | c5200, c5310 | m25_00_00_00 | works | Arena: own, boss_52000000, the cloud floor h003100 24 m ahead of the MSB spot (MAP_CENTRE_DIST, boss_arenas.py:43-44 and 53), with a 3000 m far view (boss_arena |
| 53000000 | Old Dragons of the Tree (npc_names.json:61) | c5300 | m25_00_00_00 | broken | Arena: own export boss_53000000 with the same settings as the Dragon (MAP_CENTRE_DIST 24, boss_arenas.py:53; map_boss_arenas.json:13). |
| 71000000 | Genichiro Ashina (prologue fight, Ashina Reservoir) | c7100 | m11_02_00_00 | partial | Arena: the shared silvergrass field boss_54000000 (extracted/map_boss_arenas.json:17). |
| 71001000 | Genichiro Ashina (Ashina Castle top) | c7100 | m11_01_00_00 | works | Arena: his own castle top boss_71001000 (extracted/map_boss_arenas.json:18). |
| 71100000 | Genichiro, Way of Tomoe (Ashina Castle) | c7110 | m11_01_00_00 | works | Normally brought in mid-fight by 71001000's script (cast part 1110801). |
| 71101000 | Genichiro, Way of Tomoe (Ashina Reservoir, opens the Sword S | c7110 | m11_02_00_00 | partial | Arena: moved onto Genichiro's castle top boss_71001000 (extracted/map_boss_arenas.json:20 and moved list :22-28; tools/boss_arenas.py:71-79). |
| 54000000 | Isshin, the Sword Saint | c5400 | m11_02_00_00 | works | Arena: moved onto the castle top boss_71001000 (extracted/map_boss_arenas.json:14 and moved :24; tools/boss_arenas.py:79), so he is a stranger there (warps skip |
| 54300000 | Isshin Ashina (Shura ending) | c5430 | m11_01_00_00 | partial | Arena: boss_71001000 through MOVED (extracted/map_boss_arenas.json:15 and moved :25; tools/boss_arenas.py:71-79). |
| 74000010 | Emma, the Gentle Blade (Shura ending) | c7400 | m11_01_00_00 | partial | Arena: boss_71001000 through MOVED (extracted/map_boss_arenas.json:21 and moved :27; tools/boss_arenas.py:71-79). |

## Per fight

### 10200000 General Naomori Kawarada

- **chr / map:** c1020, m11_00_00_00 (Ashina Outskirts: extracted/enemies/npc_names.json:4,14; extracted/enemies/roster.json:1412-1424). MSB entity 1100300 (model c1020); the mini-boss bar is 2003[87] Display Miniboss Health Bar, nameId 911021, from common event 20005330. I found this with a read-only query that runs tools/boss_events.py:65 msb_parts and tools/boss_scripts.py instances/names over extracted/event/m11_00_00_00.emevd and extracted/map/mapstudio/m11_00_00_00.msb.
- **Setup:** Plain duel. Sv1 has no special setup for this fight. Arena: none. map_boss_arenas.json:1-28 has no 10200000 key, so map.rs:112-135 boss_arena returns None and map_id falls back to config [world] map (map.rs:94-106, switch_map map.rs:1249). Today that map is m11_01_00_00 (config.toml:94), the gate cut around the regular General c1020_0004 / 10203010 (kb/map.md:7-8; extracted/map_m11_01_00_00.json centre). With no map set, the flat floor is used. On that map BOSS_ARENA is false (map.rs:1278): Wolf starts at (0,4) and the boss at (0,-4) (map.rs:168-170, enemy.rs:881-889), not at entity 1100300. Boss script: none. extracted/enemies/boss_scripts.json has 20 keys and no 102xxxxx row, because tools/boss_scripts.py:345 only takes 2003[11] (boss bar), and this row's bar is 2003[87]. Phase events: none (boss_events.json has no key for this row). Extra cast: none; config enemy.group can add enemies by hand (config.rs:296-297, enemy.rs:890-897). Ways to start it: config [enemy] chr="c1020" npc_row=10200000 (config.rs:292-297); the picker's Bosses tab (menu.rs:175-206; data.rs:1459-1460 gives boss=false, bar=true); or SHINOBI_FIGHT=c1020:10200000 (menu.rs:745-751).
- **Status:** partial
- **Tests:** No test loads row 10200000. all_enemies_smoke runs c1020 only on its roster default row 10202060 (sim_tests.rs:3450-3460; data.rs:867-878; roster.json:1398). all_enemies_full runs the roster default rows plus the boss-script rows only (sim_tests.rs:3724-3744). SHINOBI_ENEMIES=c1020:10200000 matches no case (sim_tests.rs:3748-3750). The c1020 default row passes: smoke is 102/102 and full is 95/110, and the 15 failures are all non-fighters (HANDOFF.md:539-541; docs/reports/enemies.md:137-148). The two-deathblow path is tested only on c7100 (sim_tests.rs:637-660, 5699).
- **Correction (completeness check):** partial. No test runs this row. all_enemies_full takes only each chr's roster default plus boss-script rows (src/sim_tests.rs:3720-3742). 10200000 has no boss script, and c1020's roster default is 10202060 (roster.json). That puts it on the same footing as the rows marked unknown (10701000, 10801000, 14701005 ...). Its only mentions are AI-think defaults (src/ai.rs:1575, 1632), not a fight check.
- **Done:**
  - NpcParam 10200000 loads as itself: HP 1273, posture 480, ninsatuNum 2, behaviorVariationId 10200, resident 241100 'Middle boss sound reaction' (the regular rows have 241000) (extracted/enemies/c1020.json params.NpcParam; roster.json:1412-1424).
  - Think row 10200020 comes from its placement (data.rs:879-884; roster.json:1423) -> battleGoal 102000. The script's Naomori-only branch runs: think 10200020 opens with Act03 and switches Act07/Act09 off (extracted/ai_src/102000_battle.lua:31-32,85,177), read through GetNpcThinkParamID (ai.rs:650).
  - Two deathblows. Enemy.ninsatsu comes from ninsatuNum (enemy.rs:1038-1041). The first deathblow plays the ThrowDef reaction, then Rising (enemy.rs:346-362). HP comes back at the reaction's SpEffect 300610 (c1020 a000_012000 frame 135; kb/enemy.md:191-197). GetNinsatsuNum returns the real count (ai.rs:646-649), so Goal.Parry's spin 3007 only fires at 1 bar or fewer (102000_battle.lua:1149).
  - The picker uses the game's bar name (npc_names.json:14; data.rs:1449-1460). The Won screen comes once the enemy is down, because the fight has no boss script (menu.rs:816-835).
- **Open:**
  - No arena. The fight runs on the regular General's gate (config.toml:94) or the flat floor. Naomori's real spot (m11_00 entity 1100300) was never exported, because tools/boss_arenas.py:106-111 only builds arenas for boss_scripts rows.
  - Map events are not exported or run: 20005330 (mini-boss bar), 20005331 (includes 2004[17] Request Character AI Command), 20005340, 11107648 (includes 2007[17] Show Tutorial Text). This comes from the EMEVD query above; the instruction names are from reference/DarkScript3 sekiro-common.emedf.json.
  - The row's own growthDopingSpEffectID is 7010. src/ never reads it: every enemy gets config enemy.area_doping (enemy.rs:1013-1022; config.toml:65-68).
  - The HUD has no mini-boss bar (name or red dots) during the fight, only the floating vitality and posture bars (hud.rs:356-416). There is no music either: boss_music needs a BossScript run plus a boss_bgm.json row (fmod.rs:690-694), and this row has neither.
  - Clip references are not compared yet: kb/clips.md:77 (finishers clip 0:10-0:15, stealth + finisher), :128 (all-bosses clip 4:48-5:22), :293 (hitbox showcase #49). kb/damage.md:252 still lists this comparison as to do.
  - No test for this row.
- **Key files:** `extracted/enemies/c1020.json`, `extracted/enemies/roster.json`, `extracted/ai_src/102000_battle.lua`, `extracted/event/m11_00_00_00.emevd`, `src/data.rs`, `src/enemy.rs`, `src/map.rs`, `tools/boss_scripts.py`, `tools/boss_arenas.py`

### 10200010 General Tenzen Yamauchi

- **chr / map:** c1020, m11_00_00_00 (Ashina Outskirts: npc_names.json:4,15; roster.json:1425-1437). MSB entity 1100301; bar 2003[87] nameId 911022 from common event 20005330 (same EMEVD/MSB query as for 10200000).
- **Setup:** Same as 10200000. No arena: map_boss_arenas.json has no key, so the fight falls back to config [world] map (map.rs:94-106,1249). No boss script, because the bar is 2003[87] (tools/boss_scripts.py:345). No phase events (boss_events.json). No extra cast. Start it with config npc_row=10200010; config.rs:292-297 already uses this row as the 'with haori' example, and the debug-menu fallback list names it 'castle, with haori' (debug_menu.rs:22). The picker Bosses tab and SHINOBI_FIGHT=c1020:10200010 also work (menu.rs:745-751).
- **Status:** partial
- **Tests:** No test loads 10200010; the c1020 smoke and full runs use row 10202060 (sim_tests.rs:3450-3460, 3724-3744; roster.json:1398). The c1020 default passes both (HANDOFF.md:539-541). The two-deathblow path is tested only on c7100 (sim_tests.rs:637-660).
- **Correction (completeness check):** partial. Not run by any harness: no boss script, and not c1020's roster default 10202060 (roster.json; src/sim_tests.rs:3720-3742). Only an outfit option in src/debug_menu.rs:22 and a config example in src/config.rs:292-297.
- **Done:**
  - Row data: HP 1273, posture 600, ninsatuNum 2. modelDispMask12 = 1 (the haori; kb/visuals.md:243 'with haori rows such as 10200010 add 12'). Resident 200051 'Behavior pattern change 1' and 241100 mid-boss reaction (c1020.json params.NpcParam['10200010']).
  - Think row 10200010 (data.rs:879-884; roster.json:1436) -> battleGoal 102000. The battle script reads 200051 from the start: it sets local14/local15, which enables the acts gated on them (102000_battle.lua:45-49), through HasSpecialEffectId on the resident list (ai.rs:639).
  - Two deathblows, HP refill and the GetNinsatsuNum phase use the same generic path as 10200000 (enemy.rs:346-362, 1038-1041; ai.rs:646-649).
- **Open:**
  - No arena, so the fight is not at entity 1100301 (tools/boss_arenas.py:106-111; map.rs:112-135).
  - Map events are not run: 20005330/20005331/20005340, plus 20005200 (includes 2003[18] Force Animation Playback and 2004[31]) (EMEVD query).
  - The row's growthDopingSpEffectID 7010 is not read; config area_doping is used instead (enemy.rs:1013-1022).
  - No mini-boss bar or music during the fight (hud.rs:356-416; fmod.rs:690-694).
  - Clips: kb/clips.md:130 (all-bosses clip 6:36-7:22). The kb itself marks the finishers-clip label as uncertain ('Tenzen Yamauchi?', kb/clips.md:99). No comparison done.
  - No test for this row.
- **Key files:** `extracted/enemies/c1020.json`, `extracted/ai_src/102000_battle.lua`, `extracted/event/m11_00_00_00.emevd`, `src/data.rs`, `src/enemy.rs`, `src/map.rs`, `src/config.rs`, `src/debug_menu.rs`

### 10201000 Leader Shigenori Yamauchi

- **chr / map:** c1020, m11_02_00_00 (Ashina Reservoir per npc_names.json:6,16; roster.json:1438-1450 'Tutorial Samurai General Hidden Prison'). MSB entity 1120250; bar 2003[87] nameId 911020 from common event 20005330 (EMEVD/MSB query over extracted/event/m11_02_00_00.emevd).
- **Setup:** No arena (no map_boss_arenas.json key), so the fight falls back to config [world] map (map.rs:94-106,1249). No boss script (2003[87] bar; tools/boss_scripts.py:345). No phase rule in boss_events.json, although the game has one wait on its bars: event 11127333 waits for 'IF Number of Character Health Bars' < 2 on 1120250 and then only shows 2007[17] Show Tutorial Text and sets a flag. tools/boss_events.py:375-376 drops rules that have no boss action. No extra cast. Own AI script: think 10201000 -> battleGoal 102010, loaded from extracted/script/m11_02_00_00.luabnd.d/102010_battle.lua (ai.rs:1273-1307; decompiled copy at extracted/ai_src/maps/102010_battle.lua). Start it with config npc_row=10201000, the picker, or SHINOBI_FIGHT=c1020:10201000.
- **Status:** partial
- **Tests:** The only test that names this row is encounters_carry_the_health_bar_names, and it only checks the picker name (sim_tests.rs:5731-5740). No fight test loads the row or think 10201000. smoke and full use c1020 row 10202060 (sim_tests.rs:3450-3460, 3724-3744), and the ai.rs tests load think 10200000 (ai.rs:1575). Whether 102010_battle.lua runs without Lua errors in Sv1 is unknown.
- **Correction (completeness check):** partial. The only test is the name check src/sim_tests.rs:5740. No fight harness runs the row (no boss script; c1020 default is 10202060 per roster.json; src/sim_tests.rs:3720-3742).
- **Done:**
  - Row data: HP 1113, posture 180, ninsatuNum 2, behaviorVariationId 10210, resident 200051 and 241100, its own mesh masks 6/7/16/21/22/23/26/27/28 (c1020.json params.NpcParam['10201000']).
  - Variation 10210 attacks: c1020.json has 21 'v10210:' attack keys, which select_variation makes the default (data.rs:892-900, load_kind data.rs:1530-1531).
  - Its battle script's natives exist in ai.rs: Get/SetStringIndexedNumber for 'FirstAttackFlag' (102010_battle.lua:11,123; ai.rs:771-776) and GetNinsatsuNum (102010_battle.lua:480; ai.rs:646-647).
  - Two-deathblow path and HP refill as 10200000 (enemy.rs:346-362).
- **Open:**
  - The tutorial setting is not exported. EMEVD events that name entity 1120250: 20005290, 11120201 (large event: 2004[41], 2002[4], 2003[82]/[83] ...), 11125247, 11125252, 11125254, 11127331, 11125332 (2007[16]/[14] hint boxes), 11127333 (tutorial text after the first bar) (EMEVD query).
  - 102010_battle.lua is untested in Sv1. In passive practice mode (enemy.rs:1140) the Rust parry port is the one ported from 102000's Goal.Parry (enemy.rs:1109-1215), not 102010's own Goal.Parry (102010_battle.lua:416).
  - The row's growthDopingSpEffectID is 7000. src/ never reads it, and the exporter only writes 7010-7014 (tools/sekiro-extract/src/export.rs:475-477).
  - No arena, mini-boss bar or music (map.rs:112-135; hud.rs:356-416; fmod.rs:690-694).
  - Clips: kb/clips.md:126 (all-bosses clip 0:00-0:24), :294 (hitbox showcase #50). No comparison done.
- **Key files:** `extracted/enemies/c1020.json`, `extracted/script/m11_02_00_00.luabnd.d/102010_battle.lua`, `extracted/ai_src/maps/102010_battle.lua`, `extracted/event/m11_02_00_00.emevd`, `src/ai.rs`, `src/data.rs`, `src/enemy.rs`, `src/sim_tests.rs`, `tools/boss_events.py`

### 10202000 General Kuranosuke Matsumoto

- **chr / map:** c1020, m11_01_00_00 (Ashina Castle per npc_names.json:5,17; roster.json:1451-1463). MSB part c1020_0000, entity 1110410, at (-143.4, -3.5, 214.6). That is 65 m (3D) from c1020_0004, the centre of the exported gate map extracted/map_m11_01_00_00.json; I computed this with a read-only MSB query. Bar 2003[87] nameId 911023 from common event 20005330.
- **Setup:** No arena key, so the fight falls back to config [world] map (map.rs:94-106,1249). That map is the same block (m11_01), but the export is centred on c1020_0004 and the boss spawns at the line-up spot (0,-4), not at entity 1110410 (enemy.rs:881-889). No boss script (tools/boss_scripts.py:345). No phase events. No cast. Think 10203100 -> battleGoal 102000 (roster.json:1462; c1020.json NpcThinkParam). The debug-menu fallback list labels it 'Honjo, with haori' (debug_menu.rs:22).
- **Status:** partial
- **Tests:** No test loads 10202000; c1020 smoke and full use row 10202060 (sim_tests.rs:3450-3460, 3724-3744). The c1020 default passes (HANDOFF.md:539-541).
- **Correction (completeness check):** partial. Only an outfit option in src/debug_menu.rs:22. No harness runs the row (no boss script; c1020 default 10202060, roster.json; src/sim_tests.rs:3720-3742).
- **Done:**
  - Row data: HP 1273, posture 600, staminaRecoverBaseVel 60 (10200010 has 30), ninsatuNum 2, haori mask 12, residents 200051, 241100, 3102050 '[NTC] Samurai General PC item use reaction' (the drink punish; kb/enemy.md:32) and 300605-300607 'for core elite' (other rows have 300600-300602) (c1020.json params.NpcParam['10202000']).
  - The real AI reads these residents through HasSpecialEffectId (ai.rs:639). Two deathblows go through the generic path (enemy.rs:346-362).
- **Open:**
  - Whether entity 1110410 lies inside the exported gate cut is unknown. The export radius is recorded as 60 (kb/map.md:18) and as 80 (config.toml:92), and he stands 65 m from the centre.
  - Map events not run: 20005330/20005331/20005340, plus 11117405 (includes 2007[17]) (EMEVD query).
  - The row's growthDopingSpEffectID 7040 is not read and not exported (tools/sekiro-extract/src/export.rs:475-477; enemy.rs:1013-1022).
  - No mini-boss bar or music (hud.rs:356-416; fmod.rs:690-694).
  - Clips: kb/clips.md:140 (all-bosses clip 21:33-22:03). No hitbox-showcase entry for him in kb/clips.md:284-294.
  - No test for this row.
- **Key files:** `extracted/enemies/c1020.json`, `extracted/map_m11_01_00_00.json`, `extracted/map/mapstudio/m11_01_00_00.msb`, `extracted/event/m11_01_00_00.emevd`, `extracted/ai_src/102000_battle.lua`, `src/enemy.rs`, `src/map.rs`

### 10210000 Seven Ashina Spears - Shikibu Toshikatsu Yamauchi

- **chr / map:** c1021 (the spear General). It plays c1020's animations (roster.json:1522-1524 animChr c1020; NpcParam normalChangeAnimChrId 1020; data.rs:792-793) and has its own model (extracted/model_c1021.bin) and sound bank (extracted/sound/c1021.fev)., m11_02_00_00 (Ashina Reservoir per npc_names.json:6,18; roster.json:1530-1542). MSB entity 1120530; bar 2003[87] nameId 911024 from common event 20005330 (EMEVD query).
- **Setup:** No arena key, so the fight falls back to config [world] map (map.rs:94-106,1249). No boss script (tools/boss_scripts.py:345). No phase events. No cast. This row is c1021's roster default (roster.json:1527), so config chr="c1021" with no npc_row already gives this fight (data.rs:867-878). Think 10204000 -> battleGoal 102020, loaded from the first map luabnd that has 102020_battle.lua: extracted/script/m11_01_00_00.luabnd.d (ai.rs:1287-1299; the m11_02 copy is the same file per ai.rs:1273-1274).
- **Status:** partial
- **Tests:** This is the best-covered row. all_enemies_smoke and all_enemies_full both run c1021 on this default row (sim_tests.rs:3450-3460, 3724-3730). all_enemies_full plays both deathblows: the first must not kill, the last must (sim_tests.rs:3774, 3836-3838). c1021 is in none of the failure lists (HANDOFF.md:385-386, 539-541; docs/reports/enemies.md:114-117, 137-148), so both pass as of 2026-10-11. That is inferred from the lists; no per-line output is saved in docs. Named test: an_enemy_plays_its_own_anim_set on this row asserts anim_group 1 and that a100 clips are played (sim_tests.rs:416-436).
- **Done:**
  - Anim set a100 from resident 200031 'Anime ID offset [1]' (enemy.rs:1005-1008; actor.rs:124; docs/reports/enemies.md:47-52). AI move requests use that set (docs/reports/enemies.md:90).
  - Row data: HP 1598, posture 600, ninsatuNum 2, variation 10201 (29 'v10201:' attack keys in c1021.json), residents 5200 and 241100 (c1021.json params.NpcParam['10210000']).
  - Deathblows: ThrowParam 11021000/0001/0010/0110/0111/0021/0031 use the same suffixes as c1020 (c1021.json params.ThrowParam). The reaction anims carry 300610 (HP reset) (c1021.json anims a000_012000 frame 135). Two-deathblow flow as in enemy.rs:346-362.
- **Open:**
  - BUG, Mikiri Counter: combat.rs:921 looks up the mikiri row as throw_row(190) = 11021190 (data.rs:827-829). c1021's mikiri row is 11021690 ('見切り PC→侍大将_大槍', atkAnimOffset 210, Wolf a210_511800, additionalConditionSpecialEffect_forDef 200031), and 11021190 exists in neither c1021.json nor combat_data.json. So a Mikiri Counter on its 7 perilous thrusts (AtkParam 10201060-10201130, atkType 2, disableGuard) plays no throw. The attacker's mikiri posture damage is still added before the lookup (combat.rs:917), and the hit then falls through to the normal path (combat.rs:923-942).
  - GetDistYSigned is a stub that returns 0 with no gap comment (ai.rs:623). The 'Wolf 3 m or more above' branch of 102020_battle.lua:460-465 (Act27) therefore never runs.
  - Passive practice mode only: data.rs:833-839 looks for extracted/ai_src/102020_battle.lua, but the copy is in extracted/ai_src/maps/. common_parry stays None, and the passive parry ignores its Common_Parry(…, 50, 0, 2, 3102) (102020_battle.lua:584) and uses the 102000 port (enemy.rs:1140-1215).
  - Map events not run: 20005330/20005331/20005340, plus 20005300 (2004[49] Change Character Patrol Behavior). Patrol routes are blocked in Sv1 (PLAN.md:77).
  - The row's growthDopingSpEffectID 7070 is not read or exported (export.rs:475-477; enemy.rs:1013-1022).
  - No mini-boss bar or music (hud.rs:356-416; fmod.rs:690-694). c1021 is missing from docs/deathblow_groups.md (only c1020 at :82).
  - Clips: kb/clips.md:86 (finishers clip 1:44-1:50), :139 (all-bosses clip 20:39-21:33), :284 (hitbox showcase #40 Ashina Seven Spear). No comparison done.
- **Key files:** `extracted/enemies/c1021.json`, `extracted/script/m11_01_00_00.luabnd.d/102020_battle.lua`, `extracted/ai_src/maps/102020_battle.lua`, `extracted/event/m11_02_00_00.emevd`, `src/combat.rs`, `src/data.rs`, `src/ai.rs`, `src/enemy.rs`, `src/sim_tests.rs`

### 10211000 Seven Ashina Spears - Shume Masaji Oniwa

- **chr / map:** c1021 (plays c1020's anims, like 10210000), m11_02_00_00 (Ashina Reservoir per npc_names.json:6,19; roster.json:1543-1555 'When Tokugawa Invades'). MSB entity 1120521; bar 2003[87] nameId 911025 from common event 20005330 (EMEVD query).
- **Setup:** Same as 10210000: no arena (fallback to config map, map.rs:94-106,1249), no boss script, no phase events, no cast. Needs config chr="c1021" npc_row=10211000, the picker, or SHINOBI_FIGHT=c1021:10211000. Think 10204001 -> battleGoal 102020 (c1021.json NpcThinkParam).
- **Status:** partial
- **Tests:** No test loads 10211000. c1021's smoke and full runs and an_enemy_plays_its_own_anim_set all use the default row 10210000 (sim_tests.rs:416-436, 3450-3460, 3724-3730). Those pass (inferred from the failure lists, HANDOFF.md:539-541). SHINOBI_ENEMIES=c1021:10211000 selects nothing (sim_tests.rs:3748-3750).
- **Correction (completeness check):** partial. No grep hit anywhere in src/docs/tools. Not c1021's roster default (10210000) and no boss script, so all_enemies_full never runs it (src/sim_tests.rs:3720-3742).
- **Done:**
  - The data differs from Shikibu only in what the game reads elsewhere: NpcParam changes growthDopingSpEffectID 7150 vs 7070, GameClearSpEffectID, souls and skillPoint. Think 10204001 changes the eye angles (80/40 vs 18), backhome distances and enableNaviFlg_Edge (c1021.json params diff). Combat, anim set a100, deathblow rows and AI script are the same as 10210000's, so the same code paths run (enemy.rs:1005-1008; enemy.rs:346-362).
- **Open:**
  - Same Mikiri bug as 10210000: throw_row(190) finds nothing, and c1021's row is 11021690 (combat.rs:917-923; c1021.json params.ThrowParam).
  - Same GetDistYSigned stub (ai.rs:623; 102020_battle.lua:462) and the same passive-mode common_parry path issue (data.rs:833-839).
  - Map events not run: 20005330/20005331/20005340, plus 11125360 (2004[50] Set Lock On Point, 2004[39]), 11125361 (2004[65] Force Character Target) and 11125362 (EMEVD query).
  - The row's growthDopingSpEffectID 7150 is not read or exported (export.rs:475-477).
  - No mini-boss bar or music (hud.rs:356-416; fmod.rs:690-694).
  - Clips: kb/clips.md:105 (finishers clip 5:08-5:12), :178 (all-bosses clip 1:19:51-1:21:05), :284 (hitbox showcase #40). No comparison done.
  - No test for this row.
- **Key files:** `extracted/enemies/c1021.json`, `extracted/ai_src/maps/102020_battle.lua`, `extracted/event/m11_02_00_00.emevd`, `src/combat.rs`, `src/ai.rs`, `src/data.rs`, `src/sim_tests.rs`

**Notes for this group:**
- Ownership (HANDOFF.md:40-49): every row in npc_names.json 'rows' is the contributor's, including these six (npc_names.json:14-19). The regular c1020 stays with the Claude sessions, along with the shared engine (combat.rs, the AI runner, the script interpreter). HANDOFF.md:48 points to docs/reports/bosses-handover.md, which does not exist yet (docs/reports/ holds README, S1-S3, V1-V2, enemies.md).
- Regular c1020 versus the boss rows. Tests: sim_tests app() = app_with("c1020") with npc_row None (sim_tests.rs:17-33, 51), which resolves to the roster default 10202060 'Honjo Internal Work' (ninsatuNum 0, think 10200000) (roster.json:1392-1398; data.rs:867-878). The user's live General is 10203010, also the no-extract fallback (HANDOFF.md:60; data.rs:812-821). The gate map is cut around that General's part c1020_0004 (kb/map.md:7-8; config.toml:92-94). All the c1020 sim tests and the hit table (kb/damage.md:244-245) are on those regular rows. All four c1020 boss rows share chr data (extracted/enemies/c1020.json: model, TAE, attacks, ThrowParam 11020xxx). Three of them also share battle script 102000 with the regular General. Anything changed there changes the regular General too.
- None of the six rows has a boss script, phase rules, arena or music. boss_scripts.json has 20 keys, boss_events.json 13, map_boss_arenas.json:1-28 and boss_bgm.json 17, and none is 102xxxxx. Cause: tools/boss_scripts.py:345 only takes characters given 2003[11] Display Boss Health Bar. These mini-bosses get 2003[87] Display Miniboss Health Bar (common event 20005330), which only tools/npc_names.py reads, for the names (npc_names.py:3,61; kb/enemy.md:203-207). boss_arenas.py builds arenas only for boss_scripts rows (tools/boss_arenas.py:106-111). boss_events.py scans event/m*.emevd for 4[37] waits and drops rules with no boss action (tools/boss_events.py:247,275,375-376).
- Map. For a row with no arena index entry, map_id / switch_map use the config's [world] map (map.rs:94-106, 1249): today m11_01_00_00, the regular General's gate (config.toml:94); with no map, the flat floor. BOSS_ARENA is only true for 'boss_' maps (map.rs:1278), so Wolf starts at (0,4) and the line-up at (0,-4) (map.rs:168-170; enemy.rs:881-889). enemy.rs on_own_arena is false for these rows (enemy.rs:842-875).
- Starting a fight: config [enemy] chr / npc_row (config.rs:292-297; the user's config.toml is theirs, HANDOFF.md:59-62). Also the picker's Bosses tab (rows with bar=true; menu.rs:175-206; data.rs:1430-1462; red dots = roster deathblows, HANDOFF.md:556-558), SHINOBI_FIGHT=<chr>:<row> (menu.rs:745-751), or the F1 debug menu built from roster placements (debug_menu.rs:36-60). The AI row can be traced with SHINOBI_THINK=<think id> think_trace (docs/reports/enemies.md:14).
- Generic boss-row mechanics these rows rely on, all data-driven. ninsatuNum 2 -> Enemy.ninsatsu (enemy.rs:1038-1041). A non-final deathblow plays the ThrowDef reaction, then Rising (enemy.rs:346-362). HP refill at the reaction's TAE SpEffect 300610 (kb/enemy.md:191-197; c1020/c1021 a000_012000 frame 135 and others). Every deathblow kind calls Enemy::deathblow (player.rs:581, 1773, 2985, 3017). GetNinsatsuNum / GetNinsatsuMaxNum are real (ai.rs:646-649). The think row comes from the row's placement (data.rs:879-884). The Won screen comes when all enemies are down, since there is no boss script (menu.rs:816-835). The generic two-deathblow tests use c7100 (sim_tests.rs:637-660, 5699).
- Test coverage. all_enemies_smoke runs each roster chr once on its default row (sim_tests.rs:3443-3460). all_enemies_full runs the roster defaults plus boss-script rows only (sim_tests.rs:3715-3744), and SHINOBI_ENEMIES cannot select another row (sim_tests.rs:3748-3750). So only c1021's 10210000 is fight-tested. Latest recorded results: smoke 102/102, full 95/110 with the 15 failures all non-fighters (HANDOFF.md:539-541). The failure lists name neither c1020 nor c1021 (HANDOFF.md:385-386; docs/reports/enemies.md:114-117, 137-148). Suite: 198 tests pass, 33 ignored, 2026-10-11 (HANDOFF.md:102).
- Area scaling. Every enemy gets config enemy.area_doping (default 7010; config.toml:65-68; enemy.rs:1013-1022). NpcParam growthDopingSpEffectID is read nowhere in src/ or tools/. The rows carry 7010, 7010, 7000, 7040, 7070 and 7150, and the exporter only writes 7010-7014 (tools/sekiro-extract/src/export.rs:475-477). What the game does with the per-row value is not traced (unknown).
- HUD: enemies get only the floating vitality and posture bars (hud.rs:356-416). No mini-boss name bar or red dots in the fight. Boss music needs a BossScript run (fmod.rs:690-694).
- No '// gap:' comment in src/ names c1021 or any of the six rows. The only gap that mentions c1020 is the generic armour-rustle sound (sound.rs:366-368). The new findings (c1021 mikiri row 11021690, the GetDistYSigned stub at ai.rs:623, the ai_src/maps path at data.rs:834, the 2003[87] filter at boss_scripts.py:345) come from reading the code and data for this report. They are not recorded in docs.
- Method note: MSB entity ids, bar nameIds and event lists came from read-only Python queries in my scratchpad. The queries call tools/boss_events.py msb_parts / msb_places (lines 65-122) and tools/boss_scripts.py emedf / instances / names over extracted/event/m11_0{0,1,2}_00_00.emevd and extracted/map/mapstudio/*.msb. Instruction names are from C:\DeadlockModding\reference\DarkScript3\DarkScript3\Resources\sekiro-common.emedf.json. No file in the repo was changed, nothing was built or run.

### 10400000 Long-arm Centipede Sen'un

- **chr / map:** c1040 (animChr c1040; extracted/enemies/roster.json:1638-1654; name extracted/enemies/npc_names.json:20), m20_00_00_00 Senpou Temple, Mt. Kongo (roster.json:1649 'Centipede Temple'; place name npc_names.json:10)
- **Setup:** Arena: none. map.rs boss_arena only looks the row up in extracted/map_boss_arenas.json (src/map.rs:112-135), which has no 10400000 (map_boss_arenas.json:2-21), so the map is config [world] map (src/map.rs:94-106; unset = flat floor, src/config.rs:158-163; the user's config.toml:94 is currently m11_01_00_00, the Outskirts gate cut, HANDOFF.md:37-38). It is not a boss arena (BOSS_ARENA only for 'boss_' ids, src/map.rs:1278), so Wolf starts at (0,4) and the enemy at (0,-4) (src/map.rs:168-170, src/enemy.rs:881-889). A picker swap keeps that map (src/map.rs:1244-1252). Boss script: none (no key in extracted/enemies/boss_scripts.json:1; src/enemy/script.rs:784-808 starts a run only for rows with one). Phase events: none (no key in boss_events.json; src/enemy.rs:259-274 returns empty). Extra cast: none (src/enemy.rs:227-256 finds nothing to enable). No boss music (not in boss_bgm.json; src/fmod.rs:692-694). Plain duel from NpcParam: ninsatuNum 2, hp 1274, stamina 360, variation 10400 (extracted/enemies/c1040.json:1 params.NpcParam.10400000). Think 10400000 -> battle goal 104000 / logic 104000 (c1040.json:1 NpcThinkParam), loaded from the first map luabnd that has it (src/ai.rs:1273-1299; extracted/script/m12_00_00_00.luabnd.d/104000_battle.lua).
- **Status:** partial
- **Tests:** all_enemies_smoke and all_enemies_full run one case per chr with the roster default row (src/sim_tests.rs:3450-3458, 3725-3729; src/data.rs:867-878), and for c1040 that row is 10400000 (roster.json:1642). Per-chr output lines are not saved in the repo. c1040 is in none of the failure lists (HANDOFF.md:385-386; docs/reports/enemies.md:114-117, 137-148; NON_FIGHTERS src/sim_tests.rs:3439), and the latest totals are smoke 102/102 and full 95/110 (HANDOFF.md:539-541; PLAN.md:80). So it passes, by inference. Named test: enemy_reach_table (ignored) has c1040 in its default list (src/sim_tests.rs:5636-5663). Its recorded result is a000_003023, window from f28, contact f28.5-29, reach 5.5 m (docs/kb/damage.md:246, where the row is mislabelled 'Ogre').
- **Done:**
  - Model, textures, cloth, anims and Wolf's per-enemy anims are exported (extracted/model_c1040.bin/.cloth.json/.dummies.json, anim_c1040.bin, anim_c0000_c1040.bin; docs/kb/damage.md:53)
  - The real Lua AI runs, with the shared AI runtime fixes (docs/reports/enemies.md:33-39, 96-103)
  - 2 deathblows: the non-final one plays ThrowDef only, then Rising with an HP refill from SpEffect 300610 in the ThrowDef TAE (docs/reports/enemies.md:40-46; docs/kb/enemy.md:134-135, 191-196). 300610 is in c1040's a000_012000/012200/013200 TAE (python string check of c1040.json).
  - Deathblow rows are ThrowParam 11040000-11040241 (c1040.json:1). Wolf plays a215 front/plunge and a202 behind (docs/deathblow_groups.md:110-114). There is no Todome 180/181 row (docs/reports/enemies.md:52-55).
  - Fur cloth: an empty constraintExecution now defaults to every set (src/cloth.rs:173-181; docs/kb/visuals.md:139-151). After 5 blow-ups in a row a cloth is carried rigidly (src/cloth.rs:268-276; docs/kb/visuals.md:186-189).
  - Reach table entry (docs/kb/damage.md:246)
  - Listed on the picker's Bosses tab under its health-bar name (src/data.rs:1430-1460; src/menu.rs:174-176, 197-205; HANDOFF.md:556)
- **Open:**
  - No own arena: Senpou Temple is not exported around him. Only boss-script rows get arenas (tools/boss_arenas.py:102-110), and only 2003[11] boss-bar characters get scripts (tools/boss_scripts.py:3-4). The mini-boss bar is 2003[87] (docs/kb/enemy.md:203-206).
  - His map EMEVD events are not exported, so nothing feeds the AI's event requests. 104000_battle.lua and 104000_logic.lua call GetEventRequest (bytecode strings), which returns -1 unless a script sets it (src/ai.rs:656-665).
  - 104000_battle.lua calls IsInsideTargetRegion, which ai.rs answers false (src/ai.rs:726-728; docs/kb/enemy.md:189-190). Which acts this blocks is unknown: there is no decompiled copy in extracted/ai_src.
  - No clip comparison yet. B1 clip comparison is open (HANDOFF.md:519-520). Available clips: hitbox showcase #41 (docs/kb/clips.md:285) and all-bosses chapter 18 (docs/kb/clips.md:144).
  - Doc mislabels: c1040 is called the Ogre in docs/kb/damage.md:246, docs/kb/visuals.md:136-151 and src/cloth.rs:173. It is the Long-arm Centipede (PLAN.md:36; docs/kb/clips.md:221-222).
  - Ownership overlap: HANDOFF.md:45 says the Claude sessions keep c1040 as a regular enemy. But both placed c1040 rows are mini-boss bar rows (npc_names.json:20-21; roster.json:1652, 1665), so the harness's c1040 case is this fight.
  - No boss/battle music for fights without a boss script (src/fmod.rs:680-683 gap)
- **Key files:** `extracted/enemies/c1040.json`, `extracted/enemies/roster.json`, `extracted/enemies/npc_names.json`, `src/sim_tests.rs`, `src/data.rs`, `src/map.rs`, `src/enemy.rs`, `src/ai.rs`, `src/cloth.rs`, `extracted/script/m12_00_00_00.luabnd.d/104000_battle.lua`

### 10401000 Long-arm Centipede Giraffe

- **chr / map:** c1040 (same chr as Sen'un; roster.json:1659-1667; name npc_names.json:21), m17_00_00_00 Sunken Valley (roster.json:1662 'Centipede Canyon'; place name npc_names.json:9)
- **Setup:** Same as 10400000. No arena (map_boss_arenas.json:2-21; src/map.rs:112-135), so it uses the config map (src/map.rs:94-106). No boss script, no phase events, no extra cast, no music (boss_scripts.json:1, boss_events.json, boss_bgm.json have no key; src/enemy/script.rs:784-808; src/enemy.rs:259-274). Reached only by picking the row: src/data.rs:867-884 takes the configured row and its placement's think 10400000 (roster.json:1667), and src/data.rs:1530-1537 applies its variation and resident SpEffects. In combat it matches 10400000: a python diff of c1040.json:1 NpcParam shows only GameClearSpEffectID 7570/7580, growthDopingSpEffectID 7070/7080, souls 105/122, skillPoint 428/778 and the name differ.
- **Status:** partial
- **Tests:** Not run directly. Both harnesses run one case per chr with the roster default row, here 10400000 (src/sim_tests.rs:3450-3458, 3725-3729; roster.json:1642). Extra full-test cases exist only for boss-script rows (src/sim_tests.rs:3731-3744). The fighting data matches the passing c1040 case (see 10400000). No named test.
- **Correction (completeness check):** partial. No grep hit in src/docs/tools. c1040's roster default is 10400000 and the row has no boss script, so no harness runs it (src/sim_tests.rs:3720-3742).
- **Done:**
  - Shares all c1040 work with Sen'un: export, AI, 2-deathblow flow, ThrowParam 11040000-11040241, cloth fix (see 10400000)
  - Listed on the picker's Bosses tab under its own name (src/data.rs:1430-1460; src/menu.rs:197-205)
- **Open:**
  - No direct harness case or named test for this row (src/sim_tests.rs:3725-3744)
  - No own arena in Sunken Valley and no map events (tools/boss_arenas.py:102-110; tools/boss_scripts.py:3-4; docs/kb/enemy.md:203-206)
  - Same AI stubs as Sen'un: IsInsideTargetRegion is false (src/ai.rs:726-728) and GetEventRequest is -1 (src/ai.rs:656-665)
  - No clip comparison yet (HANDOFF.md:519-520). Clips: hitbox showcase #35 'no deflect works every time - read description' (docs/kb/clips.md:279), finishers clip 1:50-1:54 showing one deathblow (docs/kb/clips.md:87) while NpcParam ninsatuNum is 2 (c1040.json:1), all-bosses chapter 21 (docs/kb/clips.md:147)
  - Same 'Ogre' doc mislabel and HANDOFF.md:45 ownership overlap as 10400000
- **Key files:** `extracted/enemies/c1040.json`, `extracted/enemies/roster.json`, `src/data.rs`, `src/map.rs`, `src/sim_tests.rs`, `src/menu.rs`

### 10500001 Shinobi Hunter Enshin of Misen

- **chr / map:** c1050 (animChr c1050; roster.json:1676-1692; name npc_names.json:22), m10_00_00_00 Hirata Estate (roster.json:1687 'Spear Monk Soldier Appearance 2 Samurai Residence'; place name npc_names.json:3)
- **Setup:** Arena: none. It is not in map_boss_arenas.json:2-21, so it uses the config map (src/map.rs:94-106, 112-135; config.toml:94). Boss script, phase events, extra cast and music: none (no key in boss_scripts.json:1, boss_events.json or boss_bgm.json; src/enemy/script.rs:784-808; src/enemy.rs:227-274; src/fmod.rs:692-694). Plain duel: NpcParam ninsatuNum 2, hp 1276, stamina 360, variation 10500 (c1050.json:1). Think is 10500010 (roster.json:1692), his own row. The other spear monks use 10500000, with the same battle goal 105000 but other back-home distances (c1050.json:1 NpcThinkParam). AI from extracted/script/m10_00_00_00.luabnd.d/105000_battle.lua (src/ai.rs:1273-1299).
- **Status:** partial
- **Tests:** The c1050 case of all_enemies_smoke and all_enemies_full is this row: the roster default npc is 10500001 (roster.json:1680; src/data.rs:867-884, think 10500010). c1050 is in no failure list (HANDOFF.md:385-386; docs/reports/enemies.md:137-148; src/sim_tests.rs:3439), and the totals are smoke 102/102 and full 95/110 (HANDOFF.md:539-541). So it passes, by inference. Per-chr lines are not saved. No named test.
- **Done:**
  - Model, textures, cloth, anims and Wolf's per-enemy anims are exported (extracted/model_c1050.*, anim_c1050.bin, anim_c0000_c1050.bin; docs/kb/damage.md:53)
  - Real Lua AI (goal 105000)
  - 2 deathblows: non-final ThrowDef then Rising, HP refill by 300610 in its ThrowDef TAE (docs/reports/enemies.md:40-46; docs/kb/enemy.md:191-196; 300610 found in c1050.json a000_012000/012200/013200)
  - Deathblow rows ThrowParam 11050000-11050161 (c1050.json:1). Wolf plays a200, mikiri a216 (docs/deathblow_groups.md:32).
  - On the picker's Bosses tab; its regular rows go to the Enemies tab (src/menu.rs:197-205)
- **Open:**
  - No Hirata Estate arena and no map events. Mini-boss bars (2003[87]) get no script or arena (tools/boss_scripts.py:3-4; tools/boss_arenas.py:102-110; docs/kb/enemy.md:203-206).
  - 105000_battle.lua calls AddObserveArea / IsInsideObserve (bytecode strings), which are a no-op / false (src/ai.rs:726-728, 747-749). GetTeamOrder is 0 (src/ai.rs:684-686).
  - No clip comparison yet (HANDOFF.md:519-520). Clips: all-bosses chapter 6 (docs/kb/clips.md:131), and hitbox showcase #25 'Shinobi Hunter' (docs/kb/clips.md:269), where it is unknown whether it shows Enshin or a regular c1050.
- **Key files:** `extracted/enemies/c1050.json`, `extracted/enemies/roster.json`, `src/data.rs`, `src/sim_tests.rs`, `src/ai.rs`, `src/map.rs`, `docs/deathblow_groups.md`

### 11300000 Armored Warrior

- **chr / map:** c1130 (animChr c1130; roster.json:2318-2334; name npc_names.json:30), m20_00_00_00 Senpou Temple, Mt. Kongo (roster.json:2329 'Nanban Armor Temple'). The clip notes put the fight on the Senpou bridge (docs/kb/clips.md:82).
- **Setup:** Arena: none. It is not in map_boss_arenas.json:2-21, so it uses the config map (src/map.rs:94-106, 112-135; config.toml:94). Boss script, phase events, extra cast and music: none (boss_scripts.json:1, boss_events.json, boss_bgm.json; src/enemy/script.rs:784-808; src/enemy.rs:227-274). Plain duel: NpcParam ninsatuNum 0 (one deathblow kills, docs/reports/enemies.md:45), hp 1915, stamina 750, superArmorDurability 200, variation 11300 (c1130.json:1). Roster deathblows 0 (roster.json:2319, 2326). Think 11300000 -> goal 113000 (extracted/script/m20_00_00_00.luabnd.d/113000_battle.lua).
- **Status:** partial
- **Tests:** The c1130 case of both harnesses is this row, its only placement (roster.json:2322, 2332). c1130 is in no failure list (HANDOFF.md:385-386; docs/reports/enemies.md:137-148; src/sim_tests.rs:3439), and the totals are smoke 102/102 and full 95/110 (HANDOFF.md:539-541). So it passes, by inference: with 0 deathblows, all_enemies_full expects the first deathblow to kill (src/sim_tests.rs:3768-3774, 3836-3838). Per-chr lines are not saved. No named test.
- **Done:**
  - Model, textures, cloth, anims and Wolf's per-enemy anims are exported (extracted/model_c1130.*, anim_c1130.bin, anim_c0000_c1130.bin)
  - Real Lua AI (goal 113000, which calls GetNinsatsuNum / GetNinsatsuMaxNum; these are real, docs/reports/enemies.md:39)
  - Deathblow rows ThrowParam 11130000-11130241, including 11130190 throwKind 170000 (c1130.json:1). Wolf plays a247 front/stealth/behind and a201 mikiri (docs/deathblow_groups.md:176-180). One deathblow kills (docs/reports/enemies.md:45).
  - Listed only on the picker's Bosses tab: its one placed row is a bar row (src/menu.rs:197-205)
- **Open:**
  - The game's ending is being pushed off the Senpou bridge (docs/kb/clips.md:82). Sv1 has no bridge arena (map_boss_arenas.json:2-21), and his m20 EMEVD events are not exported (tools/boss_scripts.py:3-4 covers 2003[11] bosses only). How the game kills him at the fall is not traced (unknown).
  - NpcParam superArmorDurability is 200 (c1130.json:1), but combat.rs assumes 0 and has no super-armour meter (src/combat.rs:1322-1326 gap; docs/kb/tools-exe.md:47-49). Nothing in src reads superArmorDurability.
  - 113000_battle.lua and 113000_logic.lua call GetEventRequest, which is -1 without a script (src/ai.rs:656-665). AddObserveArea is a no-op (src/ai.rs:747-749).
  - No clip comparison yet (HANDOFF.md:519-520). Clips: hitbox showcases #44 and #45 (docs/kb/clips.md:288-289), all-bosses chapter 17 (docs/kb/clips.md:143), finishers clip 1:06-1:12 (docs/kb/clips.md:82).
- **Key files:** `extracted/enemies/c1130.json`, `extracted/enemies/roster.json`, `src/combat.rs`, `src/sim_tests.rs`, `src/map.rs`, `extracted/script/m20_00_00_00.luabnd.d/113000_battle.lua`, `docs/deathblow_groups.md`

### 11910000 Snake Eyes Shirafuji

- **chr / map:** c1191 (animChr c1190; roster.json:3317-3346; name npc_names.json:31), m17_00_00_00 Sunken Valley (roster.json:3341 'Valley Enemy Valley Female Sniper')
- **Setup:** Arena: none. It is not in map_boss_arenas.json:2-21, so it uses the config map (src/map.rs:94-106, 112-135; config.toml:94). Boss script, phase events, extra cast and music: none (boss_scripts.json:1, boss_events.json, boss_bgm.json; src/enemy/script.rs:784-808; src/enemy.rs:227-274). Reached by picking the row: src/data.rs:867-884 gives think 11900102 (roster.json:3346) -> goal 119010 / logic 119000 (c1191.json:1). NpcParam: ninsatuNum 2, hp 1597, stamina 480, variation 11901 (c1191.json:1). Resident SpEffect 200031 selects anim set a100 (docs/reports/enemies.md:47-51), and c1191.json has 43 a100 anims.
- **Status:** partial
- **Tests:** Not run directly. The harnesses' c1191 case uses the roster default row 11915000 (roster.json:3321; src/sim_tests.rs:3450-3458, 3725-3729), a 'Revenge Spirit' sniper with ninsatuNum 0 and phantomShaderId 330 (c1191.json:1). That case passes, by inference (in no failure list: HANDOFF.md:385-386, docs/reports/enemies.md:137-148; totals HANDOFF.md:539-541). It covers c1191's AI goal 119010, attacks, bullets and deathblow rows. ThrowParam rows are per chr (src/data.rs:827-829), so they are the same for this row. It does not cover this row's two deathblows or its think row 11900102. No named test.
- **Correction (completeness check):** partial. No grep hit in src/docs/tools. c1191's roster default is 11915000 (roster.json) and the row has no boss script, so it is not run (src/sim_tests.rs:3720-3742).
- **Done:**
  - Model, textures and cloth exported (extracted/model_c1191.*; c1191 hakama in docs/kb/visuals.md:169). Anims come from c1190 (anim_c1190.bin, anim_c0000_c1190.bin) through animChr (roster.json).
  - Rifle bullets are exported: bullets 100-103 and 980/981 with v11901: variants (c1191.json:1), flown by src/enemy_bullet.rs (docs/reports/enemies.md:23-25; HANDOFF.md:532-535)
  - Hook grab ThrowParam 21191000 (AtkChrId 1191, atkAnimId 4100, defAnimId 600150; c1191.json:1). A grab already holding Wolf does not restart ('the valley sniper's a100_004100', src/combat.rs:947-952; docs/reports/enemies.md:86-87). That note does not name the chr, and c1190 has the same pattern (ThrowParam 21190000).
  - Deathblow rows ThrowParam 11191000-11191241 (c1191.json:1). 300610 HP reset is present in its ThrowDef TAE (a000/a100/a200/a300_012000; python string check), so the 2-deathblow Rising flow applies (docs/reports/enemies.md:40-46)
  - On the picker's Bosses tab under its own name (src/data.rs:1430-1460; src/menu.rs:197-205)
- **Open:**
  - No direct harness case or named test for this row: its 2-deathblow flow and think 11900102 are unverified (src/sim_tests.rs:3725-3744)
  - No Sunken Valley arena and no map events (tools/boss_arenas.py:102-110; tools/boss_scripts.py:3-4; docs/kb/enemy.md:203-206)
  - 119010_battle.lua calls AddObserveArea / IsInsideObserve (no-op / false) and GetEventRequest (-1) (src/ai.rs:656-665, 726-728, 747-749). GetTeamOrder is 0 (src/ai.rs:684-686).
  - Not in docs/deathblow_groups.md (generated 2026-10-09, docs/deathblow_groups.md:7). Whether Wolf has every c1191 deathblow anim is checked only by the harness's 11915000 case.
  - No clip comparison yet (HANDOFF.md:519-520). Clips: all-bosses chapter 20 (docs/kb/clips.md:146). No Shirafuji video is in the hitbox playlist table (docs/kb/clips.md:241-293).
- **Key files:** `extracted/enemies/c1191.json`, `extracted/enemies/roster.json`, `src/enemy_bullet.rs`, `src/combat.rs`, `src/data.rs`, `src/sim_tests.rs`, `src/ai.rs`, `extracted/script/m17_00_00_00.luabnd.d`

### 11911000 Snake Eyes Shirahagi

- **chr / map:** c1191 (animChr c1190; roster.json:3349-3359; name npc_names.json:32), m17_00_00_00 Sunken Valley (roster.json:3354 'Valley enemy Valley lower layer female sniper')
- **Setup:** Same as 11910000: no arena (config map), no boss script, no phase events, no extra cast, no music. The python diff of c1191.json:1 shows NpcParam differing from 11910000 only in stamina 420 (vs 480), resist_poison 700 (vs 100), souls, skill points and NG+ SpEffect ids. ninsatuNum is 2. Think is 11900101 (roster.json:3359): the same goal 119010, but wider sight (eye_ang_*_normal 50 vs 20, eye_dist_normal 60 vs 25 on 11900100; c1191.json:1 NpcThinkParam).
- **Status:** partial
- **Tests:** Not run directly. As for Shirafuji, the harnesses' c1191 case is row 11915000 (roster.json:3321; src/sim_tests.rs:3725-3729), which passes by inference (HANDOFF.md:385-386, 539-541). No named test.
- **Correction (completeness check):** partial. No grep hit in src/docs/tools. c1191's roster default is 11915000 and the row has no boss script, so it is not run (src/sim_tests.rs:3720-3742).
- **Done:**
  - Shares all c1191 work with Shirafuji: export, rifle bullets, hook grab 21191000, deathblow rows, a100 anim set, 300610 refill (see 11910000)
  - On the picker's Bosses tab under its own name (src/data.rs:1430-1460; src/menu.rs:197-205)
- **Open:**
  - No direct harness case or named test: the 2-deathblow flow and think 11900101 are unverified (src/sim_tests.rs:3725-3744)
  - No Sunken Valley (lower) arena and no map events (tools/boss_arenas.py:102-110; tools/boss_scripts.py:3-4)
  - The same AI stubs as Shirafuji (src/ai.rs:656-665, 684-686, 726-728, 747-749)
  - resist_poison 700 matters only if the status code reads it. Not checked here (unknown).
  - No clip comparison yet (HANDOFF.md:519-520). Clips: hitbox showcase #34 (docs/kb/clips.md:278), all-bosses chapter 25 (docs/kb/clips.md:151).
- **Key files:** `extracted/enemies/c1191.json`, `extracted/enemies/roster.json`, `src/enemy_bullet.rs`, `src/combat.rs`, `src/data.rs`, `src/sim_tests.rs`

**Notes for this group:**
- Ownership: since 2026-10-12 every row in extracted/enemies/npc_names.json, all six here included (npc_names.json:20-22, 30-32), belongs to the outside contributor (HANDOFF.md:6-7, 40-49). HANDOFF.md:48 points to a status sheet docs/reports/bosses-handover.md, which does not exist yet: docs/reports holds only README, S1-S3, V1-V2 and enemies.md.
- None of the six rows has a boss script, phase rules, arena or boss music. boss_scripts.json:1 has 20 keys, boss_events.json 13, map_boss_arenas.json:2-21 lists 20 rows, boss_bgm.json 17, and none of them is one of these rows (python key check). The cause is upstream: tools/boss_scripts.py:3-4 takes only characters with 2003[11] Display Boss Health Bar, while these six have mini-boss bars, 2003[87] (tools/npc_names.py:3-5; docs/kb/enemy.md:203-206). tools/boss_arenas.py:102-110 cuts arenas only for boss-script rows.
- Arena for all six: none. map.rs falls back to config [world] map (src/map.rs:94-106, 112-135), which means a flat floor when unset (src/config.rs:158-163). The user's own config.toml:94 currently says m11_01_00_00, the Outskirts gate cut (HANDOFF.md:37-38, 59-60). BOSS_ARENA stays false (src/map.rs:1278), so Wolf is at (0,4) and the enemy at (0,-4) (src/map.rs:168-170; src/enemy.rs:881-889). A picker swap keeps the config map (src/map.rs:1244-1252). The rule 'everyone else on Isshin's field boss_54000000' (HANDOFF.md:646-649; docs/kb/map.md:414-427) covers boss-script rows only, not these.
- Without a boss script, BossScript.run never starts (src/enemy/script.rs:784-808). Won shows once every enemy has been down for WON_HOLD (src/menu.rs:815-842). There is no music: boss music needs a run (src/fmod.rs:692-694), and ordinary battle music is a gap (src/fmod.rs:680-683).
- Harness coverage: all_enemies_smoke and all_enemies_full run one case per chr with the roster's default NpcParam row (src/sim_tests.rs:3450-3458, 3725-3729; src/data.rs:867-878). Extra cases are added only for boss_scripts.json rows (src/sim_tests.rs:3731-3744). The defaults: c1040 -> 10400000 (roster.json:1642), c1050 -> 10500001 (roster.json:1680), c1130 -> 11300000 (roster.json:2322), c1191 -> 11915000 (roster.json:3321, not a mini-boss row). So 10401000, 11910000 and 11911000 are never run by the harnesses.
- Latest recorded results: smoke 102/102 and full 95/110, where the failures are only the non-fighters (HANDOFF.md:539-541; PLAN.md:80). The failure lists are HANDOFF.md:385-386, docs/reports/enemies.md:114-117 and 137-148, and NON_FIGHTERS in src/sim_tests.rs:3439. c1040, c1050, c1130 and c1191 are in none of them, so their default rows pass by inference. Per-chr output lines are not saved anywhere in the repo.
- Mini-boss deathblows use the shared engine. ninsatuNum 2 rows (10400000, 10401000, 10500001, 11910000, 11911000) get a non-final ThrowDef followed by Rising and an HP refill from SpEffect 300610 in the ThrowDef TAE (docs/reports/enemies.md:40-46; docs/kb/enemy.md:134-135, 191-196). ninsatuNum 0 (11300000) dies from one (docs/reports/enemies.md:45). None of the four chrs has a Todome 180/181 ThrowParam (docs/reports/enemies.md:52-55; c*.json:1). Deathblow rows are per chr, 10_000_000 + chr*1000 + suffix (src/data.rs:827-829), so rows that share a chr share them.
- AI natives these battle scripts call (strings in the Lua 5.0 bytecode under extracted/script/m*.luabnd.d): GetEventRequest returns -1 unless a map/boss script sets it (src/ai.rs:656-665). IsInsideTargetRegion and IsInsideObserve return false (src/ai.rs:726-728). AddObserveArea is a no-op (src/ai.rs:747-749). GetTeamOrder returns 0 (src/ai.rs:684-686). Only 101000 and 102000 are decompiled in extracted/ai_src, so which acts these stubs block in 104000, 105000, 113000 and 119010 is unknown.
- Picker: Encounter.bar comes from npc_names.json, so all six rows are on the Bosses tab (src/data.rs:1430-1460; src/menu.rs:174-176, 197-205; HANDOFF.md:556). c1040 and c1130 have no placed non-bar row, so they are not on the Enemies tab at all. That conflicts with HANDOFF.md:45, which lists c1040 as a regular enemy the Claude sessions keep. c1040's only other NpcParam row, 10409000, is an unplaced 'sample' (c1040.json:1).
- Super armour: combat.rs assumes NpcParam superArmorDurability is 0 (src/combat.rs:1322-1326 gap; docs/kb/tools-exe.md:47-49), and no src file reads it. c1130 (200) is affected here, and so are many other chrs.
- Doc mislabel: c1040 is called 'Ogre' in docs/kb/damage.md:246, docs/kb/visuals.md:136-151 and src/cloth.rs:173. c1040 is the Long-arm Centipede; the Chained Ogre is c5020 (PLAN.md:36; docs/kb/clips.md:221-222).
- Clip comparison: none of the six has one yet. The only finished check is the Chained Ogre (docs/kb/clips.md:221-230), and B1 clip comparison is open (HANDOFF.md:519-520). The reference clips are local: the all-bosses video chapters (docs/kb/clips.md:115-151), the finishers clip (docs/kb/clips.md:66-92) and the hitbox playlist (docs/kb/clips.md:233-299).

### 10700000 Juzou the Drunkard

- **chr / map:** c1070, m10_00_00_00 Hirata Estate (roster.json:1880; npc_names.json:3). NpcParam name 'Shura Samurai Samurai Residence' (roster.json:1872). HP 1917, posture 750, think 10700000, 2 deathblows (roster.json:1878,1884,1885,1870)
- **Setup:** Plain one-enemy duel. There is no own arena and it is not on the shared flower field either: the row is missing from extracted/map_boss_arenas.json, so map.rs boss_arena returns None (map.rs:120-129) and the fight loads the config [world] map m11_01_00_00, the General's gate (map.rs:94-105, config.toml:94, kb/map.md:5-9). With [world] map = sandbox it loads the sandbox instead. There is no boss script: the row is not in boss_scripts.json, because only 2003[11] boss bars are exported (tools/boss_scripts.py:3,211). There are no phase events either (not in boss_events.json, queried), no extra cast, no boss music (not in boss_bgm.json, queried). The single enemy stands at (0,y,-4) and Wolf at (0,y,4) (enemy.rs:881-889, map.rs:168-170). It is the chr's default row (roster.json:1873), so it also spawns when config enemy.chr = c1070 with no npc_row (data.rs:867-878). Picker: in the Bosses tab as 'Juzou the Drunkard (Hirata Estate)' (data.rs:1449-1490, menu.rs:197-200).
- **Status:** partial
- **Tests:** Tested as the c1070 default row. all_enemies_smoke: OK, 'ai lua [SyuraSamurai_107000_Battle > AttackTunab] 4 anims, hit 4.3s | deathblow a229_510000 / ThrowDef12000' (Bosses session scratch run, C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\b5798fc2-c742-4214-9108-534312f06bc6\scratchpad\smoke.log:71, total 102/102 at :161). all_enemies_full: OK, 'hits 2 (1 atk) blow [ThrowDef12000] left 1 | hits 2 (2 atk) blow [ThrowDef12000, ThrowDefDeath12001] ... DEAD' (same scratchpad, full.txt:71, total 95/110 at :170). The docs only record the totals (smoke 102/102, full 95/110: HANDOFF.md:539-541, PLAN.md:80), and c1070 is in no failing list (enemies.md:114-117,137-148; sim_tests.rs:3439). No named sim test covers c1070. The harness runs without a map (sim_tests.rs:42-54).
- **Done:**
  - Loads with its own model, anims, NpcParam/think row and the real Lua AI 107000_battle.lua (ai.rs:1273-1299; data.rs:867-889).
  - Poison fog: resident 3107040 '[NTC] Shura Samurai_Poison' (stateInfo 950) opens the TAE-gated bullets 360/369 in a000_003012/003022/003091 (c1070.json, queried; gate code data.rs:1136-1138,1530-1537; enemies.md:28-29). Build-up SpEffect 9012 (poizonAttackPower 20) feeds Wolf's poison gauge (status.rs:1-7,170-185).
  - Two deathblows: the first plays ThrowDef only, then he gets up with HP from the ThrowDef TAE's 300610 (enemy.rs:346-362; HANDOFF.md:508-509). The last one kills. Wolf plays a229_510000 front, a201 behind/broken, a211 plunge (docs/deathblow_groups.md:75).
  - Enemy grab ThrowParam 21070000 'Shikona trampling throw' (atk 4100, Wolf def a229_600190, present in extracted/anim_c0000_c1070.bin) goes through the generic grab path (enemies.md:30-32).
  - The c1070 hair cloth (#10#_hair1) was fixed with the empty-constraintExecution rule (cloth.rs:173-177; HANDOFF.md:768-773; kb/visuals.md:143-147).
- **Open:**
  - No real Hirata Estate arena: the fight is on the General's gate map (map.rs:1245-1249). Whether this row should get an arena is not decided anywhere (HANDOFF.md:427-431 covers boss-script rows only).
  - No fight-specific map events, cast or music: mini-boss bars (2003[87]) are not exported as boss scripts (tools/npc_names.py:3-4,61 vs tools/boss_scripts.py:3,211). Whether the game's Juzou fight has adds: unknown (no Sv1 data).
  - Area scaling: Sv1 applies config [enemy] area_doping 7010 to every enemy (config.rs:290-291,374-376; enemy.rs:1013-1022). The row's own growthDopingSpEffectID 7030 and GameClearSpEffectID 7530 are not exported in c1070.json (queried: only 7010-7014) and not read in src (grep growthDoping: no hits).
  - Resident 5921 '[Behavior] Disable fall prevention assist_Infinite life' (behaviorRefId 1000150) is not read anywhere in src (grep 5921/1000150: no hits).
  - The grab a229_600190 and the poison fog on Wolf are not checked by any test.
  - Hit reach against the clips was not compared for c1070. kb/damage.md has no c1070 reach entry, and the PLAN B1 list leaves it out (PLAN.md:36). Clips: kb/clips.md:84,135,168,264,312.
  - Grapple target SpEffects 3107050-52 (kb/grapple.md:132) are not in c1070.json spEffects or TAE (queried), so enemy/grapple.rs wire_effect (src/enemy/grapple.rs:38-45) never opens him. How the game sets them is unknown ('scripts' per kb).
  - gap enemy.rs:351-352: the posture bar reset after a non-final deathblow is not traced to an event.
  - 107000_battle.lua is not decompiled (extracted/ai_src has no 107000 files), so any per-row branches in the AI are unread.
- **Key files:** `extracted/enemies/c1070.json`, `extracted/enemies/roster.json`, `extracted/script/m10_00_00_00.luabnd.d/107000_battle.lua`, `extracted/anim_c0000_c1070.bin`, `src/data.rs`, `src/enemy.rs`, `src/map.rs`, `src/status.rs`, `src/enemy_bullet.rs`, `src/sim_tests.rs`

### 10701000 Shigekichi of the Red Guard

- **chr / map:** c1070, m11_00_00_00 Ashina Outskirts (roster.json:1893; npc_names.json:4). NpcParam name 'Shura Samurai Castle Tokugawa Shura Samurai' (roster.json:1895). HP 2237, posture 840, think 10700000, 2 deathblows (roster.json:1891,1897,1898)
- **Setup:** Same plain duel as 10700000: config map m11_01_00_00, no own or shared arena (map.rs:120-129,94-105), no boss script, no phase events, no cast, no boss music (absent from boss_scripts / boss_events / map_boss_arenas / boss_bgm jsons, queried). It is reached only by config enemy.npc_row = 10701000 or the picker (Bosses tab, 'Shigekichi of the Red Guard': data.rs:1430-1462). EnemyFile::foe keeps the row because c1070.json has its NpcParam (data.rs:871-872). Row differences that Sv1 applies: resident 3107041 '[NTC] Shura Samurai_Flame' (stateInfo 951) in place of the poison 950, so the fire-breath bullet 380 opens and the poison 360/369 stay shut (c1070.json TAE, queried; data.rs:1530-1537). Burn SpEffect 9120 (registBlood 31) feeds Wolf's burn gauge (status.rs:170-185). modelDispMask 10 off and 11/20/21/22/23 on, a different outfit (model.rs:433-465). fireDamageCutRate 0.6 (combat.rs:351-363). teamType 24, hostile to team-6 enemies in a group (combat.rs:745-757).
- **Status:** unknown
- **Tests:** Never run. all_enemies_smoke and all_enemies_full test only each chr's default row plus boss_scripts.json rows (sim_tests.rs:3450-3458,3724-3744; app_with passes npc_row None, sim_tests.rs:26-28). No named sim test, no duel record and no doc entry mention 10701000 or Shigekichi.
- **Done:**
  - Selectable in the picker and through config. It loads the c1070 kind with its own NpcParam row (data.rs:867-889,1494-1537).
  - The fire variant switch is pure data and handled by the generic resident-gate code (data.rs:1136-1138,1530-1537): 3107041 -> 951 -> bullet 380 'fire breathing'.
  - The same generic two-deathblow flow and Wolf anims as the other c1070 rows (enemy.rs:346-362; docs/deathblow_groups.md:75).
- **Open:**
  - Untested: the 951 fire-breath path, the burn on Wolf and the outfit masks have never run or been looked at.
  - No Ashina Outskirts arena, cast, map events or music (same reasons as 10700000: tools/boss_scripts.py:3,211; map.rs:1245-1249).
  - physGuardCutRate 0 and attriBGuardCutRate 0 (100 on the other c1070 rows) are not read anywhere in src (grep GuardCutRate: only DamageCutRate hits).
  - Area scaling: the row's growthDopingSpEffectID 7160 / GameClearSpEffectID 7660 are not exported or applied. Sv1 uses config area_doping 7010 (enemy.rs:1013-1022).
  - teamType 24 only matters with team-6 enemies in a group (combat.rs:751-757). The hostility rule is from row names, and the exe team table is a gap (combat.rs:745-750).
  - Clip reference: kb/clips.md:179 (all-bosses video 1:21:05-1:21:45).
  - The grapple, posture-reset and AI-decompile gaps are the same as 10700000 (kb/grapple.md:132; enemy.rs:351-352).
- **Key files:** `extracted/enemies/c1070.json`, `extracted/enemies/roster.json`, `extracted/script/m11_00_00_00.luabnd.d/107000_battle.lua`, `src/data.rs`, `src/model.rs`, `src/combat.rs`, `src/status.rs`, `src/enemy.rs`, `src/map.rs`

### 10703000 Tokujiro the Glutton

- **chr / map:** c1070, m15_00_00_00 Mibu Village (roster.json:1906; npc_names.json:8). NpcParam name 'Shura Samurai Sick Village' (roster.json:1908). HP 1917, posture 750, think 10702000, 2 deathblows (roster.json:1904,1910,1911)
- **Setup:** Same plain duel on config map m11_01_00_00. No arena, boss script, phase events, cast or music (absent from all four boss jsons, queried; map.rs:120-129). Reached by config npc_row or the picker ('Tokujiro the Glutton'). Think row 10702000 comes from its placement (data.rs:879-884). It runs the same battle goal 107000 and differs from 10700000 only in sight/home distances: eye_dist_normal 25 vs 35, eye_dist_around 45 vs 55, backhome 48/8/64 vs 50/4/75, corpse_eye_dist_rate 20 vs 14 (c1070.json NpcThinkParam, queried). Residents match 10700000's poison set (3107040 -> 950) but without 5921. modelDispMask matches Juzou's (queried).
- **Status:** unknown
- **Tests:** Never run. It is not the c1070 default and not a boss_scripts row, so neither harness covers it (sim_tests.rs:3724-3744). No named test and no doc entry.
- **Done:**
  - Selectable through the picker or config, loading the c1070 kind with this NpcParam and think row (data.rs:867-889).
  - Poison fog, two-deathblow flow and grab use the same generic paths as 10700000 (data.rs:1530-1537; enemy.rs:346-362; enemies.md:30-32).
- **Open:**
  - Untested in any form.
  - No Mibu Village arena, cast, map events or music (tools/boss_scripts.py:3,211; map.rs:1245-1249).
  - It looks identical to Juzou in Sv1: same modelDispMask, same model. Whether the game gives Tokujiro another look or weapon is not recorded in Sv1 data (unknown).
  - Area scaling: the row's growthDopingSpEffectID 7100 / GameClearSpEffectID 7600 are not applied (enemy.rs:1013-1022; config.rs:290-291).
  - Clip references: kb/clips.md:97 (finishers 3:48-3:52) and kb/clips.md:154 (all-bosses 40:41-41:05).
  - The grapple, posture-reset and AI-decompile gaps are the same as 10700000.
- **Key files:** `extracted/enemies/c1070.json`, `extracted/enemies/roster.json`, `extracted/script/m15_00_00_00.luabnd.d/107000_battle.lua`, `src/data.rs`, `src/enemy.rs`, `src/map.rs`

### 10704000 Juzou the Drunkard (c1070, second Hirata visit)

- **chr / map:** c1070, m10_00_00_00 Hirata Estate (roster.json:1919; npc_names.json:3). NpcParam name 'Shura Samurai Samurai Residence_Truth Edition' (roster.json:1921). HP 1917, posture 750, think 10700010, 2 deathblows (roster.json:1917,1923,1924)
- **Setup:** The same boss as 10700000 in the 'Truth' version of Hirata Estate. Sv1 sets it up as the same plain duel on config map m11_01_00_00. No arena, boss script, phase events, cast or music (absent from all four boss jsons, queried). Think row 10700010 differs from 10700000 only in backhomeBattleDist 3 vs 4, backhomeDist 45 vs 50 and maxBackhomeDist 60 vs 75 (c1070.json, queried). Residents are the same as 10700000, including 3107040 poison (950) and 5921. Picker label: 'Juzou the Drunkard (Hirata Estate) 2', because both Juzou rows share name and place, so the later row gets a number (data.rs:1473-1490).
- **Status:** unknown
- **Tests:** Never run. Not the c1070 default, not a boss_scripts row (sim_tests.rs:3724-3744). No named test.
- **Done:**
  - Selectable through the picker or config. Loads with its own think row (data.rs:879-884).
  - The same generic poison, two-deathblow and grab paths as 10700000.
- **Open:**
  - Untested in any form.
  - The biggest gap from the real fight is area scaling. In the game this row carries growthDopingSpEffectID 7120 / GameClearSpEffectID 7620, and kb/damage.md:119-120 measured the matching area SpEffect 7124 'Growth Doping: Samurai Residence (Truth)' at physicsAttackPowerRate 3.75 and maxHpRate 4.25/4.4. Sv1 applies only config area_doping 7010 to every enemy (enemy.rs:1013-1022; config.rs:290-291,374-376), so the second Juzou fights with first-visit numbers.
  - No Hirata Estate arena, cast, map events or music (tools/boss_scripts.py:3,211; map.rs:1245-1249).
  - Clip reference: kb/clips.md:168 (all-bosses 'Juzou the Drunkard #2' 1:03:34-1:04:06). The row-to-clip mapping is not recorded.
  - The grapple, posture-reset, 5921 and AI-decompile gaps are the same as 10700000.
- **Key files:** `extracted/enemies/c1070.json`, `extracted/enemies/roster.json`, `src/enemy.rs`, `src/config.rs`, `src/data.rs`, `docs/kb/damage.md`

### 10800000 Shichimen Warrior (Abandoned Dungeon)

- **chr / map:** c1080, m13_00_00_00 Abandoned Dungeon (roster.json:1944; npc_names.json:7). NpcParam name 'Seven-sided warrior dungeon' (roster.json:1946). HP 1596, posture 420, think 10800000, 2 deathblows (roster.json:1942,1948,1949)
- **Setup:** Plain one-enemy duel on config map m11_01_00_00. There is no own or shared arena (map.rs:120-129,94-105), no boss script (tools/boss_scripts.py:3,211), no phase events, no cast and no music (absent from all four boss jsons, queried). It is the c1080 default row (roster.json:1937), so config enemy.chr = c1080 without npc_row spawns it. Picker label: 'Shichimen Warrior (Abandoned Dungeon)' (data.rs:1473-1478). Residents include 3108050 '[NTC] Individual identification_Dungeon' and 3108030 'spirit body SFX' (c1080.json, queried).
- **Status:** partial
- **Tests:** Tested as the c1080 default row. all_enemies_smoke: OK, 'ai lua [7menmusya_108000_Battle > AttackTunableS] 2 anims, hit 2.0s | deathblow a229_510000 / ThrowDef12000' (Bosses session scratch run, C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\b5798fc2-c742-4214-9108-534312f06bc6\scratchpad\smoke.log:72). all_enemies_full: OK, 'hits 2 (1 atk) blow [ThrowDef12000] left 1 | hits 2 (1 atk) blow [ThrowDef12000, ThrowDefDeath12001] ... DEAD' (same scratchpad, full.txt:72). Earlier smoke runs on 2026-10-10 failed with 'hit none' (smoke2.txt:14 to smoke4.txt:44) and pass from smoke5.txt:44 on. The docs record totals only (HANDOFF.md:539-541). No named sim test covers c1080.
- **Done:**
  - Loads with model, anims and the real Lua AI 108000_battle.lua (ai.rs:1273-1299).
  - Spirit balls and its other bullets fly through enemy_bullet.rs: BehaviorParam -> Bullet rows 10800500-10800800 (src/enemy_bullet.rs:1-20; enemies.md:23-25). Their magic damage counts (atkMag 16 on 10800500: data.rs:228; enemies.md:26).
  - Two deathblows with HP refill from the ThrowDef TAE's 300610 (enemy.rs:346-362). Wolf plays a229 front, a201 behind/broken/plunge (docs/deathblow_groups.md:72).
  - Hair cloth gets a direct display (kb/visuals.md:165-167). TAE 193 opacity keys are handled generically (Actor::opacity, HANDOFF.md:369-371).
- **Open:**
  - Terror is not implemented. Its bullets carry 9211/9212 'Scary Damage [Grief Bullet]' (stateInfo 5, changeHpRate 100, c1080.json, queried), but status.rs builds up only stateInfo 2 (poison) and 6 (burn) (status.rs:28-29,170-185). These SpEffects land on Wolf's plain timed list with no gauge and no terror death (enemy_bullet.rs:266-277). The build-up field registCurse (Paramdex SDT SpEffect.xml:420) is not among the export's 51 SpEffect fields.
  - The anti-air deathblow is never used. ThrowParam 11080140/11080141 'Anti-aircraft 0/1' (Wolf a218_511300/511310, shipped in extracted/anim_c0000_c1080.bin) exist, but player.rs only picks suffixes 0/1, 5/6, 20/21, 110/111 (+500 HU, 180/181, +8) (player.rs:517-547), and grep finds no 140/141. When the game uses it is not researched.
  - Sound: c1080.fsb stalls FMOD. Only 7 of its 112 subsounds decode (kb/sound.md:10-17; HANDOFF.md:741-743), and extracted/sound_pcm holds 8 c108* files against 138 c107* (listing). Whether its FMOD events play at runtime (fmod.rs) is not recorded (unknown).
  - Its laser and terror balls have not been compared with the clips (kb/clips.md:254 'laser and terror balls', :287 hitbox showcase, :90, :150, :153, :172).
  - No Abandoned Dungeon arena, cast, map events or music (map.rs:1245-1249; tools/boss_bgm.py:16-17).
  - Area scaling: the row's growthDopingSpEffectID 7080 / GameClearSpEffectID 7580 are not applied (enemy.rs:1013-1022).
  - Grapple SpEffects 3108000-02 (kb/grapple.md:133) are not in c1080.json spEffects or TAE (queried), so it is never a wire target (src/enemy/grapple.rs:38-45).
  - 108000_battle.lua is not decompiled (extracted/ai_src has no 108000 files). Whether the AI branches on the 'individual identification' residents 3108050/51/52 is unknown.
  - gap enemy.rs:351-352: the posture reset after a non-final deathblow is not traced.
- **Key files:** `extracted/enemies/c1080.json`, `extracted/enemies/roster.json`, `extracted/script/m13_00_00_00.luabnd.d/108000_battle.lua`, `extracted/anim_c0000_c1080.bin`, `src/enemy_bullet.rs`, `src/status.rs`, `src/player.rs`, `src/enemy.rs`, `src/map.rs`, `src/fmod.rs`, `src/sim_tests.rs`

### 10801000 Shichimen Warrior (Fountainhead Palace)

- **chr / map:** c1080, m25_00_00_00 Fountainhead Palace (roster.json:1957; npc_names.json:11). NpcParam name 'Seven-sided warrior submerged city' (水没都, roster.json:1959). HP 1596, posture 420, think 10800000, 2 deathblows (roster.json:1955,1961,1962)
- **Setup:** Same plain duel on config map m11_01_00_00. No arena, boss script, phase events, cast or music (absent from all four boss jsons, queried). Reached by config npc_row or the picker ('Shichimen Warrior (Fountainhead Palace)'). Same think row and battle goal as 10800000. Residents differ (c1080.json, queried): 3108052 'Individual identification_Submerged city', 91110 (not in c1080.json spEffects, so Sv1 cannot apply it) and 9480 'Electric shock_resistant scraping permission' (stateInfo 901).
- **Status:** unknown
- **Tests:** Never run. Not the c1080 default, not a boss_scripts row (sim_tests.rs:3724-3744). No named test.
- **Done:**
  - Selectable through the picker or config. Loads the c1080 kind with this NpcParam row (data.rs:867-889). Bullets and deathblows use the same paths as 10800000.
- **Open:**
  - Untested in any form.
  - The NpcParam name says 'submerged city'. Whether the game fights this one underwater is not recorded in Sv1 (unknown). Sv1 has no underwater fighting: c1340 'underwater only' is listed as not working (enemies.md:142-144).
  - Resident 91110 is not exported, and whether 9480 (stateInfo 901) does anything in Sv1 is unknown (no grep hit for 9480 in src).
  - The Terror, anti-air deathblow, sound, grapple, area-scaling (growthDoping 7170 / GameClear 7670) and AI-decompile gaps are the same as 10800000.
  - Clip references: kb/clips.md:150,153,172 (Shichimen #1-#3). Which number is which row is not recorded.
- **Key files:** `extracted/enemies/c1080.json`, `extracted/enemies/roster.json`, `extracted/script/m25_00_00_00.luabnd.d/108000_battle.lua`, `src/data.rs`, `src/enemy.rs`, `docs/reports/enemies.md`

### 10802000 Shichimen Warrior (Sunken Valley)

- **chr / map:** c1080, m17_00_00_00 Sunken Valley (roster.json:1970; npc_names.json:9). NpcParam name 'Seven-sided warrior valley lower layer' (roster.json:1972). HP 1596, posture 420, think 10800001, 2 deathblows (roster.json:1968,1974,1975)
- **Setup:** Same plain duel on config map m11_01_00_00. No arena, boss script, phase events, cast or music (absent from all four boss jsons, queried). Reached by config npc_row or the picker ('Shichimen Warrior (Sunken Valley)'). Think row 10800001 is identical to 10800000 except its id (c1080.json NpcThinkParam diff, queried). The only resident difference is 3108051 'Individual identification_Valley'.
- **Status:** unknown
- **Tests:** Never run. Not the c1080 default, not a boss_scripts row (sim_tests.rs:3724-3744). No named test.
- **Done:**
  - Selectable through the picker or config (data.rs:867-889). Bullets and deathblows use the same paths as 10800000.
- **Open:**
  - Untested in any form.
  - No Sunken Valley arena, cast, map events or music (map.rs:1245-1249).
  - The Terror, anti-air deathblow, sound, grapple, area-scaling (growthDoping 7130 / GameClear 7630) and AI-decompile gaps are the same as 10800000.
- **Key files:** `extracted/enemies/c1080.json`, `extracted/enemies/roster.json`, `extracted/script/m17_00_00_00.luabnd.d/108000_battle.lua`, `src/data.rs`, `src/enemy.rs`

**Notes for this group:**
- Paths without a drive are relative to C:\DeadlockModding\shinobi-combat. 'Queried' means read with python -X utf8 from the extracted jsons during this survey. Nothing was built or run.
- Ownership: all seven rows are in extracted/enemies/npc_names.json 'rows' (npc_names.json:23-29), so they belong to the outside contributor (HANDOFF.md:6-7,40-48). The per-boss sheet docs/reports/bosses-handover.md named at HANDOFF.md:48 does not exist yet: docs/reports holds only README, S1-S3, V1-V2 and enemies.md.
- All seven are mini-boss rows. tools/boss_scripts.py exports only characters with a 2003[11] Display Boss Health Bar (tools/boss_scripts.py:3,211,345), while mini-boss bars are 2003[87] (tools/npc_names.py:3-4,61). So none of the seven is in boss_scripts.json (20 rows), boss_events.json (13 rows), map_boss_arenas.json (20 rows + 'moved') or boss_bgm.json (17 rows), queried. Every boss-script feature is therefore absent for them: script.rs events, phase rules (enemy.rs:259-275), own or shared arena, boss music (tools/boss_bgm.py:16-17; fmod.rs:643-694), FIGHT_LIGHTING (map.rs:1207-1213).
- Arena: tools/boss_arenas.py builds its index only from boss_scripts.json rows (tools/boss_arenas.py:102-110), so these rows do not even get Isshin's shared flower field (HANDOFF.md:427-431). map.rs boss_arena returns None (map.rs:120-129), and map_id / switch_map fall back to config [world] map (map.rs:94-105,1245-1249) = m11_01_00_00 (config.toml:94; kb/map.md:28), the General c1020_0004's gate (kb/map.md:5-9). That is not a 'boss_' map, so on_boss_arena is false (map.rs:1278,83-85). The line-up is enemy at z -4, Wolf at z 4 (enemy.rs:881-889; map.rs:168-170).
- Per-row data Sv1 does apply (generic code, no row-specific code anywhere): the NpcParam row and the placement's think row (data.rs:867-889), resident StateInfo gates (data.rs:1530-1537,1136-1138; enemies.md:28-29), the behaviour variation (10700 / 10800, data.rs:1529-1530), modelDispMask outfits (model.rs:433-465) and elemental cut rates (combat.rs:351-363). The AI battle goal comes from the first map luabnd that has it (ai.rs:1273-1299). The m10/m11/m15 copies of 107000_battle.lua and the m13/m17/m25 copies of 108000_battle.lua are byte-identical (md5 checked).
- c1070 (four rows): resident 3107040 '[NTC] Shura Samurai_Poison' (stateInfo 950) on 10700000/10703000/10704000 opens poison-fog bullets 360/369. 3107041 '[NTC] Shura Samurai_Flame' (951) on 10701000 opens fire-breath bullet 380, in a000_003012/003022/003091 (c1070.json, queried). Unplaced c1070 NpcParam rows 10702000/10709000/10709090 also exist in c1070.json.
- Deathblows: ninsatuNum 2 on all seven rows (queried) -> Enemy.ninsatsu (enemy.rs:1038-1041). A non-final deathblow plays ThrowDef + Mode::Rising, with HP back from 300610 in the ThrowDef TAE (enemy.rs:346-362; HANDOFF.md:508-509). Open: gap enemy.rs:351-352, the posture-bar reset is not traced. Neither chr has Todome rows 180/181 (ThrowParam list in c1070.json / c1080.json, queried).
- Area scaling for every enemy is the single config [enemy] area_doping (default 7010; config.rs:290-291,374-376; applied enemy.rs:1013-1022). NpcParam growthDopingSpEffectID ('growth doping, subject to progress offset', Paramdex SDT NpcParam.xml:2430-2432) and GameClearSpEffectID are never read in src (grep: no hits). The per-row values (c1070 7030/7160/7100/7120; c1080 7080/7170/7130) are not in the chr jsons, which export only 7010-7014 (queried).
- Test coverage: all_enemies_smoke and all_enemies_full run one case per exported chr with its roster default row (sim_tests.rs:3450-3458,3724-3730; app_with -> npc_row None, sim_tests.rs:26-28), plus boss_scripts.json rows (sim_tests.rs:3731-3744). Only 10700000 and 10800000 are ever tested. They run on flat ground: the harness adds no MapPlugin (sim_tests.rs:42-54). No named sim test mentions c1070, c1080 or these rows (grep). Recorded totals: smoke 102/102, full 95/110 (HANDOFF.md:539-541; PLAN.md:80). c1070/c1080 are in no failing list (enemies.md:114-117,137-148; HANDOFF.md:385-386; NON_FIGHTERS sim_tests.rs:3439). The only per-chr result lines found are in the Bosses session's scratchpad (C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\b5798fc2-c742-4214-9108-534312f06bc6\scratchpad\full.txt:71-72, smoke.log:71-72), not in the docs.
- Picker: data::encounter_list makes one Encounter per placed row with bar = true (npc_names) and boss = false (no boss script) (data.rs:1430-1462). They appear in the Bosses tab (menu.rs:174-200; HANDOFF.md:556-559). Duplicate names get the place, then a number (data.rs:1465-1490). Place names come from tools/npc_names.py with a gap on the place id formula (tools/npc_names.py:10). A fight without a boss script ends on the Won screen when every enemy is dead (HANDOFF.md:546-547). No boss / mini-boss name bar exists in the HUD: the only 'boss' hit in hud.rs is hud.rs:393.
- Grapple: kb/grapple.md:132-133 lists the wire SpEffects 3107050-52 (c1070) and 3108000-02 (c1080) as set 'by scripts'. They are in neither chr's exported spEffects nor TAE (queried), so src/enemy/grapple.rs wire_effect (grapple.rs:38-45) never makes these enemies wire targets.
- No '// gap:' comment in src names c1070, c1080, these rows or these bosses (grep). The code comments that mention them are notes or fixes: cloth.rs:173-177 (c1070 hair constraint order), data.rs:228 and enemy_bullet.rs:3 (Shichimen spirit balls), map.rs:654 (wire variation rows 10700..).
- Real-game references for the contributor: the finishers video (kb/clips.md:84 Juzou, :90 Shichimen, :97 Tokujiro), the all-57-bosses video chapters (kb/clips.md:135,150,153,154,168,172,179), the hitbox playlist (kb/clips.md:254 Shichimen laser/terror balls, :264 Juzou, :287 Shichimen) and the combat-flow clip with a Juzou chapter (kb/clips.md:310-316).

### 13001000 Mist Noble

- **chr / map:** c1300, m15_00_00_00 Mibu Village (roster.json:4372-4381; npc_names.json:8,34). MSB entity 1500200 (m15 MSB, read with tools/boss_events.py:65 msb_parts)
- **Setup:** No own fight setup. Boss script: none (boss_scripts.json has 20 keys, none of them 13001000). The cause: tools/boss_scripts.py:3,345 only takes 2003[11] Display Boss Health Bar, and this row's bar is 2003[87] Display Miniboss Health Bar [1,1500200,0,911300] in the m15 common event 20005330 (extracted/event/m15_00_00_00.emevd, decoded read-only with tools/boss_scripts.py instances). Arena: none (map_boss_arenas.json:1-27 has no key), so map.rs:99-103 falls back to config [world] map, or to the flat floor when that is unset. Phase events: none (boss_events.json keys at lines 2-1862, none of these rows). Extra cast: none. It loads only as a plain enemy from the picker's Bosses tab (menu.rs:175-200; data.rs:1442-1462). Its think is 13000001, taken from its placement (data.rs:880-884; roster.json:4380), which runs Lua 130000 (c1300.json:1 NpcThinkParam.13000001 battleGoalID 130000). NpcParam 13001000 (c1300.json:1): hp 397, posture 150, ninsatuNum 2, resident 5021, variation 13000.
- **Status:** unknown
- **Tests:** No test runs row 13001000. all_enemies_smoke and all_enemies_full run each chr's default roster row (sim_tests.rs:26-27,3443-3457,3724-3729; data.rs:877), and for c1300 that is 13000001 (roster.json:4264). They add only rows that have a boss script (sim_tests.rs:3731-3743). Results for c1300 row 13000001: smoke passes only through the NON_FIGHTERS 'stands' rule (sim_tests.rs:3436-3439,3536-3537; smoke 102/102 at HANDOFF.md:539-541). Full fails: '16 ... c1300 Aging' (HANDOFF.md:385-386), and the latest 95/110 lists 'the 15 non-fighters' as the failures (HANDOFF.md:541; PLAN.md:80). The recorded reason is 'breath only builds Aging, no damage' (enemies.md:116-117,146-147).
- **Done:**
  - The generic per-row load works for this row: model, Lua AI, variation 13000 attacks and bullets, front deathblow ThrowParam 11300000+ (c1300.json:1 ThrowParam), and Wolf's c1300 deathblow clips (extracted/anim_c0000_c1300.bin; deathblow_groups.md:52, front a202)
  - Its official name 'Mist Noble' shows in the picker's Bosses tab (npc_names.json:34; data.rs:1449-1460; menu.rs:198)
  - Two deathblows (ninsatuNum 2) go through the generic multi-deathblow path, Mode::Rising (enemies.md:40-46)
- **Open:**
  - The c1300 test result does not cover this row. Resident 5021 sends the Mist Noble down 130000_battle.lua:73-90 (Act02 anim 3004, Act04 3009, Act05, Act27). The tested row 13000001 has 5020 and takes the breath branch at 130000_battle.lua:31-72. Anim 3004 carries AttackBehavior 110 (AtkParam 13000110, atkPhys 32) and 3009 carries 130 (13000130, atkPhys 80) (c1300.json:1 anims/attacks), so 'no damage' is not established for 13001000. It is untested either way.
  - Aging status is not done (the known open item). status.rs builds only poison and burn (status.rs:28-29,171,183-184); see also PLAN.md:71-72,76, HANDOFF.md:291,384,1028-1029, enemies.md:146-147 and sim_tests.rs:3438. The AI reads Wolf's Aging 9620 at 130000_battle.lua:23,42,59,399,405. The Aging deathblow rows 11300260-11300281 exist in c1300.json:1 ThrowParam, but no throw_row call in src uses suffix 260, 270 or 280 (data.rs:827; player.rs:506-552; combat.rs:918-1094).
  - The fight's map events are not ported, because there is no boss script. m15 event 11505205 keeps 1500200 invincible with AI state 0 until Wolf enters region 1502430. Event 11505200 fires on its HP 0: it changes Map Ceremony 15 (the mist), toggles map hits 1504214-1504336, map parts 1507210 and 1507305, objects and SFX 1502500/1502501 and character 1505400, awards lot 13001000 and sets flags 9380 and 11500200. Both were decoded read-only from extracted/event/m15_00_00_00.emevd.
  - c1300 is in NON_FIGHTERS as a whole chr (sim_tests.rs:3439), so the smoke check can never fail for any c1300 row. A case for c1300:13001000 has to be added by hand, because all_enemies_full only adds rows that have a boss script (sim_tests.rs:3731-3743).
  - No own arena (the Mibu temple): the fight falls back to the config map (map.rs:99-103). boss_arenas.py:3,102-110 only cuts arenas for rows that have a boss script.
  - Its look is unchecked. Its modelDispMask set (0,10,15,20,22,23) differs from 13000001's (0,11,15,17,21,22,24) (c1300.json:1). model.rs:433-462 applies the masks, but no shot of this row is recorded, and HANDOFF.md:822-824 reports masks that did not cut groups on c1260.
- **Key files:** `extracted/enemies/c1300.json`, `extracted/enemies/roster.json`, `extracted/ai_src/maps/130000_battle.lua`, `extracted/event/m15_00_00_00.emevd`, `src/status.rs`, `src/sim_tests.rs`, `src/data.rs`, `tools/boss_scripts.py`, `docs/PLAN.md`

### 13100310 Okami Leader Shizu

- **chr / map:** c1310, m25_00_00_00 Fountainhead Palace (roster.json:4590-4600; npc_names.json:11,35). MSB parts 2500406 and 2500699 (m25 MSB)
- **Setup:** No boss script, no arena and no phase events, for the same reason as the Mist Noble: her bar is 2003[87] [1,2500406,0,911310] in m25 event 12505215, and boss_scripts.py:345 only takes 2003[11]. In the game data, 2500406 is the fighter. 2500699 is not a fighter: m25 12505699 makes it a bullet owner and disables it, and 12505600 shoots bullet 1002500 from 2502600 while Wolf is in region 2502610 (m25_00_00_00.emevd, decoded read-only). Extra cast: none. Sv1 loads a single actor from the picker with think 13100301 (roster.json:4599), which runs battle Lua 131030 (c1310.json:1 NpcThinkParam.13100301). Variation 13103 is the kemari set, with bullets v13103:100/140/160/180/190. Bullet 180 is 'Lightning Overhead Shoot': AtkParam 13103180, spAttribute 10, homing 130 (c1310.json:1 bullets). Resident 200033 selects the a300 anim set (c1310.json has 71 a300 clips). NpcParam: hp 791, posture 150, ninsatuNum 1 (c1310.json:1).
- **Status:** unknown
- **Tests:** No test runs 13100310. The c1310 default row is 13100000 'Genpei warrior _ sword' (roster.json:4458; sim_tests.rs:3724-3743). For that row: smoke 102/102 (HANDOFF.md:539-541). In the full test it is neither in the named 16 failures (HANDOFF.md:385-386) nor among the 15 non-fighters (HANDOFF.md:541; NON_FIGHTERS at sim_tests.rs:3439), so it passes. That is inferred; no per-chr output line is saved in the docs.
- **Done:**
  - The generic load works with variation 13103 attacks and bullets (data.rs:894-900 select_variation) and the a300 anim set from SpEffect 200033 (enemies.md:48-52)
  - Lightning Reversal against spAttribute 10 attacks and bullets is generic (combat.rs:1010-1030; enemy_bullet.rs:10-12)
  - Deathblow rows 11310000+ exist and use Wolf's shared a200 clips (c1310.json:1 ThrowParam; deathblow_groups.md:23)
  - Its official name 'Okami Leader Shizu' shows in the picker's Bosses tab (npc_names.json:35; menu.rs:198)
- **Open:**
  - The map events are not ported. m25 12505215 shows the bar only inside region 2502345 with AI state 3 and sets flag 9201. 12505216 runs Force Character Target on Wolf while he has 110040 'Underwater judgment' or 110041 'Water judgment' (m25_00_00_00.emevd, decoded; SpEffectParam names from extracted/json/params/SpEffectParam.json).
  - Her own think-id branch (131030_battle.lua:22-30: Wolf at 40 m or more and in or under water) calls IsInsideTargetRegion(2502630), which Sv1 always answers false (ai.rs:726-728).
  - Lightning Reversal charge strength: Sv1 always applies 9490 (gap noted at actor.rs:67-71).
  - No arena (her roof spot in Fountainhead Palace): the fight falls back to the config map (map.rs:99-103).
  - Never run in a test and never checked against a clip (reference clips at kb/clips.md:173,255).
- **Key files:** `extracted/enemies/c1310.json`, `extracted/ai_src/maps/131030_battle.lua`, `extracted/event/m25_00_00_00.emevd`, `src/combat.rs`, `src/enemy_bullet.rs`, `src/ai.rs`, `src/data.rs`

### 13400000 Headless (underwater, Ashina Castle)

- **chr / map:** c1340 (animChr c1350), m11_01_00_00 Ashina Castle (roster.json:4985-5002; npc_names.json:5,36). MSB 1110660, an underwater fight
- **Setup:** This is the c1340 default row (roster.json:4990), so the tests run it. No boss script, no arena and no phase events: the bar is 2003[87] [1,1110660,0,911350] in common event 20005330, which boss_scripts.py:345 skips. Extra cast: none. m11_01 event 11115410 keeps SpEffect 220600 'sight 0 hearing 0' on 1110660 until Wolf has 110040 and not 110041. That event is not run in Sv1. NpcParam (c1340.json:1): hp 1913, posture 660, ninsatuNum 1, residents include 5021 and 200031 (a100 set). Think 13400000 runs battle Lua 134000. ThrowParam has only 11340250 and 11340251 (underwater collapse, throwKind 260000/260010) plus the grab 21340000 (c1340.json:1).
- **Status:** partial
- **Tests:** Smoke: listed as failing on 2026-10-10 ('c1340 (underwater only)', enemies.md:142-144). It was 102/102 on 2026-10-11 (HANDOFF.md:539-541), so it now passes. It has no front deathblow row, so the deathblow part is skipped (can_blow false: sim_tests.rs:3498-3504,3539). Full: not among the named 16 failures (HANDOFF.md:385-386), and 95/110 means everything but the 15 non-fighters failed nothing (HANDOFF.md:541), so it passes. That is inferred. Without a deathblow row the full check runs only one phase and checks hits (sim_tests.rs:3773-3774), so it is never killed or defeated in the check.
- **Done:**
  - Loads and fights on land with c1350 clips and lands hits, per the test results above
  - Its grab 21340000 is used on land because the chr has no other grab (data.rs:1034-1042; kb/enemy.md:133)
  - Picker name 'Headless (Ashina Castle)', from npc_names.json:36,5 and data.rs:1473-1478
- **Open:**
  - No water or swimming: gaps at data.rs:1039-1042 and map.rs:1709, and the plan skipped swimming (HANDOFF_PLAN.md:140). No file in src references 110040 or 110041 (grep).
  - No deathblow: only the underwater ThrowParam rows 11340250/11340251 exist (c1340.json:1), so the fight cannot be finished in Sv1.
  - m11_01 event 11115410 (blind and deaf until Wolf is underwater) is not ported, so in Sv1 it is active from the start.
  - Terror build-up is missing. Its attacks carry 9213/9214/9215 'Scary damage' (stateInfo 5, behaviorRefId 590, replaceSpEffectId 9250) (c1340.json:1 attacks/spEffects). status.rs handles only stateInfo 2 and 6 (status.rs:28-29). The export has no terror amount field, only poizonAttackPower and registBlood.
  - Doc conflict: enemies.md:142-144 still lists c1340 as failing, while HANDOFF.md:385-386 and 539-541 imply it passes.
- **Key files:** `extracted/enemies/c1340.json`, `extracted/ai_src/maps/134000_battle.lua`, `extracted/event/m11_01_00_00.emevd`, `src/data.rs`, `src/sim_tests.rs`, `src/status.rs`, `docs/reports/enemies.md`

### 13401000 Headless (underwater, Fountainhead Palace)

- **chr / map:** c1340 (animChr c1350), m25_00_00_00 Fountainhead Palace (roster.json:5007-5015; npc_names.json:11,37). MSB 2500550, an underwater fight
- **Setup:** No boss script, no arena and no phase events (bar 2003[87] [1,2500550,0,911350] in common event 20005330). In the game data it has an extra cast member: the illusion 2500551, NpcParam 13401001 'Underwater neckless submerged city illusion' (hp 1113, posture 480; roster.json:5020-5028; no official name). The m25 events (decoded) work like this: 12505500 creates NPC parts 5080/5081 on both, or disables both once flag 12500550 is on. 12505501 swaps their part SE/SFX while Wolf has 3451 'spirit enchantment'. 12505502 kills 2500551 when 2500550's health bars reach 0. Sv1 loads only the single actor from the picker: think 13401000 runs battle Lua 134000. NpcParam: hp 1913, posture 750, ninsatuNum 1, residents 5021 and 200031 (c1340.json:1).
- **Status:** unknown
- **Tests:** No test runs 13401000. The c1340 default row is 13400000 (roster.json:4990; sim_tests.rs:3724-3743).
- **Done:**
  - It shares the generic c1340 path and data with 13400000 (c1340.json)
  - Picker name 'Headless (Fountainhead Palace)', from npc_names.json:37,11 and data.rs:1473-1478
- **Open:**
  - It has the same underwater gaps as 13400000: no water (data.rs:1039-1042) and no land deathblow row (c1340.json:1).
  - The illusion 13401001 is not set up. boss_events.py drops event 12505502 because it does not export Force Character Death on another character as an action (boss_events.py:337-372).
  - NPC parts 5080/5081 are a listed gap of the script runner (script.rs:21-22). Wolf's spirit enchantment 3451 is not referenced anywhere in src (grep).
  - Terror build-up from 9213-9215 is not applied (status.rs:28-29).
- **Key files:** `extracted/enemies/c1340.json`, `extracted/event/m25_00_00_00.emevd`, `tools/boss_events.py`, `src/enemy/script.rs`, `src/data.rs`

### 13500000 Headless (Ashina Outskirts)

- **chr / map:** c1350, m11_00_00_00 Ashina Outskirts (roster.json:5045-5053; npc_names.json:4,38). MSB 1100330
- **Setup:** This is the c1350 default row (roster.json:5041), the one the tests run. No boss script, no arena and no phase events (bar 2003[87] [1,1100330,0,911350] in common event 20005330). Extra cast: none. The common Headless events 20005290, 20005331, 20005340 and 20005430-20005433 are set up for 1100330 in m11_00, but Sv1 does not run them. NpcParam (c1350.json:1): hp 1915, posture 600, ninsatuNum 2, residents include 5021, 230640 'ghost barrier neckless' and 3135500. Think 13500000 runs battle Lua 135000. Deathblows: ThrowParam 11350000+ (front a229, behind-broken a201: deathblow_groups.md:71) and Wolf's clips in extracted/anim_c0000_c1350.bin. Its grab is 21350000.
- **Status:** partial
- **Tests:** all_enemies_full passes on 2026-10-11: 'the Headless is c1350 and passes' (HANDOFF.md:352-353; kb/enemy.md:169-170). Smoke is 102/102 (HANDOFF.md:539-541). The full check breaks posture by setting it to max (sim_tests.rs:467-474), and Wolf closes to 2.5 m (sim_tests.rs:3604), so neither the barrier's posture cut nor the at-range AI branch below is exercised.
- **Done:**
  - Loads, attacks, takes both deathblows and dies in the full check (HANDOFF.md:352-353)
  - Picker name 'Headless (Ashina Outskirts)', from npc_names.json:38,4 and data.rs:1473-1478
  - The earlier misname is corrected: c1350 is the Headless (HANDOFF.md:352-353; kb/enemy.md:169-170)
- **Open:**
  - The ghost barrier is half applied. Resident 230640 has def*StaminaDmgRate 0.1 in the export (c1350.json:1 spEffects.230640), and def_stamina_rate applies it (combat.rs:673-689,1289; residents from enemy.rs:1002), so posture damage is always x0.1. The barrier's HP damage-cut rates (0.1 in extracted/json/params/SpEffectParam.json row 230640) are not in the per-chr export, and player damage uses only NpcParam cuts (combat.rs:354-364). The barrier erase SpEffects 230645/230646 (stateInfo 933) are not exported for c1350, and neither they nor Wolf's 3451 appear in src, so there is no Divine Confetti path.
  - Terror build-up is missing. Its attacks carry 9213/9214/9215 (stateInfo 5) (c1350.json:1), and status.rs handles only stateInfo 2 and 6 (status.rs:28-29,171,183-184).
  - The common events are not run. 20005430 creates NPC parts 5020-5023. 20005431 sends AI command 10 in slot 1, then command 0 on SpEffect 3135000 'swamp generation', and on event message 10 shoots bullet 213500310. 20005432 handles the barrier erase (SpEffects 5301/300340 and part SE/SFX). 20005433 handles transparency 3135010 plus message 20: it warps the Headless to Wolf when he leaves region or swims and forces anim 20004 or 20001. All decoded from m11_00_00_00.emevd. NPC parts are also a listed gap of the script runner (script.rs:21-22).
  - AI branches change because of these gaps. Act07 (anim 3021) needs GetEventRequest(1)==10 (135000_battle.lua:53-54), and Act30 (anim 3020) needs GetEventRequest()==20 (135000_battle.lua:32-33). Sv1 returns -1 when no request was sent (ai.rs:662-665), so neither is ever chosen.
  - Region branch: with resident 5021, IsInsideTargetRegion(1102331) always false (ai.rs:726-728) and Wolf at 3 m or more, the AI weights Act25 LeaveTarget 8.5 m at 10000 (135000_battle.lua:36-39,508-513). Inferred from the code: it backs off whenever Wolf stands 3 m or more away.
  - No arena and no boss music: boss_bgm.json has none of these rows, and ordinary battle music is a gap (fmod.rs:682-683).
- **Key files:** `extracted/enemies/c1350.json`, `extracted/ai_src/maps/135000_battle.lua`, `extracted/event/m11_00_00_00.emevd`, `src/combat.rs`, `src/status.rs`, `src/ai.rs`, `src/sim_tests.rs`, `extracted/anim_c0000_c1350.bin`

### 13501000 Headless (Sunken Valley)

- **chr / map:** c1350, m17_00_00_00 Sunken Valley (roster.json:5058-5066; npc_names.json:9,39). MSB 1700500
- **Setup:** Same as 13500000: no boss script, no arena, no phase events, no extra cast (bar in common event 20005330; common events 20005331, 20005340 and 20005430-20005433 are set up for 1700500 in m17 but not run). It loads from the picker with think 13500000 (roster.json:5066). The NpcParam matches 13500000 except that its resident is 5020 instead of 5021 (c1350.json:1), so the region branch at 135000_battle.lua:36 does not apply.
- **Status:** unknown
- **Tests:** No test runs 13501000. The c1350 default row is 13500000 (roster.json:5041; sim_tests.rs:3724-3743).
- **Done:**
  - Shares all chr data, AI, deathblows and Wolf's clips with the passing 13500000 (c1350.json; extracted/anim_c0000_c1350.bin)
  - Picker name 'Headless (Sunken Valley)', from npc_names.json:39,9 and data.rs:1473-1478
- **Open:**
  - It has the same open items as 13500000 except the region branch: the ghost barrier is half applied (combat.rs:354-364,673-689), Terror is missing (status.rs:28-29), the common events 20005430-20005433 and Act07/Act30 are missing (135000_battle.lua:32-33,53-54; ai.rs:662-665), and there is no arena (map.rs:99-103).
  - The row itself has never been run.
- **Key files:** `extracted/enemies/c1350.json`, `extracted/event/m17_00_00_00.emevd`, `extracted/ai_src/maps/135000_battle.lua`

### 13502000 Headless (Mibu Village)

- **chr / map:** c1350, m15_00_00_00 Mibu Village (roster.json:5071-5092; npc_names.json:8,40). MSB parts 1500680 and 1500681, both with this row
- **Setup:** No boss script, no arena, no phase events. The bar and the common events (20005330, 20005331, 20005340, 20005430-20005433) name only 1500680. None of the m15 events I decoded names 1500681. The roster has two placements of this row, with think 13500000 (roster.json:5077-5079) and think 13500020 (roster.json:5090-5092). Which MSB part has which think is unknown. Sv1 lists the row once (data.rs:1444-1447) and uses the first matching placement's think, 13500000 (data.rs:880-884). Residents have neither 5020 nor 5021 (c1350.json:1).
- **Status:** unknown
- **Tests:** No test runs 13502000. The c1350 default row is 13500000 (roster.json:5041; sim_tests.rs:3724-3743).
- **Done:**
  - Shares all chr data, AI, deathblows and Wolf's clips with the passing 13500000 (c1350.json)
  - Picker name 'Headless (Mibu Village)', from npc_names.json:40,8 and data.rs:1473-1478
- **Open:**
  - It has the same open items as 13500000 except the region branch: barrier, Terror, common events, Act07/Act30 and arena (combat.rs:354-364,673-689; status.rs:28-29; 135000_battle.lua:32-33,53-54; map.rs:99-103).
  - The second placement's think 13500020 (c1350.json:1 NpcThinkParam.13500020) is never used (data.rs:880-884), and what part 1500681 does in the fight is unknown.
  - The row itself has never been run.
- **Key files:** `extracted/enemies/c1350.json`, `extracted/enemies/roster.json`, `extracted/event/m15_00_00_00.emevd`, `src/data.rs`

**Notes for this group:**
- All 7 rows are mini-boss bars: 2003[87] Display Miniboss Health Bar, nameIds 911300 / 911310 / 911350, decoded read-only from each map's EMEVD. tools/boss_scripts.py only reads 2003[11] (boss_scripts.py:3,196,345). So none of these rows has a boss_scripts.json entry, and with that none gets an arena (boss_arenas.py:3,102-110; map_boss_arenas.json:1-27), a phase rule (boss_events.json keys at lines 2-1862), a hand-over or a cast. Arena for every one: config [world] map, else the flat floor (map.rs:94-104). The user's config uses m11_01_00_00 (config.toml:94).
- Only the default rows are tested. all_enemies_smoke and all_enemies_full run one case per chr from roster.json's top-level npc (sim_tests.rs:26-27,3443-3457,3724-3729; data.rs:867-878) plus boss_scripts rows (sim_tests.rs:3731-3743). Tested: 13000001 (c1300), 13100000 (c1310), 13400000 (c1340), 13500000 (c1350). Never run: 13001000, 13100310, 13401000, 13501000, 13502000.
- Latest recorded test results: smoke 102/102 with the NON_FIGHTERS 'stands' rule, and full 95/110 with 'the 15 non-fighters' failing (HANDOFF.md:539-541; PLAN.md:80). The 2026-10-11 named failure list is at HANDOFF.md:385-386. The docs keep no per-chr output lines. The test_take and summary counts are all from 2026-10-11.
- All 7 rows appear in the picker's Bosses tab because npc_names.json gives them a bar (menu.rs:175-200; data.rs:1449-1460). The names are 'Mist Noble' and 'Okami Leader Shizu' (npc_names.json:34-35). The five Headless rows share a name, so the place is added in brackets (npc_names.json:36-40; data.rs:1473-1478).
- Engine gaps that hit several of these fights: IsInsideTargetRegion always returns false (ai.rs:726-728). GetEventRequest returns -1 when no map event sent one (ai.rs:662-665). Status build-ups exist only for poison (stateInfo 2) and burn (stateInfo 6) (status.rs:28-29,171,183-184), so the Headless Terror (9213-9215, stateInfo 5) and the c1300 Aging (9600, stateInfo 116) are not built. NPC parts are a listed script gap (script.rs:21-22). There is no water (data.rs:1039-1042; map.rs:1709). The original plan skipped swimming and aging (HANDOFF_PLAN.md:140). None of these rows has boss music (boss_bgm.json; fmod.rs:682-683).
- c1350 misnaming: the 2026-10-11 correction says 'the Headless is c1350 and passes' (HANDOFF.md:352-353) and 'c1350 = the Headless' (kb/enemy.md:169-170). What c1350 was wrongly called before is not in the repo or the git history. I searched commits ec9e85d to 9df085f; the older misname found there is c5300 as the Divine Dragon (enemies.md at ec9e85d:104, now enemies.md:142-143). The roster's machine names are 'Neckless castle / valley / sick village' (roster.json:5040,5063,5076); the picker uses the official names instead (data.rs:1449-1453).
- c1340 has no own animations: it uses c1350's (animChr, roster.json:4985), and Wolf's clips for both are in extracted/anim_c0000_c1350.bin. c1340 has no land deathblow, while c1350 does (c1340.json / c1350.json ThrowParam).
- Reference clips: Mist Noble kb/clips.md:155; Shizu kb/clips.md:173,255; Headless #1-#5 kb/clips.md:132,156,160,162,174; underwater Headless kb/clips.md:83,274,275; Headless hitboxes kb/clips.md:295. The uploader numbers the Headless clips, and which number matches which row is unknown.
- The contributor's status sheet that HANDOFF.md:47-48 points to (docs/reports/bosses-handover.md) does not exist yet: docs/reports holds only README, S1-S3, V1-V2 and enemies.md. Section 0.1 (HANDOFF.md:40-49) gives every boss in npc_names.json to the contributor. THIRD_PARTY.md has nothing specific to these fights.
- Docs are being edited live: HANDOFF.md changed at 04:44 on 2026-10-12, during this read. The HANDOFF line numbers cited here were checked after that edit.

### 13700000 Blazing Bull

- **chr / map:** c1370, m11_01_00_00 Ashina Castle (extracted/enemies/roster.json:5378-5389 placement, maps m11_01_00_00; npc_names.json:5 place name, :41 bar name)
- **Setup:** Arena: none of its own. There is no 13700000 key in extracted/map_boss_arenas.json:1-29, so map.rs:103 (startup) and map.rs:1249 (picker swap) fall back to the config's [world] map. In the user's config.toml:91-94 that is m11_01_00_00, the area exported round the General's part c1020_0004. That is the Bull's own MSB map, but the slice is centred on the General's spot, not the Bull's. on_boss_arena() is false there (map.rs:1278), so Wolf stands at (0,_,4) (map.rs:168-170) and the Bull at (0,_,-4) (enemy.rs:882,889). The flower-field rule in HANDOFF.md:427-429 and kb/map.md:414-422 does not reach this row: tools/boss_arenas.py:107-111 only indexes boss_scripts.json rows. Boss script: none; boss_scripts.json has no 13700000 key (tools/boss_scripts.py:345 only takes 2003[11] Display Boss Health Bar characters). Phase events: none; there is no boss_events.json key, so enemy.rs:259-275 phase_events returns an empty list. Extra cast: none. Data: NpcParam 13700000 hp 2232, stamina 600, ninsatuNum 1, teamType 6, behaviorVariationId 13700 (extracted/enemies/c1370.json:1 params.NpcParam). Think 13700000 has battleGoalID / logicId 137000 (c1370.json:1 params.NpcThinkParam), run from extracted/script/m11_01_00_00.luabnd.d/137000_battle.lua.
- **Status:** partial
- **Tests:** 13700000 is c1370's roster default row (roster.json:5376), so both harnesses run it: all_enemies_smoke via app_with(chr) (sim_tests.rs:3443,3460) and all_enemies_full (sim_tests.rs:3728). The docs do not list c1370 among the failures: docs/reports/enemies.md:114-117 and :137-148; HANDOFF.md:539-541 records smoke 102/102 and full 95/110, where the 15 failures are non-fighters. The per-chr lines exist only in another session's scratchpad logs (shared_notes). smoke.log:100 reads 'OK c1370 ... ai lua [FireBull_137000_Battle > AttackTunableSp] 2 anims, hit 1.0s | deathblow a238_510000 / ThrowDef12000'. full.txt:101 reads 'OK c1370 ... hits 2 (2 atk) blow [ThrowDef12000, ThrowDefDeath12001] left 1 DEAD'. No named sim test covers the Bull.
- **Done:**
  - The real Lua AI (137000) loads, attacks and lands hits in both harnesses (smoke.log:100, full.txt:101).
  - Its object-only hitbox AtkParam 13700800 (opposeTarget 0, 'for destroying objects') is skipped: data.rs:219-220, docs/reports/enemies.md:27.
  - Resident gate: 3137040 'for burning attack judgment' (stateInfo 950) turns on its 950-gated AttackBehaviors: data.rs:441-442, data.rs:1533-1537, data.rs:1137-1138, enemies.md:28.
  - Fire bullets: BulletBehavior judge 922 for variation 13700 (c1370.json:1 bullets 'v13700:922'), flown by enemy_bullet.rs (enemies.md:23-25).
  - Deathblow: Wolf a238 (docs/deathblow_groups.md:88; extracted/anim_c0000_c1370.bin). It dies in one (ninsatuNum 1 -> enemy.rs:1039), playing ThrowDef12000 then ThrowDefDeath12001 (full.txt:101).
  - The picker lists it under Bosses with its bar name: data.rs:1455-1460 sets bar = true and boss = false; menu.rs:174-178.
- **Open:**
  - Not on its own arena: it fights on the config's gate map at the map centre (map.rs:103, map.rs:168-170, enemy.rs:882,889), or on a flat floor when [world] map is empty (config.toml:92).
  - Wire points 3137030 / 3137031 (log ends; docs/kb/grapple.md:135, tools/sekiro-extract/defs/names/SpEffectParam.txt:2638-2639) are never put on. They are not in c1370's exported TAE (0 occurrences in c1370.json anims), not among the number constants of 137000_battle.lua, and a byte search of extracted/event/*.emevd finds nothing, so enemy/grapple.rs never offers the Bull as a grapple target. Where the game sets them: unknown.
  - Immortality ON / OFF 3137015 / 3137016 (SpEffectParam.txt:2635-2636): same finding, no setter in the TAE, the AI constants or EMEVD. Unknown.
  - The log-fire resident 3137005 (vfxId 3137000 / 3137001, c1370.json:1 spEffects) is probably not drawn: SpEffect vfxIds are only shown for status afflictions (status.rs:303-325), and no resident-effect path exists in fxr.rs, vfx.rs, ffx.rs or model.rs (grep 'resident'). Not checked in game.
  - No boss music: fmod.rs:692-694 needs a BossScript run, and boss_bgm.json has no key for the row. fmod.rs:682 marks ordinary battle music as a gap.
  - An undecoded m11_01_00_00.emevd event names NpcParam 13700000 (a raw int32 byte search finds 1 match). It is not exported, so what it does is unknown.
  - No clip comparison yet. References: docs/kb/clips.md:80 (finisher, 0:38-0:48) and :137 (all-bosses chapter 11, 18:03-19:26).
- **Key files:** `extracted/enemies/c1370.json`, `extracted/enemies/roster.json`, `extracted/script/m11_01_00_00.luabnd.d/137000_battle.lua`, `src/data.rs`, `src/enemy.rs`, `src/enemy/grapple.rs`, `src/map.rs`, `src/status.rs`, `src/sim_tests.rs`, `docs/kb/grapple.md`

### 13800000 Sakura Bull of the Palace

- **chr / map:** c1380, m25_00_00_00 Fountainhead Palace (roster.json:5403-5414; npc_names.json:11 place, :42 bar name)
- **Setup:** Arena: none of its own (no key in map_boss_arenas.json:1-29). It falls back to the config's [world] map m11_01_00_00 (map.rs:103/1249, config.toml:94), at the map centre (map.rs:168-170, enemy.rs:882,889). The only exported m25 arenas belong to the Monk and the Dragons (map_boss_arenas.json:3-4,12-13). Boss script: none (boss_scripts.json has no key; tools/boss_scripts.py:345). Phase events: none (no boss_events.json key). Extra cast: none. Data: animChr c1370, so it uses the Bull's anims (roster.json:5397-5400). NpcParam 13800000 hp 3191, stamina 990, ninsatuNum 1, teamType 6, variation 13700. Residents include 3138000 (log-fire SFX) and 3138040 (spirit-attribute attack judgment, stateInfo 951) (c1380.json:1). Think 13800000 has battleGoalID 137000 (the same battle script as the Blazing Bull), BattleStartDist 30, eye distance 60 (c1380.json:1 NpcThinkParam).
- **Status:** partial
- **Tests:** 13800000 is c1380's roster default row (roster.json:5401), so both harnesses run it (sim_tests.rs:3460,3728). The docs do not list c1380 among the failures (enemies.md:114-117, 137-148; HANDOFF.md:539-541). Scratchpad logs: smoke.log:101 reads 'OK c1380 ... hit 0.9s | deathblow a238_510000 / ThrowDef12000'; full.txt:102 reads 'OK c1380 ... hits 2 (1 atk) blow [ThrowDef12000, ThrowDefDeath12001] left 1 DEAD'. No named sim test.
- **Done:**
  - Lua AI 137000 runs, attacks and hits (smoke.log:101, full.txt:102).
  - Resident gate 3138040 (stateInfo 951) turns on its 951-gated attack set through the same path as the Blazing Bull's 950 gate: data.rs:1533-1537, data.rs:1137-1138, enemies.md:28.
  - Bullets v13700:922 (c1380.json:1). The opposeTarget-0 hitboxes are skipped (data.rs:219-220).
  - Deathblow: Wolf a238 (deathblow_groups.md:89). It dies in one (ninsatuNum 1, enemy.rs:1039).
  - The picker lists it with its bar name (data.rs:1455-1460).
- **Open:**
  - Its region checks never pass, and since they are gated on the Sakura Bull's own resident they are the row's real difference from the Blazing Bull. 137000_battle.lua calls IsInsideTargetRegion(TARGET_SELF, 2502571) right after HasSpecialEffectId(TARGET_SELF, 3138040), the Sakura Bull's resident: fn@121 pc359-365 and fn@1394 pc124-130. It also calls it on TARGET_ENE_0 with 2502571 / 2502570 (fn@121 pc424 / pc443). ai.rs:726-728 answers IsInsideTargetRegion with false.
  - A byte search finds region 2502570 in extracted/event/m25_00_00_00.emevd and m25_00_52_00.emevd (2 matches each), and NpcParam 13800000 in m25_00_00_00.emevd and m25_00_52_00.emevd (1 match each). These events are undecoded and not exported. Unknown.
  - The log-fire resident 3138000 (vfxId 3138000 / 3138001) is probably not drawn: status.rs:303-325 only covers status afflictions.
  - No arena of its own (it fights on the gate map) and no boss music (fmod.rs:692-694).
  - No clip comparison yet. Reference: docs/kb/clips.md:171 (all-bosses chapter 44, 1:07:49-1:08:56).
- **Key files:** `extracted/enemies/c1380.json`, `extracted/enemies/c1370.json`, `extracted/script/m25_00_00_00.luabnd.d/137000_battle.lua`, `src/ai.rs`, `src/data.rs`, `src/map.rs`, `extracted/event/m25_00_00_00.emevd`

### 14000200 Ashina Elite - Jinsuke Saze

- **chr / map:** c1400, m11_01_00_00 Ashina Castle (roster.json:5482-5493, NpcParam name 'Swordsman_Iai Honjo'; npc_names.json:43)
- **Setup:** Arena: none of its own (no key in map_boss_arenas.json). It fights on the config's [world] map m11_01_00_00 at the map centre (map.rs:103/1249, map.rs:168-170, enemy.rs:882,889). Boss script, phase events and extra cast: none (no key in boss_scripts.json, boss_events.json or boss_bgm.json; mini-boss bars are 2003[87], which tools/boss_scripts.py:345 does not read; tools/npc_names.py:3-4,61). How it loads: config enemy.npc_row = 14000200, or the picker's SpawnEncounter. data.rs:871-883 takes the row and the think of the placement that uses it, 14000210. NpcThinkParam 14000210 has battleGoalID 140020 (the Iai script), logicId 140000, BattleStartDist 45 (c1400.json:1); extracted/script/m11_01_00_00.luabnd.d/140020_battle.lua exists. behaviorVariationId 14002 selects the 31 'v14002:' attacks (c1400.json:1; data.rs:1530-1531). Resident 200031 'Anime ID offset [1]' (stateInfo 271) selects the a100 set; c1400.json has 69 a100 clips (enemy.rs:181, actor.rs:123). NpcParam: hp 714, stamina 270, ninsatuNum 2, teamType 6.
- **Status:** unknown
- **Tests:** Not covered by any harness. all_enemies_smoke runs c1400 once with its roster default row 14005000 (roster.json:5426; sim_tests.rs:3460; data.rs:871-878). all_enemies_full adds only boss_scripts.json rows (sim_tests.rs:3724-3743). The default row passes: smoke.log:102 reads 'OK c1400 ... npc 14005000 think 14000000 ... [Kenkaku_weak_140000_Battle ...] hit 2.7s | deathblow a216_512500 / ThrowDef14500', and full.txt:103 is OK. That says nothing about the Iai script 140020 or the a100 set. The multi-deathblow mechanism is tested only on Genichiro c7100 (sim_tests.rs:637-640, 5699-5700). encounters_carry_the_health_bar_names (sim_tests.rs:5731-5743) only prints the bar rows. Whether this fight plays correctly is unknown.
- **Done:**
  - The row's data is exported: NpcParam 14000200, NpcThinkParam 14000210, the v14002 attacks and the a100 clips (c1400.json:1).
  - The Iai battle script exists (extracted/script/m11_01_00_00.luabnd.d/140020_battle.lua) and ai.rs runs it with the placement's think id (ai.rs:650, data.rs:880-883).
  - Two deathblows go through the generic path: enemy.rs:1039 sets ninsatuNum 2. The non-final blow plays the ThrowDef reaction, refills HP from TAE 300610 and puts the enemy in Mode::Rising (enemy.rs:348-361). c1400's ThrowDef12000 / ThrowDef14500 clips (a000_012000 / a000_014500) carry 300610 (c1400.json:1 anims).
  - The picker lists it under Bosses with the bar name (data.rs:1455-1460, menu.rs:174-178). Won triggers once every enemy is down (menu.rs:819-835).
- **Open:**
  - The row was never run by a harness: the a100 Iai move set, script 140020 and the two-deathblow flow are all unverified. A test case is needed; the SHINOBI_ENEMIES filter (sim_tests.rs:3723) can only pick cases the harness already lists.
  - No arena of its own, no music (fmod.rs:692-694), and the game's events and cast for this fight are not exported (unknown what they are).
  - 140020_battle.lua calls GetTeamOrder, which is stubbed to 0 (ai.rs:684-686).
  - No clip comparison yet. References: docs/kb/clips.md:102 (finisher, 4:24-4:36) and :141 (all-bosses chapter 15, 22:03-22:27).
- **Key files:** `extracted/enemies/c1400.json`, `extracted/script/m11_01_00_00.luabnd.d/140020_battle.lua`, `src/data.rs`, `src/enemy.rs`, `src/actor.rs`, `src/ai.rs`, `src/sim_tests.rs`

### 14004200 Ashina Elite - Ujinari Mizuo

- **chr / map:** c1400, m11_01_00_00 Ashina Castle (roster.json:5586-5597, NpcParam name 'Swordsman_Iai Honjo_Red eye when Tokugawa invades'; npc_names.json:44)
- **Setup:** Set up the same way as Jinsuke Saze 14000200: no arena of its own (it fights on the config's map, map.rs:103/1249), no boss script, no phase events, no cast. Think 14000200 (placement, roster.json:5594-5596) has battleGoalID 140020, logicId 140000, BattleStartDist 25 (c1400.json:1). Variation 14002 and resident 200031 (a100 set) as for Jinsuke. Extra residents (c1400.json:1 NpcParam.14004200): 220300 'Red Eye VFX' (vfxId 9500, spCategory 100), 291400 'Swordsman_Red Eye Doping' (the exported row carries only its name, no other non-default field) and 6020 '[Behavior] Play the flame reaction animation' (stateInfo 921, which becomes a resident TAE gate through data.rs:1533-1537). NpcParam: hp 714, stamina 270, ninsatuNum 2, teamType 6.
- **Status:** unknown
- **Tests:** Not covered, for the same reason as 14000200: the harnesses run only c1400's default row 14005000 (sim_tests.rs:3460, 3724-3743), which passes (smoke.log:102, full.txt:103). No named test. Unknown.
- **Done:**
  - The row's data, think and attacks are exported, and the AI script exists (same as 14000200).
  - The two-deathblow generic path applies, with HP refill from 300610 on a000_012000 / a000_014500 (enemy.rs:348-361).
  - The 921 gate from 6020 is applied as a resident gate (data.rs:1533-1537).
  - The picker lists it with its bar name (data.rs:1455-1460).
- **Open:**
  - The row was never run by a harness (unknown whether it fights correctly).
  - The red-eye VFX 220300 (vfxId 9500) is probably not drawn: status.rs:303-325 only shows vfxIds of status afflictions, not NpcParam residents.
  - What 291400 'Red Eye Doping' changes: unknown (no non-default fields in the exported row).
  - No arena of its own, no music, no game events or cast exported (unknown). GetTeamOrder is stubbed (ai.rs:684-686).
  - No clip comparison yet. Reference: docs/kb/clips.md:177 (all-bosses chapter 50, 1:19:26-1:19:51).
- **Key files:** `extracted/enemies/c1400.json`, `extracted/script/m11_01_00_00.luabnd.d/140020_battle.lua`, `src/data.rs`, `src/enemy.rs`, `src/status.rs`, `src/sim_tests.rs`

### 14701005 Lone Shadow Vilehand

- **chr / map:** c1470, m11_01_00_00 Ashina Castle (roster.json:6250-6261, NpcParam name '...Ninja Army Invasion Poisonous Hands Medium Boss Attic'; npc_names.json:45)
- **Setup:** Arena: none of its own. It fights on the config's [world] map m11_01_00_00 at the map centre (map.rs:103/1249, map.rs:168-170, enemy.rs:882,889). Boss script, phase events and extra cast: none (no key in boss_scripts.json, boss_events.json or boss_bgm.json; tools/boss_scripts.py:345). Think 14701004 (placement, roster.json:6258-6260) has battleGoalID 147000, logicId 147000, BattleStartDist 30 (c1470.json:1). Script: extracted/script/m11_01_00_00.luabnd.d/147000_battle.lua, shared by every Lone Shadow. It branches on the think id: the compare against 14701004 is at fn@181 pc521, and GetNpcThinkParamID returns the placement's think (ai.rs:650). Variation 14700: 43 attacks, bullets 290 / 500 / 980 / 981 (c1470.json:1). NpcParam: hp 1913, stamina 540, ninsatuNum 2, teamType 24; residents include 90111 'Poison invalid' and 5021 / 5023 (logic-judgment flags the script reads).
- **Status:** unknown
- **Tests:** Not covered. Both harnesses run c1470 only with its roster default row 14701003 (roster.json:6144; sim_tests.rs:3460, 3724-3743). That row passes: smoke.log:105 reads 'OK c1470 ... npc 14701003 think 14701000 ... [Koeishu_147000_Battle > ComboAttackTunab] 2 anims, hit 1.1s | deathblow a200_510000 / ThrowDef12000', and full.txt:106 is OK. 14701003 has ninsatuNum 0 and no 5021 / 5023, so Vilehand's branches and its two deathblows were never run. No named test.
- **Done:**
  - Data is exported (NpcParam / NpcThinkParam rows in c1470.json:1), and the Lua 147000 script runs the row's own think branch (ai.rs:650, data.rs:880-883).
  - Two deathblows go through the generic path; c1470's ThrowDef12000 / ThrowDef14500 carry 300610 (c1470.json:1 anims; enemy.rs:348-361).
  - Deathblow anims: Wolf a200 front, a201 behind / plunge, a241 mikiri (deathblow_groups.md:33; extracted/anim_c0000_c1470.bin).
  - c1470 damage numbers were live-checked on the shared chr (docs/kb/damage.md:87). The x0.74 guard factor is Wolf's Flowing Water skill (sim_tests.rs:2106-2110).
  - The picker lists it with its bar name (data.rs:1455-1460).
- **Open:**
  - Region checks always fail: 147000_battle.lua calls IsInsideTargetRegion with 1112250 at fn@181 pc725/731 (after the 5021 / 5023 checks), fn@1739 pc64/70, fn@2395 pc248/254 and fn@2750 pc25/31, and with 1112470 at fn@181 pc244/304. ai.rs:726-728 answers false. Neither region id turns up in a byte search of extracted/event/*.emevd; what the regions are: unknown.
  - GetEventRequest (fn@181 pc50) always returns -1, because no phase rules exist for the row (ai.rs:662-665, enemy.rs:259-275).
  - Its poison attacks are unchecked for this row: the build-up path is status.rs, and enemy_bullet.rs:266 marks a gap.
  - The row was never run by a harness. No arena of its own, no music.
  - Clips to compare against: docs/kb/clips.md:267 (hitbox showcase 23) and :163 (all-bosses chapter 37, 48:13-48:52).
- **Key files:** `extracted/enemies/c1470.json`, `extracted/script/m11_01_00_00.luabnd.d/147000_battle.lua`, `src/ai.rs`, `src/data.rs`, `src/enemy.rs`, `src/enemy_bullet.rs`, `src/status.rs`, `docs/kb/clips.md`

### 14701006 Lone Shadow Masanaga the Spear-Bearer (Ashina Castle row)

- **chr / map:** c1470, m11_01_00_00 Ashina Castle (roster.json:6263-6274; npc_names.json:46). The NpcParam row name says '...When the Ninja Army Invades Poisonous Hands-on Medium Boss White Snake Shrine' (毒手使い = poison-hand user).
- **Setup:** The same fight as 14703002 (both rows carry the bar name 'Lone Shadow Masanaga the Spear-Bearer', npc_names.json:46-47). The picker separates them with the place name (data.rs:1470-1477), so this row shows '(Ashina Castle)'. Arena: none of its own; it fights on the config's map at the map centre (map.rs:103/1249). Boss script, phase events and cast: none. Think 14701002 (roster.json:6271-6273) has battleGoalID 147000, BattleStartDist 30, maxBackhomeDist 60 (c1470.json:1). 147000_battle.lua compares 14701002 at fn@181 pc439/449. Residents: 3147520 'Samurai Residence_First action uses candy' (checked at fn@181 pc158), plus 5021 and 5023. NpcParam: hp 1917, stamina 540, ninsatuNum 2, teamType 24.
- **Status:** unknown
- **Tests:** Not covered. Only c1470's default row 14701003 runs and passes (sim_tests.rs:3460, 3724-3743; smoke.log:105, full.txt:106). No named test.
- **Done:**
  - The row's data and its own think branch are in place: the script compares 14701002 (fn@181 pc439/449) and ai.rs:650 returns the think id.
  - Two deathblows use the generic path (enemy.rs:348-361, 1039).
  - The picker lists it with the bar name plus '(Ashina Castle)' (data.rs:1455-1477).
- **Open:**
  - Identity check needed: npc_names.json:46 names this row Masanaga, but its NpcParam row name calls it a poison-hand user (like Vilehand 14701005), and its modelDispMask set [1,10,17,19,21,27,28,29] equals 14701005's. Masanaga's Hirata row 14703002 shares the Longswordsman's mask instead (c1470.json:1). The masks therefore do not separate the spear, longsword and poison-hand loadouts. How Sv1 shows the right weapon is unknown; the other path is the TAE 233 draw masks, model.rs:437-449. Not checked on screen.
  - Region checks (1112470 after 3147520 at fn@181 pc244/304, and 1112250) always return false (ai.rs:726-728). GetEventRequest always returns -1 (ai.rs:662-665).
  - The row was never run by a harness. No arena of its own, no music. The game's events for this fight are not exported. A byte search finds 3147120 'attack enhancement VFX', which the script checks at fn@181 pc176, in m11_01_00_00.emevd (1 match); it is undecoded.
  - Which all-bosses chapter is this row: unknown. Candidates are docs/kb/clips.md:161 (#1, 46:30-47:14) and :167 (#2, 1:02:51-1:03:34).
- **Key files:** `extracted/enemies/c1470.json`, `extracted/enemies/npc_names.json`, `extracted/script/m11_01_00_00.luabnd.d/147000_battle.lua`, `src/model.rs`, `src/ai.rs`, `src/data.rs`

### 14703002 Lone Shadow Masanaga the Spear-Bearer (Hirata Estate row)

- **chr / map:** c1470, m10_00_00_00 Hirata Estate (roster.json:6315-6326, NpcParam name 'Kokageshu Samurai Residence_Truth Edition Normal Medium Boss'; npc_names.json:3 place, :47 bar name)
- **Setup:** Same boss as 14701006; the picker shows this row with '(Hirata Estate)' (data.rs:1470-1477). Arena: none of its own. It falls back to the config's [world] map m11_01_00_00, not Hirata (map.rs:103/1249, config.toml:94). Boss script, phase events and cast: none. Think 14700002 (roster.json:6323-6325) has battleGoalID 147000, BattleStartDist 30, maxBackhomeDist 45 (c1470.json:1). 14700002 is not among the think ids the script compares (14703000, 14701002, 14701003, 14701004), so which branch it takes is unknown. Residents: 3147500 'first action is a finger whistle' (checked at fn@181 pc136), 3147111 'Samurai Residence_Generate_End' (fn@181 pc284, fn@2183 pc99), 5022 and 5023. NpcParam: hp 1913, stamina 540, ninsatuNum 2, teamType 6 (the other Lone Shadow mini-boss rows are 24).
- **Status:** unknown
- **Tests:** Not covered. Only c1470's default row 14701003 runs and passes (sim_tests.rs:3460, 3724-3743; smoke.log:105, full.txt:106). No named test.
- **Done:**
  - Row data and think are exported (c1470.json:1), and the Lua AI runs with this think id (ai.rs:650, data.rs:880-883).
  - Two deathblows use the generic path (enemy.rs:348-361, 1039).
  - The picker lists it with the bar name plus '(Hirata Estate)' (data.rs:1455-1477).
- **Open:**
  - The dog whistle does nothing: the script checks HasSpecialEffectId(TARGET_FRI_0, 3115030 'dog whistle can be ordered') at fn@181 pc639 and fn@2395 pc122/156. ai.rs:490-491 reads any non-self target as Wolf's SpEffects, so these checks are always false, and no dogs are spawned.
  - The 'Generate' mechanic is not exported: a byte search finds 3147110 / 3147111 / 3147112 (Generate start / end / can't generate any more) in m10_00_00_00.emevd (6 / 3 / 1 matches), and those events are undecoded. What they do in the fight: unknown.
  - Region 1112250 checks after 5022 (fn@2750 pc19-31, fn@1739, fn@2395) always return false (ai.rs:726-728).
  - Not on Hirata: there is no m10 arena, so it fights on the m11_01 gate map. No music.
  - The display mask equals the Longswordsman's (c1470.json:1), so the spear loadout is unverified (see 14701006).
  - The row was never run by a harness. Chapter references: docs/kb/clips.md:161 / :167 (which one is this row: unknown).
- **Key files:** `extracted/enemies/c1470.json`, `extracted/script/m10_00_00_00.luabnd.d/147000_battle.lua`, `extracted/event/m10_00_00_00.emevd`, `src/ai.rs`, `src/data.rs`, `src/map.rs`

### 14704000 Lone Shadow Longswordsman

- **chr / map:** c1470, m11_02_00_00 Ashina Reservoir (roster.json:6328-6339, NpcParam name 'Lonely crowd hidden prison boss' 隠し牢 中ボス; npc_names.json:6 place, :48 bar name)
- **Setup:** Arena: none of its own (no key in map_boss_arenas.json). It falls back to the config's [world] map m11_01_00_00 (map.rs:103/1249). The m11_02 slice that exists, boss_54000000, is only used by boss-script rows (boss_arenas.py:107-111). Boss script, phase events and cast: none. Think 14703000 (roster.json:6336-6338) has battleGoalID 147000, BattleStartDist 30 (c1470.json:1); the script compares 14703000 at fn@181 pc409/424, fn@2183 pc191 and fn@2395 pc65. Residents 5022 / 5023. NpcParam: hp 1433, stamina 480, ninsatuNum 2, teamType 24.
- **Status:** unknown
- **Tests:** Not covered. Only c1470's default row 14701003 runs and passes (sim_tests.rs:3460, 3724-3743; smoke.log:105, full.txt:106). No named test.
- **Done:**
  - Row data, think and the 14703000 branches are in place (c1470.json:1; ai.rs:650).
  - Two deathblows use the generic path (enemy.rs:348-361, 1039). Deathblow anims as listed for c1470 (deathblow_groups.md:33).
  - The picker lists it with its bar name (data.rs:1455-1460).
- **Open:**
  - Its 5022-gated region branch on 1112250 (fn@2750 pc19-31; also fn@1739 and fn@2395 pc248/254) always returns false (ai.rs:726-728). GetEventRequest always returns -1 (ai.rs:662-665).
  - The row was never run by a harness. No arena of its own (gate map, not the reservoir prison), no music, and the game's events for this fight are not exported (unknown).
  - The longsword loadout is unverified: its modelDispMask equals 14703002's (c1470.json:1).
  - No clip comparison yet. Reference: docs/kb/clips.md:138 (all-bosses chapter 12, 19:26-20:39).
- **Key files:** `extracted/enemies/c1470.json`, `extracted/script/m11_02_00_00.luabnd.d/147000_battle.lua`, `src/ai.rs`, `src/map.rs`, `tools/boss_arenas.py`

**Notes for this group:**
- Ownership: all 57 boss and mini-boss bar rows in extracted/enemies/npc_names.json, including these 8 (npc_names.json:41-48), belong to the outside contributor (docs/HANDOFF.md:6-7, 40-49). HANDOFF.md:48 names the contributor's status sheet as docs/reports/bosses-handover.md, but that file does not exist yet (docs/reports/ holds only README, S1-S3, V1-V2 and enemies.md).
- None of the 8 rows has a boss script, phase events, music or an arena. tools/boss_scripts.py:3 and :345 take only 2003[11] (Display Boss Health Bar) characters; mini-boss bars are 2003[87], which only tools/npc_names.py reads (:3-4, :61). The rows are absent from boss_scripts.json (20 keys), boss_events.json (13 keys), boss_bgm.json (17 keys) and map_boss_arenas.json:1-29. So script.rs:147-160 finds no script, enemy.rs:259-275 returns no phase rules, and fmod.rs:692-694 plays no music.
- Arena rule mismatch in the docs: HANDOFF.md:427-429 and kb/map.md:414-422 say every other boss fights on Isshin's field boss_54000000, but tools/boss_arenas.py:107-111 only indexes boss_scripts.json rows. These 8 rows therefore fall back to the config's [world] map (map.rs:103 at startup, map.rs:1249 on a picker swap). In the user's config.toml:91-94 that is m11_01_00_00, the area exported round the General's part c1020_0004. On that map on_boss_arena() is false (map.rs:1278), Wolf starts at (0,_,4) (map.rs:168-170) and the enemy at (0,_,-4) (enemy.rs:882,889). With [world] map empty they fight on a flat floor (config.toml:92).
- Harness coverage: all_enemies_smoke runs one case per chr through app_with(chr) with the roster default row (sim_tests.rs:3443-3460; data.rs:871-878). all_enemies_full adds only boss_scripts.json rows (sim_tests.rs:3724-3743). So 13700000 and 13800000 are tested as the c1370 / c1380 defaults (roster.json:5376, 5401). For c1400 and c1470 only the defaults 14005000 / 14701003 run (roster.json:5426, 6144); none of the six mini-boss rows is tested.
- Latest recorded results: HANDOFF.md:539-541 gives all_enemies_smoke 102/102 and all_enemies_full 95/110, where the 15 failures are non-fighters; enemies.md:114-117 and :137-148 list the failures, and none is c1370, c1380, c1400 or c1470. Per-chr lines exist only in another session's scratchpad: C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\b5798fc2-c742-4214-9108-534312f06bc6\scratchpad\smoke.log:100-105 ('102 of 102 enemies pass' at :161, 2026-10-11 06:45) and \full.txt:101-106 ('95 of 110 enemies pass every phase' at :170, 2026-10-11 11:51). They predate later code changes.
- Two-deathblow mini-bosses (all six c1400 / c1470 rows have ninsatuNum 2) use the generic path. enemy.rs:1039 reads ninsatuNum.max(1). A non-final deathblow plays the ThrowDef reaction, refills HP from that clip's TAE 300610 (or at once) and then Mode::Rising (enemy.rs:348-361). In both c1400.json and c1470.json, a000_012000 and a000_014500 carry 300610. The path is tested only on Genichiro c7100 (sim_tests.rs:637-640, 5699-5700).
- AI natives these scripts hit (the Lua AI runs the compiled 137000 / 140020 / 147000_battle.lua; ai.rs). IsInsideTargetRegion always returns false (ai.rs:726-728). GetEventRequest returns -1 when no phase rule exists (ai.rs:662-665). GetTeamOrder returns 0 (ai.rs:684-686). HasSpecialEffectId on any non-self target, TARGET_FRI_0 included, reads Wolf's SpEffects (ai.rs:490-491). GetNpcThinkParamID returns the placement's think (ai.rs:650; data.rs:880-883).
- The fn@<line> pc<n> citations come from a scratch disassembly of the compiled Lua 5.0 bytecode: fn@ is the function's source linedefined, pc the instruction index. There is no decompiled copy of 137000 / 140020 / 147000 in extracted/ai_src. The disassembler script is C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\7f815a4a-5960-468d-9ce7-b0477af273cf\scratchpad\lua50.py. The EMEVD findings are raw int32 byte matches in extracted/event/*.emevd and are not decoded.
- Resident SpEffect visuals: SpEffect vfxIds are drawn only for status afflictions (status.rs:303-325), and grepping 'resident' finds no effect path in fxr.rs, vfx.rs, ffx.rs or model.rs. This affects the Bulls' log fire (3137005, 3138000) and Ujinari's red eye (220300). Not checked in game.
- Resident StateInfo gates are generic (data.rs:1533-1537, data.rs:1137-1138; enemies.md:28): 950 for the Blazing Bull, 951 for the Sakura Bull, 921 for Ujinari's 6020.
- Picker and win flow: encounter_list marks these rows bar = true and boss = false (data.rs:1430-1460), so they appear in the Bosses tab (menu.rs:174-178). The Masanaga rows share a bar name and get a place suffix (data.rs:1470-1477). Without a boss script, Won triggers when every enemy has been down for WON_HOLD (menu.rs:819-835).
- Team hostility matters only with several enemies (combat.rs:744-756, 6 vs 24). The Lone Shadow mini-boss rows are team 24 except 14703002 (team 6); the Bulls and Ashina Elites are team 6 (c1370 / c1380 / c1400 / c1470.json:1 NpcParam teamType).
- Stale doc line: kb/damage.md:134 still says the c1470 x0.74 guard factor is 'source open; NOT applied', but sim_tests.rs:2106-2110 attributes it to Wolf's Flowing Water skill 280. Also, kb/grapple.md:131 labels c1070 'Shura (Ashina Elite?)' while the Ashina Elite bar rows are c1400 (npc_names.json:43-44, roster.json:5490, 5594).
- How to run one enemy: docs/reports/enemies.md:8-14 (SHINOBI_ENEMIES, SHINOBI_DUEL, SHINOBI_THINK). The full harness can only filter the cases it already lists (sim_tests.rs:3723), so the six mini-boss rows need a case of their own (a chr:row entry) before they can be tested.

### 12600050 Folding Screen Monkeys

- **chr / map:** c1000 bar dummy (entity 2000800, row 12600050) + c1260 monkeys x4 (2000801-804, rows 12600000/010/020/030) + c1250 horde x9 (2000805-814 except 810) (boss_scripts.json:1 key 12600050; roster.json:4, :45-50, :3792, :3834, :4196, :4244-4249), m20 Senpou Temple (npc_names.json:10, :33; map_boss_12600050.json:6957)
- **Setup:** Own arena boss_12600050 (map_boss_arenas.json:2; OWN_ROWS tools/boss_arenas.py:67-69). The export is centred on c1000_0009 (map_boss_12600050.json:3-5) but picks pieces round monkey 2000802 out to 85 m (boss_arenas.py:49-55). Boss script: yes, 83 events, boss 2000800, 14 chars, flags_on [8400, 62000705] (boss_scripts.json:1); world flag 9405 forced off (tools/boss_scripts.py:215-220). No phase rules: boss_events.json has no 12600050 key, and rules only run for unscripted rows (src/enemy.rs:1405-1407). Decoded events (boss_scripts.json:1): 12005800 defeat when 4 monkeys are dead OR the dummy's HP ratio is 0, then kills the horde; 12005810 shows bar 901260 on any monkey's AI state 3 and sets 12005801; 12005820 (slots 0-3) applies SetSp 3126010 to the dummy when a monkey dies; 12005885 fall reset; 12005890/895/900 enable the horde at dummy HP <=0.75/0.5/0.25 with Wolf in region 2002851/2002861/2002881. Music m200000001 from flag 12005801 (boss_bgm.json:201).
- **Status:** broken
- **Tests:** No named test. all_enemies_full passes c1000:12600050 but only fights the dummy (docs/HANDOFF.md:825-827). The full harness adds this row because it is not c1000's default (default 10000000, roster.json:4) (src/sim_tests.rs:3724-3743). c1260 is in NON_FIGHTERS (src/sim_tests.rs:3439): it stands in smoke (stands rule src/sim_tests.rs:3536-3540) and is one of the rows failing all_enemies_full (docs/HANDOFF.md:385-386, :539-541). Last recorded runs: smoke 102/102, full 95/110 (docs/HANDOFF.md:539-541).
- **Done:**
  - Fight data exported: cast, events, flags (docs/HANDOFF.md:795-805)
  - Exporter fixes for this fight (docs/HANDOFF.md:806-810)
  - Own temple-hall arena exported (docs/HANDOFF.md:811-815; tools/boss_arenas.py:49-55, :67-69; map_boss_arenas.json:2)
  - Script runs and spawns the cast (the cast never enabled is spawned at start, src/enemy/script.rs:784-811, :954-972)
  - Test take set up at %TEMP%\sv1_takes\monkeys, with SHINOBI_FPS_LOG / SHINOBI_SCRIPT_TRACE (docs/HANDOFF.md:828-830)
- **Open:**
  - Open #1 fixed 2026-10-12 by "bug fixes" (63 fps): long script warps ran as a wall slide (src/map.rs bodies_on_terrain, TELEPORT); Act::Warp keeps the target's height (docs/HANDOFF.md 4.18)
  - Open #2 narrowed: the kimono meshes are cut right (masks 20/21/22 per row; the other two spawn hidden); only the hidden groups' cloth instances are still built (cost, not look)
  - Open #3: the fight logic itself (flee/hide, quarter-bar drops, horde, defeat) needs its own test, because full only fights the dummy (docs/HANDOFF.md:825-827)
  - SetSp only adds a resident and changes no HP (src/enemy/script.rs:1132-1139). SpEffect 3126010 (changeHpRate 26.0, SpEffectParam.json:1056774) therefore never drops the dummy's bar per dead monkey. As a result the HP-ratio defeat branch and the 12005890/895/900 horde wakes only fire if the dummy is hit directly.
  - The monkeys' AI region checks always return false (src/ai.rs:726-728). 126000_battle.lua has 195 IsInsideTargetRegion calls (e.g. extracted/ai_src/maps/126000_battle.lua:461-496), and 126000_logic.lua uses regions 2002830/2002870/2002880 (:36-42) and IsInsideObserve (:125).
  - Conditions the script does not interpret are answered false (src/enemy/script.rs:506-508): 13[01] in 12005827/12005874, 3[24] in 12005210, 5[02] in 12005855/12005860 (boss_scripts.json:1)
  - Actions the script ignores (src/enemy/script.rs:729): 2004[10] gravity x10, 2004[34]/[35] x24, 2004[39] x18, 2005[3] x29, 2012[1] x16, 2003[54] x9, 2003[88] x3, 2006[3] x6 and more (boss_scripts.json:1). Header gaps also cover map objects, SFX, cameras, gravity and draw masks (src/enemy/script.rs:21-22).
  - Missing FXR 2255505 for c1250/c1260 (docs/HANDOFF.md:631-635)
  - c1250/c1260 loop-after animation pairs not handled (src/enemy.rs:68-72)
- **Key files:** `docs/HANDOFF.md`, `src/enemy/script.rs`, `src/model.rs`, `src/ai.rs`, `tools/boss_scripts.py`, `tools/boss_arenas.py`, `extracted/enemies/boss_scripts.json`, `extracted/map_boss_12600050.json`, `extracted/ai_src/maps/126000_battle.lua`, `extracted/ai_src/maps/126000_logic.lua`, `src/sim_tests.rs`

### 50000000 Corrupted Monk (Fountainhead Palace, with the Mibu phantoms)

- **chr / map:** c5000 (+ phantoms c5005 2500851/853/854 row 50050000 hp 9999, + c5001 'Buddhist monk's bug' 2500852 row 50010000) (boss_scripts.json:1; roster.json:8503, :8541, :8579), m25 Fountainhead Palace (roster.json:8507-8517; npc_names.json:11, :49; map_boss_50000000.json:1719)
- **Setup:** Own arena boss_50000000 (map_boss_arenas.json:3; OWN_ROWS tools/boss_arenas.py:59), map m25 (map_boss_50000000.json:1719). Boss script: yes, 18 events, boss 2500850 (boss_scripts.json:1): 12505950/960/990, phase events 12505961-965, phantom events 12505970 x3, 12505980-984. The boss_events.json rules for 50000000 exist (boss_events.json:2; 12505961 at :646, 962 at :687, 963 at :715, 964 at :743) but are not used because the row is scripted (src/enemy.rs:1405-1407); the same events run through the script. Short-warp cast become Phantoms (src/enemy/script.rs:954-972). This is c5000's default row (roster.json:8507), so app_with("c5000") and smoke run it. Music: two tracks (boss_bgm.json:233).
- **Status:** works
- **Tests:** Named tests on the default row: the_corrupted_monks_last_deathblow_is_her_todome (src/sim_tests.rs:501-519); the_corrupted_monk_without_the_todome_message_still_dies, which asserts NO Event20021 (src/sim_tests.rs:521-551); the_corrupted_monks_phase_events_run_on_her_deathblows (src/sim_tests.rs:590-608); monk_phantoms_fade_in_and_out (src/sim_tests.rs:4968-5011); monk_phantoms_come_and_go (src/sim_tests.rs:5013-5101). Smoke covers c5000 (src/sim_tests.rs:3441-3567). c5000 is not in the failing lists (docs/HANDOFF.md:385-386, :539-541). Last recorded: 198 tests pass on 2026-10-11 (docs/HANDOFF.md:102). docs/HANDOFF.md:286 says 'Monk ... tested'.
- **Correction (completeness check):** Corrupted Monk (Fountainhead Palace, with the Mibu phantoms). The phantoms are c5005 50050000 'The Phantom of the Haunting Monk Submerged City', placed only in m25 (roster.json), and are cast members 2500851/3/4 of boss_scripts.json 50000000 (map m25_00_00_00). docs/HANDOFF.md:322-323 says the same. They are not Mibu (m15). The wrong label comes from tools/boss_arenas.py:59 and docs/kb/map.md:417-418 (see the open items).
- **Done:**
  - Phase events on her deathblows (AI commands, Event20010) (src/sim_tests.rs:590-608; docs/reports/enemies.md:61-63)
  - Mibu phantoms, messages, warp points, fight reset (docs/HANDOFF.md:322-345)
  - Clone fade in/out (docs/HANDOFF.md:369-371; src/sim_tests.rs:4968-5011)
  - Last deathblow is her Todome (src/sim_tests.rs:501-519; docs/reports/enemies.md:53-55; src/player.rs:527-533)
  - Own Fountainhead arena (map_boss_arenas.json:3; tools/boss_arenas.py:59)
  - Boss music by phase flags (boss_bgm.json:233; src/fmod.rs:882-892)
- **Open:**
  - The 20021 death fallback is described as done in docs/HANDOFF.md:291, :339-341 and docs/reports/enemies.md:118-121, and docs/HANDOFF.md:348 names the test the_corrupted_monk_without_the_todome_message_plays_20021. That test no longer exists. The current test asserts that 20021 does NOT play (src/sim_tests.rs:521-551), because ForceAnim is not applied on a dead boss (src/enemy/script.rs:1169-1173). The docs are stale.
  - Player message 710200 from 12505963 fires into nothing (docs/reports/enemies.md:108-113)
  - Phantom handling gap (src/enemy/script.rs:172-177); short-warp gap (src/enemy/script.rs:1023-1026)
  - TAE 110 SpawnFFX_110 ids 2/3/5 on c5000 are not played (docs/HANDOFF.md:631-635)
  - a000 placeholder animation lookup gap (src/actor.rs:212-215)
  - AI region queries return false (src/ai.rs:726-728). 500000_battle.lua tests regions 9622850, 2502853, 2502868, 2512853, 9622851, 2502854, 2512854, 2502869 (extracted/ai_src/maps/500000_battle.lua:61-108) and IsInsideObserve (:1405).
  - Ignored actions in this script: 2004[30], 2003[59], 2004[65], 2008[4], 2004[39] x24 (boss_scripts.json:1; src/enemy/script.rs:729)
  - Arena labels are swapped in the docs: tools/boss_arenas.py:59 and docs/kb/map.md:417-418 call boss_50000000 the Mibu phantom spots, but its map is m25 Fountainhead (map_boss_50000000.json:1719)
- **Key files:** `src/enemy/script.rs`, `src/enemy.rs`, `src/sim_tests.rs`, `docs/HANDOFF.md`, `docs/kb/enemy.md`, `extracted/enemies/boss_scripts.json`, `extracted/enemies/boss_events.json`, `extracted/map_boss_50000000.json`, `extracted/ai_src/maps/500000_battle.lua`

### 50001000 Corrupted Monk (Mibu Village, the true Monk on the bridge)

- **chr / map:** c5000 (roster.json:8503, row 50001000 at :8530, deathblows 1), m15 Mibu Village (npc_names.json:8, :50; map_boss_50001000.json:1409)
- **Setup:** Own arena boss_50001000 (map_boss_arenas.json:4; OWN_ROWS tools/boss_arenas.py:59), map m15 (map_boss_50001000.json:1409). Boss script: yes, 4 events 11505800/810/812/814, boss 1500800, 1 char, no extra cast (boss_scripts.json:1). Decoded events: 11505810 holds a forced 21000 loop with AI off and immortal until Wolf is within 20 m, then plays 20005 and shows bar 905000; 11505812 plays EzState 20000 on Wolf message 10; 11505800 is the defeat. No phase rules (no boss_events.json key; src/enemy.rs:1405-1407). Not c5000's default row; reached through the picker / SHINOBI_FIGHT=c5000:50001000 (src/menu.rs:745-751; src/data.rs:1409-1478). Music m150000001 (boss_bgm.json:163).
- **Status:** works
- **Tests:** No named test. all_enemies_full runs it as a non-default boss-script row (src/sim_tests.rs:3724-3743; docs/reports/enemies.md:125-127, which calls it the 'Mibu Monk 50001000'). c5000:50001000 is not in the failing lists (docs/HANDOFF.md:385-386, :539-541). Behaviour beyond 'fights and dies' is untested.
- **Correction (completeness check):** Corrupted Monk (Mibu Village, the true Monk on the bridge). roster.json c5000 placement 50001000 is 'Hakai Monk Sick Village', map m15_00_00_00, deathblows 1 (one bar = the illusion). The true form with 3 deathblows is 50000000 in m25 (roster.json c5000 placement 50000000, deathblows 3). The clip index agrees: docs/kb/clips.md:93 'Corrupted Monk illusion, Mibu | one' vs docs/kb/clips.md:103 'Corrupted Monk, true form, Fountainhead bridge ... three'. The arena is m15: extracted/map_boss_50001000.json "map" = m15_00_00_00.
- **Done:**
  - Own Mibu arena (map_boss_arenas.json:4)
  - Script wake-up (21000 loop until Wolf comes within 20 m) and bar (boss_scripts.json:1, event 11505810)
  - Harness coverage in all_enemies_full (src/sim_tests.rs:3724-3743)
- **Open:**
  - Needs a named test for the 20 m wake-up and the 20005 / 20000 transitions (src/sim_tests.rs has none; boss_scripts.json:1 events 11505810/11505812)
  - Forced-crouch region 1502600 sits 57 m from this Monk (docs/kb/enemy.md:105)
  - AI region 1502810 returns false (extracted/ai_src/maps/500000_battle.lua:84; src/ai.rs:726-728)
  - Ignored actions: 2005[7], 2005[3], 2004[34], 2004[39], 2003[11] (boss_scripts.json:1; src/enemy/script.rs:729)
  - Arena labels are swapped: docs/HANDOFF.md:436 and docs/kb/map.md:381 call boss_50001000 'the Fountainhead bridge', and tools/boss_arenas.py:59 and docs/kb/map.md:417-418 swap the labels too. Its map is m15 Mibu (map_boss_50001000.json:1409). The clip notes on the illusion vs the true Monk are at docs/kb/clips.md:93, :103.
- **Key files:** `src/enemy/script.rs`, `src/sim_tests.rs`, `tools/boss_arenas.py`, `extracted/enemies/boss_scripts.json`, `extracted/map_boss_50001000.json`, `docs/kb/map.md`

### 70000000 O'Rin of the Water

- **chr / map:** c7000 (roster.json:9380; row 70000000, hp 1434, deathblows 2), m15 Mibu Village (npc_names.json:8, :64)
- **Setup:** No boss script (no 70000000 key in boss_scripts.json:1). The exporter only takes bosses that are targets of 2003[11] Display Boss Health Bar (tools/boss_scripts.py:345), so it has no arena entry (map_boss_arenas.json has none) and the fight runs on the config [world] map (src/map.rs:1249; config.toml:94 map = "m11_01_00_00"), not Mibu. Phase rules: one rule, event 11505690, bars 0, other 1, action 'clear 4800' (boss_events.json:1530-1540). run_phase_events skips rules with bars 0 / other > 0 (src/enemy.rs:391-421), so it does not run; 4800 is only 'Medium Boss_No Money & Experience' (SpEffectParam.json:166548-166701). No music entry in boss_bgm.json. Single enemy, no cast. AI 700000_battle.lua has no region calls, so the region stub does not affect it.
- **Status:** partial
- **Tests:** No named test. Smoke runs c7000's default row 70000000 (roster.json default; src/sim_tests.rs:3441-3567). c7000 is not in the failing lists (docs/HANDOFF.md:385-386, :539-541), so it fights and dies in the harness. Last recorded: smoke 102/102, full 95/110 (docs/HANDOFF.md:539-541).
- **Done:**
  - Fights through its own Lua AI (extracted/ai_src/maps/700000_battle.lua, no region calls)
  - Passes the smoke harness (docs/HANDOFF.md:539-541)
  - Clips and deathblow group documented (docs/kb/clips.md:92, :157, :277; docs/deathblow_groups.md:65 a214)
- **Open:**
  - No boss script or arena: tools/boss_scripts.py:345 only exports bosses with a 2003[11] health-bar target, so her real fight setup (Mibu, start flags, bar) is missing; she fights on the config map (src/map.rs:1249; config.toml:94)
  - No Todome: c7000 is not in the Todome list (docs/reports/enemies.md:53-55; src/player.rs:527-533, gap :532-533)
  - No boss music (boss_bgm.json has no 70000000 entry; src/fmod.rs:668-675)
  - Phase rule 11505690 is skipped (src/enemy.rs:391-421; boss_events.json:1540); whether this matters is unknown
- **Key files:** `tools/boss_scripts.py`, `src/map.rs`, `src/enemy.rs`, `extracted/enemies/boss_events.json`, `extracted/ai_src/maps/700000_battle.lua`, `docs/kb/clips.md`

### 50200010 Chained Ogre (Ashina Castle, the 'Ninja Army Invasion (Indiscriminate)' placement)

- **chr / map:** c5020 (roster.json:8681; default row 50200010 at :8695, think 50201000, deathblows 2), m11_01 Ashina Castle (roster.json:8695; npc_names.json:5, :51)
- **Setup:** No boss script (no key in boss_scripts.json:1; exporter rule tools/boss_scripts.py:345) and no arena entry (map_boss_arenas.json has none), so the fight runs on the config [world] map m11_01_00_00 (src/map.rs:1249; config.toml:94). No phase rules (no boss_events.json key). Team 29 = this indiscriminate row; it is hostile to all (src/combat.rs:744-756; docs/HANDOFF.md:372-375). Wire throws 15020170/71 open the grappling-hook window (src/data.rs:1008-1014; src/enemy/grapple.rs:1-8; docs/kb/grapple.md:136). No music entry. Single enemy.
- **Status:** partial
- **Tests:** Named tests run app_with("c5020"), i.e. this default row: the_ogres_grab_opens_the_grapple_in_its_window (src/sim_tests.rs:5812-5844), the_hook_in_the_ogres_grab_staggers_him (src/sim_tests.rs:5846-5868), the_grappling_hook_attack_throws_the_ogre (src/sim_tests.rs:6114-6163). Smoke covers c5020. It is not in the failing lists (docs/HANDOFF.md:385-386, :539-541). Last recorded: 198 tests pass on 2026-10-11 (docs/HANDOFF.md:102).
- **Done:**
  - Grab / grapple window and Grappling Hook Attack (docs/HANDOFF.md:454-457; src/sim_tests.rs:5812-5868, :6114-6163)
  - Hitbox matches the game's clip (docs/kb/clips.md:194-230)
  - Team 29 indiscriminate hostility (src/combat.rs:744-756)
- **Open:**
  - Hostility gap for team 29 (src/combat.rs:749-750)
  - No boss script / arena / start setup, because it has no 2003[11] target (tools/boss_scripts.py:345); it fights on the config map (src/map.rs:1249)
  - No Todome row: c5020 is not in the Todome list (docs/reports/enemies.md:53-55; src/player.rs:527-533)
  - AI branch on region 1102318 (TARGET_SELF) can never be true, so acts 45-47 are never chosen (extracted/ai_src/maps/502000_battle.lua:35, :48; src/ai.rs:726-728)
  - Hair cloth has an empty constraintExecution (src/cloth.rs:173-176)
  - Docs mislabel c1040 (the Long-arm Centipede) as the Ogre: the 'Chained Ogre's fur blow-up' fix (docs/HANDOFF.md:408, :768-774), docs/kb/visuals.md:131-153 and docs/kb/damage.md:246 are about c1040, not c5020 (docs/kb/clips.md:221-222; docs/PLAN.md:36)
  - No boss music (boss_bgm.json has no entry)
- **Key files:** `src/enemy/grapple.rs`, `src/data.rs`, `src/combat.rs`, `src/cloth.rs`, `src/sim_tests.rs`, `extracted/ai_src/maps/502000_battle.lua`, `docs/kb/grapple.md`, `docs/kb/clips.md`

### 50202000 Chained Ogre (Ashina Outskirts, the 'Castle_VP (Red Eye)' placement)

- **chr / map:** c5020 (roster.json:8681; row 50202000 at :8708, think 50200010, deathblows 2), m11_00 Ashina Outskirts (roster.json:8708; npc_names.json:4, :52)
- **Setup:** No boss script (no key in boss_scripts.json:1; tools/boss_scripts.py:345) and no arena entry (map_boss_arenas.json), so it would run on the config map m11_01_00_00 (src/map.rs:1249; config.toml:94), not m11_00. No phase rules, no music. Its think id 50200010 differs from the default row's 50201000 (roster.json:8695, :8708). Reachable with SHINOBI_FIGHT=c5020:50202000 (src/menu.rs:745-751). Not verified whether the picker lists it (src/data.rs:1409-1478, shared names get a place suffix :1472-1477).
- **Status:** unknown
- **Tests:** None. Every c5020 test uses the default row 50200010 (src/sim_tests.rs:26-40, :5812-5868, :6114-6163). Smoke runs only default rows (src/sim_tests.rs:3450-3460). all_enemies_full adds only boss-script rows (src/sim_tests.rs:3724-3743), and this row has no script. So no run of 50202000 is recorded anywhere.
- **Done:**
  - Row data in roster.json (roster.json:8708) and npc_names.json:52
- **Open:**
  - Never run: add a test or a harness case for c5020:50202000 (src/sim_tests.rs:3724-3743)
  - Different think id (50200010) than the tested row; whether its AI differs in practice is unknown (roster.json:8695, :8708)
  - No arena: it fights on the config map, not m11_00 (src/map.rs:1249)
  - Same gaps as 50200010: no Todome (docs/reports/enemies.md:53-55), cloth constraint gap (src/cloth.rs:173-176)
- **Key files:** `extracted/enemies/roster.json`, `src/sim_tests.rs`, `src/map.rs`, `src/menu.rs`, `src/data.rs`

### 50800000 Gyoubu Oniwa

- **chr / map:** c5080 (roster.json:8961; row 50800000, deathblows 2), m11_00 Ashina Outskirts (roster.json:8961; npc_names.json:4, :55)
- **Setup:** Shared arena boss_54000000, Isshin's flower field m11_02 (map_boss_arenas.json:7; tools/boss_arenas.py:109-110; map_boss_54000000.json:5, :3330). This is a stranger arena, so the line-up stands 8 m ahead (src/enemy.rs:877-882), place warps are skipped (src/enemy/script.rs:992-1003), and region heights are ignored (src/enemy/script.rs:368-378). Boss script: yes, 11 events, boss 1100800, flags_on [1559] (boss_scripts.json:1): 11105842, 800, 810 (start point 6), 812, 820, 830, 840, 841, 845, 20006002, 20005420. Decoded: 11105810 turns the AI on, makes him immortal, sends AI command 1, waits for message 20 + 4 s, then sets 11105801 and shows bar 905081; 11105812 sets 11105802 at <=1 bar + 3 s; 11105820 turns immortality on at message 50 and off at message 10; 11105842 applies SpEffects 5305/5306 on messages 70/80; 11105845 applies SpEffect 3508530 by regions 1102845/846/847. No phase rules (scripted; src/enemy.rs:1405-1407). Music: m110000001 from 11105801, then m110000002 at 11105802 (boss_bgm.json:34).
- **Status:** works
- **Tests:** Named: the_grappling_hook_attack_todome_on_gyoubus_last_bar (src/sim_tests.rs:6165-6205). Smoke covers c5080's default row 50800000 (roster.json:8961). c5080 is not in the failing lists (docs/HANDOFF.md:385-386, :539-541). Last recorded: 198 tests pass on 2026-10-11 (docs/HANDOFF.md:102).
- **Done:**
  - Script runs on the shared flower field (map_boss_arenas.json:7)
  - Grappling Hook Attack and its Todome 15080179 on the last bar (src/player.rs:1745-1780; src/sim_tests.rs:6165-6205; docs/HANDOFF.md:454-457)
  - Deathblow on his '崩しトドメ' rows: 0009/0119 map to ThrowDef12090 (src/player.rs:543-548); immortality makes an exception for it via SpEffect 201000 (src/enemy/script.rs:1099-1125)
  - Wire throws 15080170/71/79 (src/data.rs:1008-1014; src/enemy/grapple.rs:1-8; docs/kb/grapple.md:138)
  - Phase music (boss_bgm.json:34; src/fmod.rs:882-892)
- **Open:**
  - Immortality / ThrowDef12090 gap (src/enemy/script.rs:1104-1105)
  - Grappling Hook Todome gap (src/player.rs:1754-1756); he has no ThrowDefDeath (src/player.rs:1760-1761)
  - Horse LeaveTarget gap (src/ai.rs:936-941, gap :939-940); EnableRangeMin 6 is greater than Max 4 (src/actor.rs:386-387)
  - AI IsInsideObserve(0/1/4) always returns false (extracted/ai_src/maps/508000_battle.lua:1172-1182; src/ai.rs:726-728)
  - Ignored actions: 2008[4] x7, 2005[19], 2007[14]/[16], 2003[62], 2004[29/34/35/50] (boss_scripts.json:1; src/enemy/script.rs:729)
  - He fights on the shared flower field, not his own m11_00 field (map_boss_arenas.json:7). Whether regions 1102845/846/847 (event 11105845) still fall where he fights is unknown (src/enemy/script.rs:368-378).
- **Key files:** `src/enemy/script.rs`, `src/player.rs`, `src/ai.rs`, `src/actor.rs`, `src/enemy/grapple.rs`, `src/sim_tests.rs`, `extracted/enemies/boss_scripts.json`, `extracted/ai_src/maps/508000_battle.lua`

### 70200000 Demon of Hatred

- **chr / map:** c7020 (roster.json:9443; row 70200000, deathblows 3) + c1001 'Dummy character for demon Buddha' 1100955 (boss_scripts.json:1), m11_00 Ashina Outskirts (roster.json:9443; npc_names.json:4, :65)
- **Setup:** Shared arena boss_54000000, Isshin's flower field m11_02 (map_boss_arenas.json:16; tools/boss_arenas.py:109-110), with the stranger-arena rules (src/enemy.rs:877-882; src/enemy/script.rs:368-378, :992-1003). Boss script: yes, 13 events, boss 1100900, flags_on [8302, 9301] (boss_scripts.json:1): 11105900, 910 (start point 9), 20006041, 20005500, 912, 920-924, 970-972. Decoded: 11105910 sends AI command 10, applies SpEffects 300320/300330/3702006, sets team 28, and shows bar 907021 by region 1102910; 11105912 is the phase event; 11105920 turns immortality off at message 10; 11105921 at 0 bars + 3702050 + Wolf message 10 plays EzState 20200 and Wolf message 710207; 11105922 plays 21000 at message 20; 11105923/24 handle team and AI. boss_events.json rules for 70200000 (:1551, 11105912 at :1577, 11105921 at :1596) are not used because the row is scripted (src/enemy.rs:1405-1407). Music m110000007/008 (boss_bgm.json:50).
- **Status:** works
- **Tests:** Named: the_demon_of_hatreds_last_phase_swaps_its_sp_effects (src/sim_tests.rs:610-634), on the default row. Smoke covers c7020. It is not in the failing lists (docs/HANDOFF.md:385-386, :539-541). docs/HANDOFF.md:286 says the Demon is tested; docs/reports/enemies.md:81-82 lists it as passing. Last recorded: 198 tests pass on 2026-10-11 (docs/HANDOFF.md:102).
- **Done:**
  - Last-phase SpEffect swap: 3702005 set, 277020/021 cleared, 277022/023 set (src/sim_tests.rs:610-634; docs/reports/enemies.md:64-71)
  - Todome with player message 710207 (docs/reports/enemies.md:53-55, :108-113)
  - Mane cloth (src/cloth.rs:414-420, :594-599; docs/kb/visuals.md:174)
  - Phase music (boss_bgm.json:50)
  - Wire SpEffects documented (docs/kb/grapple.md:143)
- **Open:**
  - Unknown condition 11[02] in 11105910 sits at index 4, before its start point 9, so it is never reached; the effect of the skipped part is unknown (boss_scripts.json:1; src/enemy/script.rs:506-508)
  - HU1/HU2 SpEffects are a no-op (docs/reports/enemies.md:64-71)
  - Team 28, then 6 within 30 frames, set by 11105910; 2004[2] team type has no effect (docs/kb/enemy.md:239-244)
  - Ember GPU particles are not drawn (docs/HANDOFF.md:490; docs/kb/visuals.md:324-326)
  - Fights on the shared flower field, not his own m11_00 site (map_boss_arenas.json:16); region 1102910 for the bar is checked without heights there (src/enemy/script.rs:368-378)
  - DAMAGE_TRANSITION_RANK__4 gap in the wire throws / hook (docs/kb/enemy.md:264)
- **Key files:** `src/enemy/script.rs`, `src/enemy.rs`, `src/cloth.rs`, `src/sim_tests.rs`, `extracted/enemies/boss_scripts.json`, `extracted/enemies/boss_events.json`, `docs/reports/enemies.md`, `docs/kb/visuals.md`

**Notes for this group:**
- Handover: all 57 npc_names.json boss rows now belong to the outside contributor. The per-boss sheet is meant to be docs/reports/bosses-handover.md, which does not exist yet (docs/HANDOFF.md:6-7, :40-49).
- Script selection: a boss script is picked by the main enemy's NpcParam row (src/enemy/script.rs:147-161). The exporter takes bosses only from 2003[11] Display Boss Health Bar targets (tools/boss_scripts.py:345), so O'Rin (70000000) and both Ogre rows have no script.
- Unknown script conditions are answered false (src/enemy/script.rs:506-508); unknown actions are ignored (src/enemy/script.rs:729). Declared gaps: map objects, SFX, cameras, lock-on points, NPC parts, gravity / hit masks, draw masks (src/enemy/script.rs:21-22). SetSp adds a resident SpEffect only, with no HP change (src/enemy/script.rs:1132-1139). ForceAnim is not applied on a dead boss (src/enemy/script.rs:1169-1173). EzState rule gap at src/enemy/script.rs:1183-1186.
- Script lifecycle: cast never enabled is spawned at start (src/enemy/script.rs:793-803); auto-restart 5 s after defeat (src/enemy/script.rs:823-831); dead without a defeat event resets after 15 s (src/enemy/script.rs:901-912).
- Phase rules from boss_events.json run only for rows WITHOUT a boss script (src/enemy.rs:1405-1407). Rules with bars 0 or 'other' > 0 are skipped (src/enemy.rs:391-421).
- Arenas: OWN_ROWS get their own export (tools/boss_arenas.py:56-70); every other scripted row maps to boss_54000000, Isshin's flower field m11_02 (tools/boss_arenas.py:106-111; map_boss_54000000.json:3330); MOVED rows go to the castle top (tools/boss_arenas.py:79). Rows without a script get no index entry and fall back to config [world] map (src/map.rs:1249; config.toml:94 'm11_01_00_00').
- Stranger arena (not the boss's own): the line-up stands 8 m ahead (src/enemy.rs:842-882), place warps are skipped (src/enemy/script.rs:992-1003), region heights are ignored (src/enemy/script.rs:368-378).
- Lua AI runs a real 5.0 VM (THIRD_PARTY.md:5-7), but IsInsideTargetRegion / IsInsideObserve / GetAreaObserveSlot always return false (src/ai.rs:726-728). This hurts the monkeys (126000), the Monk (500000), the Ogre (502000) and Gyoubu (508000); O'Rin (700000) and the Demon (702000) make no region calls.
- Harnesses: all_enemies_smoke runs each roster chr's default row (src/sim_tests.rs:3441-3567, default rows :3450-3460), with NON_FIGHTERS standing (src/sim_tests.rs:3439, :3536-3540). all_enemies_full adds boss-script rows that are not the default and not brought in by another script (src/sim_tests.rs:3713-3866, :3724-3743). Rows without a script that are not the default (50202000) are not run by either.
- Latest recorded results: smoke 102/102 and full 95/110, all but the 15 non-fighters (docs/HANDOFF.md:539-541; docs/PLAN.md:80). The older list of 16 failing includes c1260 (docs/HANDOFF.md:385-386). 198 tests pass, 33 ignored, on 2026-10-11 (docs/HANDOFF.md:102). No later run is recorded; this session ran nothing.
- Single fights: SHINOBI_FIGHT=<chr>:<row> (src/menu.rs:745-751; docs/HANDOFF.md:566); encounter picker at src/data.rs:1409-1478.
- Boss music: boss_bgm.json read by src/fmod.rs; the first track starts with the fight, with no intros (src/fmod.rs:668-675); test boss_music_follows_the_phase_flags (src/fmod.rs:882-892). There are no entries for 70000000, 50200010 or 50202000.
- Todome: rows from ThrowParam suffix 180/181 (src/player.rs:527-533, gap :532-533). The Todome list is c5000, c5060, c5100, c5400, c5430, c7020, c7110 (docs/reports/enemies.md:53-55), so c7000 and c5020 have none. A Todome message is played only for unscripted rows (src/enemy.rs:739-775, :764).
- A boss gets up (Rising) after a non-final deathblow (src/enemy.rs:346-363); fallback_death handles the rest (src/enemy.rs:468-501).
- Stale or wrong docs: Monk arena labels are swapped (tools/boss_arenas.py:59; docs/kb/map.md:381, :417-418; docs/HANDOFF.md:436), while the data says boss_50000000 = m25 (map_boss_50000000.json:1719) and boss_50001000 = m15 (map_boss_50001000.json:1409). The 20021 fallback is called done (docs/HANDOFF.md:291, :339-341, :348; docs/reports/enemies.md:118-121), but the test asserts the opposite (src/sim_tests.rs:521-551). 'Each boss fights in its own real arena' is stale (docs/reports/enemies.md:134). 'Ogre c1040' appears in docs/HANDOFF.md:408 and docs/kb/damage.md:246, but c1040 is the Long-arm Centipede (docs/kb/clips.md:221-222; docs/PLAN.md:36).
- Deathblow animation groups for these chrs: c1250 (docs/deathblow_groups.md:50), c1260 (:51), c7000 a214 (:65), c5000 a211 (:102), c5020 a213/a201 (:108), c5080 a225 (:132), c7020 a240 (:168).

### 50600000 Great Shinobi - Owl (npc_names.json:53)

- **chr / map:** c5060, m11_01_00_00 (boss_scripts.json key 50600000 'map'; boss entity 1110850)
- **Setup:** Arena: MOVED onto Genichiro's castle top boss_71001000 (tools/boss_arenas.py:71-79, map_boss_arenas.json:5 and 22-23), because his own spot put the start outside the railing (boss_arenas.py:76-78). On that arena he is a 'stranger', so his script's CutsceneWarp/Warp places are skipped (src/enemy/script.rs:992-1003; boss_arenas.py:128-130). Lighting: castle draw-param variant 0750 at 18 h (src/map.rs:1207-1213, line 1209). Boss script: yes, m11_01 events 11115860, 11115930, 11115870-11115873, 11110100, 11115720, 11115850, flags_on 6911/8301/8302/9303/9316/11110826/11115900 (boss_scripts.json key 50600000). Defeat: 2003[12] in 11115850 (script.rs:715). Phase 2: 11115870 sets 200051 and requests EzState 506000; 11115871 is the Todome (EzState 20200 on him, 710201 on Wolf). Cast: only himself in 'chars' (1110850). boss_events.json:1334 has rules for the row, but scripted rows skip phase rules (src/enemy.rs:1405-1407).
- **Status:** works
- **Tests:** all_enemies_full runs c5060 with its roster default row 50600000 (sim_tests.rs:3715-3743). It is not in the failing list (HANDOFF.md:385-386), and the later count was 95/110 with the 15 fails not named (HANDOFF.md:539-541). Named test the_owl_has_no_deathblow_during_his_heat_up (sim_tests.rs:5888-5902) checks that a000_020005 at 2 s gives no deathblow. all_enemies_smoke is 102/102 (HANDOFF.md:539-541). I did not run any test (read-only task).
- **Done:**
  - Moved to the castle top with the start fixed (boss_arenas.py:76-79)
  - Fight lighting 0750 / 18 h (map.rs:1209)
  - Script warps skipped on a stranger's arena (script.rs:992-1003)
  - No deathblow during the phase-2 heat-up (TAE marker 3506080) (player.rs:484-490; kb/enemy.md:270-274)
  - Todome (ThrowParam 180/181) row list includes c5060 (player.rs:528-534)
  - His AI reads 200051 / 3506080 (extracted/ai_src/maps/506000_battle.lua:34-36, 81, 986-990)
- **Open:**
  - EzState 506000, requested by 11115870 (boss_scripts.json key 50600000), has no state or a000_506000 clip in extracted/enemies/c5060.json, so it does nothing. The EzState handler is at script.rs:1183-1214.
  - The Todome's Wolf EzState 710201 (11115871) has no state in c0000.hkx and does nothing, which is also true in the game (docs/reports/enemies.md:108-113)
  - gap: the exe's test of the 3506080 marker is not traced (player.rs:487)
  - gap: how the exe picks the Todome rows is not traced (player.rs:532)
  - The script enables or disables entities that are not in its chars (1110722 in 11115860; 1110920 in 11115930), and 11110100 / 11115720 are cutscene events of other castle fights (boss_scripts.json key 50600000). Their effect in Sv1 is not checked (unknown).
  - The castle-top look is still flatter and cooler than the game's (HANDOFF.md:425-428)
- **Key files:** `tools/boss_arenas.py`, `src/enemy/script.rs`, `src/player.rs`, `src/map.rs`, `src/sim_tests.rs`, `extracted/enemies/boss_scripts.json`, `extracted/enemies/c5060.json`, `extracted/ai_src/maps/506000_battle.lua`, `docs/kb/enemy.md`

### 50601010 Owl (Father) (npc_names.json:54)

- **chr / map:** c5060 (cast: c5070 NpcParam 50700000), m10_00_00_00 (Hirata Estate; boss_scripts.json key 50601010, boss entity 1000900)
- **Setup:** Arena: the shared flower field boss_54000000, because the row is not in OWN_ROWS (boss_arenas.py:58-70, 108-111; map_boss_arenas.json:6). As a stranger there, its script warps are skipped (script.rs:992-1003). Boss script: yes, events 11005900, 11005910, 11005912, 11005913, 11005920-11005923, 11005930, 11005931, 11005935, 11005936, flags_on 8304/71000130/71000137 (boss_scripts.json key 50601010). Defeat: 2003[12] in 11005900 at ins 28. Phase events: 11005910 (force anims 401/20020/402, immortal on, bar); 11005921 (immortal off). Extra cast: 1000901 (c5070 npc 50700000) is enabled by 11005912 ins 10 and is warped and swapped with him by 11005922/23/31/35/36. Its Todome (11005920) is EzState 20200 on him and 710201 on Wolf. boss_events.json:1363 has rules for the row, which are skipped for a scripted row (enemy.rs:1405-1407). No FIGHT_LIGHTING entry (map.rs:1207-1213).
- **Status:** works
- **Tests:** all_enemies_full adds this row as its own case c5060:50601010 (sim_tests.rs:3721-3723 and 3729-3742; docs/reports/enemies.md:125-128). It is not in the failing list (HANDOFF.md:385-386). all_enemies_full does not require the script's defeat to fire (sim_tests.rs:3786-3852). No named test.
- **Done:**
  - Script row exported with the c5070 double as cast (boss_scripts.json key 50601010; HANDOFF.md:360-364)
  - 8304 is kept on because this fight needs it (tools/boss_scripts.py:215-220; HANDOFF.md:808-810)
  - Same phase-2 deathblow gate code as the Owl (player.rs:484-490)
- **Open:**
  - 11005900 and 11005910 each appear twice in boss_scripts.json key 50601010, both copies identical with slot 0. The cause in tools/boss_scripts.py is not checked, and the effect of running them twice in the interpreter is unknown.
  - The Todome's Wolf EzState 710201 (11005920 ins 7) fires into nothing (docs/reports/enemies.md:108-113)
  - The m10 places / regions are in the Hirata map's frame while the fight is on the flower field. Warps are skipped (script.rs:992-1003); region checks (3[2], script.rs:473-476) on that arena are not checked (unknown).
  - gap: player.rs:487 (marker test) and player.rs:532 (Todome rows), as for the Owl
- **Key files:** `extracted/enemies/boss_scripts.json`, `tools/boss_scripts.py`, `src/enemy/script.rs`, `src/sim_tests.rs`, `extracted/enemies/c5070.json`

### 50900000 Lady Butterfly (npc_names.json:56)

- **chr / map:** c5090, m10_00_00_00 (boss_scripts.json key 50900000, boss entity 1000800)
- **Setup:** Arena: own, boss_50900000 (OWN_ROWS boss_arenas.py:60; map_boss_arenas.json:8). Boss script: yes, 39 event entries (11005800, 11005810, 11005812, 20005420, 11005850-11005858, 11005820 x8, 11005870 x8, 11005860 x8, 11005880, 11005881), flags_on 6161/8304/9800-9803 (boss_scripts.json key 50900000). Cast: 1000810 (c5090 npc 50900001, the second body) and eight c1510 illusions 1000820-1000827 (npc 15100200/15100300/15100400). Phase change: the hand-over 1000800 -> 1000810 (HANDOFF.md:360-364; sim_tests.rs:3872-3887). Defeat: 11005800 ins 36 (2003[12] on 1000800).
- **Status:** works
- **Tests:** all_enemies_full runs c5090 with its default row 50900000 (sim_tests.rs:3715-3743), and the second body through hand_over (sim_tests.rs:3737-3740, 3872-3887). It is not in the failing list (HANDOFF.md:385-386). all_enemies_full does not require the defeat event (sim_tests.rs:3786-3852), so no test asserts her HandleBossDefeat.
- **Done:**
  - Own arena with the illusions' spots (boss_arenas.py:58-60)
  - Second-body hand-over (HANDOFF.md:360-364)
  - Defeat path under flag 8304 (by reading, not run): in 11005800, ins 4 (1003[101] GOTO label 0 if 9302 is OFF) comes before ins 14 (1003[2] END if 8304 is ON). 9302 is set by the event itself (ins 38), so it starts off, the END at ins 14 is skipped, and the event waits on 1000810 (201000 / HP 0 / dead, ins 21-24), then on both bodies HP 0 or dead (groups -1/-2, ins 26-32), waits 5.33 s (ins 34) and runs 2003[12] HandleBossDefeat on 1000800 (ins 36). 8304 only gates the reward branch (cond 5, ins 0-3). A body that died and was then disabled still counts as dead (script.rs:494-498). Other routes, if this one stalls: the 93xx defeat-flag rule (script.rs:681-697) and the 15 s dead-without-defeat reset (script.rs:901-912).
- **Open:**
  - 11005800 and 11005810 each appear twice, identical with slot 0 (boss_scripts.json key 50900000). 2003[12] could be queued twice, which is unknown and not checked.
  - The FLAGS_OFF note leaves 8304 on (boss_scripts.py:215-220; HANDOFF.md:808-810). By reading this does not block her defeat (see done), but no test confirms it.
  - gap: the exe's rule for the second anim set (a100) Todome / HU is not traced (player.rs:536-539)
  - gap: the exe's choice of Todome rows (player.rs:532)
- **Key files:** `extracted/enemies/boss_scripts.json`, `tools/boss_scripts.py`, `src/enemy/script.rs`, `src/sim_tests.rs`, `tools/boss_arenas.py`, `extracted/enemies/c5090.json`

### 50900001 Lady Butterfly, second body (npc_names.json:57)

- **chr / map:** c5090, m10_00_00_00 (boss_scripts.json key 50900001, boss entity 1000810)
- **Setup:** Arena: shares boss_50900000 (map_boss_arenas.json:9; the sharing rule is boss_arenas.py:112-122). Boss script: its own row with boss 1000810, the same events minus 11005810/11005856, flags_on 8304/9800-9803/11000801/11005801 (boss_scripts.json key 50900001). Cast: the eight c1510 illusions; 1000800 is not in its chars. Normally reached through the 50900000 hand-over. The picker lists every row (src/data.rs encounter_list, 1430+), so it can also be started alone. boss_events.json:1391 has rules for the row, which are skipped for a scripted row (enemy.rs:1405-1407).
- **Status:** partial
- **Tests:** No case of its own: all_enemies_full skips rows another script brings in (sim_tests.rs:3735-3740). It is only covered through the 50900000 hand-over (sim_tests.rs:3872-3887).
- **Done:**
  - Covered as the second phase of 50900000 via hand_over (HANDOFF.md:360-364)
- **Open:**
  - Started alone (picker), 11005800 can never finish (by reading, not run): group -1 (ins 26-27) needs 1000800 at HP 0 or dead, but 1000800 is not on the field and never died. 4[2] needs the character present (script.rs:499), and 4[0] needs it in dead_ids (script.rs:494-498). So no HandleBossDefeat fires, and the 15 s dead-without-defeat reset follows (script.rs:901-912; marked 'gap: no such rule in the game').
  - Same duplicate 11005800 entries as 50900000 (boss_scripts.json key 50900001)
- **Key files:** `extracted/enemies/boss_scripts.json`, `src/enemy/script.rs`, `src/data.rs`, `src/sim_tests.rs`

### 51000000 Guardian Ape (npc_names.json:58)

- **chr / map:** c5100 (cast: c5001 NpcParam 50011000), m17_00_00_00 (Sunken Valley; boss_scripts.json key 51000000, boss entity 1700800)
- **Setup:** Arena: own, boss_51000000, the valley's pond and grotto (OWN_ROWS boss_arenas.py:61; map_boss_arenas.json:10). Boss script: yes, 20 events 11705800-11705838, flags_on [8303] (boss_scripts.json key 51000000). Defeat: 11705800 ins 5-8 (msg 30 or dead, then 2003[12]). Immortality: on in 11705810 ins 12. Extra cast: 1700801 (c5001, roster name 'Buddhist monk's bug', roster.json c5001 entry), brought in only by 11705826 on msg 40 (short warp to the Ape's dummy 260, enable, anim 20001). boss_events.json:1422 has rules for the row, which are skipped for a scripted row (enemy.rs:1405-1407).
- **Status:** partial
- **Tests:** all_enemies_full runs c5100 with its default row 51000000 (sim_tests.rs:3715-3743), and it is not in the failing list (HANDOFF.md:385-386). encounters_carry_the_health_bar_names checks only its picker name (sim_tests.rs:5730-5744). No named fight test.
- **Done:**
  - Own arena; the start spot was moved out from under the grotto rock (map::player_start, HANDOFF.md:410-415)
  - Surfaces winding fix that lost the pond floor (HANDOFF.md:416-427)
  - Fur cloth drawn and its cost fixed (docs/kb/visuals.md:154-173, 202-217; HANDOFF.md:718)
  - Todome row list includes c5100 (player.rs:528-534)
  - Finisher window TAE 932 105 (data.rs:1157-1169)
  - Guard type 1 noted (docs/kb/guard-deflect.md:181)
- **Open:**
  - Flag 8303 is in flags_on (boss_scripts.json key 51000000), so 11705823, 11705824 and 11705826 END at their ins 0 (1003[2] END if 8303 is ON). Results: no msg-10 immortality-off (11705823 ins 3), no msg-50 immortal-on (11705824 ins 3), and the c5001 bug never comes in on msg 40 (11705826). 8303 is needed ON for the Headless fight (51000100's 11705860/11705880 ins 1 GOTO if 8303 is OFF), so the exporter's flags_on rule (boss_scripts.py:235-256) cannot tell the two fights apart. FLAGS_OFF has no entry for this row (boss_scripts.py:215-220), and 8303 is not mentioned in any doc.
  - The script.rs:1099-1101 comment says 11705823 ends the Ape's immortality, but with 8303 on it never runs. By reading, he stays immortal from 11705810 until a last deathblow whose reaction carries SpEffect 201000 (script.rs:1102-1114). Whether that changes his fight is unknown.
  - gap: the exe's immortality rule is not traced (script.rs:1104-1105)
  - gap: the fur cloths matched by name / count (docs/kb/visuals.md:172)
- **Key files:** `extracted/enemies/boss_scripts.json`, `tools/boss_scripts.py`, `src/enemy/script.rs`, `src/player.rs`, `src/map.rs`, `docs/kb/visuals.md`

### 51000100 Guardian Ape, headless fight (npc_names.json:59)

- **chr / map:** c5100 (cast: c5100 NpcParam 51000010, c5001 NpcParam 50011000), m17_00_00_00 (boss_scripts.json key 51000100, boss entity 1700850)
- **Setup:** Arena: own, boss_51000100, exported turned 180 degrees (MAP_CENTRE_YAW, boss_arenas.py:40-42 and 53; HANDOFF.md:436-440; map_boss_arenas.json:11). Boss script: yes, 16 events 11705850-11705886, flags_on [8303] (boss_scripts.json key 51000100). Defeat: 11705850 ins 9. Phase/cast: 11705860 sets him up (warp to 1702862, anims 21000/21005/20005, immortal, bar). 11705873 brings in the c5001 bug 1700851. 11705880 brings in the second ape 1700852 (c5100 npc 51000010) with AI commands 10/11. 11705881 is the second ape's Todome (EzState 20200, Wolf 710203). boss_events.json:1450 has rules for the row, which are skipped for a scripted row (enemy.rs:1405-1407).
- **Status:** works
- **Tests:** all_enemies_full adds it as case c5100:51000100 (sim_tests.rs:3721-3723; docs/reports/enemies.md:125-128), and it is not in the failing list (HANDOFF.md:385-386). encounters_carry_the_health_bar_names asserts its name differs from 51000000 (sim_tests.rs:5739). No named fight test.
- **Done:**
  - Arena turned so Wolf starts down the pit, with m13 textures unpacked (HANDOFF.md:436-440; boss_arenas.py:154-166)
  - Headless throws 0509 / 0619 -> ThrowDef12090 / 13290; their message 30 at 9.5 s is the defeat (player.rs:544-549)
  - Finisher window on a100_021000 (data.rs:1159-1160)
- **Open:**
  - The second ape's Todome Wolf EzState 710203 (11705881 ins 5) fires into nothing (docs/reports/enemies.md:108-113)
  - The arena is turned 180 degrees while the script's places and regions are in the map's own frame (warps 1702862, regions). Whether region checks and the 11705860 warp land right on the turned arena is not checked (unknown). Warps on an own arena are not skipped (script.rs:992-1003).
  - gap: player.rs:532 (Todome rows) and player.rs:537 (HU), shared with the Ape
- **Key files:** `tools/boss_arenas.py`, `extracted/enemies/boss_scripts.json`, `src/player.rs`, `src/enemy/script.rs`, `src/sim_tests.rs`

### 52000000 Divine Dragon (npc_names.json:60)

- **chr / map:** c5200 (cast: c5310 NpcParam 53100001 x3), m25_00_00_00 (Fountainhead Palace; boss_scripts.json key 52000000, boss entity 2500800)
- **Setup:** Arena: own, boss_52000000, the cloud floor h003100 24 m ahead of the MSB spot (MAP_CENTRE_DIST, boss_arenas.py:43-44 and 53), with a 3000 m far view (boss_arenas.py:36-38; HANDOFF.md:440-447; map_boss_arenas.json:12). Boss script: yes, 47 event entries (50, 12505801, 12505880-12505939, 12505882), flags_on 12505803-12505827 and 12505949 (boss_scripts.json key 52000000). Defeat flag 12505880. Cast: tentacles 2500880-2500882 (c5310), spawned at the start because the script only ever disables them (script.rs:785-810). No boss_events.json entry.
- **Status:** works
- **Tests:** Named tests: dragon_app plus the_divine_dragons_lightning_comes_back_at_it (sim_tests.rs:5272-5435), which asserts the script's defeat at 5427-5435; the_divine_dragons_warning_follows_wolf (sim_tests.rs:5786-5810); a_jump_in_an_updraft_is_the_storm_jump (sim_tests.rs:5226-5270). dragon_probe (sim_tests.rs:5437-5442) only runs with SHINOBI_DBG. dragon_app returns None without extracted/enemies/c5310.json (sim_tests.rs:5275-5278). all_enemies_full: c5200's default row 52000000 is not in the failing list (HANDOFF.md:385-386). docs/reports/enemies.md:127-128 says it has its own test.
- **Done:**
  - Lightning Reversal on its lightning, collapse 20002 -> loop 21000, finisher window TAE 932 104, ThrowParam 15200090 from dummy 230, defeat flag 12505880 (HANDOFF.md:355-359)
  - Shoot Bullet 2003[5] (enemy_bullet.rs ScriptShots; HANDOFF.md:355-356)
  - Cloud-sea arena, far view, blossoming tree textures (HANDOFF.md:440-447, 718-726, 763)
  - Grapple points on boss_52000000 (docs/kb/grapple.md:196-200)
- **Open:**
  - 12505801 blocks at ins 2, 3[24] (args 0, 9821, 2500806): the condition is not handled, and unknown ids are false (script.rs:504-508). So none of its intro or Old Dragons setup runs. The flags_on 12505803-12505827 / 12505949 make the fight jump straight into the dragon phase (via 12505885); the Old Dragons part of the game fight is skipped.
  - gap: what starts the 21000 loop in the game is not traced (enemy.rs:68-72)
  - gap: the finisher's diffAngMyToDef and Y range are not checked (player.rs:493-495)
  - gap: what TAE 932 104/105 is in the exe (data.rs:1163)
  - gaps at combat.rs:62-63 (env(336), PartBlend) and enemy_bullet.rs:19
  - The cloud shader is a stand-in (HANDOFF.md:442); a magenta rectangle on the horizon (HANDOFF.md:763-764); the blossoms' bloom-to-withered gradient is untouched (HANDOFF.md:789)
- **Key files:** `src/enemy/script.rs`, `src/enemy.rs`, `src/data.rs`, `src/player.rs`, `src/enemy_bullet.rs`, `src/sim_tests.rs`, `tools/boss_arenas.py`, `extracted/enemies/boss_scripts.json`, `docs/kb/map.md`

### 53000000 Old Dragons of the Tree (npc_names.json:61)

- **chr / map:** c5300, m25_00_00_00 (boss_scripts.json key 53000000, boss entity 2500810)
- **Setup:** Arena: own export boss_53000000 with the same settings as the Dragon (MAP_CENTRE_DIST 24, boss_arenas.py:53; map_boss_arenas.json:13). Boss script: yes, 63 event entries; it is the Divine Dragon's script plus 12505803 (x15) and 12505837, flags_on 12505818-12505827 and 12505949 (boss_scripts.json key 53000000). The boss entity 2500810 is the fight's shared-bar character; in the game 12505801 ins 39 sets its AI off (2004[1] 2500810 0). Cast in chars: 2500800 (Divine Dragon), 2500880-2500882 (tentacles) only. The c5300 roster default row is 53000010 'Okiryu_Combatant' (roster.json c5300 entry), so 53000000 runs as a separate boss-row case. No boss_events.json entry.
- **Status:** broken
- **Tests:** Fails all_enemies_full (HANDOFF.md:385-386; docs/reports/enemies.md:129-130 'likely unused: nothing in the scripts starts it', 142-143). The later 95/110 run (HANDOFF.md:539-541) names no fails, so whether it still fails there is unknown. c5300's combat rows pass (docs/reports/enemies.md:129).
- **Done:**
  - IsEventFlag returns a boolean (c5300 only circled before) (HANDOFF.md:365-366)
  - Anim-set SpEffect overwrite (c5300 looped 20001 before) (HANDOFF.md:366-368)
  - Arena exported (boss_arenas.py:62); grapple counts listed (docs/kb/grapple.md:200)
- **Open:**
  - 12505801 blocks at the 3[24] check (ins 2; script.rs:504-508), so ins 39 never turns 2500810's AI off, and the shared-bar dummy runs its AI as a fighter
  - The 15 slots of 12505803 refer to Old Dragon combatants that are not in the row's chars (5 entries), so they are never spawned (boss_scripts.json key 53000000)
  - Whether a real fight should exist for this row at all is not settled. The doc calls it 'likely unused' (docs/reports/enemies.md:130). That is unverified.
- **Key files:** `extracted/enemies/boss_scripts.json`, `src/enemy/script.rs`, `src/ai.rs`, `src/sim_tests.rs`, `docs/reports/enemies.md`

**Notes for this group:**
- Project root: C:\DeadlockModding\shinobi-combat. All findings are from reading only: nothing was built, run or edited. Line numbers are from 2026-10-12, and other sessions are editing docs/HANDOFF.md (its 4.11 section moved +14 lines since earlier reads).
- The handover sheet docs/reports/bosses-handover.md is named at HANDOFF.md:47-48 but does not exist (docs/reports holds README, S1-S3, V1-V2, enemies.md).
- Latest totals: 198 tests pass, 33 ignored (HANDOFF.md:102); all_enemies_full 94/110 with the 16 fails named (HANDOFF.md:385-386), later 95/110 with the fails not named (HANDOFF.md:539-541); all_enemies_smoke 102/102 (HANDOFF.md:539-541).
- all_enemies_full does not require a script's HandleBossDefeat (sim_tests.rs:3786-3852). It skips rows another script brings in (sim_tests.rs:3735-3740) and adds boss rows that are not a chr's default (sim_tests.rs:3721-3742).
- Scripted rows skip boss_events.json phase rules (enemy.rs:1405-1407) and the Todome-message fallback (enemy.rs:764). fallback_death (enemy.rs:1459) and final_events (enemy.rs:1469) have no scripted check; whether the boss_events.json rules of 50600000/50601010/50900001/51000000/51000100 (boss_events.json:1334, 1363, 1391, 1422, 1450) act there is not checked.
- The exporter takes every flag an event reads but never sets as ON (tools/boss_scripts.py:235-256, 380). FLAGS_OFF has only 12600050 (boss_scripts.py:215-220; HANDOFF.md:806-810). This hits 8304 (Butterfly, Owl Father) and 8303 (both Ape fights; wrong for 51000000).
- Unknown condition ids are false (script.rs:504-508), so an event waiting on one blocks forever (the Dragon's 12505801 at 3[24]).
- When the boss is dead and no defeat fired for 15 s, the duel resets (script.rs:901-912, 'gap: no such rule in the game').
- Cast the script only ever disables is spawned at the start (script.rs:785-810). Disabled characters are left out of the snapshot, and a death shows a frame late (script.rs:850-899). A missing character counts as dead only if it died (script.rs:494-498).
- Fights on a stranger's arena (shared field boss_54000000, or MOVED rows) skip CutsceneWarp and place warps (script.rs:992-1003; boss_arenas.py:128-130); cast keeps its height on own arenas (events.rs:27-34, enemy.rs:842-875).
- Arena rule (user, 2026-10-11): own arenas only for the fights whose mechanics live in their surroundings (boss_arenas.py:56-70); everyone else is on boss_54000000; the Owl is moved to boss_71001000 (boss_arenas.py:71-79; HANDOFF.md:428-434).
- Wolf's Todome EzStates 710201 / 710203 have no state in c0000.hkx and fire into nothing, as in the game (docs/reports/enemies.md:108-113). This affects the Owl, Owl (Father) and the headless fight's second ape.
- boss_scripts.json duplicates 11005800 / 11005810 (Butterfly rows) and 11005900 / 11005910 (Owl Father) as identical slot-0 entries; the cause and the effect are not checked.
- Boss phases, arenas, lighting, deathblows and clip checks belong to the outside contributor; Claude sessions keep the shared engine (HANDOFF.md:40-49).

### 71000000 Genichiro Ashina (prologue fight, Ashina Reservoir)

- **chr / map:** c7100, m11_02_00_00 Ashina Reservoir (extracted/enemies/boss_scripts.json key "71000000" .map; place name extracted/enemies/npc_names.json:6; roster placement "Rival Bowless Hidden Prison _ During Tutorial", extracted/enemies/roster.json:9493). It is c7100's default roster row, so config chr="c7100" with no npc_row gives this fight.
- **Setup:** Arena: the shared silvergrass field boss_54000000 (extracted/map_boss_arenas.json:17). Sv1 counts it as his own arena because the arena's map m11_02 equals his script's map and the row is not in the "moved" list (src/enemy.rs:861). The arena is centred on Isshin's part c5400_0000 (extracted/map_boss_54000000.json "centre"). Boss script: yes. Boss part 1120800, no cast (chars holds only 1120800). Events: 11125810 starts at instruction 85, his enable. 11125800 sets flag 9300 when he is at 0 bars, or when Wolf is at 1 HP in region 1122802. 11125812 makes Wolf and Genichiro immortal while Wolf is in 1122802 (boss_scripts.json "71000000".events). Phase events: none (boss_events.json has no "71000000" key). Scripted rows use script.rs instead of boss_events.json anyway (src/enemy.rs:1376, 1407). Lighting: no FIGHT_LIGHTING row, so the field's base draw params at the config hour (src/map.rs:1207-1213, 1300). Music: m110200001 (extracted/enemies/boss_bgm.json "71000000").
- **Status:** partial
- **Tests:** all_enemies_smoke c7100: OK (think 71000000, deathblow a227_510000 / ThrowDef12000, hit 0.8 s). Source: smoke.log:136 in another session's scratchpad (C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\b5798fc2-c742-4214-9108-534312f06bc6\scratchpad), 2026-10-11, 102 of 102; same total as docs/HANDOFF.md:539. all_enemies_full c7100 (this row): OK. Two deathblows, both ThrowDef12000, then DEAD (full.txt:137 in the same scratchpad; 95 of 110 at full.txt:170 = docs/HANDOFF.md:541). Named tests using this row through app_with("c7100"): a_boss_takes_one_deathblow_per_red_dot (src/sim_tests.rs:637) and a_boss_refills_at_its_reactions_hp_reset_frame (src/sim_tests.rs:5699). Sim tests load no map (src/sim_tests.rs:43-56), so nothing covers the arena.
- **Done:**
  - 2 deathblows, HP 1598, posture 660 (roster.json c7100 placements).
  - A non-final deathblow plays the ThrowDef reaction only. HP refills at the reaction's 300610 frame (src/enemy.rs:345-361, 1394-1402; docs/kb/enemy.md:191-196).
  - The last deathblow ends the fight: the script's flag 9300 with the boss at 0 bars becomes Handle Boss Defeat (src/enemy/script.rs:686-697; test comment src/sim_tests.rs:651-652).
  - Boss music track m110200001 (boss_bgm.json; src/fmod.rs:694).
- **Open:**
  - The scripted loss is not modelled. 11125812 sets Immortal on Wolf (10000), and 11125800 ends the fight when Wolf sits at 1 HP in region 1122802. script.rs only applies Immortal to the boss and cast in run.ents, so Wolf's is dropped (src/enemy/script.rs:1076-1079). Wolf simply dies and the fight resets.
  - Arena frame mismatch (worked out from the data, not checked in play). boss_54000000 is centred on Isshin's c5400_0000, not on Genichiro's part 1120800. Region 1122831's two parts are listed in both rows 71000000 and 71101000 (boss_scripts.json). Solving one for the other puts 1120800 about 4.9 m from Isshin's spot, turned about 176.5 degrees. Because on_own_arena is true (src/enemy.rs:861), his script's region checks and warps (for example 1122802) are applied as if he stood at the arena centre facing +Z.
  - No per-fight lighting: FIGHT_LIGHTING only has boss_71001000 rows (src/map.rs:1207-1213).
  - Cutscenes, SFX and cameras are taken as done (src/enemy/script.rs:21-22). Unknown conditions return false (src/enemy/script.rs:508).
  - Stale comments: src/enemy.rs:845-846 calls 71000000 "Way of Tomoe". src/enemy/script.rs:687-689 says "Genichiro at the castle top ... (m11_02 11125800)", but 11125800 is this reservoir row.
- **Key files:** `extracted/enemies/boss_scripts.json ("71000000")`, `extracted/map_boss_arenas.json:17`, `extracted/map_boss_54000000.json`, `src/enemy/script.rs`, `src/enemy.rs:845-876`, `src/map.rs:112-135`, `src/sim_tests.rs:637`, `src/sim_tests.rs:5699`, `extracted/enemies/c7100.json`

### 71001000 Genichiro Ashina (Ashina Castle top)

- **chr / map:** c7100, m11_01_00_00 Ashina Castle (boss_scripts.json "71001000" .map; npc_names.json:67; roster placement "Rival Honjo")
- **Setup:** Arena: his own castle top boss_71001000 (extracted/map_boss_arenas.json:18). Export settings: OWN_ROWS (tools/boss_arenas.py:58-66) and OVERRIDES MAP_SKIP_MODEL m015072 (burnt floor) plus MAP_GPARAM_EXTRA 0740,0750 (tools/boss_arenas.py:53-54). The arena is centred on c7100_0000, npc 71001000 (extracted/map_boss_71001000.json "centre"). It is his own arena, so warps and heights apply (src/enemy.rs:861). Boss script: yes. Boss part 1110800; cast is Way of Tomoe c7110 71100000 (part 1110801). Events:
- 11115810 starts at instruction 90: enables Genichiro and makes Genichiro and Tomoe immortal.
- 11115820: at his 0 bars it waits 2 s, waits for Wolf alive, plays cutscene 11010020 (Wolf warped to 1112813), disables Genichiro and enables Tomoe. It also has 2003[88] sub 10/30 and 2003[81] time of day 3.
- 11115830: Tomoe at 0 bars plus Wolf's message 10 gives EzState 20200 and Wolf 710204.
- 11115831, 11115406.
- 11115800: Tomoe's defeat (2003[12], then flag 9303).
Phase rules in boss_events.json:1612-1764 (11115820, three map variants) feed event_kinds, which loads the c7110 kind (src/enemy.rs:227-258; src/data.rs:1371-1380). Lighting: FIGHT_LIGHTING ("boss_71001000", 71001000, "0000", 12.0) (src/map.rs:1208). Music: m110100001, then m110100002 once 11115803/11115802 are on (boss_bgm.json; src/fmod.rs:883-894).
- **Status:** works
- **Tests:** all_enemies_full c7100:71001000: OK. Two ThrowDef12000, then '-> next' to Tomoe, Tomoe's ThrowDef13700 + Event20200, DEAD (full.txt:167 in scratchpad b5798fc2, 2026-10-11, 95/110 = docs/HANDOFF.md:541).
Named tests:
- genichiro_becomes_way_of_tomoe (src/sim_tests.rs:4929): swap, Wolf warp and reset back to Genichiro.
- spawn_encounter_swaps_the_fight_in_place (src/sim_tests.rs:5559): Tomoe kind loaded, script runs.
- encounters_carry_the_health_bar_names (src/sim_tests.rs:5731): "Genichiro Ashina (Ashina Castle)".
- boss_music_follows_the_phase_flags (src/fmod.rs:886).
all_enemies_smoke does not cover this row: it only runs default rows (src/sim_tests.rs:3459).
The castle floor and start are checked by every_arena_start_stands_and_walks (src/map.rs:1982).
- **Done:**
  - 2 deathblows, then the hand-over to Tomoe through his own script. Tomoe comes in at the MSB spot relative to Genichiro (src/enemy/script.rs:934-971; docs/kb/enemy.md:152-153).
  - Fight reset brings Genichiro back (docs/HANDOFF.md:336-339).
  - Castle-top arena exported. 66 fps with MAP_SHADOW_BELOW (docs/kb/map.md:226-231).
  - Per-fight draw params: base _0000 at 12 h (src/map.rs:1196-1208; docs/kb/map.md:442-461).
  - Two-track boss music.
  - SHINOBI_FIGHT=c7100:71001000 starts it through the picker swap (src/menu.rs:744-746).
- **Open:**
  - Mid-fight lighting does not change. In 11115820, 2003[88] (sub 10, and 30 for m11_00) and 2003[81] (time of day 3) have no arm in script.rs step and fall to `_ => {}` (src/enemy/script.rs:729). Lighting is picked once at map load (src/map.rs:1291-1300), so the Tomoe half keeps Genichiro's base 12 h light. extracted/param/drawparam/m11_01_0010.gparam and _0710.gparam exist, but the arena exports only the 0740 and 0750 variants (map_boss_71001000.json drawparam_variants).
  - The look is still flat, about 1 stop too bright, cool and low-contrast against the real frames (docs/kb/map.md:462-467; docs/HANDOFF.md:430). The exe's time-of-day to hour table is a gap (src/map.rs:1199-1200; docs/kb/map.md:452-455).
  - An encounter swap to another fight on the same arena keeps the old lighting: switch_map returns early when the arena is unchanged (src/map.rs:1250-1252).
  - Cutscenes 11010010/11/20 are taken as done (src/enemy/script.rs:21). Wolf's 710204 after Tomoe's Todome has no c0000.hkx state, and the report says it fires into nothing in the game too (docs/reports/enemies.md:110-112).
- **Key files:** `extracted/enemies/boss_scripts.json ("71001000")`, `extracted/enemies/boss_events.json:1612`, `extracted/map_boss_71001000.json`, `tools/boss_arenas.py:53-66`, `src/map.rs:1196-1218`, `src/enemy/script.rs`, `src/enemy/events.rs`, `src/sim_tests.rs:4929`, `src/fmod.rs:883-894`

### 71100000 Genichiro, Way of Tomoe (Ashina Castle)

- **chr / map:** c7110, m11_01_00_00 Ashina Castle (boss_scripts.json "71100000" .map; npc_names.json:68; roster "Rival_naked Honjo"). It is c7110's default roster row.
- **Setup:** Normally brought in mid-fight by 71001000's script (cast part 1110801). As its own encounter:
- Arena: boss_71001000, counted as his own (extracted/map_boss_arenas.json:19; tools/boss_arenas.py:63-66 "Way of Tomoe (71100000) shares it").
- Boss script: yes. Boss part 1110801, with the same six m11_01 events; 11115820 starts at instruction 44, his enable.
- boss_events.json:1765+: 11115800 (other=1, skipped by phase rules) and 11115830 (0 bars + msg 10, giving EzState 20200 and Wolf 710204).
- No cast.
- Lighting: FIGHT_LIGHTING ("boss_71001000", 71100000, "0000", 12.0) (src/map.rs:1212).
- Music: none. boss_bgm.json has no "71100000" key (src/fmod.rs:694).
- **Status:** works
- **Tests:** all_enemies_smoke c7110: OK (think 71100000, deathblow a228_511700 / ThrowDef13700; smoke.log:137, scratchpad b5798fc2, 2026-10-11). all_enemies_full c7110: OK, Todome ThrowDef13700 + Event20200, DEAD (full.txt:138). Also covered as the second body of c7100:71001000 (full.txt:167) and by genichiro_becomes_way_of_tomoe (src/sim_tests.rs:4929). lightning_caught_in_the_air_charges_wolf_and_an_attack_throws_it_back (src/sim_tests.rs:5160) uses app_with("c7110").
- **Done:**
  - 1 deathblow, the Todome (ThrowParam suffix 180/181; src/player.rs:528-531). HP 2556, posture 900 (roster.json c7110).
  - His blue lightning: AtkParam 71100361, spAttribute 10 (src/data.rs:292-294). Wolf gets charged in the air and the attack throws it back (test at src/sim_tests.rs:5160).
  - Death through his script (11115830 EzState 20200, 11115800 Handle Boss Defeat).
- **Open:**
  - Started on its own (config row 71100000) he has no boss music (src/fmod.rs:694; no key in boss_bgm.json).
  - His lighting row is the base _0000 at 12 h. In the game, 11115820 sets sub 10 and time of day 3 before he appears (docs/kb/map.md:447-448), and that variant is not exported (see 71001000).
  - Fragile start (from reading the code). In a 71100000 run, 11115810 begins at instruction 0 and, with 9303 off, disables Tomoe himself on the first frame (instruction 20). 11115820's enable at instruction 44 restores him only because both acts are applied in order in the same frame (src/enemy/script.rs:934-990).
  - Stale comment: src/sim_tests.rs:3737-3738 names "Tomoe 71101000" as the row checked through a hand-over. By the case selection, the row brought in by another script is 71100000, which also runs as c7110's default case.
- **Key files:** `extracted/enemies/boss_scripts.json ("71100000")`, `extracted/enemies/boss_events.json:1765`, `extracted/map_boss_arenas.json:19`, `src/map.rs:1212`, `src/fmod.rs:694`, `src/sim_tests.rs:5160`, `extracted/enemies/c7110.json`

### 71101000 Genichiro, Way of Tomoe (Ashina Reservoir, opens the Sword Saint fight)

- **chr / map:** c7110, m11_02_00_00 Ashina Reservoir (boss_scripts.json "71101000" .map; npc_names.json:69; roster "Rival _ Naked Hidden Prison _ When Tokugawa Invades")
- **Setup:** Arena: moved onto Genichiro's castle top boss_71001000 (extracted/map_boss_arenas.json:20 and moved list :22-28; tools/boss_arenas.py:71-79). There he is a stranger: script CutsceneWarps and Warps to places are skipped (src/enemy/script.rs:996, 1003), MSB heights are dropped (src/enemy/events.rs:32; src/enemy/script.rs:373), and the line-up stands 8 m from Wolf's start (src/enemy.rs:882).
Boss script: yes. Boss part 1120830; cast is Isshin c5400 54000000 (part 1120860). Events: 11125840 (start 69), 11125870, 11120110, 11125830, 11125860, 11125872-11125875, 11125733.
11125830 at Tomoe's 0 bars: waits 1.33 s, Wolf alive, 0.6 s; then enables Isshin, sets 9311 and disables Tomoe.
boss_events.json:1862+ has that rule plus 11125843 (map m11_02_71_10: msg 10 gives 20200 and Wolf 710204). 11125843 is not in the script's event list.
Lighting: no FIGHT_LIGHTING row, so the castle's base at the config hour. Music: m110200001 until 11120830.
- **Status:** partial
- **Tests:** all_enemies_full c7110:71101000: OK, but it only checks Tomoe: "blow [ThrowDef13700] left 1 DEAD", with no '-> next' to Isshin (full.txt:168, scratchpad b5798fc2, 2026-10-11). It is its own case because no other script lists it (src/sim_tests.rs:3730-3742). No test covers the Tomoe -> Isshin hand-over in this row. all_enemies_smoke does not run it (default rows only).
- **Done:**
  - Script runs. Tomoe fights and takes his Todome (full.txt:168).
  - Isshin is placed relative to Tomoe from the script's chars, which works on any arena.
  - Moved-fight handling: no warps off the platform (docs/kb/enemy.md:293-298).
- **Open:**
  - The Isshin follow-on is cut short (worked out from the code, matches full.txt:168). 11125830 sets 9311 in the same frame it enables Isshin, while the run's boss, Tomoe 1120830, is at 0 bars. script.rs turns any 93xx flag set with the boss at 0 bars into BossDefeat (src/enemy/script.rs:686-697; the comment at :690-691 only covers the 54000000 run). So run.defeated is set, and the fight restarts 5 s later with AutoRestart (src/enemy/script.rs:823-829) or shows the Won screen 4 s later (docs/HANDOFF.md:546).
  - Isshin's music never plays in this run: music is keyed to the run's row (src/fmod.rs:694), and 54000000's tracks are unused here.
  - No Event20200 for Tomoe. 11125843 (m11_02_71_10) is not in the script, and 11125830 disables Tomoe about 2 s into his Todome reaction (full.txt:168 shows only ThrowDef13700).
  - Fought on the castle top, not on the silvergrass field of his game map m11_02. No lighting row.
  - Stale docs: src/enemy.rs:845-846 and docs/kb/enemy.md:301 still list 71101000 as fighting on Isshin's field as his own arena.
- **Key files:** `extracted/enemies/boss_scripts.json ("71101000")`, `extracted/enemies/boss_events.json:1862`, `extracted/map_boss_arenas.json:20`, `tools/boss_arenas.py:71-79`, `src/enemy/script.rs:681-697`, `src/enemy/script.rs:823-829`, `src/sim_tests.rs:3715-3870`, `src/fmod.rs:690-700`

### 54000000 Isshin, the Sword Saint

- **chr / map:** c5400, m11_02_00_00 Ashina Reservoir (boss_scripts.json "54000000" .map; roster c5400 placements, roster.json:9253; npc_names.json:62)
- **Setup:** Arena: moved onto the castle top boss_71001000 (extracted/map_boss_arenas.json:14 and moved :24; tools/boss_arenas.py:79), so he is a stranger there (warps skipped, heights dropped: docs/kb/enemy.md:293-298). His own silvergrass field is still exported as boss_54000000 and is SHARED_ROW, the arena of the other bosses (tools/boss_arenas.py:80).
Boss script: yes. Boss part 1120860, no cast. Events:
- 11125870 starts at instruction 24: enable, AI, immortal, boss bar.
- 11125830 starts at instruction 10.
- 11120110.
- 11125860: the finisher's message 30, wait 5 s, 2003[74] defeat, flag 9312.
- 11125872: at 2 bars or fewer with 200031, sets AI ID 54000100 and flag 11125862 (plus 2003[88] sub 30).
- 11125873: immortal until message 20.
- 11125874: 0 bars, msg 10 and 11125862 give EzState 20200 and Wolf 710206.
- 11125875.
boss_events.json:1474-1500 (11125874).
Lighting: no FIGHT_LIGHTING row, so the castle base at the config hour (src/map.rs:1207-1213, 1300). Music: m110200002, then m110200003 at 11125862 (boss_bgm.json).
- **Status:** works
- **Tests:** all_enemies_smoke c5400: OK (Kensei_540000_Battle, deathblow a242_510000 / ThrowDef12000; smoke.log:129, scratchpad b5798fc2, 2026-10-11). all_enemies_full c5400: OK. Two ThrowDef12000, then ThrowDef13700 + Event20200, left 0, DEAD (full.txt:130).
Named tests:
- isshins_3015_switches_him_to_his_second_set (src/sim_tests.rs:439)
- isshins_todome_message_ends_the_fight_with_wolfs_event_710206 (src/sim_tests.rs:554)
- isshins_break_pose_leads_to_the_deathblow_after_his_change (src/sim_tests.rs:6286)
Results date from 2026-10-11, before the 2026-10-12 move to the castle top. Sim tests load no map.
- **Done:**
  - 3 deathblows, HP 2240, posture 660 (roster.json c5400).
  - a000_003015 switches him to the a100 anim set (200031) (src/enemy.rs:1378-1393).
  - HU deathblow rows +500 for the spear phase (src/player.rs:536-538). Todome rows 180/181 (src/player.rs:528-531).
  - Kneeling finisher: 15400590 gives ThrowDef12900, whose message 30 ends the fight (src/data.rs:1157-1163; src/enemy.rs:335-344).
  - AI ID switch to 54000100 through 11125872 (row exported in extracted/enemies/c5400.json params).
  - Posture-damage cut 3540090 x0.3 during his change (docs/kb/enemy.md:286-288).
  - Blue lightning 54000750, spAttribute 10 (src/data.rs:292-294).
- **Open:**
  - Fought on the castle top, not on his game map m11_02 (the user's call: docs/kb/map.md:428-441).
  - No per-fight lighting row. His own m11_02 draw-param changes (11125872 sub 30, 11125860 sub 20) are not run (src/enemy/script.rs:729).
  - FXR 654061 named by c5400/c5430 TAE is not in any sfxbnd of the game (docs/HANDOFF.md:632).
  - No test asserts the 11125872 AI switch. It only runs inside isshins_todome_message... (src/sim_tests.rs:557-559).
  - SpEffect 5350 (no damage anims) during his change is not modelled. How several posture SpEffects combine is a gap (docs/kb/enemy.md:286-288).
- **Key files:** `extracted/enemies/boss_scripts.json ("54000000")`, `extracted/enemies/boss_events.json:1474`, `extracted/map_boss_arenas.json:14`, `tools/boss_arenas.py:79-80`, `src/enemy.rs:330-400`, `src/player.rs:528-545`, `src/data.rs:1157-1163`, `src/sim_tests.rs:439`, `src/sim_tests.rs:554`, `src/sim_tests.rs:6286`, `extracted/enemies/c5400.json`

### 54300000 Isshin Ashina (Shura ending)

- **chr / map:** c5430, m11_01_00_00 Ashina Castle (boss_scripts.json "54300000" .map; roster.json:9316; npc_names.json:63)
- **Setup:** Arena: boss_71001000 through MOVED (extracted/map_boss_arenas.json:15 and moved :25; tools/boss_arenas.py:71-79). He shares m11_01 with the castle top, but the "moved" list still makes him a stranger: warps skipped, heights dropped (src/enemy.rs:857-863).
Boss script: yes. Boss part 1110920, no cast. Events:
- 11115930 starts at instruction 43: enable, AI, BGM 510, Set Lighting 4, boss bar.
- 11115932: at 1 bar or fewer, after 3 s, sets 11115922 (track 2).
- 11115941: 0 bars, msg 10 and 8300 give EzState 20200 and Wolf 710206.
- 11115944: on SpEffect 3540510, Shoot Bullet 254000382 from owner 1110921 at regions 1112960-1112994.
- 11110100.
- 11115920: defeat after his message 50, then kill and Handle Boss Defeat, flag 9316.
- 11115942: immortal until message 40.
boss_events.json:1502-1527 (11115941). Bullets: 254000382 -> Bullet 54000382 (boss_scripts.json "54300000".bullets).
Lighting: FIGHT_LIGHTING _0740 at 18 h (src/map.rs:1211). Music: m110102002, then m110102005 (boss_bgm.json).
- **Status:** partial
- **Tests:** all_enemies_smoke c5430: OK (anim c5400, Kensei_543000_Battle; smoke.log:131, scratchpad b5798fc2, 2026-10-11). all_enemies_full c5430: OK. ThrowDef12000, then ThrowDef13700 + Event20200, DEAD (full.txt:132). The trailing "left 2" shows the fight had already reset inside the window; whether 11115920's defeat or the 15 s dead rule did it is unknown. No named test.
- **Done:**
  - 2 deathblows, HP 1600, posture 750 (roster.json c5430). Uses c5400's anims (roster animChr c5400; src/data.rs:792-793).
  - Todome rows (src/player.rs:528-531). Todome message gives EzState 20200 and Wolf 710206.
  - Draw param variant _0740 exported and picked at 18 h (src/map.rs:1196-1211; docs/kb/map.md:442-461).
- **Open:**
  - Gun volley not set up. c5400 a000_003032, which c5430 shares, adds SpEffect 3540510 at f90 via TAE 66 (extracted/enemies/c5400.json anims). 11115944 then shoots from owner 1110921 at 35 regions, but 1110921 is only a place, not in run.ents, and the source is a region. So every shot is dropped (src/enemy/script.rs:1055-1058). 11115944's 2004[7] on 1110921 falls to `_ => {}` (src/enemy/script.rs:729).
  - Time of day 4: the data has 2003[81] [4, 0.0] at 11115930 instruction 50, right after his enable at 43, so it applies at his fight start. docs/kb/map.md:449 and src/map.rs:1205 say "after his death". Sv1 never applies time of day; the exe table is a gap (src/map.rs:1199-1200).
  - His defeat 11115920 waits for his own event message 50. Whether that ever fires in Sv1 is unknown (not asserted by any test).
  - Not chained after Emma: in Sv1 he is a separate encounter (see 74000010).
- **Key files:** `extracted/enemies/boss_scripts.json ("54300000")`, `extracted/enemies/boss_events.json:1502`, `extracted/map_boss_arenas.json:15`, `src/enemy/script.rs:1052-1060`, `src/map.rs:1211`, `src/enemy_bullet.rs`, `extracted/enemies/c5430.json`, `extracted/enemies/c5400.json`

### 74000010 Emma, the Gentle Blade (Shura ending)

- **chr / map:** c7400, m11_01_00_00 Ashina Castle (boss_scripts.json "74000010" .map; npc_names.json:70; roster.json:9852). c7400's default roster row is 74000000, which has no boss script or arena.
- **Setup:** Arena: boss_71001000 through MOVED (extracted/map_boss_arenas.json:21 and moved :27; tools/boss_arenas.py:71-79). She is a stranger there (src/enemy.rs:857-863).
Boss script: yes. Boss part 1110900. chars holds only Emma, not Isshin Ashina 1110920. Events:
- 11115910 starts at instruction 45: enable, AI, boss bar.
- 11115930: Isshin Ashina's entry.
- 11110100.
- 11115800.
- 11115900: her defeat, Handle Boss Defeat 2.5 s after her death, flag 9315.
flags_on includes 9316. No boss_events.json rules.
Lighting: FIGHT_LIGHTING _0740 at 18 h (src/map.rs:1210), from 11115910's 2003[88] sub 40 (docs/kb/map.md:448-449). Music: m110102001 (boss_bgm.json).
- **Status:** partial
- **Tests:** all_enemies_full c7400:74000010: OK. ThrowDef12000 + ThrowDefDeath12001, DEAD, "left 1", meaning the fight had already reset (full.txt:169, scratchpad b5798fc2, 2026-10-11). The harness only needs a death, not a defeat. The default row 74000000 passes smoke (smoke.log:141) and full (full.txt:142). No named test.
- **Done:**
  - 1 deathblow with a normal death (ThrowDefDeath12001). HP 1595, posture 750 (roster.json c7400).
  - Castle-top placement and her own draw param variant _0740 at 18 h.
- **Open:**
  - Her defeat is never handled (bug, from the data and the code).
- flags_on for 74000010 contains 9316 (boss_scripts.json "74000010".flags_on). world_flags treats flags the script reads but never writes as on (tools/boss_scripts.py:235-256), and only Isshin Ashina's 11115920 writes 9316.
- 11115900's first instruction is END IF 9316 on, so it ends on frame 1 and Handle Boss Defeat never runs.
- After her death the run resets 15 s later with "boss script: 74000010 dead without a defeat event" (src/enemy/script.rs:899-908). No Won screen.
- The same class of problem already has the FLAGS_OFF switch (tools/boss_scripts.py:215-220, used for 12600050).
  - Isshin Ashina does not follow her. 11115930 enables 1110920, but he is not in 74000010's chars, so the enable is ignored (src/enemy/script.rs:954). This is also moot while the 9316 issue stands.
  - A plain config chr="c7400" with no npc_row gets row 74000000: no script, no castle top (src/map.rs:112-135).
- **Key files:** `extracted/enemies/boss_scripts.json ("74000010")`, `tools/boss_scripts.py:215-256`, `extracted/map_boss_arenas.json:21`, `src/enemy/script.rs:899-908`, `src/enemy/script.rs:954`, `src/map.rs:1210`, `extracted/enemies/c7400.json`

**Notes for this group:**
- Ownership: every row here is in extracted/enemies/npc_names.json and belongs to the outside contributor. The Claude sessions keep only the shared engine (docs/HANDOFF.md:40-49). The status sheet named there, docs/reports/bosses-handover.md (docs/HANDOFF.md:48), does not exist yet in docs/reports.
- All seven rows have a boss script (extracted/enemies/boss_scripts.json, a single-line JSON). For scripted rows, src/enemy.rs skips the boss_events.json phase rules and the Todome-message path (src/enemy.rs:764, 1376, 1407) and runs src/enemy/script.rs instead. boss_events.json is still used by event_kinds to load the brought-in kinds, Tomoe c7110 and Isshin c5400 (src/enemy.rs:227-258; src/data.rs:1371-1380).
- Castle top boss_71001000: own arena for 71001000 and 71100000. Moved there: 54000000, 71101000, 54300000 and 74000010, plus the Owl 50600000 (extracted/map_boss_arenas.json:5, 14-28; tools/boss_arenas.py:58-80). Moved rows are strangers: CutsceneWarp and place Warps are skipped (src/enemy/script.rs:996, 1003), MSB heights are dropped (src/enemy/events.rs:32; src/enemy/script.rs:373), and the line-up stands 8 m from Wolf's start (src/enemy.rs:882). 71000000 stays on the silvergrass field boss_54000000 (extracted/map_boss_arenas.json:17).
- FIGHT_LIGHTING has 5 entries: 71001000 and 71100000 _0000 at 12 h, 50600000 _0750 at 18 h, 74000010 and 54300000 _0740 at 18 h (src/map.rs:1196-1213). It is applied once when the map loads (src/map.rs:1291-1300). There is no row for 54000000 or 71101000. Script lighting instructions 2003[88] and 2003[81] are not run (src/enemy/script.rs:729). An encounter swap that stays on the same arena keeps the old lighting (src/map.rs:1250-1252). The look is still off compared with the game (docs/kb/map.md:462-467), and the time-of-day to hour table is a gap (src/map.rs:1199-1200). The arena only exports the 0740 and 0750 variants (map_boss_71001000.json); m11_01_0010 and _0710.gparam are extracted but unused (extracted/param/drawparam).
- Script runner limits that matter here:
- Cutscenes, SFX, sounds and cameras are taken as done (src/enemy/script.rs:21-22).
- Unknown conditions are false (src/enemy/script.rs:508).
- Acts on Wolf other than warps and EzState are dropped, because only the boss and cast are in run.ents (src/enemy/script.rs:1076-1079).
- Any 93xx flag set while the run's boss is at 0 bars counts as a defeat (src/enemy/script.rs:686-697).
- After a defeat the fight resets 5 s later with AutoRestart (src/enemy/script.rs:823-829).
- A dead boss with no defeat resets after 15 s (src/enemy/script.rs:899-908).
- Tests: sim tests run headless with no map (src/sim_tests.rs:43-56), so arenas, lighting and on-arena warps are not covered. The only arena check is every_arena_start_stands_and_walks (src/map.rs:1982). all_enemies_smoke only runs default rows (src/sim_tests.rs:3459). all_enemies_full also runs boss-script rows that are not a chr's default and not listed in another script's chars (src/sim_tests.rs:3730-3742): here c7100:71001000, c7110:71101000 and c7400:74000010.
- Latest recorded harness results: smoke 102/102 and full 95/110, "the 15 non-fighters" (docs/HANDOFF.md:539-541, 2026-10-11). NON_FIGHTERS does not include c7100, c7110, c5400, c5430 or c7400 (src/sim_tests.rs:3439). Per-line logs that match those totals are in another session's scratchpad, C:\Users\User\AppData\Local\Temp\claude\C--DeadlockModding\b5798fc2-c742-4214-9108-534312f06bc6\scratchpad: full.txt:130-170 and smoke.log:129-161. All 8 cases for these chrs are OK. No later run is recorded after the 2026-10-12 changes (moved fights, lighting, posture SpEffects).
- Shared character data: c5430 plays c5400's anims (roster animChr c5400; src/data.rs:792-793), so Isshin Ashina inherits a000_003032's 3540510 volley trigger. Todome rows (suffix 180/181) exist for c5400, c5430 and c7110 only (src/player.rs:528-531). c7100 has no ThrowDefDeath anims (docs/kb/enemy.md:135). Wolf's 710206 (both Isshins) has a c0000.hkx state; 710204 (Tomoe) has none (docs/reports/enemies.md:110-112).
- Boss music is keyed by the script run's row (src/fmod.rs:690-700; extracted/enemies/boss_bgm.json). 71100000 has no key. A hand-over keeps the first row's tracks.
- Stale comments to fix: src/enemy.rs:845-846 and docs/kb/enemy.md:301 still list 54000000 and 71101000 as fighting on Isshin's field as their own arena, and call 71000000 "Way of Tomoe". src/sim_tests.rs:3737-3738 names Tomoe 71101000 as the brought-in row, but it is 71100000. src/enemy/script.rs:687-689 ties m11_02 11125800 to the castle top. docs/kb/map.md:449 and src/map.rs:1205 say Isshin Ashina's time of day 4 comes after his death, but the data puts it at his fight start.

## More open items (completeness check)

- **50000000, 50001000:** Corrupted Monk arena labels are swapped in the docs and tool. They say boss_50000000 = 'Mibu phantom spots' and boss_50001000 = 'Fountainhead bridge', but map_boss_50000000.json is m25 (Fountainhead) and map_boss_50001000.json is m15 (Mibu). The sheet repeats the error. (tools/boss_arenas.py:59; docs/kb/map.md:417-418; docs/kb/map.md:381; docs/HANDOFF.md:436; docs/kb/grapple.md:195)
- **54000000, 71000000, 71101000:** Stale own-arena note. It says Isshin's field boss_54000000 is still the own arena of 54000000 and 71101000, and calls 71000000 'Genichiro Way of Tomoe'. In fact map_boss_arenas.json maps 54000000 and 71101000 to boss_71001000 and lists them in 'moved', and npc_names.json names 71000000 'Genichiro Ashina'. (src/enemy.rs:845-846; docs/kb/enemy.md:299-301)
- **50600000, 54000000, 54300000, 71101000, 74000010:** Moved fights (Owl, Isshin, Isshin Ashina, Emma, and Tomoe 71101000 on boss_71001000) skip the boss script's CutsceneWarp and place warps, because those places are in the fight's own map frame. This is a known divergence from the game. (docs/kb/enemy.md:293-298; docs/kb/map.md:435-437)
- **71001000, 71100000, 50600000, 54000000, 54300000, 71101000, 74000010:** Genichiro's castle-top arena leaves out the burnt floor model m015072 (MAP_SKIP_MODEL), and every moved fight uses this arena. (docs/kb/map.md:429-431; tools/boss_arenas.py:54)
- **10200000-14704000 range (all 37 rows without a boss script), 50200010, 50202000, 70000000:** The doc says every other boss row maps to boss_54000000. But map_boss_arenas.json has entries only for the 20 boss-script rows, so the 37 mini-boss rows without a boss script (O'Rin, both Chained Ogres, Generals, Headless, Shichimen, Lone Shadows ...) get no arena: boss_arena returns None and they fight on the config map. (docs/kb/map.md:420-422; src/map.rs:112-133; extracted/map_boss_arenas.json)
- **10700000, 10701000, 10703000, 10704000, 50600000, 50601010:** Wire (grapple) SpEffects for c1070 (Juzou / Shigekichi / Tokujiro) and c5060 (Owl) are put on 'by scripts', with 'none in TAE'. enemy/grapple.rs wire_effect reads only TAE and resident SpEffects. Whether a script-set SpEffect reaches it is unknown. The kb table also mislabels c1070 as 'Shura (Ashina Elite?)' and c5060 as 'Lone Shadow chief'. (docs/kb/grapple.md:132; docs/kb/grapple.md:137; src/enemy/grapple.rs:41-45)
- **71000000, 71001000:** The c7100 ThrowDef reactions also add 275080 (HP / posture total change) and 3710020 ('rival deathblowed: clash count 0'). Neither is used. (docs/kb/enemy.md:196-197)
- **10800000, 10801000, 10802000, 50600000, 50601010, 50900000, 50900001:** The sound banks c1080 (Shichimen), c5060 (Owl) and c5090 (Butterfly) hang in the decoder because Vorbis setup headers are missing (gap: where the game gets those codebooks). Their PCM is lost in a full extraction. The effect on runtime FMOD playback is unknown. (docs/kb/sound.md:14-21)
- **51000000, 51000100:** The doc says 'the only restarts left are the Guardian Ape's fur at 17 s into its idle (a jump)'. Whether the later teleport-threshold fix (visuals.md:216-218) covers it is unknown. (docs/kb/visuals.md:152-153)
- **54000000, 71000000, 71001000, 71100000, 71101000, 10800000, 10801000, 10802000:** FXR gaps in boss effects: NodeAccelerationRandomTurns 83 turn axes / distribution and RectangleEmitterShape 402 distribution. The users include c5400, c7100/c7110 and c1080. (docs/kb/visuals.md:318-320)
- **53000000, 52000000:** The doc calls 53000000 'likely unused: nothing in the scripts starts it'. But boss_scripts.json 53000000 exists (boss 2500810 c5300) and enables the Divine Dragon 2500800 (c5200 52000000) and the c5310 tentacles; the clip index lists it as '49. Old Dragons of the Tree / Divine Dragon'. (docs/reports/enemies.md:129-130; docs/kb/clips.md:176)
- **50000000:** The Monk's phantoms: how the exe takes the short-warp request (her vanish state) and how the game keeps them out of sight between attacks are both untraced. (src/enemy/script.rs:1026; src/enemy/script.rs:176-177)
- **54000000, 50000000:** The exe rule for EzState requests mid-throw (Isshin m11_02 11125874 on his finisher's message 10; m25 12505963 during the Monk's Todome) is untraced. (src/enemy/script.rs:1062-1066)
- **50800000, 51000000:** The immortality vs last-deathblow rule is untraced. Gyoubu's ThrowDef12090 kills him despite m11_00 11105820; the Ape's fake death has no 201000. (src/enemy/script.rs:1102-1106)
- **50000000:** The Monk: forced anim 20021 is skipped on a dead Monk, and a repeated 20200 request while she is dead keeps playing. Neither exe rule is traced. (src/enemy/script.rs:1170-1171; src/enemy/script.rs:1184-1185)
- **51000000:** When the Guardian Ape gets up from his 0-bar 20200, what the game gives back is untraced (taken as full HP, posture and one bar). (src/enemy/script.rs:1205)
- **all 20 boss-script rows (boss_scripts.json):** Boss script gaps: map objects, SFX, sounds, cameras, cutscenes (taken as done), lock-on points, NPC parts, gravity / hit masks, draw masks, damage type of a hit. Also the region x/z rotation is dropped and 'damaged by whom' is not checked. (src/enemy/script.rs:21-22; src/enemy/script.rs:73; src/enemy/script.rs:488)
- **50000000, 50600000, 51000000, 54000000, 54300000, 70200000, 71100000:** How long the exe keeps Wolf's Todome event message (TAE 936) is untraced; here it lasts while the anim plays. (src/enemy/script.rs:757-758)
- **all boss-script rows:** The 5 s auto-restart after Handle Boss Defeat has no counterpart in the game. (src/enemy/script.rs:823-824)
- **50000000, 50600000, 51000000, 51000100, 54000000, 54300000, 70200000, 71100000, 50900000:** The exe's choice of the Todome rows (ThrowParam 0180/0181) and of the second-form (+500 'HU') rows is untraced; it is picked by row names and anim sets. (src/player.rs:528-538)
- **52000000:** What starts the Divine Dragon's collapse loop 21000 in the game is untraced (no EMEVD, HKS or AI request found). (src/enemy.rs:68-72)
- **every row with deathblows > 1:** The posture bar's reset after a non-final boss deathblow is not traced to an event. (src/enemy.rs:350-352)
- **all boss-script rows:** The gap comment on SpawnEncounter is stale. It says 'the map stays (boss arena picked at startup only)', but map::switch_map now changes the map on a SpawnEncounter. (src/enemy.rs:909; docs/HANDOFF.md:526-527 vs docs/HANDOFF.md:432-433 and docs/kb/enemy.md:302-303)
- **71100000, 71101000:** The all_enemies_full comment says Tomoe 71101000 is 'brought in' and checked only through the hand-over. In boss_scripts.json no script enables 71101000; 71001000 enables 71100000. So 71101000 runs as its own case c7110:71101000. (src/sim_tests.rs:3737-3739)
- **boss-script rows with enabled cast (50000000, 71001000, 71101000, 53000000, 12600050):** Cast characters on another floor have their height dropped, and there is no navmesh. (src/enemy/events.rs:31)
