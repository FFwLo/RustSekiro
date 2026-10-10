# damage: Damage reactions, knockback, breaks, invulnerability, death and revival

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: hit reactions by direction + level. Enemy: Damage{Small,Middle,Large}_{Front,Back,Left,Right} from the
  attacker's side (HKS GetDirOfPlayableDamage). Player (DAMAGE_LEVEL_*): 1/2 Small/Middle x 4 dirs, 3 Large F/B,
  4 ExLargeBlow, 5-6 LargeBlow, 7 SmallBlow, 9 LargeUpper; blows launch via their TAE 920 (rows 3040/3090/3100),
  loop *FallLoop and land in the matching *Land anim. Test: heavy thrust blows the player away and lands.
- 2026-10-07 WOLF: posture break flow from HKS (DAMAGE_TYPE_DAMAGEBREAK / GUARDBREAK, c0000_transition.lua ~1614).
  Guard emptied -> StandDeflectBreak (a050_190100). Hit emptied -> by damage level: EXLARGE BreakLargeBlowStart,
  EX_BLAST/BREATH BreakExLargeBlow (no clip -> LargeBlow), SMALL_BLOW BreakSmallBlow, UPPER BreakLargeUpper, FLING
  BreakLargePound, else StandDamageBreak_F/B (a000_19000x, 3 s). Any hit while in StandDamageBreak/StandDeflectBreak
  -> StandDamageBreakDamage (a000_191000) -> StandDamageBreakDown_FaceDown/Up (a000_19001x) ->
  StandDamageLargeDownWakeUp (graph transitions; face up only after a front small blow). 25 break states exported
  (names = behavior CMSG names minus _CMSG). Test posture_break_staggers_then_a_second_hit_knocks_wolf_down. 38 pass.
- 2026-10-07 WOLF: player's KnockBackParam row found in data, not the exe: EquipParamProtector knockbackParamId = 1 on
  every outfit piece (38 rows; NPCs all use 0 via NpcParam). Row 1 holds pushes longer (damage ContTime 0.09 s vs
  0.01-0.05, guard_S 0.06/0.30). Protector knockBackCutRate_* and weapon knockBackCutRate_*_Guard are 0 for Wolf.
  (Exe side: KnockBackParam is param index 0x22 in the name table at 0x143b16380.)
- 2026-10-07 NEXT 17 done: TAE 226 SetKnockbackPercent (dispatcher case 0xe2 -> FUN_140ba34a0: knockback module +0x40
  = u8 percent * 0.01) scales the knockback the anim's OWN character receives: FUN_140ba37e0 (knockback start) sets
  push = distance * (+0x40). Wolf: Combo1 0 % at frames 23-25 (not pushed when blocked at his hit frame), Combo2 50 %,
  Combo3-5 30 %. Applied in combat.rs apply_knockback. (Also seen: leftover push distance is handed to the other
  character when the knocked one is blocked, FUN_140ba34b0 - not modelled.)
- 2026-10-07 WOLF: damage reactions re-checked against HKS normal damage (c0000_transition.lua ~1162). Fixed: NONE (0)
  and MINIMUM (8) play no reaction (were Small); PUSH (5) = Middle 4-way (was LargeBlow); EXLARGE (4) = LargeBlow (was
  ExLargeBlow first); EX_BLAST (10) = ExLargeBlow, BREATH (11) = SpecialLargeBlow (both no clip -> LargeBlow; were
  Large); FLING (6) = StandDamageLargePound (exported now, a000_100500). Knockdowns chain like the graph: BlowLand /
  UpperLand / LargePound / SmallBlow -> StandDamageLargeDown (get-up a000_10031x), prone per _setProneDir (small blow
  from the front lands face up).
- 2026-10-07 enemy damage reactions per c9997.lua ExecDamage*: PUSH -> DamagePushFront/Back, SMALL_BLOW / BREATH ->
  DamageBlowFront/Back, BLOW (4) / EX_BLAST -> DamageLargeBlow (4 s), LARGE / FLING / UPPER -> DamageLarge_<dir>,
  MINIMUM / NONE -> none (were folded into Small/Middle/Large). Player guard sizes re-checked: already match HKS
  (SMALL/MIDDLE/PUSH S, LARGE/SMALL_BLOW M, EXLARGE/FLING/UPPER L, MINIMUM XS, EX_BLAST/BREATH XL).
