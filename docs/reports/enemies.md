# Enemies - every enemy and boss from the game data
Status: 77 of 102 placed characters pass the smoke test (2026-10-10). The other 25 are mostly
not fighters (see below).

User request: "Work on all bosses and enemies".

## How to check
- Headless, all of them: `cargo test --release all_enemies_smoke -- --ignored --nocapture`
  (one line per chr: AI goal, anims, first hit on Wolf, deathblow pair). `SHINOBI_ENEMIES=c5000,c7100`
  limits the run.
- Real game, one enemy: `SHINOBI_DUEL=<file> [SHINOBI_DUEL_SECS=20]`. Wolf stays idle and cannot
  die. The file gets every combat log line plus one sample per second: distance, state, the
  distance to dummies 10/30/31, the AI plan and Wolf's hurtbox count (`src/duel.rs`).
- One AI row: `SHINOBI_THINK=<NpcThinkParam id> [SHINOBI_NINSATSU=<left>,<max>] cargo test --release think_trace -- --ignored --nocapture`.
- Build with `CARGO_TARGET_DIR=target/enemies` while a game holds `target/release/sv1.exe`.

## What changed
- Extractor: `tools/extract.ps1` unpacks every `chr/c1xxx-c7xxx` chr/ani/beh/tex binder, Wolf's
  per-enemy deathblow anibnds and the map event scripts. It also exports each roster model with
  its family texture packs (c<3 digits>8/9, NpcParam normalChangeTexChrId). `export.rs` adds
  AtkParam spEffectId0-4 / opposeTarget / friendlyTarget / atkMag/Fire/Thun/Dark / throwTypeId
  and the SpEffect chains (replaceSpEffectId, cycleOccurrenceSpEffectId).
- NPC bullets (`src/enemy_bullet.rs`): TAE 2 BulletBehavior -> BehaviorParam refType 1 -> Bullet
  row. These cover arrows, rifles, Shichimen's spirit balls, the Bull's fire and the fans and child
  bullets. A hit goes through combat.rs like a melee hit.
- NPC damage = atkPhys + atkMag + atkFire + atkThun + atkDark (Shichimen 10800500: atkMag 16).
- Hitboxes with opposeTarget 0 are skipped: they only hit objects (Bull 13700800 "for destroying objects").
- Resident StateInfo gates: NpcParam spEffectID0-31 carry the 950/951 TAE gates (Bull 3137040).
  Without them, the gated attacks never fired.
