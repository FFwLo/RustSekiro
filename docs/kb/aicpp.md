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

## 2026-11-03 Goals into moves, turns and actions; the search scheduler (aimove)

How: `tools/emu-cases/src/aimove.rs`. `aimove_f_<name>` runs whole world frames as acttrace's
duel (pre-update tasks, the General's chr frame FUN_140a4b570, Wolf's, post-update tasks) at
1/30 s with the AI on (b143d7a376 = 0) and the General updated every frame (chr +0x110 = 0),
about 2.5 s per frame (scenarios run as separate processes). Wolf's snapshot spot is a ledge
10 m above the arena: the General put 6 m in front of him fell off it and walked home
(BackToHome 4 -> MoveToSomewhere 2019, never found Wolf), so Wolf is put 3 / 4 / 6 / 10 / 16 m
in front of the General instead (General facing him, 90 deg or 150 deg off; Wolf walking away),
240 / 180 frames each; plus the forced attack 3000 (AI +0x7430 at FUN_1407e6570's entry) from
1.6 / 2.6 m. Per frame: targeting state, AI +0x73d0 move point, +0x7410 move type, +0x73e0
angles, the request at the copy, M +0x380, M +0x20, anim, position, yaw, physics +0xb10 and the
proxy integrate calls; every AddSubGoal (hook at FUN_1405ce510: goal id rdx, life r8, params r9
and the stack). JSON: extracted/emu/aimove_frames_<name>.json, aimove_sched.json.

What the goals did (think 10200020, logic / battle 102000): at 3-10 m he was in BATTLE at once,
played the battle start 1040, then 102000_battle's acts: ApproachTarget 2015 -> MoveToSomewhere
2019 (+ engine 2025) with walk 1 -> move type 1, M +0x380 = flat unit direction x 0.5, the walk
405000; walk 0 -> type 2, x 1.0, the run 405010 (~6 m/s); arrival then SpinStep 5200 / attack
3004 (CommonAttack 2200 under AttackTunableSpin 2220 / 2260). SidewayMove 2017 (target, 0 = left,
45 / 38 deg, true, walk true, -1) -> type 1, M +0x380 at 89 deg to his facing (left for 0), the
side walks 405002 / 405003, facing Wolf. The AI angle +0x73e0 is the direction to the move point
(MoveToSomewhere) or to the target (sideways, attack); M +0x20 = that minus his yaw. Move mode
FUN_1406040b0 stayed 1 (2 in SidewayMove); mode 4 (path FUN_1409eeb40) never came up. Sv1's
Native::Approach / Sideway map these the same (walk param 4 -> walk / run, 'L' for 0, face the
target). With Wolf walking off 16 m, the run's move point left the straight line by up to 125 deg
(a route round something; not modelled, Sv1 has no navmesh). When the target was behind him the
walk turned in the walk / back walk clips (405001 at 120 deg/s) or a 180 turn clip (405403).

Turn rate (fixed): his yaw goes to the AI angle at his clip's TAE 224 SetTurnSpeed, every frame
the turn is not done: run 240, the walks 405000-405003 120, battle start 1040 135 / 0 by window,
attack 3004 208 / 135 / 0 / 225, 3000 450 / 300 / 0 / 225, step 5200 90 / 0. Sv1 turned every walk
and run at NpcParam turnVellocity (135); now `enemy::ai_turn_rate` (TAE SetTurnSpeed, else
turnVellocity) for walks and runs. An idle turn keeps turnVellocity: the exe's idle turn is not
traced, and with the idle clip's rate c1010 never closed a turn clip's leftover
(`an_enemy_turns_in_place_with_its_turn_clip`). Test `general_turn_rates_match_the_exe`: 350 turn-limited frames (rates 0 / 120 /
135 / 225 / 240 / 300 / 450) to 0.002 rad.

The search scheduler (fixed, ported): FUN_140614be0 is a 100-bucket frame ring
(DAT_143d547d8 +0x28; FUN_1406142b0 sets cap +0x68 = 3, no-limit +0x69 = 0, +0x20 = 5.0): a new
interval t goes in bucket (cursor + (int)(t x 30)) % 100; a full one walks k buckets on and returns
t + k / 30. The world task FUN_140a74560 -> FUN_140614dd0 clears the cursor's bucket and moves it on
once a frame. The snapshot's ring was empty (cursor 82). The 0.62 s aiframe saw was the emulator
never running that task, so one enemy's searches piled up in a few buckets; in play a few enemies
never fill a bucket. `stealth::SearchRing` (in enemy.rs `Brains`, advanced once per think frame,
`Targeting::searched` places the interval); tests `search_ring_matches_the_exe` (400 rings:
stretched interval and the bucket bumped) and `search_ring_stretches_only_crowded_frames`.

Not covered: sound targets and the interrupt flags the Lua reads (no sound or interrupt was
raised in these runs); M +0x60 stayed empty; the run's route point off the straight line; which
act 102000_battle picks (its GetRandam draws are the exe's); a clip with no SetTurnSpeed
(turnVellocity kept as the fallback, untraced); SidewayMove with walk false (no script passes it).

## 2026-11-04 Interrupts and sound targets (aiint, row E11)

How: `tools/emu-cases/src/aiint.rs`, whole world frames as aimove (AI on, the General updated
every frame) with the goal tree's interrupt dispatch hooked (FUN_1405ada60: AI +0x40, rdx = the
mask AI +0x68; bit n = INTERUPT n of common_ai_define.lua). The mask is gathered each think by
FUN_14060c460: the targeting flags (FUN_14061f0a0: ts +0x150 bits -> 1, 6-12, 13 / 14, 20, 24,
30, 34, 47, 52; the target list's changed flags -> 46 sound, 53 indication, 55 corpse; a state
change that is not a gain -> 18), the AI +0x7460 block (FUN_1405b48e0: 21, 22, 27-29, 45, 48,
49, 54 and 31), observe areas (FUN_14059e760: 16 / 40), the SpEffect watch list (FUN_1405ace40:
43 / 44), the event request (FUN_1405b49e0: 39), 15, 25, 26, 33, 38, 57 and the rest. AI sounds raised as the TAE's CreateAISound does
(FUN_1407577b0: source {1, Wolf's handle, -1}, id, FUN_14064a5f0(manager [0x143d55070], pos,
source, extra, id)); the records live in each map area's list (area +0xb8, 0x70 a record:
+0x10 spot, +0x2c radius, +0x30 life, +0x3c rank, +0x40 id, +0x48 serial u16; one copy per
area, each with its own serial); they age in the area's AI update (FUN_140652020 ->
FUN_140611a00), which the harness runs after each frame. Scenarios: `snd` (Wolf unseen on his
ledge; 2140 rank 0 at 2 m, raised again in its life and after it, rank 1 2100 / 2110, rank 0
under rank 1, 1000 out of reach), `snd_two` (two rank 0 sounds in one frame, refreshes), `lose`
(BATTLE, Wolf moved behind the General and back). 370 frames, about 6 min.

