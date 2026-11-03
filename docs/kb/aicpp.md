# aicpp: the C++ side of the enemy AI

Dated findings, oldest first. Append new entries at the end. The Lua goal scripts are in
kb/enemy.md and src/ai.rs; this file is what the exe's C++ does around them.

## 2026-11-03 Sight cones and the distance queries checked against the exe

How: `tools/emu-cases/src/aicpp.rs` on the Naomori-arena snapshot (AI live). The enemy's chain
is ChrIns +0x50 -> +0x358 (null for enemies) / +0x120 = ComManipulator (vtable 0x1429d2fb8),
+0x340 = AiThink (`param_1[0x68]` of FUN_1407e67a0; its +0x28 is the NpcThinkParam id), AiThink
+0x7b30 = SprjTargetingSystem (vtable 0x1429635c0, updated from FUN_140608990). The sight context
is built as FUN_1406244f0 builds it (FUN_140625d50 observer side, FUN_1406170f0, FUN_140623960
ray flag, FUN_140bd3e40 target side), then FUN_140617280 (2 = normal / perceive cone, 1 = nose
sphere) and FUN_1406172f0 (around cone) run with the ray off and Wolf's point poked into the
context (+0x90 feet, +0xa0 = feet + 1.5). Six enemies with distinct rows (11500001, 12400000,
10200020, 10100100, 10100304, 74300080), six context variants (plain, perceive flag +0x56, light
index +0x79 = 1 / 2, distance cut +0x64 = 0.8, angle factors +0x68..0x74): 52,554 placements,
7.5 min. Test `sight_cones_match_the_exe`.

The context, as read: +0x20 the observer's sight origin (vfunc 0x20 = vfunc 0x28's x / z at the
root's height), +0x30 vfunc 0x28 = dummy 500 (FUN_14060e480 -> 500; without it the root + facing
x physics radius + up x physics +0xd0), +0x40 the facing (vfunc 0x40; its flat part is the body
yaw, the exe's fwd = (-sin yaw, -cos yaw)), +0x50 vfunc 0xe0 (1.0), +0x56 perceive set,
+0x58/5c/60 the light table 1 / 0.5 / 0.3 (DAT_143afb490..8), +0x64 distance cut, +0x68..0x74
angle factors upper / bottom / left / right, +0x79 Wolf's light index (ChrIns +0x1acd & 0xf),
+0x80 Wolf's physics radius 0.4, +0x90 / +0xa0 / +0xb0 Wolf's feet / raised point / facing,
+0xc4 30 deg. Row +0x53 is disableDark (light factor 1).

Matched as it was: the cone maths of `stealth::in_cone` (begin, back offset and far distance x
cut, half-width tests around the centre angles, left / right on our mirrored X): all 52,554 with
the exe's eye point and facing fed in. Fixed:
- The sight origin is dummy 500 at the root's height, not the feet: 0.15-1.0 m ahead of the root
  on the six enemies (`stealth::eye_point`, enemy.rs reads the dummy marker; fallback root +
  facing x NpcParam hitRadius).
- The light factor (Seen::light, Good 1 here: there is no map lighting) scales the back offset,
  begin and far distances, and the outer gate (radius + dist x light, no cut) as the exe's;
  disableDark rows ignore it (20 of 771 rows).

The `ai:` queries, `aicpp_query`: GetDist (thunk 0x1405e2c10 -> FUN_1405f1540, r8b 1),
GetOriginDist (FUN_1405e3fb0), GetToTargetAngle (FUN_1405e4880), GetDistY (FUN_1405e2e70),
GetDistYSigned (FUN_1405e2f30), IsInsideTarget (FUN_1405e56e0), called with a wrapper whose +8 is
the AiThink, on three enemies with Wolf (TARGET_LOCALPLAYER 21) at 0.3-12 m, 0-180 deg, 3 m below
to 2 m above: 288 placements, 41 s. Test `ai_distance_queries_match_the_exe`. Matched: the angle
(positive to the right), IsInsideTarget's full-width cone (placements exactly on an edge fall either way by rounding and are left out), GetOriginDist (3D,
root to root), GetDistYSigned (target above = positive), GetMapHitRadius = the physics radius.
Fixed:
- GetDist measures to the target's point moved flat toward the AI chr by the target's radius
  (Wolf 0.4), then minus its own radius: Wolf 2 m away level read 1.5 m, the exe 1.1 m (Sv1 had
  centre distance minus own radius). Inside the target's radius it overshoots (0.3 m away reads
  -0.4). `ai::surface_dist`; the native goals use the same (goal_dist).
- GetDistY is the absolute height difference (Wolf 3 m below: 3, signed -3). Sv1 had it signed,
  so 106000's `GetDistY <= -1.8` branch now never fires, as in the game.

Not covered: the sight ray (FUN_14061c8a0, from +0x30 dummy 500 to +0xa0; ours still runs from
capsule centres); the nose sphere (FUN_14061c940: NpcThinkParam nose_dist + Wolf's radius around
the sight origin, result 1, unless Wolf has SpEffect state 332 / 334 when ctx +0xc0 / +0xc1;
62 of 771 rows have a nose_dist, none of the six enemies, and what result 1 leads to is not
traced); ctx +0x50 (vfunc 0xe0) and +0x78; the targeting update's timers and state changes
(FUN_14061ff40), the interrupts, the goal -> pad / action request path (FUN_1407e67a0's move
vector and the manipulator); TARGET_ENE_0 with a live target (the snapshot's enemies had none:
every query read -1).

## 2026-11-03 The targeting update frame by frame, the ray, the nose, TARGET_ENE_0 (aiframe)

