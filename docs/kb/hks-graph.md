# The game's player script and behaviour graph (HKS + c0000.hkx)

How the game picks Wolf's states and clips, and what our test pipeline runs. Dated entries,
oldest first.

## 2026-10-20 The pipeline (tests only so far)

- Script: the compiled `action/script/c0000*.hks` in sekiro-rs's HKS VM (`sekiro-hks`,
  dev-dependency), `src/hks_lua.rs` `HksVm`; the decompiled `hks_src/*.lua` in Lua 5.1 (`HksPlayer`)
  runs every call beside it as a check (0 calls differ).
- Graph: `src/hkb_runtime.rs`, sekiro-rs's behaviour-graph runtime adapted (CREDITS.md), on
  `extracted/chr/c0000.behbnd.d/c0000.hkx` parsed by `sekiro_formats::hkb`.
- `ScriptChr::tick`, one frame: `Update()`, every active state's `_onUpdate`, the
  `_onDeactivate` / `_onActivate` hooks of the states the events moved until the graph settles
  (8 rounds at most), the ended clips' events, the clips' advance, the ends that advance reached.
  The script's events and SetVariable values go to the graph after each call; the graph's active
  nodes answer `hkbIsNodeActive`.
- Engine answers that belong to the graph: env 339 (anim ended) is the graph's clip (the hook's
  own state's, else the full-body clip), not the sim's.