The listener (FUN_140624b70, every frame while no character target is held): a fresh slot is
offered every record in reach (FUN_140610470 -> FUN_140610b30: team flags, reach), rank 0
records only while the held sound's rank < 1, rank 1 while < 2 (FUN_14064c540 channel = rank);
the slot keeps the higher rank, of one rank the higher serial (FUN_1406101a0: new - held > 0).
The pick goes into the list's kind 3 node (FUN_14062a2a0: slot at node +0x70: serial +0x78, spot
+0x80, id +0x94, rank +0x98; forget +0x2c refreshed each frame it is heard, 149.967 after the
frame's countdown); a serial other than the node's sets the changed flag -> 46 once. Records
(FUN_1406113b0): a sound raised again merges into a live record of the same source and rank whose
spot is within the mean of the two radii and whose radius is within 1 m (FUN_14060fa90: spot,
radius and life renewed, serial and first id kept); else a new record (FUN_14060f840, global
serial DAT_143d547b0). Seen: both sounds of one frame -> the newer (46 once); a refresh in its
life -> no 46; the same id after its life ran out -> a new record and 46 (Sv1 had none: it
compared ids); a newer record of one rank wins over the held one still heard; rank 0 under a held
rank 1 -> ignored; the General (logic 102000, chuboss reaction) answers 46 with
InterruptTableLogic_Common's ListenSound (Wait, EndureAttack 700 / 710 for rank 1), then
Replanning while timer 11 / 12 runs.

