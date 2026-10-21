# Recording research: enemy perception, lock-on, cutscene/talk, player data, effects, bullets, lighting
(exe 1.06, static RE, 2026-10-21; decomp FUN_ names via tools/fn.sh; singletons from
reference/Smithbox-src/Documentation/Memory/Sekiro_1.06.txt, cross-checked with DAT_ in the decomp; prologues
read from extracted/sekiro_dearxan.exe)

## A. Enemy perception (AiThink = [[ChrIns+0x58]+0x340], enemies only)
- Targeting system TS = AiThink+0x7B30 (SprjTargetingSystem, SekiroTool AiThinkOffsets; update FUN_14061ff40).
  TS+0x158 state 0 none / 1 caution / 2 find / 3 battle, +0x15C previous; +0x160 reason, +0x164 previous
  (FUN_1406208c0: 3 enemy slot, 4 sound, 5 memory, 6 indication, 7 corpse, 1/2 forced battle; slot ids inferred);
  +0x150 flags (0x100000 = find or battle); +8 owner. High.
- Current target: slot list head TS+0x18, node +0x10 next, vfunc +0x10 slot type (1 enemy), +0x20 accessor
  (SprjChrInsHandleTargetAccessor: +0x8 chr handle, +0x10 cached ptr), +0x30 state. Medium.
- Sight meter: vector TS+0x40..TS+0x48, stride 0x28: +0 float meter (>= 100 -> indication slot), +4 in-cone byte,
  +8 accessor, +0x10 handle (FUN_140626430, FUN_140625340). Medium-high.
- Hearing: sound slots (SprjSoundTarget), no meter; not decoded.
- Home point: ChrIns+0xE0 vec4 position, +0xF0 rotation, captured at the first update (FUN_140a01d70); patrol type 5
  overwrites (FUN_1409f60c0). Medium.

## B. Lock-on (LockTgtManImp = [0x143D78058])
- Candidates: head mgr+0x10, node next +0x80, chr handle u32 +0x78, flags byte +0x90 (0x10 candidate, 0x20 current,
  0x40 other); FUN_1409c31f0 node -> ChrIns, FUN_1409c3930 node position. Cooldowns mgr+0x2844 / +0x2848 (0.5 s on a
  switch), camera mgr+0x2838. High (FUN_1409ca480).
- Switch FUN_1409ca480 (callers: stick FUN_1409ca2d0, mouse FUN_1409ca120; update FUN_1409c5fe0). The "lock" hook
  already sees the result.

## C. Cutscene and talk
- Cutscene start: EMEVD 2002 (FUN_14090b500) -> FUN_14067b4e0(this [0x143D55600], edx cutscene id, r8d mode, r9d,
  stack, player) or FUN_14067b760 -> FUN_140ed1af0(SprjRemo [0x143D8DC08], request). The emevd hook already sees bank
  2002. Current-cutscene field in SprjRemo not found.
- Talk line: FUN_1411ca4c0(talk obj, edx TalkParam row, ...): first row (FUN_1410cfa90, row +0x18 next, +0x20 flag),
  stores obj+0x1C row / +0x20 flag, shown by FUN_1411cbe50. SprjWorldTalkMan [0x143EA0CE0]. Medium-high.
- Talk command runner FUN_1411c63a0 (vtable slot 0x142bd31d8): (talk env, EzState event); event vfunc +0x10 command id,
  +0x18 arg count, +0x20(i) arg. Ids = ESD Talk bank 1 (1 TalkToPlayer, 0x11 generic dialog, 22 shop, 24 upgrade,
  31 level up, 123 prosthetic upgrade, 124 skills). ESD state step not found.

## D. Player game data PGD = [[0x143D5AAC0]+8]
- Sen +0x7C (total earned u64 +0x80); XP toward the next point +0x160, total +0x158, skill level +0x154; attack power
  +0x48 (stat block ~+0x44..+0x68, +0x74); HP +0x18, max +0x1C, base max +0x20; kill count +0xDC.
  Source: the exe's PGD debug menu FUN_1407c15e0 / FUN_1407c51a0 + SekiroTool. A separate skill point counter not found.
- Inventory PGD+0x278 (table inv+0x168; FUN_14079d7e0 slot, FUN_14079e0f0 count). Spirit emblems = goods 1000 + 1001;
  Unseen Aid = goods 500; gourd = goods 3000 (3001 empty gourd; which holds charges inferred).
- Resurrection: max nodes PGD+0x168 (EMEVD 2003[85]), lit nodes = count of goods 1100 (fragments 1101); recast gauge
  float +0x150; +0x14C protect-life counter, +0x148 true-death counter. Medium.

## E. Effects and bullets
- Effect end: FUN_141d7beb0(wrapper): FUN_141da5af0([wrapper+8]) sets effect+0x80 |= 4 (end requested). The wrapper is
  the `out` (rcx) of the vfx create hook FUN_14092afe0, so ends match creates by pointer. Stop/free FUN_141d7c350.
- Bullet manager [0x143D84F70]: +0 pool ptr, 0x80 slots x 0xD30 (SprjBulletIns); +0x1BC active count.
  Slot: +8 handle (-1 free), +0x30 position, +0x20 orientation, +0x60 own effect, +0xB60 metres flown,
  +0xC6C flags (bit 1 active, 4 destroy requested, 8 flying). BulletParam via fly state +0x10. Per-frame update
  FUN_140b13770 (bit 4 -> destroyed); explosion at lifetime end FUN_140b20b70 -> FUN_140b0fc60. Medium.

## F. Lighting / GParam
- CSWorldSceneDrawParamManager [0x143D8DF18]: per-area entries [mgr+0x20] + i*0x118 (count mgr+0x10): +0x18..+0x28
  five current ids, +0x2C pending, +0x11 dirty, +0xC4 fade time. EMEVD 2003[88] Set Area Gparam Sub ID ->
  FUN_140ed6e80. CSGparamRepository [0x143D8EEE8]. Low-medium; the current fog/light values not found.

## Open
Hearing; current cutscene field; ESD state step; skill point counter; bullet free routine; live fog/light values.
