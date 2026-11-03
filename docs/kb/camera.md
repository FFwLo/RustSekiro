# camera: Camera: LockCamParam/CameraParam, lock-on, TAE 151/153/155

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: camera shake from data: TAE RumbleCam events (145/146 global on the player's anims, 144/147 local with
  FalloffStart/End metres on the enemy's) -> other/default.rumblebnd camera_NNN.hkx = hkaInterleavedUncompressed
  animations (42 shakes, e.g. 61: 0.17 s / 1.1 deg, 56: 0.3 s / 4 deg, 377: 3.3 s rumble); applied as the rotation
  delta from frame 0 (Havok -> game mirrored) on top of the follow camera.
- 2026-10-07 camera: CameraParam row 0 now exported. Pitch from it: rotRangeXMin..Max = -40..80 deg free (was config 65),
  rotRangeXAtLockMin..Max = -30..30 deg while locked on (camera.rs pitch_range; config camera.max_pitch removed).
  Still open (follow_smoothing gap): chase rates chrTransChaseRateXZ/Y_ForNormal 0.2/0.3 (+0xc/+0x10), lockRotChaseRate
  X/Y 0.6/0.3 (+0x110/+0x114), rotSpeed_Min/MaxX/Y (stick turn speeds); their per-frame vs per-second meaning needs
  the camera code: CameraParam is param index 0x66 (row getter FUN_140737d80), follow camera class ChrExFollowCam
  (vtable 0x1429a7d88), param wrapper ChrCamParamImp (0x1429a7228).
- 2026-10-07 gap 3 closed: FUN_140a368f0 (on PlayerGameData change) sets max HP = graph 500 (PGD +0x20), +0x2c = graph
  101, max posture = graph 501 (PGD +0x3c), then pushes them into the data module (FUN_140bd6410/20/30). Camera chase
  rates: the CameraParam row (0x1e0 bytes) is copied into the camera object at +8 (FUN_140736af0); consumer not found.
- 2026-10-07 camera follow from the exe: CameraParam row access = FUN_140741e70 (global copy DAT_143d59938 + 8). The ChrCam
  update FUN_14073c260 eases working rates (+0x220/+0x224/+0x228) toward chrTransChaseRateXZ/Y_ForNormal (+0xc/+0x10) and
  targetChaseRateXZForNormal (+0x6c), then FUN_141155d20 moves the focus in fixed 1/30 s steps, each closing `rate` of
  the gap (sub-step interpolation of the target). camera.rs focus: blend = 1 - (1 - rate)^(dt*30), XZ 0.2 / Y 0.3.
  (Wrong: a velocity chase; ported 2026-11-03, see "Follow camera checked against the exe" below.)
  follow_smoothing now only drives the lock-on yaw chase (lockRotChaseRateY 0.3 is the likely source - not traced).
- 2026-10-07 lock-on camera chase from the exe: FUN_14073c260 turns the locked camera by diff * rate * (dt / (1/30)) per
  frame, yaw rate lockRotChaseRateY 0.3 (+0x114), pitch lockRotChaseRateX 0.6 (+0x110). Yaw applied; config
  camera.follow_smoothing removed - every camera value now comes from LockCamParam / CameraParam (camera gaps closed,
  except our lock camera not chasing pitch).
- 2026-10-07 lock-on pitch (FUN_14073c260 ~asm 0x14073e000): pitch limits = rotRangeXAtLock (-30..30) widening toward
  the free -40..80 as |height gap| goes rotRangeLerpBeginHeight 0.5 -> EndHeight 3.0 m; target pitch = elevation to the
  target + FOV * 0.5 * LockCamParam lockRotXShiftRatio (0.45 -> ~9.7 deg looking down), chased at lockRotChaseRateX 0.6
  per 1/30 s. Implemented in camera.rs (the exe's exact elevation term uses an asin of a sight-line ratio; atan2 of the
  height gap over the flat distance stands in).
- 2026-10-07 sword-move cameras. TAE 153 CameraModule3 (dispatcher FUN_140b59170 case 0x99 -> FUN_140b51680 ->
  FUN_140832810/830/7f0 into the camera singleton DAT_143d5c0d0 +0x70..+0x85, cleared each frame by FUN_140833e40;
  consumed in ChrCam FUN_14073c260): distance target = LockCamParam camDistTarget chased at CameraParam
  lockCamParamLerpRate (0.05), or at the last EndInterpolationSpeed (+0x80, persists); during the event target =
  CamDistTargetOverride at StartInterpolationSpeed (param rate if <= 0); SlowStart eases from the first-frame distance
  by 1 - (p - 1)^2; the used distance (+0x214) follows at 0.1/frame. FOV (+0x50) and focus height (+0x204) lerp to the
  LockCamParam row at the same rate. TAE 155 SetLockParamID (case 0x9b -> FUN_140737430, slot 1..45, reset to 1 every
  frame in FUN_1407374e0) = CameraSetParam slot (entry id-1: lockParamId, camParamId, beginTime, endTime); row 0:
  slot 11 (air) -> CameraParam 1000 (faster Y chase), 41 (deathblow) -> CameraParam 4000 (lock chase 0.4/0.2),
  8 (death) -> 700/700; blends over beginTime (endTime going back). camera.rs CamState; deathblow test: 4 -> 1.5 -> back.
  Export: all LockCamParam / CameraParam / CameraSetParam rows.
- 2026-10-07 helper S2 integrated: TAE 151 look limits (camera singleton +0x4c..+0x58 -> FUN_140742230 absolute limits
  +0x230..+0x23c chased at lockCamParamLerpRate; +0x230/+0x234 clamp pitch, +0x238/+0x23c yaw) in camera.rs
  apply_look_limits. After the event the pitch limits chase back to the defaults at lockCamParamLerpRate
  (follow_player). Gap: cone centre from the dummy's basis.
- 2026-10-09 lock-on pitch ported from the exe (FUN_14073c260 0x14073d6xx-0x14073dd0e; camera +0xd0 = focus,
  +0xe0 = camera, written as focus - dir * dist): P = focus + CameraParam lockTgtPosRate (0.7) * (target
  DummyPoly 220 - focus); half = FOV (+0x50, LockCamParam camFovY in rad) * lockRotXShiftRatio (+0x278) * 0.5;
  a = asin(dist * sin(half) / |P - focus|) (pi/2 when not reachable; law of sines: P sits half above the
  screen centre); target pitch = a + half - elevation(P - focus) (pitch helper FUN_140733e50 =
  -atan2(y, xz)); limits rotRangeXAtLock lerped to the free range by |P.y - focus.y| over
  rotRangeLerpBegin/EndHeight; chase lockRotChaseRateX * dt / (1/30) outside lockRotChasePlayAngX (0).
  Result: ~20-30 deg looking down in melee (was ~8, a guess), steep over the target on head kicks.
- 2026-10-10 lock-on target switch (static, LockTgtManImp): update FUN_1409c5fe0 calls
  the stick switch FUN_1409ca2d0 (+0x29c0 flick mode off by default; FUN_1409c9d40 flick variant is debug only) and the
  mouse switch FUN_1409ca120 every frame. Stick: |axis| < 0.5 (@0x143289124) re-arms, > 0.95 (@0x14328918c) fires.
  Mouse: delta * -1 (@0x143289434), < 25 (@0x1432892d0) re-arms, > 50 (@0x1432892f0) fires. Each switch sets both
  cooldowns (+0x2844 / +0x2848) to 0.5 s. Pick FUN_1409ca480: screen-space d = cand - cur, score =
  cos(angle(d, input)) / |d|, best above cos(+0x2854 = pi/2). Candidates while locked: CamFront{Near,Far}Range
  LockChangeHalfAng yaw 60 / 45 deg, pitch 30 / 30, radius 8 / 200 (+0x2968..+0x2984). LockCamParam is not involved.
  camera.rs toggle_lock. gap: mouse units, and the input y sign (exe uses d.x*in.x - d.y*in.y).
- 2026-10-11 camera position chase (static, FUN_14073c260 0x140740764-0x140740d40; camera.rs chase_position):
  +0xf0 = orbit point (focus + dir * dist), +0x100 = chased position, +0xc0 / +0xd0 = focus, +0x110 / +0x40 = final.
  X / Z rate = targetChaseRateAtLockXZ (0x88, 0.4) when locked (+0x2a0 / +0x2a3) else working rate +0x228 (eased
  by dt toward targetChaseRateXZForNormal 0x6c, 0.1); Y = AtLockY (0x8c, 0.3) / targetChaseRateY (0x74, 0.4).
  X / Z rate * k, k from cos(angle(+0xf0 - focus, +0x100 - focus)) against cos(AngMin) / cos(AngMax) (0x78 / 0x7c
  10 / 20 deg; AtLock 0x90 / 0x94 = 0 -> k = 1): 0 inside Min, linear in cos to 1 at Max. Pitch of either vector
  outside +0x230 / +0x234 (0.1 deg slack) forces X / Z to 1 (14073.c 11883-11898: both prev and raw
  past the same limit, elevation = -FUN_140733e50 = atan2(y, xz); ported 2026-10-12, camera.rs chase_position
  `pitch`, limits = the TAE 151 ones or rotRangeMinX / rotRangeXMax). FUN_140831190 / 1408311b0 override (not
  traced). Then r += (1 - r) * +0x150; new = r * (+0xf0 - +0x100) + +0x100, no dt factor (taken per 1/60 s).
  Distance: |+0xf0 - focus| vs |+0x110 - focus| chased at +0x1e8 / +0x1ec = targetChaseDistRateXZ / Y (0x80 / 0x84,
  0.4), or for +0x1e4 (= targetChaseDistRateTime 2 s, set after a wall hit) toward _ForHitWall (0x58 / 0x5c, 1.0)
  by clamp(2 t / T); X / Z and Y parts rescaled separately. +0x150: set to 1 and +0x154 = rotCtrlEndWaitTime (0xa8,
  2 s) by stick / mouse camera input (unlocked); +0x154 counts down, then +0x150 -= dt / rotCtrlEndReleaseTime (0xac,
  1 s). lockCamAdjustRot_MaxX/Y / ChaseRate (0x128..0x130): no load off the param row in FUN_14073c260 (the
  +0x128.. reads at 14073.c ~9140 are camera-object fields) nor the other getter callers.
- 2026-10-11 the fulcrum (camera +0xe0), ported (camera.rs drag_fulcrum / place_fulcrum, test
  fulcrum_swings_the_camera_toward_a_sideways_run). 14073.c 9243 saves +0x90 (the chased character point); 9752-9770:
  fulcrum += (targetTransChaseRateXZ / Y 0x60 / 0x64 = 0.4, each lifted by (1 - r) * +0x194) * its move this frame.
  Unlocked, 9832-9880: yaw +0x174 = FUN_140734070(focus +0xd0 - fulcrum) every frame; with isEnableChrChaseRotX (0x28)
  the pitch +0x170 too (FUN_140733e50) when chrChaseRotXInvalidAngle (0x2c, row 0: 80 deg) <= 0, or a character
  vector (FUN_1409efea0 +0xc0, the move stick) is ~0 or its dot with the view is under cos(that angle); pitch ported
  2026-10-12 (see "Pitch chase" below).
  Then the rotation's forward re-places the fulcrum on the arm at its distance (10923), and with
  fulcrumDistRateByLrMoveMaxDist (0x40, 0.3) > 0 that distance = max((1 - clamp(|move . right| / 0.3, 0x44 0.1,
  0x48 0.99)) * |camera +0x40 - focus|, fulcrumDistMin 0x50 2 m) (10930-10990); first placement: fulcrumDistRate
  (0x4c, 0.5) * the camera distance, at least 2 m (9262-9285). +0x194 (9305-9397): 0 while the camera input
  (FUN_1409efea0 +0x80, or its +0xd0) is non-zero, 1 when it stops (held transChaseByControlEndWaitTime 0xa0, then
  falling over transChaseByControlEndReleaseTime 0xa4), 1 while locked. Effect: a sideways run leaves the fulcrum
  behind, so the camera swings to trail the run (6 m/s across: 31 deg at 0.5 s, 53 at 1 s, 75 at 2 s). Gaps: the
  pitch part; where TAE 151's limits sit against it (the resync is skipped while they run).
- 2026-10-11 TAE 151 look limits, FUN_140742230 (called from FUN_14073c260; param_2 = LockCamParam row, param_3 =
  CameraParam row): event (FUN_1408311c0 true): centre = view along -row 2 of the DummyPolyID world matrix
  (FUN_14073a1e0 -> FUN_1409e6c90), pitch FUN_140733d70 = -atan2(y, xz), yaw FUN_140733fb0 = atan2(x, z); yaw unwrapped
  toward +0x240 (seeded from +0x174); limits = centre + the four event angles; a full -180..180 yaw takes
  rotRangeYMin / Max as is. No event: pitch limits chase LockCamParam rotRangeMinX (deg) / CameraParam rotRangeXMax at
  lockCamParamLerpRate (per call, no dt), yaw = rotRangeYMin / Max (full circle in 136 / 142 rows; the 6 +-90 deg rows
  centre on row 2 of Wolf's matrix + rotBaseY). camera.rs now lerps back (was: cleared at once). Open: whether the
  dummy matrix row 2 is the FLVER forward (Wolf dmy 142 fwd = model -Z), so the centre still uses player -> dummy.
- 2026-10-11 lock switch mouse: FUN_1409ca120 reads (-v.x, -v.y), v = pad object +0x38f8 -> +0x20 / +0x24
  (FUN_14095fd40 / 50); the stick FUN_1409ca2d0 reads FUN_140960430 / 440 unnegated. The mouse state's writer is not
  found statically (scan for int->float stores at +0x20 / +0x24: none matching), so units and signs stay open.
- 2026-10-13 lock switch mouse, units (static): +0x38f8 is the pad's current frame snapshot (ring of 0xec-byte
  frames, set), filled by FUN_140960650: +0x20 = FUN_140965bd0 = analog action 0x54
  * s, +0x24 = FUN_140965c80 = action 0x55 * s, s = +1 if option byte DAT_143d5a988 +0x715 / +0x716 else -1 (likely
  camera invert X / Y, not verified); FUN_1409ca120's -1 cancels the default, so the switch sees the raw axis.
  Analog read FUN_1425e8150 (up to 4 bindings, type 2 = device float). The only DirectInput float
  source: MouseDevice poll FUN_141a8f540, GetDeviceState DIMOUSESTATE, +0x1b0 / +0x1b4 = (float)lX /
  lY, relative axis mode, no scale or accumulation: raw counts per poll, y down. So 50 / 25 are counts a frame,
  the same unit as Bevy's AccumulatedMouseMotion (camera.rs unchanged). Open: the 0x54 -> mouse binding table.
  Screen d in FUN_1409ca480 is NDC (view then projection matrix from camera +0x10..+0x4c,
  divided by clip w, no viewport transform; +0x29c7 remaps d via FUN_1402b8e90, not read): y up, so in.y > 0
  favours lower candidates; mouse down (lY > 0) picks lower, matching camera.rs. Open: the binding's lY sign;
  the stick (FUN_1409ca2d0 reads +0x10 / +0x14 unnegated) would pick lower on +y.
- 2026-10-13 lockCamAdjustRot_MaxX / MaxY / ChaseRate (row +0x128..+0x130) are unused: FUN_1407374e0 copies /
  blends whole LockCamParam rows (getter FUN_140737d80) via FUN_1407381d0 ( lerp
  them like every field) into DAT_143d59938 +8; its readers (FUN_140741e70 callers)
  never load +0x128..+0x130. Not ported (nothing to port).
- 2026-10-11 checked AKJama/sekiro-rs's camera / movement fixes (c1782f8, 502c9be, 8eaf879): locked diagonals with
  walk twist, the 4-dir step tilt and the dead-target lock drop were already here; the held-stick realign after a
  lock drop already works (probe test held_stick_realigns_after_the_lock_drops). Their camera has no fulcrum.

## Close camera: Wolf fades, safe margin (2026-10-12)

- No minimum distance: the collision cast keeps max(hit, 1e-5) of the arm (FUN_14073c260 14073.c
  12318-12395). Ours was clamped to 0.3 m, so a wall behind Wolf put the camera inside his head.
- Safe margin (14073.c 12709-12730, under Game.IsUseSPChrExFollowCam, default 1 in FUN_14097b330):
  the final camera moves toward the focus (+0xc0) by min((d - 1e-5) * camSafeMarginRate 0.1,
  camSafeMarginMax 0.3). camera.rs `safe_margin`.
- CamFade (14073.c 12743-12759; slot manager FUN_140b21ca0 / FUN_140b22000 in 140b2.c): when
  camera +0x3ae is set and |+0xa0 - camera| < CameraParam playerFadeDist (0.5 m), the player is
  asked to fade that frame. A slot's alpha falls at AlphaDecreaseRate 2.5 / s to its floor (the
  player's: camera +0x40c, 0) and climbs back at AlphaIncreaseRate 5 / s (defaults of debug floats
  Game.Camera.Obstacle.*, FUN_140b216b0; ExtensionOfTime 0). PlayerIns vtable +0x400
  (0x140a2c230) stores it at +0x2074 and FUN_140a2f780 writes it to every model slot.
  camera.rs `cam_fade_step` / `fade_player` -> Actor::cam_alpha -> model.rs update_opacity.