LoseSightTarget (31): FUN_1405b48e0 keeps, per think, whether the current target is visible
(FUN_140623a20; for the normal target its seen flag, node +0x40, set by the sight search) and
raises 31 when it was and is not now. In `lose` it came once per seen -> unseen edge (f71, f118:
the searches that missed), nothing when Wolf was seen again. Sv1 never raised 31; the General's
Goal.Interrupt uses it to flip SidewayMove (102000_battle.lua). Other bits seen in these runs and
not read by the General's scripts: 47 (ts +0x150 bit 17) and 15.

Fixed:
- `stealth::Targeting::listen`: one pick per frame from every record heard (rank gate, then
  rank, then serial), 46 on a serial change (was: each sound offered in turn, the last one kept,
  46 on an id change).
- `enemy::AiSounds::emit`: the exe's merge (owner, rank, mean radius, 1 m) instead of "same owner
  and id"; Wolf's TAE 237 sounds are records too (`AiSounds::wolf_tae`, refreshed while the
  event is on: a walk keeps one serial; stopping past the life and walking again is a new one),
  heard through `wolf_sounds`' reach rule.
- `ai.rs`: INTERUPT_LoseSightTarget (31) to the goals on the normal target's seen -> unseen edge
  (`ai::lose_sight`, AiState `was_visible`).
Tests `sound_targets_match_the_exe` (held id / rank, the 46 frames, the live records and the
serial identity on 210 frames) and `lose_sight_interrupt_matches_the_exe`.

Not covered: ranks >= 2 (FUN_140628230 / FUN_140629f10 channels; 7400 is rank 3), the bCallFriend
/ team branches of FUN_140610470 with an NPC source, the record copies per map area (Sv1 has
one); when in the frame the records age (the area update was not placed in the frame order); a
sound / memory / indication target's "visible" for 31 and 31 when a target ends (only the normal
target's edge is raised); the other mask bits (1, 6-10, 13 / 14, 18, 20, 30, 34, 47, 52, 53, 55,
15, 21 / 22, 27-29, 45, 54) are listed, not raised by Sv1 or compared; IsInterupt's binding read
as a mask test, not hooked; whether the TAE emits every frame of an event or once (the
handlers FUN_140b4fe00 / FUN_140b50930 were not run); the General's goals after 46 compared by
eye only (anim 700 / 710), not frame by frame against our Lua.

## 2026-11-04 Light, geometry bits and the nose marks (aistealth, row E33)

How: `tools/emu-cases/src/aistealth.rs` on the Naomori-arena snapshot (`aistealth_probe`,
`_bits`, `_factors`, `_light`, `_nose`; about 45 s). SpEffects are put on a chr as nodes at the
head of its list ([chr+0x11d0]+8) with the holder's stateInfo filter (+0x20) opened, from the
real rows or copies with sightCutLimitType (+0x107) or stateInfo (+0x15e) changed. JSON:
extracted/emu/aistealth.json. Tests in `src/sim_tests/emu_aistealth.rs`.

