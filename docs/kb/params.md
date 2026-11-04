# Param rows against the exe's loaded tables

RSekiro reads the game's params from its own export (`tools/sekiro-extract params` ->
`extracted/json/params/*.json`; `export` / `npcs` -> `combat_data.json`, `enemies/<chr>.json`).
This note is how those rows were checked against the tables the running game holds.

## 2026-11-03: the snapshot's tables (emulator topic params)

How the game holds them (decomp, read only):

- SoloParamRepository: singleton DAT_143d978b0 (vtable SoloParamRepositoryImp, 0x26c8 bytes).
  0x88 slots of 0x48 at +0x70: +0x70 the slot's count, +0x78 up to eight ParamResCap pointers
  (FUN_1410ce110, FUN_1410ce0e0, FUN_1410ce020).
- ParamResCap: name = wide string at +0x18 (inline when its capacity +0x30 < 8, length +0x28);
  PARAM file = [[cap+0x70]+0x70] (FUN_1410c5b10).
- ThrowParam is not in the repository: its own singleton DAT_143d6dae0, ParamResCap at +8
  (FUN_14095dcb0; a sorted id index sits before the file, at file - [file-0x10] rounded).
- PARAM file as loaded = the regulation's bytes: +0xa row count, +0x10 offset of the type
  string, +0x2d format (0x84 = 64-bit offsets + type by offset), rows at +0x40 of 0x18
  (id, data offset, name offset). MenuColorTableParam is the one old-format file (0x2), skipped.

`tools/emu-cases params` walks those (no code runs), decodes each row with the Paramdex defs
(its own reader, not sekiro-extract's) and writes `extracted/emu/params_rows.json`: 140 tables,
numbers only (fixstr and padding left out), with each field's byte / bit offset. Every table's
in-memory row stride equals its paramdef size.

Tests (`src/sim_tests/emu_params.rs`):

- `param_tables_match_the_exe`: every exported table against the game's, every row and field
  (floats as the same f32). Ids a table holds twice (CutsceneParam 3, ModelSfxParam 1,
  ObjectParam 7, PhantomParam 2, ShopLineupParam 2, TalkParam 5, UpperArmParam 1) are skipped:
  the game's sorted lookup can find either. The four Lod tables of the export are not loaded in
  the game (no rows to compare).
- `character_rows_match_the_exe`: Wolf (combat_data.json player, its params) and every
  `enemies/c*.json` (102 files: the 14 bosses, 18 mini-bosses and the common enemies) as RSekiro
  loads them: the params tables, attacks (AtkParam row + hit shapes), SpEffects, bullets and
  their attacks, Wolf's BehaviorParam_PC rows and tools (EquipParamWeapon), raw and through the
  typed `Attack` / `SpEffect` / `BulletSpec` fields; and each attack key's BehaviorParam row as
  FUN_140a12b80 builds the id (200000000 + NpcParam behaviorVariationId x 1000 + judge, judge
  < 1000) must refer to the attack's AtkParam row (4,679 keys). 24,690 rows, 2,210,531 fields. The coverage table's
  "Data vs exe" column shows the count per character.

What matched as it was: all of it. The 24,690 character rows and 35,841 rows (3,915,368
fields) of the 131 exported tables the game also holds are the game's own values; no rounding, no missing rows, no field the typed loader reads
that the export leaves out (so no serde default stands in for a real value).

Fixed:

- ThrowParam sight casts (player.rs `throw_sight_ok`): FUN_140be3da0 reads +0x30 for cast 0
  (at 0.8 of the height) and +0x32 for cast 1 (0.42); Paramdex names +0x30
  sphereCastRadiusRateTop and +0x32 Low, and RSekiro had them the other way round (its comment
  said "Low (+0x30)"). The value check could not see it (a name swap, both fields right); a scan
  of every "field (+0xNN)" the code and kb cite against the paramdef offsets found it (42 others
  agree). Only the Chained Ogre's grab rows (AtkChrId 5020: 25020000 / 25020100 / 25021000 /
  25021100, Top 29 / Low 51) tell the two apart; every other row has them equal.