- Enemy grabs: ThrowParam row with AtkChrId = chr and throwKind = 1_000_000 + AtkParam.throwTypeId*10
  (zombie variants 4100/4110/4120, Wolf's a<defAnimOffset>). gap: this is a data pattern; the
  exe match is not traced.
- AI runtime (`src/ai.rs`):
  * Goals whose names drop the underscore (GOAL_COMMON_ComboTunable_SuccessAngle180 ->
    ComboTunableSuccessAngle180_*) now bind.
  * A goal id with no script fails instead of hanging.
  * Goal setters (SetLifeEndSuccess etc.) now return the goal, so chained calls work. The Corrupted
    Monk's opening Act27 was stuck.
  * GetNinsatsuNum / GetNinsatsuMaxNum are real.
- Bosses with several deathblows (NpcParam ninsatuNum: Genichiro 2, Corrupted Monk 3, Isshin 3, ...):
  * A non-final deathblow plays the ThrowDef reaction only (c9997 HKS: not IsThrowSelfDeath ->
    IdleTransition at the anim end; `Mode::Rising`). The boss then gets up with full HP and empty
    posture. gap: the exe's refill is not traced.
  * The last deathblow kills, as before.
  * Rows with ninsatuNum 0 (regular enemies, the user's General 10203010) die from one.
  * Test: `a_boss_takes_one_deathblow_per_red_dot`.
- Phase-2 move sets (request: "work on the phase 2 boss move sets"):
  * Anim sets: SpEffect 200030-200034 ("Anime ID offset [0]-[4]", stateInfo 270-274) picks
    a000-a400. It comes from NpcParam residents (c1021 spear General: a100) or from an anim's TAE
    (Isshin a000_003015, Monk a000_020015, Demon a000_020000). `Actor::grouped` plays the a<g>00
    clip when the set has one. The extractor now keys NPC TAE ids by their group (100003001 =
    a100_003001). Tests: `an_enemy_plays_its_own_anim_set`, `isshins_3015_switches_him_to_his_second_set`.
  * The Todome: the last deathblow on a boss with ThrowParam suffix 180/181 (c5000, c5060, c5100,
    c5400, c5430, c7020, c7110) plays that pair, then its Event20200 death. Test:
    `the_corrupted_monks_last_deathblow_is_her_todome`.
  * Phase rules from the map scripts: `tools/boss_events.py` reads every `event/m*.emevd` wait on
    `IF Number of Character Health Bars` and the boss actions after it ->
    `extracted/enemies/boss_events.json` (16 rules, 12 NpcParam rows). `enemy.rs phase_events`
    runs them once each when the deathblows left match: set / clear SpEffects, AI commands
    (GetEventRequest is now real), re-plan and forced anims.
    - Corrupted Monk (m25 12505961 / 12505962): 2 left -> AI command 1; 1 left, once her
      ThrowDef no longer holds 3500010 -> AI command 2 and anim 20010. Test:
      `the_corrupted_monks_phase_events_run_on_her_deathblows`.
    - Demon of Hatred (m11_00 11105912): 1 left -> SpEffect 3702005 (its battle script switches
      to the last-phase acts), HU1 277020/277021 -> HU2 277022/277023. Test:
      `the_demon_of_hatreds_last_phase_swaps_its_sp_effects`.
    - HU1 / HU2 (2026-10-10, static): 277022 is byte-identical to 277020 and 277023 to 277021
      (all 1088 bytes of SpEffectParam; only stateInfo 155 on 021/023, all rates 1.0). No exe
      imm32, Lua or HKS reads 277020-277023 (0x43A1C-0x43A1F): the swap is a no-op in the game
      too. The real last-phase change is 3702005 (702000_battle.lua:149 etc.), already applied.
      NpcParam 70200000-2 differ only in spEffectID20 3702001 / 3702002 (same fields).
    - gap: rules at 0 bars stay with `deathblow` (Event20200); the Monk's 20021 and Wolf's
      7102xx follow-up are not played. Rules with other conditions (Wolf's event message,
      regions) are skipped; event flags no phase rule sets count as on.
    - gap: the engine may clear an AI command slot once read; here it stays set.

## Every-phase check (2026-10-10, request: "do all of them and make sure everything works")
- `cargo test --release all_enemies_full -- --ignored --nocapture` (`SHINOBI_ENEMIES=` limits):
  per enemy and per red dot, it fights until 2 hits land on Wolf (Wolf walks up, kept alive,
  swings only after 8 s without a hit), then posture-breaks it and deathblows; the last one must
  kill (a grab that only holds counts as landing). Result: 86 of 102 pass every phase (all
  bosses: Monk, Isshin, Demon, Genichiro, ...).
- Debug: `trace_phase` (`SHINOBI_ENEMIES=<chr> SHINOBI_PHASE=<deathblows first>`: AI plan and
  state every 0.5 s), `dbg_blow_after_fight`.
- Fixed by it:
  * Grabs no longer restart while they hold Wolf (the valley sniper's hook re-grabbed every few
    frames: Wolf hooked forever, she never fired).
  * A grabbed Wolf is let go when the grabber is knocked out of its throw anim before its end
    (combat.rs resolve_throws).
  * AI move requests use the enemy's anim set (`Actor::grouped`): moves that only exist in a100
    etc. no longer fail silently.
- Static check (scratchpad atkcheck.py): attack events whose BehaviorParam is the other kind
  (TAE 1 -> refType 1 bullet, TAE 2 -> refType 0) and refIds with no AtkParam / Bullet row do
  nothing in the exe either (FUN_14099fe00 / FUN_1409a0060 take the AtkParam only for refType 0;
  FUN_1410b5f80 the Bullet only for refType 1), so they stay no-ops.
- Fixed after (2026-10-10, "continue with the remaining fixes"):
  * AI GetDist / GetDist_Point subtract the AI chr's own radius (Lua thunks 1405e2c10 /
    1405e2fe0 = `mov r8b,1; jmp FUN_1405f1540`, which subtracts its chr's vtable +0x68 radius);
    GetOriginDist (1405e3fb0, r8 = 0) is the plain distance. gap: the exe distance is 3D and the
    radius field is not traced (NpcParam.hitRadius used).
  * The AI makes its next plan in the same frame its last one ends.
  * An AI move whose clip is a single held pose lasts for its TAE (c9997.hkx Event20010_CMSG
    animeEndEventType 3 = None: the clip end fires nothing).
  * Together: the c1500 hidden ground zombie (Act16) now creeps up (20011), lies in wait (20010,
    SpEffect 5030) and rises to grab (Act15 3015 -> ThrowAtk4120, ThrowParam 21500200 "grab
    restraint from the ground", 12.7 s hold, no damage of its own; Wolf has no escape clip for
    it, a223_600375 does not exist, so it just runs out).
  * Boss endings: Wolf's Todome sends event message 10 (TAE 936, a242_511700 at 2.4 s); the
    0-bar map event (IF Character Has Event Message 10000 / 10) then gives the boss EzState 20200
    and Wolf Event7102xx (c0000.hks RequestThrowAnimInterrupt, W_Event7102xx). Only 710205 /
    710206 / 710207 have a state in c0000.hkx (m10 boss, Isshin, Demon of Hatred); the Monk's
    710200 and 710201 / 710203 / 710204 fire into nothing in the game too. boss_events.json
    gained "message" and "player"; `enemy.rs todome_message`.
- Still failing (16): non-fighters (merchants, nuns, handmaidens, Hanbei, lookout gong),
  scripted (c1260 chigo monkey: Folding Screen arena regions; c1550 "hidden for directing";
  c5300, c5410, c7200, c7300, c1320, c1460), and c1300 (its breath only builds Aging, not done
  in status.rs).
- Done 2026-10-10 (several enemies at once, HANDOFF 4.10): the Monk's clones (m25 12505961
  enables 2500851/3/4 at 2 bars; event 12505970 calls them on her SpEffect 5031) and Genichiro ->
  Tomoe c7110 (11115820). The Monk's 20021 at 0 bars (m25
  12505964: flag 12505954 off = no Todome message, 3500010 off): done (`fallback_death`). Demon HU1 / HU2:
  identical rows that nothing reads, so nothing to do (see above).

