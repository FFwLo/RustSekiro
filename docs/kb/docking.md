# docking: wall hug (cover) and ledge hang

Owner: VXf. Static research 2026-10-11. Not built yet. It is parked because boss fights don't
use it (Map's view to the user: hug / hang belong to the stealth part, mini-bosses in the open
world) and the gate map has almost no usable edges (see 5).

## 1. Button

HKS ACTION_ARM_WALL_HANG (12). The help text (menu FMG) uses key icon `Kc@20`
"会話/調べる/壁張り付き/ぶら下がり/空中ぶら下がり掴み" (talk / examine / wall hug / hang /
air hang grab): "Hug wall", "near edge: Ledge Hang", "in air near ledge: Ledge Hang",
"Holding Kc@20 while falling or jumping can make it easier to grab a ledge".

The `Kc@N` ids are DefaultKeyAssignParam `gameAction` rows. Row 20 = pad 2006 and keyboard 87
in all 5 presets. The keyboard ids run along the QWERTY rows (W 86 / S 100 / A 99 / D 101 =
the move rows 1-4, Q 85 crouch, R 88 use item, F 102 grapple, Space 126 jump), so 87 = E.
In our game E is "use item" (Sekiro: R), so the hug / hang key needs a free key.
gap: which face button pad 2006 is (2004 jump = cross, 2005 step = circle; 2000-2003 are the
d-pad, since menu cursor up / down / left / right = 2000 / 2001 / 2002 / 2003).

## 2. When the game starts them (HKS)

- **Cover start** (c0000.lua 759, c0000_transition.lua 6269): on the ground (styles 1-2),
  button pressed, env 3043 IsWallHangPossible, not forced crouch, no TAE SpEffect ref 9
  DISABLE_COVER_START. The start anim, by `CoverStartEdgeAngle` and the stick
  (c0000_transition.lua 4518; angles c0000_define.lua 290-297):
  - stick idle: |angle| > 135 → StandToCover2Back; |angle| ≤ 45 → StandToCover2Right180 /
    Left180 (by sign); else StandToCover2Right / Left;
  - stick held: -150..-30 → MoveToCoverLeft, 30..150 → MoveToCoverRight, beyond → Back,
    else Right180 / Left180.
  - Cover end: the button again → CoverEnd (CoverEndFromCoverLook from the look style).
- **Ground hang start** (6304, 4571): button, env 3044. `HangStartEdgeAngle` -45..45 →
  StandToHang1Back, 45..135 → 1Left, -135..-45 → 1Right, else StandToHang. The FootHold
  variants when the edge's env 3046 type = 1 (HANG_EDGE_TYPE_ENABLE_HOLD_FOOT).
- **Air hang start** (6312, 4602): env 3045 air hang type ≠ 0 and either the TAE ref 5
  ENABLE_AIR_HANG_GRAB window, or (standby or TAE ref 105 ENABLE_AIR_HANG) with the button
  held (env 1108 > 0). Angle -180..-45 → FreeFallToHang(LongRange)Left, 45..180 → Right,
  else FreeFallToHang(LongRange). Type 1 = short range, 2 = long range.
  Holding the button in the air without the grab window = AirHangGrabStart / Loop / End.
- **Hang end**: button or jump → climb (HangToStand), SP move (step) → HangToFreeFall.
  Corner moves need env 3048 / 3049 / 3059 / 3060 (corner checks) and the stick.

## 3. How Wolf is held to the edge (c0000.hkx)

105 CustomDockingGenerator nodes. Each binds `translationOffset` to one behavior variable.
Initial values (hkbBehaviorGraphData variableInitialValues; read with a scratch probe over the
extractor's hkx.rs):

| variable | value | used by |
|---|---|---|
| DockingTranslationOffset_Cover | (0, 0, 0.7) | Cover*, StandToCover2*, MoveToCover*, a000_500600/500700/510600/510700 (hug deathblows) |
| DockingTranslationOffset_Hang | (0, 1.6, -0.7) | Hang*, FreeFallToHang*, StandToHang2*, a000_500800/501000/510800/511000 (hang deathblows) |
| DockingTranslationOffset_HangStart | (0, 0, -0.4) | StandToHang (rotation 180° about Y), StandToHangFootHold1 |
| DockingTranslationOffset_HangBackStart | (0, 0, 0.4) | StandToHang1Back, StandToHangFootHold1Back |
| DockingTranslationOffset_HangLeftStart | (-0.4, 0, 0) | StandToHang1Left, StandToHangFootHold1Left |
| DockingTranslationOffset_HangRightStart | (0.4, 0, 0) | StandToHang1Right, StandToHangFootHold1Right |
| HangWallAngle | 90 (bounds 80-90) | |

blendType 0 = the start nodes (blend in), 1 = the held states. HangToStand /
HangFootHoldToStand have no offset (titleDokingType 11). gap: the sign convention of the offset
(edge → Wolf or Wolf → edge, the edge frame's axes). Exe: the docking search is
FUN_140b7d810 / FUN_140b89320 (cover box, DockingTranslationOffset_Cover) / FUN_140b76110
(hang); the box size uses DAT_143d85db4 × 0.5 (writer not found).

## 4. Animations (exported 2026-10-11)

tools/sekiro-extract/export_states.txt "Wall hug (cover) and ledge hang" section, all in
combat_data.json: CoverIdle 21000, CoverWalk/Run Start/Loop/End L/R 211xx-216xx,
StandToCover2* 218103-218107, MoveToCover* 218111/218112, CoverEnd 218490, HangIdle 20000,
HangFootHoldIdle 20010, HangMove* 201xx-203xx (HangMoveL has animId 0 in the CMSG),
corner moves 204xx, StandToHang* 2171xx, StandToHangFootHold* 2172xx, FreeFallToHang*
2171xx/2172xx/2173x, HangToStand 217500, HangToFreeFall 217520, HangFootHold* 2176xx.

## 5. The map's edges (`map::LedgeEdges`)

- Hug (kind 3) `variation` = HKS COVER_EDGE_TYPE: 0 no look, 1 free look, 2 left look only,
  3 right look only (10-13 the "peak move" versions). Hang (kind 2) `variation` =
  HANG_EDGE_TYPE: 0 no foothold, 1 foothold.
- Gate map (m11_01_00_00): 39 hug edges, 1 hang edge (o113500, 10.3 m over the floor below).
  The arenas: 0 hang, 7 hug (Lady Butterfly's hall).
- Probe `docking::tests::edge_wall_side_probe` (ignored): rays out both sides of each edge's
  middle. Most hug edges find no wall within 1.5 m. They come from objects, whose collision
  isn't in the .hit. The wall side vs the a-b direction could not be settled. `c` lies about
  0.1 m to the left of a-b's middle (left = +Y × (b-a), Bevy space) and 0.13-0.17 m below on
  the long edges, but elsewhere on short ones.

## 6. To build it later

Needs: the object collision (or a sandbox wall with a synthetic edge), the edge-frame
convention, a free key, then the states above plus the hang / hug deathblows
(`Combat::throws_of_kinds`, ThrowKind 90000/100000/110000).