- sekiro-extract `param.rs`: a def that names several fields alike (CoolTimeParam's four
  limitationTime / observeTime pairs, NetworkParam's sets) kept only the last; now
  `<name>_1`, `_2` ... CoolTimeParam row 101 read 0 / 0 instead of the game's 900 / 600, 600 /
  450, 300 / 200. RSekiro reads neither table. The two files were re-exported in place (the
  other 133 came out byte-identical).

Not covered / gaps:

- Field names are Paramdex's. The value checks prove the bytes; a name swap like the throw one
  is only caught where the code cites the exe's offset. (2026-11-04, E30 below: a read trace of
  the exe's param accessors; covers the fields with a cited function, not every field.)
- (Done 2026-11-04, E15 below.) The NPC bare-judge attack keys through the export's + 1000 fallback.
- Runtime changes to rows: the snapshot is one moment (Naomori's arena, NG). Rows the game
  rewrites later (none seen here) or per-NG scaling (SpEffect-driven, not a param write) are not
  covered.

## 2026-11-04 (E15): how the exe turns a TAE judge into a BehaviorParam / AtkParam row

The chain (decomp, read only; run in the emulator):

- TAE AttackBehavior FUN_140b50ae0 (and the other behaviour events FUN_140b55510,
  FUN_140b565e0, FUN_140b56ba0) call the chr's vfunc 0x1c0 (judge, hand) for the row id, then
  the attack record FUN_140b679e0 -> info fill FUN_14084fc50: FUN_1410b5cf0 looks the id up in
  repository slot 0xc + chr type (BehaviorParam_PC / BehaviorParam, a plain binary search), and
  the AtkParam id is the row's refId when the row exists and its refType (+9) is 0, else -1 (no
  hit entries). Nothing looks again with another id.
- EnemyIns vfunc 0x1c0 = FUN_140a12b80: judge < 1000 (else -1); id = (200000 + NpcParam
  behaviorVariationId (row +0, through vfunc 0x178's +8)) x 1000 + judge; then the worn
  SpEffects' (addBehaviorJudgeId_condition, _add) pairs (FUN_140bffb40): the FIRST whose
  condition is judge / 100 adds its add, and an add of 0 makes the id 5 (no row). No fallback.
- PlayerIns vfunc 0x1c0 = FUN_140a24560: the hand's weapon (vfunc 0x338; 110000 with flags &
  0x60), its EquipParamWeapon behaviorVariationId v < 100000, id = (100000 + v) x 1000 + judge;
  when BehaviorParam_PC has no such row, (1000 + v / 100) x 100000 + judge, i.e. variation
  v / 100 * 100: an art's 50xx falls back to the katana's 5000, never to another level of the
  art; then the same SpEffect shift. The right hand's weapon is the equipped art (the
  snapshot's: 5008) for every anim, not only the art's (no art judge 200-267 / 999 is used
  outside the art anims, so RSekiro's art-anim-only lookup gives the same rows).

Checked (`tools/emu-cases params_keys`, extracted/emu/params_keys.json): the General's
FUN_140a12b80 + FUN_14084fc50 with his NpcParam row's variation set (in the emulator's copy) to
each of the 2,710 (variation, judge) pairs the enemy files use (main variation and every
"v<variation>:" one x their attack keys and AttackBehavior judges): 1,846 resolve to an AtkParam
row. All 66 bare keys the export had only through base + 1000 + judge are -1 in the exe (c1020 /
c7410's 18: variation 10201 is the spear General c1021's; c1190's 11: c1191's; the others' own
variant rows, which the files also hold as "v<variation>:" keys). Wolf's rule against
`atkrec_judges` (1,000 judges, art 5008): every right-hand row is the art's when it exists,
else 5000's.

Fixed:

- sekiro-extract `export.rs`: bare keys are base + judge only (no + 1000). Re-exported c1010,
  c1011, c1020, c1180, c1190, c1220, c1310, c1450, c7410, c7490 and combat_data.json: the 66 NPC
  keys and Wolf's bare 200 / 201 (Whirlwind's rows) gone, and c7490's SpEffect 104000 (only
  from a dropped attack); nothing else changed (last-digit f64 noise in a few derived floats).
