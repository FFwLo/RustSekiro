# grapple: the Grappling Hook ("wire" in the game's data)

Dated findings, oldest first. Append new entries at the end. Owner of this file: Map; peers
append their own sections.

Added 2026-10-11: many bosses need the grapple.
Everything the engine calls the hook is "wire" (ワイヤー): WirePointSearchParam, WireSetParam,
WireVariationParam, HKS `ACTION_ARM_WIRE_SHOOT`, TAE events 900-905 and 921, SpEffect fields
`chrWire*`, HUD `MENU_Wire_*`. Static research 2026-10-11 (Map), nothing live yet.

## 1. Where the grapple points come from

Two sources, both placed by the MSB (a third, the `o000100` "marker" objects, turned out to be
item spots):

- **Object `.edge` files.** 59 of the extracted objbnds carry `e<id>.edge` (SoulsFormats EDGE:
  header `u32 4, count, id, 0`, then 64-byte records: V1, V2 (each vec3 + 1.0), V3, `f32 unk2C`,
  `i32 unk30` (= 1-based index), `u8 type` (1 grapple, 2 hang, 3 hug), `u8 variation`,
  `u8 unk36`, 0, 0, 0). 39 of them hold type-1 grapple edges with VariationID 0/1/3/5/20/40/41
  (o002700 = the kites, var 40/41 "kite temple <-> valley / Honjo"; o1756x0/o1758x0 var 20
  "can't ride"). Example o002040: three grapple edges, V1 ~ V2 (a point), V3 0.3 m above and
  aside = the landing spot. Coordinates are in the object's local space; apply the part's
  transform (msb::Part::transform). 35 of these models are placed on m11_01_00_00 (o003610 x84,
  o003600 x29, o111800 x18, o003500 x14, ...): trees, roofs, poles. The gate map export has
  113 of their points within 90 m of the General.
- **Map `.edge` files** (`map/<id>/<id>.edgebhd/.edgebdt`, BXF4, one `.edge` per collision part
  `h<id>_NNNNNN`, local to that part): on m11_01_00_00 they hold 963 hang (2) + 1326 hug (3)
  edges and **no grapple edges**; Hirata m10_00_00_00 393 hang + 864 hug, none either. Ledge
  hangs and wall hugs come from here later (HKS `BEH_A_GROUND_HANG_START`, `COVER_*`).
  Since 2026-10-11 the exporter writes the hang / hug records of both sources within the
  radius + 10 m raw into the map json as `"ledges"` ({a, b, c, kind 2 / 3, var, src}, the
  three vertices in the arena frame); the game loads them into `map::LedgeEdges` (Bevy
  space, X mirrored) and draws them with H (hang orange, hug cyan, a-b line, tick to c).
  gap: which vertex is what per kind (a-b is the segment; c looks like the stand / hang spot).
  Maps exported earlier have no `"ledges"` key and load none.
- **Not a source: `o000100`** (a 1347-vertex FLVER with dummies 90 at (0, 0.27, 0) and 170 at
  (0, 0.77, 0), placed 60-89 times per map; o005300, o005390, o177800 carry dummy 90 too).
  It was read as a hook marker until the exporter check of 2026-10-11: every one of the 40
  within range of the General is the target part of an MSB Treasure event (EventParam type 4,
  TreasurePartIndex at its type data +8), 0.25 m over the floor (o000100_0073 = entity
  1111510, m11_01 event 11115652 "Set Object Treasure State"); o005300/o005390 are the corpse
  pickups. The exporter skips every treasure part (`msb.treasure_parts`).

Which edge the engine prefers when several overlap is not in the data (gap: exe
`WireEdgesPrioritize` env 3035 / `IsWirePossible` env 3000).

## 2. Search rules (params)

- **WireSetParam** picks up to 20 WirePointSearchParam rows per map state; row 0 (default) =
  search rows [0, 100]; the others are per-event sets (1100200 "Castle valley crossing" ->
  19920 etc., chosen by event scripts; not needed for arenas).
- **WirePointSearchParam 0**: actionDistanceNear 3 m, actionDistanceFar 15 m (the hook can be
  fired), findDistanceFar 25 m (the point is shown but greyed: `MENU_Wire_NG`),
  filteringDistanceHeightUpper/Lower 10.2, filteringDistanceWidth 14.3 (the screen-space
  rectangle a point must be inside to be a candidate), softlockDistanceHeight 10.2 /
  Width 7.15 (once a point is the target it stays while inside this smaller rectangle).
  Row 10200 (Hirata, "recognition range expanded") finds to 50 m; 11020 acts to 20 m.
