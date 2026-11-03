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
  is only caught where the code cites the exe's offset. gap: no systematic exe-accessor scan of
  every field RSekiro reads.
- The NPC bare-judge attack keys: the export also tries BehaviorParam base + 1000 + judge (the
  player's art rule) when base + judge has no row; FUN_140a12b80 has no such fallback. 66 bare keys exist only through it
  (c1020, c1190 and c7410 have the most). The test counts and prints them and does not fail on them: whether the exe falls back
  elsewhere when the row is missing is not traced (the vfunc that calls FUN_140a12b80 has no
  direct caller in the decomp).
- Runtime changes to rows: the snapshot is one moment (Naomori's arena, NG). Rows the game
  rewrites later (none seen here) or per-NG scaling (SpEffect-driven, not a param write) are not
  covered.
