# attacks: Wolf's attacks: combos, releases, air/sprint/step attacks, combat arts, lunge, auto-homing

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: area scaling: "Growth Doping" SpEffects 7010-7014 (Ashina Outskirts morning = base values; day/evening/
  night/bell = x1.25/1.5/2/3 maxHp, maxStamina (posture), physics attack, staminaAttackRate, posture regen). config
  [enemy] area_doping (default 7010); applied at spawn + Actor.atk_rate / stam_atk_rate in combat.
- 2026-10-06: player attack routing = HKS ExecAttack: combo refs 214-219, GroundStepAttack_{N,F,B,L,R} (ref 224),
  SprintAttack (ref 1), HardDeflectAtk{Small,Middle}{F,L,R} out of StandDeflectHard*/HardDeflectDmg* (the fast
  counter after a deflect), EasyDeflectedAtk{L,R}, DeflectGuardAttack from guard (ref 213), else Combo1; each with its
  Release on ref 208. Long scripted duel vs the real AI (duel_trace) is now a regression test (no state > 6 s).
- 2026-10-06: Whirlwind Slash (attack + guard together): skill -> virtual weapon 5100 "rotating slash"
  (spAtkcategory 100 -> anim a100_316000 in c0000_a07x, behaviorVariationId 5001 -> judges 200/201 resolved at +1000).
  Its damage is the "dark" element: player damage now sums attackBase{Physics,Magic,Fire,Thunder,Dark} x AtkParam
  element corrections x NpcParam <element>DamageCutRate (c1020 dark 0.6). Dummy ids 10000+ = weapon dummy (id-10000).
  Focus from here (user): Wolf's own combat.
- 2026-10-06: Wolf's prosthetic - Loaded Shuriken (F): weapon 70000 (wepmotionCategory 70 -> offsetType 14 -> a070
  anims in c0000_a07x / a70.tae; behaviorVariationId 7000). GroundSubAttackCombo1 (a070_400000, held: TAE 2 judge 100
  frame 21 -> Bullet 700000) or let go -> GroundSubAttackCombo1Release (a070_400100, judge 150 frame 3 -> Bullet
  700050). Bullet: 80 m/s, life 0.5 s, past dist 5 m decelerate (accelOutRange -40) and drop (gravityOutRange 5),
  hitRadius 0.1, AtkParam 7000150: 200 % of attackBasePhysics 20, posture 5. Exporter: bullets per judge
  (BehaviorParam refType 1 -> Bullet -> atkId_Bullet). src/prosthetic.rs: projectiles vs ragdoll hurtboxes, enemy
  guard/deflect, 1 Spirit Emblem per throw (config player.spirit_emblems = 15). Gap: hand dummy 2 position.
- 2026-10-07 FIX: TAE events with StateInfo != 0 only apply under an SpEffect of that state (exe FUN_140bfebf0 in
  the TAE handlers). Wolf had 163 such AttackBehaviors live: the weapon-enchantment extra hits (SpEffectParam
  stateInfo 152 poison blood, 153 red blood, 357 white blood, 358 orange wind, 940 gold wind) fired with every slash.
  Now gated in Event::active / attack_windows / sounds; also shuriken upgrades 907 and coins 996-999. Shuriken spawns
  from its event's DummyPoly (2, the hand) when the model is loaded.