- **WireVariationParam** (per edge / per enemy SpEffect): targetMarkerType (0 filled circle,
  1 ring with an arrow = "can't ride"), extendWireActionDistance (row 5: +10 m),
  recognitionAngleUp/Down/Left/Right (the player must be within these angles of the edge's
  reference direction: row 0/3/20/23: 90/90/180/180, rows 1/2/21/22: 90 all round),
  chrPhysicsHomingIdOffset (row 20 "can't ride": 20 -> homing 300000020; rows 24 -> 24, 40/41
  kites -> 40/41; enemies: 10700.. -> 300010700..).

## 3. The player's wire move (HKS + TAE c0000 a00, all exported in clips_c0000.json)

HKS: `BEH_A_GROUND_WIRE_SHOOT` (107) and `BEH_A_AIR_WIRE_SHOOT` (108) validate with
`env(ActionRequest, ACTION_ARM_WIRE_SHOOT)` (or the TAE add-action input ref 415 + cancel 416)
and `env(ActionUnlocked, ACTION_UNLOCK_TYPE_SUB_WEAPONE)` and `env(IsWirePossible)` (3000) and
not force-crouching. Ground shoot is allowed in styles stand, crouch, cover, ground guard, sprint
and swim; air shoot in free fall and wire fall (`g_behaviorTable`). On fire: `act(WireAction, 0)`
(3018), `SetVariable("WireMoveStartIndex", GetWireMoveStartIndex(state))`: upslope / vertical /
horizontal from `hkbGetVariable("WireDramaticAngle")` (> 50 deg vertical, > 15 upslope, else
horizontal) when already in a wire state, else the stored index; `act(SetMovePrecalculation,
GROUND_WIRE_MOVE_START, 202010/202011/202012)`.

State -> anim (statemap_c0000.json) and the TAE events that matter:

| state | anim | length (TAE) | events |
|---|---|---|---|
| GroundWireShoot | a000_202000 | 0.63 s | SetTurnSpeed 1080 deg/s 0-0.2; **921 Event921 {4, 300000000}** 0-0.2 (homing param 300000000, target type 4 = the wire point); 900 WireSyncWithTarget_Start {50} 0.47-0.63 + 902 WireLookAtTarget (the hook flies, Wolf is pulled); flags SetNoGravity, DontCancelIfFalling, Disable Turning 0.2+ |
| GroundWireMoveStart_UpperSlope / _Up / _Vertical | 202010 / 202011 / 202012 | 1.93 s | 903 WireEventUnk01 {950} 0-0.1; 901 WireSyncWithTarget_Travel + 902 0-0.6 (reeling, position synced to the rope); 904 WireSlideToStartPoint 0.5-0.67; **920 ChrPhysicsVelocityChange row 1100** at 0.67 (202010) / 0.5 (202011/12) = launch; PCBehavior judge 220 0.5-1.93 (body push); cancels: jump 0.83, R1/R2 0.83, L1/L2 1.0-1.17 |
| WireMove | 202020 | loop | the flight; cancels open from 0 (attack, art, tool, jump, guard) |
| LandWireMove | 202035 | 1.53 s | the landing on a ride point (turn 0); PCBehavior 211 0-0.1 |
| WireFallLandReady | 202045 | 3.33 s | landing out of a wire fall |
| AirWireShoot | 202100 | 0.6 s | 905 WireDisableFalling 0-0.5 (hangs in the air), 921 {4, 300000000}, 900 {50} 0.37-0.6, turn 1080 |
| AirWireMoveStart_Up/UpperSlope / _Vertical | 202110 / 202112 | 1.37 / 1.27 s | 903 {950}, 901+902 0-0.3, **920 row 1100 at 0.17** |
| AirBreakFallWireShoot / AirWireMoveStartFromBreakFall | 202300 / 202310 | | same as the air pair, out of a throw's large-blow fall |
| WireMoveWireShoot / WireMoveWireMoveStart | 202600 / 202610 | 0.43 / 1.43 s | wire-to-wire: turn 2700 deg/s, 905 0-0.33, 900 0.2-0.47, 920 at 0.17 |
| WireMoveWire2Shoot / ...2MoveStart | 202700 / 202710 | | the second chained wire (HKS `W_WireMoveWire2Shoot` when already in WireMoveWireMoveStart) |
| SwimWireMoveReady / SwimWireMoveStart_* | 528000 / 202410 | | from water |
| WireFallDeflect, WireMoveDeflectGuardStart/Loop/End | a050_203400 / 203410 / 203420 | | guard in the air out of a wire (lands into W_LandAirDeflectGuard) |
| WireFallAttackCombo1 / LandWireFallAttackCombo1 | a050_309000 / 309050 | | the air attack out of a wire and its landing |
| AddActionInputWireShoot | 299026 | 0.4 s | the add-action buffer |

