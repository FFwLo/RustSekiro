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
- (closed by P10b) the play world drawing and colliding by the objects, map parts and hits;
  Wolf's ObjAct action.
- gap: how long an executed ObjAct record stays in the exe's list is not traced (one frame here;
  P10b's re-scout found no removal, so it may stay).
- gap: 4[3]'s type number for Wolf (the class behind PlayerIns+0x2118, its slot 0x68) is not traced.
- gap: the writer of 1003[13]'s own-world byte is not traced.
- gap: in world mode cutscenes (2002[4] / [13]), bullets and Wolf's own warps are requested but
  world.rs does not apply them (other characters' warps: P10b).
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

## P10b: the event state in play, all six play blocks (2026-11-04)

The play world (sr_map: m11_00, m11_01, m11_02, m13, m17, m20) now follows the events' state
(src/enemy/world.rs `apply_state`, after run_world each fixed step):
- Drawn: map parts 2012[1] off are hidden; objects destroyed (2005[1]), deactivated (2005[3]
  off) or open are hidden. Through sekiro-rs's new `map::visibility::EventVisibility` (entity id
  or entity group per block, `sr_map::map_of`), which write_visibility reads after the taken
  items and before the arena's, Job C's and the story's opinion; parts the events never named
  keep the story's state.
- Collided: `sr_map::install` gives the collision world the switchable set
  (world.rs `switchable`: 2011[1] / [4] targets, objects named by 2005[1] / [2] / [3] / [7] and
  every ObjAct's object, over the six scripts: 82 collision ids, 233 object ids, 670 parts and
  objects with 149949 triangles once entity groups are expanded). `map_world::
  collision_world_switched` takes them out of the static mesh and adds them as event parts
  (`CollisionWorld::append_event_part`, crates/sim unchanged), on as the story starts them;
  `set_event_part` follows 2011[1] / [4] and the objects' shown state. Collision parts that are
  swim hits or event walls are not switchable (kept as before).
- Open is **hidden + collision off**: the cache's object GLBs carry no animation (1039 checked,
  0 with one; the objbnd anibnds are not converted), so a door is not posed. An object is open
  when its last 2005[7] Reproduce Object Animation is one of its ObjActs' objAnimId (the HUB
  door 1111700: 999940 anim 1 / 999941 anim 2; the gate 1111603: 999900 anim 1), or when Wolf
  works one of its ObjActs (the ObjAct plays objAnimId on the object). Other 2005[7] anims are
  kept but change nothing.

Wolf's ObjAct action (world.rs `objact_interact`, last of idol.rs world_hooks' interactions):
the action button is Interact (E, Action::Interact; the same press that kneels at an idol). On
the ground and free, the nearest ObjAct on offer whose reach holds is worked: Run::use_objact
with its ObjActEntityID (what 5[2] names: 20005615 waits on 1113709 / 1113710, the HUB door's
two ObjActs) and the object's open animation; it holds Wolf for that step. Data:
tools/world_scripts.py `objacts` exports each map's MSB ObjAct events (type 7: ObjActEntityID,
part, ObjActParam row, EventFlagID) with the row's validDist / validPlayerAngle /
validObjAngle / objAnimId and the object FLVER's dummy objDummyId in MSB space (the HUB door's
121 / 120 sit 0.6 m out on either side, facing into the door). Counts: m11_00 7, m11_01 24,
m11_02 5, m13 1, m17 1, m20 11. Offered: object not destroyed or deactivated, its row not
switched off for it by 2005[6], reach above 0. Reach (re-scout, FUN_1406b0c10): distance from the
dummy at most validDist x 0.01 x 2 m (1.4 m for 70), Wolf facing along the dummy's forward within
validPlayerAngle, standing behind it within validObjAngle, 1.5 m up or down. The exe's record key
is the ObjAct instance's "KickEventID" (+0xc), pushed once per execution (FUN_1406b2af0, from the
execute handler FUN_1406bc1b0 at once or after the animation by eventKickTiming); the data says
it is the ObjActEntityID.

