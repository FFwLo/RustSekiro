# SpEffect list tick

How the exe adds, counts down, fires and removes SpEffects on a character, and what RSekiro
(`src/status.rs`, `src/actor.rs` run_sp_effect_chains, `src/prosthetic.rs` timed) does with it.

## 2026-11-03: emulator check (`tools/emu-cases` `speffect`)

Run on the Naomori-arena snapshot: rows added to Wolf and the General (NpcParam 71003080) through
the game's add FUN_1409e9c00 (source = the chr, four 1.0 factors, rate 1.0), then the chr's tick
vfunc 44 (enemy FUN_140a02a10, Wolf FUN_140a36cc0) frame by frame at 30 and 60 fps. 54 scenarios,
32,049 frames, 204 s; the result is `extracted/emu/speffect_ticks.json` (git-ignored). Test
`speffect_ticks_match_the_exe` replays the 16 runs whose rows RSekiro has (8,730 frames) through
`Statuses::build_up` / `start` / `status::tick_effects`; `gourd_heal_matches_the_exe` checks the
gourd.

### Add (FUN_1409e9c00 -> FUN_1409e9d10 -> FUN_1409f8320)

- FUN_140c02280: a row of spCategory >= 10000 (poison 10003, burn 10004, terror 10005, shock
  10007) is refused while an entry of its category is on: no entry, no refresh and no build-up.
  A burning Wolf hit by 9100 or 9105 keeps his burn as it is (burn_again, burn_then_9105).
- FUN_140c02000 "already on": the same id, the same stateInfo 2 / 5 / 6, or FUN_140c05a80's
  category rules (100 same priority, 110, 0x82-0x85, 0x8c / 0x8d lower priority, 0x96-0xae,
  0xbe-0xcc, 0xaf+) take the refresh path FUN_140c06860: time back to effectEndurance (Ako's 3401
  re-added at 10 s: 30.0), interval timer to 0. Other categories stack: pellet 3021 and ohagi
  3061 (both 20) run together.
- FUN_140c03af0 entry init: time = effectEndurance x FUN_140c00de0's multiplier; interval timer 0
  (stateInfo 50 / 52 / 67 / 68: motionInterval); effectEndurance 0 marks the entry expired at once.
- Build-ups (FUN_140a00f60 into the resist module [bag+0x20], FUN_140bd9d30): the row's whole
  points times the add's factors, which carry the defender's worn def*ResistDamageRate: Wolf
  wears 8610 (Hard mode: poison / burn x1.5, terror x1.2), so a 9100 burn builds 46 (31 x 1.5,
  fills 200 on the 5th hit) and terror 9200 30. Until the resist breaks the entry is removed again
  (FUN_140bfb950). The module's +0x10.. counts down from the max (+0x24..) and refills on a break.

### Tick (FUN_140a03340 -> FUN_140c005b0, per entry FUN_140c06b70)

In list order, per entry:
1. Expired on an earlier tick (and not added since): removed; its replaceSpEffectId is added
   through the add (with build-up) and ticked in the same pass.
2. Time <= 0 and > -1 (before this tick's count): marked expired (entry +0x25); it still fires.
3. Interval timer <= 0: fires (FUN_140c06c60, flag 4), timer += motionInterval (0: every tick).
4. Timer and time count down by dt while > 0; the time stops at -FLT_EPSILON.

So a 9.9 s burn filled at frame 12 fires at 12, 42, ... 282 (10 times at 30 and 60 fps), is
marked at 309 and gone at 310. 109450 (endurance 0) is there on its add tick; the next tick it
becomes 109451, already at 0.9667 s; 109451 is there frames 1-31 and gone at 32. An instant row
(4011, terror 9200) fires on its add tick and hands on the next.

### Reactions (vf44 after the list tick)

- HP FUN_140b658b0 -> FUN_140bfd0a0 -> FUN_140c014f0: the fired rows' changeHpRate and
  changeHpPoint are summed, then one int: (int)(-(max x rate x 0.01) - point). Burn on Wolf's
  1120: 9 (0.5 % + 4 = 9.6); poison 9045: Wolf 20 (8.4 + 12), the General 111 (99.05 + 12);
  poison and burn on one tick: one sum. The gourd's changeHpEstusFlaskRate times the entry's
  factors and the worn changeHpEstusFlaskCorrectRate product, held within +-100 %: 1120 x 40 %
  x 1.5 (Knowledge of Medicine LV5) = 672. Applied by FUN_140bd4d40 -> FUN_140bd64e0 (0..max).
- The "no damage" check vfunc 0x558 (enemy FUN_1409f3300) zeroes the damage part: the General
  waits for his fight with chr +0x1a11 bit 0x20 set (cleared in the emulator copy).
- Posture FUN_140bfe2c0: changeStaminaRate / changeStaminaPoint of fired rows (aging, round 46).
- Buff duration skill (stateInfo 193): rows with isExtendSpEffectLife (+0x16c bit 2) count down
  at dt / (1 + sum(row+0x30c - 1)): Ako's 3401 and Ungo's 3421 at dt / 1.5 on Wolf.

### Fixed in RSekiro

- HP per tick: summed rows, one int (burn 9.6 -> 9 a tick on Wolf, poison 20.4 -> 20, the
  General's poison 111.05 -> 111).
- The interval: motionInterval 0 fires every tick (was held to 0.1 s).
- Expiry: the entry lives the tick its time is found run out and goes the next, its replace
  added then and ticked at once (burn 297 -> 298 frames on; 109451 30 -> 31 frames; the chains
  in actor.rs and Wolf's timed rows use `status::count_down`).
- Instant rows (effectEndurance 0) are entries that fire on their add tick (was a separate
  instant path that skipped their changeHpPoint).
- Category >= 10000 refuse: no build-up, no refresh while burning / poisoned (was: refreshed in
  place; test `reigniting_a_burn_refreshes_it` now checks the refuse).
- The build-up times the defender's worn def*ResistDamageRate (`ResistRules::resist_damage_rate`).
- The gourd heal is whole points (`player::estus_heal`, held within 100 %).

### Gaps

- gap: FUN_140c02000's stateInfo and FUN_140c05a80 category matches (only the same id refreshes).
- gap: the replace is added without its build-up (4011's 4012 goes into the burn gauge in the
  exe, 200 -> 178 on Wolf; RSekiro burns at once).
- gap: Wolf's worn rows (8610 Hard mode, Knowledge of Medicine) are not passed to `hit_wolf`.
- gap: regen items (3021 pellet +50 a tick, 3061 / 3062 ohagi, 3071 persimmon), Ako / Ungo
  buffs, the buff-duration slow count: not run through `Status`.
- gap: the General's burn build-up is not taken in the emulator (cause not traced); terror resist
  999 is immunity.
- gap: stateInfo 155 / 120 / 121 fire at add.
- Terror 9200 and the floor's 4011 are not in RSekiro's exported rows, so the test skips them
  (their kinds' 9222 / 4001 take the same path).
- Emulator note: `Emu::call` adds and deletes a block hook per call; after about 650 vf44 calls
  Unicorn segfaulted, so the tick uses `call_quiet` without it (also 3x faster).
