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