- Gaps: +0xa0 taken as the focus; the draw is a plain alpha blend (the exe's consumer may be the
  DitherFade material property, not traced).
- Obstacle fade, parked (14073.c 11253-11320): with camera +0x3ad, a camCastSphereRadiusForFade
  (0.3 m) sphere cast from the desired camera (+0x100) toward the midpoint of +0x90 / +0xa0,
  shortened by the radius, FUN_14094ea80 with Havok filter group 0x76 (118, "Camera (fade
  occluding OBJ)"; filter info bits 0-6 = group, FUN_140932c00). FUN_140933390 pairs group 118
  with objects (0x18 / 0x19 / 0x1b / 0x20), 0x29, 0x2f, 0x43, 0x6c, 0x77 and the enemy / boss
  character groups 2, 5, 7, 10, 0x66, 0x67, 0x69, 0x6a; not the player (1). Each hit
  (FUN_140950620: short kind at +8; 1 = ChrIns, 6 = object, 0 / 2 ignored) goes to FUN_140b21d40
  (fade to AlphaMin 0.2), a character only when byte +0x1f of *(owner + 0x18) is 2
  (FUN_1408705a0; FUN_14095ce70 wants 1 for its own use). That byte is not identified, so
  whether ordinary enemies fade is open: a live read of it on an enemy settles it.