- 2026-10-07 WOLF: resurrection. GroundDeathStart_F -> GroundDeathLoop_F, then LMB resurrects while a node is left
  (config player.resurrections = 1, the game's start): GroundRevival_F (a000_110030), whose TAE applies SpEffect
  110015 "Resurrection Technique_HP Half Recovery" (changeHpRate -50 -> +50 % max HP). No node: the old auto reset.
  Exporter now carries changeHpRate / changeHpPoint / changeHpEstusFlaskRate and SpEffect 3000 (gourd -40, was a
  fallback constant). Test resurrection_returns_wolf_at_half_hp. 42 pass. Also: TAE 155 SetLockParamID (8 / 11 / 41)
  indexes CameraSetParam lockParamIdN, which is 0 (LockCamParam row 0) for all three in the default set: no-op.
- 2026-10-07 invulnerability windows: TAE 950 (dispatcher 0x3b6 -> action module +0x70 |= 2) sits on all 55 player
  knock-down / launch / death / revival anims (frames 0-18..33, death 0-103) and 951 (|= 1) on the deathblow throws
  a201_*: now full i-frames in combat.rs dodged() for both sides (no juggling a downed Wolf, invulnerable while
  resurrecting frames 0-27 and during deathblows).
- 2026-10-07 gap 7 closed: Wolf's per-enemy anims ship in chr/c0000_cXXXX.anibnd (dictionary: c1020, c1040, c1050,
  c1070, ...). c0000_c1020 holds a210_600000 (being grabbed and thrown by the General, 170 f), a210_600001,
  a210_610001, a210_511800 (no TAE for the last three). The neck grab now plays it (combat.rs grab; resolve_throws
  applies the ThrowAttackBehavior damage without replacing the clip, back to StandIdle when it ends).
  TAE mini header type 0 (Standard): +0x18 flags, +0x1c ImportHKXSourceAnimID (-1 = own clip); tae.rs exports it
  as hkxFrom and export.rs uses that clip when the anim has none of its own (no Wolf anim uses it today).
  Behind-the-broken-enemy deathblow rows (ThrowParam 11020110 start: attacker within 0-90 deg of the defender's back;
  11020111 body atk a201_511200 / def 13200, isTurnAtker 1) have NO clip in any unpacked anibnd -> not used.
  Plunge rows 11020150/151 (broken + falling, dist 20/10, fall-orbit check) exported, not wired yet.
- 2026-10-08 LIVE (tools/rec_hits.py; data module +0x130 HP, +0x148 posture REMAINING (counts down), +0x14C max):
  this save's Wolf 1120 HP / 420 posture. Deflect/guard costs seen: -50, -53, -58, -66, -67, -75, -83 posture
  (no HP); body hits -120..-480 HP with -135..-202 posture; posture 0 -> break. Posture regen while moving
  ~85-110 points/s (walk/run anims), ~320/s in a000_000000. Enemy attack ids (slot 1 anim) are logged for
  matching against AtkParam later (gap: map enemy chr + attack -> AtkParam to compare our numbers).
- 2026-10-08 LIVE per-attack match (tools/rec_attacks.py <rec> c1180; NPC behaviour id = 200,000,000 +
  behaviorVariationId x 1000 + judge; TAE anim ids carry the group, e.g. 400003017; ChrIns +0x68 = NpcParam id).
  Enemy scaling k = 3.75 at this save (c1180 NpcParam HP 1916 -> 4050 live). Wolf, unguarded: HP = atkPhys x k
  (96->360, 112->420, 128->480, 32->120, 48->180, all exact); posture = directAtkStamDamage x k (48->180,
  54->202, 36->135) = our model. Guarded: deflect (StandDeflectHard*) = atkStam x k x 0.74 (27 -> 75 twice; 21 ->
  58), block (StandDeflectEasy*) = repelLostStamDamage x k x 0.74 (60 -> 167). The x0.74 on guarding is not in
  our model and not in Wolf's weapon rows (only iron fan 76x00 / wire 9500000 have staminaGuardDef) -> likely a
  progression-scaled guard cut (this save: 1120 HP / 420 posture). gap: find its source (CalcCorrectGraph or
  PlayerGameData stat) before applying; at level 1 it may be 1.0.
  Source found (static): defender posture FUN_1408439a0 -> PlayerIns vtable +0x200 = FUN_140a24180 -> FUN_140840870:
  cut% = clamp((FUN_140845d80(weaponRow, 0xa3, stat @ PlayerIns+0x260, correctType @ row+0xee) + staminaGuardDef
  (row+0xd4, just-guard row+0x10a) x reinforce rate(+0x4c) + 1) x (1 + row2+0x6c / 100) x rate, 0, 100).
  The progression term (correction graph of the player stat at +0x260) gives ~25 here -> 26% cut -> x0.74.
  Next: identify the stat at +0x260 and graph 0xa3's mapping, compute it at level 1, then apply in combat.rs.
