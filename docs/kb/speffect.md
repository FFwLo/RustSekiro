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

### Gaps (closed below, 2026-11-03 later, except where it says)

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

## 2026-11-03 (later): the gaps closed (`tools/emu-cases` `speffect2`)

Same snapshot and harness (`speffect::run_set`, now with a per-scenario `Setup`: posture poke,
worn rows whose disable bits are cleared in the copy). 40 scenarios (20 x 30 / 60 fps), 12,420
frames, 100 s; `extracted/emu/speffect2_ticks.json`. The test `speffect_ticks_match_the_exe` now
replays every run of both files (gourd and Wolf's TAE rows aside) with the character wearing the
snapshot's list (Wolf: 8610, 150204, 150600 ...; the General: his residents).

### The add's "already on" (FUN_140c02000 -> FUN_140c064c0), confirmed

FUN_140c02000 says "on" when an entry has the same id (FUN_140c059e0: not expired, not category
10), or the new row's stateInfo 2 / 5 / 6 (1000 / 1001: either) is an entry's (FUN_140c05a10), or
FUN_140c05a80's category rule holds (100: same categoryPriority; 110, 140; 141: the entry's
priority below the new one's; 130-133, 150-204: the same category). Then FUN_140c064c0 looks for
the entry the new row takes over (FUN_140c06860: the entry gets the new id, time =
effectEndurance x mult, timer 0 unless stateInfo 50 / 52 / 67 / 68): category 20 the same id;
100 the same priority; 110, 130-133, 140, 150-174, 190-204 the same category; 141 never; 175-189
the same category with a priority not above the entry's; stateInfo 1000 / 1001 either. None:
the add returns -2, nothing changes and no build-up is taken.

| run | exe |
|---|---|
| ako_then_ungo / ungo_then_ako (162) | the second takes the first's entry (3401 <-> 3421) |
| cat100_220030 | 220031 (prio 50) takes 220030; 5400 (prio 0) is added |
| cat110_resonance | 41 -> 42 -> 41, one entry |
| cat130_5580 | 5581 takes 5580 |
| cat141_6022 | 107712 (prio 10 > 1) refused; 230630 (prio 1) added beside |
| cat150_aging | 9640 takes 9620 |
| cat181_medicine (General) | 150204 (50) takes 150203 (60); 150203 and 150200 then refused |
| same_id_9250 | category 0 again: refused, time runs on (0.9667 at 1 s) |
| pellet_again | category 20 same id: refreshed (9.8667) |
| burn_then_bleed / _elixir | 6400 / 3044 (stateInfo 6) on a burning Wolf: refused, no build-up |

Fixed in RSekiro (`status::add_kind` / `start`): before, any same id refreshed and nothing else
matched (so a second 9250 restarted at 2.0 s, Ungo stacked beside Ako, the bleed built up).
FUN_140c00de0's lifeReductionType / Rate multiplier is ported too (no row in these runs uses it).

### Replace through the add (floor_4011_x10)

The expired entry's replaceSpEffectId goes through FUN_1409e9c00 (FUN_140c005b0), build-up
included: each 4011 takes 2 % (Wolf 22, the General 264) and the next tick 4012 puts 22 (15 x
1.5) into Wolf's burn gauge (15 into the General's); the tenth fills it and 4012 burns, ticked in
the same pass. RSekiro: `tick_effects` sends a build-up replace through `build_up_one` (before:
4012 burned at once).

### Regens, posture, buff duration

- HP (FUN_140bfd0a0): two sums, each truncated once: changeHpRate / changeHpPoint and
  changeHpEstusFlaskRate / changeHpEstusFlaskPoint, a heal's estus rate times the worn
  changeHpEstusFlaskCorrectRate product and held within +-100 % per row (a heal's HP rate times
  hpRecoverRate). Pellet 3021 +50 (1120 x 4.5 %), ohagi 3061 +117, both on one tick +168.
- Posture (FUN_140bfe2c0, applied at <decomp ref>): (int)(max x sum(changeStaminaRate) x 0.01 +
  sum(changeStaminaPoint)) onto the posture left: persimmon 3071 +42, ohagi 3062 +84 (posture
  set to 100 first), aging 9620 -420 at once. RSekiro's posture is damage, so it is subtracted;
  aging's drain moved from `set_aging` into this path.
- Buff duration (FUN_140c005b0): rows with isExtendSpEffectLife count by dt / (1 + sum over the
  stateInfo-193 rows of extendLifeRate - 1): Wolf's 150600 (1.5) -> Ako 3401, Ungo 3421, 1150
  at 29.9778 after one 30 fps frame; isContractSpEffectLife by dt x (1 + sum stateInfo-303
  contractLifeRate - 1) (ported, unchecked: no row on).
- stateInfo 120 / 121 / 155 (FUN_140c03af0) clear the expired flag and fire at the add: an
  endurance-0 5370 still lives exactly its add frame, 1150 / 2090 as any timed row.

### Wolf's worn rows, the General's burn

- `player::wolf_worn`: his actor's rows, the learned skills' SpEffects (Knowledge of Medicine at
  its level, 150200 -> 150204) and 8610 when `player.hard_mode` (new config switch, Demon Bell /
  Charmless) -> `Statuses::hit_wolf_wearing` (combat.rs, enemy_bullet.rs) and the list tick.
  Test `wolf_worn_rows_reach_his_build_up`: a 9100 builds 31, 46 in hard mode.
- The General's burn did not take because the snapshot's General wears 30201 "[Conversation]
  damage completely invalid" (disablePoison / Disease / Blood / Curse / Freeze all 1): the
  pre-fight conversation row. With its bits cleared in the copy (general_burn) 9100 builds 31 a
  hit into 300, the tenth burns, 70 a tick (13207 x 0.5 % + 4 = 70.035).

### Exported rows

The combat_data.json in use came from an export_states list that is not this checkout's (a full
export here drops 206 states), so the rows are added with `sekiro-extract listsp <extracted>`
(export::WOLF_LIST_SP; serde_json float_roundtrip so nothing else changes): 4011 / 4012, 9200 /
9250, the items, the sugars, the category rows above, 150600. The full export takes them too.

### Not covered

- gap: cycleOccurrenceSpEffectId rows (Ungo's 3421 adds 3422 at its fire, 3422 adds 3423; when
  Ako takes 3421's entry the exe zeroes 3422 / 3423) are not in `Status`; the test leaves them
  out of the list.
- gap: a worn resident taken over (FUN_140c064c0 on a resident) is added beside it here.
- gap: the refresh's fire flags (FUN_140c06860 sets 4 or 1 by the row's heal fields) are not
  modelled; the counts matched without them.
- The General's terror stays immune (resist 999).