- Pitch chase, done 2026-10-12 (camera.rs `chases_pitch`): the unlocked pitch is read back from
  the dragged fulcrum too (FUN_140733e50 = -atan2(y, |xz|) of focus - fulcrum) when
  isEnableChrChaseRotX and (chrChaseRotXInvalidAngle <= 0, or the manipulator's +0xc0 ~ 0, or
  dot(+0xc0, -camera z row) < cos(angle)). FUN_1409efea0 = the chr's ChrManipulator (ChrCtrl
  +0x358 override, else +0x120; PadManipulator for Wolf); +0xc0 = the move stick (-x, 0, -y)
  turned by the camera yaw, unnormalised, halved on walk, 0 when let go (FUN_1407f0f50, 1407f.c
  1270); +0x80 the right stick, +0xd0 the mouse. So a climb or fall tilts the camera, except
  while running toward it. gap: that the -z row means "toward the camera" is inferred.
- 2026-10-17 TAE 156 SetCameraAngle (template order: s32 VerticalAngle, s32 HorizontalAngle, s32
  InterpolationType, b OverrideVertical, b OverrideHorizontal). FUN_140b51490 (only on the camera's
  chr, FUN_1409f3430) -> FUN_1408327a0 fills DAT_143d5c0d0 +0x88 Vertical rad, +0x8c Horizontal rad,
  +0x90 type+1, +0x94 p = (t - start) / (end - start) <= 1, +0x99 / +0x9a the overrides. ChrCam
  (14073.c 10606-10855): while +0x90 != 0 (FUN_140831ea0) the lock-on turning is skipped; at the
  first frame (+0x98) the start angles are read from focus - fulcrum (+0x1f0 pitch, +0x1f4 yaw);
  w = 1 - (p - 1)^2; pitch +0x170 = lerp(start, Vertical, w) (absolute); yaw +0x174 = lerp(start,
  Horizontal + chr yaw, w), wrapped by the type (0 shortest, 1 up, 2 down); then +0x150 = 1. Ours:
  camera.rs set_camera_angle; the map mirrors x, so target yaw = Wolf's yaw - Horizontal. 211
  events on Wolf's anims (184 with OverrideVertical, 116 with OverrideHorizontal). The fulcrum yaw
  resync now also runs under TAE 151. Gyoubu (a225_510000) checked against the reference clip; the
  clip's final deathblow is a225_510090 (Horizontal 50 / 90 / -30), not traced here.