## Bosses finished (2026-10-11, request: "do all of them", HANDOFF 4.11)
- all_enemies_full now also runs each boss script row (Owl Father 50601010, Headless Ape
  51000100, Butterfly's second part via the hand-over, Genichiro -> Tomoe, Emma 74000010, the
  Mibu Monk 50001000): 94 / 110 pass. The Divine Dragon c5200 (52000000) has its own test and
  its deathblow (ThrowParam 15200090 in the 932/104 window of its collapse loop 21000).
- c5300 is the Old Dragons of the Tree, not the Divine Dragon; its combat rows pass. Its row
  53000000 still fails (likely unused: nothing in the scripts starts it).
- c7300 Divine Child: friendly NPC, no fight in the game.
- The Monk's clones fade in / out (TAE 193); invaders (team 24) and Ashina (team 6) fight
  each other; the Red Ogre (29) fights everyone.
- Each boss fights in its own real arena (docs/kb/map.md 2026-10-11).
- The Great Serpent c5010 stays a set piece (regions + fall throws; see HANDOFF 4.11).

## Still failing (25 at 2026-10-10; 16 at 2026-10-11), and why
- Not fighters (no attack in their AI or data): c1120 lookout gong, plus the memorial mob
  merchants c7540/7550/7560/7590/7600 that share its AI. Also c1260 folding-screen monkeys,
  c1012 Hanbei, c1110/c1111 handmaidens, c7440/c7450 nuns, c7420/c7430 merchants, c5021
  (conversation giant) and c7200 Kuro.
- Scripted / special: c5300 row 53000000 (Old Dragons of the Tree, not the Divine Dragon as
  first written), c5410 cutscene Isshin, c7300 (Divine Child, friendly), c1320 carp, c1340
  (underwater only), c1460 kites.
- Real but minor:
  * c1300 Mibu villager: the breath only builds Aging (9600, stateInfo 116), no damage. That status
    belongs to the status-effects session.
  * c1550 bandit row 15501007 ("hidden for directing") loops stance 3040.

## Gaps
- The Monk's phantoms: region heights are not checked; they vanish by their TAE 193 fade
  (2026-10-11) and are parked after their attack. (Messages 50 / 70 = TAE 231 and
  the regions 2502856-9: done.)
- Wolf's "recovery prohibited" SpEffects 105051 / 150302: nothing found that applies them.
- Map collision for bullets; homing / attached bullets (EmittePosType, FollowType).
