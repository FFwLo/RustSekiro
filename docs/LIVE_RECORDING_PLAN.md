# Live recording plan (one session, log everything)

Goal: one ~5 minute play session in the real game, recorded by a single memscope script, so every
open question is answered from the log without more game runs. Static data stays the source of
truth; the log confirms it, settles the "gap:" items, and gives ground truth to compare Shinobi
against.

Setup: Claude started as administrator, `tools/live_check.ps1`, attach `sekiro.exe` (base
0x140000000 = the static decompile's base, so decompile addresses apply directly).
Output: `memscope-data/logs/rec_<date>/frames.csv` (per-frame stream) + `events.jsonl` (hook events).
Every row carries the game's frame counter and time, so streams line up.

Status of each item: **known** = address / offset already in the decompile notes; **find** = locate
statically before the session (`tools/fn.sh`, extracted/decomp).

## Status (2026-10-08): recorder ready

`tools/memscope/recorder.lua` (installed in memscope-data/scripts/sekiro.exe/) records raw module blocks of
Wolf + the 2 nearest enemies at 60 Hz (dry run: 179 ticks / 3 s, no slow ticks, ~0.6 MB/s per 30 Hz).
`python tools/rec_read.py <rec_dir>` summarises; `--field slot:block:off:type` follows a value;
`--changes slot:block` lists the words that change (to decode offsets after the session).

Layout found live (static decompile confirms the classes via RTTI):
- WorldChrMan = [0x143d7a1e0]; +0x88 = PlayerIns (vt 0x142a2b338); +0x998 = array of EnemyIns (vt 0x142a27f28).
- ChrIns +0x1FF8 = module bag: +0x00 ActionFlag, +0x08 BehaviorScript, +0x10 TimeAct, +0x18 Data, +0x20 Resist,
  +0x28 Behavior, +0x30 BehaviorSync, +0x38 Ai, +0x40 SuperArmor, +0x48 Toughness, +0x68 Physics, +0x70 Fall,
  +0x80 ActionRequest, +0x88 Throw, +0x90 HitStop, +0x98 Damage, +0xA8 KnockBack, +0xB8 BehaviorData,
  +0xF8 SwordArts, +0x108 AutoHoming, +0x120 HitWall, +0x130 WireAction, +0x140 Sound.
- ChrIns +0x48 ChrModel, +0x50 PlayerCtrl, +0x58 PadManipulator, +0x60 SprjChrTaeAnimEvent, +0x11D0
  SpecialEffect, +0x2000 PlayerGameData (+0x1C/+0x20 max HP, +0x38/+0x3C max posture).
- Data module +0x130 HP, +0x134 max HP; posture words +0x148/+0x14C/+0x150 (current vs max: decode from a hit).
- Physics module +0x80 position (vec4), +0x90 previous position, +0xD0 1.5 / 0.4 (capsule height / radius?).
- TimeAct module: ring of recent anims, 0x14 bytes each {anim id, t0, t1, length, ?} from +0x20; +0xE8.. indices.
- DAT_143d5c0d0 is GameMan (not the camera); camera object not located yet (ChrExFollowCam vt 0x1429a7d88).

Not in v1: camera, cloth internals, event hooks (damage / FireEvent). Per-frame data at 60 Hz covers
transitions, timing, movement, HP / posture; hooks are added only where the log is ambiguous.

## 1. Per-frame stream (30 Hz reads, player + nearest enemy + camera)

| Value | Why | Status |
|---|---|---|
| Position, yaw, velocity | movement speeds, turn rates, root motion vs TAE 760 boost | find (ChrIns +pos) |
| Current anim id + TAE time | ties every other value to the anim and frame | find (TimeAct module) |
| HKS behaviour state | transition timing, buffer windows | find |
| HP, posture, posture max | damage / posture / regen curves | known (PlayerGameData +0x20 / +0x3c) |
| Active SpEffect ids | refs (203 deflect, 208 release, 223 art combo, 1 sprint ...) as the game sees them | find (SpEffect list) |
| ChrActionFlags / cancel bits | input windows (115 attack, 117 guard, 26/25 step, 119 jump, 11 move) | known (ActionRequest +0xd0 accept, +0x188 clear) |
| Lock-on target | twist / camera target switches | find |
| Camera pos, rot, FOV, distance, CameraSetParam slot | camera blends (TAE 151/153/155), look limits | known (DAT_143d5c0d0) |
| Twist state per chain (yaw / pitch) | TAE 700 newTargetGain vs onGain | find (CustomLookAtTwistModifier instance) |
| Root bone + Spine2 / Head world rotation | animation blend and twist check | find (hkbCharacter pose) |

## 2. Event hooks (one log line per call)

| Hook | Logged | Answers | Status |
|---|---|---|---|
| Damage apply | attacker, AtkParam id, damage, posture damage, guard result (hit / guard / deflect / just deflect), time since guard pressed | deflect window, guard levels, posture numbers | find |
| Deflect / guard resolution | window frame, guard level, chain index (StandToDeflectGuard 1-4) | spam-deflect penalty | find |
| Input buffer write / clear | action, frame, accept mask | buffer timing | known (+0xd0 / +0x188 writers) |
| HKS FireEvent / FireEventNoReset | event name, state before / after | every transition incl. BEH_A_GROUND_SP_ATTACK_RELEASE (art early release) | find |
| Throw / deathblow start | ThrowParam row, distance, angle | deathblow ranges (front / back / plunge) | find |
| Knockback apply | KnockBackParam row, velocity per frame | knockback curves | find |
| TAE 760 boost | root-motion scale chr+0x2fc | lunge reach | known (FUN_140842980) |
| Posture regen tick | regen ratio, graph 504 value | regen in each state | known (FUN_140850b10 caller) |

## 3. Animation

- Crossfade duration and weight curve at each clip change (compare with TAE Blend events).
- Additive layer weights (AddDeflectGuardBlend, AddDamageBlend) over time.
- Twist chain angles while locked on, and when switching targets (newTargetGain).
- Foot IK on slopes (SkeletonParam footPlacement): flat ground first; slopes only if a map has them.
- Locomotion blend: walk / run selection by stick level (speed thresholds).

## 4. Movement

- Walk / run / sprint speeds, accelerations and stops (compare: 1.62 / 5.29 / 8.77 m/s from root motion).
- Turn rate per state (idle, run, sprint, guard walk, attacks with Disable Turning).
- Step / dodge distance and direction choice; jump launch velocity, gravity, air control; land frame.
- Homing / auto-aim during attacks (TAE 920 ChrPhysicsVelocityChange, ChrPhysicsHomingParam).
- Character push / separation radius against the enemy (player proxy radius gap).

## 5. Physics

- Cloth: per-step particle velocity before and after damping (globalDampingPerSecond meaning),
  bend-stiffness constraint inputs and outputs, collision push-outs. Hook hclSimulateOperator's
  integrate and constraint loops (find).
- Character controller: capsule radius / height, step height, slope limit.
- Ragdoll on death: which bones go limp, blend-in time (only if it is cheap to hook).

## 6. Play checklist (one take, ~5 minutes, save near an Ashina Samurai General)

1. Stand still 5 s (idle, cloth at rest). Walk, run, sprint in a straight line, stop each.
2. Turn on the spot and while running; quick-turn out of a sprint.
3. Step / dodge in 4 directions; jump in place and forward.
4. Lock on; walk around the enemy; switch target if a second enemy is near; lock off.
5. Light-attack combo (5 hits), hold a charged thrust, sprint attack, jump attack.
6. Combat art: once full, once letting go of attack early; hold the guard + art for a hold art.
7. Let the enemy attack: deflect 5 hits on time, guard 3, take 2 hits, spam-deflect a combo.
8. Break posture and deathblow; one deathblow from behind if possible.
9. Run around for 10 s so the scarf and the enemy's robes swing.

After the session: Claude reads the log, settles each item, updates code + docs/kb, commits.
Missing data = add one hook and repeat only that checklist step.

## Before the session (Claude, static)

Locate every **find** item in extracted/decomp and write the recorder (`memscope-data/scripts/
sekiro.exe/recorder.lua`) with a dry run against the main menu to check that the reads are valid.