## Follow camera checked against the exe frame by frame (2026-11-03)
- tools/emu-cases `camera_frames` runs FUN_140734790 (-> FUN_14073c260) on a memory snapshot of the game
  (General Naomori's arena, Wolf placed by `emu-cases scene`) for 17 scenarios (still, stick yaw / pitch /
  diagonals / left, mouse, Wolf running sideways, toward the camera, climbing, stopping, a turn then a run,
  30 fps), 1,650 frames. Sim test `follow_camera_matches_the_exe` (part11) checks the helpers on each frame
  from the exe's state before it (docs/kb/emulator-pilot.md pilot 3b).
- Focus follow: FUN_141155d20 is not "close rate of the gap per 1/30 s step" (the 2026-10-07 note above)
  but a velocity chase: camera +0x1b0 velocity (m per 1/30 s), +0x1c0 last target, +0x1d0 output, +0x410
  clock (FUN_141155ec0 adds dt, wraps at 1/30). At each step boundary the velocity becomes
  rate * (gap at the boundary + the target's own move over a step); between boundaries the focus moves at
  that velocity. The target is Wolf's position (+0x90; +0xc0 = it + focus height +0x204). Ported as
  `FocusChase`, matched within 1e-5 m. Old blend: half the speed at a run's start and no spring lag.
- Fulcrum: the sideways move is taken across last frame's camera right axis (+0x10 of the final matrix),
  not this frame's angles; the fulcrum distance is (1 - k) * |+0x40 (last final camera) - focus|.
- Fast turn (+0x160 not 0): yaw turns at the full fast rate with the stick's side only (not its size); the
  pitch goes to stick tilt / sin(directPitchAng 60 deg) * the pitch limit on that side, at directPitchRate
  0.06 per 1/30 s (`direct_pitch`; fitted within 3e-6 rad on 600 frames, the code is not traced). A full
  sideways stick levels the camera.
- Matched as they were: stick rates (+0x250 pitch / +0x254 yaw normal, +0x260 / +0x264 fast; lerp of the
  CameraParam min / max by the speed option squared), the fast ramp (dt / rotHiSpeedRateIncTime 0.6),
  the mouse (degrees, minus), the yaw / pitch read back from the dragged fulcrum, the orbit point, the
  position chase (+0x100), the safe margin (+0x40), the control timers +0x150 / +0x154 / +0x194.
- Exe stick layout: Wolf's manipulator +0x80 = (pitch, yaw), + pitch raises the camera; the exe's yaw
  points from the camera to the focus (ours = exe - pi). Which pad axis feeds each is not checked.
- Not covered: the wall / ground handling (when the cast hits, the orbit point itself is pulled in: seen
  at a wall about 10 m along +X and when the stick pitched the camera to the ground), lock-on, TAE camera
  events, other frame rates.

## Follow camera at walls and the ground, checked against the exe (2026-11-03, camwall)
- tools/emu-cases `camwall` (src/camwall.rs) runs FUN_140734790 -> FUN_14073c260 on the Naomori-arena
  snapshot with every cast of the update hooked (at entry its arguments, at its return site al and the
  fraction): 10 scenarios, 2,345 frames in `extracted/emu/camwall_frames.json` (the stick pitching the
  camera to the ground at 60 and 30 fps, down while turning, Wolf walking toward the wall about 9 m along
  +X, the stick swinging the arm through that wall, wall and ground together, Wolf squeezing the arm
  against the wall and walking back, the escape-wall option on). `camwall_probe` maps the walls around
  the spot with the same cast. Sim test `follow_camera_at_walls_matches_the_exe` (src/sim_tests/
  emu_camwall.rs) replays each frame from the exe's state before it with the exe's cast results.
- The casts: FUN_14094dfc0 while ChrExFollowCam +0x3ad is set (1 in the snapshot), else FUN_14094dc70,
  same arguments (physics world [[0x143d6d640]+0x98], filter [WorldChrMan +0x3550] = 0x30, &start,
  &vector, radius, out point, out normal, &fraction of the vector): Havok linear sphere casts. In order:
  1. fulcrum (14073.c 10993-11062): focus +0xc0 -> the placed fulcrum, camCastSphereRadius 0.05; a hit
     puts the fulcrum at max(fraction * length, 0.01) (Game.IsUseSPChrExFollowCam on);
  2. orbit point (11066-11251): +0xf0 = focus + the fulcrum's direction * +0x214, cast, cut to
     max(fraction * length, 1e-5);
  3. obstacle fade (11253-11320, FUN_14094ea80, group 0x76): from last frame's +0x100 toward the
     midpoint of +0x90 / +0xa0, shortened by camCastSphereRadiusForFade 0.3 (matched within 7e-6 m);
  4. final (12318-12411): focus -> the chased +0x100, cut the same way; any hit (also fraction 1.0 when
     the camera rests on the cut point) sets +0x1e4 = targetChaseDistRateTime (2 s). +0x110 keeps the
     result; the safe margin makes +0x40.
  The position chase measures the distance it starts from at +0x110 (not +0x100) and chases it at
  +0x1e8 / +0x1ec = lerp(targetChaseDistRateXZ / Y 0.4, _ForHitWall 1.0, clamp(2 * +0x1e4 / 2 s)), then
  +0x1e4 -= dt (11948-11979): after a wall the arm snaps back for a second, then eases.
- Ported (camera.rs follow_player, `cast_cut`, `hit_wall_rates`, `chase_position_from`, `WallCast`;
  RSekiro's own collision behind `map_cast`: a ray over length + radius stopped short by the radius,
  a touch within 1 mm past the end counts as the exe's fraction-1 hit). Before, only the final cast
  cut the camera: the chase ran toward the uncut orbit point and the distance chase started from the
  uncut +0x100 at 0.4; on these frames that put +0x100 up to 1.6 m off the exe's.
- Matched (from the exe's state before each frame, 2,225 frames): yaw and pitch read back from the
  cut fulcrum (not on the three first frames where Wolf is put by the wall), the fulcrum cast's
  start / end / radius and the cut fulcrum, the orbit cast and cut point, the hit-wall rates and
  clock, the position chase (60 fps frames), the final cast, +0x110 and +0x40, all within 1e-4.
- Escape wall: option byte +0xd of GameDataMan [0x143d5aac0] +0x50 is copied every frame by
  FUN_140734790 into UseEscapeWall (DAT_143d599c0); 0 in the user's save (which menu entry it is was
  not checked). On, the fulcrum and orbit casts are skipped and two wing casts (escapeWallWing*) run;
  a wing hit sets the working rates +0x220 / +0x224 / +0x228 to the _ForEscapeWall row values (0.15 /
  0.3 / 0.1) and turns the yaw by +0x334 (escapeWallRotAdd*, 0 in row 0, wings 1 cm). Run once
  (ground_pitch_down_escape: no fulcrum or orbit cast); not ported.
- Focus pull-in (9580-9640): a ray (FUN_14094d140 / cf90) from Wolf's point (+0x200 offset) toward the
  chased focus +0x90, half his radius (FUN_1409efb80, 0.4) longer; on a hit a 0.4 m sphere cast pulls
  +0x90 in toward Wolf. It hit only on a 28 m teleport here; not ported.
- Obstacle fade hits: map objects only (kind 6, objects 0x1017d and 0x101cd by the +X wall, 660
  requests). A character fades only when byte +0x1f behind its ChrIns +0x18 is 2 (FUN_1408705a0):
  0 for Wolf and all 89 enemies of the snapshot, either way the pointer is read, so ordinary enemies
  do not fade. Not ported (RSekiro draws map objects merged into the map).
- Not covered: the 1 cm fulcrum floor (smallest fulcrum fraction seen 0.44), the zero-length final
  cast, the position chase at 30 fps (exe once a frame, RSekiro per 1/60 s), walls while locked on,
  the TAE overrides of the distance rate (FUN_1408311a0) and of +0xf0 (FUN_140831190).

## Lock-on camera and the pad mapping checked against the exe (2026-11-03, lockcam)
- tools/emu-cases `lockcam` puts a target into the snapshot's LockTgtMan (DAT_143d78058; its one entry, the
  enemy ~29 m off: entry +0x0 point, +0x10..+0x3f its 3x4 matrix, +0x88 owner), hands it to the camera
  with FUN_1409c5f20(ChrCam, entry, 0) every frame (fills the lock helper at ChrCam +0x90, +0x10d0 = 1)
  and runs FUN_140734790 -> FUN_14073c260: 16 scenarios, 1,545 frames (1,485 locked): target ahead / right /
  behind-left / above / below / at 1.5 m / circling / passing over Wolf / running off / the real enemy,
  Wolf strafing, 30 fps, the stick held, the lock let go after 1 s, a lock point that jumps 0.5 m (60 and
  30 fps). Sim test `lock_camera_matches_the_exe` (src/sim_tests/emu_lockcam.rs).
- The camera's lock point +0x280 is the lock helper's +0x1020 (when DAT_143afe850 == 1): the target point
  averaged over the last 0.5 s (DAT_143afe848; CSPositionRecorder, 240 samples, newest first while the
  summed dt <= 0.5), kept in the target's local frame, so a rigidly moving target is followed exactly and a
  jump of the point eases in linearly over 0.5 s. Ported as `LockPointAverage` (was: the raw point).
- The angles (`lock_chase`): w = focus (+0xd0) - fulcrum (dragged +0xe0, FUN_140734070 / FUN_140733e50 on
  camera -> focus), P = focus + lockTgtPosRate * (lock point - focus) (+0x290), reach = sin(half) * |w|
  (half = +0x50 FOV * +0x278 lockRotXShiftRatio * 0.5), a = asin(reach / |P - focus|) (pi/2 when inside),
  pitch wanted = a + half + pitch(P - focus), clamped to rotRangeXAtLock lerped to the free limits
  (+0x230 / +0x234) by |P.y - focus.y| over rotRangeLerpBegin/EndHeight; yaw wanted = yaw(P - focus). Both
  chased by wrap(d) * rate * dt / (1/30) (lockRotChaseRateX / Y) with the play angle taken off when the
  step would land inside it. The exe wraps with x - trunc((x + pi*sign x) / 2pi) * 2pi (`wrap_pi`).
  Matched within 1e-5 rad on every locked frame.
- Sv1 before: |w| was the configured camera distance (4.5) instead of the fulcrum's (~3.8; steady pitch on a
  target 5 m ahead 0.446 vs the exe's 0.411 rad), yaw came from Wolf's position to the target's root, not
  focus -> P, a hold when the target was overhead the exe does not have, and no averaging of the lock point.
- Matched as they were: the focus follow, the fulcrum put back on the arm, the orbit point, the position
  chase at targetChaseRateAtLockXZ / Y (angle factor 1), the safe margin, +0x194 held at 1 while locked;
  after the lock is let go the free yaw is read back from the dragged fulcrum and +0x194 falls by
  dt / transChaseByControlEndReleaseTime. The stick does nothing to the angles while locked.
- Position chase, fixed: the angle factor's cosine is now rounded as the exe does it (divide by
  sqrt((x² + z²) + y²), dot (x + y) + z). With AngAtLockMin 0 (cos 1.0) the exe stops the X / Z chase on
  frames where the f32 cosine of nearly parallel vectors rounds above 1.0 (82 of 85 such frames stopped,
  1,146 frames at or below 1.0 moved); glam's normalize rounded differently and kept chasing (up to
  0.4 mm off on 81 frames). The exe applies the chase rates once a frame with no time factor; at
  30 fps it closes 0.4 a frame where `chase_position` (per 1/60 s) closed 0.64 (fixed, camera2 below).
