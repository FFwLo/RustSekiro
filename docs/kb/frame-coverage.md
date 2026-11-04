# What the game runs each frame that Sv1 has not ported (2026-11-03)

How the list was made: `emu-cases trace` runs one call of the exe on the Naomori-arena snapshot
(emulator-pilot.md) and records every function it enters and who called it;
`tools/emu-cases/frame_tree.py` puts them in a call tree and counts how many Sv1 cites (an address in
`src/` or `docs/kb`). Citing is a proxy: a cited function may be ported only in part, and a ported
rule may carry no address. Names are from RTTI vtable slots, strings in the code and the kb; the ones
marked (?) are guesses from what the function touches.

Run again after porting: `emu-cases <exe> extracted/emu trace wolf_frame 140a4b570 wcm 0 player
x1=0.016666668 --snapshot <dir>`, then `python tools/emu-cases/frame_tree.py out.md
extracted/emu/trace_wolf_frame.json`.

## The frame

`FUN_140a4b570(WorldChrMan, dt, chr)` is one character's frame. Order: ChrIns pre-update
`FUN_1409f78a0` (per-chr dt with hit stop), vtable slots vf22 Update, vf23, the Havok behaviour world
step (`FUN_141015fa0`, through the FD4Time singleton 0x143d93050), vf24, vf25, vf26, vf27, vf28,
vf29, vf30. WorldChrManImp's world tasks run around it: pre-update `FUN_140a509f0` and
`FUN_140a4db20`, post-update `FUN_140a50880`, group 0x51 `FUN_140a488b0`.

| Trace (dt 1/60) | Functions | Code | Cited by Sv1 |
|---|---|---|---|
| Wolf's frame | 2,510 | 871 KB | 77 |
| Enemy's frame, AI off (snapshot as taken) | 1,826 | | 54 |
| Enemy's frame, AI on (`b143d7a376=0`) | 2,392 | +99 KB over AI off | 92 |
| The four world tasks | 391 | about 230 not in Wolf's frame, 76 KB | 22 |

So the missing behaviour is in the per-character frame, not in the world tasks.

## Branches, by what they mean for the feel

Sizes count everything reachable below the function, so shared helpers (maths, allocators, param
lookups) are counted in each branch that uses them. Wolf's trace unless marked.

**Rules that shape the feel: port or check against the exe**

| Function | What | Functions / bytes / cited | State in Sv1 |
|---|---|---|---|
| `FUN_1409f78a0` | ChrIns pre-update: per-chr dt from the hit-stop module, throw and +0xc8 module ticks | 86 / 21 KB / 5 | hit stop checked frame by frame (modticks, kb/modticks.md); the other ticks are bookkeeping |
| `FUN_140a04850` ChrIns::vf22 | Update: posture and stamina regen, ChrCtrl update, modules 0x58/0x88/0x20/0x68 | 436 / 107 KB / 22 | regen ported (kb/enemy.md, damage.md) |
| `FUN_140a37560` PlayerIns::vf22 | Player update around ChrIns::vf22 (weapon glow, player state) | 807 / 210 KB / 32 | partly |
| `FUN_1407ef120` PadManipulator::vf9, `FUN_1407ef960` | Pad input to action requests | 116 / 23 KB / 9 | input buffer ported (kb/input-buffer.md) |
| `FUN_1407f3c90` PlayerCtrl::vf17 | Yaw turn controller | 61 / 14 KB / 3 | turn agent (2026-11-03) |
| `FUN_1407e2400` | ChrCtrl movement and facing step | 71 / 17 KB / 5 | turn agent |
| `FUN_140b45440`, `FUN_140b49d30` | Behaviour module before Havok: animation speed, hkb variables (TurnAngle) | 120 / 36 KB / 5 | partly (kb/behaviour-graph.md) |
| `FUN_140b44980` → `FUN_140b5c730` | Behaviour module after Havok: replays the TAE windows queued during the multithreaded Havok step (the window rule is FUN_140654b70 -> FUN_1425e5640), root motion | 127 / 32 KB / 5 | window rule checked and ported (kb/taewin.md, `src/tae_window.rs`) |
| `FUN_140a03340`, `FUN_140a02a10` ChrIns::vf44, `FUN_140a36cc0` PlayerIns::vf44 | SpEffect list tick and the state-info reactions | 74 / 12 KB / 5; 218 / 37 KB / 21; 253 / 44 KB / 22 | partly (kb/guard-deflect.md, skills.md) |
| `FUN_140be1b50` | Throw module (CSChrThrowModule, bag +0x88) tick: clears the request, empties the candidate list; decides nothing (the search is FUN_140be2a30) | 11 / 9 KB / 0 | range and throwable states checked (kb/guard-deflect.md 2026-11-03); the tick has nothing to port |
| `FUN_140a2e690` → `FUN_140a315e0`, `FUN_14083e060`, `FUN_140a31f30` | Player stats recompute: attack power per weapon and damage type, defence, equip load | 261 / 61 KB / 11 | damage agent checks amounts |
| `FUN_1407ded90` → `FUN_1407e67a0` → `FUN_140608990` (enemy, AI on) | Enemy AI frame: manipulator, targeting (SprjTargetingSystem::vf2) | 656 / 112 KB / 44 | AI from the battle Lua; the C++ side partly |
| `FUN_140b7d950` | Docking module: wall hug, ledge hang (?) | 160 / 45 KB / 2 | parked (kb/docking.md) |

**Port only if Sv1 lacks the result**

| Function | What | Functions / bytes / cited |
|---|---|---|
| `FUN_140fe7310` CSHkBehCharaProxySceneModifier::vf3 | Havok character proxy and the physics world step: collision movement, ground contact | 434 / 157 KB / 6 |
| `FUN_140bd0a70`, `FUN_1409f6860`, `FUN_1409d3470` ChrActPntSlot::vf3 | Action points: the player's prompts (?) | 100 / 16 KB / 5; 171 / 30 KB / 6 |
| `FUN_1407df170`, `FUN_140957810`, `FUN_140a04e30` ChrIns::vf27 | Ragdoll (FrpgRagdollIns) | 63 / 24 KB / 1; 43 / 22 KB / 0; 99 / 31 KB / 0 |

**Engine plumbing: no port**

- Havok behaviour world scheduling: `FUN_141015fa0` → `FUN_1413b99d0` hkbWorld::customStepMultithreaded,
  the task queue `FUN_141669340` / `FUN_14169b540`, hkbWorld::updateCharacter `FUN_1413a73c0`,
  hkbBehaviorGraph::update / generate. 1,330 functions, 514 KB, 27 cited: the largest branch, and
  Sv1 already plays the graph's result. What the graph decides (kb/behaviour-graph.md) is a port
  question; the scheduling is not.
- Foot IK `FUN_140fe7a50` (cosmetic), hkaAnimatedSkeleton sampling `FUN_1416084a0`.
- Equipment model attach and draw: PlayerIns::vf27 `FUN_140a38a60`, vf28 `FUN_140a38b60`,
  `FUN_140a361c0`, `FUN_140e6*`.
- Model pose push ChrIns::vf29 `FUN_140a04500`; end-of-frame cleanup and music request ChrIns::vf30
  `FUN_140a04760`.

## Gaps of this list

- One frame, one moment: standing in the arena, no input, no attack, no hit. The actions are in
  "Mid-action frames" below (2026-11-03); a landed hit, a real deflect, grapple and death are not.
