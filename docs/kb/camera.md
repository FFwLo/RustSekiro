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
- 2026-10-10 lock-on target switch (static, LockTgtManImp): update FUN_1409c5fe0 (decomp/1409c.c:11171-11185) calls
  the stick switch FUN_1409ca2d0 (+0x29c0 flick mode off by default; FUN_1409c9d40 flick variant is debug only) and the
  mouse switch FUN_1409ca120 every frame. Stick: |axis| < 0.5 (@0x143289124) re-arms, > 0.95 (@0x14328918c) fires.
  Mouse: delta * -1 (@0x143289434), < 25 (@0x1432892d0) re-arms, > 50 (@0x1432892f0) fires. Each switch sets both
  cooldowns (+0x2844 / +0x2848) to 0.5 s. Pick FUN_1409ca480 (1409c.c:6345): screen-space d = cand - cur, score =
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
  frames, set 14096.c:3750), filled by FUN_140960650 (14096.c:3190-3192): +0x20 = FUN_140965bd0 = analog action 0x54
  * s, +0x24 = FUN_140965c80 = action 0x55 * s, s = +1 if option byte DAT_143d5a988 +0x715 / +0x716 else -1 (likely
  camera invert X / Y, not verified); FUN_1409ca120's -1 cancels the default, so the switch sees the raw axis.
  Analog read FUN_1425e8150 (1425e.c:6483, up to 4 bindings, type 2 = device float). The only DirectInput float
  source: MouseDevice poll FUN_141a8f540 (141a8.c:10829), GetDeviceState DIMOUSESTATE, +0x1b0 / +0x1b4 = (float)lX /
  lY, relative axis mode, no scale or accumulation: raw counts per poll, y down. So 50 / 25 are counts a frame,
  the same unit as Bevy's AccumulatedMouseMotion (camera.rs unchanged). Open: the 0x54 -> mouse binding table.
  Screen d in FUN_1409ca480 is NDC (1409c.c:6512-6532, view then projection matrix from camera +0x10..+0x4c,
  divided by clip w, no viewport transform; +0x29c7 remaps d via FUN_1402b8e90, not read): y up, so in.y > 0
  favours lower candidates; mouse down (lY > 0) picks lower, matching camera.rs. Open: the binding's lY sign;
  the stick (FUN_1409ca2d0 reads +0x10 / +0x14 unnegated, 1409c.c:4870) would pick lower on +y.
- 2026-10-13 lockCamAdjustRot_MaxX / MaxY / ChaseRate (row +0x128..+0x130) are unused: FUN_1407374e0 copies /
  blends whole LockCamParam rows (getter FUN_140737d80, 14073.c:3459) via FUN_1407381d0 (14073.c:5223-5225 lerp
  them like every field) into DAT_143d59938 +8; its readers (FUN_140741e70 callers 14073.c:5426 / 7434 / 8919)
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