The geometry bits (FUN_140616f50, from FUN_140bd3e40 through FUN_140616ef0 with ctx +0x20 the
sight origin, +0x90 Wolf's feet, +0xb0 his facing, +0x56 the perceive set): the flat unit vector
from Wolf to the observer dotted with his facing (3D, so a tilted facing counts its flat part
only). Bit 1 when the dot >= cos(front / 2) and the perceive set is off; bit 2 when the dot <
cos(180 - back / 2). front / back are DAT_143afbb04 / 08 (getters FUN_14064c4a0 / 4b0 on the AI
manager [0x143d55070]): 180 and 220 on the snapshot, so bit 1 = the observer in Wolf's front
half, bit 2 = more than 70 deg off his facing. When Wolf wears no effect with a non-zero
sightCutLimitType the bits are 3 (FUN_140bfedb0). 7,776 direct cases equal
(`geometry_bits_match_the_exe`).

The factors (FUN_140bfe8a0 distance cut, FUN_140bfea80 / bfe6e0 / bfe7c0 / bfe9a0 angle cuts,
FUN_140bfc050 meter rate): each walks Wolf's active effects; type 0 counts always, type 1 only
with bit 1, type 2 only with bit 2. The distance cut also skips stateInfo 7 / 53 effects unless
ctx +0x10 (observer vfunc 0x68, 0 on all six enemies) and multiplies by the holder's +0x14 (1.0).
Clamps: cut and angles 0..1, rate 0..999.9. The meter fill (FUN_140612400) computes its own bits
from the observer's feet (FUN_1409f0870), not the sight origin. Data: 109210 wall hug is type 2
(left / right angle cut 90, rate 0), 109220 hang is type 1 (distance cut 100, rate 0); no other
row has a type. Seven effect sets x 4 bit pairs, and the whole target side with the observer
(11500001) every 5 deg round Wolf at 6 m facing him, perceive off / on: 1,064 cases equal, cone
results included (`sight_factors_match_the_exe`). Sv1 had left both effects out entirely;
now `Seen::limited` + `from_eye`.

The light (FUN_140617150): ctx +0x79 = Wolf's ChrIns +0x1acd & 0xf, then, if the observer (ctx
+8 = the observer's SpEffect holder, observer vfunc 0xf0; equal to [chr+0x11d0] on all six) wears
a stateInfo 340 effect, FUN_140616e90 maps index 0 / 1 / 2 to its mapVisibilityOverrideGood /
Dark / PitchDark (+0x104..0x106). The same effect on Wolf does nothing. 18 cases
(`light_remap_matches_the_exe`). 11500001 already wears one (Good / Dark / PitchDark -> 0 / 0 / 1,
200511's). NpcParam: 200500 (all Good) on 63 rows, 200511 on 59, 200530 (Good -> PitchDark, the
cones x 0.3 in daylight) on 12, 200520 on 7. Sv1 now reads the observer's resident / clash /
chained SpEffects (`stealth::Observer`, enemy.rs).

Wolf's index itself: FUN_1409f5820 writes +0x1acd = the MapVisibility of the MSB Collision part
of his floor hit (FUN_1409b5c80: part type 5 by the hit record's index, type data +0x27; low and
high nibble), when the floor part changes; then each call, with a part value, low = high unless
the chr is inside a CSDarkSight region ([0x143d87fd0], FUN_140c0c170: MSB spheres / cylinders /
boxes and SpEffect spheres, any containing the point), which makes it 0 (Good). On the snapshot
Wolf and all six enemies read 0 and Wolf is in no region. Not ported (data missing): Sv1's hit
collision has no part per triangle and no DarkSight region is exported. It needs (1) per hit
triangle (or per MSB Collision part) the part's MapVisibility (crates/formats msb.rs already
reads `map_visibility`; the .hit export would carry it next to the material) and (2) the
DarkSight regions with their shapes. Only m11_00 (2 Dark + 4 PitchDark parts), m17 (19
PitchDark) and m20 (7 Dark) have non-Good parts (kb/enemy.md 2026-10-12; the m11_01 gate's 221
are all Good).

The nose marks (FUN_14061c940): ctx +0xc0 = Wolf wears stateInfo 333, +0xc1 = 335
(FUN_140bd3e40); the nose misses when +0xc0 and the observer wears 332, or +0xc1 and 334. 25
pairs (`nose_marks_match_the_exe`). No SpEffectParam row has stateInfo 332-335, so with the
game's data nothing hides from the nose; ported (`Seen::nose_marks` / `nose_hidden`) for
completeness. (The "332 / 334" of the old gap note are these stateInfo values, not effect ids.)

Not covered: Wolf's light index from the map (above); ctx +0x10 (the 7 / 53 rule taken as off);
the meter's fifth argument to FUN_140612400 (taken as the perceive set); the observer's TAE
SpEffects for the 340 remap (Sv1 reads resident, clash and chained ones); index values above 2
(MapVisibility has three); ctx +0xc3 (FUN_1409f3430 player branch, DAT_143d7a36f).
