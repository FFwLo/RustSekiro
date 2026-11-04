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
  (reset to -1 each frame, FUN); IsLockOnCheck events only apply while locked on (chr+0x1070) - now
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
- 2026-10-10 slide: crouch while ref 1 (sprint) is up -> W_SprintToCrouchReady (a000_216010, its root motion
  slides), at its end TurnAngle > 0 -> SprintToCrouchLeft (216020) else Right (216021), crouched (HKS
  c0000_transition.lua 4498 / 405). gap: ACTION_UNLOCK_TYPE_SPRINT_TO_CROUCH (26) taken as unlocked.
- 2026-10-10 sprint deflect: SprintToDeflectGuard (a050_203001) slides 3.04 m on root motion and turns only
  by TAE SetTurnSpeed (180 f3-6, 360 f6-9); Sv1 snapped Wolf to the target first, so the slide carried him
  into the enemy. HKS BEH_A_DEFLECT_GUARD_START sets no facing; now no snap.

## Blend-in root motion (2026-10-14, exe)
Havok's blending transition mixes the world-from-model: from x (1 - w) + to x w over the to clip's
TAE Blend, w = smoothstep (FUN_141408650 curve 0; FUN_14140cfc0 -> FUN_1413bf6c0). The from
generator keeps playing meanwhile, so an attack out of a run keeps the run's speed through its
first ~0.1 s (live rec_* medians: live / unweighted clip 0.7-1.0 over 6 ticks). Effects with
Havok flag 1 (c0000.hkx TaeBlend_NoSrcMotion*) take the to root at once: extracted/blend_src.json
per target state (sekiro-extract sms), data::blend_keeps_src. actor.rs Actor.blend_* / advance.
A clip restarted on its own key is the generator looping, not a transition (no blend), and the
guard walk's four directions are layers of one state (DeflectGuardMove, hkbLayerGenerator): a
direction change clears the blend. A looping from clip (hkbClipGenerator mode 1, 247 of Wolf's
2566: extracted/loop_clips_c0000.json from `sekiro-extract clips`; data::loops_c0000) wraps its
clock through the blend. Enemies' graphs carry no loop flag (gap: their from clips stop at the
end). See HANDOFF 4.63 item 8.

## Lock-on turning, quick-turn loop, sprint deflect, collision (2026-10-15)
- Quick-turn loop: StandQuickTurn{Left,Right}{90,180}
  (a000_000010-13, crouch / guard kin) have no root motion (no hkaDefaultAnimatedReferenceFrame;
  the Master bone ends where it starts) and no TAE 224, so Wolf never turned and the locked idle's
  >60 deg check replayed the clip every 0.67 s. Now the turn controller's default rate (720 deg/s,
  FUN_1407daa20) turns him to the target during the clip (180 deg in 0.25 s). gap: the exe's turn
  request for these states. (TAE 607 on 2287 of Wolf's anims is FaceAnimation, not motion.)
- Lock-on facing no longer snaps: walking / running locked on turns at turn_rate(locked) (TAE 224
  with IsLockOnCheck, else 720 deg/s); the guard idle the same (its TAE 0 -> 720). Locked steps
  and positioning jumps no longer snap to the target first: HKS reads KickAngle / MoveAngle
  against the facing as it is (_fireGroundStep 5280-5300).
- Guard start: HKS BEH_A_DEFLECT_GUARD_START sets no facing; act 156 (FUN_140b2e090) only
  stores a homing point and turns nothing; SprintToDeflectGuard's TAE turns at most ~54 deg
  (Disable Turning f0-3, 180 deg/s f3-6, 360 f6-9; root 3.04 m straight ahead). Tried keeping
  the sprint heading: user, from the game, "deflect sideways then see the enemy" -> the snap to
  the target is back. gap: where the game's turn to the enemy comes from (the exe turn
  controller only clamps a yaw delta from the behaviour graph, FUN_1407dab40 / FUN_1407da8f0).
- Collision (map.rs): Wolf's map capsule 1.5 m / r 0.4 (FUN_140bc3140, hard-coded; NPCs read
  NpcParam hitHeight / hitRadius in FUN_140bc2e00), hknpCharacterProxy maxSlope 60 deg
  (FUN_14104d3d0, 0x3f860a92). Were 0.35 / 50 deg from sekiro-rs. gap: step-up height (0.5 kept)
  and the ledge / support logic are not traced; the top wall sphere (1.55 + 0.4) sits above the
  1.5 m capsule. (2026-11-03: checked in the emulator, "Character proxy against the exe" below.)
- Open: jumping feel (details pending); foot IK on the run
  loop; _set4DirJumpTilt; wall jump (HKS BEH_A_WALL_JUMP, not in Sv1); ledge hang / climb (edges
  loaded, unused); swim.
- Sprint stop fixed (actor.rs advance): a transition that drops the source motion (blend_src
  false, e.g. SprintLoop) weighted the to clip's root by the blend too, so SprintStartFromStep_F
  -> SprintLoop went 8.6 -> 0 -> 9 m/s over 0.3 s. Now full
  weight from frame 0. Test sprint_keeps_its_speed_into_the_loop. Still from the data (not bugs):
  the run start a000_000400 dips 5.6 -> 4.9 m/s at 0.9-1.0 s, the step -> sprint start blend dips
  to ~3 m/s for 0.1 s (ref 15 opens at 0.4 s while the step brakes; blend 0.2 s keeps the source).