Landing: HKS `BEH_R_LAND_WIRE` (22) / `BEH_R_LAND_WIRE_READY` (24), only valid in the wire-fall
style (table column 8). `LAND_WIRE_READY_ENABLE_FALL_SPEED = -1000`.

Flight physics (what row 1100 means): ChrPhysicsVelocityChangeParam 1100 "wire" = fallType 2
(the exe's Wire fall table, kb/movement.md: h accel -15, gravity -9.8, stick accel 9,
stick max 6), homingId 300000000, homingTargetType 4, no velocity change of its own. Row 1101
"[system] ground wire acceleration" h 30 / v 30 and 1102 "[system] aerial wire deceleration"
0 / 0 are applied by the engine, not the TAE (gap: when). ChrPhysicsHomingParam 300000000
"PC wire default": fallCorrectionGuaranteeArrival 1, fallCorrectionTurn 1,
initialVelocityCorrection 1, no chase, offsets 0 -> the launch velocity is set so the arc under
gravity -9.8 arrives at the point (the same mechanism as the plunge deathblow, player.rs
`PlungeDeathblow` homing on 200000300, which re-aims the horizontal velocity on the time to
the floor). Variants: 300000020 "can't ride default" offset (0, -2.5, +7.5): the arc goes 7.5 m
past and 2.5 m below a no-ride point so Wolf swings through; 300000024 "can't ride, castle
snake valley" (0, 3, 2); 300000040/41 kites (0, -30, 30) / (0, -22, 25); enemies 3000<var>
(see 4). The 903 argument 950 and the 900 argument 50 are not decoded (gap: exe handlers for
TAE 900 / 903; 50 is likely the rope speed, 950 a ms time or an FX).

Camera: CameraParam `wireRotX` / `wireRotY` (CAMERA_PARAM_WIRE_ST) are 0 in every row: the
camera does nothing special; it follows Wolf as in a jump. The rope: weapon model type 3
"Grappling Hook" in the TAE weapon-model enum (the hook arm plays on the prosthetic slot; the
rope FX is an FXR, id not found yet: gap for VXf).

HUD (SB_FE.layout in menu/hi/01_common.sblytbnd): `MENU_Wire_Marker` (the green circle on the
point), `MENU_Wire_Cursor`, `MENU_Wire_Distance`, `MENU_Wire_NG` (grey, out of action range).

## 4. Enemies as grapple targets (the "boss" part)

An enemy is a wire target while it has an SpEffect whose `chrWireVariationNo` is set
(SpEffectParam fields chrWireVariationNo, chrWireTargetDmypolyId, chrWireLandingPointDmypolyId).
The enemy's TAE puts it on with event 66 AddSpEffect_Multiplayer during the move where the
prompt appears. 41 rows; the ones for fighters in Sv1:

