# Behaviour graph and HKS survey (2026-11-03)

How much of Wolf's animation and action logic Sv1 takes from the game's own files, and how much is
hand-ported. Goal: run the game's graph (`c0000.hkx`) and scripts (`hks_src/c0000*.lua`) as data,
with the runtime rules taken from byte-matched exe code in DSekiro.

## Graph (`extracted/chr/c0000.behbnd.d/c0000.hkx`, `sekiro-extract hkxdump`)

| Node type | Count | In Sv1 |
|---|---|---|
| hkbClipGenerator | 2,566 | loop flag only (`loop_clips_c0000.json`); playback speed all 1 |
| CustomManualSelectorGenerator | 2,041 | clip reuse only (`blend_src.json`) |
| hkbStateMachine (StateInfo 1,360, TransitionInfo 1,898) | 48 | hand-ported in `player.rs` |
| hkbManualSelectorGenerator | 362 | no |
| hkbVariableBindingSet (861 bindings) | 839 | no |
| CustomDockingGenerator | 105 | hand-ported (`docking.rs`) |
| hkbLayerGenerator (54 layers, 32 bone weight sets) | 22 | guard walk only, by hand |
| CustomLookAtTwistModifier / hkbTwistModifier | 20 / 19 | twist by hand (`anim.rs`) |
| CustomTransitionEffect / hkbBlendingTransitionEffect | 10 / 3 | durations by hand; most crossfades are TAE Blend events |
| hkbBlenderGenerator | 2 | no (locomotion blend by hand) |
| Foot IK / hand IK modifiers | 1 / 2+1 | no (feet slide) |
| hkbScriptGenerator, handle modifiers, others | 4, 6 | no |

At runtime Sv1 reads two facts from the graph; the rest is hand-ported or missing.

## HKS (`extracted/hks_src/c0000*.lua`, 30,038 lines)

4,098 script functions. The engine surface they call is small:
`env` 1,605 calls (81 ids), `act` 683 calls (45 ids), `hkbGetVariable` 195, `hkbFireEvent` 2,
bone access (`hkbGet/SetBoneModelSpace`, `hkbIsBoneValid`, `hkbIsNodeActive`) 6, plus Lua builtins.
The exe's `act` dispatcher is FUN_140b32410 and the `env` dispatcher FUN_140b36410; both and their
per-id handlers are on DSekiro's combat lists (rows C4-C6). Sv1 has no Lua runtime: the scripts'
input, cancel and state rules are hand-ported into `player.rs`.

## Plan

1. DSekiro matches the dispatchers and handlers (rows C4-C6) and Havok's behaviour runtime
   (`data/targets/behavior.txt`, 915 functions, 760 open; rows H1b-H2b).
2. Sv1 runs the HKS files with a Lua runtime, `env` / `act` answered from the matched handlers.
3. Sv1 runs `c0000.hkx` with a small graph player (state machines, transitions, selectors,
   layers, blenders, IK) whose rules come from the matched Havok code; the sim tests keep it honest
   while it replaces the hand-ported parts one by one.

Files are read from the player's own install at runtime; none are shipped.