Warps (2004[3] Character Warp Request, and 2004[40-42] / [61], all Act::Warp): a character's
warp whose target is in a play block's script (world.rs `warp_target`: a character where it
stands for type 2, else the place or character of any block, the requester's first) is carried
out: on the field it is moved there (a place: its floor point + the capsule half height),
otherwise it comes in there (its spot moves with it). m11_01's nine 2004[3] all send 1110701 to
points 11127xx. Wolf's own warps are not carried out (20005601 / 20005605's idol and fall
returns, 11105880's cutscene spots: their conditions are approximated), logged with
SHINOBI_SCRIPT_TRACE.

All six blocks' events run from the start (world.rs `load` already ran every play block that has
a script; P10b checks them). Per map (test
`the_play_worlds_six_blocks_run_with_their_listed_gap_counts`):

| map | events | instructions | not run | kinds not run |
|---|---|---|---|---|
| m11_00 Outskirts | 393 | 10064 | 901 | 72 |
| m11_01 Castle | 479 | 14955 | 1167 | 53 |
| m11_02 Castle (reservoir) | 234 | 6087 | 717 | 65 |
| m13 Abandoned Dungeon | 100 | 3227 | 164 | 29 |
| m17 Sunken Valley | 324 | 9525 | 849 | 50 |
| m20 Senpou Temple | 388 | 10142 | 810 | 65 |

Kinds not run that m11_01's list above does not have:
- m11_00: 3[4], 4[34-36], 4[44], 5[0], 5[3], 1000[6], 2000[3], 2002[3], 2003[3], 2003[75],
  2003[83], 2003[90], 2003[93], 2004[14], 2004[24], 2004[43], 2004[47], 2004[63], 2004[67],
  2007[10], 2007[11], 2007[16], 2010[2], 2011[5], 2012[8].
- m11_02: 3[4], 3[26], 3[34], 4[36], 1000[0], 1003[8], 2002[3], 2003[83], 2004[43], 2004[55],
  2007[10], 2007[11], 2007[16], 2011[2], 2011[3], 2011[5], 2012[8].
- m13: 3[4], 12[4], 2004[24].
- m17: 3[4], 4[34], 1000[6], 2003[64], 2003[65], 2003[75], 2004[23], 2004[24], 2004[43],
  2007[2], 2010[2], 2011[5].
- m20: 3[4], 5[12], 1000[0], 1000[6], 1003[14], 2002[3], 2002[7], 2003[3], 2003[6], 2003[54],
  2003[75], 2003[83], 2004[18], 2004[23], 2004[24], 2004[27], 2004[63], 2007[2], 2008[2],
  2010[2].
(Some of m11_01's kinds are absent elsewhere; the test prints each map's full set.)

Tests: src/enemy/world.rs `wolf_opens_the_castle_hub_door_with_the_action_button`,
`opened_gates_broken_objects_and_switched_parts_reach_the_play_world`,
`a_warp_request_finds_its_target_in_the_play_world`,
`the_play_worlds_six_blocks_run_with_their_listed_gap_counts`,
`the_play_world_collision_takes_the_events_switchable_parts`; crates/game map_world.rs
`a_switched_door_blocks_until_the_events_open_it`.

Unverified (P10b):
- Not seen in the game window: no run of the play world was made; the state is checked by the
  VM tests and the collision ray test only.
- Which ObjAct angle is which (player facing vs side) is read from the HUB door's dummies, not
  traced; the 1.5 m height window and the dummy's parent bone (taken as the root) are assumed.
- 5[2]'s key being the ObjActEntityID rests on the data (20005615); the re-scout read the exe's
  key as the instance's KickEventID without finding where it is filled.
- The executed record lasting one frame (the exe seems to keep it); the EventFlagID is not set
  on execution (none found); whether a set EventFlagID stops the offer (not found).
- ObjActs on by default before 2005[6] (the 2005[6] handler is not traced).
- Hiding an open door instead of posing it; a deactivated object being hidden as well as
  non-colliding (as the story treats DeactivateObject).
- Wolf's playerAnimId (the door push) is not played; the exe's eventKickTiming wait is not
  modelled.
- Reach-0 ObjActs (the revolving walls 90071-90076: spQualified path) are never offered.