- Clip lengths: `sekiro-extract clip-lengths extracted/chr c0000 extracted/clip_lengths/c0000.json`
  (every clip in c0000*.anibnd, hkaAnimation duration; tools/extract.ps1). The combat export keeps
  only the states in its list (1176 of the graph's ~2040), so clips like the map entry
  a000_700010 were missing; a clip with no length never ends.
- The game starts Wolf with the map entry (c0000.lua InitState: W_GroundMapEnter a000_700010, or
  Crouch / Swim / Dive / Aging versions by env 3061 / 3055); our sim starts him standing.

## 2026-10-20 How a CMSG picks its clip (exe)

- offsetType (CustomManualSelectorGenerator enum, Smithbox HKX2 templates): 0 none, 11 idle
  category, 13 right weapon category, 14 left weapon category, 15 anim id offset, 16 hand style,
  17 throw category (Sekiro; Paramdex ThrowParam atk/defAnimOffset say so).
- FUN_1411e6b40 -> FUN_1411e9670 -> callback FUN_140b417a0 -> FUN_1409f1830: for 13 / 14 the hand's
  row (13: the combat art's row on the right): its spAtkcategory N when the chr has `aN_<animId>`
  (FUN_140987ad0), else the hand's wepmotionCategory (category array chr+0x120, the env 225 one).
  17: the throw's ThrowParam atkAnimOffset (+0x76) / defAnimOffset (+0x78).
- FUN_1411e6b90: the clip named `a%03d_%06d`, else `a000_<animId>`, else the CMSG's last clip.
- So the katana combos stay a050 (no a1xx twin) while GroundSpecialAttackCombo1 plays the art's
  a100..a110; tools a070..a079 by the tool's wepmotionCategory. sekiro-rs's fixed {13: 50, 14: 70}
  with a first-clip fallback only matched the default art and tool.
- gap: the idle category array (slot 2) and its +300 candidate; offsetType 15 for NPCs.

## 2026-10-20 Locomotion variables (exe FUN_140b4afd0, from FUN_140b49d30)

- Written each frame through a 91-name table at 0x143b08e30 (slot 0 IsLockon, 1 MoveAngle,
  2 MoveSpeedLevel, 3 MoveDirection, 4 TurnAngle, 29/30 JumpAngle / JumpStickLevel, ...).
- MoveSpeedLevel: the camera-rotated stick's length, 0..1; 2.0 while the sprint is held (hold
  timer past 0.4 s); capped by a per-chr mode (1: 1.0, 2: 0.1, 3: 0). No smoothing here
  (MoveSpeedLevelReal is the script's ConvergeValue).
- MoveSpeedIndex is the script's (_SpeedUpdate, c0000_transition.lua 4882): to 1 above 0.9, back
  to 0 at 0.7 or below.
- MoveAngle: the stick against the facing, degrees, right positive, 0 with no stick. TurnAngle
  (FUN_140b4c600): manip+0x24 in degrees (source inferred). MoveDirection: the script's
  (_MoveDirectionUpdate: |a| < 55 F, > 125 B, else L / R by sign).
- Ours: player.rs speed_level 0 / 1 / 2 (walk / run) converged at 3 or 6 per s, and the stop
  check `speed_level > 1.2`, is a different scale from the exe's (2 = sprint held). Open.
- The script's GetLocomotionType reads the walk / run clip's TAE refs (90 MOVING_WALK, 91
  MOVING_RUN): player.rs's sp_ref_active now reads the shown clip's TAE (the procedural walk,
  run and idle clips too), not only `a.anim`.

## 2026-10-20 Lockstep result

hks_lua::lockstep: 33 runs of 60 frames (combos, guard, air, arts 5300 / 5600, tools 71000 /
72000, step, step-attack, crouch-attack, gourd, walk / run and stop, run-attack, run-jump): the
clip the game's script + graph plays equals player.rs's shown clip on 1969 frames; 11 frames the
game is one frame ahead at a clip's end (sekiro-rs ends a clip in the same tick its advance
reaches the end; when Havok sends the end event is not traced yet); 0 differ.

## 2026-10-20 Enemy script (c9997 + c1020 on c1020's c9997.hkx)

hks_lua::enemy_lockstep runs the shared NPC script and the soldier's own on its graph beside
enemy.rs. Engine answers, from the sim:
- The AI's request: env 106 (attack id, 9910 guard, 9920 general stance); the cancel slots 107 /
  120 / 118 while the clip's TAE window (flags 23 / 86 / 79) is open. The request stays posted
  until the script plays it (enemy.rs starts it the frame it asks).
- env 2000 (can cancel into movement) = the clip's flag 78 "End If AI Move Queued" (gap: the
  exe's NPC side of 2000). TurnAngle from its facing to the requested point (else Wolf).
  env 1114 IsExistAnime from the chr's clips. MoveSpeedLevel / MoveAngle from the move request
  (gap: the exe's NPC writes are not traced).
- TAE reads (windows, SpEffects) are of the script side's clip, as the game's engine reads the
  clip its own graph plays. The AI thinks before the script runs: the sim steps first.
- A hit (Enemy::damage_env, set in combat.rs): env 202 type (0 plain hit: gap, 3 guard, 1001
  guard break, 1027 posture break, 1019-1021 its swing blocked, 1008-1010 deflected, 1033 the
  deflect that broke it), 236 level (0 for its own met swing), 222 side (0 left, 1 right,
  2 front, 3 back: GetDamageDirectionOffset onto DamageSmall a000_008000 front / 1 back), 3032
  back, 3056 / 3057 guard sides, 3018 / 3019 bound sides, 258 NpcParam guardLevel.
- Throws: the engine fires W_ThrowAtk<id> / W_ThrowDef<id> and W_ThrowDefDeath<id> into the
  graph itself; env 1112 on the first frame (ExecPassiveTransition leaves the clip to the throw,
  before ExecDeath), 206 in a throw, 273 the id. The script's own 276 path would fire
  "W_ThrowDefDeath" .. ThrowID = W_ThrowDefDeath12000, which the graph does not have (it has
  12001): the death clip's start is the engine's (inferred, as sekiro-rs duel.rs).
- After a death clip the script goes to DeathIdleDefault (a000_010001, one frame, TAE 228
  RagdollReviveTime: the body is ragdoll); enemy.rs holds the death clip's last frame. Counted
  apart ("ragdoll"), not as a difference.

Result (6 runs; frame 0 is the script's own InitState -> W_IdleBattle):
- the enemy fighting alone, 900 frames: 899 the same;
- Wolf attacking every 45 frames (flinches, its guard 3100 and GuardDamageSmall /
  JustGuardDamage), 600: 599;
- Wolf holding guard (AttackNoBoundEnemy, the additive AttackGuardBound_Add01, its grab
  ThrowAtk4100), 900: 899; tapping guard (AttackBoundEnemy), 900: 899;
- a 2400-frame mixed fight: 2399;
- posture break (TrunkCollapseFront a000_008300) and deathblow (ThrowDef12000, ThrowDefDeath12001),
  480: 376 the same, 103 ragdoll.
No enemy.rs reaction differed from the script once the engine answers were right.

### Every regular enemy (survey, 2026-10-20)

`the_games_enemy_scripts_beside_enemy_rs_for_every_regular_enemy` (ignored; `SHINOBI_LOCKSTEP_CHRS`
picks chrs, space-separated; `LOCKSTEP_DEBUG=f,f,...` prints those frames' clip, flags, request,
TurnAngle and 3036 refs) runs 30 regular chrs for 1200 frames each: c9997 + the chr's own
c<chr>.hks on its own behbnd's c9997.hkx, Wolf attacking / guarding on a timer. All 81 c*.hks
are decompiled into extracted/hks_src (DSLuaDecompiler, as c0000 / c9997). Harness answers added
on the way (each was the harness, not enemy.rs):
- AnimOffsets offsetType 15 = the chr's anim set (anim_group * 100): every NPC CMSG in c9997.hkx
  is type 15 (c1020: 264 of them), so the a100 enemies play a100 clips.
- env 330 = the AI's emergency turn; env 1000 HP reads 1 at 0 HP while not dead (IsNoDead);
  env 201 grounded is off while the hit was aerial; IsExistAnime has the TAE aliases too.
- The AI-state SpEffects 200000 / 1 / 2 / 4 are one slot (spCategory 100, categoryPriority 1):
  the AI's state replaces a NpcParam resident one. 51 NpcParam rows wear 200000 from spawn (the
  piranha c1320): with both on, UpdateAIState (DEFAULT first) picked TurnDefault.
- TurnAngle below 0.001 degrees is 0 (float noise made an emergency ExecTurn turn 90).
- env 275 = Wolf escaped its grab (his ThrowEscape) -> ThrowAtkCommonFunction's
  W_ThrowRelease<ThrowID>. env 273 follows the throw's state byte (case 0x111 ->
  FUN_140bdfb20): atkAnimId / defAnimId, and once escaped atkAnimId + 2 / defAnimId + 3, which
  sets ThrowID to the zombie's 4122 (its graph has W_ThrowRelease4122, not 4120).
- IsExistAnime (case 0x45a, 140b3.c:10600): the exe tries the id plus each anim offset first
  (FUN_140988510, animmgr +0x60..0x6c), then the raw id, so an a100 enemy's IsExistAnime(4120)
  is its a100_004120. gap: what fills the other offset slots.

enemy.rs fixes it found:
- A turn needs the move window (flag 78 or free), not only a free state, and a turn clip's end
  does not chain into another turn (c9997 TurnCommonFunction passes enable_turn FALSE): a broken
  soldier's TrunkCollapse turns to Wolf at its flag 78.
- The chr's own HKS end chains (data::hks_end_chains, parsed from extracted/hks_src/<chr>.lua):
  Event<n>_onUpdate's `if env(339, 1) == TRUE then Fire("W_Event<m>")`, under its enclosing
  `env(3036, ref)` checks; random (`fate`) and elseif ones are left out. 34 chrs have them, e.g.
  the zombie c1500's held pose 20010 -> 21015 (same pose and TAE), c1020 20002 -> 21001 /
  20004 -> 21007. This replaced the c5310-only chain for the Divine Dragon's trees (their loop
  repeat stays). Mode::Rising's loop_after guess is gone: an event state holds in its clip
  until its HKS moves it (data::hks_events).
