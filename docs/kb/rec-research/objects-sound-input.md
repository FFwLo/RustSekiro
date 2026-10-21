# Recording hook research: map objects, FMOD params, raw stick, enemy hearing (exe 1.06, base 0x140000000)
Static read, 2026-10-21 (decomp via tools/fn.sh, prologues from extracted/sekiro_dearxan.exe, file offset = RVA). Nothing run live.
Reference repos have no object-manager layout (checked SekiroTool, practice tool, Smithbox Sekiro_1.06.txt). All offsets below are from the decomp.

## A. Map objects
- Object manager WorldObjManImp = [0x143D5D600] (not in Smithbox; ~200 uses; ctor FUN_140896d40 sets the layout, RTTI 0x1429EC048). High.
  mgr+0x28 int block count, mgr+0x30 ptr to block pool, stride 0x40410 (FUN_140899380 mgr,handle -> ObjIns*). mgr+0x18/+0x20 = area index (0x20 each).
- Block (one per map area): +0xE8 u32[8192] live slot list, +0x80E8 free list, +0x100E8 ObjIns*[8192], +0x200E8 u32 live count, +0x200EC free count,
  +0x40134 int N and +0x40138 ptr to N sorted pairs {u32 entityId, u32 handle} (FUN_1408919e0 binary search), +0x40158 list of breakables.
  Walk: for b<[mgr+0x28]: for i<[blk+0x200E8]: obj = [blk+0x100E8 + 8*[blk+0xE8+4*i]]. For entity ids walk the +0x40138 pairs, resolve the handle with FUN_140899380. FUN_140890ee0 (blk,idx) does the same lookup. High for layout, medium for count = live count.
- Entity id to ObjIns: FUN_1406d5cc0(ecx entityId) = FUN_14089a120 (id to handle) + FUN_140899380 (handle to obj). Bytes 48 83 ec 28 8b d1 48 8b 0d. High.
- ObjIns (ctor FUN_140872b40, vtable 0x1429E8278): +8 handle u32; +0x10 state/draw node (+0x14 dirty bits); +0x18 data wrapper whose [+0] points to the ObjectParam row (first s16 = HP, -1 = unbreakable);
  +0x20 ObjRes; +0x124 s16 HP now; +0x126 bit5 breakable-and-alive, bit6 locked; +0x127 bit2 and +0x128 bits0-1 set when broken (FUN_14087a090); +0x128 bit1 active flag
  (FUN_140878b30); +0x138 s32 current anim/state id (FUN_1408777f0 = is state id X); +0x1F0 -> physics body (FUN_140e89ce0 copies a 4x4 from [[+0x1F0]+0x20]+0x70).
  Position: float x/y/z at +0x1CC / +0x1DC / +0x1EC (3x4 row-major matrix at +0x1C0; the break code uses these as effect position). Medium-high.
  Model name (oNNNNNN): NOT found. The destroyed test the game uses = FUN_140877840 (bit6 or param flag).
- DESTROYED hook: FUN_14087A090(rcx ObjIns) bool; runs once when HP <= 0 (checks s16 +0x124 > 0 first, then sets the broken bits, clears bit5). Only caller FUN_140893ca0 (per-block update). Bytes 40 53 48 83 ec 50 48 8b 05 (6 clean bytes). Medium-high.
  Destroy REQUEST (EMEVD or hit): FUN_14087D410(rcx ObjIns, rdx vec4* pos, r8d level (>0 forces HP 0), r9b, ...) bytes 40 53 55 56 57 48 83 ec 48 48 8b 05; callers 140877ec0 / 140877f60 / 14087cfb0 / 140878710.
  EMEVD route: bank 2005 handler FUN_14090A090 (case 1 Request Object Destruction -> FUN_1406d65c0(ecx entity, dl flag) -> FUN_140878710). The existing emevd hook (FUN_140900d10) already logs bank 2005 args, so script destruction and ObjAct commands (2005[5] init, [6] set state, [14] activate) arrive with entity ids.