How: `tools/emu-cases/src/aiframe.rs` on the same snapshot. Note: the snapshot has DebugFlags
+0x10 (0x143d7a376, disable AI) SET, not off; FUN_1407dd8e0 then skips the AiThink update, so
the manipulator runs clear it in the emulator's copy. `aiframe_targeting` calls
SprjTargetingSystem vfunc 0x10 (= FUN_14061ff40, dt in xmm1) every frame with Wolf's physics
+0x80 placed, for NpcThinkParam 11500001, 12400000, 10100304 and 7-8 scenarios each (seen then
lost until the sight and memory slots run out, around meter fill and drain, flicker, above the
cone, FIND then BATTLE, circling, nose with nose_dist poked to 3): 22 scenarios, 10,134 frames,
about 11 min (TB flush per frame: the host crashed at ~900 frames without it). Per frame: state
+0x158 / prev +0x15c, AI_TARGET_TYPE +0x160 / prev +0x164, flags +0x150 (0x100000 perceive,
0x200000 HUD shown), search timer +0x28, the meter (FUN_140625cd0), the target list (head
+0x18, nodes linked by +0x10, kind = node vfunc 0x10: 1 normal, 6 LastSightPos, 12 memory, 13
indication, 2 allies' sight; forget time +0x2c, +0x40 seen, +0x43 nose), the sight ray to Wolf,
and every 5th frame GetDist / GetOriginDist / GetToTargetAngle on TARGET_ENE_0. Test
`targeting_frames_match_the_exe` (the search forced on the frames the exe searched): 9,414
frames and 1,884 queries equal; the two circling scenarios of the enemies in an enclosed spot
are left out (the exe's ray hit the map inside the cone; no map of this area here).

Found and fixed (`stealth::Targeting::update`):
- The around meter is started by the meter update (FUN_140626430, every frame, before the
  search) from the last search's around sighting and incremented the same frame: a full meter
  turns into an indication target and a new one starts the next frame (exe frame 120 full,
  121 at 0.83; Sv1 waited for the next search, 0.2-0.4 s). `around_seen`.
- Every slot's timer runs every frame (FUN_14062a510): the indication slot no longer stops
  while a normal target is held; memory is refreshed to MemoryTargetForgetTime while the normal
  target is held and decremented the same frame (149.967 at 30 fps), so CAUTION -> NONE comes
  150 s after the normal target goes.
- The nose sphere (FUN_14061c940): Wolf's feet within nose_dist + his radius of the sight
  origin (ctx +0x20, 3D) makes a normal target like sight (FIND, BATTLE inside BattleStartDist
  the same frame) but not "seen" (node +0x40 0, no LastSightPos slot); its forget time is held
  while smelt. Sv1 had no nose (62 of 771 rows have one). `in_nose`, `Normal::held`; 189 direct
  cases (`nose_sphere_matches_the_exe`). Exemptions: SpEffect state 332 / 334 when ctx +0xc0 /
  +0xc1 (0 on the snapshot).
- The sight ray (FUN_14061c8a0) runs from ctx +0x30 = dummy 500 at its own height (0.94-1.8 m
  above the root) to ctx +0xa0 = Wolf's feet + 1.5 (his physics +0xd0) through the map
  (FUN_14094e310 filter 0x32 on [0x143d6d640]+0x98); Sv1 cast between capsule centres.
  `sight_head`, `WOLF_SIGHT_HEIGHT`. On the snapshot it hit the map for most points around
  11500001 and 10100304, none around 12400000.
- TARGET_ENE_0 with nothing held reads GetDist / GetOriginDist / GetDistY(Signed) -1, angle 0,
  IsInsideTarget false (ai.rs `no_target`; Sv1 measured to Wolf). With a target: normal target
  = Wolf's side (radius 0.4) minus the own radius; memory / indication = the spot (radius 0),
  as Sv1 had.

Matched as it was: FIND -> BATTLE checked every frame (BattleStartDist from the root); the
sight forget starts at the first search that misses (forget - dt), the target dropped at <= 0;
the memory slot follows Wolf while the normal target is held; the perceive set once found; the
meter fill 25 x dt and drain 20 x dt against the cone every frame.

ctx +0x50 = 1.0 (observer vfunc 0xe0), +0x57 = 1, +0x78 byte 0, +0x7c 1.5 on all three
enemies; not traced further.

The manipulator (ComManipulator vfunc 0x48 = a jump to FUN_1407e67a0 (M, dt, ChrCtrl)) runs on
the snapshot with the Lua goals (FUN_140608990, ~6,000 stub calls on its first frame), but with
the chr not animating the goals stayed idle (move type 0). The goal -> move step was driven by
poking AiThink +0x73d0 (move point), +0x7410 (move type) and +0x73e0 (pitch, yaw) with the
AiThink update stubbed: M +0x380 = flat unit direction x 0.5 for type 1 (walk), x 1 for type 2
(run), 0 for type 0; in move mode 4 (FUN_1406040b0, by the current goal) the direction is the
path's (FUN_1409eeb40), not the straight line; M +0x20 = target (pitch, yaw) minus the chr's,
wrapped to +-pi; M +0x60 (FUN_140601540, a look point) stayed empty. Sv1 has no stick for
enemies (Command::walk picks the walk / run states), so this is recorded, not compared.

Not covered: the search interval stretch (FUN_140614be0 on DAT_143d547d8; intervals up to
0.62 s seen; Sv1 keeps 0.2-0.4 s); the LastSightPos slot (gone a frame before the normal target,
no effect on the state); the normal node's +0x34 timer (14 s down, purpose unknown); ray hits
against Sv1's map; sound targets; the interrupts the Lua reads; which goal sets which move type
and point (needs an animating chr), M +0x60 action requests.