- 2026-10-08 LIVE Wolf -> enemy (tools/rec_wolf_attacks.py; c1180 samurai, this save's attack power): slash judges
  10/11/12/50/51 (atkStam 15) unguarded: -121 HP / -65 posture; some hits -348 / -130 (x2.9 HP, x2 posture; enemy
  reaction a000_009503 -> counter-hit / state bonus? gap). Deflecting their attacks: attacker -163 posture each
  (once -57 inside the spam chain). Shield samurai 11807400 blocking: -7 to -9 posture. Art 106 (316000 judge
  200): -522 or -870 HP / -324 posture; its release 316110 judge 211: -102/-194 HP / -106. Deathblows show as
  -4897 / -5577 HP (a000_790050 on Wolf). Ratios to compare at level 1 once the attack-power graph is known.
- 2026-10-08 LIVE #2 c1470 (k = 3.75 again): hit 80 phys -> 300 HP, direct 18 -> 67 posture (exact); deflect stam 18
  -> 50 = 18 x 3.75 x 0.74 (guard factor confirmed on a 2nd enemy). Hit while Wolf attacks: normal (no counter
  bonus on Wolf). Odd ones: during GroundStep_F x1.2 HP / x1.33 posture; GroundStep_N x0.73 HP; during his own
  StandDamage reactions x0.6-0.83 (no damage-rate SpEffects in those anims; gap: hit direction / repeat-hit cut?).
  Wolf->enemy x2.9 hits: not from NpcParam (rows identical but HP); temporary effect (gap).
- 2026-10-08 the "odd" live hits are explained, no hidden mechanic: -400 / -220 = HP capped at death (Wolf at 0);
  -360 / -89 = normal 300 / 67 + a second contact 60 / 22 in the same tick; -180 / -45 = a smaller attack
  (atkPhys 48 / atkStam 12 equivalent) seen in many states - rec_attacks.py took the first of two active attack
  events (tool ambiguity). Step / hit-reaction damage is NOT scaled. Remaining real gap: guard x0.74 (hook
  script tools/memscope/posture_hook.lua ready: run while deflecting).
- 2026-10-08 scalings explained from data: enemy k = 3.75 = area SpEffect 7124 / 7820 "Growth Doping: Samurai
  Residence (Truth)" (physicsAttackPowerRate 3.75, staminaAttackRate 3.75, maxHpRate 4.25/4.4). Wolf's attack x4.35
  = "Player Growth_Core Level N" SpEffect 160000+N (Attack Power; level 15 = 4.35: physics/stamina/
  attackHitParryStamina/defStamina rates all equal, stateInfo 204; L0-1 = 1.0, +0.25 per level to L10 = 3.25).
  Guard x0.74: one live call of FUN_140840870 (before the 2nd crash) = rdx BehaviorParam_PC 105000090 "Right
  hand sword_guard" -> AtkParam_Pc 5000090 (guardStaminaCutRate +0x6c = 0); weapon term EquipParamWeapon
  staminaGuardDef (+0xd4) x ReinforceParamWeapon staminaGuardDefRate (+0x4c) = 0 for the katana; graph 163 = 0.
  So the rate is param_4 (xmm3) = FUN_140bfcde0(Wolf SpEffects, stateInfo 0x9e=158 / 0xcc=204) at 0x140a24276
  (asm via tools/ghidra.ps1 asm:140a24180). Next: decompile FUN_140bfcde0 (which field it collects; candidates
  among the 158/204 SpEffects: 105010, 105020-23, growth core 1600xx). Inline hooks crash Sekiro: static only.
  FUN_140bfcde0 = product of SpEffect.guardStaminaCutRate (+0x13c) over the defender's active SpEffects with
  stateInfo (+0x15e) == 158 / 204 that pass FUN_140c03aa0. All of Wolf's candidates (105010, 105020-23, growth
  core 1600xx) have 1.0 (only shield 1040/1050, test 109035 / 900001 differ) -> param_4 = 1, and the traced
  terms give a ~1% cut, not 26%: the output path of FUN_140840870 (void in the decompile; caller uses xmm0) and
  FUN_140a24180's final x (PlayerIns+0x21ec = 200 live) / 100 when module(+0x1ff8.. [0x3ff]+0xc0)+0x10 is set
  are not understood yet. Status: x0.74 measured twice (c1180, c1470), source open; NOT applied in combat.rs.
