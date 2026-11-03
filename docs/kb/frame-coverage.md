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
| `FUN_140b44980` → `FUN_140b5c730` | Behaviour module after Havok: TAE event dispatch over the frame's time window, root motion | 127 / 32 KB / 5 | TAE events ported by id; this dispatcher's window rules not checked |
| `FUN_140a03340`, `FUN_140a02a10` ChrIns::vf44, `FUN_140a36cc0` PlayerIns::vf44 | SpEffect list tick and the state-info reactions | 74 / 12 KB / 5; 218 / 37 KB / 21; 253 / 44 KB / 22 | partly (kb/guard-deflect.md, skills.md) |
| `FUN_140be1b50` | Throw module: grab and deathblow state machine (?) | 11 / 9 KB / 0 | deathblow from ThrowParam; this tick not |
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

- One frame, one moment: standing in the arena, no input, no attack, no hit. Functions that run only
  during an action (attack, guard, deflect, jump, grapple, death) are not in it. Trace frames from
  the system agents' scenarios to cover them.
- Called through tail jumps: 38 of the 185 functions the pre-update task entered have no caller edge
  (the tree shows 147).
- The module bag offsets were matched to their classes by what they do, not from the constructor.