- Event states fire before the clip's end too (2026-10-20, EndChain at_end / grounded): with no
  talk ESD running `env(3054) ~= ref` holds and `== ref` never (c1010 21003 -> 21000, c1020
  21007 -> 21005); `env(201) == TRUE` once on the ground (c1100 21005 -> 20005);
  ExecTalkTransitionIndividual's NOT_TALKING moves, run first in EventCommonFunction (c1070
  21000 -> 20001). 81 decompiled files start with a BOM, which had hidden their first function
  (c1070 Event20000_onUpdate). Harness: env 3053 = -1 (not talking), and `GetVariable("Debug<n>")`
  answers a table (c9997 DebugPrint writes `[0]` of the graph's vector variables; c1110 calls it).
  `a and b` conditions are split into parts (c1500 21008 -> 21004 under ref 1000021), and a talk
  check nested under `if anim_id == <n>` counts (c1110 21004 -> 21003). Survey hks_lua
  `the_games_event_anim_ends_and_enemy_rs_agree` (ignored, ~2 min): every a000_02xxxx event of
  the 29 regular chrs forced as a map script does, Wolf far off; the state after it matches the
  game's graph + c9997 + c<chr>.hks in all of them (was 5 off, plus a crash on c1110).
- Death picks by worn SpEffect ref first (ExecDeath): TO_DEATH_EVENT20000 + n (1020000-20) the
  one-shot Event200nn, TO_DEATH_UNIQUE_CASE 1000180 DeathStartUniqueCase 10160 (the zombie killed
  as it lets go).
- The cautious soldier now turns to a sound inside its 1010 reaction's move window and finds Wolf
  standing there (test running_behind_an_unaware_enemy_turns_it_to_wolf); before, the AI's turn
  was dropped until the clip ended. a_cautious_enemy_listens_to_a_new_sound now places its
  sounds ahead of the soldier.

Result: 28 of 30 match on every frame (ragdoll / random death frames apart; c1030 and c1210 one
frame early once at a clip end). Left: c1000 (the hidden bar holder) and c1460 (the Nightjar
kite) have only a T-pose / a few clips: the game stays in Init (a000_000000) while enemy.rs
shows its idle name. Not fighters.

## 2026-10-20 Hit answers in the exe (env 202, 236, 237, 222, 3032, 3056-3057, 3018-3019, 256)

All read the chr's action-flag module D = *(ChrIns+0x1ff8) (env switch FUN_140b36410, 140b3.c);
FUN_140b27c20 (140b2.c:5823) resets them each frame before the script runs (D5c = 99999, the
directions -1; ordering inferred). Static research, not traced live.
- 202 = D5c. Defender (FUN_140b6bd40, 140b6.c ~6480-6590), first match: dead in the air 9, dead
  2, rec+0x48 0x3f 1002; guard break 1001; guarded 3 (1028 GUARDATTACKER_STAMZERO when the
  AtkParam doesBreakRepelStamDamage bit is set), a deflect 3 too (1028 likewise), a hit's break
  1027, a plain hit **99999** (c9997's DAMAGE_TYPE_NONE; enemy.rs's DamageEnv kind 0 is a guess).
  Attacker (FUN_140b698d0): deflected 1000 GUARDED (category 2: 1003, 6-8: 1008-1010), blocked
  1017 (category 2: 1018, 6-8: 1019-1021), its posture emptied 1033.
- 237 = a 5x12 table at 0x143b01bc0 [guardLevel][damage level] (FUN_140843820): the guard
  reaction size (0, 1, 3, 4). guardLevel 0 (an unguarded hit) all 0; rows 1-4 by level 0-11:
  `0 1 3 3 4 1 3 3 0 3 4 3`, `0 1 1 3 3 1 3 3 0 3 4 3` (rows 2 and 3), `0 1 1 1 3 1 3 3 0 3 4 3`.
  Wolf's guardLevel is his weapon's EquipParamWeapon.guardLevel; 236 is the remapped level.
- 222 = an 8-way sector (FUN_140b6b350, 45 degree steps in x/z from the facing and the hit
  direction rec+0x150) folded to 4: sectors 0/7 front 2, 1/2 right 1, 3/4 back 3, 5/6 left 0
  (side labels inferred). 3032 = 1 (back) for sectors 2-5.
- 3056 = the AtkParam's deflectAction when guarded, 3057 its justDeflectAction on a deflect (-1
  otherwise). 3018 / 3019 = the attacker's deflectedAction / justDeflectedAction.