- Pad (FUN_140960650 fills the frame at pad +0x38f8; FUN_1407f0f50 PadManipulator, emulated on made-up
  frames): frame +0x0 / +0x4 = move up / right (actions 0x4f / 0x51; 0x4d walk halves them) -> M+0xc0 =
  up * view + right * (view.z, 0, -view.x) (left-handed); +0x10 / +0x14 = lock switch (x, -y);
  +0x18 / +0x1c mouse -> M+0xd4 / +0xd0; +0x28 / +0x2c camera yaw / pitch -> M+0x84 / +0x80. The right
  stick's vector action 0x52 gives +0x28 = s10 * x (FUN_1409668f0) and +0x2c = -s11 * y (FUN_140966960),
  s = +1 when option byte 10 / 11 ([[0x143d5aac0]+0x50]) is set, -1 when cleared; the snapshot's
  options have both set. Sv1's orbit_with_stick (yaw -= x, pitch -= y, invert_y = option 11 cleared),
  stick_world, chases_pitch and the lock switch's (x, -y) all agree; test `pad_mapping_matches_the_exe`.
- gap: which physical axes action 0x52 / 0x4f / 0x51 are bound to (the key config) is not traced; taken as
  XInput RX / RY and LX / LY (+0x258 / +0x25c, +0x24c / +0x250 of the device, + right / + up). Not
  covered: walls and the ground while locked, TAE 151 / 153 / 155 while locked, target rotation (every
  scenario keeps the snapshot's), the lock-on entry / exit blend of FOV and distance (LockCamParam).

## TAE camera events checked against the exe (2026-11-03)
- tools/emu-cases `taecam` runs the camera frame (FUN_140734790 -> FUN_14073c260) on the snapshot with the camera
  singleton DAT_143d5c0d0 filled as the TAE handlers fill it (151 FUN_140b517a0, 153 FUN_140b51680, 155
  FUN_140737430 into the CameraSetParam manager DAT_143d59938 +0x3e8, 156 FUN_140b51490), then the singleton's
  per-frame clear and timers (FUN_140838b50 from +0x3ce: FUN_140833e40, +0x44 / +0x68 / +0x7c count down by dt and
  clear with their partner at 0). 17 scenarios, 2,160 frames. Sim test `tae_camera_events_match_the_exe`
  (sim_tests/emu_taecam.rs) passes; docs/kb/emulator-pilot.md pilot taecam.
- 153 (`cam_distance_step`): every rate is per frame with no dt (ported 1 - (1 - r)^(dt * 60); Sv1 used dt * 30,
  half speed at 60 fps). The used distance +0x214 is set to +0x218 each frame (14073.c 9751): no 0.1 smoothing.
  EndInterpolationSpeed (+0x80) holds only while TransitionKeyframe (+0x7c) counts down after the event, then the
  rate is lockCamParamLerpRate again (Sv1 kept it forever). Override target at StartInterpolationSpeed (param rate
  when <= 0); SlowStart eases from +0x21c taken at the first frame. Old code was up to 2.0 m off +0x214.
- 155 (`cam_slot_step`, `CamBlend`, FUN_1407374e0 / FUN_1407381d0): at a switch the blended CameraParam as it is
  (+8) is kept at +0x1e8 and blended from (Sv1 blended from the old slot's row: a switch mid-blend jumped); length
  +0x3f0 = new slot's beginTime (old slot's endTime back to 1), clock +0x3f4 restarts at 0 and advances by dt
  before the weight, clamped to the length; weight w = 1 - (p - 1)^2 (Sv1 linear, up to 0.25 off); isEnable*
  rounded, rotBaseY / resetBaseY take the new row. The LockCamParam row is the new slot's at once (Sv1 blended
  it): distance, FOV +0x50 and +0x278 chase it per frame. Focus height +0x204 = CameraParam chrOrgOffsetY +
  (LockCamParam chrOrgOffset_Y - it) * lockCamParamLerpRate, anew each frame (not a chase). All 95 named fields
  of the blended row match within 2e-6.