| chr | SpEffect | variation | target dmy | land dmy | anims (TAE 66 window) |
|---|---|---|---|---|---|
| any | 500000 / 500001 "enemy general purpose wire possible" | 0 | 650 | 260 | by scripts |
| c1070 Shura (Ashina Elite?) | 3107050/51/52 | 10700-02 | 650/651/652 | 260 | none in its TAE (scripts) |
| c1080 Shichimen | 3108000/01/02 | 10800-02 | 650/651/652 | 260 | |
| c1180 servant | 3118050 | 11800 | 650 | 260 | |
| c1370 fire bull | 3137030/31 | 13700/01 | 31 / 30 | same | "wire point at the left/right e(ar)" |
| c5020 Chained Ogre | 3502020 (grab), 3502021 (tackle), 3502022 (collapse), 3502540, 3502900 (the choker) | 50200-02 | 650 | 260 | a100_003005 0.63-1.07, a100_003007 2.3-3.47, a100_003008 1.27-2.27 (grabs); a100_003013/14 1.43-2.63 and a100_004100/4110 4.5-5.67 (tackles); a000_008300/8301/8310/8650/8651 0-5.0 (collapsed) |
| c5060 Lone Shadow chief | 3506100/01/02 | 50600-02 | 650/651/652 | 260 | none in TAE |
| c5080 Gyoubu | 3508020/21/22 (helmet), 3508060/61 (drifting 3032/3033) | 50800-04 | 650 | 260 | a000_003009 1.77-3.93, 003010 0.9-4.7, 003046 0.4-3.77, 007620, 008300-008330 (var 2); 003032/41 and 003033/42 (drifts) |
| c5100 Guardian Ape | 3510050 "sword can be wired" | 51000 | 650 | 40 | a000_003008 4.57-5.57, 003018 1.23-2.9, 005002/005003 and 405002/405003 0-2.17 |
| c5200 Divine Dragon | 3520050/51/55/56 | 52000/01/05/06 | 650/651 | 600 / 650/651 | (the lightning-tree trees are objects) |
| c5310 tentacles | 3531500 | 53100 | 600 | 600 | |
| c5320 old dragon (large) | 3532010/11/12 | 53200-02 | 650/651/652 | 12 | a000_003003 7.57-9.57 (11+12), a000_003012 2.47-4.47 (10) |
| c7020 Demon of Hatred | 3702030/31 (right shoulder), 3702110 (jump test) | 70200/01, 70210 | 650 / 10 | 650 / 10 | |

Dummies: 650/651/652 sit on the body part to hook (Ogre 650 at (0, 3.6, 0.36) on bone 92;
Ape 650 at (-0.78, 3.6, -1.81) on bone 132 = the sword; Gyoubu 650/651 on bone 229 at
y 4.25/4.13 = the helmet), 260 is the model root (0, 0, 0) on every chr = "land at his feet";
the Ape lands on dummy 40 (bone 133, (0, 2.72, -2.45)), the old dragon on 12. The player's arc
uses ChrPhysicsHomingParam 3000<variation> (300050200 Ogre-grabbing: target offset (0, 1.5,
-2); 300050201 tackle (0, 1, -2); 300050202 collapse (0, 4, 0); 300013700/01 fire bull left /
right (-3, 0, -2.3) / (-1.1, 0, -2.1); the c1070/c1080/c5060 rows all (0, 0, 0)).

The **Grappling Hook Attack** (skill, HKS `ACTION_UNLOCK_TYPE_WIRE_MOVE_ATTACK` 14) is a throw:
ThrowParam 15020170 "Breaking wire 0 PC -> Akaoni" (atk a000_511600 offset 213, def 13600,
throwType 3023, dist 20, y range +-12), 15020171 (511610 / 13610, type 3123), the same pair
for the giant Innsmouth (15021170/71) and Gyoubu (15080170/71, offset 225) plus 15080179
"Breaking wire todome" (511690 / 13690, dist 5). The enemy side: c9997.hks
`SP_DAMAGE_WIRE` (17) when `SP_EFFECT_REF_WIRE_ATTACK` (1000410) and one of
`ENABLE_WIRE_DAMAGE0-4` (1000400-1000404) are up; anims `THROW_DEF_BREAK_WIRE` 13600, `_FIRST`
13610, `DEATH_THROW_BREAK_WIRE` 13611, `_FINISH` 13690. ChrPhysicsVelocityChange 8020 "wire
shinobi: horseman / red demon" = homing 200001600 "Wire 0 default" (200001601 horseman offset
y 1.5, 200001602 Akaoni y 2.0).

## 5. Split (2026-10-11)

