# Recording research: animation layers, final pose, ragdoll (exe 1.06, static RE, 2026-10-21)

CI = ChrIns, bag = [CI+0x1FF8], T = [bag+0x10] TimeAct, B = [bag+0x28] SprjChrBehaviorModule, C = [CI+0x50] ChrCtrl.
Static only (decomp + vtable reads); not checked live.

## A. Animations playing at once
- TimeAct T (0x108, ctor FUN_140b5bd20): T+0x10 anim resource R (R+0x68 count, [R+0x70] entries stride 0x48:
  +0x30 anim id, [[+0]+0x20]+0x1C duration s). Ring of 10 {id, srt, end, duration, start tick} at T+0x20..0xE8
  (write T+0xE8, read T+0xEC): only the TimeActs started in the last ~2 frames, not what plays now.
  T+0x100 = id of the last SetAnim (FUN_140b5c0f0), hence face anims interleave there.
- Active hkb clips (the real blend layers): W = [B+0x10], H = [W+0x28] hkbCharacter, G = [H+0x98] hkbBehaviorGraph,
  AN = [G+0xE0] hkArray<hkbNodeInfo*> ([AN] data, i32 [AN+8] count). nodeInfo+8 node, nodeInfo+0x3E bit 1 active;
  node+0x53 == 4 = clip generator (dev debug display 14100.c:10730). Clip: +0x98 name (hkStringPtr, low bit tag),
  +0xC6 i16 binding index (anim id = entry idx of R), +0x140 f32 localTime s, +0x144 accumulated, +0xC0 user time
  fraction, +0xC4 mode, +0x150 atEnd, +0x152 pingPong, +0x100..0x12F extracted motion (root motion) of this update.
  Control [clip+0xD8] hkaDefaultAnimationControl: +0x18 localTime, +0x1C masterWeight, +0x64 playbackSpeed.
- Open: per-layer blend weights (hkbBlenderGenerator children), character-level root motion.
- ChrCtrl C+0x18 holds a 1-slot hkaAnimatedSkeleton player; not the main path.

## B. Final pose
- I = [B+0x28] SprjFD4LocationHkaPoseImporter; hkaPose at I+0x48: +0x48 skeleton ([+0x38] bone count, [+0x20] parents
  i16), +0x50 localPose data, +0x58 count, +0x60 modelPose (may be stale: lazy), +0x70 bone flags (bit 2 model stale).
  hkQsTransform stride 0x30: translation vec4, rotation xyzw, scale vec4. c0000: 146 bones.
- Filled each frame by FUN_140b44980 -> FUN_140ffe330 when B+0xBC1 != 0 (ragdoll clears it).
- Draw matrices exporter SprjFD4LocationGxModelMatricesAndAabbExporter: not traced from CI.

## C. Ragdoll
- [C+0x28] FrpgRagdollIns: state +0xEC (FUN_140953cf0). Requested C+0x1C8 (-1 none), applied C+0x1CC, timer +0x1D4.
  Trigger TAE 228 RagdollReviveTime (FUN_140b53e90 -> FUN_1407de950); per-frame machine FUN_1407e2d10.
