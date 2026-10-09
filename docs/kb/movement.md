# movement: Locomotion, turning, quick turns, sprint, step, jump, air control, kick

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: NEXT 6a done: jump physics from data + exe. Jump Start anims fire TAE 920 ChrPhysicsVelocityChange
  (a000_201011 -> row 100: 9.64 m/s up; a000_201010 -> row 101: + 6.4 m/s forward; scales 0 = velocity replaced).
  Fall control is a static exe table (0x143b09ce0, exposed by the physics debug menu), per fallType:
  Normal / Normal_Uncontrollable / Decelerating: horiz accel -0.1, vert -13.72 m/s^2, stick accel 10, stick max 2 m/s;
  Wire: -15, -9.8, 9, 6. Vertical jump apex 3.39 m at 0.70 s (= frame 21, matches SpEffect 100397 frames 18-24), 1.41 s air.
  Ready -> Start -> *GroundJumpFall loop -> GroundJumpLandReady_F. [jump] config gap removed. Sim test added.
  Gap: how stick input combines with the launch velocity (modelled as a separate capped stick velocity).
- 2026-10-06: free turning uses the anim's TAE SetTurnSpeed when present (SprintLoop 360 deg/s, sprint start 180->360,
  guard idle 0); config movement.turn_speed only for walk/run, which carry none.
- 2026-10-06: locomotion: directional walk/run clips (a000_0001/0004 + F/B/L/R) chosen from the move direction vs
  facing (locked-on strafing), and stick release plays StandRunStopF / StandWalkStop_F (their root motion slides).
- 2026-10-07 WOLF/GHIDRA: air control from the exe (FUN_140bac120, fall module; table entries = name ptr + h_accel
  +0x78, gravity +0x7c (+0x1c4 extra), stick accel +0x80, stick max +0x84). One horizontal velocity v: decays along
  its direction by h_accel*dt (skipped once slower than a step); stick a = accel*stick*dt split along dir(v) (or the
  stick when still): the across part always applies, the along part applies in full when braking, and when
  accelerating only up to stickMax - |v|. So a fast launch is steered and braked by the stick but never pushed
  faster; the separate capped stick velocity model is gone (Actor.air_stick removed).
- 2026-10-07 WOLF: turning from the exe. TAE 224 SetTurnSpeed handler FUN_140b574b0 writes the action module +0x308
  (reset to -1 each frame, FUN at 140b2.c:9873); IsLockOnCheck events only apply while locked on (chr+0x1070) - now
  honoured. FUN_1407daa20 picks the turn rate: TAE +0x308 when >= 0, else module(0xb8)+0x16c when >= 0, else the turn
  controller default +0x28 = 720 deg/s (FUN_1407da540). Config turn_speed 720 is therefore exe-confirmed (gap closed).
  Posture after an unused deathblow: not in the collapse TAE (only SpEffects 220420/5359/220500) or HKS; env(1001) /
  env(2010) = posture cur/max at data module +0x148/+0x14c; FUN_140bd62d0 (ratio restore) is only reached from TAE
  event 961, which no exported anim uses. Still 50 % (gap 5/12).
- 2026-10-07 cleanup: config enemy.walk_speed removed (unused; enemy locomotion already plays c1020's Walk*/Run*Battle
  clips with their root motion). [input] comment updated to the accept/execute model.
- 2026-10-07 kick-off-enemy research: HKS [BEH_R_ENEMY_JUMP] = env(2004) and SpEffect ref "TAE enable kick enemy jump"
  (AirKick a000_213100 has SpEffect 100334 "kicking enemy jump transition possible" on frames 9-21; no AttackBehavior).
  env(2004) = bit 4 of +0x58 on the module from FUN_140b360d0 (engine contact flag; setter not found yet). NEXT: gate
  our kick jump to that 9-21 window (data) and find the flag's setter for the reach test.
