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
- 2026-10-10 slide: crouch while ref 1 (sprint) is up -> W_SprintToCrouchReady (a000_216010, its root motion
  slides), at its end TurnAngle > 0 -> SprintToCrouchLeft (216020) else Right (216021), crouched (HKS
  c0000_transition.lua 4498 / 405). gap: ACTION_UNLOCK_TYPE_SPRINT_TO_CROUCH (26) taken as unlocked.
- 2026-10-10 sprint deflect: SprintToDeflectGuard (a050_203001) slides 3.04 m on root motion and turns only
  by TAE SetTurnSpeed (180 f3-6, 360 f6-9); Sv1 snapped Wolf to the target first, so the slide carried him
  into the enemy (user: "I launch to the enemy"). HKS BEH_A_DEFLECT_GUARD_START sets no facing; now no snap.

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
- Quick-turn loop (user: "Wolf shaking with no input"): StandQuickTurn{Left,Right}{90,180}
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
  1.5 m capsule.
- Open: jumping feel (user: "not the same as the game", details pending); foot IK on the run
  loop; _set4DirJumpTilt; wall jump (HKS BEH_A_WALL_JUMP, not in Sv1); ledge hang / climb (edges
  loaded, unused); swim.
- Sprint stop fixed (actor.rs advance): a transition that drops the source motion (blend_src
  false, e.g. SprintLoop) weighted the to clip's root by the blend too, so SprintStartFromStep_F
  -> SprintLoop went 8.6 -> 0 -> 9 m/s over 0.3 s (user: "slows down then speeds up"). Now full
  weight from frame 0. Test sprint_keeps_its_speed_into_the_loop. Still from the data (not bugs):
  the run start a000_000400 dips 5.6 -> 4.9 m/s at 0.9-1.0 s, the step -> sprint start blend dips
  to ~3 m/s for 0.1 s (ref 15 opens at 0.4 s while the step brakes; blend 0.2 s keeps the source).
- Air kick: HKS 2814-2823, under ref 431 (SpEffect 100247, AirKick 0.4-0.7 s) a jump press only
  fires the additive W_AddActionInputJump, no new kick; now honoured. Test
  mashed_air_kicks_kick_once_per_jump (a plain jump allowed only one kick already: jump start's
  119 opens at 0.1 s, the jump lands before AirKick's 0.7 s). gap: the additive clip; where the
  user's repeated kicks came from (off an enemy's head?) not reproduced.
- 2026-10-15 LIVE auto-test (Sekiro via me3 `C:\DeadlockModding\me3\sekiro-research.me3`, keys sent by
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
- AirKick CORRECTION: the recorder's anim field never shows a000_213100 (the kick is not in TimeAct +0x100; the fall loop 299020 stays), so the 2026-10-15 "no air kick when mashing" conclusion was wrong. SpEffects 100334 + 100247 (AirKick's own) fired 6 times in this recording, each when the user pressed jump in the air, two of them 0.9 s apart in ONE flight (ticks 8842 / 8896). Sv1's rule (kick again once ref 431's 0.4-0.7 s window is over) matches that spacing. Use the SpEffect events, not the anim, to find kicks.
- Open: lock-on turn rate and the sprint-deflect facing still have no isolated case; jump presses are not recorded (no pad block in this recording), so mashing vs single presses can't be separated.
- Jump presses vs kicks (decoded pad, recorder session schema 11, movement_clean): 14 jump presses in the air, 6 kicks (W_AirKick). A press kicks only when it lands after the take-off frames (a press 12 ticks after take-off did nothing, the next one 24 ticks in kicked, W_AirKick 2 ticks later) and never within about 0.4 s of the previous kick (presses 1, 12, 14, 24, 26 ticks after a kick: nothing; 54 ticks after: kick). Presses on the landing frames also do nothing. Same shape as Sv1's accept-flag gate plus the ref 431 window; a live-vs-sim run with the same press ticks is still to do.

## Lock-on turn and sprint deflect (2026-10-16, rec_20261016_094717_lockturn_sprintdeflect, 57 s, 7 locked sprint deflects)
- Sprint deflect (SprintToDeflectGuard, a050_203001): NOT a snap. Wolf keeps his sprint heading (90-150 deg off the enemy) for the first ~0.1 s (Disable Turning flag 7), turns 180 deg/s to 0.2 s and 360 deg/s to 0.3 s (the clip's SetTurnSpeed), then at the controller default 720 deg/s until square on the enemy (0 deg error at +0.5 s). This is the "deflect sideways, then see the enemy" the user described. Sv1 now turns the same way (player.rs sprint deflect block; test `a_locked_sprint_deflect_turns_to_the_enemy_instead_of_snapping`); the old snap and the "turns at most 54 deg" gap are gone. gap: the exe's target for that turn (HKS sets no facing) is taken as the lock target.
- Standing deflect: Wolf is already facing the target (+-6 deg live), so the aim stays.
- Locked strafe (a000_000502/503/602): facing stays 1-2 deg off the enemy while orbiting (follow rate 55-110 deg/s median/p90). Sv1 strafes at 0-1 deg error. Match.

## Run stop slide and debug test tools (2026-10-16)
- Bug: releasing the stick at a run slid Wolf about 1 m over 0.3 s. Cause: `data::blend_keeps_src` found no entry for `StandRunStopF/B/L/R` (no underscore, graph state `StandRunStop`, blend_src.json: keeps no source motion), so the stop kept the run's momentum through its blend. Fixed by also trying the name without its direction letter. Now 0.5 m/s three ticks after release and 0.3 m in all, as live (rec_20261016_* `W_StandRunStop`: 0.15-0.3 m) and as the clip a000_000600 (0.3 m). Test `releasing_the_stick_at_a_run_stops_in_about_a_third_of_a_metre`.
- Debug menu (F1) section MOVEMENT TESTS: fall test height (5 to 60 m) + "lift Wolf to that height and drop" (20 m = the kill limit starts, 45 m kills and returns him); test hook height (4-16 m) + "place a test hook" (10 m ahead of Wolf, rideable) + "remove the test hooks". File `src/debug_menu.rs` is outside the movement files; edited at the user's request.
- Open: non-lethal fall damage HP, void kill height, grapple compared against the live wire shots (rec_20261016_083843_movement_hooks), ledge hang / wall hug / swim (need recordings).

## Jumping a sweep and kicking off the head (2026-10-16)
- Live (rec_20261016_* boss fights): over a perilous sweep, Wolf jumps forward locked on, holds at the contact distance in front of the enemy (1.14 m from the Monk, 0.95 m from the Owl) at about 0.7 m/s, and the kick lands 18-20 ticks after the press (19 of 19 kick jumps). At sprint speed most kicks miss in the real game too (11 of 13), as the kick's window (frames 9-21) opens after Wolf has passed the enemy.
- Bug: Sv1 pushed Wolf out of the enemy's body but kept the jump's full air speed, so he slid round the head and flew past at 6-7.5 m/s; the kick missed and he landed behind the enemy. Fix (actor.rs `separate`): a body pushed in the air loses its launch velocity (stick air control still adds its bit). gap: Havok's char proxy response is not traced; this is the recorded result. Sim scan: 32 of 32 sweep jumps (jump 6-18 frames before the hit, kick 2-12 ticks after launch, stick neutral or forward) now kick off 18-21 ticks after the press.
- Test `a_forward_jump_over_a_sweep_stops_at_the_body_and_the_kick_lands`.