- OBJACT: manager SprjWorldObjActMan = [0x143D55EC8] (Smithbox agrees): +0x28 int area count, +0x30 ptr, stride 0xE0 containers; container +0x78 ptr to entries (stride 0x130), +0x80 count, +0x20 area id.
  Entry: +8 u32 id, +0x50 object handle, +0x54 s32 state (4 after exec), +0x6C last state id, +0x70..0x72 flags, +0x88 kind object (vfunc +0x30 = type byte).
  START hook: FUN_1406BC1B0(rcx container, rdx ExecInfo*, r8b flag) = execute an ObjAct (plays the object state, sets entry +0x54, sends the net message). ExecInfo (built by FUN_1406BA7B0 / FUN_1406BCBC0): +0 u32 area id, +4 s32 entry index, +8 -1, +0xC u32 caller param, +0x10/+0x14, +0x18 u32 state id given to the ObjIns, +0x1C. Entry = [container+0x78] + 0x130*[rdx+4]. Bytes 40 55 56 57 41 54 41 55 41 56 41 57 48 8d. Wrapper FUN_1406BDCE0(mgr, area, index, p3) (called from 1406ae300). Medium-high; the meaning of +0xC/+0x10/+0x14 is inferred.

## B. FMOD (this exe uses FMOD Ex 4.4: fmodex64.dll + fmod_event64.dll, NOT FMOD Studio)
- IAT (QWORD slots): EventParameter::setValue 0x142929D70, getValue 0x142929D50, EventParameter::getInfo 0x142929D38, Event::getParameter(name) 0x142929E18,
  getParameterByIndex 0x142929DD8, Event::start 0x142929E38, stop 0x142929EB0, setVolume 0x142929DF0, setPitch 0x142929DE8, set3DAttributes 0x142929D78, EventSystem::getEvent 0x142929DA0, update 0x142929E60.
- Exe wrappers on a cue (DLMO::MOFmodCue, vtable 0x142E4A610, event ptr at cue+0x2D8): FUN_141CA05B0(rcx cue, rdx char* name, xmm2 float) bool [vtable slot 0x142E4A6B8];
  FUN_141CA0600(rcx cue, dx u16 index, xmm2 float) [slot 0x142E4A6B0]. Bytes of both: 48 83 ec 38 48 8b 89 d8 02 00 00 0f 29 74. Hook these = every per-event parameter write (Shahei occlusion etc.).
  Group form: FUN_141C8EAD0(rcx obj, rdx name, xmm2) -> all cues (FUN_141CA3270 -> FUN_141CABD30). High (read from code).
- GLOBAL (mix / reverb / deathblow) parameters: FUN_1409312B0(rcx SoundMan [0x143D6CE08], rdx char* name, value float, probably xmm2) bytes 40 53 48 83 ec 30 0f b6 05. Called per frame from FUN_14092FC30 for sd_reverb_size and the room slot
  (eased 2.6 s) and from FUN_140931320 for the three deathblow slots. It queues to the sound thread: FUN_141C7B280 (by slot, command 0x02010010: word +8 slot, float +0xC) or FUN_141C7B610 (by name, command 0x14010018: float +8, name ptr +0x10), both via FUN_141C92B80(rcx queue = FUN_141C830F0()+200, rdx msg).
- BEST SINGLE CHOKE POINT for all sound commands (play, stop, volume cmd 6/10, params): FUN_141C92B80 bytes 48 85 d2 75 03 32 c0 c3 81 3a 53 4d 4f 4d (msg dword0 SMOM = 0x4D4F4D53, dword1 = command code).
- No battle-intensity or area FMOD parameter exists in the exe. Music is chosen by EMEVD event 410 / bank 2010 (docs/kb/sound.md:195-225); the existing music hook FUN_1408cebe0 sees it. Event parameters present in the banks: (distance) and Shahei only (sound.md:126).
- Open: float register of FUN_1409312b0 (xmm2 assumed from the call site), the full command-code table of FUN_141C92B80.