- 2026-10-07 WOLF: air kick per HKS. A jump press in the air now always plays AirKick (BEH_A_AIR_KICK; blocked by an
  SpEffect with behaviorRefId 108), and the enemy jump happens only while AirKick's SpEffect 100334 (behaviorRefId
  204, frames 9-21) is up and Wolf meets a body (stand-in for env 2004). Was: instant kick-jump on the press.
  player.rs sp_ref_active = HKS env(3036, ref). Test kicks on the descent. 40 pass.
- 2026-10-07 WOLF: the air kick is a real attack. HKS env(2004) = action module +0x58 bit 4, set by the hit processing
  FUN_1409e5fd0 when one of the character's attacks lands. AirKick's TAE 307 PCBehavior (judge 901, GetWeaponData 0,
  frames 9-21) -> BehaviorParam_PC 901 "Kick_in the air" -> AtkParam_Pc 901 (10 phys, 10 posture, push dmgLevel 5,
  capsule dummies 612-613 r 0.4). Exporter resolves 307 judges as attacks "pc<judge>" (also pc210/211/221/222 body
  contact pushes, 0 damage -> ignored). Kick-jump = kick connected (Actor.attack_hit) inside SpEffect 100334's window;
  the KICK_REACH stand-in is gone (gap closed).
- 2026-10-07 quick turns (HKS): sprint quick turn (SprintLoop/SprintStartFromStep_F, SpEffect ref 2 up, not locked,
  stick > SPRINT_BRAKE_ANGLE 135 deg from facing -> SprintQuickTurnReady -> SprintQuickTurn{Right,Left}180, root motion
  turns; then SprintLoop if dodge still held else SprintStopReady). Locked idle: target > 60 deg off -> StandQuickTurn
  {Right,Left}{90,180} (180 past 120 deg).
- 2026-10-07 `sekiro-extract selectors <behavior.hkx>`: every (Custom)ManualSelectorGenerator with its bound variable
  and children by index. "SprintAttack Selector" <- Selector_GroundJumpType: [0] _L a050_302010, [1] _R a050_302011.
  Engine variable table (exe .data 0x143b08e10, {wchar* name, flags}) holds JumpAngle/AttackAngle/SubAttackAngle/KickAngle;
  AttackAngle = signed stick angle from the facing (> 0 right, as _SetStepAngle reads it). Sprint attack now L when
  steering right out of SprintLoop (or left/straight out of SprintStartFromStep_F), else R (was always R).
- 2026-10-08 LIVE (rec_20261008_041541, tools/rec_speeds.py; physics module +0x80 position): run a000_000400 up to
  5.43 m/s (median 5.02, stick not always full) vs our root motion 5.29; SprintStartFromStep_F (a000_001151) keeps
  playing as the sprint for 3.7 s at up to 8.64 m/s (ours 8.70); a000_201200 (not exported, follows a sprint
  before a step, 8.91-8.95 m/s) and a000_000500 (5.26 m/s) unidentified; GroundStep_L 3.61 m/s steady.
  Physics module +0xD0 = 1.5 / 0.4 (capsule height / radius?) = our 0.4 m body radius.
  Identified: a000_201200 = SprintJumpReady (8.9 m/s), a000_000500 = StandRunLoopF (5.26 m/s live, the run loop;
  our run uses a000_000400 at 5.29).

## Jump gravity (live, 2026-10-08)

Five clean vertical jumps across three recordings: launch 9.4-9.8 m/s, rise 1.98 m, 0.82 s in the air, constant fall acceleration 23.52 m/s^2 (quadratic fit per frame). That is Havok world gravity 9.8 plus the fall type's verticalAcceleration 13.72 (exe table 0x143b09ce0). The table value alone (the old model) floated for 1.4 s and peaked at 3.4 m. Per-frame heights match the analytic arc (v dt + g dt^2/2), so the sim uses the exact step. Forward jump (a000_201100): rise 1.26 m, 0.65 s. Air combo mashed: slashes start 8 / 28 / 47 frames after the launch, LandAirComboAttack3 on touchdown.


## Locked-on (positioning) jumps (2026-10-08)

