# The boss brain

A pluggable "boss brain" for an enemy with a Lua battle goal: his high-level choices can come from his real Lua AI (the default) or from an outside controller, including a hosted model.
Four characters have action tables: the Great Shinobi Owl (c5060), the Ashina soldier with a one-handed sword (c1010), castle Genichiro Ashina (c7100), and his Way of Tomoe phase (c7110).
The code is in `crates/sim/src/brain/`, with a small hook in the Lua goal runtime (`crates/sim/src/lua_ai/runtime.rs`, `ActHook`).

```powershell
cargo run --release -p sekiro-game -- --arena owl --brain scripted --brain-log
cargo run --release -p sekiro-game -- --play --enemy soldier --brain scripted --brain-memory on
cargo run --release -p sekiro-sim --bin duel-sim -- --enemy soldier --brain scripted --brain-log --steps 1800
cargo test -p sekiro-sim --test it -- owl_brain:: soldier_brain:: brain_tactics::
```

`--brain coach` runs a CLI coach with a free local checkpoint chooser by default (see "The coach" and "Local checkpoint chooser"); `--brain lua` (the default) leaves his Lua AI unchanged; `--brain scripted` runs the deterministic test controller with that enemy's own test plans; `--brain-log` writes a JSONL log to `cache/brain_logs/`; `--brain-memory off|on|<path>` keeps the memory of every fight (on by default for hosted brains, see "Memory across fights").
`--force-act <id>` (the enemy requests that act whenever he is free) works for either enemy.

Three environment variables tune the driver (no flags): `SEKIRO_BRAIN_MODE=slot|plan_end|checkpoints` (when requests go out, see "Timing model"; the default is `checkpoints` for hosted brains and `slot` for the others), `SEKIRO_BRAIN_TEMPERATURE` (sampling temperature, default 1.0) and `SEKIRO_BRAIN_SEED` (seeds the sampler; random otherwise).

The baseline to compare a model against is the Lua brain with memory on: `--brain lua --brain-memory on` keeps `logs/memory/<chr>.lua.boss.json` and `<chr>.lua.player-default.json` with the same per-fight metrics ("The score to beat"), apart from the model's boss knowledge and player profiles.

## Recording overlay and stage presets

