# Reference clips

Game footage kept outside the repo in `..\clips\` (never committed). Pulled
with the watch skill (`..\reference\claude-video`, yt-dlp + ffmpeg; the
local engine extracts frames, Gemini can watch a whole clip once a key is in
`~/.config/watch/.env`). For a frame-exact look use ffmpeg on the kept file, e.g.

```bash
ffmpeg -ss 00:01:52 -i "../clips/sekiro_all_skills_rYcHlOgI7pg.mp4" -t 3 -vf fps=30,scale=960:-1 -q:v 3 out/f_%03d.jpg
```

The clip's own chapter title is burned into the top-left of every frame, so a frame
identifies its skill by itself.

## All skills and prosthetics (2026-10-11)

`sekiro_all_skills_rYcHlOgI7pg.mp4`: "Sekiro All Skills Attacks Moveset [Combat Arts, Shinobi
Martial Arts, Ninjutsu Skills]" by cad5150, https://www.youtube.com/watch?v=rYcHlOgI7pg,
15:03, 1280x720 (AV1), no speech. Every combat art, the Shinobi martial arts and the
ninjutsu, mostly at the Dilapidated Temple at night (dark frames: boost exposure for sheets).
One demo per chapter, several casts each (hit and whiffed, some at a dummy enemy).

| Start | End | Chapter | Sv1 side |
|---|---|---|---|
| 0:00 | 0:22 | Shadowrush | art (player.rs) |
| 0:22 | 0:58 | Shadowfall | art |
| 0:58 | 1:49 | Nightjar Slash Reversal | art (and Nightjar Slash) |
| 1:49 | 2:33 | Ichimonji Double | art (and Ichimonji) |
| 2:33 | 2:55 | Praying Strikes - Exorcism | art (and Praying Strikes) |
| 2:55 | 3:26 | Senpou Leaping Kicks | art |
| 3:26 | 3:59 | High Monk | art |
| 3:59 | 4:19 | Dragon Flash | art |
| 4:19 | 4:43 | Floating Passage | art |
| 4:43 | 5:11 | Spiral Cloud Passage | art |
| 5:11 | 5:30 | Whirlwind Slash | art |
| 5:30 | 5:57 | Ashina Cross | art |
| 5:57 | 7:16 | One Mind | art |
| 7:16 | 7:52 | Empowered Mortal Draw | art (and Mortal Draw) |
| 7:52 | 8:09 | Grappling Hook Attack | shinobi martial art |
| 8:09 | 8:38 | Mid-air Deathblow | deathblow (B1) |
| 8:38 | 9:01 | Mid-air Prosthetic Tool | prosthetic.rs air use |
| 9:01 | 9:17 | Vault Over | movement (HANDOFF L345: in-game check wanted) |
| 9:17 | 9:33 | Run and Slide | movement |
| 9:33 | 9:40 | Mid-air Deflection | guard-deflect |
| 9:40 | 9:56 | Chasing Slice - Loaded Shuriken | tool follow-up |
| 9:56 | 10:27 | Chasing Slice - Loaded Spear | tool follow-up |
| 10:27 | 10:46 | Chasing Slice - Shinobi Firecrackers | tool follow-up |
| 10:46 | 11:10 | Fang and Blade - Loaded Axe | tool follow-up |
| 11:10 | 11:28 | Fang and Blade - Loaded Spear | tool follow-up |
| 11:28 | 11:45 | Fang and Blade - Mist Raven | tool follow-up |
| 11:45 | 12:20 | Projected Force - Finger Whistle | tool follow-up |
| 12:20 | 12:30 | Projected Force - Loaded Umbrella | tool follow-up |
| 12:30 | 12:50 | Living Force - Flame Vent | tool follow-up |
| 12:50 | 13:10 | Living Force - Divine Abduction | tool follow-up |
| 13:10 | 13:29 | Ledge-hang Deathblow | deathblow (B1) |
| 13:29 | 13:47 | Wall-hug Deathblow | deathblow (B1) |
| 13:47 | 14:17 | Bloodsmoke | ninjutsu (VFX) |
| 14:17 | 14:44 | Puppeteer | ninjutsu |
| 14:44 | 15:03 | Bestowal | ninjutsu (VFX) |

Correction (2026-10-12, from the burned-in labels): plain "Ichimonji" runs to ~1:48,
"Ichimonji Double" 1:49-~2:12, "Praying Strikes" from ~2:13; plain "Mortal Draw" to ~6:50-7:13,
"Empowered Mortal Draw" 7:14-7:52 (plain Mortal Draw already shows red trails). The table's
other rows are the uploader's chapter marks, not re-checked.

Use: VXf's B6 skill sheets (`SHINOBI_SKILL_SHOTS`) against these chapters (arc place, size,
colour of each art's trail and effect); Bosses' B1 deathblow placement (mid-air, ledge,
wall-hug); the cape on the arts for B2 (the dark temple hides it; a daylight clip is better).

## All Shinobi executions / finishers (2026-10-11)

`sekiro_all_finishers_wodFkPrFUag.mp4`: "All Shinobi Executions / Finishers - SEKIRO" by
Hatenak Le Petit Bridé, https://www.youtube.com/watch?v=wodFkPrFUag, 6:08, kept at 1280x720
(VP9) and, for frame-exact measuring (Bosses' Ochimusha drift), the 4K 60 fps source as
`sekiro_all_finishers_wodFkPrFUag_4k60.mp4` (913 MB, VP9 3840x2160). French UI and subtitles, no chapters. Every boss and mini-boss deathblow in game order, the camera
cuts of each 忍殺 included. Times are from a 2 s contact sheet (±2 s); the 19 scene cuts the
watch skill picked are too sparse for this clip.

| Time | Enemy (FR label in the clip) | Deathblows shown |
|---|---|---|
| 0:10-0:15 | General Naomori Kawarada (Général), outskirts gate | stealth + finisher |
| 0:16-0:23 | Chained Ogre (Ogre enchaîné), first | two |
| 0:24-0:36 | Gyoubu Masataka Oniwa, on horseback | two (the horse rear) |
| 0:38-0:48 | Blazing Bull (Taureau enflammé) | one, from the side |
| 0:50-1:05 | Genichiro Ashina, castle top ("Shinobi de l'héritier divin") | three, incl. Way of Tomoe |
| 1:06-1:12 | Armored Warrior (Guerrier en armure), Senpou bridge | pushed off the bridge ("Robert!") |
| 1:14-1:20 | Headless (pink glow, underwater) | one |
| 1:22-1:26 | Juzou the Drunkard (Juzo le pochard) | one |
| 1:26-1:44 | Lady Butterfly (Dame Papillon), both phases | two + the illusion phase end |
| 1:44-1:50 | Seven Ashina Spears, Shikibu Toshikatsu Yamauchi, stairs | one |
| 1:50-1:54 | Long-arm Centipede Giraffe (white-haired) | one |
| 1:54-2:10 | Guardian Ape (Singe gardien), phase 1 | decapitation at 2:04-2:08 |
| 2:12-2:28 | Great Serpent (Grand serpent), both stealth kills | the valley hang + the cave |
| 2:30-2:46 | Shichimen Warrior (Guerrier Shichimen), purple | one |
| 2:48-3:00 | Guardian Ape, headless phase, and the Headless Ape pair | two |
| 3:02-3:10 | O'Rin of the Water (O'Rin de l'eau) | one |
| 3:14-3:20 | Corrupted Monk illusion (Religieux corrompu), Mibu | one |
| 3:20-3:34 | Great Shinobi Owl (Grand shinobi - La Chouette), roof | two, the jump-in at 3:28 |
| 3:34-3:38 | Chained Ogre, second (castle) | one |
| 3:40-3:46 | red-aura swordsman on a castle roof (Lone Shadow, label unread) | one |
| 3:48-3:52 | Tokujiro the Glutton (club, grey rocks) | one |
| 3:54-3:58 | green-lit depths (Mibu / poison pool, unread) | one |
| 4:00-4:04 | Tenzen Yamauchi? (Chef ... Yamauchi, Hirata) | one |
| 4:04-4:10 | Genichiro in the silvergrass under the moon | one (Hirata memory) |
| 4:12-4:22 | unlabeled snow fight, pavilion (Hirata) | one |
| 4:24-4:36 | Ashina Elite Jinsuke Saze, dojo door, red slash at 4:32 | one |
| 4:44-4:58 | Corrupted Monk, true form, Fountainhead bridge (red leaves) | three |
| 5:00-5:06 | Divine Dragon, the lightning deathblow | one |
| 5:08-5:12 | Seven Ashina Spears, Shume Masaji Oniwa | one |
| 5:14-5:16 | Genichiro, Way of Tomoe, silvergrass field | one |
| 5:18-5:50 | Isshin the Sword Saint (Isshin Ashina, le maître épéiste), all phases | four, "Pour toi, Sekiro" at 5:42 |
| 5:48-6:02 | Demon of Hatred (Démon de la haine) | three |

Use: Bosses' B1 deathblow geometry (front/behind placement, the throw position
interpolation, the camera cuts per boss), VXf's deathblow blood and the 忍殺 mark art.

## All 57 bosses, no damage, Demon Bell (2026-10-11)

`sekiro_all_bosses_oS_gJf8ur8M_1080p60.mp4`: "Sekiro: Shadows Die Twice - All 57 Bosses (No
Damage/Demon Bell) + All 4 Endings" by Kephket, https://www.youtube.com/watch?v=oS_gJf8ur8M,
1:46:08, kept at 1920x1080 60 fps (AV1, 2.7 GB; the 4K 60 fps source is 15 GB, not kept;
seek with `-ss` before `-i`, AV1 decode is slow). Every
boss and mini-boss fight whole, Demon Bell on (enemies hit harder, more posture), no damage
taken, so every boss attack is answered by a deflect, dodge or jump: the pacing reference for
B5 (AI timing against a passive-free player) and the attack reach for B1. English UI. One
chapter per fight (the numbering is the uploader's):

| Start | End | Chapter |
|---|---|---|
| 0:00 | 0:24 | 1. Leader Shigenori Yamauchi |
| 0:24 | 4:48 | 2. Genichiro Ashina |
| 4:48 | 5:22 | 3. General Naomori Kawarada |
| 5:22 | 6:36 | 4. Chained Ogre #1 |
| 6:36 | 7:22 | 5. General Tenzen Yamauchi |
| 7:22 | 7:56 | 6. Shinobi Hunter Enshin of Misen |
| 7:56 | 9:55 | 7. Headless #1 |
| 9:55 | 10:27 | Ringing the Demon Bell |
| 10:27 | 12:21 | 8. Gyoubu Oniwa |
| 12:21 | 13:27 | 9. Juzou the Drunkard #1 |
| 13:27 | 18:03 | 10. Lady Butterfly |
| 18:03 | 19:26 | 11. Blazing Bull |
| 19:26 | 20:39 | 12. Lone Shadow Longswordsman |
| 20:39 | 21:33 | 13. Seven Ashina Spears - Shikibu Toshikatsu Yamauchi |
| 21:33 | 22:03 | 14. General Kuranosuke Matsumoto |
| 22:03 | 22:27 | 15. Ashina Elite - Jinsuke Saze |
| 22:27 | 28:16 | 16. Genichiro Ashina / Genichiro, Way of Tomoe |
| 28:16 | 28:54 | 17. Armored Warrior |
| 28:54 | 29:26 | 18. Long-arm Centipede Sen'un |
| 29:26 | 32:23 | 19. Folding Screen Monkeys |
| 32:23 | 33:00 | 20. Snake Eyes Shirafuji |
| 33:00 | 33:26 | 21. Long-arm Centipede Giraffe |
| 33:26 | 33:52 | 22. Great Serpent |
| 33:52 | 36:02 | 23. Guardian Ape |
| 36:02 | 37:03 | 24. Shichimen Warrior #1 |
| 37:03 | 37:34 | 25. Snake Eyes Shirahagi |
| 37:34 | 39:12 | 26. Guardian Ape (Headless) |
| 39:12 | 40:41 | 27. Shichimen Warrior #2 |
| 40:41 | 41:05 | 28. Tokujiro the Glutton |
| 41:05 | 41:28 | 29. Mist Noble |
| 41:28 | 42:22 | 30. Headless #2 |
| 42:22 | 44:42 | 31. O'Rin of the Water |
| 44:42 | 45:13 | 32. Corrupted Monk (Illusion) |
| 45:13 | 45:47 | 33. Chained Ogre #2 |
| 45:47 | 46:30 | 34. Headless #3 |
| 46:30 | 47:14 | 35. Lone Shadow Masanaga the Spear-Bearer #1 |
| 47:14 | 48:13 | 36. Headless #4 |
| 48:13 | 48:52 | 37. Lone Shadow Vilehand |
| 48:52 | 56:03 | 38. Emma, the Gentle Blade / Isshin Ashina |
| 56:03 | 58:42 | Ending: Shura |
| 58:42 | 1:02:51 | 39. Great Shinobi - Owl |
| 1:02:51 | 1:03:34 | 40. Lone Shadow Masanaga the Spear-Bearer #2 |
| 1:03:34 | 1:04:06 | 41. Juzou the Drunkard #2 |
| 1:04:06 | 1:06:11 | 42. Owl (Father) |
| 1:06:11 | 1:07:49 | 43. Corrupted Monk |

The chapter starts are where the uploader's chapter begins, not where the fight begins. A
chapter includes the walk-in and the cutscene, so the fight itself can start more than a minute
later. The table is not shifted. Measured fight moments (bug fixes R97, 2026-10-14):

- Juzou #1 (ch.9): still fighting at 13:00-13:20.
- Lady Butterfly (ch.10): the Kuro / Emma / Owl cutscene runs 13:28-14:30. The fight runs about
  14:40-16:36 (Shinobi Execution about 16:36). Phase 2 starts about 15:47, and the posture arrows
  show from 15:52. No phantoms are summoned in this run.
- Gyoubu (ch.8): the HP trails are at 11:59-12:03; the fatal deathblow is at 12:07.3; the banner
  runs 12:16.8-12:20.1.
- Folding Screen Monkeys (ch.19): the name shows about 30:40, and the fight runs about
  30:40-31:20. Over-head bars show from 30:59.
- Sen'un (ch.18): on screen at 29:00-29:10.
- Shirafuji (ch.20): fighting at 32:20-32:50.
- Giraffe (ch.21): starts about 32:50.
- Perilous counts (boss fidelity 2026-10-15, 2 fps centre crops; the 危 is a small round red seal
  near Wolf's head; the big red two-character 阿攻 over Wolf is the player's Ako's Spiritfall buff,
  not a perilous, and a red-pixel counter cannot tell them apart in the fire arenas):
  - Great Shinobi Owl, castle (ch.39): fight 1:01:25-1:02:50 (phase 1 to 1:02:05, the ash throw,
    phase 2 1:02:10 on), **0 perilous**. The opening "Been a while since we did this" 阿攻 is
    Spiritfall.
  - Owl (Father) (ch.42): 危 at 1:04:56 and 1:05:36 among others; the earlier "2-4 per 65 s" for
    "the Owl" was this fight.
  - Corrupted Monk, Fountainhead (ch.43, NpcParam 50000000 on m25): phase 1 1:06:14-1:06:47 with
    危 at ~6:23, ~6:28, ~6:32 (and maybe ~6:37); phase 2 1:06:57-1:07:31 with one at ~7:18.
    4-5 in ~67 s = ~4 / min.
  - Demon of Hatred (ch.53): fight 1:21:51-1:24:05; two motion-blurred seals (~1:23:39, ~1:23:57),
    ~0.9 / min. The player keeps him staggered (purple art buff), so few full combos.
| 1:07:49 | 1:08:56 | 44. Sakura Bull of the Palace |
| 1:08:56 | 1:10:04 | 45. Shichimen Warrior #3 |
| 1:10:04 | 1:10:19 | 46. Okami Leader Shizu |
| 1:10:19 | 1:11:17 | 47. Headless #5 |
| 1:11:17 | 1:14:11 | 48. Great Colored Carp |
| 1:14:11 | 1:19:26 | 49. Old Dragons of the Tree / Divine Dragon |
| 1:19:26 | 1:19:51 | 50. Ashina Elite - Ujinari Mizuo |
| 1:19:51 | 1:21:05 | 51. Seven Ashina Spears - Shume Masaji Oniwa |
| 1:21:05 | 1:21:45 | 52. Shigekichi of the Red Guard |
| 1:21:45 | 1:24:17 | 53. Demon of Hatred |
| 1:24:17 | 1:29:23 | 54. Genichiro, Way of Tomoe / Isshin, the Sword Saint |
| 1:29:23 | 1:29:42 | Final Build |
| 1:29:42 | 1:32:15 | Ending: Immortal Severance |
| 1:32:15 | 1:35:32 | Ending: Purification |
| 1:35:32 | 1:38:43 | Ending: Return |
| 1:38:43 | 1:40:57 | 55. Inner Genichiro |
| 1:40:57 | 1:43:22 | 56. Inner Father |
| 1:43:22 | 1:46:08 | 57. Inner Isshin |

Use: Bosses' B5 AI pacing (attack intervals, combo choice at range, posture breaks), B1 hit
frames of the six common attacks vs the sim trace, Map's arenas (every boss on its real
arena in daylight and at night: lighting shots for B4), VXf's enemy VFX catalogue per boss.

## Chained Ogre hitboxes (2026-10-11)

`sekiro_ogre_hitboxes_G1yqXihDh_g_1080p60.mp4`: "Chained Ogre - HitBoxes" by Spicy We1ner,
https://www.youtube.com/watch?v=G1yqXihDh_g, 2:17, 1920x1080 60 fps H.264, 78 MB. A hitbox
mod draws the Ogre's (c5020) attack spheres as yellow wireframes on the live model: "you
touch the yellow balls, you get hit". The grabs are shown at 25 % speed, so the sphere
placement per animation frame is readable; the common attacks at speedrun speed. Ashina
Outskirts, the first Ogre, daylight. Times from a 1 s sheet (±1 s):

| Time | Section |
|---|---|
| 0:00-0:03 | title: Chained Ogre Hitboxes, 25 % speed |
| 0:03-0:13 | the running grab at 25 %: two spheres, one per hand, then the hold and the throw |
| 0:10-0:13 | a whiffed grab ("Wut?") |
| 0:14-0:15 | title: The Grabs, slow walking, fast, diving |
| 0:16-0:35 | the three grab variants at 25 %: walking grab (spheres from ~0:17), fast grab (~0:21), diving grab (three spheres at 0:30, two low at 0:31) |
| 0:36-0:37 | title: Common Attacks in Speedrun |
| 0:38-1:11 | kick/stomp (sphere at the foot, 0:41), drop kick (0:45-0:47), hand swing (0:51), backhand (0:55), swipe (1:00), double-hand slam (two spheres, 1:04-1:05), grab (1:10) |
| 1:14-1:15 | title: Getting "hit" and Extras |
| 1:16-1:47 | the grab landing on Wolf and the throw (1:16-1:22), the drag (1:37-1:40), double slam spheres (1:40-1:42), diving grab (1:48-1:49) |
| 1:50-2:17 | outro text |

Use: Bosses' B1 hit-capsule verification of the Ogre (plan: the six common enemy attacks
incl. the Ogre, c5020): sphere radius and placement against the AtkParam dummy-poly capsules in
the sim trace, frame by frame on the 25 % sections (60 fps, so one game frame is 4 clip
frames there). The mod's sphere is the AtkParam hit sphere (radius, dmy poly), not a hurtbox.

Findings (Bosses, 2026-10-11). The Chained Ogre is c5020 (NpcParam 50200010; c1040 is the
Long-arm Centipede). His AtkParam spheres in extracted/enemies/c5020.json match the mod's:
- 0:03-0:13 "running grab" is the tackle grab a100_003014: AtkParam 50200580 (frames 33-40,
  three spheres r 0.7 on dmy 10 / 90 / 2: both hands and the centre at his head) then 50200581
  (40-44, the same three at r 0.5). In the 25 % frames (5.4 s, 5.95 s) three spheres, hands and
  head, about 3.5 face-widths across (r ~0.6-0.7 m): matches. Visible ~16 game frames at 60 Hz
  (11 TAE frames at 30 expected = 22); the clip's slow-down is likely not exactly 25 %.
- Dash grab 50200540 / 550 (a100_003007 / 3008): two spheres r 0.5 on the hands (dmy 10, 2);
  both-hands grab 50200510 (a100_003005): r 0.3. Slam 50200530: one capsule hand-to-hand (dmy 2-10,
  r 0.5), the "two spheres" at 1:04. Kicks: foot capsules dmy 3-15 / 4-16.

## Hitbox showcase playlist, 51 enemies (2026-10-11)

Playlist https://www.youtube.com/playlist?list=PLsYpcRmfPaU1fx6b6whkOBC8tBjau4hBZ: one
hitbox-mod showcase per boss and mini-boss, 3 h 59 min in all, the attack spheres drawn on
the live model as in the Ogre clip above. Kept at 1920x1080 60 fps (all 51, 9.9 GB; `-f "bv*[height<=1080][fps>=50]+ba"`) in
`..\clips\hitboxes\<index>_<title>_<id>.mp4` (yt-dlp with a download
archive, `archive.txt` there; rerun the same command to fill gaps). Several descriptions
carry corrections ("read description": missed attacks, the laser deflection, the second
Headless Ape phase), fetch them with `yt-dlp --skip-download --print "%(description)s" <id>`.

| # | Enemy | Length | id |
|---|---|---|---|
| 01 | Inner Owl (father) hitboxes showcase | 10:19 | `e6fIh0ms9eY` |
| 02 | Inner Isshin hitboxes showcase | 8:51 | `xYzbo7xAQjw` |
| 03 | Inner Genichiro hitboxes showcase | 5:59 | `vksr-D2arPg` |
| 04 | Sword Saint Isshin hitboxes showcase | 10:45 | `VwrJJXw-GY0` |
| 05 | Genichiro - Way of Tomoe hitboxes showcase | 4:12 | `_VYO6j34pGA` |
| 06 | Demon of Hatred hitboxes showcase | 14:36 | `2G9xe-5WoJo` |
| 07 | Elite Ashina hitboxes showcase | 1:04 | `N8sJLGm4vqc` |
| 08 | Divine Dragon hitboxes showcase | 5:10 | `mIedkm2qByE` |
| 09 | Okami Warrior (katana) hitboxes showcase | 4:07 | `wJnYikzBWFY` |
| 10 | Deflecting shichimen warrior laser and terror balls with hitboxes showcase | 1:24 | `410D96wmchs` |
| 11 | Okami Leader Shizu hitboxes showcase | 0:55 | `MKlc_l8iXZI` |
| 12 | Okami Warrior (Kemari) hitboxes showcase | 1:22 | `PP4Lfp_aog0` |
| 13 | Okami Elite (Lightning Naginata) hitboxes showcase | 4:58 | `cpAfbbbhd1M` |
| 14 | Okami Leader lightning hitbox | 0:28 | `skSxhb0zCwk` |
| 15 | True Monk hitboxes showcase | 10:30 | `ViDLU8wIJwc` |
| 16 | Headless Ape hitboxes showcase (+ 2nd phase in description) | 6:55 | `1AtEvqj2GdA` |
| 17 | Owl father hitboxes showcase | 7:28 | `s5qQcEa8Rpw` |
| 18 | Lady Butterfly hitboxes showcase | 9:13 | `TroiayyPlS4` |
| 19 | Elder Monkey hitboxes showcase | 4:01 | `OLPDtIxFb0c` |
| 20 | Juzou hitboxes showcase | 5:41 | `eMku0XNjtyA` |
| 21 | Taro Troop (Kanabo) - hirata estate - hitboxes showcase | 2:27 | `heCRBogLsUQ` |
| 22 | Taro Troop (Bell) hitboxes showcase | 4:14 | `eH4G3aRziJM` |
| 23 | Lone Shadow Vilehand hitboxes showcase | 4:34 | `Dd2cbcnoNFI` |
| 24 | Taro Troop (Greatshield) - hirata estate - hitboxes showcase | 2:56 | `3DtTtSXOFPE` |
| 25 | Shinobi Hunter hitboxes showcase | 2:09 | `207UVG6xxEM` |
| 26 | Isshin Ashina hitboxes showcase | 8:56 | `4mibTB9qeK8` |
| 27 | Emma hitboxes showcase | 4:52 | `4A9MSV-iLYQ` |
| 28 | Great Shinobi Owl hitboxes showcase | 8:36 | `QwOBKHvBafc` |
| 29 | Red Guard hitboxes showcase | 2:17 | `mYYnhF_X3kA` |
| 30 | Deflecting underwater headless attacks | 1:37 | `6l6l53AQv-k` |
| 31 | Ungo Headless hitboxes showcase | 2:33 | `PI1DVLr6L2E` |
| 32 | fake Monk hitboxes showcase | 5:06 | `QxzM5bfw8yA` |
| 33 | O'Rin of the water hitboxes showcase | 2:36 | `uDi7vHc4ZyE` |
| 34 | Snake Eyes Shirahagi hitboxes showcase | 2:49 | `JGZqinA8yJk` |
| 35 | Why my centipede giraffe no deflect works every time (hitboxes showcase) - read description | 1:24 | `dpTfZEkDxOM` |
| 36 | Chained Ogre hitboxes showcase | 1:49 | `UpRxNisYue0` |
| 37 | Genichiro Ashina hitboxes showcase (two attacks missed, read description) | 12:45 | `Bvr1rrTEkPM` |
| 38 | Nightjar ninja hitboxes showcase | 1:08 | `8pxg1N7OxXQ` |
| 39 | Guardian Ape hitboxes showcase | 7:24 | `3A1hrh6nmZw` |
| 40 | Ashina Seven Spear hitboxes showcase | 3:36 | `BUi7OujrfvU` |
| 41 | Sen'un hitboxes showcase | 0:59 | `IUlZFhJspBY` |
| 42 | Spear Adept Monk hitboxes showcase | 4:50 | `XVCQjYQ7o6c` |
| 43 | Shichimen Warrior hitboxes showcase (read description for laser deflection) | 1:42 | `-tDichC4e5A` |
| 44 | Armored Warrior hitboxes showcase remake full deflect | 1:49 | `cIDgk-rGv5E` |
| 45 | Armored Warrior hitboxes showcase | 3:31 | `x10ky6He3Vo` |
| 46 | Blazing bull hitboxes showcase | 2:13 | `zzltbsfEtFE` |
| 47 | Gyoubu hitboxes showcase | 7:29 | `5H5fDHVA5Is` |
| 48 | Great Serpent first encouter hitboxes showcase | 1:21 | `sEMKFRkxGvM` |
| 49 | General Kawarada hitboxes showcase | 5:54 | `jVRGcFu4_7o` |
| 50 | Leader Shigenori Yamauchi hitboxes showcase | 2:09 | `AAD1X74kDK4` |
| 51 | Headless hitboxes showcase | 5:59 | `vVzI0SRWRhw` |

Use: Bosses' B1 hit-capsule verification for every fighter here against the AtkParam
export (sphere radius and dummy poly per attack frame), the way the Chained Ogre clip was
checked; VXf's attack effects per boss.

Bosses' note (2026-10-11): these play at full speed with the camera moving. A 1 s sheet of #05
(Way of Tomoe, 0:00-1:00) shows the blade capsule along the sword and ground cages for the
sweeps, but one attack lasts only 2-4 frames there. So use them per attack: find the move in
the sim trace (anim + AttackBehavior frames), then pull that moment at fps=60. They're for the
cases where a sim hit looks wrong, not a full pass. The Ogre (c5020) section above is the full
check, and it matched the AtkParam export.

## Combat flow reference, ONGBAL "Sekiro's Combat Is Still Best" (2026-10-11)

https://www.youtube.com/watch?v=dReREg4Dtyc, 7:00, kept at 1920x1080 60 fps (H.264, 294 MB) as
`..\clips\sekiro_combat_flow_ongbal_dReREg4Dtyc_1080p60.mp4`. Chapters:
0:00 Juzou, 1:37 Nightjar Elite, 3:28 Lone Shadow, 5:10 Seven Spear (ends with a leap off a cliff and a fade-out at 6:59; no death screen, see the correction below
). **Caveat: this is a modded game.** The description credits *Sekiro Resurrection 1.16.1*
(Nexus 723, a "complete overhaul", tagged Animation-Modified and NPC Behaviours) and *Multi Combat
Arts* (Nexus 1058, several arts equipped at once). So the enemy movesets, the flame arts the player
chains (the fire arcs all through the Juzou chapter, the burning status kanji on enemies) and the
art-to-art chaining are **not vanilla data**; what transfers is the *feel*: no idle
gap between a deflect and the next input, prosthetic use mid-chain, arts out of a deflect, lock-on
camera that never fights the player.

What the contact sheets (1 fps per chapter, 6 fps on 0:20-0:40 and 3:35-3:55) show:

- Cadence: deflect spark rings come every 1-2 s during an enemy string; the player's follow-up
  slash or art starts within about 0.3 s of the spark (2 frames at 6 fps), never a visible neutral
  pose between them.
- Prosthetic in the chain: firecracker flash -> immediate art; a Spear/Axe-class tool swing lands
  between two deflects without a step back first.
- Arts chain back to back at roughly 0.7 s per art in the Juzou chapter (modded multi-art, so the
  *count* is not a target, the lack of recovery pause between an art and the next deflect is).
- The camera stays locked and level through all of it; the player circles the enemy during enemy
  wind-ups rather than stepping away.

For Sv1 this is the acceptance reference for B3 (camera/controls feel, VXf) and for the player's
cancel windows (TAE cancel events 16/... already ported; check that deflect -> attack, deflect ->
art, tool -> art and art -> deflect each allow input in the frame the game's TAE opens it, not a
frame later). Frame grabs: `ffmpeg -ss <t> -i <file> -vf fps=30 -frames:v N` (H.264, seeks fast).

**Checked 2026-10-11 (VXf): the chain windows match the TAE.** Test
`deflects_arts_and_tools_chain_on_the_cancel_frame` (src/sim_tests.rs) holds each next action from
the start of a state and checks it fires on the tick after its execute flag (115 attack / art, 117
guard, 137 tool) opens. That one tick is the game's own: the HKS reads the flags the previous
frame's TAE update set (live recording, docs/kb/input-buffer.md). Results, 60 Hz ticks from the
state's start:

| From | Then | Flag opens | Fires | Into |
|---|---|---|---|---|
| StandDeflectHardSmall_F | attack / art | 12 | 13 | HardDeflectAtkSmallF / GroundSpecialAttackCombo1 |
| StandDeflectHardSmall_F | tool | 18 | 19 | GroundSubAttackCombo2 |
| StandDeflectHardLarge_F | attack / art | 36 | 37 | GroundAttackCombo1Reverse / art |
| StandDeflectHardLarge_F | tool | 49 | 50 | GroundSubAttackCombo2 |
| tool tap (Combo1Release) | art / guard | 11 / 17 | 12 / 18 | art / StandToDeflectGuard |
| art (whirlwind) | attack, tool / guard | 54 / 60 | 55 / 61 | |
| attack tap | guard | 0 | 0 | StandToDeflectGuard |

Guard during a deflect recoil does not change state: frames 0-9 (ref 228 without 412) it plays the
additive deflect (AddHardDeflectGuard) over the recoil, as HKS c0000_transition.lua:4438 does, so
the deflect itself is live from frame 0; from ref 412 (frame 9) it moves on to
StandToDeflectGuard1Reverse. So nothing in Sv1 holds an input back past the game's window; the
remaining feel gap is the root-motion blend-in (HANDOFF item 3), not the cancel frames.

## Medal screen recordings (user's own, indexed 2026-10-19)

13 screen recordings in `C:\Medal\Clips\Screen Recording\` (`MedalTVScreenRecording<stamp>.mp4`, 2580x1080 at 60 fps, the game in the left 1935 px, the rest is the desktop). Only four show the real Sekiro; the others are our Sv1 clone (checker floor or the real map with the debug menu) or another game. Check the clone ones before using them as reference. Frame grabs: `ffmpeg -ss <t> -i <file> -vf crop=1935:1080:0:0 -frames:v 1`. `C:\Medal\Clips\sv1` is empty. A scan against the atlas questions is in `atlas/verify/clips-check.json`.

| File (stamp) | Length | What it shows | Notable times |
|---|---|---|---|
| 20261005070753561 | 4:15 | EldenKill (ULTRAKILL in Elden Ring), first person. Not Sekiro. | none for Sekiro |
| 20261007190653883 | 0:35 | Sv1 clone, duel against a swordsman on the checker arena. | Windows overlay 0:20-0:26 |
| 20261009042448864 | 2:49 | Sv1 clone, Wolf against an armoured enemy and a mounted one, debug panel on the right. | Medal settings window 2:24 |
| 20261009182556411 | 0:12 | Sv1 clone, short fights on the checker arena. | none |
| 20261009182637721 | 0:14 | Real Sekiro. Wolf idles and walks about a burning castle gate at night next to a glowing lamp, no enemies. | 0:04 purple wisp, no rest |
| 20261009182708278 | 0:33 | Sv1 clone duel. The real game flashes up for a moment while switching windows. | real game 0:18-0:22 |
| 20261010013052179 | 0:17 | Real Sekiro, burning snowy castle area. Behind deathblow on a red-armoured soldier on a wooden scaffold, then Wolf walks down a plank and runs over snow. Vitality stays full. | deathblow 0:02-0:05, plank 0:06-0:08, hop from rock onto roof 0:14.5 |
| 20261010013749267 | 0:14 | Real Sekiro, snowy bamboo clearing. Wolf repeats forward tumbles and gets up, no enemies, vitality full. | whole clip |
| 20261010013834205 | 1:27 | Real Sekiro, fight with several Ashina soldiers on stairs and in a courtyard, fire arts at 0:14-0:22, Healing Gourd at 0:51-0:53. Vitality drops at 0:21, 0:36 and 0:49. | deathblows 0:30-0:33 and 0:44-0:46 (lock-on switches at 0:33.0), perilous mark 0:48-0:49 |
| 20261010054828588 | 0:07 | Sv1 clone, two models on a blood-sprayed checker floor. | none |
| 20261011222656376 | 2:50 | Sv1 clone on the real castle roof map: title menu at 0:00, fight with a blue-robed boss, perilous mark at 0:40 and 0:52, Pause popup at 1:16 and 1:48, then walking in a snowy courtyard and field. | menu 0:00, pause 1:16 |
| 20261011233458607 | 0:05 | Sv1 clone, dark roof with a boss list menu overlay. | none |
| 20261014131548464 | 0:02 | Sv1 clone with the debug menu: Wolf behind a passive test enemy at a shadow edge in daylight (stealth test). | whole clip |

Findings from the real clips and the downloaded ones (2026-10-19):
- Lock-on switch: in 20261010013834205 the lock dot sits on a dying soldier at 0:32.8 and on another soldier at 0:33.0, so the lock jumps to another enemy at once when the target dies.
- Small drops: in 20261010013052179 steps of 1 to 2 m cause no damage and no landing stagger. No medium-height fall is recorded.
- Camera on a fall: it pitches down within about 0.3 s of a leap, in the combat-flow clip at 6:55.9 (modded game) and in 20261010013052179 at 0:14.2.
- Correction: the combat-flow clip has no death screen at 6:40. At 6:45-6:51 there is a red Bloodsmoke cloud, and at 6:59 the clip fades out while Wolf falls off a cliff.
- Still not settled by any clip: fall death and return, ledge stop, death noise, shadow crouch, blind duration, dodge through a swing, death screen timing, pause menu, idol rest, cutscene skip, New Game+, Dragonrot.
