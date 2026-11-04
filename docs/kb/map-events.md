# Map events (open-world EMEVD)

The open maps' own event scripts run through the boss-script VM (src/enemy/script.rs `step` /
`pred`) in world mode (src/enemy/world.rs, behind `SHINOBI_WORLD_ENEMIES=1`; kb/enemy.md "The
open world's map events"). tools/world_scripts.py exports each base map's instances: the map's
own events and the common_func events it starts (Initialize Event / Initialize Common Event,
2000[0] / 2000[6], all started at load), parameters filled in. `script::handled` names what the
VM runs; a condition it does not know never holds, an instruction it does not know does nothing.
Instruction names and argument layouts: DarkScript3 sekiro-common.emedf.json.

## P10a: Ashina Castle, m11_01_00_00 (2026-11-04)

(The board row says "Ashina Outskirts m11_01_00_00"; m11_01 is Ashina Castle, m11_00 the
Outskirts. This pass is m11_01, as the row's map id says.)

479 event instances, 14955 instructions: 13788 run (was 12134), 1167 not run (53 kinds).
Tests: src/enemy/world.rs `tests` (the gate 11115613, the door 20005615 both ways, the breakable
20005546, a hand-made event for the offline rules, and the gap list below with its counts).

Run since P10a (uses in m11_01):
- Conditions: 0[1] IF Parameter Comparison 434; 4[3] IF Character Type 246 (Wolf alone is type
  0, the DarkScript3 numbering; asked of Wolf only); 4[16] HP Ratio New 20 and 4[38] Health Bars
  (Other) 18 (the 4[2] / 4[37] layouts); 4[45] IF Entity Loaded 48 (loaded unless unloaded by
  2004[29] or left out by its MSB flag); 5[2] IF ObjAct Event Flag 23; 11[3] IF Hit Loaded 38
  (the block's collisions load with it).
- Control: 1000[8] END IF compiled group 15; 1000[105] GOTO IF Comparison 166; 1003[4] END IF
  Batch Event Flags 4; 1003[13] END IF Player Is Not In Own World 42 (alone he always is: only
  "is not in" 0 ends); 1003[200] / [202] GOTO / SKIP IF Player In/Outside Area 1 / 7; 1005[2]
  END IF Object Destroyed 3.
- Flags: 2003[62] / [63] Set (Batch) Network-Connected Event Flag 50 / 35 (plain flags offline).
- Characters: 2004[3] Character Warp Request 9 (a warp request, as 2004[40]).
- Objects (script.rs `ObjState`, Run::object): 2005[1] Destruction 9, [2] Restoration 1, [3]
  (De)activate 124 (kept for every object now, not only attached ones), [4] Treasure State 32,
  [6] ObjAct State 56, [7] Reproduce Object Animation 29 (a door's open pose), [13]
  Invulnerability 1, [19] Object Interaction 88; 2012[1] Activate Map Part 158 (Run::map_part_on);
  2011[1] Activate Hit 61 and 2011[4] Activate Hit and Create Navimesh 60 (Run::hit_on).

The exe (re-scout of the DSekiro decomp, 2026-11-04; behaviour only): each condition is an
`EcTest*` class, evaluate at vtable slot 1. 5[2] is EcTestObjActExecuted (0x1408fcb90): it looks
the flag up in a list of executed ObjAct records (singleton DAT_143d56058 +0x70), not in the
event flags: a trigger, held one frame here (Run::use_objact, cleared after the frame's events).
4[3] is EcTestChrType (0x1408f3300), a visitor over the chr manager; 1003[13] reads
EcTestIsMyWorld (0x14074b300), a byte of a game-state global; 4[7] / 5[10] are
EcTestChrBackReadReady / EcTestObjBackReadReady with level / edge modes.

tools/world_scripts.py now also places the object parts (MSBS part types 1 Object, 9
DummyObject) the events name, so radius checks on objects work (m11_01: 200 -> 271 places).
Re-run it after pulling (`python tools/world_scripts.py`).

Gaps (m11_01 uses, not run):
- gap: the objects' state is kept but the play world (sr_map) neither draws, animates nor collides
  by it; map parts and hits are not switched in the map either.
- gap: Wolf has no ObjAct action, so nothing calls Run::use_objact in the game: doors that wait on
  5[2] stay shut (20005615's 1113709 / 1113710).
- gap: how long an executed ObjAct record stays in the exe's list is not traced (one frame here).
- gap: 4[3]'s type number for Wolf (the class behind PlayerIns+0x2118, its slot 0x68) is not traced.
- gap: the writer of 1003[13]'s own-world byte is not traced.
- gap: in world mode warps (2004[3] / [40-42] / [61]), cutscenes (2002[4] / [13]) and bullets are
  requested but world.rs does not apply them.
- gap: common.emevd's own events (the global ones: 50, 100, ...) are not part of a map's script.
- gap: 3[24] IF Action Button: 9
- gap: 4[06] IF NPC Part HP: 12
- gap: 12[02] IF Map Alert Music State: 16
- gap: 12[05] IF Game Is Loading: 1
- gap: 13[01] IF Player Is Looking At Entity: 18
- gap: 2000[02] Set Network Sync State: 12
- gap: 2000[05] Save Request: 9
- gap: 2002[01] Play Cutscene to All: 2
- gap: 2003[04] Award Item Lot: 1
- gap: 2003[23] Set Player Respawn Point: 1
- gap: 2003[28] Award Achievement: 4
- gap: 2003[36] Award Items (Including Clients): 69
- gap: 2003[76] Disable Loading Screen Tips: 1
- gap: 2003[79] Set Wire Searchability: 6
- gap: 2003[80] Set Area Envmap: 6
- gap: 2003[81] Set Lighting + UNKNOWN: 7
- gap: 2003[82] Set Menu Fade: 22
- gap: 2003[87] Display Miniboss Health Bar: 16
- gap: 2003[88] Set Area Gparam Sub ID: 10
- gap: 2004[07] Create Bullet Owner: 1
- gap: 2004[10] Set Character Gravity: 140
- gap: 2004[13] Set Character Home: 5
- gap: 2004[15] Set Character Invincibility: 37
- gap: 2004[16] Clear Character's AI Target: 1
- gap: 2004[22] Create NPC Part: 12
- gap: 2004[26] Change Character DispMask: 40
- gap: 2004[30] Set Character HP Bar Display: 5
- gap: 2004[31] Set Character MapHit: 129
- gap: 2004[34] Set Network Update Rate: 39
- gap: 2004[35] Set Character Default Backread State: 2
- gap: 2004[37] Force Character Treasure: 5
- gap: 2004[39] Set Character Animation State: 77
- gap: 2004[49] Change Character Patrol Behavior: 27
- gap: 2004[50] Set Lock On Point: 6
- gap: 2004[58] Set Multiplayer-dependent Buffs (Non-Boss): 11
- gap: 2004[62] Set NPC Part Damaging Prosthetic: 6
- gap: 2004[64] Force Use ObjAct: 3
- gap: 2004[65] Force Character Target: 133
- gap: 2006[01] Delete Map SFX: 74
- gap: 2006[02] Spawn Map SFX: 60
- gap: 2006[03] Spawn Oneshot SFX: 1
- gap: 2006[04] Create Object-following SFX: 27
- gap: 2006[05] Delete Object-following SFX: 43
- gap: 2007[01] Display Generic Dialog: 6
- gap: 2007[13] Show Hint Box: 1
- gap: 2007[14] Remove Hint Box: 1
- gap: 2007[17] Show Tutorial Text: 2
- gap: 2007[18] Show Large Inspect Box: 3
- gap: 2008[04] Set Area CameraSetParam SubID: 8
- gap: 2009[03] Register Bonfire: 11
- gap: 2010[03] Set Map Sound State: 12
- gap: 2010[06] Enable Map Sound With Fade: 1
- gap: 2010[08] Set Map Sound State + UNKNOWN: 16

Stand-ins the VM already ran before (counted as run): 3[31] a cutscene is finished at once, 4[15]
in the draw group, 4[7] / 5[10] backread always ready, 11[2] any ground counts, 3[23] / 4[1] any
HP loss counts as the damage.
