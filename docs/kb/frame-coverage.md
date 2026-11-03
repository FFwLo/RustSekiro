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
  found yet), or the General's dummy polys stay at his snapshot pose.

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

### Gaps of the mid-action list

- No hit lands (above): getting hit, block and deflect reactions, posture break, death not traced.
- Grapple: no grapple point in reach from the snapshot spot (no wire anim; 2 functions new).
- Prosthetic: no bullet issued within 70 frames; the equipped tool and its ammo are as saved.
- One input timing per action; combos past the second slash, air attacks, crouch, the other arts
  and tools are not run. The General runs only behaviour 3000.
- Names of the new branches come from a read of their first lines (strings, RTTI, fields);
  "(?)" ones are guesses.