- 2026-10-07 NEXT 15 done: swept hit test. combat.rs resolve keeps every dummy's world position from the previous tick
  (Local map) and tests the AtkParam capsule at SWEEP_STEPS = 4 interpolated poses between last tick and now, so a fast
  slash no longer passes through thin ragdoll capsules between ticks. (Idea from the Elden Ring rewrite's blade sweep.)
- 2026-10-07 NEXT 16 done: attack lunge (TAE 760 BoostRootMotionToReachTarget), decoded from FUN_140842980's assembly
  (return in xmm0): scale = clamp(|arrive - self|, EnableRangeMin, EnableRangeMax) / ReferenceDist, arrive = target +
  rotY(ArriveAngle) * dir(self - target) * ArriveDist; RangeMin/Ref when already inside ArriveDist; 1.0 without the
  event or with ReferenceDist 0. Recomputed every frame (FUN_1407f0a70 -> chr+0x2fc). 20 player anims (held combo,
  step/sprint attacks, deflect counters, DeflectGuardAttack, Whirlwind) and 6 enemy ones. Implemented as
  Actor.root_scale (actor.rs boost_root_motion). Target: lock-on target (Wolf) / player (enemy). Gap: Wolf's fallback
  target point when not locked (chr+0x1080 in FUN_1407f0a70) not identified -> no boost unlocked. Test
  locked_slash_lunges_toward_a_far_target. 39 pass.
- 2026-10-07 lunge target, exe detail: chr+0x1070/+0x1080 = lock-on flag / lock target point (written by the lock
  selection FUN_1409c5fe0 via FUN_1409f6130). FUN_1407f0a70 first asks chr module slot 0x21 (FUN_140b2dab0): it returns
  that module's +0x38 target point only while NOT locked on = the unlocked attack auto-aim target. NEXT: identify module
  0x21 (RTTI of the object at modules[0x21]) and how it picks +0x38, then use it for unlocked lunges and attack facing.
- 2026-10-07 WOLF: attack auto-homing (unlocked). chr module slot 0x21 = CSChrAutoHomingModule (RTTI via vtable
  0x142a73540; built in FUN_140a002f0). HKS _StartAutoAim on attack transitions; _UpdateAutoAim calls act(156) for
  1/6 s while not locked -> FUN_140b2e090 -> FUN_140b30fa0 -> FUN_1409c58e0: nearest lock candidate (with line of
  sight) passing FUN_140b2ddc0: 3D distance from feet+1.5 m <= LockCamParam closeMaxRadius (3.0; _forD/_forPD in
  darkness), height within -closeMinHeight (3.0) .. +closeMaxHeight (1.0), angle to stick (or facing) <= closeAngRange
  (30 deg). Limits copied from the LockCamParam row by FUN_140b2e0e0 (param index 0x1f, offsets 0x20-0x44 incl. the
  bullet* auto-capture). FUN_1409efdb0 = attack target point: lock point (chr+0x1080) else homing target. Implemented:
  player.rs start_auto_aim / auto_homing_target, Actor.homing (turning during attacks + TAE 760 lunge). Gaps: darkness
  radii, line-of-sight ray.
- 2026-10-07 FOCUS: sword play (user). Ground routing gained ref 231 GroundAttackCombo1Reverse (a050_300001, + Release
  a050_300101) in the HKS order 214,215,231,219,216,217,218: after every easy deflect, Combo2, DeflectGuardAttack,
  DeflectGuardToStand, sprint/step L attacks the next slash starts from the other side (was Combo1).
- 2026-10-07 air slashes: BEH_A_AIR_ATTACK refs 214/215/216 -> AirComboAttack1/2/3 (1 -> 2 -> 3 -> 2 ...). Landing while
  ref 201 (ENABLE_ORIGINAL_LAND_ACTION, frames 0-27) is up -> LandAirComboAttackN continued at the same time
  (HKS StartTime_00 = env(3063)/1000; Actor::continue_state keeps landed hits). LandAirComboAttack2 carries 231.
- 2026-10-07 attack while holding guard = combat art (ACTION_ARM_SPECIAL_ATTACK); DeflectGuardAttack only routes with no
  art equipped (HKS: env(345, HAND_RIGHT) == SP_ATK_TYPE_NONE). Sim test attack_while_holding_guard_is_the_combat_art.
- 2026-10-07 `sekiro-extract objects <hkx> <Type>` dumps every object's members (raw i32/f32, strings, array items).
  TAE 700 CustomLookAtTwistModifier (c0000.hkx): TwistParam {i16 bone from, i16 to, f32 weight, f32 x2 speeds};
  0_Twist (lock-on idle/turns, TargetType Lockon) LR +-45 / UD +-25 over bones 7-44 (0.3) and 79-80 (0.7), sensing dmy 261;
  100/110/120/130_Attack (Freeaim, slashes) UD only, up 30-35 / down 25-35, bones 7-43 weight 1; 30/31_Throw on dmy 260.
  Not implemented: on flat ground the attack twists are ~0 (the lock-on idle twist is visual only).
SWORDPLAY NEXT (for the next session): (a) TAE 700 twists (vertical aim of slashes at height differences, idle torso
  twist to the lock target); (b) TAE 151 look limits (+0x4c..+0x58, used by FUN_140741?/14074.c:401); (c) user feedback.
- 2026-10-07 sprint actions: ref 1 SP_EF_REF_ENABLE_SPRINT_ACTION (SprintStartFromStep_F, SprintLoop, SprintToDeflectGuard
  6-15) opens every action (player.rs sprint_window in buffers/accepts): SprintLoop has no ChrActionFlags, so a sprint
  attack out of SprintLoop was unreachable (found by helper S3). Test attack_out_of_a_sprint_loop_is_a_sprint_attack.
- 2026-10-07 combat arts selectable: config player.combat_art = virtual weapon (EquipParamWeapon 5100 Whirlwind Slash,
  5300 Ichimonji ("真っ向"), 5200/5400-5900 others; spAtkcategory -> anim group a1xx, behaviorVariationId 5001..5020).
  Export resolves art judges per group through the art's variation (BehaviorParam_PC base + (var - 5000) * 1000 + j)
  into attacks "a<group>:<judge>"; data.rs attack_windows prefers them for a100-a110 anims. First anims exported for
  every art (a1xx_316000 / a101_316001). Art follow-ups (combo 2, finishers) not wired: HKS _FireSpAttackCombo needs
  ref 223 + the unlock SpEffects. Test equipped_combat_art_ichimonji_plays_with_its_own_attack.
- 2026-10-08 ART FOLLOW-UPS (HKS _FireSpAttackCombo) done. Window = ref 223 SP_EF_REF_TAE_ENABLE_SP_ATK_COMBO
  (SpEffect 100252 in a102_316000 f54-66, a105_316000..030, a106_316000, a107_316010/020, a108_316010...).
  Chain states (c0000.hkx CMSGs, ids in the art's group a<spAtkcategory>): Combo1 316000, Combo1Release 316100,
  Combo2 316010, Combo2Finish 316011, Combo2Release 316110, Combo3 316020, Combo4 316030, Combo5 316040,
  VariationCombo2/3 317010/317020. Unlocks = resident SpEffect behaviorRefId of the upgraded art weapon:
  6100 (cat 104) 287, 7000 (101) 280, 7100 Ichimonji: Double (102) 281, 7200 (105) 282, 7300 (106) 283,
  7400 (107) 284, 7500 (108) 285, 7600 (109) 286; 6000 = cat 109, 7700 = cat 110 (no unlock).
  Rules: 102 w/o 281 no combo; 107/108 w/o unlock: Combo1 -> Combo2Finish and stop; 105 w/o 282: Combo2 ->
  VariationCombo3; else Combo1 -> 2 -> 3 -> 4 -> 5. player.rs art_combo_next / art_kind; tests
  ichimonji_double_follows_up_but_ichimonji_does_not, floating_passage_chains_its_strikes.
  Variation judges (2026-10-08): every art weapon 5000-7999 exports "v<variation>:<judge>" (7200 -> 5021,
  6000/7700 now covered too); CharData.art_variation (player.rs sync_art_variation) picks them first.
  gap (old, fixed): judges were per group from the 5x00
  row's variation. gap: 104 hold arts (VariationCombo2 from HOLD_ACTION) and sprint/air art starts not wired.
- 2026-10-08 ART STARTS (HKS BEH_A_GROUND_SP_ATTACK): sprint window (ref 1) -> SprintSpecialAttack (316300 in the
  art group; 104 excluded), 101 (Nightjar) -> {Ground,Sprint}SpecialAttackStep_{F,B,N} (316001/316002/316000,
  sprint 316301/316302): _set2DirStepDir F = stick within +-90 deg of the facing (measured before the art
  turns Wolf), N = no stick, else B; without unlock 280 (art 7000) always F. Else GroundSpecialAttackCombo1.
  SprintSpecialAttack chains like Combo1. Exported per group: 3163xx sprint, 3165xx/317000 hold (104),
  3167xx jump (107/110). Test art_out_of_a_sprint_and_nightjar_steps.
  Art early release (2026-10-08, from the live recording): ref 222 SP_EF_REF_TAE_ENABLE_SP_ATK_RELEASE (a106_316010 f6-18,
  a106_316000 f15-27, a102_316000 f18-30); attack let go -> Combo1/Combo2/SprintSpecialAttack Release. Done (test
  letting_go_of_attack_in_an_art_releases_it). Old gap text: (no ref 208 in art anims; the trigger
  BEH_A_GROUND_SP_ATTACK_RELEASE source unknown), 104 hold flow, 107/110 jump flow, NoResource variants.
- 2026-10-08 HOLD ART (spAtkType 104, 5500 / 6100): art press -> GroundSpacialAttackHoldStart a104_316500 (sprint:
  SprintSpecialAttackHoldStart 316300) -> HoldLoop 316510 (loops) while attack + guard held. Attack let go
  (BEH_A_GROUND_SP_ATTACK_RELEASE in an ATK_HOLD state) -> HoldAction 317000 with unlock 287 (6100's resident
  SpEffect 100286), else GroundSpecialAttackCombo1 316000; guard let go (..._GUARD_RELEASE) -> HoldEnd 316520.
  Test hold_art_sheathes_and_releases. gap: hold move / quick turns (HoldMove, HoldQuickTurn*), VariationCombo2.

## General's attack choices, live vs ours (2026-10-08)

Live (rec_20261008_054259, NpcParam 10203010, the player fighting actively): 96 attacks in ~140 s
(~1.5 s apart). Counts: 3000 x19, 3004 (perilous sweep) x12, 3008 x8, 3003 x7, 3020 x6 (always at
4.8-5.5 m: the long-range opener), 3006 x4, 3061 x4, 3007/3009/3056/3060/3063/3067/3068 x3,
3001/3012/3015/3051/3088 x2, 3002/3014/3057/3065/3066 x1.
Ours (sim_tests ai_attack_distribution: player only guarding at 2.4 m, 180 s): 70 attacks (~2.55 s
apart); 3000 x12, 3008 x9, 3003/3007 x7, 3006/3009 x5, 3004 x4, 3014 x6, 3067 x4, 3063 x3, ...
Same main set. 3020 never shows at 2.4 m (it is the far opener), and the 305x/306x counters need
player attacks. A fair pace comparison needs the same player behaviour (record a passive fight).
