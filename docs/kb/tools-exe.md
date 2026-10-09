# tools-exe: Extractor, Ghidra/decompile, gamedb, RTTI, exe unpacking

Dated findings, oldest first. Append new entries at the end.

- 2026-10-06: extractor, HKS decompile, statemap, export, Bevy game (player FSM, enemy, combat, HUD) built.
- 2026-10-06: tools/sekiro-dearxan (uses tremwil/dearxan, cloned to tools/dearxan): SteamStub 3.1 unwrap works
  (app 814380, .text AES-decrypted, OEP 0x14235a28c restored) -> extracted/sekiro_dearxan.exe (memory layout,
  loads as a normal PE). dearxan found 0 Arxan stubs: Sekiro has no `test rsp,0Fh` signature, so its Arxan
  variant is not one dearxan supports (tested games: DSR, DS2, DS3, ER, AC6, NR). Further Arxan analysis was
  blocked by the session's permission policy; waiting on the user before continuing.
- 2026-10-06: user supplied a Steamless-unpacked exe (Sekiro\sekiro.exe.unpacked.exe). All sections byte-identical
  to tools/sekiro-dearxan's SteamStub unwrap (confirms it). Ghidra project: extracted/ghidra (sekiro), decompiles
  to extracted/decomp/. Exe debug labels found: posture regen debug menu ("current stamina control type: none"
  -> default = 100%, now applied), GuardCut formula label at 0x142a2cb20, player damage = attackBasePhysics x
  atkPhysCorrection (applied; player_hp_damage gap removed).
- 2026-10-06: research: Sekiro has no Arxan (tremwil / me3 blog), so no deobfuscation is needed; Steamless + Ghidra is the
  route. Helpers: RTTI class names in the exe, fromsoftware-rs crates/sekiro, sekiro-coop SDK (AOBs/offsets), LukeYui's
  Sekiro-Debug-Patch (dev debug menu with live posture/stamina readouts, for verifying the prototype against the game).
- 2026-10-06: player death plays GroundDeathStart_F (100 f) -> GroundDeathLoop_F, then revives (prototype loop).
- 2026-10-07 NEXT 19 done: tools/decomp_join_signatures.py joins Ghidra signatures split over two lines (return type
  alone, name below; 11545 of them) in extracted/decomp. gamedb now indexes 182,628 of 185,039 functions (was 178,981)
  and 500k call edges (was 459k). Run it after any new decompile_all.ps1 export, then `gamedb index`.
- 2026-10-07 NEXT 20 done (light version, no Ghidra write): tools/rtti_vtables.py scans the exe's MSVC RTTI (12662 type
  descriptors -> 11055 complete object locators -> 11055 vtables) into extracted/rtti_vtables.txt and annotates the
  decompile (`&PTR_FUN_142a73540 /* CSChrAutoHomingModule */`, 45212 references). rg the class name to find a
  module's constructor, then its methods next to it. Re-run after decompile_all + decomp_join_signatures.
- 2026-10-07 10:09 scheduled run: checked in, no action (last log write 08:13 < 3 h ago; 5h window at 42 %).
- 2026-10-07 15:09 scheduled run: checked in, no action (last log write 14:46 < 3 h ago; another session likely active).

## Community offsets cross-check: SekiroTool (2026-10-08)

github.com/borgCode/SekiroTool (read as source, not run; reference/SekiroTool). Its patch 1.6.0 table
matches the user's exe (WorldChrMan +0x3D7A1E0) and every offset we found ourselves: PlayerIns +0x88,
ChrIns handle +0x8 / id +0x68 / SpEffect +0x11D0 / modules +0x1FF8, Data HP 0x130 / posture 0x148,
Physics yaw +0x74 / position +0x80. New for live work (1.6.0, base 0x140000000):
- AiThink = [[ChrIns+0x58]+0x340]: +0xB742 LastAct (battle-script act), +0xB744 LastKengekiAct,
  +0xB741 / +0xB743 force bytes. Recorder block 22 "ai_think" (read only).
- WorldAiMan 0x143D55070 (+0x4C6EC global force act), DamageManager 0x143D77EF0 (+0x30 hitbox view),
  WorldChrManDbg 0x143D7A388 (+0x6F debug draw), DebugFlags 0x143D7A366 (+0x10 DisableAi, +0xE
  AllNoAttack, +0xC AllNoDamage, +0x17 AllNoPosture), Behavior module +0xD00 anim speed.
  These are writes (toggles) - only with the user's go-ahead, and never hooks.
- Smithbox Documentation/Memory/Sekiro_1.06.txt: singleton addresses for 1.06 (= 1.6.0), e.g.
  SprjSound 0x143D9A7B8 (the singleton the hit-SE code FUN_140b63fe0 calls), SprjWorldAiManager
  0x143D55070, CSAttachDummyChrHitSystem 0x143D87A58, SprjHkAiManager 0x143D91848. Smithbox's SDT
  alias JSONs are empty and its TAE template names nothing beyond ours (708 / 930-946 unnamed there too).
- SekiroToolFork (gonlad-x, fork of borgCode/SekiroTool): same Offsets.cs; adds a target overlay
  (UI only). Its poise read (ChrSuperArmor module +0x28 current / +0x2C max / +0x34 timer)
  confirmed on rec_20261008_054259: the General's poise and max poise are 0 for all 18480 ticks -
  no super-armour meter, as NpcParam superArmorDurability 0 says and combat.rs assumes. (Its
  basic-slash poise damage table 40 / 38 / ... / 24 per NG cycle is community-measured, not data.)