- 156 (`set_camera_angle`): the target +0x1e0 = HorizontalAngle + Wolf's yaw is taken once at the first frame;
  Sv1 used his current yaw (0.98 rad off when he turns during the event). Types, start angles and +0x150 = 1 /
  +0x154 = 0 matched as they were.
- 151: focus on the dummy at min(unk1, 1) (normal rates <= 0); after the event it chases Wolf at unk3 while unk2
  seconds count down (Sv1 had no after-rate). The cast from Wolf's focus to the dummy (FUN_14094cf90) sets +0x1a0
  when it hits: focus stays on Wolf, limits fall back (map::raycast here). Limits (`look_limits_step`, a literal
  FUN_140742230): centre = view along minus row 2 of the dummy's world matrix (our dummy's +Z: Wolf's dummy 142
  view matches the exe's within 0.05 rad; Sv1 used the Wolf -> dummy vector, which is straight up for 142); yaw
  unwrapped near +0x240 (camera yaw at the event's start, FLT_MAX between events); every limit chases per frame
  from where it was (Sv1 started at the target, dt * 30, wrapped deltas); the event's yaw limits only when not the
  full circle; no event: LockCamParam rotRangeMinX / CameraParam rotRangeXMax and the param yaw range; blocked
  with the full circle the event's pitch limits stay around 0. The clamp runs on the free camera after the
  resync (unlocked branch only; 156 is not clamped): pitch plain, yaw by the exe's 2pi window (`clamp_yaw_exe`;
  Sv1's centre-based clamp differed for cones across +-pi such as a213's -160..-159).