- `data::select_variation`: a placed row of another variation drops the bare keys that
  variation has no row for (it kept the main row's).
- `CharData::attack_windows_on`: with an art equipped, an art anim's judge with no row of that
  art falls back to the bare (5000) key, not to the art group's base level ("a<group>"): the
  upgraded Spiral Cloud Passage 5021 has no judge 209, the base 5020 has, and the exe hits
  nothing there.

Tests (`src/sim_tests/emu_params.rs`): `npc_attack_keys_resolve_like_the_exe` (every
(variation, judge) of every enemy file after `select_variation` against the exe's AtkParam id:
3,385 per-file (variation, judge) checks over 102 enemies, 2,419 of them attacks),
`wolf_art_judges_resolve_like_the_exe` (the fallback rule on 1,000 judges; the a107 anims'
16 windows against the exe's rows), and `character_rows_match_the_exe` now fails on any bare key
without an exe row.

Not covered:

- The SpEffect shift's order (the exe takes the first matching worn SpEffect in its list order;
  RSekiro sums every match: differs only with two matching enchantments worn at once, none
  seen) and the add-0 rule (only SpEffectParam row 0 has a condition with add 0).
- ThrowAttackBehavior (TAE 304) judges from 1000 (c1020's 1010-1090, c5000's 1000-1080, ...):
  FUN_140a12b80 returns -1 for them, so the throw handler builds its id another way; not
  traced, those keys stay base + judge.
- Combat-art bullets: prosthetic.rs looks only at the art's own "v<variation>:" bullet, with no
  5000 fallback (no art judge has a 5000 bullet row today).
- The left hand (the prosthetic's variation, 7300 in the snapshot; same rule): RSekiro's tool
  keys were not re-run here.
- Only the General's EnemyIns runs the NPC function (it reads nothing chr-specific but the
  NpcParam row and the worn SpEffects).

## 2026-11-04 (E30): which exe code reads which param field

The exe has no field names (its only ones are a dev test step's three asserts on ATK_PARAM_ST:
dmgLevel, atkPhys, guardAtkRate), so names are checked through the code that reads them:

- Read watch (`tools/emu-cases`, params.rs `watch` / `ReadsLog`): with `EMU_PARAM_READS=<file>`
  every Emu hooks reads of the row data of all 140 loaded tables, whatever generator runs, and
  records (instruction, table, row offset -> the Paramdex fields on that byte, count);
  `emu-cases ... params_reads` with DSEKIRO_FUNCTIONS names each instruction's function. Run
  over atkrec_rows, guard, speffect, speffect2, hits, bullets, items, aiint, lockcam,
  air_falldmg, wire, damage, knockback, brkwin (throw and acttrace_attack stop early on this
  snapshot): 2,234 (instruction, field) pairs, extracted/emu/params_reads.json (local).
- Cross-check: every place RSekiro's source names a param field within two lines of an exe
  function it cites (630 pairs, 211 functions). 140 pairs: the cited function reads that very
  field -> 130 (table, field, function) claims over 113 fields of 14 tables, kept in
  `CITED_READS` and checked by `cited_param_fields_are_read_by_the_cited_exe_code`. 107: the
  function reads other fields of the table; each looked at, all are flows where the field is
  read by a callee getter (SpEffect FUN_140c04xxx accessors, FUN_14083dea0 / FUN_14083e060 for
  the attack amounts from FUN_14084fc50) or two cites on neighbouring lines (FUN_140ba8a50 = SFX
  materials, FUN_140ba8840 = SE: as the code says), no swap. Sibling names were looked for in
  particular (Top / Low, 1 / 2, _Attacker, Rate / Type): FUN_140c00de0 reads lifeReductionRate
  +0x124 in its decomp (no traced case matched).
- Cited offsets: every "field ... +0xNN" in src/ and docs/kb against the paramdef offsets: 56
  agree; the rest are chr / camera struct offsets, not param fields. One stale kb line fixed:
  guard-deflect.md had sphereCastRadiusRateLow / Top for +0x30 / +0x32 (it is Top / Low, as
  player.rs reads since the 2026-11-03 fix).

Mismatches found: none in RSekiro's code.

Not covered:

- Fields RSekiro reads with no cited function nearby (most of the typed Attack / SpEffect /
  BulletSpec fields, the camera's CameraParam rows): their bytes are proven equal to the game's,
  their meaning only by Paramdex.
- 404 pairs whose cited function did not run in any traced case (status build-up, stealth /
  NpcThinkParam forget times, prosthetic costs, terrain, sounds), and 149 whose function reads
  only other tables in the traces (the field read by a callee on another path).
- A read is attributed to the function holding the instruction, not its callers.
