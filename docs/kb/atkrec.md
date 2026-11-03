# Attack record (atkrec)

How a TAE behaviour event becomes the attack the hit test carries, run on the exe.

## 2026-11-03 attack info and hit entries vs the exe

Run: tools/emu-cases `atkrec` (atkrec_judges, atkrec_rows, atkrec_frames) on the snapshot.

**Chain.** TAE AttackBehavior (type 1) FUN_140b50ae0 -> FUN_140b679e0: hand from the event's
"Source" (byte +0xd: 1 Force Right Hand -> -1, 2 Force Left Hand -> -2, 0 -> chr vfunc 0x330,
Wolf -1), row = vfunc 0x1c0(judge, hand). PCBehavior (type 307) FUN_140b55510 -> FUN_140b67740 ->
FUN_14084fbf0: row = vfunc 0x1c8 = the judge itself (BehaviorParam_PC row), hand vfunc 0x330,
flags = the event's (always 4). Both build the info with FUN_14083d220 + FUN_14084fc50(info,
chr, row, hand, flags) and issue it with FUN_14099fc00, which registers one hit entry per
AtkParam shape (FUN_1410b6150 -> FUN_1410b6880 / 6760, FUN_14099f310).

**Rows.** PlayerIns 0x1c0 FUN_140a24560: weapon = vfunc 0x338(hand) (110000 when flags & 0x60),
v = EquipParamWeapon behaviorVariationId, row 100000000 + v x 1000 + j if that row exists, else
100000000 + (v / 100) x 100000 + j; then the SpEffect judge shift (data::shift_judge). Ported as
`data::pc_behavior_row`, used by `combat::guard_row` (was 100000000 + v x 1000 + j, no fallback).
ChrIns 0x1c0 FUN_140a12b80: 200000000 + v x 1000 + j, no fallback. Snapshot Wolf: right hand
-1 = the art's virtual weapon 7400 (variation 5008: judges 200/201/210/211/220/221/230 own rows,
the rest 105000xxx), left -2 = the Loaded Axe 73000 (7300).

**Info** (0x150): +8 AtkParam id, +0xc pc, +0x14 BehaviorParam category, +0x18 flags, +0x70..+0xf8
rates from SpEffects (FUN_140bf8900), +0x128 hand, +0x12c weapon, +0x134 second weapon, +0x138
power = (flags & 0x68) == 0 && hitSourceType == 1.

**Entry** per shape: amounts x5, +0x34 atkStam x posture rate, +0x38 (directAtkStamDamage > 0 ?
direct : atkStam) x rate, +0x3c repelLostStamDamage x rate, +0x40 flick, +0x60 radius, +0xd0 /
+0xd4 dummies, +0x8c.. 5 SpEffects. Shape 0 is registered even when empty (radius 0, dummy -1);
shapes 1-15 only with radius > 0 and dummy >= 0 - every real shape is RSekiro's DmyPoly1 > 0.

**Wolf's attack power.** Amounts (FUN_14083e060, player branch) = the hand weapon's
attackBase* x AtkParam atk*Correction / 100 (all stat / reinforce / durability factors 1); every
right-hand / art weapon has attackBase 40, the Loaded Axe 45. hitSourceType 1 (body) rows use
their own atkPhys.. (the air kick 901: 10; RSekiro gave 0). Attack Power is SpEffect 160000 + level
("Player Growth_Core Level N"), every rate equal (1.0 at L0-1, 3.25 at L10, 7.23 at L99 = the
snapshot): it lands in info +0xb4 / +0xe8-0xf4 and scales the posture amounts (atkStam 15 ->
108.45), not the entry's HP amounts. gap: where HP gets the level rate (past the entry, the
defender's sum) is not traced. The General: info +0x70 / +0xa4-0xb0 = 5.36 (his doping), amounts
= atk* x 5.36, posture x 5.36, flick = guardAtkRate.

**Cost.** PlayerIns vf63 FUN_140a240e0 = FUN_1408406c0: BehaviorParam stamina x param 0x7e +0x30 x
SpEffect rate (x weapon +500 when flags & 3), subtracted from posture by FUN_140bd4de0. Only row
10 "egg attack" (20) is non-zero and no anim reaches it: nothing to port.

**Stick requests.** FUN_1407ed630 (from PadManipulator FUN_1407ef960 on attack / sub attack)
sets ActionRequest bits 0x19 / 0x1a from the stick against the facing (param 0x7e byte +0x5a
degrees) and three thresholds. HKS defines no ACTION_ARM 25 / 26 and no C++ reads them: written,
never read. Nothing to port.

**Second spawn event** (item 2): PCBehavior 307 spawns contact rows 200-222 (no damage;
atkBehaviorId 9 on 210 / 211), foot / hand sfx rows 280/281/285/286 and the air kick 901.
RSekiro resolves 307 as "pc<judge>", the same.

**Results.** 6,000 judge resolutions, 724 rows (325 Wolf rows x 2 hands + 74 General rows), 7
action runs + the General's 3000. Tests `attack_rows_match_the_exe`,
`behaviour_rows_resolve_like_the_exe`, `tae_attack_events_match_the_exe` (src/sim_tests/
emu_atkrec.rs). Fixed: "Force Right Hand" events in tool anims resolve through the right hand
(Sabimaru a075 140/141 -> 5000140/141, was the kodachi's 7500140/141); hitSourceType 1 rows
carry their own amounts (`combat::player_amounts`, `Attack::amounts_zero`); `guard_row` falls back
like the exe; Wolf's body rows (hitSourceType 1: kicks, deathblows 600-806, 72 cases) flick
with the row's guardAtkRate 0 (FUN_14084c7b0 when info +0x138 is set), was 30 / 90. gap: the frames' anim field is not the TAE key (events checked by resolution, not by
window); the prosthetic run fired no event; the enchantment rows 180-186 stay gated (no issue).