- Called through tail jumps: 38 of the 185 functions the pre-update task entered have no caller edge
  (the tree shows 147).
- The module bag offsets were matched to their classes by what they do, not from the constructor.

## Mid-action frames (2026-11-03, acttrace)

How: `emu-cases <exe> extracted/emu acttrace [acttrace_<name>...] --snapshot <dir>`
(`tools/emu-cases/src/acttrace.rs`) runs whole chr frames `FUN_140a4b570` at 1/60 from the
snapshot, Wolf's pad written before each frame: the pad frame `[pad+0x38f8]` byte `+0x67 + id`
per HKS ACTION_ARM (id from `FUN_140b258d0`, the map `FUN_1407ef960` reads: attack 14, sub 15,
guard 16, wire 17, jump 89, step 90, item 92), `+0x3c` / `+0x3e` attack / guard held, `+0x0` /
`+0x4` the stick. Each run writes `acttrace_<name>.json` (`trace`'s format, plus the anim per
frame). Then `frame_tree.py out.md acttrace_<name>.json 3000 --base acttrace_idle.json --base
trace_wolf_frame.json [--show <addr>]` lists only what the idle frames did not enter.

Emulator-copy patches (none touch the game): the snapshot has a menu open (`FUN_1408c58e0` =
0x800008, `[FUN_140d872c0()+0x100]` set), so `FUN_1407ef960` dropped every button; the copy gets
`xor eax, eax; ret` at `FUN_1408c58e0` and the menu window cleared. CRT ISA level 1 and the plain
longjmp (as `proxy`). The General: chr `+0x110` (update omission, 30 frames at his distance) = 0;
his AI on (`b143d7a376=0`) never attacked in 600 frames, so the forced runs write his AI's attack
request (AI `+0x7430`, SetAttackRequest) = 3000 at the entry of the manipulator's copy
`FUN_1407e6570` for 3 frames; he starts a000_003000 the next frame.

| Run | Frames | Anims seen (besides idle) | Functions | New vs idle (fn / KB / cited) | Not in half the actions |
|---|---|---|---|---|---|
| idle | 60 | - | 2,648 | - | - |
| attack (R1 f0-2, f24-26) | 70 | 50300000, 50300100 | 3,672 | 1,016 / 216 / 40 | 90 / 23 / 3 |
| guard (held 40) | 60 | 50002000, 50203000, 50203010 | 3,563 | 910 / 192 / 27 | 152 / 38 / 0 |
| deflect (guard tap, nothing to deflect) | 40 | 50203000, 50203010 | 3,555 | 902 / 191 / 27 | 144 / 36 / 0 |
| jump | 70 | 201001, 201011, 201040, 201050 | 3,681 | 1,024 / 213 / 44 | 119 / 29 / 11 |
| step (stick right) | 45 | 400, 213300 | 3,813 | 1,154 / 246 / 36 | 217 / 51 / 2 |
| sprint (step held, stick forward) | 60 | 200000, 213300 | 3,718 | 1,059 / 230 / 46 | 166 / 49 / 15 |
| combat art (R1 + L1) | 90 | 107316700, 107316710, 107316730 | 3,754 | 1,097 / 229 / 52 | 155 / 32 / 12 |
| prosthetic | 70 | 73400000, 73400100 | 3,575 | 918 / 192 / 27 | 123 / 20 / 2 |
| item | 70 | 250000 | 3,652 | 1,000 / 196 / 22 | 260 / 46 / 7 |
| grapple | 60 | none (no point in reach) | 2,650 | 2 / 0 / 1 | - |
| General attack (forced 3000) | 122 | 3000 | 3,151 | 734 / 128 / 27 (vs his AI-on and AI-off frames) | - |
| duel: world tasks + General + Wolf, block / tap / nothing | 92 each | Wolf 50002000 / 50203005-7 / idle | 5,362 / 5,381 / 5,205 | 287-306 not in any other run / 43-48 KB / 12-14 | - |

So every action enters about 1,000 functions (200 KB) the idle frames do not, and 947 of them
(198 KB, 40 cited) are the same whatever the action: an anim change, not a rule.

### Common to any action (947 fn / 198 KB / 40 cited)

- Engine plumbing, no port: the Havok behaviour transitions and blends of a new anim under
  `FUN_141375970` (hkbWorld::updateCharacter `FUN_1413a73c0`; 358 fn / 98 KB / 14 cited, e.g.
  `FUN_141379460`, `FUN_14137a880`, `FUN_1413c59f0`, `FUN_1413a2580`); sfx and sound slots
  under ChrIns::vf30 -> `FUN_1409f6860` (ChrMagicGoodsUseSfxSlot / ChrMultiSfxSlot /
  ChrSingleSfxSlot::vf3, `FUN_14092afe0`, FXR `FUN_141d7c030`; 283 fn / 45 KB / 2 cited); the
  TAE sound / sfx events `FUN_140b50930` PlaySound, `FUN_140b53520` SE emit, `FUN_140b54690`;
  the HKS Lua compiler (`FUN_14145d750`, `FUN_14145d920`, `FUN_14143bf70` loadstring: the scripts
  compile a chunk on some state changes); the TAE context fill `FUN_140b4f790` (PlayerIns::vf25 ->
  `FUN_140a3bc90`); model lookups `FUN_14102b5b0`.
- Already cited: the ChrActionFlag handler `FUN_140b51b70` and its sub-id sibling `FUN_140b53170`
  (every action, 50-1,200 calls a run), the docking module `FUN_140b7d950` (parked).

### Per action, what only it adds (frame_tree, base = idle + the common set)

