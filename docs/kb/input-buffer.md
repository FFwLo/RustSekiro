# input-buffer: Input buffer, accept/execute flags, FireEvent resets

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06 GHIDRA (NEXT 11, partial): ActionRequest mask writers found. Accept (+0xd0) setter FUN_140b2be00(mod, bit, on),
  execute (+0xe0) setter FUN_140b2bdc0. Both are rebuilt every frame (zeroed at the end of FUN_140b2c190). Main writer:
  FUN_140b51b70 = a per-event switch on a u16 id (looks like the TAE ChrActionFlag handler, gated by an SpEffect check
  on arg[7]): case 1 -> accept bits 0,1,25,26 (attack); case 4 -> execute same bits; 0x10 -> execute 2 (guard);
  0x16 -> 22; 0x1a -> 5,13,14; 0x1d -> 16,17; 0x1f -> 7; 0x20 -> 6,9,10,27,12,15,19. Accept-only helpers:
  FUN_140b5b9a0 (attack), FUN_140b5b8e0 (2, +3 unless state 0x3fc0), FUN_140b5b7e0 (6,9,10,27,19), FUN_140b51970
  (a 7-byte bool struct, maybe HKS). NEXT: confirm the u16 is the TAE flag id (our player gates on 115/117/26/119,
  not 1/4), then accept window = accept-flag ranges, execute = cancel flags. Tool: DecompileRefs range:/orq: options.
- 2026-10-06: NEXT 6b: input buffer from the exe. SprjChrActionRequestModule (RTTI vtable 0x142a729d8), update FUN_140b2c190:
  33 action bits (HKS ACTION_ARM_*); pressed = held & ~prevHeld; pending |= pressed & acceptMask(+0xd0);
  requested(+0x30, env 1106 via FUN_140b2b6d0) |= pending & executeMask(+0xe0); masks are rebuilt every frame;
  pending has NO timer (cleared only by an engine flag / on use). Per-action hold timers at +0xe8 (env 1108).
  Implemented: buffer = 0 (no expiry); pending cleared when combat puts the player in a damage state.
  Gap: what sets the accept mask (+0xd0) and the clear flag (+0x188 bit 0) - writer not found yet.
- 2026-10-07 WOLF/GHIDRA: NEXT 11 done (found with the full decompile + gamedb in minutes). FUN_140b59170 is the TAE
  event dispatcher (case 0 ChrActionFlag -> FUN_140b51b70, 0xe0 SetTurnSpeed, 0x140 event 320 bool accept set).
  ChrActionFlag -> ActionRequest bits (HKS ACTION_ARM_*: 0 attack, 1 sub, 2 guard, 4 jump, 5/13/14 step, 7 item,
  18 shinobi tool). ACCEPT (+0xd0, press latched): 87 all, 1 attack, 9/150 guard, 25 step, 151 jump, 30 item,
  136 tool. EXECUTE (+0xe0, request fires): 4/115 attack, 116 sub, 16/117 guard, 26 step, 119 jump, 31 item, 137 tool.
  The flags we already gated on were the execute ones. A flag with StateInfo != 0 applies only with that SpEffect
  state (none in Wolf's accept/execute flags). Reset: HKS FireEvent -> ResetRequest -> act(9101) -> FUN_140b2afd0
  (+0x188 bit 0) drops every pending press; FireEventNoReset (locomotion, quick turns, falls, landings) keeps them.
  Implemented in player.rs (buffers / keeps_buffer). Effect: e.g. the tap slash buffers from frame 6 (executes 15),
  Combo1 hold from 21 (executes 33), guard/jump/step get early cancel windows (frames 3-15). Mash before the window
  is dropped. Test early_mash_is_dropped_but_a_press_in_the_accept_window_chains. 37 tests pass.
- 2026-10-08 LIVE RECORDING (memscope, rec_20261008_041541, 60 Hz; tools/rec_timeline.py, rec_windows.py):
  - Inputs: PadManipulator (ChrIns +0x58) byte 0xF4 bit 0 = attack held, 0xF5 bit 0 = guard held.
  - Current anim: TimeAct module +0x100 = group * 1e6 + number (50300000 = a050_300000).
  - Buffered inputs fire about 1 TAE frame (2 game frames) after our flag start in every matched case
    (attack 115: 16.0 vs 15, 9.9 vs 9, 13 vs 12, 19 vs 18; guard 117: ~7.0 vs 6). Fresh press -> anim in ~1 game
    frame. Modelled (2026-10-08): accepts() reads the execute flags at the previous step's time (the HKS
    sees the flags the previous frame's TAE update set), one game frame after the flag start.
  - Release slash: let go before ref 208 -> Release starts at TAE f6.0-6.5 (= our 208 window at f6). Matches.
  - GUARD needs the button held when the buffered press executes: taps let go before the window opened
    were dropped (GroundStep_N, HardDeflectedL, Combo1Release, Combo3: the switch came only after a new
    press). Attack presses survive being let go. Exceptions that keep released guard taps: the spam chain
    (ref 212, StandToDeflectGuard*) and the additive deflect (ref 228) + guard cancel (412 at f9, e.g.
    StandDeflectHardSmall: press f1.5 let go f4 -> StandToDeflectGuard at f9.5). Implemented in player.rs
    (pressed_guard needs guard held or a same-frame press); test
    guard_tapped_before_the_window_is_dropped_but_held_guard_fires.
  - (Closed 2026-10-08) LandAirComboAttack1 -> guard at "f5": the land anim starts at the air slash's
    time (HKS StartTime_00 = env(3063)), and the recorder counted from the land anim's appearance, so f5
    is f5 + that offset, past flag 117 at f12. No separate landing rule. a000_7900x0 anims appear between actions (not exported; layer?).

## HKS env/act ids cross-checked (2026-10-08)

Community names (github.com/iitsigor/SekiroHKS, from Vawser's / Meowmaritus' dumps) agree with every
id the sim relies on: env 1106 ActionRequest, 1108 ActionDuration, 1118 IsLockedOn, 2004
IsAtkContactTarget (kick-jump), 3036 GetBehaviorRefID (SP_EF_REF checks), 3063 GetVariableChangeValue,
273 GetThrowAnimID, 276 IsThrowSelfDeath (ThrowDef -> ThrowDefDeath), 339 IsAnimEnd, 345
GetEquipWeaponSpecialCategoryNumber (art type); act 9101 ResetInputQueue, 156 SetAutoCaptureTarget,
136 SetThrowState. Our 160 paramdefs match soulsmods/Paramdex SDT/Defs field for field.