`--brain-overlay` starts a recording panel at the top right; it is off by default and F8 toggles it during play.
While the panel is visible, the debug state and key-help readout is hidden, leaving the combat bars visible.
The fixed-width dark panel wraps text, uses the HUD's warm colours and Windows Segoe UI, and moves the floating boss bars beside it if they would overlap.
The panel shows the controller and coach model, fight number (including the snapshot's earlier fights), selected player, active set and description, running plan with its current step in brackets, and the current move's label.
Lua choices display `Lua`.
The last checkpoint shows its reason, set switch or keep decision, and age.
Accepted coach notes appear as a highlighted pre-, mid- or post-fight card for eight seconds; boss and player principles remain beneath it, and `coach thinking…` shows while a coach request is pending.
The last ban appears for five seconds, including the driver's actual whiff or failure evidence.
A fight's scorecard appears for six seconds with the winner, time to kill Wolf, Wolf's posture breaks, deflect rate and mean break conversion delay.
The view reads driver state directly and works without `--brain-log`.

`--brain-stage <name>` selects one of these before/after presets:

| Stage | Controller | Snapshot |
|---|---|---|
| `lua` | Lua, no memory | None |
| `early` | Coach | `logs/datasets/owl-2026-10-11-golden/c5060.json` |
| `alive` | Coach | `logs/datasets/owl-2026-10-11-alive/c5060.json` |
| `outsmarted` | Coach | `logs/datasets/owl-2026-10-11-outsmarted/c5060.json` |

The stage overrides `--brain` and `--brain-memory`, so even an explicit normal memory path cannot receive stage writes.
The coach stages require Owl and fail clearly if their dataset is absent or invalid.
Each run copies only the snapshot into a unique `%TEMP%\sekiro-brain-stage-...` directory, validates and migrates that copy into boss/player layers, and prints the directory before play.
All subsequent memory writes use that temporary directory; `logs/memory/` and the source dataset stay untouched.
The temporary copy is retained after exit for inspection.
`--player <name>` selects the profile name and seeds it with the snapshot's habits and history, without duplicating its boss evidence.
`--coach`, `--coach-model`, `--coach-effort` and `--coach-interval` still configure the coach normally.

PowerShell 5.1, from the repo root, using the normal release executable after it contains these changes:

```powershell
.\target\release\sekiro-rs.exe --arena owl --progression castle-roof --brain-stage lua --brain-overlay
.\target\release\sekiro-rs.exe --arena owl --progression castle-roof --brain-stage early --brain-overlay --coach codex --coach-model gpt-6.1-sol
.\target\release\sekiro-rs.exe --arena owl --progression castle-roof --brain-stage alive --brain-overlay --coach codex --coach-model gpt-6.1-sol
.\target\release\sekiro-rs.exe --arena owl --progression castle-roof --brain-stage outsmarted --brain-overlay --coach codex --coach-model gpt-6.1-sol
```

This session built only `target/alt/release/sekiro-rs.exe`; substitute `target\alt\release` in those commands to use the verified build without replacing the executable in use.
`--window-size 1920x1080` selects the recording window size; the default remains 1600x900, and 1280x720 was also visually checked.
The minimum accepted size is 800x600.
All visual checks used `--brain scripted --brain-memory off`, with no model calls.
The raw input replay `crates/game/scripts/brain_overlay_toggle.txt` checks F8 off/on/off; use it with `--input`, the scripted brain and repository-local `--shots` paths.

## Design

The controller sets the **plan**, the game keeps the **reflexes**.

- A plan is 1 to 3 steps run in order as one act. A step is one of the boss's own action ids (the animation ids his Lua acts request) or a *tactic*: a movement run through the engine's own native goals (waiting, guarding, walking or running to a distance from Wolf, strafing, backing off, keeping a range; see "Tactics").
  A reply can be a *plan set*, a weighted list of plans the boss samples from locally, so one answer serves many acts.
- Reflexes stay with the Lua goal runtime and keep working while a plan runs. For the Owl: his `Goal.Parry` (guard 3100, deflect 3101, special deflect 3102, back step 5201), shuriken reactions, the deflect counters (`Kengeki_Activate`), punishing a heal, turning to a Wolf behind him. For the soldier: `Common_Parry` (guard, deflect, back step), his deflect counters (`Kengeki_Activate`, 3050 to 3076) and his answers to Wolf lying down. For both, everything their HKS script does (staggers, getting up).
- The observation is fair and mirrored: only what the player can see of the boss, and the same about Wolf.
- Outcome memory lives in our code, because hosted models keep nothing between calls: every request carries this fight's tally and the memory of every fight with Wolf's habits, which is also kept on disk across sessions.
- At most one request is in flight, sent when the request mode says so (every slot, the end of a plan, or only at checkpoints); if no answer is ready when an act starts, the Lua AI's own choice runs (the boss never waits).
- Once either fighter is dead nothing is requested or logged until the next fight starts.

### Where a plan enters his AI

The Owl's battle goal (`GOAL_NingunOsa_506001_Battle`, `506001_battle.lua`) and the soldier's (`GOAL_Ochimusha_katate_101000_Battle`, `101000_battle.lua`) do, on every activation:

1. reactions first: `Kengeki_Activate` (counters after being deflected) returns early, and `Common_ActivateAct` may queue a reaction (a heal punish, a step away);
2. weights for each of his acts by distance and state;
3. `Common_Battle_Activate(ai, goal, weights, acts, after, params)`, the shared helper that picks one act and runs it.

`AiBrain::enable_act_hook` keeps the script's `Common_Battle_Activate` under another global name and puts a host function in its place.
That host function is a *slot*: if a plan is waiting and the moment is not a reaction, it queues the plan on the battle goal instead of calling the script's pick; otherwise it calls the script's own function unchanged.
A moment is a reaction when the script already queued subgoals in step 1, or when it weighted one of the boss's *reflex acts*:

- the Owl: Act21, which turns to punish a heal or to face a Wolf behind him, and Act40, his answer to SpEffect 110125;
- the soldier: Act23 (strafe), Act25 (back away) and Act27 (wait and strafe), which his script weighs before any distance band, when Wolf is down (SpEffect 109031), on SpEffects 3170200 and 3101540, or with no path to Wolf.

A deflect counter bypasses the normal plan slot because `Kengeki_Activate` returns first.
Its shared helper now lets the driver reweight only the callbacks Lua offered, as described below.
The soldier's interrupt also replans when Wolf goes down (SpEffect 109031, after his leaping slash 3003 for example), so a plan that knocks Wolf down is cut short by his own back step or retreat (Act24, Act25), as one of his own acts would be.

A plan runs exactly as one of his acts would:

- an approach with the scripts' own `Approach_Act_Flex`, to the distance of the act that opens with that action (less his hit radius, as his acts do), evaluated when that attack step runs, after earlier tactics;
- a single attack as `AttackTunableSpin`; a chain as `ComboAttackTunableSpin`, `ComboRepeat`, ..., `ComboFinal`; a step as `SpinStep`;
- each attack sets the AI number its act sets: for the Owl, number 5, his pressure score read by `Kengeki_Activate`; for the soldier, number 5 is set to 1 by his rushes (3010, 3011), which his script then stops weighing from afar.

So animations, TAE cancel windows, the combo-cancel envs and reactions go through the same goals and the same native `CommonAttack` as his own choices.
When a reflex interrupts a plan (a `Parry` clears the battle goal's subgoals and queues a guard), the rest of the plan is dropped, as with his own acts.
The `deflect` readiness tactic gives the same parry callback a local queue, so its guard/deflect response finishes that tactic and preserves the following attack.
Damage, observed-effect replanning and other reactions still run normally.

With `--brain lua` the hook is still installed but never has a plan, and the script runs unchanged; `lua_brain_leaves_his_choices_unchanged` (in both `owl_brain.rs` and `soldier_brain.rs`) checks that a fight takes the same action sequence with and without it.

## The observation

One JSON object per request, about 450 to 500 bytes, always under 1 KB.
Numbers are rounded to two decimals, angles to whole degrees.

| Field | Meaning |
|---|---|
| `t` | Session clock, seconds |
| `boss`, `wolf` | One object per side, below |
| `dist` | Horizontal distance, metres |
| `dy` | Wolf's height above the boss, metres |
| `wolf_bearing` | Where Wolf is from the boss's facing, degrees, positive to the left |
| `boss_bearing` | Where the boss is from Wolf's facing, degrees, positive to the left |
| `locked_on` | Wolf is locked on to the boss |
| `since_boss_hit` | Seconds since the boss last landed an unguarded hit on Wolf (`null`: never, this fight) |
| `since_wolf_hit` | Seconds since Wolf last landed an unguarded hit on the boss |
| `plan` | The plan running now, in its text form such as `walk_in4>3013` (left out while the Lua AI's own act runs) |

Each side:

| Field | Meaning |
|---|---|
| `hp` | HP left, fraction of max |
| `posture` | Posture damage built up, fraction of max (the bar the game shows: 0 calm, 1 broken) |
| `lives` | Deathblow marks left for the boss (2 at the start for the Owl, 1 for the soldier); Wolf's own (1: resurrection is not modelled) |
| `state` | Leaf name of the behaviour state playing, such as `Attack3005`, `GroundAttackCombo1Release`, `StandDamageMiddle` |
| `act` | The boss only: his current action id (attack, step or guard animation) |
| `attacking` | Wolf: his clip is in the window the game signals to enemies as an incoming attack (TAE flag 63); the boss: an attack action plays |
| `guarding` | In a guard (TAE flag 3) |
| `airborne` | Not on the ground |
| `staggered` | In a damage or recoil reaction (behaviour state group `*Damage*`) |
| `deathblow_open` | Can be deathblown now (posture broken, or 1 HP with marks left) |

There are no item counts, AI numbers, params, cool-downs or other hidden state.

## The actions

Each boss has a `BossActions` table (`brain::actions`): his name for the instructions, the action ids with labels, approach distances and AI numbers, his reflex acts, the most actions per plan (3), named combos offered to hosted models as one choice, his back steps (for the habits) and the scripted controller's test plans.
`BossActions::for_chr` picks the table by character id; `Duel::set_brain` refuses an enemy without one.

### The Great Shinobi Owl (c5060)

`brain::actions::OWL`; approach distances and pressure come from the act in `506001_battle.lua` that uses each id, descriptions from his TAE and AtkParam rows (`docs/OWL.md`).

| Id | What | Approach (m) | Kind |
|---|---|---:|---|
| 3000 | Kesa slash, combo opener | 4.8 | Attack |
| 3001 | Second slash | 5.8 | Attack |
| 3002 | Third slash, combo finisher | 4.8 | Attack |
| 3003 | Jumping forward roll, slash down | 10.7 | Attack |
| 3005 | Front kick (posture only) | 2.8 | Attack |
| 3009 | Shuriken | 9.7 | Attack |
| 3011 | Rushing slash | 11 | Attack |
| 3014 | Second shuriken | 9.7 | Attack |
| 3015 | Rushing slash curving left | 9 | Attack |
| 3018 | Shuriken, then a dash slash | 11 | Attack |
| 3031 | Recovery-sealing throw | 5.5 | Attack |
| 3032 | Recovery-sealing throw follow-up | 9 | Attack |
| 3063 | Rushing follow-up, two hits | 4.8 | Attack |
| 5201 | Back step | | Step |
| 5202 | Side step (his act picks it with Wolf on his right) | | Step |
| 5203 | Side step (his act picks it with Wolf on his left) | | Step |

Firecrackers (3010) use Act07's 3.5 m approach and set pressure number 5 to zero.
The other tool acts 3017 (explosive slash), 3030 (poison balls) and 3069 (smoke screen) are in the vocabulary since their bullets work (`docs/OWL.md`, "Tools"); their standalone approach distances are ours, from where their bullets land.
Their scripts use them as counter/reaction follow-ups without a separate approach, so those standalone choices retain the documented approximation.
Left out on purpose: warps (3035 to 3047; the transparency is not rendered), grabs (3037, 3040, 3041; they now throw Wolf, `docs/OWL.md` "Grabs", but are not in the vocabulary yet), phase-2 acts other than the poison balls 3030, the perilous sweeps of Act32 and Act33, and the deflect counters (3060 to 3066 with the firecracker counter 3064, 3090, 3091), which stay reactions; the smoke screen 3069 is offered as an act.
Combo: `3000+3001+3002` (three-slash combo).
Scripted test plans: the kick 3005, then the recovery-sealing throw 3031 once two kicks were deflected.

### The Ashina soldier (c1010)

`brain::actions::SOLDIER`, from his battle goal 101000 (`101000_battle.lua` in his AI bundle `m11_00_00_00.luabnd`, NpcThinkParam 10100000).
Approach distances are the base distance of the act that uses each id (his acts add 2 m when Wolf is outside his front 120 degrees, `karaburiDist`; a plan does not); labels from his BehaviorParam rows (the judge ids in his TAE, `2101000xx`) and AtkParam rows (`101001xx`, `101002xx`).

| Id | What | Approach (m) | Act | AI number |
|---|---|---:|---|---|
| 3000 | Sweeping slash, combo opener | 2.5 | Act01 | |
| 3001 | Second sweeping slash (follows 3000) | 2.5 | Act01 | |
| 3002 | Running sweeping slash | 7 | Act02 | |
| 3003 | Leaping slash | 7.1 | Act03 | |
| 3005 | Kick (posture only, 1 HP) | 3.7 | Act05 | |
| 3007 | Rising slash (follows the kick) | 3.7 | Act05 | |
| 3008 | Flurry of three slashes | 4.7 | Act06 | |
| 3009 | Stone throw | none | Act07 | |
| 3010 | Pouncing leap (1 HP) | 5 | Act08 | 5 = 1 |
| 3011 | Running lariat, then a pounce (1 HP) | 7.6 | Act09 | 5 = 1 |
| 3012 | Left sweep, then an overhead slam | 3.5 | Act11 | |
| 3013 | Perilous thrust (cannot be guarded; deflect or counter it) | 5 | Act14 | |
| 3014 | Perilous low sweep (cannot be guarded or deflected; jump) | 3.5 | Act15 | |
| 5201 | Back step | | Act24 | |
| 5211 | Short back step (his act uses it within 4 m) | | Act24 | |

- 3001 and 3007 only follow 3000 and 3005 in his acts; as openers they use their act's distance (ours).
- The stone throw's act never approaches, so neither does a plan opening with it.
- Left out: 3004 and 3006 (Act04) and the side steps 5202 and 5203 (Act22), which his script names but his TAE (`c1010.tae`) has no clips for (the script is shared with other soldier variants); 3013 toward an event point (Act10) and 3030 (Act12, a dodge used only in the combat tutorial); the deflect counters (3050 to 3076, `Kengeki01` to `Kengeki08`), which stay reactions.
- Combos: `3000+3001` (both sweeping slashes, his Act01) and `3005+3007` (kick, then rising slash, his Act05).
- Scripted test plans: both sweeping slashes, then the kick and rising slash once two openers were deflected.

A plan with an unknown id (for the soldier also an id his script names without a clip, such as 3004), an unknown tactic, more than 3 steps or none is refused and logged, and the Lua AI picks that act.

### Genichiro Ashina (c7100)

`brain::actions::GENICHIRO` controls castle row 71001000, think 71000000, using `GOAL_Rival_710000_Battle` in `m11_01_00_00.luabnd.d/710000_battle.lua`.
Distances and chains were read from that compiled Lua's act instructions; labels and guard restrictions come from `c7100.tae`, BehaviorParam `271000xxx` and AtkParam_Npc `71000xxx`.
Every offered id has its own TAE and a decoded clip `a000_<six-digit id>`.

| Id | What | Approach (m) | Lua function |
|---|---|---:|---|
| 3000 | Right kesa slash, opener | 3.6 | Act01 |
| 3001 | Left kesa slash | 3.6 | Act01 |
| 3002 | Left elbow | 3.6 | Act01 |
| 3003 | Right body slash, finisher | 3.6 | Act01, Act31 |
| 3010 | Two spinning slashes | 3.6 | Act01 |
| 3025 | Perilous thrust, deflectable and Mikiri-able | 3.6 | Act01 |
| 3004 | Right then left body slashes | 3.2 | Act02 |
| 3005 | Draw slash | 2.2 | Act03 |
| 3006 | Advancing two slashes after sheathing the bow | 4.8 | Act10 |
| 3007 | Perilous low sweep from left step, jump it | 5.9 | Act05, Act34 |
| 3016 | Running draw slash | 5.2 | Act06 |
| 3009 | Bow shot, bullet judge 500 | none | Act30 |
| 3044 | Jumping four-arrow volley, bullet judges 530 to 533 | none | Act30, Act24 |
| 3011 | Bow shot after the sweep, bullet judge 500 | 5.9 | Act34 |
| 3014 | Braking bow shot, bullet judge 500 | 8.9 | Act15 |
| 3015 | Diving slash after braking bow shot | 8.9 | Act15 |
| 3040 | Floating Passage, first four slashes | 4.5 | Act09 |
| 3041 | Floating Passage, final three slashes | 4.5 | Act09 |
| 3037 | Circling slash | 3.2 | Act11 |
| 3045 | Roundhouse kick after body slash | 3.6 | Act31 |
| 3062 | Perilous back-step thrust, deflectable and Mikiri-able | 3.2 | Act20 |
| 5201 | Back step | | Act24 |
| 5202, 5203 | Right / left side step | | Act22 |

Act15 sets AI number 7 to 1, suppressing its forced opening bow sequence; 3014 and 3015 retain that assignment when chosen separately.
Act24 sets number 2 to 1; the back step retains it.
Other offered attacks set no AI number.
Follow-ups offered as independent openers inherit their parent act's approach, as with the other tables.
Act30 never approaches, so neither 3009 nor its volley follow-up does.
Plans keep the generic 9999 success range and three-step cap instead of the Lua's distance-dependent branch exits and four-step Act01 branch.

Named combos are `3000+3001+3002`, `3001+3010+3025`, `3014+3015`, `3040+3041`, `3009+3044`, `3003+3045` and `3007+3011`.
Act01's alternate branch is four actions (`3000+3001+3010+3025`), so the offered thrust chain is its final three actions, respecting the existing cap.
The first combo chains at TAE flag 23 windows 0.900 and 0.900 seconds, producing nominal hits at 0.667, 1.667 and 2.067 seconds.
The seven-hit Floating Passage sheet uses the same combo-aware timing and AtkParam posture resolver as Owl.
Projectile rows retain clip timing/cancels but have no melee hit/reach entries, as with Owl's shuriken.
There are 24 actions, seven combos, seventeen tactics and fifteen curated chains, 64 choices including `lua`.
Scripted plans start with the kesa slashes and elbow, then switch to `3009+3044` after two deflected openers.

Excluded from planning:

- 3020 and 3021 are grab starts, not ordinary sword hits; Act11's 3037 opener remains available alone.
- 3022 (Act16) is the downed-Wolf punish, and Kengeki counters remain Lua reflexes; 3062 is offered because Act20 explicitly selects it outside Kengeki.
- 3013 (Act48) is the predictive braking arrow variant, using TAE 3013's imported 3014 clip and bullet judge 540; target prediction is not implemented, so the ordinary Act15 variant is offered.
- Lightning bow 3039 and lightning jump slashes 3042/3043 belong to the separate castle Tomoe table `brain::actions::TOMOE` (`c7110:71100000`).
  Castle Tomoe's separate `711000_battle.lua` defines 3039 in Act13/Act17 (no approach), 3043 in Act18 (8.3 m), and 3042 in Act19 (3.5 m); those functions are absent from c7100's battle goal.
  c7110 now offers 3039 (no approach), 3043 (8.3 m) and 3042 (3.5 m); c7100 has no matching lightning attack rows and keeps excluding them.
  The automatic map hand-over retains the brain and switches to the full c7110 table within the same fight.
- Registered Act07/08/18/19/40/41/42/45/46/47 have no function definition in this castle battle goal, so no attack or approach is inferred from their names.
  No offered action lacks a TAE clip; missing ids such as 3033 and 3047 are not inferred into the vocabulary.

Act21 turns toward Wolf, Act28 handles Wolf's throw/down effects, and Act16 punishes downed Wolf; those slots stay reflexes.
`Kengeki_Activate` returns before the planning slot, and `Goal.Parry`, `Damaged`, `ShootReaction` and observed-effect interrupts remain automatic.
Act23/25/26/27/28 provide strafe, leave, wait and approach goals; his graph and parry callback support guard, deflect readiness and back steps, so all seventeen existing tactics are offered.
The posture-pressure checkpoints, local posture-break response, plan-set validation and six-second attack pacing policy use the unchanged generic driver.

Hosted memory is `c7100.boss.json` and `c7100.player-<name>.json`; Lua/scripted baselines retain their own suffixes.
It cannot collide with c5060 or c1010 unless a caller explicitly supplies the same JSON stem, which is a user-selected override.
At the second deathblow the map hand-over retains the driver and loads castle c7110 row 71100000 with its own 711000 Lua battle goal and phase-specific brain table.
Tomoe therefore runs Lua, with no c7100 plans or brain memory applied to him; resetting after that hand-over currently requires reinstalling the brain through a new game run.

```powershell
$env:CARGO_TARGET_DIR='target/alt'
$env:CARGO_BUILD_JOBS='6'
target/alt/release/sekiro-rs.exe --arena genichiro --brain scripted --brain-memory off --brain-overlay --god
cargo test --offline --release -p sekiro-sim -j 6 --test it -- genichiro_brain::
```

The same table is selected by `--brain coach|decisions|responses` and the menu's AI choice through the existing character lookup.
Verification uses scripted controllers, temporary memory and existing offline provider/CLI fakes only.

## Tactics

A tactic is a plan step that is not one of his animations: it runs through one of the engine's own native goals (`NativeGoal` in `lua_ai/runtime.rs`), the same goals his Lua acts use for approaching, waiting and backing away, so there is no movement code of ours.
Distances are metres from Wolf, centre to centre (the observation's `dist`), never map coordinates.
Each tactic's goal has a lifetime cap, and a goal queued by a plan *succeeds* when that lifetime ends (`Goal::timed`), so a tactic that cannot reach its distance in time ends anyway and the rest of the plan runs; a tactic already satisfied (he is already inside the band) ends at once.

| Token | What | Native goal (id) | Parameters | Cap |
|---|---|---|---|---:|
| `hold1`, `hold2`, `hold3.5` | Stand still facing Wolf | Wait (2000) | target Wolf | 1, 2, 3.5 s |
| `guard1`, `guard2.5` | Hold his guard | Guard (2101) | action 3100, target Wolf | 1, 2.5 s |
| `deflect` | Face Wolf ready for the battle script's guard or deflect response to TAE flag 63 | Local plan wrapper around his Lua parry callback and its own goals | Script rank, facing, weapon reach, interval and consecutive-guard rules | 1.5 s |
| `dodge_back` | Step back when Wolf's attack wind-up is visible within 2.5 m | Local plan wrapper requesting a back-step action | Soldier 5211 (his short step), Owl 5201 | 1.5 s |
| `walk_in6`, `walk_in4`, `walk_in2.5` | Walk toward Wolf until within that distance | MoveToSomewhere (2019), walking | distance | 5 s |
| `run_in3` | Run at Wolf until within 3 m | MoveToSomewhere (2019), running | distance | 4 s |
| `strafe_left`, `strafe_right` | Walk around Wolf, facing him (left and right are the boss's own) | SidewayMove (2017) | 1 left, 0 right | 2 s |
| `back_off6`, `back_off9` | Walk backwards, facing Wolf, until that far away | LeaveTarget (2016), walking | distance | 4 s |
| `retreat12` | Turn away and run until 12 m away | LeaveTarget (2016), running, facing away | distance | 5 s |
| `keep_range` | Hold 4 to 6 m from Wolf, moving as needed | KeepDist (2018), walking | 4 to 6 m | 2.5 s |

- Walking is move level 0.5 and running 1.0 (the NPC script runs above 0.75).
- Two things the native goals did not do and now do for a plan only: `KeepDist` holds its range until the lifetime ends instead of succeeding the moment it is inside it, and `LeaveTarget` can face the way he runs (`retreat12`; his other leaves face Wolf). Both are flags on the goal (`Goal::timed`, `Goal::away`) set by `queue_act`; his own acts are unchanged (`lua_brain_leaves_his_choices_unchanged`).
- An attack that follows a tactic gets its own approach (`Approach_Act_Flex`) when its action has an approach distance, since a tactic can leave him far away; one that opens the plan gets it as before.
- Tactics are exchanges in the tally under their token (see "The tally"): `n`, and `punished` when Wolf hit him during one. The sampler penalises a tactic Wolf keeps punishing.
- Both tables offer all seventeen tactics.
  Their movement speeds come from each one's own clips (the Owl backs off and strafes slowly, the soldier walks about 1 m/s and runs only a little faster over a short distance).

`deflect` does not request an unconditional perfect deflect or extend an animation's deflect window.
The soldier's `Common_Parry(ai, goal, 50, 25, 0, 3102)` checks rank SpEffect 221002, facing, weapon reach and a 0.1 s interval.
Ordinary incoming slashes request guard 3100 unless the script's roll is within consecutive-guard count times 50, in which case it requests deflect 3101.
The Owl uses his own `Goal.Parry` rules.
Their HKS and TAE choose the guard judge and active effects; `CombatRules::resolve` compares the guard's data-derived repel defence with the attack's repel power to decide block versus deflect.
Parry goal 2113 remains an ordinary action goal; the battle scripts actually request EndureAttack/CommonAttack for these reflexes.

Soldier chains: `deflect>3000+3001`, `deflect>3005+3007`, `guard1>deflect>3008`, `dodge_back>3002`, `dodge_back>3014`.
Owl chains: `deflect>3000+3001`, `deflect>3005`, `guard1>deflect>3000`, `dodge_back>3003`, `dodge_back>3011`.
Both tactics contribute their 1.5 s caps to the anti-stall lead time and require an attack in a plan set; `dodge_back` also consumes the eight-second retreat allowance.

### The choice list

`BossActions::choices()` is what a hosted model is offered, and what `BossActions::parse` accepts: his actions, his named combos, each tactic, and a curated list of two and three step plans (`BossActions::chains`, validated by the tests), then `lua`.
The text form joins steps with `>` and consecutive attacks with `+` (they run as one combo chain), for example `walk_in4>hold2>3013`, `3005+3007`, `strafe_left>3005+3007`.
The soldier has 15 actions, 2 combos, 17 tactics and 45 chains, 80 choices with `lua`; the Owl 20 actions, 1 combo, 17 tactics and 43 chains, 82 with `lua` (the Decisions API takes up to 255).
The chains are, for both bosses, variations of: walk in slowly to a band, then an attack; hold still, then a running or leaping attack; back off or retreat, then a ranged attack or a rush (stone throw, shuriken); strafe, then a combo or a kick and rising slash; guard, then a combo; run in, then a combo; keep range, then a perilous attack; an attack, then a back off or strafe; and a few three step baits (retreat, run in, leaping slash; walk in, hold, perilous thrust; strafe left, strafe right, thrust).
Run `cargo test -p sekiro-brain-net both_bosses_offer_tactics -- --nocapture` to print the counts.

## The tally

`brain::Tally` keeps one *exchange* per action of each side in the current fight: the boss's attack and step animations (3000 to 3099, 5200 to 5299), and each of Wolf's attacks from the step his clip raises the parry-timing flag (63) until the clip changes, keyed by its behaviour state name shortened for requests (`GroundAttackCombo1Release` becomes `AttackCombo1`: the `Ground` prefix and `Release` are dropped).
An exchange stays open for a punish window of 1 s after its clip ends, then counts.

| Count | Meaning |
|---|---|
| `n` | Exchanges |
| `hit` | One of its hits landed unguarded |
| `blocked` | The other side guarded one of its hits |
| `deflected` | The other side deflected one of its hits |
| `whiffed` | An attack contacted nothing (dodged, out of range, interrupted before contact); tactics and back steps cannot whiff |
| `punished` | Its side took an unguarded hit during it or within 1 s after |

Each flag counts at most once per exchange; delayed projectiles retain their firing animation and credit its open or retained exchange.
`n` includes tactic and back-step exchanges, so it is not a count of plan uses or attacks alone.
Older saved counts included non-attacking tactics as whiffs; existing memory files are left untouched, so historical totals still include those false whiffs.
The request carries this fight's counts per key (`tally.boss`, `tally.wolf`, most used first) as arrays `[n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]`; the same counts over every fight are in `memory.boss` and `memory.wolf`, so a controller can notice "the player deflects 3005" and "the player has adapted" (this fight differs from all fights).

```json
{"boss":{"3005":[8,7,0,0,1,2],"3031":[1,0,0,0,1,0]},
 "wolf":{"AttackCombo2":[1,1,0,0,0,1]}}
```

A fight runs from the start, or a respawn, until either side dies (when the duel schedules its respawn; a boss with deathblow marks left gets up from a deathblow and fights on).
Then the tally and the habits go into the memory, and the next fight starts with a fresh tally.

## Memory across fights

`--player <name>` selects a profile in the game (default `default`); names are 1 to 48 letters, digits, underscores or hyphens.
The two layers live beside the legacy memory path:

- `logs/memory/<chr>.boss.json`, version 1, holds shared `evidence` (fights, wins, action/plan/break-response outcome counts), `players` (count-bearing contribution snapshots), `principles`, reasoned `bans`, and `best` (sanitized game plan, evidence snapshot, score and last result).
- `logs/memory/<chr>.player-<name>.json`, version 5, holds this player's habits, recent history/trend, raw outcome counts, discounted `attack_learning`, contextual `move_bandit`, and coach plans, hypotheses and retained lessons.

```json
{"version":1,"chr":"c5060","evidence":{"fights":21,"boss_wins":20,"wolf_wins":1,"actions":{"3010":[9,0,0,0,9,0]},"plans":{},"break_plans":{}},"players":{},"principles":["Strike immediately after a posture break"],"bans":[{"choice":"3010","reason":"shared: 9/9 whiffs"}],"best":null}
```

This abbreviated boss example omits the contribution snapshots and the full best plan.
`best.plan` contains the opener, sets, start set and `on_wolf_break`, with player notes, principles, avoids and retries removed.
The best candidate prefers wins, faster wins, successful break conversions and damage efficiency; stalled or void fights cannot improve a demonstrated plan.
During migration the newest legacy plan seeds `best`, using the last legacy result as a proxy because old files do not associate each historical plan with its result.
Legacy unclassified principles stay in the default player profile; subsequent coach replies explicitly promote universal lessons.

On first load, `<chr>.json` is copied byte-for-byte to `<chr>.v2-backup.json` without overwriting an existing backup, then split into boss knowledge and `player-default`.
The original is also retained and never written again.
Action, plan, break-response, win and fight totals are preserved exactly.
Migration is idempotent and also happens when the first selected player has another name.
Invalid layers disable persistence with a warning rather than overwriting the source.

A new player inherits `best.plan` at fight 1 once shared knowledge has at least 20 fights; otherwise the Lua baseline plays.
Existing players resume their own latest validated plan.
Only an explicit, reasoned coach `boss_bans` entry creates a shared ban, applying to every player and offered chain containing the action.
Automatic all-whiff and counter-rate bans are retired; generated legacy reasons beginning `shared: ` are ignored and removed when contributions are reconciled.
Player avoids and retries remain scoped to the selected player; a player retry cannot reopen a shared coach ban.
Shared principles merge newest first, deduplicate case-insensitively and keep six.

Coach JSON sends `boss_knowledge` first, `player_profile` second, then the current fight evidence.
The shared object contains counts, player/fight totals, principles, bans and a compact best plan/result; it contains no habits, history or player notes.
The player object contains fight count, habits, trend, notes, `player_principles`, local outcomes and bans/failures.
The response remains a full game plan, now with `player_principles`, `boss_principles` and `boss_bans` arrays.
Strict hosted schemas require the arrays (use an empty `boss_bans` for no update); older replies using `principles` still deserialize as player principles.
Both layers and current fight evidence share the 4096-byte budget, trimming low-priority rows before retained shared lessons and exclusions.
The local checkpoint chooser and optional Decisions chooser use the same offered sets.

Saves replace each file through a temporary file at fight end, respawn, 15-second autosave and exit.
Contribution snapshots replace each player's earlier evidence rather than adding it again, so repeated saves cannot inflate totals.
Autosave retains provisional activity only in the player profile; shared learning uses committed active fights, and load reconciles a durable profile snapshot after an interrupted save.
Saving a different profile preserves the contribution snapshots already on disk.
`--brain-memory off` keeps session-only memory; `on` uses the default folder; an explicit JSON path supplies the base stem, and any other path supplies the folder.
Lua and scripted baselines retain their separate `<chr>.lua` or `<chr>.scripted` stems for both layers.
`duel-sim` uses the default player through the same driver path.

The player file format, version 2 (an example after two short soldier fights in `soldier_brain.rs`, rounded; `history` is new in version 2):

```json
{
  "version": 2,
  "chr": "c1010",
  "fights": 2,
  "boss_wins": 1,
  "wolf_wins": 0,
  "boss": {"3000": [18, 1, 0, 0, 17, 0], "3008": [1, 0, 0, 0, 1, 0]},
  "wolf": {"AttackCombo1": [14, 2, 12, 0, 0, 0], "EasyDeflectedAtk": [20, 0, 7, 13, 0, 0]},
  "habits": {
    "seconds": 30.7,
    "range_seconds": [29.2, 1.5, 0.0],
    "attack_dist": {"sum": 41.9, "n": 36},
    "after": {"guarded": {"n": 32, "to": {"attack": 32}, "attack_dist": {"sum": 29.4, "n": 32}}},
    "answers": {"3000": {"n": 18, "to": {"hit": 1, "none": 17}}, "3008": {"n": 1, "to": {"none": 1}}},
    "punish": {"3000": {"sum": 13.1, "n": 15}},
    "heals": {"hp": {"sum": 0.0, "n": 0}, "at_low": 0, "low_spells": 1},
    "jumps": 0
  }
}
```

A version 1 file loads (its `history` is empty) and is saved as version 4 from then on (`memory_v1_migrates_and_keeps_ten_fights`).
`history` holds the last 10 finished fights, oldest first, each as `{"winner", "metrics", "decisions", "lua_acts", "requests", "choices", "tokens", "latency_ms", "brain"}` (see "The score to beat"); `boss` also holds tactic tokens next to action ids.
A missing file starts fresh with a note on stderr.
An unusable layer (bad JSON, another `version`, or another enemy's `chr`) disables persistence with a warning and keeps every existing file intact.
Every field but `version` and `chr` has a default, so a file from a build with fewer fields still loads.

### Recent per-attack evidence

`brain/adaptation.rs` records settled attack exchanges as phase-specific recent successes and severity-weighted failures.
Three observed uses are the safety minimum; mature rates are passed to the coach as `attack_rates[action] = [samples, success_share, failure_share]`.
The coach decides whether a move needs a different setup, a lower weight or an explicit exclusion, without waiting for 10/10 failures.
Raw ten-entry outcome arrays remain the audit trail and preserve the separate counter kinds.

| Outcome | Recent success | Recent failure |
|---|---:|---:|
| Clean unguarded hit | 1 | 0 |
| Actual Wolf HP/posture progress | +0.25 | 0 |
| Wolf posture break | +1 | 0 |
| Mikiri, sweep kick, other counter such as lightning return | 0 | 2 |
| Deflected contact, including mixed clips | 0 | 1 |
| Fully guarded attack with no clean hit | 0 | 0.6 |
| Whiff, including dodged perilous sweep, out of range or interruption | 0 | 1 |
| Boss HP/posture damage during the attack or its punishment window | 0 | +1.5 |
| Movement, guard tactic or back step without contact | No attack sample | No attack sample |

Flags coexist, so a hit followed by punishment teaches both success and failure, and guarded posture progress retains both its benefit and its guard failure.
Actual resolved posture damage is used rather than treating a guard as no progress.
`TallyEvent::Countered` supplies counter kinds, `Hit` supplies contacts, `Progress` supplies resolved damage/breaks, and `Damaged` includes Wolf's guarded posture damage to the boss.
`BrainDriver::settle` records the completed exchange and `attribute` also reports net-bad plan uses; fatal and fully deflected plan-use counts remain available to coach judgement.

Recent masses retain 0.9 per subsequent attack observation and 0.85 per completed active fight, without decaying raw audit counts.
The multiplier is `clamp((2*(success+0.5)/(success+failure+1))^3, 0.01, 2)` after three uses, otherwise 1.
Chains use their weakest attack multiplier, and a failed opener is probabilistically skipped into the active set instead of repeating at every reset.
The selected player's full recent evidence is combined with other players at quarter strength, capped at one effective borrowed observation per attack; its own contribution is excluded from pooling.
The same signal reduces Lua's offered counter callback weights, including whiffs and guards that the previous counter-only formula ignored.
Only explicit coach zero weights, avoids or shared bans veto a callback; outcome evidence keeps a 1% multiplier floor.
Mandatory single-candidate Lua branches and the existing all-zero fallback remain Lua-owned, so the floor is a relative weight, not a guarantee of alternatives that Lua never offered.

Version 4 adds defaulted recent maps to player profiles and contribution snapshots while shared boss files remain version 1.
Older counts seed at most six effective own observations, without inventing counter kinds, temporal ordering, damage or break flags.
Six-, seven- and ten-entry outcome arrays remain readable; migration preserves raw counts and is idempotent.
New fight records and session reports retain per-phase, per-action outcomes for each fight, closing the historical aggregate-only reporting gap.
See [BRAIN-LEARNING.md](BRAIN-LEARNING.md) for the read-only evidence audit and offline experiment.

## Wolf's habits

`brain::habits` watches Wolf the way the boss could (mirrored fairness): his position, behaviour state, guard and airborne flags, his HP bar and the hits, never his buttons.

| Habit | How it is measured |
|---|---|
| Attack distance | Distance when each of his attacks starts (when his swing is signalled, TAE flag 63) |
| Range | Seconds close (under 3 m), mid (3 to 6 m) and far (over 6 m) |
| After a trigger | His first response within 1.2 s after `deflect` (he deflected a boss hit), `guard` (he guarded one) and `guarded` (the boss guarded or deflected his hit), and within 2 s of the start of a boss `back_step` (5201 for the Owl, 5201 and 5211 for the soldier): `attack` (with its distance), `dodge`, `jump`, `heal`, else at the window's end `guard` (still guarding), `back` (1.5 m further away) or `wait` |
| Answers per boss action | The first hit of the exchange: `deflect`, `guard`, `hit` (the boss's hit landed), `counter` (Wolf's unguarded hit landed first); without one, `dodge` or `jump` if he did, else `none` |
| Punish speed per boss action | Seconds from the action's end to his next attack, if within 1.5 s |
| Heals | HP at each heal (an `Item*` state), heals below half HP, and how often his HP fell below half |
| Jumps | Jumps started (a `*Jump*` state, or airborne outside a damage state) |

Dodges are `*Step*` states other than `*FromStep*` and attacks; these state rules are ours, from the names in `c0000.hks`.

The request carries a compact summary (`HabitSummary`): only entries with at least 2 samples, most sampled first, rounded.

```json
"habits":{"attack_m":3.0,"range_pct":[66,34,0],
 "after":{"back_step":"attack 100% from 3.0m n10","deflect":"guard 100% n10"},
 "vs":{"3000":"deflect 100% n10","3005":"dodge 50%, jump 50% n2"},
 "punish_s":{"5201":0.4},"jumps_pm":1.6}
```

(from `habits_from_a_scripted_wolf`: a Wolf who always attacks from 3 m 0.4 s after the soldier's back step and holds his guard after deflecting).

## The request and its size

```json
{"id":12,"why":"pressure","obs":{...},"tally":{"boss":{...},"wolf":{...}},
 "memory":{"fights":2,"boss_wins":1,"wolf_wins":0,"boss":{...},"wolf":{...},"habits":{...},
           "trend":{"defl":[100,100,80],"guard":[0,0,10],"hits":[0,1,2],"hp":[0,7,12],"kill_s":[38,31,null],"now":[67,1]}}}
```

`why` is the checkpoint that made the request (`start`, `pressure`, `pressure_critical`, `half_hp` (fallback), `mark_lost`, `punished_twice`, `deflected_thrice`); it is left out in the other modes.
`memory` counts the earlier fights (`fights`, `boss_wins`, `wolf_wins`; the rest were unfinished) and sums the counts and habits over every fight, this one included.
`memory.trend` is Wolf's play over the last 3 finished fights, oldest first (see "The score to beat"): perfect-deflect percent (`defl`), guard percent, hits the boss landed, his HP lost in percent of max, and seconds he took to kill the boss (`null`: the boss won); `now` is this fight so far, `[deflect percent, hits landed]`.
The whole request is kept within 1400 bytes of compact JSON (`REQUEST_BUDGET`): `BrainRequest::fit` drops the least telling entries (fewest samples) until it fits, in this order, each map down to 4, 3, 2, 1 and 0 entries: `memory.boss`, `memory.wolf`, the habits' `punish_s`, `vs` and `after`, then `tally.wolf` and `tally.boss`; last the single habit values, the older fights of the trend and the memory itself.
Measured (`the_request_with_a_trend_and_tactics_stays_under_the_budget`, `requests_fit_the_budget`, `memory_survives_a_death_and_a_new_session`): a typical request with one or two earlier fights is 760 to 970 bytes and needs no trimming; a worst case (123 fights, every soldier action, five tactics and every habit filled, ten recorded fights, the widest observation) is 2829 bytes before and 1392 after, keeping the trend and the habits' single values.
`BrainRequest::excluded` (the plans repelled three times in a row) is not part of the input: the Responses request leaves them out of its choices.

## Timing model

Per duel step (`BrainDriver`):

1. Poll the controller.
   An answer is a plan set (one plan is a set of one).
   The driver samples the next plan from it (see "Choosing plans"), checks it against the table and hands it to the act hook, where it waits for the next slot.
2. The Lua AI thinks.
   At a slot the waiting plan is queued, unless the moment is a reaction (then it keeps waiting); with no plan waiting, the Lua AI's own pick runs.
3. A request goes out according to the **request mode** (`BrainDriver::mode`, `SEKIRO_BRAIN_MODE`).
   At most one is in flight, and an event that arrives while one is out waits for it:
   - `slot` (the default for `scripted` and other brains; today's behaviour): after any slot, so the controller thinks while the current act runs. The answer is used for one plan.
   - `plan_end`: when the last step of the running plan starts, so a fresh answer is ready as the plan ends; also at slots where the Lua AI acted. The answer is used for one plan.
   - `checkpoints` (the default for hosted brains): `start`, `pressure` at about 6 estimated Wolf hits remaining, `pressure_critical` at about 3 (only with at least 2 left), `half_hp` as an HP fallback, `mark_lost`, `punished_twice` and `deflected_thrice`.
     Both posture checkpoints re-arm when remaining posture recovers above 70%; growing damage crossing both thresholds prefers critical, with no delayed pressure event.
     HP fallback fires once per fight, lost marks once per mark, and the hit streak events once per fight.
     Between requests the set persists and plans are sampled at each slot; the request count depends on posture recovery cycles.
4. After the step, action changes and hits feed the tally, the habits and the metrics; a death ends the fight, a respawn starts the next.

A single-shot plan (`slot`, `plan_end`) whose answer is older than `max_staleness` (6 s) when it would run is dropped and a fresh request goes out; a set's plan is picked when its slot nears, so it does not go stale.
A reply that is `lua`, a failure or a timeout leaves the act to the Lua AI; in `checkpoints` nothing is retried until the next event.
Before the first answer arrives (the start request takes a fraction of a second), his own AI acts (`lua_acts_while_no_answer_comes_in_checkpoints_mode`).
A reply to a request of an earlier fight is dropped.

`ScriptedBrain` models its latency on the fight clock, so tests are deterministic: with a 4 s delay (longer than his acts) most slots fall back to the Lua AI, and the Owl is never idle longer than his own AI makes him (`slow_brain_falls_back_to_his_lua_ai_without_freezing`); a controller that never answers leaves every act to the soldier's AI (`lua_picks_when_no_answer_comes`).

## Local checkpoint chooser

The checkpoint chooser switches sets; the contextual move fighter described below selects plans inside those sets.
`--brain-moves off` restores the previous persistent-evidence sampler for comparison.

`--brain coach` defaults to `--checkpoint local` with a Codex or Claude CLI coach, so both plan execution and checkpoint selection work without an API key.
`--checkpoint decisions` explicitly selects the existing OpenAI Decisions chooser and requires a key.
`--checkpoint off` keeps the coach's start set; direct `--brain decisions` and `--brain responses` remain available.
The start menu's coach option selects local checkpoints.
The local path reads no credential source, creates no HTTP agent and performs no DNS warmup or network request.

Contexts are the eleven existing checkpoint kinds: start, Wolf posture 25/50/75/broken, boss pressure/critical pressure, half HP, mark lost, punished twice, and deflected thrice.
Distance and habit buckets are deliberately omitted to keep evidence dense.
Arms are exactly the validated plan sets offered by `GamePlan::offers()` to the Decisions request; selecting the active set is equivalent to `keep`, without a duplicate arm.
Normal plan sampling, bans, reflexes, checkpoint suppression and anti-stalling limits still apply.
Each arm draws from `Beta(success + 4*p, failure + 4*(1-p))` and the largest draw wins, using the driver's seeded generator.
The draw is multiplied by the set's coach-weighted mean per-attack multiplier, including its chains, so a countered move lowers a set even outside the reward window.
Ordinary plan sampling and Lua counter callbacks consume the same attack signal independently of checkpoints.
Unseen arms draw from the coach prior, so they retain exploration probability alongside established arms.

The coach may supply `checkpoint_prefs: {"wolf_posture_75": {"finisher": 0.8, "bait": 0.2}}`.
Each number is a prior reward mean, clamped to 0.05..0.95 for sampling, with four total pseudo-counts.
Missing preferences give `p = 0.8` for a set whose `when` matches the checkpoint, otherwise `p = 0.5`; explicit preferences override the hint.
Unknown checkpoints, unknown surviving sets, non-finite numbers and values outside 0..1 are dropped with warnings.
Old plans remain valid without this field.
Strict coach output expresses the same preferences as `checkpoint_prefs: [{"checkpoint":"wolf_posture_75", "set":"finisher", "weight":0.8}]`, or an empty array for uniform priors.
Both representations deserialize into the same map; saved plans use the map.
The coach must translate actionable notes and player principles into plan weights, avoids/retries, counter weights and checkpoint preferences or hints.
Those structured fields execute its judgement on the local path; free prose is retained for reasoning and display, not interpreted with keyword heuristics.
The Decisions checkpoint brief includes bounded coach reasoning alongside seven scalar fields and set descriptions (see "Decisions request").

Reward measures cumulative metric deltas from selection until the next checkpoint, four seconds, phase hand-over, or fight end, whichever comes first.
All HP and posture damage terms are fractions of a full bar, accumulated without refunding recovery or healing.
The formula is `clamp(Wolf posture damage + 2*Wolf HP damage + 0.5*Wolf breaks - boss HP damage - boss posture damage - 0.25*successful Wolf counters - 0.2*fully deflected plan uses, 0, 1)`.
Fully deflected uses come from the driver's existing plan-failure classification, not from a single deflected hit.
A wholly passive window receives zero; neither elapsed time nor survival adds reward.
If the finished fight exceeds the existing 35% passivity limit, all its window rewards become zero before learning is committed.
Activity checks prevent an inactive player from teaching the bandit.
Within-fight evidence is provisional and only committed when the finished fight passes the existing void-fight checks.
The four-second window can miss a late counter, and zero reward cannot distinguish degrees of failure or a lack of progress.
Persistent attack evidence below updates when each exchange settles, including outside those windows, and directly affects plans, counters and checkpoint quality.

Player memory is version 4 and keeps `bandit.<phase>.<checkpoint>.<set> = {"success": ..., "failure": ...}` beside phase-specific `attack_learning`.
Version 1 through 3 profiles load with retained raw counts, bounded attack priors and defaults for missing bandit fields.
The shared boss file retains version 1 and adds the same defaulted map to each player's contribution snapshot and aggregated evidence.
Repeated saves replace contributions instead of counting them again.
Own evidence has full weight; other players' snapshots contribute at quarter strength only while choosing, and borrowed counts never enter the selected player's profile.
Pooled transfer is capped at four counts per arm so a long shared history cannot overwhelm learning about the selected player.
Each non-void completed fight discounts old own evidence by 0.98 before adding its new rewards; coach pseudo-counts are never decayed or stored as evidence.
`--brain-memory off` learns only within the session.

The overlay shows checkpoint context, chosen set and posterior mean.
The session report lists choices, posterior means and settled rewards.
Coach reviews receive a compact `checkpoints.<phase/context>.<set> = [rewarded choices, mean reward]` summary for the current fight inside the 8192-byte input budget.
Local choices consume zero model tokens and zero billed API calls.

## Choosing plans

The driver samples every plan locally, from the controller's plan set, so variety does not need network calls (`brain/sampler.rs`).
A Decisions answer lists a probability for each choice; the set is those probabilities (values that are not a valid plan are dropped, `lua` stays as the option to leave the act to his AI).
Each act, one plan is drawn with weight

`(probability * attack_multiplier) ^ (1 / temperature) * max(0.01, 0.5 ^ streak) * 1.5 if untried`

- `temperature` is `BrainDriver::temperature` (`SEKIRO_BRAIN_TEMPERATURE`, default 1.0; below 1 sharpens, above flattens, 0 or less always takes the heaviest).
- `streak` sums over the plan's steps consecutive net-bad exchanges: counters, deflects, fully guarded attacks, whiffs or punishment outweighing hits/progress; a tactic is repelled when Wolf hits him during it.
  Keys are action ids and tactic tokens, counted per fight, with a 1% streak multiplier floor for sampled distributions.
- *untried*: the plan's text was not run yet this fight.
- The generator is a seeded xorshift (`SEKIRO_BRAIN_SEED`, `BrainDriver::seed`), so tests are deterministic.
- A set of one (the Responses backend, or an answer without probabilities) is not sampled: it is dropped for the Lua AI's act once any of its steps was repelled 3 times in a row. The Responses request also leaves such plans out of its choices (`BrainRequest::excluded`), so the model picks another; its answer stays the argmax.

Tests: `sampling_follows_the_probabilities_and_is_seeded`, `repeated_deflects_push_an_option_down_and_untried_ones_get_chosen`.

## The score to beat

Per fight (from its start or a respawn until a death) the driver computes how well Wolf did, as numbers the controller is asked to make worse (`brain/metrics.rs`):

| Metric | Meaning |
|---|---|
| `deflected`, `guarded`, `hits_taken` | How each of the boss's hits on Wolf was resolved: a deflect, a guard that was not a deflect, an unguarded hit that landed |
| perfect-deflect rate | `deflected / (deflected + guarded + hits_taken)` |
| guard rate | `guarded` over the same total |
| `hp_lost` | The boss's damage to Wolf as a fraction of his max HP, summed over drops (healing does not take it back) |
| `posture_gained` | His posture damage to Wolf as a fraction of the bar, summed over rises |
| `punishes` | The boss's actions (attacks and tactics) Wolf hit during or within a second after them (the tally's `punished`) |
| `kill_time` | Seconds Wolf took to kill the boss, when he did |

The last 10 fights are kept in the memory file (`history`, below) with the choices made, so the Lua baseline and a model can be compared by reading two files.
Every request carries the trend of the last 3 fights and this fight so far, and the instructions tell the controller its goal: win faster, break Wolf posture and land its follow-up, deal more damage per minute and take less damage; probe with untried options, drop what keeps being deflected or guarded, bait his habits.
The same metrics are computed and saved when the Lua brain fights (`--brain lua --brain-memory on`, file `<chr>.lua.json`; `a_fight_summary_and_the_baseline_are_saved_with_the_metrics`).

At the end of each fight the driver prints one line to the console and writes the same record to the log (`fight_summary`):

```
fight over: boss won in 38.2s (decisions/checkpoints); 14 plans, 3 lua acts, 3 requests [walk_in4>3013x4 hold2>3002x3 ...]; wolf deflect 62% guard 8% hits taken 4 hp lost 61% posture 35% punishes 2; tokens 2310/9; latency p50 420 ms p90 610 ms
```

(winner, duration, brain and mode, plans run and Lua acts, requests, the six most run choices, Wolf's metrics, tokens and the request-to-answer latency in fight time.)

## The log

With `--brain-log`, one file per session: `cache/brain_logs/<unix ms>-<chr>-<brain>.jsonl`.

| `ev` | Fields |
|---|---|
| `start` | `brain`, `mode`, `chr`, `max_plan`, `choices` (value and label of every offered plan) |
| `request` | `t`, `request` (`id`, `why`, `obs`, `tally`, `memory`): exactly what the controller saw |
| `reply` | `t`, `id`, `plan` (the best choice, or `null`), `options` (size of the plan set), `latency` |
| `slot` | `t`, `by` (`plan`, `lua`, `reflex`), `plan` (text form) |
| `outcome` | `t`, `side`, `key` (an action id, a tactic token or a Wolf attack), `start`, `end`, `hit`, `blocked`, `deflected`, `punished`, `whiffed` |
| `fight_end` | `t`, `winner` (`boss`, `wolf`, `none`), `fights` (in the memory now), `summary` (the console line) |
| `fight_summary` | `t`, `record` (the fight's record: metrics, choices histogram, tokens, latency p50 and p90) |
| `fight_start` | `t` (after a respawn) |
| `stale`, `invalid`, `late` | `t`, `id` or `plan` (and `error` for `invalid`) |

After `fight_end` and `fight_summary` nothing is logged (no request, reply, slot or outcome) until `fight_start`: once a fighter is dead there is nothing to decide, and the boss's idle moves are not recorded (test `nothing_is_requested_or_logged_once_a_fighter_is_dead`).

Requests and replies are enough to replay a controller's decisions offline; the slots and outcomes show what each decision led to.

## Adding a controller

Implement `brain::BossBrain`:

```rust
pub trait BossBrain {
    fn name(&self) -> &'static str;
    fn plans(&self) -> bool { true }
    fn default_mode(&self) -> RequestMode { RequestMode::Slot } // hosted brains: Checkpoints
    fn usage(&self) -> (u64, u64) { (0, 0) }                    // tokens in and out, for the fight summary
    fn submit(&mut self, request: &BrainRequest);
    fn poll(&mut self, now: f32) -> Option<BrainReply>;
}
```

- `submit` must not block; `poll` returns the answer to the request in flight once it is ready (with the request's `id`).
- A controller that needs time (a network call) owns a worker thread: `submit` sends the serialized request down a `std::sync::mpsc` channel, the worker does the call and sends back a `BrainReply` (`id`, the best `plan`, and optionally the whole weighted plan `set`), and `poll` does `try_recv`.
  The duel is not `Send` (the script VMs use `Rc`), but the controller's channel ends are, so the worker never touches the simulation.
- Add it to `BrainKind` and its `by_name` / `build(chr)` so `--brain <name>` picks it.

Another boss needs its own `BossActions` table (name, action ids, approach distances, AI numbers, reflex acts, combos, back steps, scripted test plans) and a `BossActions::for_chr` entry; the driver, observation, tally, habits and hosted requests are the same for every boss.

## Hosted model controller

`sekiro-brain-net` implements `HostedBrain` with Decisions and Responses backends.
The game CLI builds a boxed `BossBrain` directly (`sekiro_brain_net::from_cli(name, model, chr)`, which offers the enemy's own `BossActions`), keeping `sekiro-sim` independent of HTTP and credentials.
Only the worker sends HTTP: `submit` serializes compact request JSON and uses an unbounded channel, while `poll` uses `try_recv` and returns the original request id.
One rustls `ureq` agent and its connection pool live for the fight.
Redirects and environment proxies are disabled.
The global per-request deadline defaults to 3 s (it was 1.5 s while every act waited for an answer; at checkpoints a late answer only delays the plan set, and the larger request, about 6 KB of instructions and 4 KB of choice descriptions plus the input, and the first TLS handshake make 1.5 s tight) and covers connection setup, headers and the response body; the live latency of the model is not verified.
When the worker starts it warms the route (`HostedConfig::warm_up`, on by default): it resolves the host and opens a TCP connection to it, then closes it, sending nothing, so no API request is made and the first decision skips the DNS lookup.
ureq 3 has no public way to pool a connection it did not open itself, so the TLS handshake still happens on the first real request.
On drop, the channel closes, queued work is discarded, and the worker finishes its in-flight call under that deadline before joining.

### Flags and configuration

```powershell
$env:CARGO_TARGET_DIR = 'target/codex'
cargo run --release -p sekiro-game -- --arena owl --brain decisions
cargo run --release -p sekiro-game -- --arena owl --brain responses --brain-model gpt-6-luna
cargo run --release -p sekiro-game -- --play --enemy soldier --brain decisions --brain-memory logs/memory
```

These commands authorize real calls when the player runs them; development tests make no provider API calls.
`lua`, `scripted`, `--force-act`, and the independent simulation `--brain-log` remain available.
Hosted brains remember every fight in shared boss knowledge and a selected player profile unless `--brain-memory off` is given (see "Memory across fights").
`--simple-ai` retains precedence over the boss controller.
`HostedConfig::from_env` checks `OPENAI_API_KEY` in the environment first, then only the `OPENAI_API_KEY=` setting in the working directory `.env`.
Surrounding single or double quotes on the dotenv value are accepted.
No real key or `.env` was inspected during implementation or tests.
`ApiKey` prints `***` in Debug; configuration diagnostics redact the key, and errors never include HTTP headers, bodies or transport details.
Authorization headers are marked sensitive.
The `log` dependency caps its macros at Debug at compile time because `ureq-proto` Trace dumps raw wire bytes, including credentials.
This disables Trace from libraries using `log`; the game's separate `tracing` macros are unaffected.
A missing key or controller startup failure prints a fixed diagnostic and falls back to the Lua brain without panicking.
Library callers can override `backend`, `model`, `base_url` (default `https://api.openai.com`), `timeout`, `log_path`, and `api_key`.
The default model is `gpt-6-luna` for both backends.

### Verified API contract (2026-10-10)

Sources: [Decisions create reference](https://developers.openai.com/api/reference/resources/decisions/methods/create), [Decisions guide](https://developers.openai.com/api/docs/guides/decisions), and [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create).
Decisions uses text `input` and a `questions` array; a choice question has 2 to 255 unique `choices[].value` values with optional descriptions.
Its response contains `model`, `answers` and `usage`; the named answer has type `choice` with a `choice` value and `probabilities` (one `{value, probability}` for every choice, plus a `confidence`), or type `refusal`.
The client keeps the probabilities as the plan set the driver samples from (`plan_set` in `protocol.rs`; values that are not a valid plan are dropped); the guide documents no temperature or log-probability option, so the temperature is applied locally to the probabilities.
Responses structured output is under `text.format`; its assistant message content contains `output_text` or `refusal` parts.
Both backends extract `usage.input_tokens` and `usage.output_tokens` when supplied, including usage on failed answers.
The docs establish these field names; account access and this model's acceptance of `none`, strict structured output, and a 16-token cap on Responses have not been live-verified.

Both requests use the same instructions and choices from the enemy's `BossActions::choices()` (his actions, named combos, the fifteen tactics and his curated chains, see "The choice list"), and `lua` to leave an act to his own AI.
The instructions name the enemy (`BossActions::name`), state the goal (make Wolf play worse: fewer perfect deflects and guards, more hits on him, a slower kill), tell the model to probe untried options, drop what keeps being deflected or guarded and bait habits, explain the input (observation, this fight's tally arrays, memory with the trend, `why`), say it is asked only a few times a fight and that the boss samples from its probabilities all fight (so spread them), and list every choice with its description.
Below are the complete request bodies for a tiny example boss (one attack, a back step, two tactics and one chain; the doc test builds it from the real request builder, `DOC_BOSS` in `protocol.rs`); a real boss lists 73 or 75 choices.
Actual `input` contains the compact serialization of `BrainRequest`.
Bodies are emitted as compact JSON; indentation here is for reading.

### Decisions request

In coach mode (`--checkpoint decisions`), input contains `why`, active `set`, both fighters' HP/posture fractions and `dist`, plus a `coach` context.
That context carries the active plan's notes, the current set's intent, the first two boss and player principles, the newest two retained review lessons, and up to three move controls ranked by confidence as `[plan, confidence, lock]`.
All offered sets' descriptions remain in the choices, including the current set's description.
The chooser follows the coach's plan and intent while adapting its set choice to the moment.
Coach context is capped at 768 serialized UTF-8 bytes, including JSON overhead and escaping; the brief size test deliberately permits 1000 bytes instead of 200 to accommodate it.
Initial field limits are 200 characters of notes, 100 of intent, 70 per principle and 90 per lesson.
If it still exceeds the byte cap, trim lowest-confidence controls first, then oldest lessons, lowest-ranked boss principles, lowest-ranked player principles, notes, and finally intent.
String trimming preserves UTF-8 boundaries; full histories and move tables stay with the coach.

`POST {base_url}/v1/decisions`

```json
{
  "input": "{\"id\":1,\"obs\":{},\"tally\":{},\"memory\":{}}",
  "model": "gpt-6-luna",
  "questions": [
    {
      "choices": [
        {
          "description": "front kick",
          "value": "3005"
        },
        {
          "description": "back step",
          "value": "5201"
        },
        {
          "description": "stand still facing Wolf for 2 s",
          "value": "hold2"
        },
        {
          "description": "walk slowly toward Wolf until 4 m away",
          "value": "walk_in4"
        },
        {
          "description": "walk slowly toward Wolf until 4 m away, then front kick",
          "value": "walk_in4>3005"
        },
        {
          "description": "leave it to his own AI",
          "value": "lua"
        }
      ],
      "instructions": "You are a doc example boss fighting Wolf. Reward order: win and win faster (time_to_kill_wolf, lower is better); then break Wolf posture and land a hit right after (wolf_posture_breaks, hits_after_break); then damage to Wolf per minute (HP plus posture); then take little damage (boss_damage_taken_per_min, HP plus posture) and reduce Wolf perfect deflects, each of which builds your posture. engaged_time is information and only a small tiebreaker when losing while landing hits; survival time is never a goal. Stalling, running away or passivity counts as failure: a fight where no attack of yours started in the last 4 s more than 35% of the time failed, whatever else happened, plans without an attack are ignored, you must start an attack at least every 6 s and may back off or retreat only once per 5 s. Retreat chains are allowed as baits when followed by an attack, including turning to punish a chase. Movement tactics exist to set up attacks (baits, spacing, timing mixups), not to avoid fighting. Think like a swordsman: turning your back or retreating while the opponent is in sword range leaves you open, so tactics should set up attacks safely. Probe instead of repeating: try untried options, drop what keeps being deflected or guarded, bait his habits (stand still briefly, walk in slowly, strafe, mix timings) and punish them. Input: obs is a fair view of both fighters (plan is what runs now); tally counts this fight per action or tactic as [n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]; memory holds every fight so far (the same counts, Wolf's habits, and trend: his deflect%, guard%, hits you landed, hp% lost, attacks per minute, damage per minute and passive percent, and kill seconds for information only, over the last fights, now = this fight so far); why is what triggered this question; break contexts include up to four viable_punishes rows [id,name,first seconds,seconds left,hits,window HP/posture,total HP/posture,stun_lock,guard posture after,recovery,evidence,conversion rate], with the same facts as the coach. You are asked only a few times a fight. The boss samples from your probabilities all fight, so spread them over several good options. Reflexes (guarding, deflecting, counters and reactions) are automatic. Choose lua to leave an act to your own AI. Options are single actions, movement tactics (distances from Wolf, left and right are yours) and plans of up to 3 steps joined with > (consecutive attacks joined with +):\n3005: front kick\n5201: back step\nhold2: stand still facing Wolf for 2 s\nwalk_in4: walk slowly toward Wolf until 4 m away\nwalk_in4>3005: walk slowly toward Wolf until 4 m away, then front kick",
      "name": "boss_action",
      "type": "choice"
    }
  ]
}
```

The client reads the one answer named `boss_action` and accepts `answers[0].choice`, such as `"3005"`.
A typed refusal is a failure, rather than an intentional `lua` choice.
The reply carries the chosen plan and the whole set (`BrainReply::set`, from `answers[0].probabilities`).

### Responses request

`POST {base_url}/v1/responses`

```json
{
  "input": [
    {
      "content": "{\"id\":1,\"obs\":{},\"tally\":{},\"memory\":{}}",
      "role": "user"
    }
  ],
  "instructions": "You are a doc example boss fighting Wolf. Reward order: win and win faster (time_to_kill_wolf, lower is better); then break Wolf posture and land a hit right after (wolf_posture_breaks, hits_after_break); then damage to Wolf per minute (HP plus posture); then take little damage (boss_damage_taken_per_min, HP plus posture) and reduce Wolf perfect deflects, each of which builds your posture. engaged_time is information and only a small tiebreaker when losing while landing hits; survival time is never a goal. Stalling, running away or passivity counts as failure: a fight where no attack of yours started in the last 4 s more than 35% of the time failed, whatever else happened, plans without an attack are ignored, you must start an attack at least every 6 s and may back off or retreat only once per 5 s. Retreat chains are allowed as baits when followed by an attack, including turning to punish a chase. Movement tactics exist to set up attacks (baits, spacing, timing mixups), not to avoid fighting. Think like a swordsman: turning your back or retreating while the opponent is in sword range leaves you open, so tactics should set up attacks safely. Probe instead of repeating: try untried options, drop what keeps being deflected or guarded, bait his habits (stand still briefly, walk in slowly, strafe, mix timings) and punish them. Input: obs is a fair view of both fighters (plan is what runs now); tally counts this fight per action or tactic as [n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]; memory holds every fight so far (the same counts, Wolf's habits, and trend: his deflect%, guard%, hits you landed, hp% lost, attacks per minute, damage per minute and passive percent, and kill seconds for information only, over the last fights, now = this fight so far); why is what triggered this question; break contexts include up to four viable_punishes rows [id,name,first seconds,seconds left,hits,window HP/posture,total HP/posture,stun_lock,guard posture after,recovery,evidence,conversion rate], with the same facts as the coach. You are asked only a few times a fight. The boss samples from your probabilities all fight, so spread them over several good options. Reflexes (guarding, deflecting, counters and reactions) are automatic. Choose lua to leave an act to your own AI. Options are single actions, movement tactics (distances from Wolf, left and right are yours) and plans of up to 3 steps joined with > (consecutive attacks joined with +):\n3005: front kick\n5201: back step\nhold2: stand still facing Wolf for 2 s\nwalk_in4: walk slowly toward Wolf until 4 m away\nwalk_in4>3005: walk slowly toward Wolf until 4 m away, then front kick",
  "max_output_tokens": 16,
  "model": "gpt-6-luna",
  "reasoning": {
    "effort": "none"
  },
  "store": false,
  "text": {
    "format": {
      "name": "boss_action",
      "schema": {
        "additionalProperties": false,
        "properties": {
          "action": {
            "enum": [
              "3005",
              "5201",
              "hold2",
              "walk_in4",
              "walk_in4>3005",
              "lua"
            ],
            "type": "string"
          }
        },
        "required": [
          "action"
        ],
        "type": "object"
      },
      "strict": true,
      "type": "json_schema"
    }
  }
}
```

The client requires a completed response with one assistant output text object, such as `{"action":"3000+3001+3002"}`.
Every step is checked against the boss's table (actions and tactic tokens), and plans longer than `max_plan` are refused.
The Responses request leaves out the plans the driver says were repelled three times in a row this fight (`BrainRequest::excluded`); the answer is a single plan with no probabilities.
`lua` returns `plan: None` and counts as answered.
Timeouts, HTTP failures, transport failures, refusals, malformed JSON, incomplete responses, invalid answers and invalid actions return `plan: None` and count by failure reason.
An unknown value is never copied into diagnostics or logs.
When an answer has not arrived at an act slot, `BrainDriver` runs the Lua choice without waiting; its existing staleness and reflex rules continue to apply.

### Hosted telemetry log

The game enables one separate JSONL file at `logs/brain-<unix nanoseconds>.jsonl` for either hosted backend.
The root whitelist `.gitignore` ignores `logs/`.
Each completed call appends and flushes one line:

```json
{"id":1,"backend":"decisions","model":"gpt-6-luna","round_trip_ms":42.5,"status":"answered","failure":null,"chosen":"3005","usage":{"input_tokens":450,"output_tokens":0}}
```

Failed calls have `status: "failed"`, `chosen: null`, and a failure such as `"timeout"`, `"refusal"`, `"unknown_action"`, or `{"http":429}`.
Logs contain no key, headers, raw provider payloads or observation; the separate simulation log remains available with `--brain-log`.
`HostedBrain::stats()` returns requests, answered calls, failures by reason, mean/max round-trip milliseconds, summed input/output tokens and log write errors.
Keep `stats_handle()` before boxing the controller to inspect the same stats later on the game thread.
Log write failures stop further logging and increment `log_errors` without discarding a valid plan.

### Offline verification

All HTTP tests bind a mock to `127.0.0.1:0` and supply a fake key.
They check both request bodies, action labels, ids, combo parsing, Lua answers, refusals, HTTP 429/500, malformed JSON, unknown ids, incomplete/invalid answers, usage totals, timing, redaction, nonblocking submit and bounded drop.
Two sequential Decisions requests are checked to reuse exactly one accepted TCP connection (with the warm-up off; `warm_up_connects_without_sending_anything` checks the warm-up connects and sends zero bytes).
A Decisions answer with `probabilities` becomes the reply's plan set (`a_decisions_answer_carries_its_probabilities_as_a_plan_set`), and a Responses request leaves out the plans the driver excluded (`a_responses_request_leaves_out_what_was_repelled_and_has_no_set`).
A unit test compares both complete request examples above against the actual request builder (for the small example boss).
Credential-source tests use in-memory dotenv text and never read the real environment key or `.env`.

## The coach

Coach input is now bounded to 8192 bytes so the lifetime move table and ten-fight manager review can survive alongside the played plan.
See "Contextual move fighter and cross-fight review" below for the current controls, digest and trimming policy.

Real-time per-move calls are wasteful, and the Decisions API cannot reason.
FromSoftware players fight the same enemy dozens of times, so the boss learns between fights and reviews during long fights.

- **Coach (between fights, full reasoning).** When a fight ends (a death, a win, a respawn that restarts it), the driver sends the coach a compact summary of it (`CoachInput` in `crates/sim/src/brain/coach.rs`, bounded to 8 KB, measured in `the_coach_input_is_bounded_and_keeps_the_notes_and_the_played_plan`).
  The coach writes a **game plan** for the next fight.
  It runs on its own thread in `sekiro-brain-net` (never the game thread), so latency does not matter: the plan is used from the next fight if it arrived by then (during the 6 s respawn), else from the fight after; a failed or empty answer keeps the previous plan.
- **Fighter (in the fight, cheap).** The driver runs the plan's sets locally with the sampler (`sampler.rs`, with the repelled-plan penalty and the untried bonus).
  At the checkpoints the local bandit picks which set to play; `--checkpoint decisions` uses the Decisions API and its `keep` option.
  No per-move calls.
- **Phases across fights.** Fight 1 for a new player uses the shared best plan once boss knowledge has at least 20 fights; with thinner knowledge it starts with his own Lua AI; the coach can review it mid-fight and runs after it.
  From fight 2 on the coach's plan drives him: the opener runs first, then plans sampled from the start set.
  The newest validated player plan is kept in the player profile (`coach.plan`, with `coach.plans`, the outcomes of every plan text), so a new session starts from it.
  No coach call is made at exit (the game would wait for it); the unfinished fight is saved in the memory as before.

```powershell
.\target\release\sekiro-rs.exe --arena owl --enemy soldier --brain coach --god
.\target\release\sekiro-rs.exe --play --enemy soldier --brain coach --coach codex
.\target\release\sekiro-rs.exe --play --enemy soldier --brain coach --coach api --coach-model gpt-6-luna --coach-effort low
```

| Flag | Meaning |
|---|---|
| `--brain coach` | The coach plus the local checkpoint chooser by default; `--checkpoint decisions` selects the API chooser. Memory is on by default (`logs/memory/<chr>.boss.json` and `<chr>.player-default.json`); `--brain-memory off` turns it off. `decisions` and `responses` stay as they were |
| `--coach claude\|codex\|api` | Who writes the plan. `codex` (default) uses `gpt-6.1-sol`, medium reasoning effort, ignores user config, and uses a read-only sandbox, ephemeral sessions and the output schema. `api` uses Responses with `gpt-6-luna`, low effort, strict schema, `store:false` and a 45 s deadline. `claude` uses `claude-haiku-5-5`, our replacement system prompt, empty settings and MCP configuration, safe mode, low effort, no tools or session persistence, and the JSON schema. Only `api` needs an API key |
| `--coach-model <name>` | Default `claude-haiku-5-5` (claude), `gpt-6.1-sol` (codex), `gpt-6-luna` (api) |
| `--coach-interval <secs>` | Mid-fight review interval, default 30 seconds; 0 disables all mid-fight triggers |
| `--coach-effort <level>` | `codex` and `api`: defaults `medium` and `low`, respectively |
| `SEKIRO_COACH_CMD` | Replaces the program of the `claude` and `codex` coaches (the tests point it at a fake) |

The CLI coaches run from their own temporary directory with no tools, are killed through their own child handle at the 120 s deadline and when the game exits, and never have their output echoed (it may contain the prompt).
A CLI that is not on PATH prints "the claude CLI was not found on PATH (...)" and the previous plan (or his own AI) stays.
The Claude Code envelope's `usage` and its `result` (or `structured_output`) are read; its dollar field is ignored for subscription usage.
Codex runs with `--json`: completed-turn usage events supply input, cached input, output and reported reasoning tokens, while the plan still comes from the `-o` last-message file.
The flags come from `claude --help` and `codex exec --help` (read on 2026-10-11; the tests never run the real CLIs, only a fake).
Without an `OPENAI_API_KEY` a CLI coach and the default local checkpoint switches both work.
On Windows the codex npm shim is tried as `codex.cmd`; killing it by its handle may leave its node child running until it ends.

### The game plan

Versioned JSON (`GamePlan`, version 2; versions 0/1 migrate), checked against the boss's `BossActions` (`GamePlan::validate`): unknown choices, plans and sets are dropped with a warning on the console, set names must be 1 to 24 of letters, digits, `_`, `-` (not `keep` or `lua`), at most 4 sets of 8 plans, notes at most 400 characters, the start set falls back to the first set, and a plan with nothing valid (no opener and no set) is ignored (his Lua AI plays).
Plans in sets must contain an attack and may not be in `avoid` (see "Hard limits"); the opener may be tactics only.

```json
{"version": 1,
 "opener": "hold3.5>walk_in4",
 "sets": [
  {"name": "press", "description": "keep attacking in combos; good while he is passive",
   "plans": [{"plan": "3000+3001", "weight": 2.0}, {"plan": "run_in3>3008", "weight": 1.0}], "when": null},
  {"name": "bait", "description": "slow walk-ins and holds before the thrust to break his timing",
   "plans": [{"plan": "walk_in4>hold1>3013", "weight": 1.0}, {"plan": "hold2>3002", "weight": 1.0}],
   "when": "pressure"}],
 "start_set": "press",
 "on_wolf_break": [{"plan":"3000", "weight":1.0}],
 "player_principles": ["this player rarely answers the kick"],
 "boss_principles": ["convert posture breaks with an immediate attack"],
 "boss_bans": [],
 "retry": [],
 "avoid": [{"choice": "3003", "reason": "deflected 5 of 5"}],
 "notes": "He deflects thrusts early but never the kick; he backs off after a deflect."}
```

The Responses request uses strict structured output (`text.format` `json_schema`, name `game_plan`) with every property required and the choice strings enum-constrained through one `$defs` entry; counts and lengths are validated after the answer.
Sources, read 2026-10-11: [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create), [Structured outputs guide](https://developers.openai.com/api/docs/guides/structured-outputs) (nullable fields as `["string", "null"]` with `null` in the enum), [Reasoning guide](https://developers.openai.com/api/docs/guides/reasoning) (`reasoning.effort`, reasoning tokens in `usage.output_tokens_details.reasoning_tokens`, `status: "incomplete"` with `incomplete_details.reason: "max_output_tokens"`).
Not live-verified: that the model accepts `$defs`/`$ref`, the null enum and `low` effort with this schema.
After a valid plan arrives the console shows 3 to 5 lines:

```
coach: fight 1 over, sent the fight summary (485 bytes); the new game plan plays from the next fight if it arrives during the respawn, else the one after
coach: game plan arrived after 3.1 s, played from fight 2 on (tokens 3100 in / 900 out, 700 reasoning, 14.2 s)
  opener: hold3.5>walk_in4
  sets: press [start] (2 plans: keep attacking in combos; good while he is passive); bait (2 plans: slow walk-ins and holds before the thrust to break his timing)
  avoid: 3003 (deflected 5 of 5)
  notes: He deflects thrusts early but never the kick; he backs off after a deflect.
coach: fight 2 plays the coach's game plan, start set press
```

The full plan goes to the brain log (`coach_plan` event, with the warnings and tokens; `coach_request` has the input, `coach_failed` the reason).

### What the coach is told

The input (user message) is the compact JSON of `CoachInput`: `fight`, `result` (won, secs, `defl`, `guard`, `hits`, `hp`, `posture`, `punishes`, `apm`, `dpm`, `passive`), `trend` (the last 3 fights), `plans` and `sets` (this fight, per plan text and per set of the plan played, as `[exchanges, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]`), `range` (this fight's measured melee spacing), `wolf` (his attacks), `overall` (every fight per plan text), `habits`, `played` (the plan the fight ran) and `notes` (the coach's own notes from last time, so its reasoning accumulates).
`range` maps each plan to `[mean attack-start distance, mean observed melee reach]` in metres, rounded to two decimals.
Each attack samples its own start and maximum active dummy-poly capsule envelope, including actual root travel, AtkParam capsule radius and Wolf's body radius.
Projectile-only attacks and attacks interrupted before any measurable melee window have no sample.
This is a radial envelope relative to the attack's starting centre: distance above reach indicates insufficient reach to Wolf's starting position, while distance below reach does not establish contact.
Direction, height, timing and Wolf's movement still matter; per-plan means are diagnostic summaries rather than individual-hit proofs.
The entries are paired with the surviving plan outcomes during budget trimming; range entries are the final fallback to trim.
Entries are trimmed (overall plans first, then Wolf's attacks, habits, this fight's plans, the oldest trend fights) to stay under 8192 bytes.
The instructions (`protocol::coach_instructions`, followed by every choice with its label, 80 or 82 lines) are:

> You are the coach of {name}, who fights the player (Wolf) again and again.
> After each fight you write the game plan for the next one.
> Reward order: win and win faster (time_to_kill_wolf, lower is better); then break Wolf posture and land a hit right after (wolf_posture_breaks, hits_after_break); then damage to Wolf per minute (HP plus posture); then take little damage (boss_damage_taken_per_min, HP plus posture) and reduce Wolf perfect deflects, each of which builds your posture.
> engaged_time is information and only a small tiebreaker when losing while landing hits; survival time is never a goal.
> Stalling, running away or passivity counts as failure: passive = the percent of the fight with no attack of yours started in the last 4 s, and a fight with passive above 35 failed regardless of every other number; if the last fight failed this way, make the next plan attack more.
> Retreat chains are allowed as baits when followed by an attack, including turning to punish a chase.
> Movement tactics exist to set up attacks (baits, spacing, timing mixups such as a hold or a slow walk-in before a strike), not to avoid fighting.
> Think like a swordsman: turning your back or retreating while the opponent is in sword range leaves you open, so tactics should set up attacks safely.
> The fighter enforces limits you cannot override: a plan in a set must contain an attack (plans without one are dropped), an attack must start at least every 6 s, and back_off, retreat or back-step plans run at most once per 5 s.
> Players learn patterns, so probe with untried tactics, exploit the habits you see and retire what keeps failing.
> Test one idea at a time and write down what you learn.\n\
> How combat works: Posture is the real bar: breaking it opens a deathblow; the deathblow spends one enemy life.
> Guarding costs the guarder posture; deflects cost the attacker posture and less to the deflector.
> HP loss slows posture recovery.
> Pressure only pays off while the enemy can still act; stop planning during a deathblow opening or stagger.
> Attack until deflected, then answer the counter.
> Jump and stomp sweeps, Mikiri thrusts (or deflect), and dodge grabs.

> Input (compact JSON about the fight just over): fight = fights so far; result = Wolf's result (won = who won, secs, defl/guard = Wolf's perfect-deflect and guard percent, hits = hits you landed, hp/posture = percent of his bars lost, punishes = your actions he hit, apm, dpm = percent of a bar per minute, passive percent); trend = his deflect%, guard%, hits, hp%, kill seconds (information only), apm, dpm and passive over the last fights; plans = this fight per plan text, as [exchanges, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other] (lua = your own AI's acts); sets = this fight per set of the game plan played: secs and the same counts (opener = the opener, lua = own AI); wolf = his attacks this fight, same counts; overall = every fight per plan text; habits = how Wolf behaves; played = the game plan this fight ran; notes = your own notes from last time. With no played plan this fight was your plain AI, the baseline.\n\
> Output: one game plan. opener = a plan run first in the fight (may be tactics such as hold3.5>walk_in4, or lua for none). sets = 2 to 4 named sets (name of letters, digits, _ or -, at most 24 characters), each with a one-line description for the fighter who picks the set (what it does and when it suits), 2 to 6 weighted plans from the choices below (weights relative), and when = the checkpoint it suits or null. start_set = the set played from the start. avoid = choices not to use, each with a short reason. notes = at most 400 characters: your hypotheses about this player, kept and shown to you next time so your reasoning accumulates; revise them with the new evidence. In the fight a cheap model only picks which set to switch to at the checkpoints (start, pressure, pressure_critical, half_hp (fallback), mark_lost, punished_twice, deflected_thrice, or keep), and your plans run locally, sampled by weight with a penalty for plans that keep being deflected, so spread weights over good options and make the sets clearly different. pressure means about 6 Wolf hits to break, pressure_critical about 3 but at least 2; both re-arm above 70% remaining posture. mark_lost only fires with lives still left; checkpoints wait until the boss can act. half_hp is the backup event at 50% HP left. Reflexes (guarding, deflecting, counters, reactions) are automatic. Use only the choices listed; lua leaves an act to the boss's own AI. A choice is a single action, a movement tactic (distances from Wolf, left and right are the boss's) or a plan of up to {max} steps joined with > (consecutive attacks joined with +). Choices:

The optional Decisions checkpoint chooser (`--checkpoint decisions`) gets only a few numbers and the set names, about 100 input tokens plus the choice list (the sets' descriptions and `keep`):

```json
{"model": "gpt-6-luna",
 "input": "{\"why\":\"pressure\",\"set\":\"bait\",\"boss_hp\":0.48,\"boss_posture\":0.55,\"wolf_hp\":0.91,\"wolf_posture\":0.12,\"dist\":3.4}",
 "questions": [{"type": "choice", "name": "plan_set",
   "instructions": "an Ashina soldier with a one-handed sword: win fast; break+hit; Wolf HP+posture/min; low boss HP+posture/min and Wolf deflects. Passive>35% fails. Retreat to attack.",
   "choices": [{"value": "press", "description": "..."}, {"value": "bait", "description": "... (suits: pressure)"}, {"value": "keep", "description": "stay on the current set"}]}]}
```

No memory, habits, tally or 73-choice list goes in; those are the coach's.
Without a game plan no checkpoint call is made at all.
`the_coach_mode_checkpoint_request_is_tiny` asserts the whole body stays under 1200 bytes with four sets at the longest description.
The legacy `--brain decisions` request is unchanged apart from its instructions.

### Hard limits (`brain/limits.rs`)

The goals prioritize winning faster, breaking Wolf posture and landing a hit after, dealing damage per minute, and taking little damage; a fight where no attack of his started in the last 4 s more than 35% of the time (`passive_share`) failed whatever else happened.
`FightMetrics` has `boss_attacks` and `passive_secs` (`attacks_per_min`, `damage_per_min` = HP plus posture in percent of a bar per minute, `passive_share`); `kill_time` stays as information.
They are in the scorecard line, the trend (`apm`, `dpm`, `passive`) and the coach input.
The driver enforces, for every hosted brain (`BrainDriver::anti_stall`), however the model answers:

- A plan in a set must contain an attack step (`has_attack`); the opener is exempt.
- After `MAX_PASSIVE_SECS` (6 s) without an attack starting, only plans that reach an attack within `OVERDUE_LEAD_SECS` (1 s of tactics) are eligible; an empty or Lua fallback instead requests an allowed attack from the boss's own table.
  A plan whose lead-in (the tactics' lifetime caps) could outlast the time left is not eligible earlier either.
- Guarding never resets the attack clock.
  Holding guard without a signalled Wolf attack for 0.5 s records `idle_guard` failure evidence, represented in compact outcome counts as a whiff for the responsible tactic, plan and set.
  After the 6 s attack deadline, an idle guard replans once through the existing act hook into an allowed attack, so an unstartable defensive plan cannot keep recycling guard.
- At most one plan with a `back_off`, `retreat` or back step per `RETREAT_INTERVAL_SECS` (5 s).

There is no rule against retreating beyond that rate: the tally scores tactics (punished when Wolf hits him during one) and the coach should learn from it.
The instructions also say to think like a swordsman: turning your back or retreating in sword range leaves you open.

### Facing before a swing

Every attack step of a plan turns him toward Wolf first when Wolf is more than `FACE_FIRST_DEG` (30 degrees) off his facing (`runtime::action_update`, for up to `FACE_FIRST_SECS` = 1.5 s).
His idle state does not turn (its TAE sets the turn speed to 0), so he steps toward Wolf and the behaviour graph plays its own turn state (`TurnBattle_Left180`), as the game's approach goals do.
Test `an_attack_after_retreat12_turns_to_face_wolf_first`: `retreat12>3005` with Wolf behind him, the kick starts with Wolf 0 degrees off his facing.

### Stats and cost

`HostedStats` counts per call kind: the checkpoint chooser (`requests`, `answered`, tokens, `reasoning_tokens`, round trip) and the coach (`coach.*`); telemetry records the backend and usage, with USD estimates only for billed API calls.
The fight summary line ends with the reasoning tokens, the coach calls that finished during the fight, and the seconds each set was played.
Reasoning tokens count against `max_output_tokens` (the reasoning guide recommends reserving at least 25000 while experimenting); the default here is 6000 for `low` and unverified, raise it if calls come back `incomplete`.
Estimate per fight with `--coach api`: one coach call of about 3 KB input plus the 6 KB instructions (3000 to 4000 input tokens) and 1000 to 3000 output tokens including reasoning, plus up to 5 chooser calls of about 250 input tokens each; with `--coach claude` or `codex` the coach runs on the subscription.
All prices are unverified; the first session's Decisions calls cost $0.01 at 4200 input tokens each.

## Inferred and ours

- The slot is `Common_Battle_Activate`: every table battle goal of this generation calls it once after weighting its acts (scripts); replacing its pick there keeps the reactions before it.
- The reflex acts (Act21, Act40) and what counts as a reaction (subgoals already queued) are our reading of `506001_battle.lua`.
- A plan's approach runs when the distance is more than 3 m beyond the target distance and has longer lifetimes than his acts (3 s walking, 5 s running, against his 1.5 s and 3 s): his script only picks an act at a distance that suits it, while a plan may start from anywhere.
- Plan chains use 9999 as every step's success distance (his acts sometimes use shorter ones to break off a combo when Wolf backs away).
- `staggered` from the behaviour state group name, and Wolf's attacks from TAE flag 63, are ours.
- The soldier's reflex acts (Act23, Act25, Act27) are our reading of `101000_battle.lua`: they are the acts his script weighs only as reactions, before any distance band.
- The habit rules (the windows, the range bands, `back` at 1.5 m, the state-name rules for dodges, jumps and heals, the first hit of an exchange as its answer) are ours.
- A fight ends when the duel schedules its respawn; an unfinished fight (the game closed) counts as a fight with no winner and no `history` record.
- The tactic distances, caps and speeds, the checkpoint events, the sampling weights (0.5 per repelled exchange in a row, 1.5 for untried, exclusion at 3), what counts as a repelled tactic, and the metric definitions are ours.
- Move levels: a plan walks at 0.5 and runs at 1.0, as the AI runtime does for his acts (the NPC script runs above 0.75); the soldier covers only about 1 m/s over a short run, so `run_in3` from 9 m does not reach 3 m within its cap.
- The warm-up opens a TCP connection only; the TLS handshake cost is not removed.


### Playtest verification (2026-10-11)

The API default and configurable effort are checked offline; strict output, store:false and usage including reasoning tokens are unchanged.
Fake CLI tests assert the complete ordered argument vectors, Claude's empty MCP file and its summary-only stdin.
Installed CLI help confirms all flags used; --exclude-dynamic-system-prompt-sections explicitly does nothing with --system-prompt and is omitted.
Codex --ignore-user-config removes user-configured MCP servers without relying on a possibly merged mcp_servers override.
No real API request or authenticated CLI execution was made, so provider latency and total context size remain unverified.

### Combat-aware checkpoints (2026-10-11)

See [COMBAT-FUNDAMENTALS.md](COMBAT-FUNDAMENTALS.md) for sourced base mechanics and the ranked fidelity gaps.
The coach now receives a compact combat paragraph covering posture, guard/deflect costs, HP-dependent recovery, actionable pressure and perilous counters.
`BrainDriver` estimates hits to break as remaining posture divided by Wolf's first combo hit's direct posture damage, using its real TAE attack judge, AtkParam, weapon/reinforcement, enemy defence and current effects through `CombatRules::wolf_posture_per_hit`.
This is a lead-time estimate, not a prediction of guarding or future deflects.
`PRESSURE_HITS = 6`, `PRESSURE_CRITICAL_HITS = 3`, `PRESSURE_MIN_HITS = 2` and `PRESSURE_REARM = 0.70` document our decision policy in `brain/driver.rs`.
Warnings fire once each per posture cycle while damage grows; recovery above 70% remaining re-arms them without raising warnings on recovery itself.
HP-half remains a fallback when no posture event is raised, including unavailable/zero damage estimates.
No checkpoint request is sent during an enemy deathblow opening, posture break, damage stagger, active deathblow, death or respawn.
Warnings wait through a brief damage stagger and are revalidated against the hit range before sending; deathblow openings, death and respawn discard transient warnings, while a surviving `mark_lost` waits for the get-up.
Only a deathblow leaving another mark raises `mark_lost`: the soldier never does, Owl does after his first.
The old `posture_half` and `posture_critical` names are replaced in the schema and hints; old saved hints are rejected by normal validation while their valid plans remain usable.
Offline real-data duel tests exercise both soldier posture cycles and actual soldier/Owl deathblows without a model call.

### Plan spacing and reactive tactics (2026-10-11)

The soldier table and Owl's standard attack entries match the base values in `101000_battle.lua` and `506001_battle.lua`; the boss radius was subtracted once, and NPC attacks already apply TAE 760 through `Character::attack_target`.
The audit also replaces Owl's approximate 4 m firecracker approach with Act07's actual 3.5 m and its pressure reset.
The range defect was evaluating `Approach_Act_Flex` when the whole plan was queued: its range check could skip an approach while close, before an earlier tactic moved the boss away.
A private deferred goal now calls that same helper when the attack step actually activates, without changing the attack clip, its lunge, input buffering, hit stop or Wolf's movement.

With a stationary Wolf initially 1.6 m away, the original runtime's `retreat12>3000` slash starts at 4.57 m, opens its first active window at 3.19 m and has a 4.29 m observed capsule/root-travel envelope from its starting centre; it whiffs.
The fixed runtime starts it at 2.02 m, opens the active window at 0.80 m and measures 3.92 m of reach from the starting centre; it contacts Wolf.
The six reported plans all contact stationary Wolf in the original runtime at 2.5, 4 and 6 m, so this replay does not reproduce their historical whiff rates.
At 6 m, first-active distances are 2.48 m (`walk_in2.5>3013`), 4.58 m (`keep_range>3013`), 3.98 m (`walk_in4>3013`), 4.58 m (`walk_in6>3013`), 1.10 m (`strafe_right>3005+3007`) and 0.80 m (`walk_in4>3000+3001`); the thrust's later hit window follows its lunge.

There was also a measurement defect: tactic exchanges with no outgoing contact incremented `whiffed` in the tally, per-plan/per-set summaries and outcome logs.
The offline 15 s `walk_in4>3000+3001` fight has four plans, eight landed attacks and four movement exchanges: the old per-plan count was `[12,8,0,0,4,0]`, and the corrected count is `[12,8,0,0,0,0]`.
Historical memory is not rewritten.

`brain_tactics::plan_attack_range_reproduction` records first-active distances, observed reach and contact for all thirteen soldier attacks plus nine mixed plans at 1.6, 2.5, 4 and 6 m.
`retreat_rechecks_range_before_a_close_slash` retains the actual range-whiff reproduction.
`reactive_tactics_answer_pressure_and_keep_the_attack_chain` drives Wolf's real HKS with attack presses every eight frames at 1.6 m against both bosses and checks script guard/deflect, timed back-step motion, the following attack request and idle caps.
The soldier's readiness finishes after 36 ticks under pressure, its short step gains 0.86 m and finishes after 38 ticks, and both idle waits finish after 90 ticks at 60 Hz.
`coach_reports_measured_attack_ranges_per_plan` checks spacing telemetry and excludes tactic whiffs; the worst-case coach-input test keeps the summary inside 8192 bytes with per-action and range entries.

### Five playtest findings (2026-10-11)

A fight is void when Wolf makes fewer than three attack, guard, dodge, jump or movement-stick presses during its first ten seconds, or holds any of those inputs for less than 40% of the fight.
The activity tracker observes the duel's raw InputFrame before deathblow consumption, independently of hit stop and animation input acceptance.
Void fights print `fight void: no player input` and contribute no fight count, wins, history, tally, habits, plan failures or coach call.
Autosaves and shutdown snapshots also exclude an inactive current fight.
The game requests focus and cursor lock on its first input frame, with another grab on the first left click inside the focused window.
Windows can still refuse foreground activation; the void check protects learning in that case.

`Memory.coach.failures` keeps per-plan `uses`, `deflected_all` and `fatal` counters with serde defaults for older files.
A plan use is `deflected_all` only when at least one attack started and all its attack exchanges were deflected without a hit or ordinary block.
A whiff or an attack with mixed contact outcomes prevents that classification; tactics do not count as attacks.
A use is `fatal` if the boss loses a deathblow mark or dies within three seconds of its last attack, or Wolf punishes it into a posture break.
The three-second attribution window is anchored to the last attack exchange ending, survives a subsequent plan starting, and settles a use only once.
Plan-use counts inform the coach, while recent per-attack evidence supplies soft sampling penalties after three uses.
The diagnostic fatal and deflected counters can overlap, so their sum is not a probability of distinct failed uses.
Outcome counts no longer create automatic bans at startup or during a fight; legacy automatic phase bans are retained only when the current coach plan explicitly avoids the choice.
A validated coach reply's optional `retry: [{"choice":"3000","reason":"a new timing hypothesis"}]` re-allows a player exclusion.
Unknown choices, empty reasons and choices also listed in avoid are rejected.
A retry keeps historical and recent evidence; further failures teach the sampler and coach rather than imposing another automatic ban.
Void fights roll back their provisional bans and never persist their counters.

Coach input includes `banned` and `failures`, with failure counters packed as `[uses, deflected_all, fatal, failed_attack_use]`.
Budget fitting prioritizes those counters, trims ordinary summaries first, and can omit the played plan and shorten notes in the extreme case where the full known banned vocabulary is sent.
The normal input retains the played plan and previous notes.
The schema includes retry as an array, with an empty array meaning no retry; old JSON and memory can omit it.
All coach CLI tests use a fake executable and all HTTP tests use a loopback server.
Codex defaults to `gpt-6.1-sol` and `model_reasoning_effort="medium"`; model and effort overrides are checked against the fake CLI's ordered arguments.

The goals prioritize winning faster, break follow-ups, damage to Wolf per minute, and little boss damage taken per minute including posture from perfect deflects.
`engaged_time` is the fight duration when `passive_share < 0.35`, otherwise zero, and appears in persisted metrics, the scorecard, the trend and coach result.
Retreat plans are permitted once per five seconds, including short retreat chains followed by an attack.
The six-second attack rule, lead-in deadline checks and four-second passivity window are retained, so a chain whose lifetime caps exceed the attack deadline remains ineligible.


### Owl posture pressure and local punish responses (2026-10-11)

The read-only golden dataset contains five boss wins at 30.8, 8.7, 217.3, 99.2 and 120.6 seconds.
Its action 3040 has six uses, zero contacts, six whiffs and four punishments; the last three fights contain 127, 64 and 49 Lua acts.
Boss-side pressure alone could not tell the chooser when Wolf was close to breaking, and grouping Lua plans concealed the individual moves' performance.
The separate posture-recovery work addresses combat mismatch 4 in [COMBAT-FUNDAMENTALS.md](COMBAT-FUNDAMENTALS.md); this brain change does not alter recovery rules.

`wolf_posture_25`, `wolf_posture_50` and `wolf_posture_75` each fire once when Wolf's posture damage reaches that fraction of maximum.
All three re-arm below 15% damage, and a jump across several thresholds queues each event.
`wolf_broken` fires on the rising edge of Wolf's broken posture, including breaks caused by deflecting his attacks.
These events coexist with boss pressure and the punish/deflect streaks, and use the existing suppression during deathblows, boss breaks, death and respawn.

Fight metrics, scorecards, trends and coach results include `wolf_posture_breaks`, `hits_after_break`, `break_punish_rate`, `wolf_posture_peak` and `time_to_kill_wolf`.
The hit window is 2.5 seconds after a Wolf break and excludes the breaking blow itself.
The punish rate is the fraction of breaks followed by at least one landed hit, so several hits after one break cannot inflate it above 100%.
Peak posture is a fraction of maximum damage; kill time is present only for a boss win.
Old memory fields default to zero or null rather than inventing historical break counts.
The coach prioritizes winning faster before breaks and their immediate follow-ups, damage per minute, and low boss damage taken; engaged time only breaks ties on losses while landing hits.

A game plan can contain `on_wolf_break`, one to three weighted attack-containing plans validated against the boss's normal vocabulary and avoid list.
With a move sheet, the brain chooses the fastest reachable damaging normal move even when this list is absent or empty; a plain Lua brain keeps its own follow-up.
Owl's compiled `506001_battle.lua` maps its grab helper to `Act40` (source line 1513), which queues 3040 through `AttackTunableSpin`; normal plans preserve this reflex.
The local response owns this one follow-up after the battle goal activates, replacing its queued Lua act and surviving the delayed replan from observed effects.
A single attack uses the existing `ComboFinal` cancellation path, so the current clip's TAE still decides when it can start.
Priority ends when the response's first attack starts or its queue deadline expires (at least 2.5 seconds, extended through a longer data-defined recovery); an already active paired throw cannot be forced to cancel.
No model request is needed to execute the response.

Response outcomes are counted by plan in coach input `on_wolf_break` and persisted in `Memory.coach.break_plans`.
Failure evidence uses the key `on_wolf_break:<plan>` so ordinary successful uses of the same move cannot dilute its response ban.
Whiffed or punished response uses add `failed`, excluding uses already classified as fatal or entirely deflected.
The existing threshold of at least three uses and a failure rate of at least 0.6 applies locally, with inactive fights excluded.
A reasoned retry for the ordinary plan text also resets the response's evidence baseline.

Validation caps the combined Lua weight to at most 25% of each set, including duplicate Lua entries, and drops a Lua-only set.
There is no variety warning or penalty; a successful move can keep its weight while it keeps working.
Coach input `actions` contains individual `[n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]` counts for Lua and coached moves, with zero-count rows for untried offered actions.
`action_range` supplies each measured action's mean attack-start distance and melee reach, rounded to two decimals; projectile-only actions have no reach sample.
The coach can therefore probe under-used moves using their own outcomes rather than the aggregate `lua` plan count.
The input budget is 8192 bytes; the small in-fight chooser request remains unchanged.

`player_principles` and `boss_principles` each keep up to six lessons of at most 90 Unicode characters.
Coach inputs separate shared lessons from player lessons, and console plan summaries mark each as boss or player.

In play with the real Lua AI or any boss brain, F9 and normal exit print the session's per-action success/range table and fight trend.
Each report appends to `logs/brain-session-<timestamp>.md`, including duration, boss-win kill time, Wolf breaks and post-break hits.
Reports aggregate only this session, exclude void fights, and include an active unfinished fight as a snapshot.
Standalone simulation tests do not enable report files; the append/exit test uses a disposable file in the system temporary directory.

Offline regressions cover threshold re-arming and actual chooser requests, response bans, the 2.5-second boundary and rate denominator, legacy memory defaults, bounded lesson persistence, action-level Lua evidence, schema fields, and report append on exit.
In the scripted Owl duel, a real 3001 slash breaks low-posture Wolf, then the 3000 response animation starts after 18 ticks (0.300 seconds); without a response plan Lua requests 3040 instead.
These checks use extracted local data, fake coach CLIs and loopback HTTP only.
Live model plan quality, live-game fidelity, and the F9 presentation in a rendered window remain unverified.


### Mid-fight coach reviews

Coach mode reviews every 30 seconds of fight time since the last coach send, with a loaded starting plan counting as a send at fight start.
A surviving boss mark loss, two Wolf posture breaks since the last review, or all coached plans in the active set being banned triggers an early review, even if a Lua fallback remains.
Each send resets the timer and break counter.
Calls are serialized across fights: a blocked trigger waits for the outstanding call, and an ended fight's post review supersedes the pending mid review.
Reviews wait until 20 seconds have elapsed, the player is active, and both fighters are outside breaks, deathblows and respawn.
`--coach-interval 0` disables the timer and all early triggers.

Requests use `phase: "mid"` and start with this fight's partial result and per-plan/per-set outcomes before older evidence.
They include elapsed seconds, `[wolf_hp, wolf_posture, boss_hp, boss_posture]` fractions, boss marks left and the current game plan, with a strict 8192-byte compact JSON budget.
The coach keeps what works, replaces what fails and returns a full game plan.
The next decision-slot choice uses the replacement start set and skips its opener; queued active attack goals continue normally.
Validated plans are saved through the existing memory path.
Console review lines name the trigger, arriving plans print their summary, and coach JSONL lines include `kind: "coach"` and `phase: "mid"`.
Fight records include mid-review counts and total completed review latency in milliseconds.

The objective order is win and win faster (`time_to_kill_wolf`), Wolf posture breaks and immediate landed hits, Wolf HP plus posture damage per minute, then little boss HP plus posture damage per minute and fewer Wolf perfect deflects.
Passive share above 0.35 fails regardless of the other results, and retreats are attack setups.
`engaged_time` remains informational and only a small tiebreaker for losing fights while landing hits.
`boss_damage_taken_per_min` sums observed boss HP drops and posture rises, normalized to percent of a bar per minute, without subtracting healing or recovery.


## Combo-aware move sheet and break log (2026-10-11)

Installing the boss brain computes `move_sheet` once from the loaded character's clips, resolved TAE timelines and BehaviourParam/AtkParam rows.
Every offered action, named combo and attack group inside a tactic plan has its hit windows, duration, recovery, NPC cancel windows and nominal melee reach.
`hits` stores `[time_seconds, Wolf_guard_posture, Wolf_deflect_posture, behaviour_judge]`; its length is the hit count.
Distinct judge/start windows follow the hit detector's identity, rather than counting individual hit capsules as separate blows.
Counter-only dummy behaviours and inactive conditional weapon hits are excluded.
Projectile-only tools have zero melee hits and `reach: null`; this sheet does not invent projectile contact times.
The guard/deflect numbers use the same `CombatRules::posture_for_outcome` path as live resolution, including the loaded Wolf weapon, reinforcement, attack-power effects and the NPC's attack-time TAE effects.
The AP1 and AP6 fixture verifies these forecasts against real guard and deflect resolution; these Owl-to-Wolf values are identical at both powers.

`duration` is the decoded clip duration, and `recovery` is the time from the last hit window's opening to the first unconditional attack cancel or natural end.
`cancels` stores `[start_seconds, end_seconds, ChrActionFlag, stateInfo]`, retaining conditional requirements instead of claiming they are always available.
NPC attack windows are ChrActionFlag 23 (combo) and 86 (attack); flag 79 permits a step and flag 11 permits movement.
NPC HKS env queries 107, 120 and 118 read the queued combo, attack and step actions respectively, through `Character::fill_env`.
They are query numbers, not new TAE flags or invented cancel windows.
A combo's nominal timeline chains at each component's first attack window; a zero-length interval in its sheet marks the consumed chaining point.
The final component retains its natural duration and recovery.
These nominal times omit hit stop and frame rounding, which remain part of actual execution.
Reach samples animated dummy capsules at 60 Hz, includes Wolf's radius and accumulated root translation/yaw across a combo, and assumes neutral steering and root travel.
TAE 760 target-dependent travel scaling, target movement and facing can change real contact, so reach is an envelope rather than a guaranteed hit distance.

Generated Owl example from `owl_move_sheet_matches_guard_and_deflect_resolution_at_current_power` (seconds; posture points per hit; nominal reach in metres):

| action | hits | hit times | guard / deflect posture | duration | recovery | first combo / attack cancel | reach |
|---|---:|---|---|---:|---:|---|---:|
| 3000 | 1 | 0.567 | 29 / 14 | 2.500 | 0.333 | 0.900 / 1.700 | 6.666 |
| 3001 | 1 | 0.333 | 23 / 11 | 2.733 | 0.334 | 0.667 / 1.767 | 5.033 |
| 3002 | 1 | 0.700 | 41 / 20 | 2.900 | 0.233 | 0.933 / 1.867 | 6.832 |
| 3005 | 1 | 0.467 | 29 / 14 | 2.033 | 0.066 | 0.533 / 1.533 | 4.360 |
| 3017 | 1 | 1.133 | 59 / 29 | 3.500 | 1.734 | 2.867 / 2.867 | 7.840 |
| 3063 | 2 | 0.467, 0.767 | 11 / 5; 17 / 8 | 2.667 | 0.066 | 0.833 / 1.767 | 7.657 |
| 3000+3001 | 2 | 0.567, 1.233 | 29 / 14; 23 / 11 | 3.633 | 0.334 | chain at 0.900; final 1.567 / 2.667 | 6.849 |
| 3000+3001+3002 | 3 | 0.567, 1.233, 2.267 | 29 / 14; 23 / 11; 41 / 20 | 4.467 | 0.233 | chain at 0.900, 1.567; final 2.500 / 3.434 | 10.095 |

Every Wolf break now records the boss action or offered combo, hit index/count, attacking/recovering/free state, remaining clip-clock wait, whether the response actually started, its start delay and outcome.
Actual clip starts supply the start delay; queuing an AI goal is not counted as starting the punish.
Breaks from a counter or other source with no matching boss hit use index zero.
`break_log` supplies recent records to both post-fight coaching and mid-fight review, and `break_summary` retains per-action/hit `[breaks, converted]` totals when older detail is trimmed.
Outcomes are `hit`, `guarded` (including deflects), `whiffed`, or `missed window`; an unresolved live record is `pending`.
A landed hit within 2.5 seconds, excluding the breaking hit, converts the break, including a continued combo hit or a Lua follow-up.
`break_conversion_delay` is the mean delay to the first landed conversion for each converted break, not the queue delay; it is null without conversions.
It appears in persisted metrics, the trend, coach result, scorecard and session report.
Session reports show individual break records and per-action/hit conversion totals.
Old memory defaults the new metric to null and the log to an empty list.

The local response replaces the remaining combo goals and queues an attack with NPC combo cancellation enabled.
The character's existing HKS/TAE gates start it at the first compatible window, or at the first free frame if no such window exists.
Repeating the current attack waits for a new clip clock rather than treating the old swing as the response.
The repeated-3001 regression reproduced a 14.8-second late start before that distinction; after the fix both a 3005 punish and a repeated 3001 start 0.300 seconds after a middle-hit break at clip time 0.383 seconds, using the 0.667-second window before the 2.733-second natural end.
The no-window fixture removes only cancel flags from a copy of 3001's in-memory TAE, retains its timings and hit stop, and starts the punish at the first free frame (2.400 seconds after the same break).
All three assert the previous clip clock against the allowed boundary and verify the logged break is hit 2 of 3.

Coach and mid-review instructions ask for breaks on the last combo hit or a fast single move near breaking, a short `finisher` set for `wolf_posture_75`, and inspection of failed conversions.
The input still fits 8192 bytes: trimming prefers the current plan's moves and the most-used other actions, keeps recent break detail and aggregate totals, then uses the existing summary trimming.
The four Owl fixtures measured 3916 to 4067 bytes for the complete fitted coach JSON, including the move sheet, break log and aggregate totals.
The mixed `walk_in4>3000+3001` fixture also checks that the current attack group remains in the fitted sheet and that its break is logged as hit 2 of 2.
No hosted calls or real model CLIs are needed for these tests.

## Cost and observability

Tokens are the primary session metric.
Subscription coaches (`--coach codex` and `--coach claude`) record only the tokens the CLI reports, labelled as a plan proxy.
Hidden system/tool tokens and plan quota rules may differ from those counts; these numbers do not measure remaining subscription allowance.
No subscription dollar estimate is computed, including when Claude emits `total_cost_usd`.
USD estimates apply only to API-key requests (Decisions, Responses and `--coach api`).

The editable `config/brain-prices.toml` records verification dates and source URLs, with input, cached input and output rates per million tokens.
Responses uses standard short-context text rates from the [OpenAI pricing page](https://developers.openai.com/api/docs/pricing) and [model catalog](https://developers.openai.com/api/docs/models).
Decisions uses $0.10/M input and no output charge, as specified in the [Decisions guide](https://developers.openai.com/api/docs/guides/decisions).
Luna Responses is $0.10/$0.01/$0.50 for input/cached/output; Sol Responses is $2.00/$0.10/$10.00.
The Haiku $0.10/$0.50 seed is task-specified, with cached pricing explicitly marked TODO; subscription Claude never uses this entry.
An unknown model records tokens and unknown billed USD, without guessing a rate.
Reasoning is included in output unless the table explicitly sets its separate rate; cached input is a subset of input, and neither subset is added twice.
These are local estimates, not provider invoices; tools, other processing tiers and long-context premiums require an appropriate table update.

Each worker JSONL line retains legacy `model`, `usage`, `kind`, `phase` and `round_trip_ms` fields for one compatibility version.
It also carries `gen_ai.system`, `gen_ai.request.model`, `gen_ai.operation.name`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens` and `sekiro.*` extras for cached/reasoning usage, latency, kind, phase, billing and the plan-proxy flag.
`usd_estimate` and `sekiro.usd_estimate` are null for subscription CLIs and unknown prices; `billed` distinguishes them.
Codex usage sums `turn.completed` events, ignores item messages and printed totals, and reads the final plan from `-o`.
Claude input includes reported cache creation and cache reads.
Failed API answers retain reported usage even when their plan is unusable.

Fight summary lines include both fight and session totals, counting calls that complete during that fight (including a previous fight's coach review).
Session reports include a Cost table by kind and model, with plan-proxy tokens, API tokens and billed USD.
F9 and normal exit print the report; the F8 recording overlay shows the session line and persistent budget notice.
For example: `session: 12 calls · 48.0k tok (plan proxy) · 2.0k tok · $0.000200 (API)`.

Measured coach context is about 11k reported tokens per Codex call, while a Decisions checkpoint uses about 250 input tokens.
With the default local chooser, two subscription coach reviews use roughly 22k plan-proxy tokens and checkpoint choices add zero tokens or billed usage.
With `--checkpoint decisions`, eight checkpoints add about 2k API input tokens and $0.0002 billed API usage per fight (output is free for Decisions).
Mid-fight reviews add about another 11k plan-proxy tokens each.
For an API Luna coach, an illustrative 10k input + 1k output call without caching is $0.0015, so two such calls with local checkpoints are approximately $0.003 per fight, or $0.0032 with eight Decisions checkpoints.
Actual input/output splits and cache hits come from the telemetry, rather than an assumption that all coach tokens are input.

Use `--brain-token-budget 50000` as the main per-session stop, counting all reported input plus output tokens across both workers.
Use `--brain-budget 0.01` to limit billed API usage independently; subscription calls do not spend this USD budget.
Both default to no limit, and zero stops calls immediately.
With either budget enabled, the two workers serialize call admission and recheck the shared totals before each call, including already queued work.
Once a completed call reaches either limit, no further model calls start in that session.
The current fight keeps its last installed plan, and the next fight uses the boss's own Lua AI.
A console notice and overlay notice explain the stop.
A USD budget also stops after an unknown billed price, because the remaining spend cannot be established.

Usage is known after completion, so the final call can exceed the limit; this is a stop-after-call budget, not a hard provider spending cap.
API-key testers should set both budgets and provider-side spending limits, and keep the existing request deadline/output cap small.
No third-party telemetry service is involved.

## Continuous Genichiro brain and counter learning (2026-10-11)

The c7100 to c7110 EMEVD hand-over is a phase change within one fight.
The same driver, coach worker, clock, player profile and habits survive it.
The intermediate castle defeat does not increment fight totals or send a post-fight review.
Every enemy reset reloads its Lua hook and rebinds the retained driver; a supported reset starts the next numbered fight.
Owl's second phase changes his existing character's effects rather than replacing the driver, so it already retains the session; its reset now follows the same explicit rebind path.
The supported tables cover soldier, Owl and both Genichiro characters; unsupported hand-overs are outside this session path.

### Way of Tomoe action table

`brain::actions::TOMOE` is for castle c7110, NpcParam/NpcThinkParam 71100000.
The derivation follows the c7100 method: read `GOAL_Rival_hadaka_711000_Battle` in the installed `m11_01_00_00.luabnd.d/711000_battle.lua`, resolve its requested clips and TAE, then build the move sheet from the loaded BehaviorParam and AtkParam rows.
Local research output is gitignored under `cache/brain_research`; no decompiler text is included in source.
Distances below are the script's base approach before its hit-radius subtraction.
Follow-ups chosen alone retain their parent act's distance, as with the existing tables.

| Lua act | Offered animation ids | Approach (m) |
|---|---|---:|
| 01 | 3000, 3001, 3002, 3003, 3010, 3025 | 3.6 |
| 02 | 3004 | 3.0 |
| 03 | 3005 | 2.2 |
| 05 / 34 | 3007, 3011 | 5.9 |
| 06 | 3016 | 5.2 |
| 09 | 3040, 3041, 3067 | 4.5 |
| 10 | 3006 | 4.8 |
| 12 | 3037 | 3.5 |
| 13 | 3012, optional 3039 | none |
| 15 | 3014, 3015 | 8.9 |
| 17 | 3039, lightning bow | none |
| 18 | 3043, advancing lightning jump slash | 8.3 |
| 19 | 3042, lightning jump slash | 3.5 |
| 20 | 3008 | 12.5 |
| 22 / 24 | 5202, 5203 / 5201 | none |
| 30 | 3009, bow shot | none |
| 31 | 3003, 3045 | 3.6 |

3039 sets AI number 2 to 1 and 3008 sets number 7 to 1.
The normal bow and lightning bullets use their existing combat paths; the move sheet does not invent projectile contact times.
Lightning bow 3039 resolves TAE judge 521 through BehaviorParam 271100521 to Bullet 71100521 and AtkParam_Npc 71100521; 3042/3043 use melee judge 361 and AtkParam_Npc 71100361, as recorded in `docs/GENICHIRO.md`.
Combos offered are the first three sword hits, Floating Passage with its perilous finish, braking bow into diving slash, and bow into lightning.
All existing tactics and three curated attack chains are supported.
The phase's scripted controller uses 3039 and 3042, rather than reusing c7100's test plans.

Excluded normal plans: 3020/3021 from Act07/08/11/12 are paired throw starts; 3022 in Act16 is a downed-Wolf reflex; turning Act21 and reaction Act28 stay Lua.
The 3085/3086/3087 branches of Act09/15/40-45/48 require SpEffect 3711500, outside this castle row's normal phase.
Act48's 3013 opener belongs to that variant branch and is excluded.
Act01's complete four-step branches exceed the three-step cap.
Movement Act23/25/26/27 is represented through tactics rather than extra attack ids.
Counter callbacks are separately described to the coach and cannot be requested as arbitrary normal plans unless that id also belongs to the normal table.

### Phase memory and validation

Both Genichiro characters resolve default persistence to c7100's boss and player files, isolated from Owl and soldier.
Tomoe action evidence lives in `evidence.phase_actions.c7110`, and its plan outcome keys have the `c7110:` prefix.
The player layer uses the same `phase_actions` store and keeps `coach.phase_plans` and `coach.phase_bans` by character phase.
Every game plan includes `boss_phase`, which tags its opener and sets together.
Missing phase stores, tags and counter weights default cleanly for old files; historical whiffs are not retrospectively labelled as Mikiri.

Pooling requires the same act id, byte-identical decoded clip and equal resolved TAE event timelines.
Verified ids are recorded in `shared_actions`; only these also contribute Tomoe outcomes to the common action store.
The installed attack clips inspected during this session differ between c7100 and c7110, so matching ids alone do not pool their evidence.
Habits are retained independently of that stricter move identity check.
Coach input identifies `boss_phase` and uses that phase's move sheet and counter statistics.
The offline/hosted coach schema is selected from this tag for every review, using the same worker.
Replies tagged for a different phase are discarded, and every plan is validated against the current table before use.

### Successful Wolf counters

The tally consumes combat resolution reports, rather than inferring a counter from a dodge or airborne state.
Mikiri is reported only after forward-step TAE flag 125, thrust/counter-dummy checks, facing and the matching ThrowParam row have succeeded, and Wolf's `W_ThrowAtk511xx` pair starts.
The sweep jump outcome requires an actual anti-ground kick hit while the enemy's jump-kick-effective SpEffect is active; merely jumping over a sweep remains avoidance.
The HKS-powered lightning return is reported at its judge-281 attack-window edge as `other`.
The exchange carries `countered: mikiri|jump|other`; it cannot also count as a whiff.
Counts now append total counters and the three kind counts to the old six entries: `[n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]`.
Six-entry legacy arrays and the transitional seven-entry form default the missing values to zero.
Metrics, coach results, action reports and the recording overlay expose the total.
Countered plan exchanges contribute strong recent failure evidence after the three-use safety minimum, with soft penalties instead of automatic counter-rate bans.
The coach receives mature rates and chooses explicit avoids or shared bans when justified.

### Reweighting Lua deflect counters

Lua still decides when `Kengeki_Activate` runs and computes its candidate callback weights.
The new hook wraps only `Common_Kengeki_Activate`, reweights positive offered entries, and calls the saved helper with the same callbacks and parameters.
Callback bodies, their combos, pressure settings, timers and conditional branches are preserved.
The driver uses each callback's recent per-attack outcome signal, shared with ordinary plans and checkpoint quality; counters, deflects, guards, whiffs and punishment reduce its multiplier, while hits and actual progress reward alternatives.
Coach `counter_weights` supplies additional multipliers from 0 to 10 for legal counter animation ids; zero, an explicit avoid or a shared coach ban vetoes the callback.
For a multi-animation callback the least favourable multiplier governs the whole callback.
The coach receives a ranked `counters` map that retains at least four entries under the 8192-byte budget, alongside counter move-sheet rows.

The hook never introduces a positive weight for a callback Lua did not offer.
If all offered weights would become zero, the original Lua weights stand.
Thus a sole mandatory counter remains mandatory, even after repeated Mikiri.
When Lua offers NoAction, it can be selected: index 50 for Genichiro/Tomoe and soldier, index 26 for Owl.
Those callbacks return -1 to the helper, allowing the battle goal to continue normally.
Soldier's offered Act24/25/30 and Genichiro's existing step callbacks also remain available under their own Lua validity checks; no new guard/step fallback is invented.
Lua-only brains leave counter weights unchanged.

Verification uses headless real-data hand-over/reset, lightning choices, successful Mikiri and sweep-kick fixtures, isolated phase evidence, fitted coach input, legacy defaults and synthetic offered-candidate checks.
No hosted calls or persistent playtest-file writes are needed.
Rendered playtest presentation and native-game comparisons remain outside this offline change.
The counter move sheet excludes script-named variants without a decoded character clip: soldier 3070/3076 and Tomoe 3060/3065/3075/3020.
Castle Genichiro's 3039 is also excluded because lightning belongs to Tomoe.
These callbacks remain Lua-owned; the coach cannot assign them explicit weights.
Plan failure keys also carry the c7110 prefix, so phase-only failures cannot ban the castle plan after reset.

## Contextual move fighter and cross-fight review

Implemented 12 October 2026, independently of the checkpoint set bandit.
Ordinary slots use `BrainDriver::pick_move`; posture-break slots use its deterministic fastest-reachable branch, without sampling learned evidence.
`--brain-moves local|off` defaults to `local`, needs no model or key, and `off` preserves the previous sampler and its persistent outcome multipliers.
`--brain-move-every 1|2|3` defaults to 2 attacks, exposed as `MOVE_EVERY` and the driver's configurable interval.
A multi-attack plan is indivisible: the fighter chooses again at its next legal slot rather than interrupting a TAE/Lua chain.
Holding counts constituent attacks, never more than three, and is cancelled when physical context changes or the arm becomes ineligible.
Contexts contain near/mid/far distance (3 and 7 metre boundaries), low/high boss and Wolf posture (75%), a separate broken-Wolf state, and the previous plan's first named animation family (`id / 10`).
Evidence is keyed by character phase, context and full plan text, so different setups/chains and Tomoe stay separate.
Arms are only the current filtered set or explicit break responses; coach avoids, bans, limits and pacing remain authoritative.
Lua retains reactions, callbacks and fallback acts, including the existing per-attack callback evidence.
An explicit `lua` entry retains the coach's delegation share, capped at 25% while attack plans are eligible; it cannot become a permanently untried Thompson arm.

Each arm draws from `Beta(success + 4*p, failure + 4*(1-p))`.
The coach prior is `p = clamp(0.25 + 0.35*weight/max_weight + 0.35*confidence, 0.05, 0.95)`.
Before three effective observations, a 0.4 novelty bonus encourages aggressive coverage.
Mature draws are compared to the best available posterior mean, using `draw * min(draw/best_mean, 1)^3`, without an absolute failure-rate threshold or automatic exclusion.
The largest independent draw wins; this is a relative posterior comparison, not a second weighted selector.
Explicit random probes occur at low/normal/high rates of 1%, 2.5% and 8% per decision, reduced by confidence with a 0.5% floor.
Novelty disappears as successful evidence accumulates, while the coach can increase exploration when the trends deteriorate.
Shared player evidence contributes at most four effective observations per arm and at most 25% of another profile's mass; personal and current-fight evidence dominate.
Per-fight retention is 0.98, matching checkpoint decay.
Both direct and layered profile loaders migrate versions 1 through 4 to version 5 without inventing contextual history.
The shared boss envelope remains version 1 with optional contextual evidence in contribution snapshots.
Void fights discard provisional learning; passive fights convert their success mass to failure before commitment.

Move settlement uses the existing success/failure classifier: counters including Mikiri and jump counters, deflects, full guards, whiffs/dodged sweeps and damage taken are failures.
Clean hits, actual progress and breaks are successes, with overlapping evidence retained.
Breaking posture adds two success units on top of the classifier's break reward.
A connecting hit in the broken-Wolf context within 2.5 seconds of the opening adds `2 / max(contact_delay, 0.25)` success units; missing that opening adds 0.5 failure units.
The contact timestamp, not clip recovery or exchange duration, determines the conversion bonus.
Safe movement/setup exchanges are neutral; getting punished during a setup teaches failure for that plan.
The fractional reward is `success / (success + failure + 0.25)`.
Measured HP/posture bar damage contributes a bounded 0.5 success bonus using damage per minute over elapsed plan time, including setup.
A non-passive win supplies three terminal observations with reward `0.98 + 0.02*60/(60+kill_seconds)`, so wins outrank ordinary rewards and faster kills score higher.
No survival or waiting reward exists.
The overlay's MOVE line shows arm, context, posterior mean and posterior failure share; checkpoint cards continue to show set switches.

Game plan version 2 adds `explore`, `move_controls: [{plan, confidence, lock}]`, and at most six 120-character `lessons`.
Versions 0 and 1 remain readable and validate into version 2 with normal exploration and empty controls/lessons.
Confidence must be finite and within 0..1, and controls must name a valid boss plan; invalid entries warn and are dropped.
`lock` holds that arm for up to three eligible attacks after selection, subject to context and hard limits; it is not a forced illegal attack or a permanent veto of alternatives.
Lessons are deduplicated, repeated lessons refresh recency, and the newest six survive; oldest lessons are evicted first.
The boss profile retains these lessons and six previous hypotheses, their plan texts and measured kill-time/conversion/damage outcomes.

`digest.moves` is a terse fixed-column lifetime arsenal table: id, short label, uses, hit%, deflected/guarded%, Mikiri/jump-counter%, whiff%, progress%, and recent/lifetime hit%.
Counts use the current character phase over all fights, falling back to legacy aggregate counts only when phase counts are unavailable.
The recent column uses this fight in the same phase; `-` means no recent samples rather than a zero hit rate.
Unused rows lead, then weaker lifetime hit rates relative to counter/punishment rates; flags can overlap, so percentages need not sum to 100.
Never-tried moves are explicitly `unused`, rather than silently absent.
Historical files have no progress counts or first-deathblow attribution; `?` and -1 mean unknown, not zero.
New fights record progress and the player's first deathblow.
Progress rates use observed exchanges since version 5; `*` marks a partial historical denominator.
The digest includes best/worst three mature moves, the last ten completed fights as `[kill_seconds_or_-1_loss_or_-2_unknown, break_conversion, damage_per_minute, player_first_deathblow_or_-1, passive_share]`, their deterministic means in `trends` (unknown values excluded), retained lessons and bounded hypotheses.
Legacy boss wins with a known fight duration supply the kill time rather than being mislabeled losses when their newer kill-time field is absent.
The coach's existing trend also uses ten fights; shorter general-purpose request trends remain compatible.

The input budget deliberately rises from 4096 to 8192 bytes, still small for CLI coaches, to preserve a full typical arsenal and ten results.
The format comparison test writes JSON-encoded text and arrays-of-arrays to `target/alt-fb/table-formats.json`; `uv run --offline --no-project -m docs.brain_table_measure target/alt-fb/table-formats.json` compares wire bytes and explicitly approximate tokens.
No provider tokenizer or vocabulary is installed locally, so token estimates are labelled rather than represented as exact Codex/Claude token counts.
The supplied phase-specific profile snapshot measured 1550 wire bytes and 654 estimated tokens for text, versus 1862 bytes and 1284 estimated tokens for arrays.
The text format avoids array punctuation and repeated string quoting.
Trimming first reduces the existing attack/checkpoint/counter detail, move timings, break timelines, redundant shared summaries, plan outcomes and detailed ranges, following `CoachInput::trim_one`.
Only after those details are exhausted does it remove table rows from the tail, preserving unused/worst rows first, then oldest fight results and lessons, and finally old hypotheses while keeping the latest one.
The review boolean survives trimming and the final serialized byte count is checked before submission.

`--coach-review-every <n>` defaults to `REVIEW_EVERY = 5`; 0 disables cross-fight reviews.
The next coach call at each interval, or after an eight-fight plateau in kill/conversion score, uses REVIEW MODE once per completed fight.
Only completed non-void wins and losses advance the interval; unfinished saves do not count.
The same worker, coach infrastructure and reply validator ask whether the strategy works, what was learned, which hypothesis to test next, how exploration should change, and which moves to promote or retire.
Failure keeps the previous validated plan and lessons, just as a normal coach failure does.
There is no second persistent coach: the same coach reviews the lifetime arsenal and its own measured hypotheses, avoiding competing plans, duplicate model cost and divergent memories.
Prompt audit added contextual relative comparisons, failure classes, the table/ten-fight field legend, confidence/exploration/lock controls and bounded lessons to the normal coach, plus a separate review block selected from `digest.review` for both CLI and API adapters.
The Decisions checkpoint instructions now explicitly rank fast wins, breaks plus quick contact, damage rate and low boss damage, classify counters/deflects/guards as failures, and leave per-move decisions to the local fighter.

The offline experiment copies the supplied Genichiro boss/profile files to a temporary directory and never writes under `logs/`.
With no local memory fixture, CI runs the same synthetic environment from empty memory.
It runs 64 seeds, six fights and 96 eligible slots per fight, with four ordinary slots followed by four broken-posture slots.
Wolf always Mikiri-counters 3025 and deflects 3016; 3000 connects quickly, while 3001 misses the opening despite eventually connecting.
The three pickers are the old counter-blind current-fight streak sampler, previous persistent evidence, and the contextual move bandit.
This is a selection replay through production outcome settlement, not a full replay of player HKS or mandatory Lua callbacks.
Its assertions require combined bad-move use below 5% by fight five, continuing probes, rising quick-punisher share and family switches below 55% of boundaries.
Detailed final numbers and validation are recorded in `target/alt-fb/` and the dated status entry.

| Sampler | Bad moves fight 1 | Fight 2 | Fight 3 | Fight 4 | Fight 5 | Fight 6 | Quick punisher fight 6 | Family switches |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Old streak | 37.27% | 36.36% | 36.56% | 35.89% | 36.23% | 36.95% | 31.36% | 49.6% |
| Previous persistent evidence | 1.77% | 1.11% | 1.04% | 0.81% | 0.94% | 0.81% | 49.59% | 2.1% |
| Contextual move bandit | 37.76% | 3.09% | 9.47% | 2.34% | 2.08% | 2.31% | 94.47% | 7.8% |

Bad moves combine the always-Mikiri and always-deflected arms, over 6144 choices per fight.
Persistent evidence starts with the real player's already trained history; the new contextual posterior starts empty because old files cannot reconstruct distance, posture and previous-family context.
The regression explicitly keeps that version-4 migration starting point even if the live profile later gains version-5 contextual evidence.
The contextual fighter falls below 5% at fight two, explores rare transitions again at fight three, and stays below 5% from fight four onward.
It retains probes and satisfies the stricter per-seed, per-fight cap of 48 family switches over 95 boundaries.
Baseline persistent numbers can change if the read-only live profile advances between runs; the replay transcript identifies the measured snapshot's output.
The brain library and brain-net suites pass, and libraries/binaries pass Clippy with warnings denied.
At that checkpoint, the requested integration filters and workspace/all-target Clippy were blocked by an outdated `GamePlan` literal in `crates/sim/tests/it/coach.rs`.
The fixture compatibility gap has since been resolved; Task A validation below covers the requested checks.

## Move sheet as a gate and a feature (2026-10-13)

The sheet in `brain::moves` is cached by character in the driver and shared by plan eligibility, live brain-owned attack goals, break punishment, telemetry and coaching.
It resolves the decoded clips, root motion, animated hit capsules, TAE attack/cancel windows, BehaviorParam attack judges and AtkParam profiles through the combat damage resolver.
Each melee row includes first-window reach samples at 60 Hz, active windows, per-hit direct HP/posture damage, guard/deflect posture, recovery, cancels and named chains.
Combo rows use the existing legal cancel timing, accumulate root travel and hits, and publish guarded posture totals and time to the last hit.
Cached `chain_next` and `cancel_into` edges name the following combo moves and normal damaging moves accepted by unconditional NPC cancel windows.
Projectile rows include launch time, animated launch offset, nominal parent travel, speed/lifetime/radius, HP and direct/guard/deflect posture rather than pretending to be melee.
The geometry samples stay local; the coach receives their compact reach and timing summaries.

The ordinary chooser and opener reject attacks whose first active window cannot reach Wolf's predicted position, using observed horizontal position changes and a 0.2 m tolerance.
The gate checks each first-window sample at its own time, so an attack can connect later in its first active window without borrowing reach from a later combo hit.
Movement setups are considered at their destination and native approach distance, then every brain-owned attack goal is checked again at execution, including combo continuations.
A rejected attack goal moves toward Wolf instead of requesting a swing.
With no eligible attack, the driver generates a run-in followed by a fast attack, using the sheet's reachable spacing.
The existing attack deadline still forces a reachable attack or an approach; guards and utility animations never reset that deadline.
Lua goals and reaction callbacks have no brain plan-step tag and bypass this gate.

On a posture break the brain ignores learned move weights and chooses the earliest reachable damaging normal move from the full sheet.
If none reaches yet, it queues an approach into an attack and retries while the conversion window remains open.
TAE cancellation and HKS still determine when the queued response can start.
Wolf's remaining posture also exposes a named combo whose guarded prefix can break him with another hit within 2.5 s left in the chain.
The chooser receives that combo as an additional option, and its context includes the combo name and posture total alongside the existing Wolf posture bucket.
This predicts a guarded breaking sequence, not a guarantee against a player who deflects or evades it.

No-threat guarding keeps the existing failure path, now with a small 0.25 failure weight and a separate classification from attack whiffs.
Stationary idle time beyond 1.5 s uses the same path; active movement and damage reactions are excluded.
The resulting exchange is attributed to the plan and set that caused it, and current-fight plan failures are included in mid-fight coach input.
Neither idle nor harmless guarding counts as an attack for passive metrics.

The moves table adds first-hit seconds, first-window reach, recovery, posture per hit, best combo posture/last-hit time, and separate hit/miss distance means.
The coach receives Wolf's posture explicitly and the detailed sheet within the existing 8192-byte budget.
Trimming reduces less relevant table rows before sacrificing the active combo or recent break records.
The instructions rank wins, fast kills, converted breaks and damage per minute, and permit repeated successful moves without a variety penalty.

### Whiff audit and distance telemetry

The old test for an attack was its animation number in 3000-3099, so pure utility clips could count as whiffs.
The four cited castle Genichiro moves are genuine attacks: 3000 is melee, 3018 and 3034 fire damaging arrows, and 3062 is a thrust.
In `brain-session-1791839616326.md` their recorded whiff rates are 68/83 (81.9%), 135/140 (96.4%), 118/128 (92.2%) and 105/140 (75.0%).
The melee misses include evasion, range failures and interruption before contact; resolved Mikiri counters are separate.
The absence of a measured melee reach for the bow moves does not make them utility moves.

There was a second measurement defect: delayed projectile contacts were credited to the latest action, so the historical bow rates are not reliable real-miss rates.
Firing animation identity now survives projectile children and reaches the duel report and tally; the firing exchange stays available through the projectile lifetime.
The saved profiles and session summaries lack shot-level provenance, so exact corrected historical bow rates cannot be reconstructed from those aggregates.
Historical files remain untouched.
Utility exchanges cannot whiff, and idle failure no longer inflates the miss column.
Simultaneous contacts on the breaking tick no longer count as zero-delay break conversions.

The driver records actual separation at the nominal first-hit clock, including nominal arrow flight from the starting separation.
It classifies the sample after the exchange settles, so delayed hits are included and guarded/deflected contacts are excluded from both unguarded-hit and true-miss means.
Session reports show separate first-hit hit/miss means and the coach table retains phase-specific sums/counts in defaulted profile fields.
An interrupted animation that never reaches its first-hit clock has no distance sample, rather than a fabricated miss distance.
Old profiles have no hit/miss distance evidence and display missing values until new samples arrive.

### Installed-data validation

The 30-second Genichiro retreat/guard A/B uses the same contact classification and projectile settlement on both sides, disabling only melee reach rejection in the baseline.
Ordinary weighted slots are reserved for the scripted brain in this fixture; queued Lua reactions still run.
Every observed live brain attack request must belong to the reachable set, at least one such request must occur, and the boss must start an attack within the six-second passive deadline.
Misses fall from 6/18 (33.3%) to 3/14 (21.4%), a 36% relative reduction.
The remaining Lua attacks are deliberately ungated.

The stationary 1.6 m Owl break fixture compares the old supplied 3000 response with the full-sheet fastest reachable response.
Both convert 1/1 breaks, but landed-hit delay falls from 0.917 s to 0.450 s.
These small controlled regressions establish the mechanism, not a live player win-rate or kill-time estimate.

Example Genichiro rows at the fixture's power are below.
First active is the TAE window start or arrow launch, while first at 3 m includes sampled melee geometry or nominal projectile flight.
Melee reach is the first-window envelope with root motion; arrow reach is nominal lifetime travel plus launch offset and radius.
HP/direct posture and guarded posture are separate damage outcomes, per hit.

| Move | First active (s) | First at 3 m (s) | Reach (m) | Recovery (s) | HP/direct posture | Guarded posture |
|---|---:|---:|---:|---:|---|---|
| 3000 | 0.667 | 0.718 | 4.695 | 0.233 | 64/18 | 17 |
| 3062 | 1.033 | 1.151 | 3.757 | 1.267 | 128/36 | 35 |
| 3018 | 0.867 | 0.895 | 161.096 | 0.243 | 64/18 per arrow | 17 per arrow |

The Owl's 3001 starts its first active window at 0.333 s, reaches about 5.03 m and deals 23 guarded posture at the default fixture power.
His 3000+3001+3002 combo totals 93 guarded posture with its final melee hit at 2.267 s.
The real-data sheet tests compare damage with actual guard/deflect resolution and spot-check the existing authored timelines.

All seven requested release filters (`brain`, `coach`, `brain_learning`, `genichiro`, `owl`, `soldier_brain`, `brain_tactics`) and all 42 brain-net tests pass; existing slow tests remain ignored.
Workspace/all-target Clippy with warnings denied and `cargo fmt --all --check` pass.
Builds and tests use `CARGO_TARGET_DIR=target/alt-ta`, six jobs and offline dependencies; coaching tests use local fakes without real model calls.

In play, check pursuit against retreating Wolf, fast follow-ups after a posture break, an early breaking combo near full posture, and the new hit/miss distance columns after fresh fights.
Also check that Lua reactions still fire and a held harmless guard does not stall the boss.
Prediction remains horizontal and uses constant Wolf velocity and nominal projectile travel, as recorded in `FIDELITY.md`.

## Coach-selected posture-break punishes (2026-10-13)

Wolf's ordinary guard-break reaction is `a050_190100`, selected by his installed HKS for damage type 1001.
The extracted c0000 TAE starts unconditional guard (117), dodge (26) and jump (119) execution cancels at 1.3 seconds, before movement at 2.0 seconds and the 2.6667-second clip end.
Flags 150, 151 and 25 at 0.6 seconds accept buffered input; they do not execute a recovery.
An offline fixture drives the real HKS with repeated dodge input and confirms that Wolf remains in the break clip until the 1.3-second execution window.
The driver reads this timing from the loaded TAE and clip, then uses the current break reaction's timing when it is available.
Missing timing admits no viable options rather than inventing a stun duration.

`moves::viable_punishes` returns executable move and combo ids in deterministic lexical order.
A first hit must be reachable at Wolf's predicted position and land strictly before the remaining break window minus one simulation tick (1/60 second).
The live calculation subtracts elapsed break time and the boss's remaining data-defined cancel wait.
Melee uses the cached radial contact envelopes; projectiles solve an interception equation using target velocity, launch time, speed, lifetime, radius and parent travel.
Code does not sort or score options by predicted damage.
Each option reports its name, first-hit time, time left, potential reachable hits and HP/posture damage inside the original window, whole-move HP/posture totals, stun-lock, guard posture after the window, recovery time, and observed uses, landed uses, conversions and window damage.
Stun-lock compares consecutive contacts with Wolf's normal direct-hit recovery timings, selected by his HKS for each damage level and read from the resulting TAE cancel windows.
Unknown reaction timing is reported as null.
A stun-lock is true only for a multi-hit sequence whose later contacts arrive before the preceding reaction recovers; a single hit is false.
Window damage is potential contact damage, so read it alongside stun-lock rather than assuming every contact is guaranteed.

The coach receives `punish_window` and near/mid/far `viable_punishes` buckets, evaluated at 1.5, 4 and 8 metres with stationary Wolf.
Near means less than 3 metres, mid means 3 to less than 6, and far means at least 6.
To keep all viable rows within the 8192-byte input budget, `punish_options` maps each id to `[name, whole-move [HP, posture], recovery seconds]` once.
Bucket rows are `[id, first seconds, seconds left, hits, window [HP, posture], stun_lock, guard posture after, evidence [uses, landed], conversion rate]`.
Facts are rounded to milliseconds and three decimal places in the wire representation.
The duplicate raw `move_sheet` is trimmed before these punish facts; the existing history/digest trimming then makes room without dropping viable options, and mid-fight inputs can trim prior preference reasons while retaining their ids.
Decisions break requests receive up to four full rows in the same stable id order, including the common name/total/recovery facts inline.

The response schema adds `punish_preferences`, an object with near, mid and far ordered arrays of `{choice, reason}`.
Empty arrays mean no preference; each entry names a punish id and has a one-line reason, and validation keeps at most eight entries per bucket.
The coach may prefer a slower or weaker hit for stun-lock, follow-ups or guard posture.
At a break the driver takes the first preferred id that is currently viable.
Without a usable preference, a per-punish Thompson sampler chooses only among the viable options using actual landed and conversion outcomes with a neutral 0.5 prior.
If no option is viable, the existing fastest-reachable fallback remains available.
Legacy `on_wolf_break` plans remain readable, but ordered preferences are the executable punish policy when a move sheet is present.

Memory version 6 adds phase/bucket/id punish evidence.
Versions 1 through 5 migrate with empty punish evidence, preserving their existing data and never interpreting historical aggregate hits as successful punishes.
A use begins when the break-response plan actually starts; its observed clean hit, HP/posture damage inside the original break window, and conversion to a completed lethal throw/deathblow or other kill are recorded separately.
Those observations drive the neutral bandit and the coach's evidence columns across fights.
No hosted model calls are needed at the break.

The predictions retain the move sheet's radial-envelope approximation, rather than guaranteeing facing, obstacle clearance or target-dependent root-motion scaling.
Direct-hit recovery probes use grounded Wolf's normal reactions without situational SpEffects; projectile interception assumes constant velocity and the sheet's nominal travel.
Actual contact and damage telemetry, rather than predictions, supplies punish success evidence.

Examples from the installed c7100/c5060 fixture, with stationary Wolf and the ordinary 1.3-second window:

| Boss | Distance | Example viable ids | First hit (seconds) |
|---|---:|---|---|
| Genichiro | 1.5 m | 3000, 3000+3001, 3001 | 0.668, 0.668, 0.768 |
| Genichiro | 8 m | 3006, 3009 (bow), 3011 | 0.801, 0.950, 1.091 |
| Owl | 1.5 m | 3000, 3000+3001, 3001 | 0.568, 0.568, 0.334 |
| Owl | 8 m | 3003, 3009 (shuriken), 3014 (second shuriken) | 0.968, 0.817, 0.316 |

The complete fixture lists contain 25 near and 8 far Genichiro options, and 14 near and 7 far Owl options.
The fixture also exercises an actual guarded posture break and verifies that punish uses reach memory without hosted requests or memory-file writes.

Punish observations are provisional during a fight and immediately inform subsequent break choices.
Valid fights merge them into durable memory; void fights discard them alongside the other bandits' provisional evidence.
Only uses, landed uses and conversion rate travel in the compact coach evidence column; conversion counts and cumulative window damage remain in memory.
The preference schema includes attack groups inside tactic plans, so listed combo prefixes such as Owl's 3000+3001 are selectable even when absent from the whole-plan choice vocabulary.