- **Attack** (90 fn / 23 KB / 3): `FUN_140b4fba0` (TAE window walk) -> `FUN_140b50ae0` TAE
  InvokeAttackBehavior -> `FUN_140b679e0` attack record -> `FUN_14099fc00` (up to 16 attack
  entries from a BehaviorParam row) -> `FUN_14099f310` -> `FUN_1409a1290` (cited: issues the
  attack, AppearAiSoundId) and `FUN_140998980` -> SprjPlayerDamageModule::vf8; `FUN_1410b6150` ->
  `FUN_1410b6880` (cited, the hit's SpEffects) -> PlayerIns::vf62; `FUN_140a240e0` PlayerIns::vf63
  (a per-behaviour cost from param table 0x7e x chr +0x21ec / 100 (?)); `FUN_140b5b280` (debug
  hit-point sfx). The whole InvokeAttackBehavior branch: 138 fn / 27 KB / 10 cited (art run).
  Input: `FUN_1407ed630` (stick toward camera / facing; sets ActionRequest 0x19 / 0x1a with
  attack and sub-attack). Trace sfx `FUN_1409e0310` (plumbing).
- **Guard, deflect tap** (152 fn / 38 KB / 0): only footstep sfx (`FUN_1409d8710`
  ChrFootEffectSlot::vf3, 122 fn / 33 KB) and the sfx event `FUN_140b54710`. The guard rules
  themselves run in code the idle frame already enters (ChrActionFlag, the HKS); the hit-side
  guard (`FUN_140b6c880`) needs a landed hit (below).
- **Jump** (119 fn / 29 KB / 11): physics module step `FUN_140bccd50` (`[chr+0x1ff8]+0x68`,
  velocity +0xa50) -> the fall module `FUN_140bac120` (cited, air control) and its sibling
  `FUN_140bacc10` (2.7 KB, uncited); the throw range check `FUN_140be0270` (cited; the air
  deathblow search) from `FUN_140be3a90`; `FUN_140b55510` TAE behaviour event -> `FUN_140b67740`
  (85 calls; jump, step, sprint, art); landing / fall check `FUN_140b97a30` -> `FUN_140b97b30`
  (fall damage by the scout; also runs on still grounded frames of attack, step, prosthetic).
- **Step** (217 fn / 51 KB / 2): footstep sfx (114 fn / 32 KB), `FUN_140b54710`, `FUN_140b55510`,
  `FUN_140611c10` (5 fn, AI-side, probably the noise the step makes).
- **Sprint** (166 fn / 49 KB / 15): `FUN_140bccd50` -> fall module as jump (ran off ground);
  ledge-hang probing `FUN_140b788d0` from `FUN_140b761d0` ("DockingTranslationOffset_Hang"; the
  docking module, parked); `FUN_140be0270`.
- **Combat art** (155 fn / 32 KB / 12): InvokeAttackBehavior as attack (27 calls), the physics /
  fall step, `FUN_140be0270`, `FUN_140b55510`, bone matrix fetch `FUN_140e7dc30`.
- **Prosthetic** (123 fn / 20 KB / 2): FXR `FUN_141dc4720`, trace sfx, `FUN_140b50f00` (sfx slot),
  handle pool `FUN_140987d60`. No bullet issued in 70 frames (`FUN_140b0fc60` not entered).
- **Item** (260 fn / 46 KB / 7): TAE event `FUN_140b57740` (152 fn / 33 KB / 2): IsItemUseable
  `FUN_1409f3890` (cited, hks_env 233) -> `FUN_14084ee80` -> `FUN_1408522b0` goods use (58 fn /
  14 KB, uncited) -> `FUN_1409a8b20`; the SpEffect add `FUN_1407d5d50` (67 fn / 13 KB) -> camera
  fade CSCamFadeSlot::vf6 (plumbing); PlayerIns::vf125 `FUN_140a262d0` -> AdjustItemCount
  `FUN_14079ab70` (cited). Sfx ChrMagicGoodsUseSfxSlot (plumbing).
- **General's attack** (734 fn / 128 KB / 27; base his AI-on and AI-off frames): the same
  InvokeAttackBehavior branch (161 fn / 32 KB / 12: `FUN_14099fc00`, `FUN_1409a1290`,
  `FUN_1410b6880`, `FUN_14084fc50` -> `FUN_140bf8900` 8 KB uncited), footstep / multi / trace sfx
  (plumbing, 80 KB), `FUN_140b54710`, Havok `FUN_1413a3880`, ComManipulator::vf11 ->
  `FUN_140feb320`, `FUN_140b97b30`.
- **Duel** (both chrs and the four world tasks, 2.5 m apart): adds the AI's attack goal
  SprjGoalCommonAttack::vf2 / vf3 (cited), targeting `FUN_140628860`, `FUN_140629250`,
  TargetSearchSysSlot_NormalFriend::vf1 (cited), EzState callback `FUN_1411de8b0`, physics
  `FUN_1418fb080`, `FUN_140bc5470`. No hit landed: Wolf's HP stayed 548 and his anims show no
  hit or block reaction; the hit delivery `FUN_140a4f940` (pre-update task) ran every frame with
  nothing queued, and the hit resolution chain (`FUN_14099aab0` <- `FUN_1409a05c0`;
  `FUN_140b68ff0` -> `FUN_140b6f690` -> `FUN_140b6c880` defender) was never entered. The attack
  record is issued (`FUN_1409a1290`) but the collision test that would queue the hit runs in a
  task outside the chr frames and these four (the Havok physics world step or a hit manager, not
  found yet), or the General's dummy polys stay at his snapshot pose. (2026-11-03 hits: found, the
  hit manager step FUN_1409a0dc0 in MoveMapStep's task FUN_140a746f0; hits land with it, kb/damage.md
  "Landed hits".)

### Next wave: the biggest uncited gameplay branches

Ranked by uncited bytes of rule code (plumbing left out):

1. Item use: `FUN_140b57740` -> goods use `FUN_1408522b0` (14 KB) and the SpEffect add from an
   item `FUN_1407d5d50` (13 KB, minus the camera fade): what a gourd / pellet does on use, frame
   by frame. Sv1 has the gourd as SpEffect 3000 (kb/enemy.md); check its timing and amounts.
2. The attack record build `FUN_140b50ae0` -> `FUN_140b679e0` -> `FUN_14099fc00` (17-22 KB,
   9 cited) with `FUN_14084fc50` -> `FUN_140bf8900` (8 KB uncited): which attack params and
   SpEffects an InvokeAttackBehavior event issues, for Wolf, arts and the General. Check against
   Sv1's attack table per event.
3. The hit test itself: find the task that runs the attack collision (`FUN_14099aab0` /
   `FUN_1409a05c0`, the hit list `FUN_140a4f940` reads) and trace a duel frame with it; without it
   no landed hit, real deflect or block reaction can be traced.
4. Physics / fall step `FUN_140bccd50` (8 KB, 1 cited) and `FUN_140bacc10` (2.7 KB): the air move
   next to the ported air control; check jump arcs frame by frame.
5. `FUN_140b55510` -> `FUN_140b67740` (1 KB, 85-114 calls per run in jump / step / sprint / art): a
   second behaviour-spawn TAE event (chr vfunc 0x1c8 path).
6. PlayerIns::vf63 `FUN_140a240e0` (a behaviour's cost, x chr +0x21ec) and `FUN_1407ed630` (stick
   direction -> ActionRequest 0x19 / 0x1a: which attack variant a held direction picks).
7. Landing / fall check `FUN_140b97a30` -> `FUN_140b97b30` (2.8 KB): fall damage and its
   thresholds (0.3 for the landing reaction).
8. Ledge-hang probe `FUN_140b788d0` (4.5 KB): with the docking module, parked.

## One frame's order (2026-11-03, animstart)

How: `emu-cases <exe> extracted/emu animstart [animstart_<case>...] --snapshot <dir>`
(`tools/emu-cases/src/animstart.rs`) runs whole chr frames `FUN_140a4b570` from the snapshot as
acttrace does (menu patch, pad per frame) with code hooks on each stage and a write hook on the
TimeAct module's current anim (`[chr+0x1ff8]+0x10 +0xf8`); per frame it logs the stages in call
order. Cases: Wolf's attack, guard and Loaded Axe pressed on frame 2 (held 3 / 2 / 3), the
General's forced 3000 (AI on, request on frames 2-4). Test
`a_pressed_clip_starts_the_frame_after_the_press` (`src/sim_tests/emu_animstart.rs`).

**The world frame.** MoveMapStep's tasks run by task group: the hit manager step `FUN_1409a0dc0`
inside `FUN_140a746f0` (group 0x4b) before the player's chr frame in `FUN_140a70020` (group
0x52; the groups from the task registrations in MoveMapStep's setup). So a hit is tested on the
poses and attack records the chrs left the frame before.

**One chr frame**, as logged (Wolf and the General alike):
1. pre-update `FUN_1409f78a0`;
2. vf22: the pad manipulator `FUN_1407ef120` -> its button map `FUN_1407ef960` (the AI's request
   copy `FUN_1407e6570` for the General), the behaviour module before Havok `FUN_140b45440`;