- Air kick: HKS 2814-2823, under ref 431 (SpEffect 100247, AirKick 0.4-0.7 s) a jump press only
  fires the additive W_AddActionInputJump, no new kick; now honoured. Test
  mashed_air_kicks_kick_once_per_jump (a plain jump allowed only one kick already: jump start's
  119 opens at 0.1 s, the jump lands before AirKick's 0.7 s). gap: the additive clip; where the
  reported repeated kicks came from (off an enemy's head?) not reproduced.
- 2026-10-15 LIVE auto-test (Sekiro via me3 a local me3 profile, keys sent by
  scratchpad sk_input.py (SendInput scan codes reach DirectInput), memscope script move_log.lua: TimeAct
  +0x100 anim, physics +0x80 pos / +0x74 yaw). A pre hook on FUN_1407dab40 ran 335 calls, no crash.
  - Run a000_000400: 0 -> 6.4 m/s in 0.35 s, then 4.65-6.4 (the dips are real, as in Sv1).
  - Step 213301 -> SprintStartFromStep 001151 stays 6.7-8.8 m/s. Sv1 dipped to 3: blend_keeps_src
    looked up "SprintStartFromStep_F" / "GroundStep_F", the table has the graph names -> now strips
    suffixes for Wolf (data.rs). Test sprint_keeps_its_speed_into_the_loop checks both.
  - Jump mashed in open air (jump_mash_20261015_131809.csv): NO AirKick (213100) at all, only jump
    201000/201001 -> 299020 -> next jump. Sv1 kicks on any air press: OPEN - find what gates
    env(1106, ACTION_ARM_JUMP) in the air (likely a kick target in reach). Next: fix that, then the
    locked sprint-deflect (needs an enemy; this save's spot has none in WorldChrMan+0x998).

## Fall death, void return (2026-10-16)
- Exe (static): FUN_14084e5d0 = the kill test: fall height >= HitMtrlParam +0x4c (`fallDeathDist`, 40 default, 80 on deep-water planes 22/36/38), and only from 20 m up (DAT_143b01948 = 20.0, read from sekiro.exe .data; 5 m while SpEffect type 0x155 is worn, DAT_143b01950/54). FUN_140843660 = the damage factor (not the kill); FUN_1408433f0 reads the row's +0x50 `fallDamageRate`. Debug-menu FUN_140a27c60 case 7 confirms the two fields. Same numbers as sekiro-rs (its source was the same table).
- Sv1: `map::fall_death_dist` (HitMtrlParam, disableFallDamage rows = no death), landing in player.rs: fatal drop -> `LandDeadReturnStart` (a000_200909, HKS DAMAGE_TYPE_LAND_DEAD_RETURN) -> `GroundFallReturn` at `Player::safe_pos` (last dry ground), HP kept; falling 40 m with no ground below -> `FallDeadReturnStart` (a000_199120). Test `a_forty_metre_fall_returns_wolf_to_safe_ground`. The three states were added to tools/sekiro-extract/export_states.txt (re-export needed).
- gap: the non-lethal HP loss (20 m up, FUN_140843660's factors), whether the game returns or kills outright, the void kill height / return volume. Needs a live drop test (recorder).
- sekiro-rs hints checked 2026-10-16: gravity, air control, 720 deg/s, radius 0.4, input buffer, walk twist were already in Sv1 from the same exe tables. Its ledge hang / wall hug / swim numbers (HANG_DEPTH 1.5, OUTWARD 0.35, reach 1.0, 45 deg) are marked "placeholder, not traced" there, so they are no better than a guess.

## Recording check (2026-10-16, rec_20261016_083843_movement_hooks, m10, 236 s)
Live Sekiro (rec.db, slot 0) vs Sv1 clip data / sim:
- Run start from idle (speed every 67 ms): live 1.0 2.6 4.2 5.6 6.2 6.0 5.5 5.7 5.6, Sv1 1.8 2.5 4.2 5.7 6.3 6.0 5.7 5.6 5.6. Match.
- Loop speeds (median m/s, live vs clip root motion): sprint a000_001151 8.68 / 8.74, run a000_000420 5.50 / 5.50, crouch walk a000_021402/3 2.32 / 2.36, run a000_000400 5.12 / 5.55 (live includes the start ramp). Match.
- Run jump: live rise 1.96-1.97 m (a000_201000 / 201200), Sv1 1.97 m. Match.
- Not settled: walk a000_000100 (live median 0.49 m/s, clip 1.64 - transitions dominate, needs a clean walk recording); locked sprint-deflect facing (no clean case in this recording).

## Clean movement recording (2026-10-16, rec_20261016_092035_movement_clean, 186 s)
- Loop speeds, live vs clip root motion (m/s): walk a000_000100/200 1.62-1.63 / 1.64 (the first recording's low walk value was transitions), strafe walk 000201 1.43 / 1.42, run 000500 5.58 / 5.61, lock-on run 000501 4.28 / 4.27, 000503 4.21 / 4.19, 000403 4.22 / 4.16, sprint 001151 8.80 / 8.74. Match.
- Run jump: live press -> landing clip (a000_201055) 0.93-1.07 s, rise 1.93-2.00 m; Sv1 1.02 s, 1.97 m. Match.
- AirKick CORRECTION: the recorder's anim field never shows a000_213100 (the kick is not in TimeAct +0x100; the fall loop 299020 stays), so the 2026-10-15 "no air kick when mashing" conclusion was wrong. SpEffects 100334 + 100247 (AirKick's own) fired 6 times in this recording, each when jump was pressed in the air, two of them 0.9 s apart in ONE flight (ticks 8842 / 8896). Sv1's rule (kick again once ref 431's 0.4-0.7 s window is over) matches that spacing. Use the SpEffect events, not the anim, to find kicks.
- Open: lock-on turn rate and the sprint-deflect facing still have no isolated case; jump presses are not recorded (no pad block in this recording), so mashing vs single presses can't be separated.
- Jump presses vs kicks (decoded pad, recorder session schema 11, movement_clean): 14 jump presses in the air, 6 kicks (W_AirKick). A press kicks only when it lands after the take-off frames (a press 12 ticks after take-off did nothing, the next one 24 ticks in kicked, W_AirKick 2 ticks later) and never within about 0.4 s of the previous kick (presses 1, 12, 14, 24, 26 ticks after a kick: nothing; 54 ticks after: kick). Presses on the landing frames also do nothing. Same shape as Sv1's accept-flag gate plus the ref 431 window; a live-vs-sim run with the same press ticks is still to do.

## Lock-on turn and sprint deflect (2026-10-16, rec_20261016_094717_lockturn_sprintdeflect, 57 s, 7 locked sprint deflects)
- Sprint deflect (SprintToDeflectGuard, a050_203001): NOT a snap. Wolf keeps his sprint heading (90-150 deg off the enemy) for the first ~0.1 s (Disable Turning flag 7), turns 180 deg/s to 0.2 s and 360 deg/s to 0.3 s (the clip's SetTurnSpeed), then at the controller default 720 deg/s until square on the enemy (0 deg error at +0.5 s). Sv1 now turns the same way (player.rs sprint deflect block; test `a_locked_sprint_deflect_turns_to_the_enemy_instead_of_snapping`); the old snap and the "turns at most 54 deg" gap are gone. gap: the exe's target for that turn (HKS sets no facing) is taken as the lock target.
- Standing deflect: Wolf is already facing the target (+-6 deg live), so the aim stays.
- Locked strafe (a000_000502/503/602): facing stays 1-2 deg off the enemy while orbiting (follow rate 55-110 deg/s median/p90). Sv1 strafes at 0-1 deg error. Match.

## Run stop slide and debug test tools (2026-10-16)
- Bug: releasing the stick at a run slid Wolf about 1 m over 0.3 s. Cause: `data::blend_keeps_src` found no entry for `StandRunStopF/B/L/R` (no underscore, graph state `StandRunStop`, blend_src.json: keeps no source motion), so the stop kept the run's momentum through its blend. Fixed by also trying the name without its direction letter. Now 0.5 m/s three ticks after release and 0.3 m in all, as live (rec_20261016_* `W_StandRunStop`: 0.15-0.3 m) and as the clip a000_000600 (0.3 m). Test `releasing_the_stick_at_a_run_stops_in_about_a_third_of_a_metre`.
- Debug menu (F1) section MOVEMENT TESTS: fall test height (5 to 60 m) + "lift Wolf to that height and drop" (20 m = the kill limit starts, 45 m kills and returns him); test hook height (4-16 m) + "place a test hook" (10 m ahead of Wolf, rideable) + "remove the test hooks". File `src/debug_menu.rs` is outside the movement files; edited on request.
- Open: non-lethal fall damage HP, void kill height, grapple compared against the live wire shots (rec_20261016_083843_movement_hooks), ledge hang / wall hug / swim (need recordings).

## Jumping a sweep and kicking off the head (2026-10-16)
- Live (rec_20261016_* boss fights): over a perilous sweep, Wolf jumps forward locked on, holds at the contact distance in front of the enemy (1.14 m from the Monk, 0.95 m from the Owl) at about 0.7 m/s, and the kick lands 18-20 ticks after the press (19 of 19 kick jumps). At sprint speed most kicks miss in the real game too (11 of 13), as the kick's window (frames 9-21) opens after Wolf has passed the enemy.
- Bug: Sv1 pushed Wolf out of the enemy's body but kept the jump's full air speed, so he slid round the head and flew past at 6-7.5 m/s; the kick missed and he landed behind the enemy. Fix (actor.rs `separate`): a body pushed in the air loses its launch velocity (stick air control still adds its bit). gap: Havok's char proxy response is not traced; this is the recorded result. Sim scan: 32 of 32 sweep jumps (jump 6-18 frames before the hit, kick 2-12 ticks after launch, stick neutral or forward) now kick off 18-21 ticks after the press.
- Test `a_forward_jump_over_a_sweep_stops_at_the_body_and_the_kick_lands`.

## Bodies block, they don't shove; locked-on speeds (2026-10-16)
- Live (rec_2026101x_*): an idle Wolf (a000_000000) with an enemy within 1.3 m moved in 32 of 1267 ticks while the enemy closed in and stopped at 0.95 m; Wolf running into an enemy (9306 ticks within 1.2 m): both median 0.0 m/s along the line between them. So the char proxies stop each other; neither is shoved.
- Bug: Sv1's `separate` split every overlap half each, so an enemy walking or lunging into Wolf slid him in his idle pose. Fix: each side's share of the push is its own approach this tick (the one that walked in is stopped; both moving: by how much each came in; neither: half each). gap: Havok's char proxy response is not traced. Test `an_enemy_walking_into_idle_wolf_does_not_slide_him`.
- Locked-on run speeds, 6-tick averages, live loop clips (a000_0005xx) vs Sv1: F 5.54 vs 5.64, B 4.06-4.26 vs 4.41, L 4.25 vs 4.18, R 4.13 vs 4.13; diagonals use the F / B clips in both (live 400 at -45 deg 5.56). B is about 0.2 m/s fast in Sv1 (open, small).

## Deflect spam replays (2026-10-16)
- Spamming guard steps the chain StandToDeflectGuard (a050_203000) -> 2 (203005) -> 3 (203006) -> 4 (203007), then every further press replays 4 (HKS BEH_A_DEFLECT_GUARD_CONTINUE, ref 212). Live the same order: 203006 -> 203007 33 times, else 203010 or a recoil.
- Bug: anim.rs crossfaded only when the shown clip's key changed, so each replay of 203007 snapped the pose back to frame 0. Now a replay (the state clock running back to 0 on the same key) fades by the clip's TAE Blend like any transition (applies to every character). gap: the graph's self-transition taken as its TaeBlend; not visible live (the recorder's anim field does not change on a replay). Checked by eye only (the sim tests draw no poses).

## Turning and the move request from the stick, checked in the emulator (2026-11-03, exe)
Wolf's PlayerIns update (FUN_140a37560, vtable 0x142a2b338 slot 0xb0) runs whole in tools/emu-cases on the
2026-11-03 snapshot: `emu-cases <exe> extracted/emu --snapshot <dir> turn` writes `turn_frames.json`, 14
scenarios / 440 frames (the stick written into the pad floats [pad +0x38f8]: [0] forward, [1] side, +1 = world
-X), test `wolf_turn_matches_the_exe` (src/sim_tests/emu_turn.rs). What a frame does:
- Pad manipulator (PlayerCtrl FUN_1407f0a70 -> free FUN_1407f0f50, locked FUN_1407f1710; object
  [[PlayerIns+0x50]+0x358] or +0x120): the move vector is (-side, 0, -forward) rotated by the camera yaw +0x94
  (-> +0xc0). It is taken only when last frame's raw stick (+0x340) was off centre too and within 45 deg of it
  (DAT_143289178 = pi/4, cos against the normalised dot); otherwise it is 0 this frame. So a press from rest moves
  one frame late, and a flick of more than 45 deg in one frame (90 deg, or a 60 deg/frame sweep: every frame) reads
  as a let-go (MoveSpeedLevel 0 - the HKS move stop runs on MoveSpeedLevel <= 0). No dead zone in the manipulator
  (0.05 is taken).
- MoveSpeedLevel (behaviour variable 2, FUN_140b4afd0 -> behaviour +0xbac) = the taken stick's length, not
  rescaled (1.2 reads 1.2); 0 when chr byte +8000 >= 0x80; 2.0 when moving with module (bag+0x80)+0x184 bit 2
  (sprint); clamped by ctrl mode [[ctrl+0x50]+0x50]+0x520 (1: <= 1, 2: <= 0.1, 3: 0). So the HKS walk/run
  hysteresis (0.9 / 0.7, `stick_runs`) reads the stick length as Sv1 does (gap closed). MoveAngle (+0xbb0) is 0 free.
- Yaw request (+0x24) = the stick's yaw minus the physics yaw, wrapped to [-pi, pi) (a half turn reads -pi).
- Next frame BehaviorTurn (PlayerCtrl +0x250, FUN_1407da6d0 -> FUN_1407dac80) adds it to the physics yaw
  (FUN_1407de0c0 -> FUN_140bc29b0, physics +0x74): whole for the player (BehaviorTurn +0x70 bit 0, set only by
  FUN_1407da5f0 from the PlayerCtrl ctor) when no rate is set (TAE 224 action +0x308 and module(0xb8)+0x16c < 0),
  else clamped to rate * dt (FUN_1407dab40; TAE 360 -> 0.1047 rad a frame). The run loop a000_000400 and walk
  a000_000100 have no TAE 224, so free running the facing snaps to the stick one frame after it moves. Live:
  rec_20261015_191200 tick 679 request 0.154 -> tick 680 yaw +8.9 deg; tick 687 request 0.855 -> tick 688 +49 deg;
  each small request is the next frame's yaw step. Locked on, the locked run a000_000500 (no TAE 224) turns 12
  deg a frame = 720 deg/s (rec_20261016_094717 ticks 357-363): Sv1's locked turning at turn_rate is right.
- Drawn facing: PlayerCtrl overrides GetRotation (vtable 0x1429d5508 slot 0x40, FUN_1407f2e90): yaw from the
  delay controller (BehaviorTurn +0x50; FUN_1407da4a0 -> +0x20 euler, +0x30 angular speed), pitch / roll from
  physics; the generic ChrCtrl's (FUN_1407dcd70) returns physics. The controller (FUN_1407e7970 -> FUN_1407e7da0)
  chases the physics yaw at 1.1x the picked rate (720 -> 13.823 rad/s, 0.2304 rad a frame), the short way, speed
  min(speed + rate * p0, rate * p1) with action +0x320 / +0x324 / +0x328 = 1 / 1 / 0 (TAE 704 sets them; c0000 has
  none), the speed reset on a direction flip, landing when the rest is within one step (speed 0). A 90 deg snap is
  drawn over 7 frames, 180 over 14.
- Sv1 before: one yaw for logic and drawing, turned toward the stick at 720 deg/s (90 deg in 8 frames) the frame
  the stick moved; no 45 deg gate. Now (`player::Turn`, `actor::ShownYaw`): the gate on the pad stick, the request
  made one frame and applied the next (whole, or clamped at the shown clip's TAE 224 rate), the drawn facing
  chasing at 1.1x. The gate drives the walk / run and the free turn only; step, jump and attack
  directions still read the stick as it is (gap: which stick the HKS's KickAngle / AttackAngle read on a press
  frame is not traced; 10 sim tests press with the stick set that same frame).
- gap: the drawn yaw is engaged only by the turn controller's free turns (a yaw other code sets is drawn at once);
  whether the exe's warps / SetRotation reset the delay controller, and that the model matrix (and the dummy-poly
  hit boxes) take GetRotation, are inferred, not traced. Not ported: the camera-yaw hold while the stick is held
  (+0x424 / +0x428, option DAT_143d5b49c; re-sampled past 60 deg), the walk halving flag ([ctrl+0x50]+0x88 bit 18),
  the PlayerCtrl +0x259 TurnAngle gate, the module +0x16c rate, the pad's own processing before +0x38f8 (dead zone,
  XInput scaling). The lock-on manipulator (FUN_1407f1710) is checked in kb/camera.md (camera2). MoveSpeedLevelReal
  (HKS GetMoveSpeed) converges toward the stick level (2 only sprinting); Sv1's `speed_level` keeps its own 1 walk /
  2 run scale (it only picks the stop clip and the release slow-down).

## Character proxy against the exe (2026-11-03, exe, emulator)
`emu-cases <exe> extracted/emu proxy --snapshot <dir>` (tools/emu-cases/src/proxy.rs) runs Wolf's whole chr
frame FUN_140a4b570 on the Naomori-arena snapshot, 13 scenarios / 810 frames (314 compared) at dt 1/60 and 1/30, with his
requested velocity (x, z) written into the proxy at hknpCharacterProxy::integrate (FUN_1418f8b60, proxy +0xb0; the
game's y kept), and records his feet (physics module +0x80) -> `proxy_frames.json`. Test
`wolf_proxy_at_walls_and_on_open_ground_matches_the_exe` (src/sim_tests/emu_proxy.rs). The path each frame:
behaviour world step FUN_141015fa0 -> CSHkBehCharaProxySceneModifier::vf3 FUN_140fe7310 -> per character the
physics pre-step (FUN_141043cd0; FUN_140bcac10 puts the proxy at last frame's feet lifted by physics +0x140 =
+0x188, 0.3 m, or +0x18c 0.8 when [phys+0x9c8] == 2; constants from FUN_140bbb010), the world's proxy pass
(FUN_14093bb50 -> FUN_14093cfd0: checkSupport FUN_1418f82a0, velocity, integrate with gravity (0, -9.8, 0)), the
post-step (FUN_141044050 -> FUN_1407de780 -> FUN_140bc4390 -> FUN_140bca6d0: read back, lowered to the ground,
copied to physics +0x80).
- Wolf's proxy: capsule r 0.4, 1.5 m (proxy +0x90 (-0.4, 0, -0.4) / +0xa0 (0.4, 1.5, 0.4)), keepDistance 0.05.
  Moved lifted 0.3 m, so the capsule spans 0.3-1.8 m over the feet while it moves.
- Open ground: the requested horizontal velocity is realised whole (dx = v dt to 1e-4 over 120 frames on the
  +-0.4 m bumps), y follows the floor; standing, the proxy's y velocity is one frame of gravity (-9.8 dt = -0.163)
  each frame, reset by the frame. RSekiro (horizontal move, floor snap) matches.
- Rock face (heading 60 deg, 7.7 m; normal (-0.93, 0.065, 0.363) from the exe's 0.4 m casts at 0.5 / 1 / 1.5 m):
  Wolf stops 0.082 m further out than the 1 m cast's sphere (0.02 of it the face's lean at the capsule's lower
  sphere, the rest keepDistance) and slides at the request's tangential part (1.549 m/s for 3 m/s at 30 deg; the
  mesh turns 1 deg further on, 1.586). 30 and 60 fps end 0.3 mm apart. RSekiro had no keep distance: off the
  wall 0.059 m worst head-on, 0.048 at 30 deg -> with `map::KEEP_DISTANCE` 0.05: 0.009 / 0.016 (along 0.03, the
  face's own curve). The top wall sphere now ends at the proxy's 1.8 m (was 1.95).
- Step-up: not run (no ledge 0.3-0.6 m in reach). Derived: the lifted capsule's bottom sphere (centre 0.7 m) climbs
  a contact within maxSlope 60 deg, so a sharp ledge up to 0.7 - 0.4 cos 60 = 0.5 m: RSekiro's STEP_HEIGHT 0.5 kept.
- Walking off the cliff edge (2.7 m along -Z): the capsule's rim holds him 0.10-0.15 m past the edge (y down 0.04 m
  on the rounded bottom), physics +0xa2 / +0xa3 clear on frames 56 / 57, then the fall at 23.52 m/s^2 (0.392 m/s
  a frame at 1/60; first air frame -0.359 = the ground frame's -0.163 - 0.196), semi-implicit (pos += v_new dt).
  RSekiro's walk-off (floor sampled 0.08 m round the feet, vel_y 0, fall type 0 23.52) differs by under 3 mm in y
  over the first 3 air frames; the edge distance depends on the mesh: gap, not compared.
- (Closed below in "Bodies, steps and the floor follow": the enemy was the c1150 dog, idle in physics mode 1.)
  The push between characters. With the General placed 1.6 m ahead (his frame run too, AI off) Wolf's proxy
  walks through his: the General's physics does not step in the snapshot (he stays where placed, 0.15 m over the
  ground), so the proxy-proxy response (and Sv1's `separate` 0.9 m contact, live 0.95-1.14 m) is not checked here.
- gap: map-mesh dependent, recorded but not compared: the post along +X 2.4 m (Wolf stops at 2.003, then slides
  round it), the face beyond 1.3 m along (wall_60, wall_sprint_30 walk on where the plane stops), the cliff fall.
- Emulator notes: a TB flush before each frame (else the host crashed on the 39th chr frame of one engine), the
  CRT longjmp replaced by a plain one in the copy (its unwind path spun on the RtlUnwindEx stub; the HKS GC jumps
  out every ~24 frames), __isa_available 1 (AVX2 strnlen).

## Move stick switches and the pad (2026-11-03, exe, emulator, topic hks2)
`emu-cases <exe> extracted/emu hks2_move --snapshot <dir>` (tools/emu-cases/src/hks2.rs) runs Wolf's PlayerIns
update as `turn` does, 7 scenarios / 168 frames -> `hks2_move.json`; test `move_stick_switches_match_the_exe`
(src/sim_tests/emu_hks2.rs). The two gaps left in the free manipulator (FUN_1407f0f50) are debug switches:
- Camera-yaw hold: DAT_143d5b49c is a debug menu bool (FUN_1425e1aa0, named "don't align the stick's forward with
  the camera's forward"), off in the snapshot. On, the camera yaw the stick is turned by (+0x420) is taken once
  when the stick engages (+0x424 latch) and kept while it is held, re-taken on a release or when the stick's world
  yaw strays from +0x428 (Wolf's yaw + request at the latch) by DAT_143b014b0 (another debug float, 60 deg). Run:
  the camera turning 4 deg a frame for 40 frames, the move direction stayed at the frame-1 camera (0.07 rad);
  off, it follows the camera of each frame. Sv1 (`stick_world` with the frame's camera yaw) matches the off case.
- Walk halving: ctrl flags +0x88 bit 18 does nothing unless the global 0x143d7a34a (next to DebugFlags, 0 in the
  snapshot) is set; then with 0x143b066e4 set (1) the move vector and MoveSpeedLevel are halved (1 -> 0.5), with
  it clear the stick is taken unrotated. Run: bit 18 alone, 12 frames identical to no bit. Sv1 ports neither.
- The pad before its floats: the object FUN_1409603d0 reads is NS_SPRJ::DbgMapWalkPad (vtable 0x142a09b30; ctor
  FUN_14095faa0 clears a 0x383c block at +0xb4 and points +0x38f0 / +0x38f8 into it; in the snapshot +0x38f8 =
  pad +0x20f8). FUN_140961460 binds its action slots to the FD4 user-input ids 0x223.. (FUN_1425e86b0); the
  device -> float step (XInput dead zone, curve) is in the FD4 input library, not traced: gap. The manipulator
  itself has no dead zone (0.05 is taken, `turn` above).
- gap: the lock-on manipulator (FUN_1407f1710), the PlayerCtrl +0x259 TurnAngle gate, the module +0x16c rate.
## Bodies, steps and the floor follow against the exe (2026-11-03, exe, emulator, topic collide)
`emu-cases <exe> extracted/emu collide_* --snapshot <dir>` (tools/emu-cases/src/collide.rs), tests in
src/sim_tests/emu_collide.rs. Same snapshot and path as the proxy section; the "General" there was the c1150 dog
(NpcParam 11500000, hitRadius 0.5, hitHeight 1.0: its proxy +0x90 / +0xa0 is a sphere at feet + 0.5), the nearest
enemy with a proxy, 27 m from Wolf.
- Why the enemy "did not step": not AI or a missing update. His chr frame runs the behaviour step and his physics
  pre-step (FUN_141043cd0 -> FUN_140bcc570), but the physics update (the function before FUN_140bc4390) picks
  +0xb10 mode 1 (no proxy move) for a body with zero velocity that rests (FUN_140bcae80); the player's +0xbbc 0x40
  forces a step every frame (mode 2: FUN_140bcac10 lift, integrate). An NPC (+0x100 set) that moves takes mode 3
  (FUN_14104da50, the world pass's second queue). Between frames every proxy rests at its feet (the post-step puts
  Wolf's back at +0x80); the mover is lifted 0.3 m for its integrate.
- Push (`collide_push_cases`, 11 scenarios, 289 frames compared): Wolf at 3 m/s into the idle dog at his own spot
  from 8 headings and at 30 fps. Wolf stops where his lifted capsule (spheres at feet + 0.7 and + 1.4, r 0.4) and
  the dog's sphere are 0.4 + 0.5 + keepDistance 0.05 = 0.95 apart in 3D: flat 0.8556 at dy 0.213 (exe) vs 0.8554,
  0.9472 vs 0.9471, 0.9498 vs 0.9496, 0.8758 vs 0.8761. RSekiro's `separate` summed the radii (0.9 flat) when the
  feet-to-top spans met: worst off by 0.025-0.050 m per scenario -> `actor::contact_flat` (capsule segments,
  keepDistance 0.05, Wolf lifted `PROXY_LIFT` 0.3, the enemy at his feet): worst 0.0003-0.0042 m (the arrival
  frame; at rest 0.3 mm). The dog stays put (Wolf walked in): `separate`'s "the one that walked in is stopped" holds.
  heading_45 (placed on a ledge 1.7 m over the dog, fell on him) and the side / 315 runs (the rock stops them
  first) give no contact.
- An enemy walking into Wolf: forced (+0xbbc 0x40 on the dog and our velocity at integrate) his mode-3 proxy
  passed through Wolf's (flat 0.24 m) while Wolf was nudged; with the AI live (`ai=1`, DebugFlags +0x10 cleared)
  the dog lunged round Wolf at 1.01 m without touching. gap: a moving enemy's proxy against Wolf (lift, filter,
  who gives way) is not settled; RSekiro keeps its recorded rule (the walker stops, live 0.95-1.14 m).
- Floor follow (`collide_snap`): Wolf put h over flat ground, standing, one frame: on the floor (+0xa2 set, y to
  1 mm) for h <= 0.58, falling from 0.62. The same with the lift +0x188 at 0.1 and with +0x184 / +0x18c at 0.5, so
  it is none of those (gap: the 0.6 constant is not found). RSekiro followed down STEP_HEIGHT 0.5 ->
  `map::FLOOR_FOLLOW` 0.6. Test `floor_follow_depth_matches_the_exe`.
- Step-up: `collide_steps` (0.05 m sphere casts on a grid, floor hits only, normal y > 0.85) over +-70 m finds no
  sharp 0.4-0.7 m ledge; only 0.15-0.25 m steps (stairs at x 138-141) and a stone block's corner at (187.5, 4.13)
  whose edge rises 0.3-0.7 m across Wolf's 0.8 m width. Walked onto it (`collide_stepwalk`, 11 walks at 3 m/s)
  along x 187.59 / 187.69 / 187.79 he climbs (edge 0.55 / 0.28 / 0.17 over the approach slope, riding up on the
  rim from 0.3 m out); x 187.39 / 187.49 start over the cliff below (no case). gap: STEP_HEIGHT 0.5 stays derived.
- Walking off a sharp edge (same block, +z): the bottom sphere rolls over the rim (0.01 m down 0.10 m past, 0.04 at
  0.2), then: drop 0.34 / 0.55 m: down on the lower floor 0.30-0.35 m past the edge, never airborne; drop 0.61 /
  0.74 m: airborne (+0xa2 clear) 0.15-0.20 m past; the 0.21 m step: down 0.15-0.20 past. Test
  `walking_off_an_edge_falls_past_the_floor_follow_like_the_exe` (follow vs fall by FLOOR_FOLLOW, 4 cases).
  gap: where along the rim he leaves (0.15-0.35 m, the drop matters) is not ported: RSekiro's floor sampling still
  holds him 0.08 m past; no rule fits all five without fitting.
- Slopes over maxSlope 60 deg: none walkable in reach (the steep faces the scan meets are walls, normal y 0.06).
- Emulator notes: thousands of sphere casts need their buffers allocated once (the emulator heap is 1 MB; the
  cast allocated 80 bytes a call and the 58k-cast scan crashed) and a TB flush every 256 casts.

## The copied collision against the exe (2026-11-04, exe, emulator, row E8)
sekiro-rs's `CollisionWorld` (crates/sim collision.rs) is what RSekiro stands on under sekiro-rs's map
(sr_map.rs `WorldCollision for CollisionWorld`: floor_hit, floor_material, raycast, slide, water_surface; the
enemies' slide and floors default to the same). Test file src/sim_tests/emu_srcollide.rs builds m11_00's play
world as sr_map.rs does (`sekiro_rs::map::world::collision_world`, SEKIRO_CACHE) and runs the cases already
recorded on the Naomori-arena snapshot through it (exe space = Bevy with X mirrored; no new emulator run: the
snapshot was deleted and the game is not started for this):
- Floors (`copied_floor_under_wolf_matches_the_exe`): 717 grounded frames of `proxy_frames.json` and
  `collide_steps.json` where the ground under the capsule is one plain surface: the exe's feet are 1.0 mm over
  the copied floor on average, worst 4.5 mm. Fixed: the map's compressed mesh shapes carry convexRadius 0.01
  (180 of 184 shapes of m11_00's `l` binder, 169 of 173 of `h`; read from the hkx, the other four 0) and the
  exe's bodies meet that pushed-out surface; the copied floor was the bare triangles, 10-12 mm under Wolf's feet
  (the exe's own 0.05 m casts land 0.01 above them too). Now `MESH_CONVEX_RADIUS` raises each map triangle's
  floor by 0.01 / |n.y| (placed objects keep theirs: polytopes are expanded by their own radius at export).
  495 frames on an edge or a step's nose are the terrain's rim support, not the floor query: not compared.
- Walls (`copied_slide_matches_the_exe`): 1,170 grounded frames, one frame each from the exe's feet with the
  frame's request. Matched within 1 cm: open ground, the rock face head-on / 30 / 60 deg at 60 and 30 fps (0.3,
  2.6, 0 mm), the post along +X (3.2 mm), the ground walks (5.8 mm). Fixed: the copied slide had no keepDistance
  (0.05, `KEEP_DISTANCE`) and no convex radius, so it stopped 0.05 m nearer the face (9 mm left after the
  keepDistance, from the top sphere); its spheres were the unlifted capsule's (0.9 / 1.1 m), so the leaning post,
  nearer at the shins, was met 1.1 cm late each frame. Now three spheres at the lifted capsule's 0.7 / 1.05 / 1.4 m
  (physics +0x188 lift 0.3, capsule 0.3-1.8 m), and a contact whose way out leans within maxSlope 60 deg of up
  (a step's nose) or of down (a beam overhead, #146's planks at 1.77 m) is no wall: support or ceiling, left to the
  floor and ceiling queries. The castle gate route test now arrives within 0.7 m of its waypoints (the body keeps
  0.06 m more off the wall it passes).
- Bounded, not matched (under 6 cm a frame): the sprint along the face past its 1.3 m plane (worst 3.8 cm where
  the rock turns), the 30 fps walk round the post (2.5 cm, one 0.1 m sub-step where the proxy integrates its
  contacts), the step walks: climbing a 52 deg nose 0.3-0.4 m high the exe's proxy stalls about five frames (4.9
  cm), and stepping down a riser the lower sphere is already in is pushed out at once (2.6 cm) where Havok recovers
  penetration over time.
- Rays (`copied_raycast_matches_the_exe_sight_rays`): the 756 enemy sight rays of `aiframe_ray.json`
  (FUN_14061c8a0, head to 36 points round three enemies): 746 agree. gap: 10 rays of 12400000 / 10200020 are clear
  in the exe but meet the map's own triangles 6-33 m out (parts the sight filter passes; not traced). The camera
  casts were recorded against the snapshot's camera module, not the map mesh: not compared here.
- Not covered by a case: water_surface (no water in the arena), the fall-assist walls (hold_inside), helper and
  event parts by filter, placed objects' own shapes, the ceiling and head sweeps, the 4 zero-radius map shapes
  (the copy gives every map triangle 0.01).

## A moving enemy against Wolf (2026-11-03, exe, emulator, topic aimove)

The General's forced attack 3000 from 1.6 / 2.6 m in front of Wolf, then his 1040 stepping in,
whole world frames with the AI on (`aimove_f_push_ledge`, `push_ledge_far`). The moving General
is in physics +0xb10 mode 2 and his proxy is integrated (FUN_1418f8b60) every frame with the
clip's velocity; it stopped at 0.950 m flat from Wolf (0.4 + 0.5 + keepDistance 0.05, the two at
the same height) for 30+ frames and Wolf (idle) did not move at all. That is `actor::contact_flat`
with the enemy's proxy at his feet and Sv1's `separate` giving the push to the one that walked in
(test `moving_enemy_stops_against_wolf_like_the_exe`). Emulator caveat: the Havok world step is
not run, so bodies stay in the broadphase where the snapshot had them; with Wolf moved next to
the General (the other aimove scenarios) the General's lunge passed right through him (0.05 m).
Not covered: an enemy pushing a moving Wolf (both moving), a lift for the enemy's proxy (the
same-height case cannot tell), the enemy against another enemy.
## In the air: jump, fall, NoGravity and fall damage against the exe (2026-11-03, exe, emulator, topic air)

tools/emu-cases `air` (tools/emu-cases/src/air.rs): whole character frames FUN_140a4b570 at 1/60 s on the
snapshot with Wolf's pad driven per frame (as acttrace), hooks on the TAE dispatch FUN_140b5c920, the NoGravity
read FUN_140bcc420, the physics step FUN_140bccd50, the fall module FUN_140bacc10 / FUN_140bac120 and the land
check FUN_140b97a30 -> FUN_140b97b30. Physics module = [chr+0x1ff8]+0x68: +0x160 air velocity, +0x854 fall top,
+0xa2 grounded, +0x92c the NoGravity byte; fall module = physics +0x1c0 (+0x180 launch timer; table +0x78 -0.1,
+0x7c -13.72, +0x80 10, +0x84 2). 17 runs (`air_jump.json`, `air_art.json`, `air_ng_launch.json`) and 24 drops
(`air_drops_*.json`, git-ignored extracted/emu).

- Jump: press f0, 201001 f1, launch 201011 f11 (9.64 m/s, gravity already applied on the launch frame: 9.248),
  -0.392 m/s a frame (23.52 m/s^2), the position on the mean of the old and new velocity, apex 1.975 m, touchdown
  after 49 air frames (0.817 s), 201050 at f59, land 201040 at f62. Matched as it was (`fall`).
- Air steering: decel 0.1 m/s^2 along the velocity, skipped below one step; the stick adds 10 m/s^2 split along /
  across the velocity, the along part only up to 2.0 m/s (off a 5.56 m/s run it adds nothing and the decel
  goes on); half stick half the rate. `player::air_step` gives every one of 873 exe air steps to 1e-6 (test
  `air_steps_match_the_exe`). Matched as it was.
- Walk-off (fixed): the first air step starts from the last ground frame's move: the walk / run velocity
  (-1.69 / -5.57 m/s) and one frame of world gravity (-0.163), then the air steps (-0.555, ...). Before: straight
  down from rest. `Player::ground_vel`; test `a_walk_off_keeps_the_ground_velocity`.
- Fall top (fixed): +0x854 is written on ground frames only and stays at the take-off height through the flight
  (GetFallHeight, the stiff landings and fall damage measure from it). Before: the flight's highest point, so a
  jump off a ledge measured 2 m more (part11 `jumps_off_ledges_land_stiff` now drops 3.5 m, not 2.3 + the rise).
  Test `the_fall_top_stays_at_take_off`.
- SetNoGravity (fixed): the physics step holds (no air step, the body still, +0x160 zeroed, the fall top set to the
  held height, +0xa2 reads grounded) when +0x92c is up and the launch timer is out. A TAE 920 sets the timer to
  0.251 s before its update's step, so a flag in the launch update (the head kick's [0, 0.2] and its 920 at 0.2)
  or within 16 updates after it does nothing. The update after a hold is a ground step (-0.163), then the air
  steps; a launch right after a hold (the head kick's relaunch) takes the air step instead. RSekiro now reads the flag on the exe's window for the player too (before: a point test, because the
  hold cancelled the launch). `player::air_frame`; test `no_gravity_waits_for_the_launch_timer` (ng_launch,
  ng_rise, ng_timer, ng_fall). gap: a flag new this update keeps +0x160 in the exe (zeroed the next), unused.
- Landing anims seen: still jump 201040; stick held 420, half stick 120, stick back 422 -> 420 -> 423; drops under
  20 m 200020, 20-39.9 m 200021 (long stiff), 40 m and up 200909 (LandDeadReturnStart); the jump kick 213100 then
  200020. RSekiro's choices for the drops match (test `fall_damage_matches_the_exe`); the jump lands are the
  existing HKS rules (not compared frame by frame).
- Fall damage (fixed, new): FUN_140b97b30 on the land frame, height = +0x854 - y: under 20 m (DAT_143b01948)
  nothing; from 20 m HP -= (int)(maxHP x CalcCorrectGraph 210(h) / 100), row 210 being the identity, with the
  factor rounded to f32 first: 20 m 223, 22 m 246, 25 m 280, 30 m 335, 35 m 392, 39.9 m 446 of 1120; from
  HitMtrlParam fallDeathDist (40 m default) HP -= (int)(maxHP x 0.4) = 448 (DAT_143b01958) and the return death,
  at 40 / 45 / 60 / 80 m alike. The debug no-death bytes (0x143d7a366 / 371) change nothing. Before: no HP lost,
  and the return kept HP. `player::fall_hp_loss`.
- StartTime clips: the land continuation's first TAE window is (t, t + dt) (kb/taewin.md).

Not covered: CalcCorrectGraph 211 / 212 and HitMtrlParam fallDamageRate (1.0 on the snapshot's ground, not
varied); the 5 m threshold under SpEffect type 0x155; a non-lethal fall that takes the last HP (548 HP never
reached it); an air art in the air (none started from the jump: the ground art ran, 107316700, and Wolf fell off
the cliff); the head kick (no enemy under Wolf); NoGravity with a horizontal velocity (all four windows on a
vertical jump); the void fall (FallDeadReturn) cost. (Graphs, fallDamageRate, 0x155, the last HP and the void
cost: next section.)

## Fall damage leftovers against the exe (2026-11-04, exe, emulator, topic air_falldmg, row E17)

tools/emu-cases `air_falldmg` (air.rs `falldmg`): no frames, direct calls on the snapshot's Wolf and his landing
module (bag +0x70): the factor FUN_140b98910 -> FUN_140843660, the kill test FUN_140b98ab0 -> FUN_14084e5d0, the
landing FUN_140b97b30 with the fall top +0x854 set h over him and HP written, the void fall FUN_140b975f0. The
landing material is poked in both places the exe reads it: [bag+0xa0]+0x10 (the rows for fallDeathDist and
fallDamageRate) and physics +0x940 (FUN_140ba8cb0, the disableFallDamage gate). SpEffects are put on Wolf's list
as guard.rs `add_sp`. The snapshot's debug no-death bytes (0x143d7a366 / 371 / 372) are cleared in all sets but
one: with them on, HP stops at 1 and the void fall costs nothing. `air_falldmg.json` (git-ignored extracted/emu).
Snapshot: HitMtrlParam 30 under Wolf, maxHP 1120, return cost (int)(1120 x 0.4) = 448.

- Factors (FUN_1408433f0): five, multiplied: CalcCorrectGraph 211 of a game-data ratio (+0x8fc, 50 here; the
  factor is 0 when that is 0), 212 of an SpEffect sum plus game data +0x25c, 213 of game data +0x264, the product
  of every worn SpEffect's SpEffectParam fallDamageRate (+0xdc), and HitMtrlParam +0x50 fallDamageRate. Rows 211 /
  212 / 213 are a flat 100 at every input, so with +0x8fc above 0 only the two rates count. SpEffectParam
  fallDamageRate is 1 on every row but 4050 / 4053 (0), and those are stateInfo 47, which zeroes the damage anyway.
- Materials: 22 / 36 (fallDamageRate 0.5, fallDeathDist 80): half the share (25 m 140, 39.9 m 223, 79.9 m 447 of
  1120), the kill from 80 m. Row 40 (disableFallDamage): nothing at any height, aged too (the gate comes first).
- SpEffect type 0x155 (9620, aging): the kill from 5 m (DAT_143b01950) on any floor but row 40, deep water too
  (the kill test does not read fallDeathDist then); under 5 m nothing (DAT_143b01954 is also 5).
- stateInfo 47 (640, 4050): the damage is multiplied by 0 before the kill is looked at, so a 45 m drop does nothing
  and plays no return.
- HP left (the debug flags off): short of the kill HP -= (int)(maxHP x factor) and a player emptied that way keeps
  1 HP (100 HP at 25 m -> 1); no hurt call (FUN_14117ae70) then. A kill with HP over 448 pays 448 and sets damage
  type 15 (the return); at or under 448 the damage is maxHP x 1, HP 0, the ordinary death FUN_14117b210 (type 16).
- Void fall FUN_140b975f0 (callers FUN_1409edf20 case 0xf with flag 1, FUN_1409f78a0 with 0; same result): HP over
  448 pays 448 and sets damage type 14 (FallDeadReturn); at or under it HP 0, type 6 and the death. Nothing while the
  no-death flags are on.
- Sv1 before: a non-lethal fall could empty HP (death); a kill with low HP still played the return; stateInfo 47
  did not stop the lethal landing; the aged landing (player.rs aging movement) played the return for free; the void
  fall cost 1/3 of max HP after the return (an earlier request). Now `player::fall_landing` (the landing, main and
  aged paths) and `player::fall_return_hp` (the walls and the void drop, taken when the return starts). An HP
  emptied by the void fall still cuts back to safe ground and dies there (the sim has no death in the void).
- Test `the_fall_landing_matches_the_exe` (emu_air: 704 landings, 192 factor + kill calls, 32 void calls).

Not covered: whole-frame drops on the other floors and aged (the clips after the landing), what FUN_14117ae70 does
after a non-lethal fall, the void kill height and when FUN_140b975f0's callers fire, the SpEffect fallDamageRate
product apart from stateInfo 47 (no row needs it), enemies' fall damage (FUN_1408433f0 leaves its factors 0
without game data; not run for an enemy).

## From the public repo (2026-10-19 to 10-23, merged 2026-11-03)

- 2026-10-15 LIVE auto-test (Sekiro via me3 `..\me3\sekiro-research.me3`, keys sent by
- Foot IK (2026-10-23, anim.rs `foot_ik`, Wolf only): c0000.hkx's hkbFootIkDriverInfo / FootIKControlsModifier values (rays 0.8 m up / down, ankle offset -0.5..0.5 m, ascending gain 0.2, descending 1, on/off 0.2) on a two-bone thigh / calf solve per leg, the foot keeping its animated turn; the pelvis drop is the exe's FUN_140b3f920 (clamp(feet height difference, 0, TAE 944 limit), 0.2 per 30 fps frame, not under TAE flag 113), so only crouch and the 944 clips lower the body. The player's capsule stands on the flat lo collision where the game has one (the Stairway platform's l-layer is a flat slab 0.2 m over the drawn craters, #276); the feet are cast on the collision the floor query uses. Gaps: ankle pitch / roll, which 944 float is which foot, the RootPos sign. `SHINOBI_FOOT_IK=0` turns it off.
- Enemy foot IK (2026-10-23, anim.rs `enemy_foot_ik`): 43 of the 73 enemy behaviours carry an hkbFootIkDriverInfo (Characters_c####.hkx) and an always-on hkbFootIkControlsModifier (Behaviors_c####.hkx); the 39 two-leg ones (legs ending in Foot_Target chains) run Wolf's solve with their own rays (0.5-0.8 m up, 0.5-2.0 down), ankle limits (mostly +-0.7; c5020 / c7300 -0.1..0.7) and ascending gain (0.2, 0.5 or 1; descending 1, on/off 0.2 everywhere), the pelvis drop on their Root bone. None: the 27 without a driver (c1000, the c1200 family, ...), c1040 / c1140 (controls modifier only). Cast through `Terrain::enemy_foot_floor`: on sekiro-rs's map the `h` ground kept around Wolf, else the enemy floor. Off-tick characters (Actor::lod) are skipped. Gaps: the four-legged c1100, c1150, c1370, c5080 (toe and hand chains), the driver's Foot_Target helper chains (the solve turns L_Thigh / Calf / Foot).
- Sv1: `map::fall_death_dist` (HitMtrlParam, disableFallDamage rows = no death), landing in player.rs: fatal drop -> `LandDeadReturnStart` (a000_200909, HKS DAMAGE_TYPE_LAND_DEAD_RETURN) -> back at the oldest of `Player::safe_ring` (the last 5 grounded ticks on dry ground, as the exe's ring: FUN_140a75f00 pushes every grounded frame, CSFallDeadStep::STEP_WarpPlayer FUN_1411d70e0 takes the oldest via FUN_140a71020; a ledge only grazed on the way down is not the return point, #201; a spot enters only over a walkable floor within 0.25 m of the feet, `player::safe_spot`, as the exe's FUN_140a00b40 gate (ground contact, no slide flag, a map hit under the feet), so resting against a too-steep face below a cliff is no return point; gap: MSB Collision UnkT17); falling 40 m with no ground below -> `FallDeadReturnStart` (a000_199120). Test `a_forty_metre_fall_returns_wolf_to_safe_ground`. The three states were added to tools/sekiro-extract/export_states.txt (re-export needed).
- 2026-10-22 non-lethal fall damage and the fall height:
  - The fall height is the last ground's Y minus Y now, not the flight's top: P+0x850 (Y +0x854) is
    copied only on a grounded update with no TAE 920 velocity change, and a warp
    sets it. `Player::air_top` is that, set at the top of player.rs decide on every
    grounded tick. A jump off flat ground reads 0; a ledge reads its own height.
  - Landing short of the kill: maxHP x `terrain::fall_damage_share` (FUN_140843660): 0 under 20 m,
    CalcCorrectGraph 210 (identity 20-100) x 0.01, graphs 211 / 212 a flat 100 %, x HitMtrlParam
    fallDamageRate (0.5 on 22 / 36 / 38), floored to whole HP; none on a disableFallDamage row or
    while stateInfo 47 is worn (SpEffects 640 / 4050 / 4053). No reaction clip of its own.
  - Aged (stateInfo 341): every landing from 5 m kills (the fall-return death), in the aged
    movement's landing too.
  - Kept: the fall return costs 1/3 of max HP (an earlier request); the exe's is 0.4 (DAT_143b01958),
    dying when HP is at or under it.
  - Tests: fall_damage_is_a_hundredth_of_max_hp_per_metre_from_twenty,
    an_aged_wolf_dies_falling_five_metres, a_forty_metre_fall_returns_wolf_to_safe_ground (25 m now
    costs 25 %); the jump-off-ledge tests set the ledge's height (2.3 m -> 3.5 m for the 3 m stiff
    landing from a jump). gap: whether the Grappling Hook's flight moves P+0x850.
- 2026-10-22 swimming in the map's own water:
  - The water planes are HitMtrlParam rows 20-24 and 35-38; the deep ones (22 / 36 / 38, fallDamageRate
    0.5, fallDeathDist 80) over a bed are surfaces, not floors (`Terrain::floor_tri` takes them only
    with nothing under them: the Yasha Ape's pool, m11 51000000, is one), and `terrain::water_surface`
    gives the highest water plane under a height (the cell's triangles, or sekiro-rs
    `Ground::water_surface` on the SR map through `WorldCollision::water_surface`).
  - swim.rs `water_at`: the boxes first, else a deep plane within 3 m above the feet makes the
    water column below its height.
  - gap: the native swim entry (BEH_R_STAND_MOVE_TO_SWIM 31) and its depth rule are not traced;
    env 3025 is not a swim test. A long fall into deep water no longer reads material 22 for the
    fall-death distance (the floor below is the bed).
  - Test a_deep_water_plane_is_a_surface_not_a_floor (terrain.rs).

## Atlas movement pass (2026-10-19)

- Walk-off: HKS BEH_R_FALL starts the fall only once env 224 (the height fallen) is 30 cm while
  moving (TAE refs 90-92 or the move loop). Ours keeps the move loop over the first 30 cm and
  lands without a land clip under it; Wolf follows the floor down 30 cm (`FALL_START_DROP`).
- Weapon state: BEH_A_GROUND_JUMP / WALL_JUMP / ITEM_USE / ADD_SUB_WEAPON_CHANGE need sheathed
  inside a non-combat area (ref 600: SpEffect 100298 / 100308 / 108900) and drawn outside it
  (`arm_style_matches`). With X sheathing anywhere, a sheathed Wolf outside such an area cannot
  jump, use items or switch tools.
- Landing: dodge pressed or held (env 1106 / 1108 SP_MOVE) with the sword drawn steps out of the
  landing before the stick and land clips (BEH_R_LAND 2027 / 2044). Into deep water from 20 m:
  LandWaterFreeFallLongStiff (a000_210011).
- Knockdown step: StandStepFromDown_* turns by the stick minus PRM_4DIR_JUMP_ANGLE_CENTER.
- Hang: a jump is always HangJumpReady (BEH_A_HANG_END_CONNECT before BEH_A_HANG_CLIMB); the hang
  button climbs. Wall kills only from the peek (ThrowGrabType COVER_LOOK).
- Land-ready poses (2026-10-21): env 248 = the exe's ground probe (physics +0x929 from FUN_140bce730: feet within the 0.3 m attach + probe radius; LAND_PROBE 0.4 m from the movement recording, 0.33-0.47 m), env 3003 = vy x 1000 (mm/s, gate -2 m/s). Replay match 5/5.
- Not done: crouch landings, swamp / waterside limits
  (floor material SpEffects are not applied), the turn lean of Move_onGenerate.
- Move window latch (2026-10-21): env 2000 = flag 11 now and last frame (exe FUN_140b36410 + FUN_1407d8ce0), so moves out of actions start a tick after the window's first frame (player::accept_move). Jump gap seen in the recordings: the vertical jump rises from tick ~11 after W_GroundJumpReady (y 0.16 at tick 12, peak 1.98 m at 36, touchdown ~60, land event 61-62); ours takes off ~2 ticks earlier (peak at 33, land event 60).
- Locked quick turn out of the run / walk stop (2026-10-21): locked on, letting go of the stick with the target more than 60 deg off runs the script's _GroundQuickTurn from BEH_A_GROUND_MOVE_STOP (W_StandQuickTurn{Right,Left}{90,180}); player.rs only turned from StandIdle, so the event fired every frame from StandRunStop*. hks_wolf now takes it from any state (test a_quick_turn_out_of_a_run_stop_is_taken_once). Open from the lock replays: after a locked sprint our walk-out turns to the target at once and the short way; the game keeps the sprint's facing through a000_000450 and turns in 000460 (rec 210505 @3076-3091).
- 2026-10-21 Locked sprint and TurnAngle: the stop validator / branch read TurnAngle in SprintStartFromStep even when locked (elsewhere MoveAngle when env 1118). In STYLE_TYPE_SPRINT states TurnAngle is the stick's angle, not the lock target's (hks_wolf move_vars; inferred from the game keeping a locked back-step sprint going, the exe's TurnAngle source manip+0x24 is not traced). Test a_locked_sprint_away_from_the_target_keeps_going.
- 2026-10-21 The move's stop is the script's: BEH_A_GROUND_MOVE_STOP's W_StandRunStop / W_StandWalkStop (by TAE ref 91 MOVING_RUN on the shown clip) is taken out of Locomotion whatever player.rs's speed level (hks_wolf, player::play_move_stop). Test a_move_out_of_a_step_let_go_at_once_stops_once.