- **Map** (this file's owner): export the points (section 1) into `map_<name>.json` as
  `"grapple": [{hook, land, fwd, var, src}]` in the arena frame for every map / boss arena
  (exporter `map.rs`; tools/boss_arenas.py re-export); load them into a `GrapplePoints`
  resource (src/map.rs) with the search (section 2: distance rows, the screen rectangle, the
  variation angles, an obstacle raycast from Wolf's chest to the hook = env 3017
  `WireObstacleJudgment`) as `map::grapple_target(cam, wolf, points) -> Option<GrappleTarget>`;
  sandbox gizmo spheres at the hooks; later the ledge/hug edges.
- **VXf** (player.rs, camera.rs, hud.rs): `Action::Grapple` on `ACTION_ARM_WIRE_SHOOT` (keyboard
  / pad binding in `[input]`), the state table above with the TAE events (921 picks the homing
  row, 905 holds the fall, 900 starts the rope, 920 row 1100 launches with guaranteed arrival
  like the plunge), the landing branch (ride point -> LandWireMove; no-ride / enemy -> keep
  falling with the homing offset), the wire-to-wire chain, the hook arm and rope FX, the HUD
  marker / cursor / distance / NG, no camera change.
- **Bosses** (enemy.rs, status, data export): keep the `chrWire*` SpEffects of an enemy's active
  SpEffects (TAE 66 windows above) and expose `Enemy::grapple_target() -> Option<(hook dummy,
  land dummy, variation)>` so the search sees enemies as points; the Grappling Hook Attack
  throw pair (511600 / 13600) via the existing ThrowParam path; the Ogre and Gyoubu first (both
  in the fighter set and in the clip list).

Verify: sim test "a grapple from 8 m lands on the point in the move-start time", the Ogre grab
window shows the prompt only in 0.63-1.07 s of a100_003005, contact sheets of the real clip
(the Ape sword-wire in the all-bosses clip 0:xx) vs ours.

## 6. Exported counts (2026-10-11, object `.edge` records within the radius + 30 m)

| map / arena | points | variation rows |
|---|---|---|
| m11_01_00_00 gate (General) | 113 | 1 x102, 3 x4, 0 x2, 5, 40 x3, 41 |
| boss_50000000 Monk | 21 | 1 |
| boss_50001000 Fountainhead bridge | 4 | 1 x3, 3 |
| boss_50900000 Butterfly's hall | 0 | (map edges: hang/hug only) |
| boss_51000000 Guardian Ape | 29 | 20 "can't ride" x26, 1 x3 |
| boss_51000100 Headless Ape | 10 | 1 x5, 0 x5 |
| boss_52000000 | 1 | 1 |
| boss_53000000 | 2 | 1 |
| boss_54000000 Isshin's field (shared) | 0 | |

The Ape arena's 26 row-20 points are the swing-through hooks over the pond (homing
300000020: the arc goes 7.5 m past and 2.5 m below the hook); `GrappleTarget.ride` is false
there.

## 6. The player's move, built (VXf, 2026-10-11)

`src/wire.rs` + player.rs `decide` + hud.rs `WireMark`. Key G, pad L2 (`Action::Grapple`).

- **Aim:** `wire::WireAim` each fixed tick = `map::grapple_target` over the map points plus
  `enemy::grapple::EnemyGrapples` (enemies are never ridden; their arc row 3000<variation>).
- **Fire:** HKS 6251 / 6154 order. A press with a target in the action range fires from standby,
  from the jump's window (flags 151 / 119; gap: the wire's own accept bits are not traced), or in
  the air from the falls / WireMove. Without a target the press is dropped (ResetRequest).
  Shoots: GroundWireShoot, AirWireShoot, WireMoveWireShoot, WireMoveWire2Shoot. They turn toward
  the hook (TAE 224: 1080 deg/s for 0.2 s; wire-to-wire 2700 deg/s for 0.1 s), and 905 holds the
  air ones.