- 2026-10-08 LIVE #4 Samurai General (rec_20261008_054259; slot 1 = lock-on target NpcParam 10203010 "Samurai
  General Honjo", c1020 behaviour 10200; area k = 3.75): 60+ matched hits. Unguarded exact (80->300, 128->480,
  112->420 HP; 36->135, 18->67, 54->202 posture). GUARD FACTOR DEPENDS ON THE ATTACK'S staminaPhysicsAttribute:
  attribute 1 (slash, all c1020 rows) x0.785 on deflect (atkStam) and block (repelLostStamDamage); attribute 2
  (c1180 / c1470 rows) x0.74. Code path: FUN_1408439a0 x vtable+0x248 (PlayerIns FUN_140a250c0) ->
  FUN_140a24f30 fills 5 equip slots (ids via FUN_1409cc120(chr[0x428], i)) + weights (FUN_140841bd0: durability
  ratio -> 1.0 / 0.7 (<=0.3) / 0.5 (0)) -> FUN_1408447b0: product over 4 of (1 - (1 - rate) x weight), rate =
  row +0x194 + (attr-1)*4 (layout = EquipParamProtector slash / lightHit / thrust / neutral / ninsatu / heavyHit
  StaminaDmgRate). Wolf's protector rows 100000-103000 all have 1.0, and no param row holds 0.785 + 0.74, so
  the table read by FUN_1410ca960 is not identified yet. Not applied in combat.rs (measured: slash 0.785,
  strike 0.74).

## Flowing Water and hit sounds (2026-10-08)

- The guard posture factor (0.785 slash / 0.74 strike) is latent skill **Flowing Water**
  (SkillParam 280 -> SpEffect 150420 deflect / 150421 guard, def<Attr>StaminaDmgRate by AtkParam
  staminaPhysicsAttribute). Implemented (combat.rs guard_skill_rate, config player.skills).
- 2026-10-09 LIVE (memscope-data/logs/rec_c1010_*, rec_c1020_*_20261009; chr id at chr_head +0x6C, NpcParam
  row at +0x68): deathblows checked against both enemies and fixed:
  * Kill follow-up (HKS BEH_R_THROW_KILL -> W_ThrowKill<id+1>): when the enemy switches ThrowDef ->
    ThrowDefDeath, Wolf goes on with anim id + 1 if it exists (only the a201 set: 510001, 510111, 510201,
    511201, 511411, 511511). General front: 510000 1.85 s -> 510001 on the frame of 12000 -> 12001.
  * Deflect break = a pose pair, not the deathblow: the breaking ground deflect plays Wolf a20x_510100 /
    enemy ThrowDef12100 (ThrowParam 0010). While the enemy anim has flag 67 ThrowStart2 (frames 0-45),
    attack -> 0011 (510110 / 12110, kill 510111), jump -> 0012 (510120). Live presses 0.47-0.79 s, before
    Wolf's own cancel flags (frame 30). An AIR deflect break keeps AttackBoundEmptyStamina.
  * Mikiri that empties posture (c1010, 3 of 3): 見切り崩し 0120 (511100 / ThrowDef13100) pose, attack ->
    0121 (511110). The General's mikiri stays 0190 (511800).
  * c1010 front: 崩し始動（近）0005 (Dist 1.2) -> 0006 (a200_502500 -> 512500, ThrowDef14500) when that close
    (3 of 4 live), else 0000 -> 0001. Reach = the start row's Dist.
  * Air attack out of a head-kick jump on a broken enemy: 蹴り崩し 0160/0161 (511500 -> 511510 [-> 511511],
    ThrowDef13500 -> 13510), checked from the kick jump's buffer flag 87 (frame 9; live 0.33 s).
  * 崩し蹴りジャンプ 0200/0201 (a200 set for both): jump in front of a broken enemy vaults over him
    (501900 0.30 s -> 511900, ThrowDef13900, still deathblow-able).
  * Head-kick jump variant by HKS _set4DirJumpDir: locked -> _F_Lock (213115), unlocked no stick -> _N
    (213114), else F/L/R/B.
  * Landing mid air deflect (ref 201 up) continues in LandAirDeflect* (air anim id + 10) from the same time.
  * Not modelled: stealth deathblows (0020/0021, 0030/0031) need an unaware enemy.