- 256 = a hit that dealt damage > 0 this frame (a fully cut block: 202 = 3 but 256 off).
- 1119 = the AtkParam's atkDir_forSfx; 1121 = the knockback magnitude (int), negative only for a
  flipped push (its source rec+0x18c not found). 1007 = a behaviour object state == 6 (unnamed).

## 2026-10-20 Live mode (SHINOBI_HKS=1)

`src/hks_wolf.rs` `HksWolfPlugin`: Wolf's script + graph run in the game, not only in tests.
`src/hks.rs` holds the runner (`ScriptChr`, `HksVm`); `hks_wolf.rs` the answers and the live
systems; `hks_lua.rs` keeps the test-only lockstep and the Lua 5.1 check.

- Order (FixedUpdate): `tick_script` before `ActorSet::Decide`, `apply_script` after it and
  before `Advance`. player.rs still moves, collides, hits and uses items; where the graph plays
  another clip than player.rs picked, the graph's state and clip win (`sync_clip`).
- Both just standing or moving (`player::is_free` and `graph_free`): player.rs keeps its own
  locomotion. player.rs names a state by its CMSG (`rt.clip_cmsg`), the graph's state can hold
  several (GroundJumpStart).
- States player.rs or combat.rs start go into the graph as the exe's own events:
  W_ThrowAtk / W_ThrowDef / W_ThrowKill / W_Event + the anim number, then the grab's
  `W_<state><ThrowId>` (W_ThrowDefDeathStart600000; the grab also sets Selector_ThrowId), then
  `W_<state>` where the graph does not show it yet. A new anim in the same state (the throw kill
  510200 -> 510201) is tried as the numbered events only. Nothing matches: the graph stands aside
  until both show the same clip.