- **Move start:** the selectors bound to WireMoveStartIndex list [0] _UpperSlope, [1] _Up,
  [2] _Vertical. Index 1 when the hook is > 50 deg up, 0 when > 15 deg, else 2 (WireDramaticAngle
  taken as the hook's elevation: gap).
- **Exe traced:**
  - TAE 900 = rope tip speed m/s (FUN_140b58290 -> wire +0x604, FUN_140e719b0 moves the tip).
  - 901 keeps the tip on the hook.
  - 904 = the 922 blend with row 1101 (+30 / +30 m/s) and fixed curves Linear 1 / EaseIn 10
    (FUN_140b54c90 -> FUN_140bb1870, .data 0x143b09db0).
- **Launch:** TAE 920 row 1100, homing 300000000. The arc is solved to arrive when the move start's
  camera slot turns 18 -> 19: 0.67 s (202010, air, wire-to-wire) or 0.83 s (202011 / 202012). The
  horizontal velocity is re-aimed each tick (guaranteed arrival); gravity is the Wire fall's
  (-19.6). The skills clip's air grapple (7:53) flies about 0.6 s. gap: the exe's
  initialVelocityCorrection is not traced.
- **Landing:** BEH_R_LAND_WIRE gives LandWireMove, LandWireFallAttackCombo1 (continued) out of
  the wire attack, or LandAirDeflectGuard out of the wire guard. A ridden point is a floor within
  0.6 m of its landing spot (`Player::perch`), because object collision is not in the terrain:
  gap.
- **Swing-through / enemy:** the arc ends at hook + targetOffset (enemy: land dummy +
  targetOffset), in the travel frame (gap), and he falls on in WireMove.
- **In the wire fall:**
  - attack -> WireFallAttackCombo1 (unlock 14);
  - guard -> WireMoveDeflectGuardStart / Loop (held) / End;
  - the press again -> wire-to-wire.
- **Enemy hook:** `wire::hook_enemy` sends `WireHit(enemy)` at the move start (Bosses' DamageWire).
- **HUD:** MENU_Wire_Marker in range, MENU_Wire_NG when only found, at the hook (atlas size scaled
  to the window; the FE movie is not decoded). The export cuts MENU_Wire_* now.
- **Rope:** the wire is a model, not an FXR (no FFX event in the wire anims): EquipParamWeapon
  9500000 "ワイヤー武器" equipModelId 9500 = parts/WP_A_9500 (kunai hook + rope skinned on an
  80-bone chain Born00..79, 4.0 m along -Y in the bind pose; its anibnd has a clip per wire anim:
  202000..202710, a999). Exported by tools/tool_export.py to extracted/tool, shown by
  model/tool.rs `ToolModel::wire` while TAE 715 WeaponModelType 3 "Grappling Hook" places it on
  dummy 144, playing the clip of Wolf's anim. The clips keep the rope at 4.0 m (only its shape
  moves), so it is aimed (-Y) at the tip from `wire::draw_rope` (WireRope) and its chain offsets
  are scaled by distance / 4.0. gap: the exe's own rope placement (TAE 717 / 900 / 901) is not
  traced. The plain line is drawn only when the model is missing.
- **Grappling Hook Attack (built):** attack in the wire fall while hooked on an enemy
  (`player::wire_throw`): its ThrowParam `<chr>0170` (ThrowKind 180000) within Dist / y range and
  the normal-fall-orbit check (as the plunge) -> Wolf "WireThrow" a213_511600, the enemy
  ThrowDef13600. On landing, `<chr>0171` (180100): Wolf "WireThrowLand" 511610 held to the enemy
  (ThrowHold, defSorbDmyId 234), the enemy ThrowDef13610. `actor::in_throw` lists WireThrowLand.
  Test `the_grappling_hook_attack_throws_the_ogre`.
  - The landing hit is a deathblow (Bosses' reading): 511610 TAE 304 judge 660 = AtkParam_Pc
    5000660 "右手刀_忍殺本体_落下" (throwFlag 2, atkPhysCorrection 60000); the Ogre's 13610 ends with
    SpEffect 300610 "HP reset" and 13611 is its death. So `Enemy::deathblow(13610)`: one bar off
    and HP refilled, or the kill on the last bar.
  - Todome (ThrowKind 180900, derived from 180100, attack, defAliveCheck 2; only Gyoubu's
    15080179: a225_511690 / ThrowDef13690, Dist 5): when the landing deathblow took the last bar,
    on the dead defender's flag 69 (13610 frame 55) or its ThrowDefDeath switch -> Wolf
    "WireThrowTodome", `deathblow(13690)`. gap: the input window (511610 has no buffer flag; an
    attack pressed anywhere in the landing counts). Test
    `the_grappling_hook_attack_todome_on_gyoubus_last_bar`.
  - gap: the DiffAng / diffAngMyToDef checks of 0170 are not applied.
- **Open:**
  - the additive attack without unlock 14 (W_AddActionInputAttack: an input-blend layer; the sim
    plays no additive layers anywhere, so the press is dropped). The throw itself needs only
    ThrowKind unlock 5 = ACTION_UNLOCK_TYPE_MAIN_WEAPONE (always), no skill;
  - crouch landings (W_CrouchMoveStartFromFreeFall / W_LandCrouchFreeFall; the moving landing is in:
    stick held -> Locomotion, test `a_wire_landing_with_the_stick_held_runs_on`);
  - AirBreakFall wire: HKS BEH_A_AIR_WIRE_SHOOT (3186) -> W_AirBreakFallWireShoot (a000_202300 ->
    AirWireMoveStartFromBreakFall 202310) only out of HKB ThrowDefLargeBlowStart / FallLoop (Wolf
    flung by an enemy grab). The sim plays an enemy grab as Wolf's "Grabbed" clip (combat.rs
    resolve_throws), without those states, so this waits on them;
  - swim wire (no water in the sim).
- **Test:** `a_grapple_lands_on_the_point_in_the_move_start_time` (0.68 s flight, lands 0.16 m from
  the spot, stays on it).