- Not covered: the cast blocked path (+0x1a0 never set in the arena), the 6 CameraParam rows with a yaw range
  under the full circle (chr row 2 taken as Wolf's facing), the CameraSetParam map row (snapshot 1100000 =
  row 0), SlowEnd (+0x85, read at 14073.c 12527, not traced), +0x278 into the lock-on pitch, 30 fps.

## Escape wall, focus pull-in, lock-on at walls, 30 fps, lock-on manipulator (2026-11-03, camera2)
- tools/emu-cases `camera2_walls` (14 scenarios, 2,730 frames checked: 1,000 with the escape-wall option, 470 at
  30 fps, 761 locked on with a cast cut) and `camera2_manip` (9 scenarios, 360 frames of Wolf's PlayerIns update
  with a lock point); tests `camera_escape_wall_pull_in_and_lock_at_walls_match_the_exe` and
  `lock_manipulator_matches_the_exe` (src/sim_tests/emu_camera2.rs).
- 30 fps, fixed: the exe takes the position chase rate once a frame with no time factor (0.4 of the remaining gap
  a frame at 1/30 as at 1/60). `per_frame` now gives the rate itself for a frame of 1/60 s or longer (was
  1 - (1 - r)^(dt*60): 0.64 at 30 fps; that was off by up to 1.35 m in these runs) and only scales shorter frames;
  the fulcrum's sideways move is taken whole a frame (`per_exe_frame`). The camwall and lockcam tests now feed
  the real dt.
