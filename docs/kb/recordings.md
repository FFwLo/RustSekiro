# Recordings of the real Sekiro (memscope)

One line per recording: `python -m tools.rec list` rewrites this file. Read a summary with
`python -m tools.rec summary <rec> --cat <movement|combat|enemies|bosses|skills|items|camera|sound|effects|script|all>`
(cached in `<rec>/summary/<cat>.md`); drill down with `python -m tools.rec query <rec> help`.
Folder: `..\memscope-data\logs`. Recorder: `tools/memscope/rec2.lua` (v1: recorder.lua).

How to record (the user plays, the session records):
1. Start Sekiro with the hook DLL: from `..\me3` run `bin\me3.exe launch -p sekiro-rec.me3`;
   `me3\sekiro-rec\sv1_rec_hooks.log` lists the installed hooks.
2. `python -m tools.rec install`, then `python -m tools.rec new <name> --note "..." --cats a,b` (prints the folder).
3. In the background: `python -m tools.rec keys <rec>` (F9 marker, F10 stop, F5 can't-die, F6 freeze AI,
   F7 speed 1/0.5/0.25, F3/F4 save/teleport position).
4. memscope: attach sekiro.exe, scripts run rec2 with args {dir: <full folder path>, seconds: N}, timeout N + 60.
5. `python -m tools.rec summary <rec> --cat every`, then `python -m tools.rec list` to refresh this file.
Turn SekiroTool's infinite items / emblems off for item tests (the summaries warn when they were on).
Our sim against a recording: `python -m tools.rec replay <rec> [--from T --to T]` plays its stick and buttons
into the sim, with the art and prosthetic Wolf had (their resident SpEffects) and, while locked on, a
stand-in at the lock target's place (the recorded enemy the camera looks at), and matches the W_ events
both HKS scripts fired; enemy, swim, teleport and recorder-stall stretches are left out (until both sides
play the same clip again, started within 2 ticks).

