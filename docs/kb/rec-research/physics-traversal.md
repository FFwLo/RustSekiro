# Recording research: physics, traversal, grapple, fall damage (exe 1.06, static RE, 2026-10-21)

Static read of extracted/decomp; nothing checked live except "known". Files: 140b3.c, 140b9.c, 140bc.c, 140bb.c,
140b7.c, 140ba.c, 140ef.c, 14084.c, 1409e.c, 1409f.c. Env names: reference/SekiroHKS/c0000_define.hks.
Module bag slots traced here (bag = [ChrIns+0x1FF8]): +0x68 Physics, +0x70 Fall, +0x78 Ladder, +0x90 HitStop,
+0x98 Damage, +0xA0 Material, +0xA8 KnockBack, +0xB0 Sfx, +0xB8 hit-wall/action ctrl (class not named),
+0x128 Docking, +0x130 CSWireActionModule.

## A. Physics module P = bag+0x68 (SprjChrPhysicsModule)
- Known: position +0x80/84/88 (+0x8C w), yaw +0x74. Real Y adds float +0xC08 when u16 +0x940 == u32 +0xC04 (FUN_140bbfc00).
- Velocity vx/vy/vz +0x160/164/168 (env 3002 = horizontal speed x1000, env 3003 = vy x1000). High.
- Floor contact this frame byte +0xA2, previous +0xA3 (a change fires the landing world event). Env 201 IsLanding =
  +0xA2 and the velocity-change object idle. +0x929 floor within reach, +0x92A forced grounded, +0x92D no gravity
  (SekiroTool). Env 248 IsTrulyLanding ~ +0x929 and not +0xA48 (medium).
- Velocity change (TAE 920) object embedded at P+0x1C0: int P+0x228 active row (-1 none).
- Fall: vec4 P+0x850 = last grounded position (copied every grounded update), fall height = P+0x854 - Y
  (FUN_140b97880; env 224 GetFallHeight = that x100). Measured from the last grounded Y, NOT the jump apex
  (hks-graph.md says apex: check). Clamp 59 m when FUN_140a58a70; 10000 m when [[ChrIns+0x50]+0x50]+0x9C bit 1
  (map hit event 0xE). +0xBF4/+0xBF8 running min of (groundY - Y). +0x8D0 air time s. Timers +0x8E0 (0.5) and
  +0x8F8 (10): velocity zeroed, byte +0x8E8 set. FUN_140bc2210 = fell >= 1500 m or air >= 30 s.
- Floor material: u16 P+0x940 (ground-contact struct, previous frame at +0x9C0) is what the fall code looks up.
  Footsteps read Material module M = bag+0xA0: u16 M+0x10 floor material, M+0x12 / +0x16 per foot; Sfx module
  [bag+0xB0]+0x2C overrides when >= 0. Writer of M+0x10 not found: record both.
- Slope: vec3 P+0xA50 slide vector (env 317 IsSlope = nonzero), +0x924 likely slope limit (deg). Ground normal as a
  plain vec3 not found; floor probe FUN_140bce730 writes +0xCB0..0xCF0 and +0x150 (+0x154 = -distance to floor).
- Wall: byte +0xA84 and int +0xAB0 (env 2012 IsHitWall = +0xA84 and +0xAB0 == 2); +0xA90/+0xA98 wall direction
  (env 3007); +0xAB4/+0xAB5 env 3020 IsWallJumpContact; +0xAC0 env 3030 IsFallPreventAssist.

## B. Ledge hang, wall hug, swim/dive, ladder
- Docking module D = bag+0x128 (CSChrDockingModule). Int D+0x10 active kind: -1 none, 0/2 hang, 1 wall hug.
  D+0x20 hang EdgeSelector (slots at D+0x80 + 0x40*i, -1 none); D+0x290 WallSelector (state D+0x2D0, handle D+0x450);
  D+0x4F0 PointSelector, D+0x570 SwimHitSelector, D+0x5F0 WireMoveStartPointSelector, D+0x6D0 HangClimbPointSelector.
  D+0x7B8 12 entries x 12 bytes, flag at D+0x7C0+12*i: 0 IsDiveAllowed (env 3038), 1 IsDiveToSwimAllowed (3039),
  11 IsHangAndClimbPossible (3050). D+0x84C/84D env 3051/3052.
  FUN_140b7d290 = one code: 1 hug, 0/2 hang, 3 point, 4 swim/dive, 5 other, 6 D+0x844, -1 none.
- Swim: no stored "is swimming" flag found; use the anim (a000_52xxxx). The SekiroHKS name IsSwimContact = 3025 does
  not match the exe (3025 reads the action module's not-locked-on byte +0x2D1 + a floor check).
- Ladder: s8 at [[ChrIns+0x50]+0x50]+0x258, -1 = not on a ladder (env 267, env 283 IsOnLadder); +0x51C hand state
  (env 293); +0x524 bit 0 descending to floor (env 296). Setter FUN_1407de280. Module bag+0x78 not decoded.

## C. Grappling hook
- Target singleton: qword at 0x143D8E058 = M. M+0x08 u8 target found, M+0x10 vec4 hook point, M+0x28 u64 handle
  (-1 none). Fired copy M+0x50 vec4 / M+0x68 handle (mirror of +0x10..+0x48), made by FUN_140efaf40
  (rcx manager, dl is-player; once per shot = hook spot). Point object +0x70 live position, +0xC WireVariationParam id
  (registry 0x143D8DFE0, FUN_140ef4830).
- Start: HKS act 3018 WireAction (case 0xBCA in FUN_140b32410): commit target, flags 0x200000/0x600000, builds a
  CSChrPhysicsHomingWireTarget (type 4) on the physics homing list P+0x248.
- Env 3000 IsWirePossible = FUN_1409f3030: wire module W = bag+0x130, needs M+0x8 == 1; W+0x19 enabled.
- Flight: FUN_140baeea0 applies ChrPhysicsVelocityChangeParam row 1101 (ground start) or 1102 (air).

## D. Fall damage and fall death
- Landing FUN_140b97b30(fall module bag+0x70): h = fall height; fraction FUN_140843660; lethal test FUN_14084e5d0
  (h, floor material, SpEffects); damage = max HP x fraction.
- No damage under 20 m (0x143b01948); with SpEffect 341 the limit is 5 m. Lethal when h >= 20 and
  h >= HitMtrlParam +0x4C fallDeathDist (40 m; 80 m on rows 22/36/38). Fraction = CalcCorrectGraph 210(h) x 0.01 x
  curves 211/212 x HitMtrl +0x50 fallDamageRate (1.0, 0.5 deep water). 0 with SpEffect 47 or HitMtrl +0xC bit 6.
- Lethal fall for the player: if HP > 0.4 x max (0x143b01958) then HP -= 0.4 x max, reaction 0xF
  DAMAGE_TYPE_LAND_DEAD_RETURN at bag[0]+0x5C, fall module +0x25 set; else death (16).
- Void fall FUN_140b975f0(fall module, 1) from map hit event 0xF (FUN_1409edf20): 0xE FALL_DEAD_RETURN (HP -= 0.4 x
  max) or 6 DEATH_FALLING; forced death on FUN_140bc2210 or P+0x8E8.

## Open
Ground normal vec3; whether M+0x10 == P+0x940 and who writes M+0x10; a real swimming flag; docking entries 2-10;
fall curves 211/212 factors; which map object emits event 0xF. Recording P raw +0x0..0xD90, D and M settles them.
