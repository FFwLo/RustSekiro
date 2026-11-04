# HUD, menus and saves

## 2026-11-04 P11: the HUD laid out from the game's own movie

**Format.** The HUD and the menus are Scaleform movies, `menu/*.gfx` in the game's data (an SWF with a
"GFX" header, version 11, 1920 x 1080 stage, 60 fps). Their layout is in the timelines, not in a param
or a FromSoftware layout file: every `DefineSprite` places characters with `PlaceObject2/3` (a depth, a
matrix in twips, a colour transform, an instance name), `FrameLabel` marks the clips' parts (`FadeIn`,
`Loop`, `FadeOut`, `State_0..3`), and the bitmaps are Scaleform `DefineExternalImage2` (tag 1009)
naming the atlas sprite (`MENU_HP_bar_L`, cut from `menu/hi/01_common.tpf` by its `.sblytbnd` layout,
which only holds the sprites' rectangles in the atlas). Text fields (`DefineEditText`) carry their box,
font height and colour. What the movie's ActionScript (DoABC) sets at run time is not in the timelines.

**Reader.** `tools/sekiro-extract/src/gfx.rs`: `sekiro-extract gfx <in.gfx> <out.json>` dumps a movie's
whole tree; `sekiro-extract hud-layout <extracted>` (run by `tools/extract.ps1`) writes
`extracted/hud/layout/<movie>.json` for 01_000_fe (the HUD), 02_000_ingametop, 02_200_mainmenu,
02_202_equipweaponselect, 03_206_skill, 05_230_optionsetting and 01_002_fe_saveicon: every instance at
frame 1 by its path of instance names (`PlayerHUD/Gauge/HP/...`, unnamed ones `#<depth>`), its world
matrix and bounds on the stage, the sprite it draws, its composed colour transform and text field; and
each sprite's labels. Checked against the numbers read by hand before (PlayerCount at 1849.9, 994.6;
Money 1702, 748.5; SkillPoint 1484, 88.4; ResurrectionList Icon_0..2 at x 129.8 + 46.8 i; the
Lockon_Ani_297 labels 1 / 16 / 84 of 90): all equal.

**Use.** `src/hud_layout.rs` resolves the HUD's elements by path (`HudLayout::from_json`); its
defaults are the same numbers (the test `the_hud_layout_defaults_are_the_fe_movie` keeps them equal),
so CI lays the HUD out the same without data. From the movie now (hud.rs):

| Element | Movie path / sprite | Was (by eye) |
|---|---|---|
| Vitality bar start, top, height | PlayerHUD/Gauge/HP .../Current MENU_HP_bar_L: x 111.8, y 976-989.2 | 5.7 %, bottom 7.8 %, 1.25 vh |
| Resurrection nodes | ResurrectionList Icon_0 MENU_Revive_Window_2 103.3, 910.9, 51 x 53; pitch 46.8 | in a column over the bar |
| Wolf's posture | SP origin x 960, MENU_Taikan_bar y 926.3-947.1, SPFrame base_03 663.8 wide | 27 vw x 1.3 vh, bottom 12.6 % |
| Build-up icons | ConditionGaugeList Item_0_0 MENU_Resist_Poison 767.8, 733.1, 51 x 67; next 54.4 up | 34 x 44 px over the vitality bar |
| Quick item | ItemPanel Item_Bottom: MENU_Item_Top box 1503.9-1600.4 x 935.7-1025.5, icon, Stock field (24 px, right) | MENU_Item_Left box |
| Prosthetic tool | Item_Left: MENU_Item_Left box 1595.3-1743 x 874.8-1031.3, icon 1607.4-1728.3 | MENU_Item_Top box |
| Art / ninjutsu marks | Item_Top MENU_FE_Skill, Item_Right MENU_FE_Ninsatul (40 px) | 1.9 % column |
| Spirit Emblems | PlayerCount Soul State_0: MENU_HP_base_4 backing, MENU_ItemIcon_00001, count field | no backing |
| Sen purse | Money Layout: MENU_Caption_02, MENU_Money, TotalSoul and GetSoul fields | dark box |
| Skill gauge | SkillPoint Layout: MENU_ME_Caption, the bar from 1492.4 to the cap at 1755.6, MENU_SkillPoint, cyan points (22 px), AddExpPoint (18 px) | dark box |
| Boss bars | BossList Item_0_0: HPGauge 112.3-523.1 (y centre), HP Icon_0 orb 50 px / Base_0 41 px, Name field 25 px grey 204, SPFrame base_03 763.4 wide, SP Bar0 y 58.1-78.7, State_1 glow 805 x 34.6 | orbs 1.9 / 1.0 vw, glow 21.1 vw |
| Text | the fields' heights (Vh of the stage) and colour 204, 204, 204 | warm white, px sizes |
| Purse / gauge time up | Money_164: FadeIn 1, FadeOut 138 (plays through its hold): 2.28 s | 4 s |