| recording | date | what | categories | length | summaries |
|---|---|---|---|---|---|
| rec_20261008_041541 | 2026-10-08 | v1 recorder |  | 330 s | movement |
| rec_20261008_044802 | 2026-10-08 | v1 recorder |  | 330 s | movement |
| rec_20261008_050912 | 2026-10-08 | v1 recorder |  | 330 s | movement |
| rec_20261008_051450 | 2026-10-08 | v1 recorder |  | 330 s | movement |
| rec_20261008_054259 | 2026-10-08 | v1 recorder |  | 330 s | combat movement |
| rec_20261015_181643_first_test | 2026-10-15 18:16 | first live test: walk, hits, guard, gourd, F9 | movement,combat,items | 60 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_182811_combat_regular | 2026-10-15 18:28 | combat vs one regular enemy: attacks, guard, deflect, hits taken, mikiri/jump, deathblow | combat,enemies,movement | 164 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_184118_skills_tools | 2026-10-15 18:41 | prosthetics + combat arts, each used several times near an enemy | skills,combat,items | 600 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_185530_skills_tools_2 | 2026-10-15 18:55 | skills part 2: remaining prosthetics + combat arts | skills,combat | 130 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_190450_items | 2026-10-15 19:04 | items: gourd, pellets, sugars, other consumables, emblems via prosthetics | items,skills | 300 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_191200_movement | 2026-10-15 19:12 | movement: walk, run, sprint, jump, fall, grapple, crouch, locked-on strafe | movement,camera | 240 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_193057_boss_yamauchi | 2026-10-15 19:30 | boss: Leader Shigenori Yamauchi | bosses,combat | 66 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_193250_boss_genichiro_first | 2026-10-15 19:32 | boss: Genichiro Ashina, first encounter (Ashina Outskirts, scripted loss) | bosses,combat | 120 s | all bosses camera combat enemies items movement skills sound |
| rec_20261015_193824_boss_genichiro_first_b | 2026-10-15 19:38 | boss: Genichiro Ashina, first encounter, part 2 (more moves) | bosses,combat | 68 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_201915_combat_regular_hooks | 2026-10-15 20:19 | combat vs regular enemy with all hooks: attacks, guard, deflect, hits taken, mikiri, grab, deathblow | combat,enemies,sound,effects,script | 180 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_202623_vfx_test | 2026-10-15 20:26 | 1 min test of the fixed vfx hook: combat with a regular enemy | effects,combat | 60 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_203558_skills_hooks | 2026-10-15 20:35 | skills with all hooks: every prosthetic + combat art several times near an enemy (infinite emblems on) | skills,effects,sound,combat | 826 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_210022_boss_genichiro_first_c | 2026-10-15 21:00 | boss: Genichiro Ashina, first encounter, part 3, all hooks (scripted loss at the end) | bosses,combat,effects,sound,script | 177 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_210505_boss_gyoubu | 2026-10-15 21:05 | boss: Gyoubu Oniwa (c5080, horseback), all hooks | bosses,combat,effects,sound,script | 188 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_211200_boss_gyoubu_b | 2026-10-15 21:12 | boss: Gyoubu Oniwa (c5080), part 2 (more moves), all hooks | bosses,combat,effects,sound,script | 180 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_211958_boss_gyoubu_c | 2026-10-15 21:19 | boss: Gyoubu Oniwa (c5080), part 3 (more moves), all hooks | bosses,combat,effects,sound,script | 228 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_225710_boss_butterfly | 2026-10-15 22:57 | boss: Lady Butterfly (c5090), all hooks | bosses,combat,effects,sound,script | 333 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_231740_boss_genichiro_castle | 2026-10-15 23:17 | boss: Genichiro Ashina, castle top (c7100, 3 phases incl. lightning), all hooks | bosses,combat,effects,sound,script | 420 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_232705_boss_genichiro_castle_b | 2026-10-15 23:27 | boss: Genichiro Ashina, castle top, part 2 (win, all phases), all hooks | bosses,combat,effects,sound,script | 266 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_233452_boss_monkeys | 2026-10-15 23:34 | boss: Folding Screen Monkeys (c1260 x4 + c1250 summoned monkeys), all hooks | bosses,combat,effects,sound,script | 580 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_234545_boss_monkeys_b | 2026-10-15 23:45 | boss: Folding Screen Monkeys, part 2 (until the end), all hooks | bosses,combat,effects,sound,script | 118 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261015_234913_boss_guardian_ape | 2026-10-15 23:49 | boss: Guardian Ape (c5100), all hooks | bosses,combat,effects,sound,script | 572 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_000242_boss_corrupted_monk | 2026-10-16 00:02 | boss: Corrupted Monk, illusion (c5000_1000, row 50001000), win, all hooks | bosses,combat,effects,sound,script | 328 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_001225_boss_owl | 2026-10-16 00:12 | boss: Great Shinobi Owl (c5060_0000, row 50600000), all hooks | bosses,combat,effects,sound,script | 594 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_002433_boss_true_monk | 2026-10-16 00:24 | boss: True Corrupted Monk (c5000_0000, row 50000000), all hooks | bosses,combat,effects,sound,script | 859 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_004230_boss_headless_ape | 2026-10-16 00:42 | boss: Headless Guardian Ape (c5100_0100) + its mate (c5100_0010), win, all hooks | bosses,combat,effects,sound,script | 417 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_070725_boss_divine_dragon | 2026-10-16 | boss: Old Dragons (c5300) + tree tentacles (c5310) + Divine Dragon (c5200), win, all hooks; stopped via inbox (no key helper) |  | 578 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_072651_boss_demon_of_hatred | 2026-10-16 | boss: Demon of Hatred (c7020), win, all 3 phases, all hooks |  | 952 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_074559_boss_owl_father | 2026-10-16 | boss: Owl (Father) (c5060_1010), win, both phases, all hooks |  | 1008 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_080607_boss_tomoe_genichiro_isshin | 2026-10-16 | boss: Genichiro Way of Tomoe (c7110) then Isshin the Sword Saint (c5400), win, all phases, all hooks |  | 962 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_082459_boss_emma_isshin_ashina | 2026-10-16 | boss: Shura route - Emma (c7400) then Isshin Ashina (c5430, uses c5400 anims), win, both phases, all hooks |  | 600 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_083843_movement_hooks | 2026-10-16 08:38 | movement with all hooks: walk, run, sprint, jump, fall, grapple, crouch, locked-on strafe | movement,camera,effects | 236 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_092035_movement_clean | 2026-10-16 09:20 | clean per-move clips: walk, run, sprint, jump standing, jump running, lock strafe, sprint deflect, open-air double jump | movement,camera | 186 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_093314_buttons_calib | 2026-10-16 09:33 | button calibration: F9 then each button once (jump, step, crouch, sprint hold, item, prosthetic, art, stick 4 directions) | movement | 82 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_094418_buttons_calib_atk_guard | 2026-10-16 09:44 | button calibration 2: guard hold, guard taps x3, attack x3, attack hold (F9 before each) | movement,combat | 47 s | all bosses camera combat effects enemies items movement script skills sound |
| rec_20261016_094717_lockturn_sprintdeflect | 2026-10-16 09:47 | lock-on turn (swing camera / switch targets, walk+run around locked enemy) and sprint then deflect next to an enemy, x5 each | movement,camera | 69 s | - |
| rec_c1010_20261009 | 2026-10-09 | v1 recorder |  | 600 s | - |
| rec_c1010_b_20261009 | 2026-10-09 | v1 recorder |  | 120 s | - |
| rec_c1020_20261009 | 2026-10-09 | v1 recorder |  | 60 s | - |
| rec_c1020_b_20261009 | 2026-10-09 | v1 recorder |  | 300 s | - |
| rec_c1020_c_20261009 | 2026-10-09 | v1 recorder |  | 120 s | - |
| rec_c1020_d_20261009 | 2026-10-09 | v1 recorder |  | 60 s | - |