- 2026-10-09 (later) deathblows vs live, round 2:
  * ROOT MOTION FRAME (actor.rs advance): a clip's root track is in the clip's START frame - each step turns
    with (current yaw - the clip's own root yaw so far), not the current yaw. Matters for clips that spin
    (behind deathblow 511200 / ThrowDef13200 both turn 180 deg): live c1010 ends with the enemy 0.59 m in front
    of Wolf, same facing; the old rule slid him 1 m sideways, the new one 0.58 m in front.
  * Exporter resolve(): importFrom = group*1e6 + id; Wolf's per-group TAEs import ACROSS groups (a201_500200
    -> a200_500200, a201_501200 -> a201_500200). Before the fix both were "no TAE": the General's behind
    deathblow had no start throw. NPC single-TAE imports keep their own group.
  * Vault (0200/0201): throw pairs skip body push-apart (in_throw), a start throw pushes only Wolf; while
    SetNoGravity (flag 27) holds a grounded character the clip's vertical root lifts it (511900: +1.49 m;
    live +1.53 m), then FreeFall -> LandFreeFall (live 200010 -> 200020 ~1.4 s in). Live end: Wolf where the
    enemy stood, enemy stumbled 1.2-1.6 m forward under him, same facing.
  * Stealth: Enemy Mode::Unaware (IdleDefault a000_000000). Notice by NpcThinkParam sight cone
    (eye_dist_normal / eye_ang_*_normal) or a Wolf TAE CreateAISound (237, AiSoundParam radius) ->
    TransToBattleFromDefault (a000_001040, live). gap: meter fill rate not traced (0.5 s at
    eye_BeginDist_normal .. 3 s at eye_dist_normal). Stealth deathblow 0020/0021 from behind, stealth plunge
    0030/0031.
  * Head-kick camera: the lock-on chase holds while Wolf is horizontally within body radii of the target
    (the direction flips there). Live kick jump height 3.36 m (Sv1 3.45 m). gap: real lock-on pitch formula
    (exe FUN_14073c260 ~2090: asin-based framing around the camera fulcrum, CameraParam fulcrumDistRate 0.5 /
    fulcrumDistMin 2.0) not ported; camera not in the live recordings.
  * Neutral step (no stick) = GroundStep_N a000_213300, the same clip as GroundStep_F (4 m forward); HKS
    _set4DirStepDir gives VERTICAL for no stick, locked or not.
- 2026-10-09 vault, enemy side: the throw absorb (follow_throw, ThrowParam atkSorbDmyId 267 on 0201) ran
  one fixed step into a200_511900, after the clip's root yaw had already turned Wolf ~10 deg (511900 turns
  him 180 deg in ~10 frames), so the enemy was snapped facing ~10 deg off and stayed crooked. The absorb
  now places the defender on Wolf's pose at the throw anim's frame 0 (root motion undone). Live c1020
  (rec_c1020_b_20261009 tick 13131): enemy yaw -3.3 deg at the ThrowDef13900 start, ~0.4 m back, then
  ~1.1 m net forward; Sv1 now 0.2-0.4 deg, net ~1.1-1.3 m forward, final dyaw 0 (was 9.4).
- 2026-10-10 Ochimusha behind deathblow mid-throw drift: the exporter's yaw "unwrap" (hkx.rs unwrap_yaw_runs)
  was wrong. Live (rec_c1010_20261009 tick 1990-2002) Wolf turns the stored way, +76 -> -189 deg, and with
  the raw track Sv1's Wolf - enemy yaw difference matches live frame for frame. Removed. Gap: mid-throw the
  enemy's lateral offset still differs from live by up to ~0.4 m (live Wolf drifts 0.23 m right during the
  spin, his track holds still) - the exe's throw model-position interpolation (adsrobModelPosInterpolationTime
  0.5, atkSorbDmyId 249) is not traced.
- 2026-10-10 root yaw sign: the HKX reference-frame yaw is in the game's handedness; mirroring X for
  Bevy also mirrors turns, so export.rs writes -yaw (TurnDefault_Right90 = +90 raw = right). Proof:
  c1020 ThrowDefDeath 13411's Pelvis turns -182 deg in the clip while its root turns +180 (they cancel
  in the game); with the raw sign the General spun 360. The in-game behind deathblow (c1010) then
  matches the live recording frame for frame in tools/rec_throw.py's convention (x mirrored, yaw =
  pi - ours).