- Working rates +0x220 / +0x224 / +0x228 (focus follow X / Z, Y, position chase X / Z) ease by (row - cur) * dt
  toward chrTransChaseRateXZ / Y_ForNormal and targetChaseRateXZForNormal, and are set to the _ForEscapeWall
  values on a wing hit (`working_rates_step`). The focus follow reads last frame's rates, the position chase this
  frame's. Ported as `CamState::rates`.
- Escape wall (option byte 13 of [[0x143d5aac0]+0x50], copied to DAT_143d599c0, default 0 in FUN_1407bc470;
  `input.escape_wall`): the fulcrum and orbit casts are skipped; two wing casts (sites 0x14073ffe5 / 0x140740025,
  escapeWallWingRadius 0.01, length escapeWallWingLen 0.01) start beside Wolf's point +0xa0 and point along the
  arm turned by -/+ escapeWallWingAngY (`escape_wings`, start within 5e-5 m: the exe's f32 rounding 200 m out).
  No wing hit in 1,000 frames (0.01 m from Wolf's point sits inside his capsule). The camera rotation from the
  wings (escapeWallRotAdd and others) is 0 in all 142 CameraParam rows, so not ported. gap: which menu entry
  sets the option.
- Focus pull-in (14073.c 9563-9664, `focus_pull_in`): a ray from last frame's focus to the chased one, |d| +
  half Wolf's radius long; on a hit, a sphere of his radius 0.4 from his root toward the old focus; the focus goes
  where it stops (min 1e-5 m). 8 pull-in frames (walked through the wall, put across it and back). Fixed: the next
  frame's focus follow starts in X / Z from the pulled focus, not from the chase's own output (+0x1d0 keeps the
  unpulled point that frame); before, the follow was off by up to 2.52 m on the 8 frames after. The Y is not
  exercised (all pulls level).
- Lock-on at walls matched as it was: the lock point average, the lock angles from the dragged fulcrum, the
  fulcrum / orbit / final casts and cuts, the hit-wall distance rates, the chase at the AtLock rates, at 1/60
  and 1/30.
- Obstacle fade, gap (not ported): FUN_1408705a0 fades map objects only; a character needs byte +0x1f of
  [[chr+0x18]] to be 2, and it is 0 for all 89 characters in the snapshot (the backing object is not known).
  RSekiro draws map objects merged into the map, so there is nothing to fade; camCastSphereRadiusForFade unused.
- Lock-on manipulator (FUN_1407f1710, from FUN_1407f0a70 when the lock module's bag +0x108 -> FUN_140b2dab0 gives
  the lock point +0x38): no 45 deg gate; the move is the raw stick turned by the camera yaw (M+0x94) that frame;
  M+0x40 = pitch and yaw (atan2(dx, dz) + pi) to the lock point, M+0x20 = their wrapped difference from Wolf's
  angles, the yaw request +0x24. Standing, the physics yaw takes the request whole the next frame (no TAE 224
  rate), the drawn yaw turns at 0.2304 rad a frame; walking locked the same. Fixed: `Turn::gate` takes the stick
  as it is while locked (a press from rest moved a frame late, a 90 deg flick counted as a let-go), and a locked
  Wolf standing still now turns to the target (before he never did). gap: RSekiro aims at the target's root, the
  exe at the lock point; MoveAngle +0xbb0 is in the chr matrix frame (stale in the run), not checked.
