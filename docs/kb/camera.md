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
  apply_look_limits. Gaps: cone centre from the dummy's basis, no lerp back to the default cone after the event.