**Still by eye / not from the movie** (`gap:` in hud.rs / hud_layout.rs):
- The vitality bar's length per maximum Vitality (script; the bar sprites are drawn at their longest),
  the posture fill's half length (script scales it; the frame's width is used), the boss orbs' spacing
  past the first, MENU_Sp_Center's place (none placed at frame 1).
- Fill colours: the vitality fill is several layers (Current: MENU_HP_bar_L x (0.602, 0.191, 0.148) +
  (-26, 12, 9) under a 0.31-alpha overlay; Delay x (0.5, 0.5, 0.52)) with blend modes the reader does
  not apply, so the user's captured colours stay; the boss vitality keeps the user's thin look
  (1.25 vh centred on the movie's 17.9 px MENU_HP_bar_Boss).
- Fades: the clips' FadeIn / FadeOut frames are exported (HudLayout `*_clip`) but the HUD still cuts.
- The over-head bars keep the user's 8.5 vw (the movie's EnemyTag0 posture frame is 160 px = 8.3 %,
  its vitality frame 133 px).
- The deathblow mark: MENU_ninsatu_02 is only placed in MenuVFX, not as a HUD instance; the engine
  draws the mark.
- Not looked at in a game window (the task forbids starting it): the placement is checked by the
  tests and the numbers only.

## Saves (src/save.rs)

RSekiro's own file, `saves/<slot>.json` under the root (`SHINOBI_SAVE=<slot>`, default `slot0`,
`0` off; tests never save unless asked). Written at an idol rest (idol.rs `rest` -> `save::request`)
and on quitting; read on the first Playing screen. Its chunks follow the exe's own save chunks
(SaveData* RTTI classes, the write / read pair of each, read from the decompiled exe, never copied):

| ours | game chunk | kept |
|---|---|---|
| `game` | SaveDataGameDataMan (0x138-byte block) | play time (GameDataMan+0x9c), cycle (+0x70) |
| `player` | SaveDataPlayerGameData (PlayerGameData serializer: player info 0x180, 0xb0 block, EquipGameData, FaceData, 2 MenuRefSpecialEffect, u32 list) | Vitality, Attack Power, Sen, skill points / exp / level, gourd charges and fill, emblems, resurrection nodes and power, equipment (art, prosthetics, quick items and slot, ninjutsu), items, tools, upgrades, skills, look |
| `event` | SaveDataEvent (EventFlagMan's own write) | flags on (boss and world scripts), idols lit |
| `start` | SaveDataStartPointInfo (0x50 bytes from GameMan) | map blocks, position, yaw, last idol |

A loaded save's flags are set on every world script when it starts (enemy/world.rs `load`), its lit
idols go back into idol.rs's list, and Wolf's place only when the save's map blocks are the run's.
Gaps: the other chunks (Gaitem, EquipHistory, Gesture, Chr/Obj init states, Ghost, Network,
Trophy, MenuSystem, options: config.toml) are not kept; which player-info field is Sen and that the
u32 list is the skills are not checked field by field; loading onto another map needs a new run.