## C. Raw stick (pad)
- Pad manager FD4PadManager = [0x143F42850] (Smithbox agrees). Pad object P = FUN_14066AF40(rcx PM, edx 0) (bytes 44 8b c2 4c 8b c9 83 fa 04 72 03 33 c0 c3; std::map at [PM+0x38+8*slot], key 0x143D55502, node value at +0x28). Move code calls it every frame (FUN_1407f0a70, 1407e3690).
  Easiest live: call it from the DLL, or hook FUN_1409603D0 (rcx = P, bytes 48 8b 81 f8 38 00 00 f3 0f 10 00 c3) which every move and camera path calls.
- P+0x38F8 = pointer to the current input frame S (ring buffer at P+0xB4, 61 frames x 0xEC, write ptr P+0x38F0, read ptr +0x38F8 = write - delay*0xEC, delay at P+0x3900). S+0 float, S+4 float = left stick,
  S+8/+0xC = look stick (x 0.0349 rad), S+0x10/+0x14 = mouse, S+0x30.. digital bytes, S+0x67+idx = virtual buttons (FUN_14095fd80). High.
- Axis meaning (FUN_1407f0f50 builds vector (-S[4], 0, -S[0])): S+0 = forward/back, S+4 = left/right. input-hks.md calls them x/y; swapped, inferred from the same code. Medium.
- Writer (pre-deadzone virtual input): FUN_140960650(rcx P) (bytes 48 89 5c 24 10 48 89 6c 24 18 48 89 74 24) fills S each frame: S+0 = FUN_140966AC0, S+4 = FUN_140966BC0 = clamp(vecA + vecB, -1, 1) * (virtual button 0x4D held ? 0.5 : 1.0),
  vecA/vecB = virtual analog 0x4F and 0x51. No deadzone here, and walk = button 0x4D halves the stick (the walk modifier). Inject by writing S+0/+4 in a post-hook on FUN_140960650. High.
- Move side: FUN_1407f0f50 (bytes 48 8b c4 48 89 58 10 55 56 57 48 8d 68 b8) reads S, treats |axis| > 0.1 as activity, rotates by the camera matrix (param_3+0x130..), writes manip +0x10/+0xC0. It halves again only when debug flag DAT_143B066E4 is set. No pad-level run/walk length threshold found; MoveSpeedLevel comes from locomotion FUN_140B4AFD0 (reads manip +0x10, see input-hks.md).
- The Wolf PadManipulator does not hold P; no ChrIns pointer path exists, use the pad manager.

## D. Enemy hearing (quick)
- A sound slot is a 0x120-byte node in the targeting list (vfunc +0x10 == 3 means sound, node+8 short == 0 sub id). FUN_14062A2A0 adds or refreshes it: rcx TS sub-object, rdx SprjSoundTarget*, r8d float weight (FUN_1410C6FF0 = [[chr]+8]+0x44), r9 owner chr.
  Target state at node+0x20: +8 s32 type (3), +0xC float weight (kept if already > 0), +0x20 byte valid; the target copy at node+0x20+0x50: +8 u16 source id, +0x10 vec4 position (from target vfunc +0x70), +0x24 u32 sound kind, +0x28 u32 value (strength or radius, unconfirmed). Built by FUN_14061D320 + FUN_140613A90. Medium.
- SprjSoundTarget (ctor FUN_1406128A0): +8 u16, +0x10 s32, +0x1C float 1.0, +0x20 byte, +0x24 u32, +0x28 qword.
- Per-enemy gather: FUN_140624B70 subscribes listener kinds 0,1,2 and table 0x142964408 {3,4,5} via FUN_14064C540(WorldAiMan [0x143D55070], kind, listener), then pulls matching sounds. No meter: a sound lands in a slot or not (state reason 4 = sound).
- EMIT side (hook this to record every noise): FUN_14064A5F0(rcx WorldAiMan, rdx source info, r8 emit-info 48 B built by emitter vfunc +0x10, r9, stack) bytes 48 89 74 24 20 41 54 41 56 41 57 48; reached from FUN_14060F050 <- FUN_140BD0340 <- chr step code (1409e66e0 etc.). Emit-info layout, kinds and radii: open.

## Open
Model name of ObjIns; ObjAct ExecInfo +0xC..+0x14 meanings; float register of FUN_1409312b0; sound emit-info layout and kind table; SoundTarget +0x28 meaning.