The live forward jumps were LockonForwardGroundJumpReady (a000_201100): locked on, HKS _SetJumpDirection
picks the jump from the stick's angle to the facing (PRM_GROUND_JUMP_*_STICK_RANGE: |a| <= 18.75
lock-on forward, then 45-degree sectors F_R / Rightside / B_R / Backward and mirrored). Ready ->
Start (launch rows ChrPhysicsVelocityChangeParam 111-118: 7.68 m/s at 0 / +-45 / +-90 / +-135 / 180
deg, 7.712 m/s up, fallType 1) -> LandGroundPositioningJump_* (201140-201147), which comes before
moving on (BEH_R_LAND). Side jumps have no graph clip (CMSG anim 0): raw anims a000_201102/03 ->
a000_201112/13. With world gravity: rise 1.26 m, 0.66 s = live 1.26 m, 0.65 s. Free (not locked):
turn to the stick and ForwardGroundJump (row 101). Test locked_on_jumps_go_where_the_stick_points.
- Locked-on step tilt (2026-10-08): HKS _fireGroundStep -> _set4DirStepDir (45-degree quadrants,
  matches ours) + _set4DirStepTilt: act(3025, angle - quadrant centre), none within +-18.75 of
  forward, so the dodge follows the stick. Implemented as the step's root-motion yaw offset.
  Test locked_on_step_tilts_toward_the_stick.

## Locomotion clips (2026-10-09)
- c0000.hkx CMSGs: StandWalk/RunStart = a000_0001xx / 0004xx (played once), StandWalk/RunLoop = 0002xx / 0005xx
  (+0 F, +1 B, +2 L, +3 R; MoveSpeedIndex from the stick, MoveDirection by _MoveDirectionUpdate 55 / 125 deg).
  We looped the *start* clips (last frame 50-105 deg off the first: a pop every 1.7-2.7 s, worst strafing).
  Now: start clip, then the loop (Actor::move_clip; the start's last frame is within a frame of loop frame 0).
- Loops repeat frame 0 as the last frame: a cycle is frames - 1 intervals (Clip::wrap; the idle wrapped a frame late).
- Speeds from the clip on screen: walk loop 1.61, run start 5.29, run loop 5.55 m/s (live run loop 5.26).
- Stops by direction: StandRunStop{F,B,L,R} / StandWalkStop_{F,B,L,R}.
- Sheathed (weapon style None) the forward clips come from a010 (c0000_a00x.anibnd: 000100/200/400/500).
- tools: `SHINOBI_TRACE=<csv>` runs a scripted input pass and logs Wolf's bones per rendered frame (src/trace.rs);
  `python tools/trace_pops.py <csv>` lists pops. 2026-10-09: 17 pops before, none after (an attack lunge's fast pelvis
  drop is real motion).

## Diagonal moves / foot sliding (2026-10-09)
- Locked-on diagonals played the F/B clip while moving at 45 deg: the planted foot slid 1.0-3.4 m/s.
- Sekiro: exe WalkTwist (WalkTwist.cpp, FUN_1407f6ff0) sets TwistMasterAngle = move angle - the class's clip
  angle (0 / pi / -+pi/2); SpinJointBehavior "Master" converges at 0.1 rad per 1/30 s (constant-rate mode).
  c0000.hkx hkbTwistModifier TwistMaster turns the Master bone by it, TwistRootRotYCancel turns Spine..Spine2
  (42-44) back (chest keeps facing the target). Classifier FUN_1407f7550: 60 deg cones, -+5 deg hysteresis.
  Ours: Actor::twist / twist_target (player.rs move_velocity, guard walk), anim.rs apply_walk_twist.
- Speed = the shown clip's root velocity at its current time (CharData::root_velocity_at), as Havok's motion layer.
- tools/trace_slide.py: planted-foot speed per trace step. Now 0.01-0.08 m/s for every walk, locked run, guard walk.
  Gap: forward run loops (a000_000500 / a010_000500) still slip ~0.9 m/s: the clip's planted foot moves back
  4.35 m/s against 5.55 root; Sekiro has foot IK with foot locking (hkbFootIkControlsModifier) - not done.
