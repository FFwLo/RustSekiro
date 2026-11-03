# docking: wall hug (cover) and ledge hang

Owner: VXf. Static research 2026-10-11; **built 2026-10-17** (`src/docking.rs`, section 7).

## 1. Button

HKS ACTION_ARM_WALL_HANG (12). The help text (menu FMG) uses key icon `Kc@20`
"会話/調べる/壁張り付き/ぶら下がり/空中ぶら下がり掴み" (talk / examine / wall hug / hang /
air hang grab): "Hug wall", "near edge: Ledge Hang", "in air near ledge: Ledge Hang",
"Holding Kc@20 while falling or jumping can make it easier to grab a ledge".

The `Kc@N` ids are DefaultKeyAssignParam `gameAction` rows. Row 20 = pad 2006 and keyboard 87
in all 5 presets. The keyboard ids run along the QWERTY rows (W 86 / S 100 / A 99 / D 101 =
the move rows 1-4, Q 85 crouch, R 88 use item, F 102 grapple, Space 126 jump), so 87 = E.
Since 2026-10-17 the game uses the real defaults (R use item), so E is free for hug / hang.
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

## 7. Built (2026-10-17, src/docking.rs; sim tests part12)

- **Button**: E (the game's default, `DefaultKeyAssignParam00` gameAction 20; the keyboard now
  uses the game's defaults, so R is use item and E is free). `PadInput::interact_held` is env 1108
  for the air catch. gap: the pad button (pad id 2006 is not decoded).
- **Edge frame (section 5's open question)**: the wall is on `c`'s side. After 2026-10-11 the
  objects' collision is in the .hit, and `edge_wall_side_probe` finds the wall within 0.2 m on
  c's side for every gate hug edge that has one near; the gate's hang edge has c on its cliff
  face, 10 m down. `out` = from the wall into the open (where Wolf hugs / hangs).
- **Offsets checked against the clips** (`docking::pose_probe`, bone positions in model space):
  HangIdle's hands are 1.56 m over the root and 0.62 m in front of it (= _Hang (0, 1.6, -0.7):
  the hands on the lip, the body at the wall); CoverIdle's pelvis is 0.50 m behind the root, so
  the root 0.7 m out from the edge puts the back at the wall.
- **Hug**: StandToCover2* by the stick-idle angle table (section 2) -> CoverIdle; Cover{Walk,Run}
  {Start,Loop,End}_{L,R} along the edge (Walk with the walk key). The clips' _L / _R are named
  from the front: _R carries Wolf to the camera's right when he faces out (sim test). E or a
  step -> CoverEnd.
- **Hang**: ground start on E at a lip Wolf stands behind (StandToHang1* -> 2*, FootHold set on
  variation 1); the air catch with E held while falling, the lip 1.0-2.6 m over the feet and
  Wolf up to 1 m (short) / 2 m (long range) off the face; HangIdle; HangMove{Start,L/R,End}
  (HangMoveL plays a000_020202, its CMSG animId is 0); E or jump -> HangToStand (lifted onto
  the top over the clip); a step -> HangToFreeFall -> FreeFall.
- **Holding** (`docking::hold`, FixedUpdate after the root motion): the start clips blend Wolf
  from where he was to the docked spot (smoothstep over the clip); held, the clips' own root
  motion carries him along and he is put back on the line, stopped 0.35 m from the ends;
  `Actor::docked` keeps `bodies_on_terrain` from sliding or dropping him, and decide's
  "nothing under the feet" fall skips him.
- gaps: the exe search box (reach values in docking.rs), the start-angle sign convention, the
  peak move (COVER_EDGE_TYPE 10-13), swimming / diving (no water in the maps yet).

## 8. Parkour finished (2026-10-17, same day; sim tests part12: 11)

Read from the HKS bytecode (`hksc_disassembler` 0.2.0 from github.com/Surasia/hksc-disassembler,
installed in a scratch venv): c0000_define WALL_JUMP_COUNT_MAX = 1, WALL_JUMP_ENABLE_FALL_HEIGHT
= 2000, SELECTOR_WALL_CONTACT_TYPE F/B/L/R = 0-3, SELECTOR_WALL_JUMP_TYPE VERTICAL / H_F / H_B /
H_L / H_R = 0-4; c0000_transition BEH_A_WALL_JUMP (16904-17000) and the WallJumpReady update
(6683-6960).
- **Wall jump**: jump in the air (fall, ground jump, sprint jump, a wall jump's take-off) with a
  wall within 0.8 m of the chest (gap: the exe's contact check; rays in 8 directions):
  WallJumpReady_<contact> by ContactWallAngle (-45..45 F, beyond +-135 B, -135..-45 L, else R),
  once per flight. At its end WallJumpStart_<contact>_<type>: no stick N (vertical), else the
  stick's cone, a jump into the wall N. Its TAE 920 (f1) launches (decide's generic velocity
  change: +2 m in the sim); then Vertical / ForwardGroundJumpFall and the jump's landing (the
  wall jump's fall and land CMSGs play the same clips, a000_201030 / 201040 / 201045).
- **Out of a hug**: jump -> CoverToWallJumpReady (a000_201301, the wall behind: contact B) ->
  WallJumpStart_B_<type> (no stick: straight up the wall). **Out of a hang**: jump with the stick
  away from the wall -> Hang(FootHold)JumpReady (held at the edge) -> WallJumpStart_F_<type>.
- **Hang corners**: at an edge end with the stick on, the hang edge whose end is within 0.6 m
  (gap: the exe's corner checks env 3048/3049/3059/3060): in line -> carry on; its face turning
  along the way Wolf moved -> HangCornerMove{Left,Right} (outside); against it ->
  HangInsideCornerMove* (FootHold sets on foothold edges); blended onto the new edge like a start.
- **Peek** (STYLE_TYPE_COVER_LOOK): at a hug edge end with the stick still on that way, where
  COVER_EDGE_TYPE allows the side (1 free, 2 left only, 3 right only):
  CoverAction{Start,Loop,End}{L,R} (the game spells the left start CoveActionStartL); E from
  it -> CoverEndFromCoverLook.
- **Moving hug start**: E with the stick held -> MoveToCover{Left,Right} by the stick's angle
  to the wall (-150..-30 / 30..150), beyond Back, else the 180s.
- **Air grab window**: the catch also fires without E inside a jump's TAE SpEffect ref 5
  (SP_EF_REF_TAE_ENABLE_AIR_HANG_GRAB).
- **Deathblows** (player.rs dock_deathblow): attack while docked with an enemy that has not
  found Wolf (or is broken), by the start row's Dist / height window / facings: hug 0060 -> 0061
  (enemy on his left: a20x_500600 -> 510600, ThrowDef12600) / 0070 -> 0071 (right), hang 0080 ->
  0081 (enemy facing away) / 0130 -> 0131 (facing him), through decide's DeathblowStart. The hang
  reach is measured from the lip (the hands), not the body 1.6 m under it. The hang kill keeps
  Wolf at the edge and goes back to HangIdle; ThrowDef12800 / 13000 have no ThrowDefDeath (the
  fall is the death). gap: the throw absorb is horizontal (follow_throw), so the enemy is not
  pulled down past Wolf as in the game.


## 9. Feel pass (2026-10-17, the user's playtest of `SHINOBI_MAP=parkour`)

- Starts, corners and the climb follow their clip's own root motion (`docking::guided`: the clip
  path from where Wolf began, in the clip-start yaw frame, plus the gap to the docked spot eased
  in over the clip), not a straight lerp: no sliding through the wall. Clips without root motion
  (FreeFallToHang*, StandToCover2Back) smoothstep onto the spot in 0.3 s and hand over at 0.5 s
  (`NO_ROOT_BLEND` / `NO_ROOT_DONE`; gap: their TAE cancel windows are not read).
- The ground hang and the hug start pick the variant whose root motion lands nearest the docked
  spot (`best_start`). The first ground-hang clip (StandToHang1*, 0.1 s) holds Wolf in place;
  StandToHang2 slips the 1.6 m over the lip between 0.35 and 0.6 s (up to 0.16 m a tick, the
  clip's own drop).
- Loops played on (shimmy, hug walk / run) keep their full step (no blend-in each cycle).
- The peek leans the camera focus 1.2 m out to the peek's side and 0.3 m forward (camera.rs).
- The wall jump turns Wolf to the stick before a horizontal take-off (TAE 920 launches along the
  facing); the ready poses are skipped by the fall check.
- Stairs on the course have a collision ramp (no step popping).
- Checks (part12): the outside corner keeps the root 0.7 m and the hands 0.08 m off the wall, no
  tick step over 0.12 m.

## 10. Second playtest (2026-10-17)

- Starts, corners and the climb hand over `SETTLE` (0.15 s) after their root motion ends
  (`docking::motion_end`), and `guided` eases the gap in over that span: the ground hang is about
  0.75 s instead of 1.4 s.
- The parkour course has a 12 m wall-jump shaft (x 14..17, z -24..-21), with shelves every 3 m on
  alternating walls (one wall jump a flight).
- player.rs: the run quick turn (StandRunQuickTurn*180, a000_000442 / 443; the heading snaps, the
  pose spins, run loop at 0.53 s); a walk-off keeps the move velocity; plain falls turn to the
  stick at 360 deg/s (tuning: FreeFall's TAE says 45).

## 11. Third playtest (2026-10-17)

- The shaft has no shelves; on the parkour map the wall-jump limit is lifted
  (`docking::WALL_JUMP_LIMIT`, map.rs) for wall-to-wall chaining. Other maps keep the game's 1.
- Air turn 120 deg/s (360 was too fast).
- Free guard walk turns to the stick and walks forward (it faced the camera's direction).
- Free walk turn eased (`WALK_TURN_EASE` / `WALK_TURN_ACCEL`, player.rs).
- anim.rs crossfades blend from the source clip still playing (`fade_src`), not a frozen pose.
- The walk jump's speed is the game's: ForwardGroundJumpStart's 920 row 101 is 0 x the move
  speed + 6.4 m/s, for a walk or a run alike.

## 12. Fourth playtest (2026-10-17)

- Air turning reverted (left to the other contributors).
- Chained wall jumps need a different wall each time (more than 90 deg from the last one's
  direction, `Player::last_wall`, `docking::wall_contact_dir`): no climbing one flat wall by
  spamming jump.
- Walk 180: StandWalkTurn{Left,Right}180 plays the in-place StandQuickTurn*180 clip while Wolf
  turns at 540 deg/s, then walks on (a stand-in: the game's walk quick-turn clips do not turn).
- anim.rs: a heading snap at a clip change (an action start's `aim`) turns the model over the
  fade (`Skeleton::yaw_off`), not in one frame; not for the run quick turn (its pose spins).