3. vf23, then the Havok behaviour step `FUN_141015fa0`. Inside it, in order: a few HKS acts of the
   current state (101 / 148 / 2015), the current clip's TAE update `FUN_140b5c920` with its in-step
   handlers (the ChrActionFlag handler `FUN_140b51b70`, from `FUN_140b4fba0`), then the HKS
   decisions (env / act dispatchers `FUN_140b36410` / `FUN_140b32410`, called from 0x141010b64;
   on the press frame acts 157, 127, 9101, 154, 3029), then the upper layer clip's TAE
   (a000_790010);
4. vf24: the behaviour module after Havok `FUN_140b44980` -> the queue replay `FUN_140b5c730` ->
   `FUN_140b58530` (the windows' other handlers);
5. vf25 .. vf30.

So the HKS decides after the frame's clip step and sees the ChrActionFlags of that step's window;
the clip it asks for takes its first update [0, 1/60] in the next frame's step:

| Case | Pressed | Old clip's last step | New clip's first window |
|---|---|---|---|
| attack a050_300000 | f2-4 | f2 (a000_000000 2.604 -> 2.621) | f3 (0 -> 0.0167) |
| guard a050_203000 | f2-3 | f2 | f3 |
| Loaded Axe a073_400000 | f2-4 | f2 | f3 |
| General a000_003000 | request f2-4 | f2 | f3 |

The hits runs say the same of a reaction: the hit step of frame h, the reaction's first window h + 1.

**What changed in Sv1.** Sv1's `decide` runs before `advance` and reads the window the last
`advance` took: it stands for the exe's HKS of the frame before. But it read the frame's pad, so a
press played its clip one frame early (the Loaded Axe release on 48 / payment on 73 for the exe's
49 / 74; a guard's first step in the press frame), and `resolve` ran after `advance`, a whole HKS
before the exe's hit step (a hit's damage and reaction a frame early relative to the contact pose,
kb/damage.md "Landed hits"). Now:
- `player::hks_pad_in`: `decide` reads the presses and held buttons of the frame before (the stick
  stays live: the turn and move speed are vf22's, before the step). The HKS's own stick reads (a
  step's or an attack's direction, the run stop) take the frame before's too since 2026-11-04
  ("The HKS's stick" below).
- `ActorSet` order Decide, Resolve, Advance: the hit test runs on the poses and attack windows of
  the last step, as the hit step does; a clip played in `resolve` (a reaction) is `Actor::fresh`
  and takes its first step in the next tick (`clear_fresh`, after decide, clears the ones decide
  or the last tick's later systems played). The throw resolution `resolve_throws` stays after the
  step (TAE 304 is the chr frame's, not the hit step's). PoseSet, bullets, sounds and particles
  stay after the step.
- `Actor::kb_wait` 2 (the hit's own step, then the reaction's first; the exe moves the defender
  from h + 2); a bullet's knockback (fly_bullets, after the step) waits 1.
- The gourd's ConsumeCurrentGoods (TAE 65) moved from `decide` to `consume_goods` after the step:
  the exe runs it in the frame whose step enters the event (frame 42 for 0.7 s).

Checked: animstart (3 Wolf cases: the frame and clip time of the first step), items (gourd heal on
42, Loaded Axe release 49 / payment 74, were 48 / 73), hits (verdicts, the deflect window by guard
clip time, the knockback's first frame and travel, now read on Sv1's frame labels with no offset),
taewin, taestate, turn, air, modticks as before.

Tests updated, on the exe evidence that a press reaches the HKS's clip the frame after:
`a_key_press_reaches_the_same_frames_fixed_step` (the attack starts the tick after the key; the
press still leaves the pad in the key's frame), `crouching_cuts_how_far_enemies_see_wolf`,
`switching_prosthetics_plays_the_next_tool`, `deathblow_from_behind_uses_the_back_throw`,
`a_grapple_lands_on_the_point_in_the_move_start_time`, `a_reset_mid_jump_lands_wolf_at_the_start`,
`use_art` (`every_art_starts_and_costs_its_emblems` and the other art tests): one tick more after
the press; `a_locked_sprint_deflect_turns_to_the_enemy_instead_of_snapping`,
`guard_release_while_moving_keeps_running`: the press / release read a tick later;
`arts_in_the_air`: the art pressed a tick earlier relative to the launch (it reaches decide at the
same point); `sheathe_walk_and_draw`: the stick let go a tick after the attack press (the stick gap
above); `an_early_jump_in_an_air_slash_becomes_the_air_kick`: one attack press (the test pressed
every tick until the slash showed, which now queued a second); `a_grab_pulls_wolf_onto_the_grabbers_dummy`:
checked a tick after the grab (the absorb now lands in the grab's own tick; the pair then drifts
3.5 cm a tick, as before); `landed_hits_match_the_exe`: Sv1's guard clip time and contact pose
read on the frame before the hit (no + dt). `force_player` primes the pad (`HksPad::prime`): a
forced state stands for one Wolf was already in, with the buttons already down.

Not covered (gap): (the old clip's last step on the frame a reaction starts and the windows of a
clip `decide` replaces in the same tick: done 2026-11-04, "Frame order at a clip change" below);
enemies' frame labels (their AI decides in `decide`, the exe's
HKS of the frame before, so only Wolf's pad needed the delay; the General's frames were logged but
not compared with Sv1's); hit-stop frames (done 2026-11-04, kb/modticks.md "Which frame a hit's stop starts on"); (a bullet's hit timing: traced 2026-11-04, "A bullet's
hit timing" below); other TAE events `decide` read a tick after the step (one-shot
SpEffects 401 / 67, weapon style 32): the exe runs them in the step's frame (fixed 2026-11-04,
"One-shot TAE events' frame" below).

## One-shot TAE events' frame (2026-11-04, frameorder, row E4)

How: `emu-cases <exe> extracted/emu frameorder [frameorder_<act>...] --snapshot <dir>`
(`tools/emu-cases/src/frameorder.rs`) runs Wolf's whole chr frames `FUN_140a4b570` through acttrace's acts
(attack, art, deflect, step, jump, prosthetic) and `style` (standing; his idle a000_000000 given two
synthetic TAE 32 events, as taewin's synthetic lists, since none of the acts' clips carries a 32). Hooks
log in call order the TAE update `FUN_140b5c920` (clip, prev, cur), the Havok step `FUN_141015fa0`, the
replay `FUN_140b5c730`, the event dispatcher `FUN_140b59170`'s handlers for 32 (`FUN_140b509b0`), 66 / 67
(`FUN_140b56500`, r8 0 / 1) and 401 (`FUN_140b56750`) with the event's arg and entered byte (+0x18), and
the SpEffect add `FUN_1409e9c00` on Wolf. 7 runs, 399 frames, 1,555 handler dispatches (84 entering),
68 adds from them. Tests `one_shot_tae_events_land_in_the_steps_frame_in_the_exe`,
`one_shot_tae_events_land_in_the_steps_tick` (`src/sim_tests/emu_frameorder.rs`).

**What the exe does.** The handlers run inside the frame's Havok step (with the multithreaded flag
clear, as in the snapshot, the TAE update dispatches there; never in the replay), on every update the
event overlaps. A 67 / 401 adds its SpEffect only on the update that enters the event (the window rule,
kb/taewin.md; on later updates no add: 16 entering dispatches added nothing, the effect already on);
every entering update matched `tae_window::starts` on its (prev, cur). So the SpEffect is on in the frame
whose step enters the event, before that frame's HKS and its SpEffect tick (vf25 -> vf44). The 32
handler writes the style on each overlapping update (gated by vfunc 0x1a8 only), first in the entering
step (`style`: frame 4 for the event 3.5 frames ahead).

**What changed in Sv1.** Its one-shots were read in `decide` on the window the last `advance` took, so
they landed a tick after the step: the TAE 32 weapon style, the one-shot recoveries (110015 half HP,
105050 / 1503x1 posture and HP, the Todome 105051 / 1503x2 held rows) and `apply_refresh_effects` (401 /
67 refresh rows: 8002-8009 cure, 8001 posture, Ichimonji's 140210). Now `player::tae_one_shots` and
`apply_refresh_effects` run right after `ActorSet::Advance` (before `resolve_throws` and the poses) on
the window that step took. `decide` of the next tick (the exe's HKS of this frame) sees them as the HKS
does. Before -> after: the sheathe's style None (a000_700500 frame 14) and the revival's f0 effects
(GroundRevival_F) on the tick after the entering step -> on it.

Not covered (gap): enemies' TAE SpEffects (`enemy::think` reads 66 / 67 / 401 AI-state rows as a state
at t, and `ffx.rs` / `CharData::sp_effects_at` treat 67 as on while the event overlaps, where the exe adds
once on entry and the row's own endurance decides how long it lasts); the 66 handler (r8 0, not hit in
these runs); which check stops a re-add (FUN_140bfefb0 / FUN_140c00340, the "already on" refresh) per row;
a real clip's 32 (only the synthetic one ran); the 401 path in another state than the deflect's f0.

## The HKS's stick (2026-11-04, hksstick, row E1)

How: `emu-cases <exe> extracted/emu hksstick [hksstick_<case>...] --snapshot <dir>`
(`tools/emu-cases/src/hksstick.rs`) runs Wolf's whole chr frames `FUN_140a4b570` from the snapshot as
animstart does (menu patch, the pad's stick floats and arm bytes written per frame). Hooks log, in call
order, the locomotion variables `FUN_140b4afd0`, the Havok step `FUN_141015fa0`, the frame's first HKS
env call and every HKS act, with the stick-derived fields read at that moment: behaviour +0xbac
MoveSpeedLevel, +0xbb0 MoveAngle, the four direction / level pairs from +0xb30 (each a world vector
and the stick's length; FUN_140b4afd0 hands them to the graph as the Jump / Attack / SubAttack / Kick
angle and level), the manipulator's move vector +0xc0, the physics yaw. Cases, stick changed on frame
8: `half` (full -> half), `release` (full -> let go), `step_turn` (turned 40 deg, inside the gate, with
the step pressed on 8-9). Test `the_script_reads_the_stick_of_its_buttons_frame`
(`src/sim_tests/emu_hksstick.rs`).

**What the exe does.** Every frame `FUN_140b4afd0` (from `FUN_140b49d30`, called by the behaviour
pre-step `FUN_140b45440` in vf22) runs after the pad manipulator and before the Havok step, so the
HKS reads the stick of its own frame, the same pad sample as its buttons:
- `half`: MoveSpeedLevel and the pairs' level read 0.5 at frame 8's HKS (1.0 up to 7);
- `release`: 0 at frame 8's HKS, and the run stop fires there (acts 157, 9101 on frame 8);
- `step_turn`: frame 8's HKS sees the move vector and the four pairs already 40 deg round with the
  step's press (acts 157, 127, 9101, 3029 on 8); the yaw snaps to it on 9 (BehaviorTurn, kb/movement.md).
- the gate (kb/movement.md 2026-11-03) holds at the HKS too: the press from rest on frame 0 reads 0 there
  and 1 from frame 1.

**What changed in Sv1.** The script and `decide` tick in the place of the exe's HKS of the frame
before, and since animstart they read the frame before's buttons but this frame's stick (`decide` even
the raw, ungated one): a let-go fired the run stop a tick before the buttons' frame, and a step or
attack took the direction of the frame after its press. Now `player::pad_manipulator` keeps the stick
taken the tick before in `Turn::hks` (a primed tick, `HksPad::prime`, takes the frame's as its buttons
do). `hks_wolf::run_script` hands it to the script (MoveSpeedLevel, MoveAngle, TurnAngle, the Kick /
Jump / Attack / SubAttack angle and level, `step_aim`, the graph's locomotion steering), and
`decide`'s `stick` is it too (action facings, step / jump directions, starts and stops, the run's
speed level and clip choice in `locomotion` / `lower_body_move`). Only the free turn (`free_turn`,
the manipulator's yaw request, vf22's: emu_turn) keeps this frame's taken stick (`move_stick`).
hks_lua's lockstep answers the script with the frame before's stick as well. Checked: Sv1's script on
tick f + 1 reads the exe's frame-f MoveSpeedLevel and AttackStickLevel (each pair is a 0x20 block from
+0xb30, its level at +0x10) on all 44 frames of the three cases.

What this means in play, as in the exe: a stick pushed from rest reaches the HKS two Sv1 ticks later
(the gate's frame, then the HKS's frame), so a step or attack pressed on the very frame the stick
leaves centre (or turns more than 45 deg) reads no stick: a neutral step, AttackAngle 0. Tests that
pushed the stick and pressed in one tick now push it a tick before (`an_unlocked_step_ignores_the_auto_aim_target`,
`sprint_attack_states`, `a_sprint_jump_is_the_fast_long_jump`, the Nightjar step in
`art_out_of_a_sprint_and_nightjar_steps`, `a_step_out_of_a_large_hit_is_the_damage_step`,
`atlas_damage_step_from_down_tilts_by_the_stick`; `attack_after_deflect_release` two ticks before the
release); `standing_a_stick_behind_quick_turns_into_the_move` stands one tick more;
`releasing_the_stick_at_a_run_stops_in_about_a_third_of_a_metre` measures the slide from the let-go
tick (that tick still steps the run, as the exe's does).

Not covered (gap): where in the world frame the pad device fills the stick floats ([pad +0x38f8]):
the emulator writes them before the chr frame, so a device sample taken after the chr update would
add a frame for the whole pad (buttons and stick alike, not for one against the other); the camera
yaw the HKS's world stick is turned by is this tick's orbit, not the frame before's; MoveAngle /
TurnAngle against the facing were not compared (free they read 0 in the exe, Sv1 writes its own
turn); the locked-on manipulator (no gate, no target in these cases); air control, swimming, aging and
docking moves in `decide` read the HKS stick too, though the exe's may be vf22's (not traced).

## Item use and prosthetic bullets (2026-11-03, items)

How: `emu-cases <exe> extracted/emu items [items_<case>...] --snapshot <dir>`
(`tools/emu-cases/src/items.rs`) runs Wolf's whole chr frames from the snapshot as acttrace does
(menu patch, pad written per frame), the item button (or the prosthetic) held on frames 0-2, with
code hooks on the use chain; per frame it writes the TAE clips updated, HP, the SpEffects added /
gone and every hooked call. The item is chosen through the inventory override `[chr+0x2000]+0x278
+0x358` (`FUN_14079e2a0`; `FUN_140a369b0` copies it to the quick item chr `+0x11c0` every frame,
`FUN_140a28160` to the item in use `+0x11c4` when the HKS fixes it, act 2024). Inventory: list
`+0x278+0x168`, `[+0x88]+1` slots, entry +4 id, +8 count (`FUN_1407aabf0`).

The chain: TAE 65 `FUN_140b57740` (item `+0x11c4`, uses `+0x11c8`) -> `FUN_1407d5d50` (the bullet
emitter ChrIns `+0x10e0`, goods id) -> launch `FUN_1409e4cd0`: EquipParamGoods refCategory (+0x3b)
2 -> `FUN_1409e4fe0` -> SpEffect add `FUN_1409e9c00`; 1 -> `FUN_1409e4e60` -> spawn record
`FUN_1409e4950` -> `FUN_1409e5660` -> BulletMan `FUN_140b1ad20` (record +0x10 BulletParam id,
+0x40 the emit matrix: rows x, y, z with the position in the 4th column). Then PlayerIns::vf125
`FUN_140a262d0` -> `FUN_14079afa0` -> AdjustItemCount `FUN_14079ab70` (slot, -1). All on the
event's frame. A TAE 2 BulletBehavior is `FUN_140b51110` -> emitter `FUN_1407d4fa0` (BehaviorParam
id): skipped for the main player when the row's +0x18 cost is set and vf 0x378 (the resource held)
is < 1.

| Case (goods) | Clip | Use frame (t) | What the use does |
|---|---|---|---|
| gourd 3000 | a000_250000 from f1 | f42 (0.7 s) | SpEffect 3000 added, HP 548 -> 1120 (cap; +672 = 1120 x 40 % x 1.5) the same frame, slot 272 -1 |
| pellet 3020 | a000_250300 | f48 (0.8 s) | Bullet 3020 at the hand (201.79, -24.73, 23.64), slot 489 -1; its 3021 / shooter 3020 not on Wolf within 90 frames (the bullet's own update is a world task, not run) |
| Ako's sugar 3400 | a000_250800 | f66 (1.1 s) | Bullet 3400, -1 |
| Gokan's sugar 3420 | a000_251000 | f66 (1.1 s) | Bullet 3420, -1 (and SpEffect 110140 at f1) |
| spiritfall 3600 | a000_250500 | f54 (0.9 s) | Bullet 3600, -1 |
| 3510 | a000_251400 (ItemStone) | f30 (0.5 s) | Bullet 3510, -1 |
| ohagi 3060 (not held; written into slot 0) | a000_259000 ItemFailed | - | refused (IsItemUseable) |
| Loaded Axe 73000 (the snapshot's tool), tap | a073_400000 f1-48, release a073_400100 from f49 | TAE 2 judge 297 (dummy 603) f66-68: no bullet (`FUN_1407d4fa0` not reached) | 2 Spirit Emblems (goods 1000, slot 293) at f74 = TAE 1 judge 102 (0.433 s) |

Compared with RSekiro (`src/sim_tests/emu_items.rs`):
- `gourd_use_matches_the_exe`: the heal and the charge on the exe's frame 42; matched as it was.
- `loaded_axe_emblems_match_the_exe`: the payment 25 frames into the release and its 2 emblems
  match; ours started a073_400000 one frame early (t 0.8 on frame 47, the exe's 48), so the
  release (exe 49) and payment (exe 74) were on 48 / 73. Fixed 2026-11-03 (animstart, "One
  frame's order" above): now 49 / 74.
- gap: RSekiro uses only the gourd. Pellets, sugars, spiritfall and throwables (goods
  refCategory 1: their Bullet row's shooter SpEffect / HitBulletID) are not in the player; the
  table above gives their clips, use frames and Bullet rows for a port.
- (closed 2026-11-04, row E6) writing PlayerGameData +0x5ec alone did nothing: the id there is
  only a mirror. The equip path is in "Prosthetic tools in the exe" below, and every tool now runs.
- gap: a written count of 0 did not stop the gourd (the drink and -1 ran on; the count's real
  field is not found); the empty gourd is not checked.

### Gaps of the mid-action list

- No hit lands (above): getting hit, block and deflect reactions, posture break, death not traced.
- Grapple: no grapple point in reach from the snapshot spot (no wire anim; 2 functions new).
- Prosthetic: no bullet issued within 70 frames; the equipped tool and its ammo are as saved.
- One input timing per action; combos past the second slash, air attacks, crouch, the other arts
  and tools are not run. The General runs only behaviour 3000.
- Names of the new branches come from a read of their first lines (strings, RTTI, fields);
  "(?)" ones are guesses.

## A bullet's hit timing (2026-11-04, bullets)

How: `emu-cases <exe> extracted/emu bullets [bullets_<case>...] --snapshot <dir>`
(`tools/emu-cases/src/bullets.rs`) runs whole world frames as `hits` does with the bullet step in
front: per frame the BulletMan step, the hit manager step `FUN_1409a0dc0`, the WorldChrMan pre
tasks, the General's chr frame (AI off, updated every frame), Wolf's, the post tasks. Wolf (placed
4 / 6 / 8 m in front of the General, facing him) uses a Pellet (goods 3020, items.rs); at the spawn
`FUN_140b1ad20` the record's BulletParam id (+0x10) becomes the shuriken's 700000 and its emit
matrix (+0x40) points from the hand at the General's chest. Per frame: each live bullet slot after
the bullet step and after the whole frame, both chrs' state (hits.rs), every resolve with the
phase it ran in, the watched functions per phase. Test
`a_thrown_shuriken_flies_and_lands_like_the_exe` (`src/sim_tests/emu_bullets.rs`).

**Where bullets run.** MoveMapStep registers its update tasks with groups: `FUN_140a74560` at
+0x270 group 0x47, the hit manager's `FUN_140a746f0` at +0x298 group 0x4b, `FUN_140a74c80` at
+0x2c0 group 0x55, the chr frames `FUN_140a70020` group 0x52. `FUN_140a74560` calls
`FUN_140a74ce0` (when MoveMapStep +0x318 is set), which runs the BulletMan step
`FUN_140b1c050([0x143d84f70], FD4Time)`: `FUN_140b1f390` (the +0x28 list), `FUN_140b1edd0` (the
bullets: `FUN_140b13770` per live slot), `FUN_140b1f4b0` (+0x48 list), `FUN_140b1f020` unless
+0x68, `FUN_140b1eee0` (counters). So a world frame is: bullets move, hits land (melee and bullet
alike), then the chrs step, where a TAE BulletBehavior spawns a bullet.

**The slot** (0x80 x 0xd30 at [BulletMan]+0): +8 handle, +0x10 position, +0x4b4 BulletParam id,
+0xb60 metres flown, +0xc6c flags (1 live, 4 ended: 0x29 flying, 0xbd ended), the fly state
`FUN_140b0ded0` runs on at +0xcb0 (+0x24 its age = slot +0xcd4, +0x31 launched).

**What a bullet does, frame by frame** (shuriken 700000: 80 m/s, dist 5, accelOutRange -40,
gravityOutRange 5, life 0.5; spawned on frame S in Wolf's chr frame):

| Frame | 4 m | 8 m |
|---|---|---|
| S+1 | launched: the first update only sets +0x31 (no homing, speed or move), age 1/60; its attack record is issued (`FUN_1409a1290`) and tested where it stands | same |
| S+2, S+3 | 1.333 m a frame | same, to 5.333 m at S+5 |
| next | | past `dist`: 1.322 m (79.33 m/s) at S+6 |
| hit | S+4: the bullet step moves it the whole 1.333 m (4.0 m), the hit step finds the General, puts it where it touched (3.57 m out) and ends it (flags 0xbd); posture 14 | S+7 (7.97 m) |
| hit + 1 | its HitBulletID 700001 (dummy, speed 0, life 0.15) is made there in the bullet step and launched; it resolves a 0 / 0 hit; the General's reaction a000_008000 takes its first step | same |
| hit + 2 | the General's knockback starts (as a sword hit's, h + 2) | |
| hit + 10 | the dummy ends (age 10/60 > 0.15) | |
| hit + 11 | 700002 is made at the dummy and hits in that frame's hit step (posture 21); then 700003 (dummy), and 700004 at hit + 22 (posture 36); the reaction restarts the frame after each, 700004's is a000_008010 | same |

The spawn re-aims the written direction: every throw flew straight at the General's root in
plan and rose at the same 6.3 deg (0.11 m a metre) whatever pitch was written, so it touched him
1.7 m up from 4 m and 2.1 m up from 8 m. Aimed 4 m (34 deg) or 12 m (63 deg, past the Bullet's
lockShootLimitAng 45) to his side from 6 m it still flew at him and hit on S+6 (the two runs are
the same frame for frame). The first update also shifts the bullet 9 mm off the hand (not traced).

**Sv1 before / after.**
- prosthetic.rs flew a bullet in its spawn tick (`spawn_bullets` and `fly_bullets` chained after
  the step), so the shuriken hit two ticks early (S+2 from 4 m), and its hit, applied there (the
  exe's hit step of the next frame), started a reaction that stepped in the next tick: a frame
  before the exe's. Its HitBulletID child started where the whole step ended, up to 1.3 m past the
  body. Now: `fly_bullets` runs after `decide` / `clear_fresh`, before `resolve` (the bullet task
  and hit step of the tick's frame); `spawn_bullets` stays after the step; a bullet's first fly
  only sets `Projectile::launched` (age counts, hit tested where it stands); a stopping hit puts it
  where it first touched (`first_touch`) and its child starts there; a tool hit's knockback waits
  2 as a sword hit's (was 1). The dummy 700001 (no damage, isPenetrate 1) ended on touching him
  (the carrier rule for the Fistful of Ash's 3511), so the chain's next hit came 2 ticks after the
  first, not 11: the rule now skips piercing bullets.
- Side effect: a bullet's hit now lands before the tick's step, so that step's posture regen
  runs on it at once, as on a sword hit (the shuriken's 5 on c1180 read 4 after the tick);
  `tool_on` (sim_tests part07) reads the hit's own posture from its log line. Not checked against
  the exe: the regen in the hit's frame.
- The test throws ours level at our General's chest from as far before the point it first
  touches him as the exe's throw was (3.56 / 5.52 / 7.54 m): our hurt capsules and pose are not
  the snapshot's (our head capsule tops out about 2.0 m; the exe's 8 m throw touched at 2.1 m), so
  the run is placed by that distance and compares frames, not shapes. Matched: metres flown each
  frame (within 2 mm), the frame it ends, the three hit frames (4 / 15 / 26, 6 / 17 / 28,
  7 / 18 / 29), the reaction's first step (5, 7, 8).
- enemy_bullet.rs: `fly` runs after `fire` in the step's tick, which already stands for the exe's
  next bullet task (its hits on Wolf go to the next `resolve`, the exe's hit step); it now skips
  the first update too (`EnemyBullet::launched`: no follow, homing, speed or move).

Not covered (gap): enemy bullets (no enemy shoots in the snapshot scene; the rule is the exe's one
bullet update for all); where the spawn re-aims (the target point, inside FUN_140b1ad20's path;
ours aims in spawn_bullets at the lock target); the 9 mm first-update shift; the 0-damage hit of
the dummy bullets (no effect seen); the hurt shapes: the emulator's General drifted 0.48 m in his
idle while his ragdoll bodies stayed where the snapshot had him (the stop point fits that body,
0.21 - 0.27 m short of its axis), and ours are other shapes (the test matches the distance to the
touch, not the touch); amounts
(posture 14 / 21 / 36 come from the Pellet's record, not a shuriken throw's attack power); a
deflected or guarded bullet; bullets hitting the map; other bullet kinds (fans, interval
children, growing radius, homing).

## Frame order at a clip change (2026-11-04, framewin, row E2)

How: `emu-cases <exe> extracted/emu framewin [framewin_<case>...] --snapshot <dir>`
(`tools/emu-cases/src/framewin.rs`) runs `hits`' world frames (hit step, pre tasks, the General's
and Wolf's chr frames, post tasks) and logs per frame each chr's TAE updates `FUN_140b5c920`, the
ChrActionFlag words module0 +0x70 (u64) and ActionCtrl `[[chr+0x50]+0x50]` +0x1f8 at the frame's
start, after the hit step and after the chr frames, every write to Wolf's +0x70 (stage, writer),
whose records the hit step's shape test `FUN_14099b3f0` takes (record +0x2c the owner's handle)
and the attack issues `FUN_1409a1290` by stage. Cases (snapshot D:/emu-snap-e10, the General's
slash lands on frame 40): g_open, g_guard, g_release38-41 (guard let go on that frame), w_deflect
(Wolf's slash deflected on 30), g_trade10 / 14 / 18 (Wolf presses attack on that frame into the
slash). Tests in `src/sim_tests/emu_framewin.rs`.

**What the exe does.**
- The ChrActionFlags are cleared at the start of each chr frame (`FUN_140b27d40`, before the
  Havok step) and set again by `FUN_140b51b70` from that frame's TAE window; nothing touches them
  between chr frames. So the hit step of frame N reads the flags of frame N - 1's window (every
  case: start-of-frame words = the last chr frame's), and so does that frame's HKS.
- Attack records: issued when the step enters an AttackBehavior (`FUN_1409a1290` inside the chr
  frame), shape-tested by the hit steps that follow while the steps keep dispatching the event;
  the first hit step after a step that no longer dispatches it drops them (record update, no
  test).
- A hit's reaction (asked for in the hit step of frame h): frame h's step still steps the OLD clip
  on from where it was, with its TAE window (idle 3.050 -> 3.067; the guard hold a050_002000
  0.150 -> 0.167; the deflected a050_300100 0.283 -> 0.300); the reaction takes [0, dt] in frame
  h + 1. The old clip's flags (the guard's ShieldBlock 0x4, the slash's 0xa200) are what the hit
  step of h + 1 reads, and its attack records are tested there once more (w_deflect: Wolf's two
  records tested on 31, gone on 32). g_trade14: the slash's AttackBehavior ENTERS in the step of
  the frame Wolf is hit (issues on 40) and its records are tested on 41, while Wolf is already in
  a000_100202.
- A clip the HKS asks for (frame N - 1) takes its first window in frame N's step, after frame N's
  hit step: g_release39 lets go on 39, the release a050_203010 steps (0, 0.017) on 40, and the hit
  step of 40 reads the hold's window of 39 (guarded).

**What changed in Sv1.** `Actor::held` (`Held`): `play` keeps the window the last `advance`
dispatched (clip, prev_t, t, and the hits its attack windows landed) until the new clip's first
step; `advance` drops it then, or for a reaction played in resolve (`fresh`) steps the old window
on by the tick's dt (the exe's extra step). `tae_anim` / `tae_step` read it, `hit_done` /
`mark_hit` keep one window's hits apart from the new clip's. Every TAE read of the hit test in
combat.rs moved to them: the attacker's AttackBehavior windows, the defender's ShieldBlock judge,
guard SpEffects, invincibility (954 / ChrActionFlags in `dodged`), MikiriCounter, flag 41 hit
stop, 24 no damage motion, 226 knockback percent, 72 recoil, damage cuts, the additive guard ref.
Before: the clip `decide` played was read at (0, 0) by the same tick's resolve (its events at 0),
and a reaction from resolve was read at (0, 0) for two resolves, the old clip's last window lost
(a deflected swing was not tested again, a guard's ShieldBlock gone on the next hit).

Tests: `the_old_clip_keeps_its_window_on_the_frame_a_reaction_starts` (g_open, g_guard: the
windows the hit steps of h, h + 1, h + 2 read are old, old stepped on, the reaction from 0, in the
exe and in Sv1; the guard's ShieldBlock on h + 1 in both), `a_deflected_swing_is_tested_once_more`
(w_deflect: the exe tests Wolf's records on h + 1 only; Sv1's resolve after the deflect reads the
slash window with its AttackBehavior dispatched, one deflect), `a_clip_decide_plays_is_read_after_the_hit_test`
(g_release39: the hit test of the tick `decide` plays a050_203010 reads the hold's window and
blocks). All three fail with `held` ignored.

Not covered (gap): `decide` itself still reads the current clip (the reaction at (0, 0) on the
tick after a hit; the exe's HKS of frame h is still the old state's and reads the old window: in
Sv1 the reaction's state already runs in that decide, a frame early); ActionCtrl +0x1f8 is logged
(guard hold 0x42, release 0x40, reaction 0x40) but its bits are not ported; a looping old clip's
extra step past its end is not wrapped; a reaction in a hit stop (the old window stepped by 0) and
lod-skipped enemies were not run; g_trade10 / 18 were logged but not compared (Sv1's swing and
the slash do not meet on the same frames); the General's own windows when Wolf's hit replaces his
clip; enemies' frame labels as before.
## Prosthetic tools in the exe (2026-11-04, prosth, row E6)
tools/emu-cases `prosth_<tool>` on snapshot emu-snap-e10: each of the 12 tools (shuriken tap and
hold, firecracker, flame vent, Okinaga's vent, axe, mist raven, Sabimaru, umbrella, Divine
Abduction, spear, spiral spear, whistle) equipped the game's way, switched to, then used at the
General (AI on) 2.5 - 6 m ahead in whole world frames as `bullets` above. Test
`prosthetic_tools_follow_the_exe_frame_for_frame` (src/sim_tests/emu_prosth.rs; skips without
extracted/emu/prosth_*.json).
How the exe equips a tool (all read in the emulator, nothing in a menu run):
- EquipGameData is PlayerGameData +0x278. Its +0x24 + 4 x slot is the inventory index, +0x98
  (PGD +0x310) is the ChrAsm, +0x374 + 4 x slot the item id (PGD +0x5ec = slot 0's id: only a
  mirror). Slots 0 / 2 / 4 are the three prosthetic slots, 1 / 3 / 5 the right hand.
- ChrAsm +0xc is the active prosthetic slot (0 - 2), +0x24 + 4 x slot the item handle (the
  inventory entry's first word). Each frame FUN_1409cc9a0 copies the PGD ChrAsm into the chr's
  own one ([chr +0x2140]); the handle is what decides the tool.
- The menu's equip call is FUN_14079f8a0 (EquipGameData, slot, handle, inventory index, ..);
  FUN_1409ccdb0 calls it for all 25 slots. Run in the emulator it stops on a CPU exception, so
  the case writes its three fields itself (handle, index, id into slot 2).
- The switch: the arm-change input (ACTION_ARM_CHANGE_WEAPON_L, arm 10) runs the HKS
  W_AddSubWeaponChange (a000_412090); FUN_140bf1640 writes ChrAsm +0xc and FUN_1409ccdb0 copies
  the chr's ChrAsm back into the PGD. The HKS reads the left weapon at [bag +0xb8] +0x370. The
  deploy then plays a0<group>_412000; the parts model ([chr +0x2148], slot records +0x5d0 +
  0xc0 x n, requested +0x274, shown +0x270) is loaded by FUN_140e7b000 from parts:/..partsbnd and
  only completes for a tool that is already resident (the case does not wait for it).
What matches frame for frame, from the press: each tool's clips (deploy and use, first step), the
emblem payment frame and amount, and each tool bullet's first live frame:
- shuriken tap and hold: release 19, 700050 at 25, paid 24, hit 29 (a hold releases at 19 too);
- firecracker: 710050 / 710060 x3 at 25, carriers 710001 / 710011 at 35, the first sparks
  710002 / 710012 at 37, paid 2 at 24;
- flame vent and Okinaga's vent (same behaviours 107200297 at 36, 107200110 at 48): 295 at 37,
  720010 at 49, 296 at 55 (295's life 0.3 counted down), paid 3 at 48, hit 49;
- umbrella: 76405010 then 76405030 at 61, its upkeep bullet 999 at 2, paid 1 at 1;
- whistle: 790001 (life 0, never live in a slot) -> 790003 at 74 -> 790004 at 76, paid 3 at 72;
- axe paid 2 at 74, Sabimaru 1 at 32, spears 1 at 48, Divine Abduction 3 at 30, mist raven 74404040.
Rules read and ported (prosthetic.rs, enemy_bullet.rs):
- intervalCreateBulletId (FUN_140b14550, from the fly state after the move, never on the
  launch-only first update): needs the id and TimeMax non-zero; a countdown (slot +0xb78) starts at
  an interval (min + rand x (max - min)) and only runs once the age (+0xb7c) reaches
  intervalCreateWaitTime; at 0 one child and the countdown gets an interval back (overshoot kept).
  Ours made the first child at the wait itself: each firecracker carrier sparked at 0.1 s (frame
  30), which the exe never does. Now `enemy_bullet::interval_child` for both.
- Life (slot +0xb64, FUN_140b13770): set at the spawn to life + rand [0, 1) x lifeRandomRange
  (FUN_140b15200, when life >= 0), counted down by dt each update while above 0 and the bullet ends
  in the update it reaches 0. Ours compared age > life + 1e-4 and ended a 0.3 / 0.4 / 0.5 s bullet
  an update late; the random range was not read (every firecracker spark of life 0 burst at once).
Gaps:
- Random draws: the exe's stream (FUN_141157720) is not ours, so the firecracker's later sparks and
  bursts 710003 / 710013 (lifeRandomRange 0.5) are not compared; the exe's run has one of each
  spark first live a frame later (38), not explained.
- Hits where the General walks in: the exe's General is AI-on (anim 1040 walks him from 3 to
  0.8 m), ours stands still, so the axe (ours hits at 75, the exe none), Sabimaru (exe 39, ours
  33: its slash reaches him late at 0.73 m) and spears are not compared. Open lead: the spear's
  three overlapping windows (judges 107 root, 106 middle, 105 tip, f44 - 52) hit once in the exe
  (at 45, then he is 0.95 - 1.5 m away in 8010) and three times in ours (hits_done per window).
- The menu equip call itself, the parts model load of a tool not resident, the switch's own
  frames (the case waits 86 Wolf-only frames for the deploy), Wolf's SpEffects and the attack
  amounts (only "took HP or posture" is compared), mist raven's warp and Divine Abduction's grab,
  the umbrella's guard. Enemy bullets share the slot code but enemy_bullet.rs still ends on
  age > life + 1e-4 with no lifeRandomRange (no enemy bullet was recorded to check it against).