- A throw the hit started (the breaking deflect's pose) replaces that frame's hit: the script
  does not get the hit too. Any other reaction to a hit is the script's own (it gets the hit's
  answers first); only where the graph still differs after the tick is player.rs's state fired.
- Landings: player.rs finds them after the script ran, so the graph keeps the fall one frame and
  the script lands itself on the next (env 248 IsTrulyLanding, 224 GetFallHeight from the
  flight's highest point): BEH_R_LAND sets Selector_LandStiff (20 m: a000_200021).
- player.rs back to standing or moving where the graph still plays an action (a respawn, a
  cancel of its own, a CMSG with no end event held at its last frame): W_Idle / W_CrouchIdle.
  player.rs's clip ending this step is left alone (the graph ends a step earlier).
- New answers: 206 / 276 (in a throw, killed while grabbed), 224 / 248, 3025 (IsSwimContact),
  3033 per unlock type, 3036 with the art's resident SpEffects, 345 0 with no art, 1108 also for
  held attack / dodge / prosthetic, the prosthetic button as both ACTION_ARM_SUB_ATTACK and
  ACTION_ARM_SHINOBI_WEP_ACTION (the umbrella's release wants both held), the X sheathe as ref
  600 (a non-combat area: the game has no sheathe outside one) until a draw starts.
- items.rs BEH_A_ITEM_USE map from the HKS: 30 / 34 ItemPowderMedicineRecoverPoison, 80 / 81
  PaperDollExchangeWhite, 84 ItemAntiHallucinogen.
- Result: the sim suite passes with SHINOBI_HKS=1 as without (698 / 0).
- Cost: ~190 us a tick for Wolf (quick profile), ~97 % of it in the VM (SHINOBI_HKS_TRACE=1
  prints it every 600 ticks): ~1 % of a 60 Hz frame. Enemies on it will need the exe's LOD.
- SHINOBI_HKS_TRACE: 1 engine events, overrides and the cost; 2 both clips every frame; 3 the
  presses and the script's events.

## 2026-10-20 Enemies live (SHINOBI_HKS=1)

`src/hks_enemy.rs`: the enemy answers moved out of the lockstep test (`answers`, which the test
now calls) and `HksEnemyPlugin` runs one `ScriptChr` per enemy (c9997.hks + c<anim_chr>.hks on
its behbnd's c9997.hkx) after `ActorSet::Decide`, at the exe's character LOD (`Actor::lod_due`,
dt x lod). Where the clips differ the graph's state and clip win, as Wolf's (both standing,
moving or turning, or enemy.rs's clip ending this step: enemy.rs keeps its own);
SHINOBI_HKS_ENEMY_LEAD=0 only counts. The lockstep tests take the runner out of their app.
- The sandbox partner's walk (enemy.rs passive / drill, ours) reads as an AI command: a forward
  walk (no turn). Its attacks and any anim forced without an AI request (`force_anim`) go in as
  the engine's W_Event<n>.
- Held by its map script (enemy.rs Mode::Held at 0 bars in its 20200), or dead with its map
  script playing events on the body (Isshin's 21000 before the finisher): the graph stands
  aside. gap: how the exe keeps c9997's ExecDeath (IsDead at HP 0) off then.
- A state enemy.rs started outside the AI's requests (map events, throw ends, test pokes) goes in
  as W_Event<anim> / W_ThrowAtk / W_ThrowDef <anim> / W_<state>; none: the graph stands aside
  till the two meet. The AI's own request is left to the script (it may take a frame).
- The same clip from another anim set (a100 / a200) counts as the same. gap: where the exe's
  offsetType 15 takes another set than Actor::anim_group.
- Cost ~87 us a tick an enemy. SHINOBI_HKS_TRACE=1 prints the differing frames and a summary
  every 600 frames; =3 the hits and the script's events.
- Sim suite with live enemies: 9.6k differing frames (from 188k at the first run): c1000 (no
  clips) 3.7k, boss event flows (c5000-c5400, c7100-c7110) most of the rest. Regular enemies left:
  c1010 / c1020 WalkFrontBattle where the game plays TransToBattleFrom* (the alert transition),
  c1020 Guard where the game idles.
- With the graph leading the sim suite passes (698 / 0); 1449 frames still differ where it does
  not lead (both free, enemy.rs holding an ended deathblow clip, c5000 turning, c1020's Guard
  where the script stays idle).

## 2026-10-20 Default on

The scripts run by default (playtested); SHINOBI_HKS=0 turns both runners off (player.rs and
enemy.rs alone). Next: delete the hand-written picking the graph now overrides, in small tested
batches.

## 2026-10-20 Deleting the hand-written picks

- **Audit.** `SHINOBI_HKS_AUDIT=<file>` appends one line per player.rs pick: the call site
  (`Actor::played_at`, `#[track_caller]` on `play` / `play_state`), player.rs's state and clip, the
  graph's state and clip after its tick, and whether the two clips match (set-only differences
  count as the same). Over the suite: jumps, item use, jump arts and several air moves match every
  time; deathblows, throws, revivals, the grapple shot, idols and swimming never do (engine
  states); attack combos match about 90 %, knockdowns about half.
- **Graph-led clip ends** (`hks_wolf::graph_chains`): where player.rs's clip has played out and
  the graph already went on by itself, tick_script takes the graph's state before player.rs
  decides (take_graph, as sync_clip does after it). Deleted from player.rs: the ground / sprint /
  storm jumps' ready -> start and start -> fall, the jump arts' ready -> leap -> fall loop, the
  slide's end. The side jumps keep their raw anims (no graph clip). With SHINOBI_HKS=0 those
  chains no longer run.
- **Shared clips.** Several CMSGs can hold one clip generator (VerticalGroundJumpFall and
  HorizontalWallJumpFall share a000_201030's): hkb_runtime::owner names it by the active CMSG.
- **Not yet: knockdowns.** The posture-break reactions start in player.rs and have no event in
  the graph (it stands aside), so their chains stay. The Ape grab's face-down landing is the
  script's own (c0000_transition 386-395 sets Selector_ProneDirection at the grab's end; the land
  state is STATE_TYPE_REACTION_DEATH_START, which the 3036 ref-512 check does not touch): it needs
  the script to end the grab rather than combat.rs's grab_end. Hits through the script come first.

