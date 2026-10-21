# Recording hook research: input injection, ActionRequest, HKS state callbacks (exe 1.06, base 0x140000000)

Static read of extracted/decomp (Ghidra) plus vtable dumps from extracted/sekiro_dearxan.exe (file offset = RVA).
Nothing here was traced live. 2026-10-21. Decomp files: 1407e.c, 1407f.c, 1407d.c, 140b2.c, 14142.c, 1411e.c.

## A. Who fills the PadManipulator, what it rewrites, where to hook
- ChrIns+0x58 = the manipulator (FUN_140a217a0 stores FUN_1407f8480(type) in param_1[0xb]; type 1 = PadManipulator, size 0x450,
  ctor FUN_1407ed3a0, vtable 0x1429d4b30). ChrCtrl (ChrIns+0x50) +0x120 holds the same object. Manipulator +0x300 = its ChrIns.
- Per frame: ChrIns step FUN_140a04850 (140a0.c:2967) -> FUN_1407ded90(ChrCtrl, dt) (1407d.c:10417) -> manipulator vtable +0x48 =
  Pad update FUN_1407ef120 (1407e.c:8076; args this, float dt, ChrCtrl; bytes 40 55 56 57 48 83 ec 70). When the ChrCtrl gate
  (ctrl+0x190 bits 0 and 1 set, chr flag bit 0x12 clear) is off it calls vtable +0x50 = FUN_1407ef300 instead (clears all, no input).
  Afterwards vtable +0x58 = FUN_1407eeb50 (post: locomotion module FUN_140b4f1a0).
- Pad update order: (1) vtable +0x40 FUN_1407e3510 zeroes +0x10,+0xC0,+0x20..0x48,+0xE0,+0xF4..0x114 and calls ActionRequest
  begin-frame FUN_140b2c380 (prev=held, held=0, requested=0); (2) base FUN_1407e3690; (3) FUN_1407f0a70 (1407f.c:322) move vector;
  (4) FUN_1407f01f0 camera-relative +0x310; (5) FUN_1407f0850 (1407f.c:13) quick-item slots; (6) FUN_1407ef960 (1407e.c:10192) turns pad
  buttons into ActionRequest bits, then calls FUN_140b2c190 (module update); (7) back in ef120: [+0xF5] = pad state+0x3e (guard),
  [+0xF4] = pad state+0x3c (attack), both OR into [+0x2DC]. Confidence high (read from code).
- So yes: stick (+0x10, +0xC0) and button bytes (+0xF4.., cleared at 1, set at 5 and 7) are rewritten EVERY frame; a write made
  earlier in the frame is wiped, so write inside or after step 6.
- Stick = world-space (camera-rotated) vector x,0,z at +0x10/+0x14/+0x18 (+0x1C = 0), copy at +0xC0/+0xC4/+0xC8/+0xCC, written at the
  end of FUN_1407f1710 (1407f.c:305-317) and FUN_1407f0f50. Length 0..1 (walk halved). Locomotion FUN_140b4afd0 (MoveSpeedLevel, MoveAngle)
  reads +0x10..+0x18 (not +0xC0); the camera chase reads +0xC0. Companions when moving: manip +0x438 byte = 1, +0x2FC float speed
  factor (1.0 default), module +0x184 bit0 (FUN_140b2bf40) set when length > 1.2e-7. Raw stick is pad state x [+0x0], y [+0x4].
  No input: ef960 zeroes +0x10/+0xC0 (1407e.c:10442). Sign convention of the rotation: inferred, see docs/kb/camera.md:154.
- BEST HOOK SPOT: FUN_140b2c190 (140b2.c:8805) PRE-hook. rcx = ActionRequest module, rdx = FD4Time pointer (float dt at +8). Bytes
  48 89 5c 24 18 55 48 83 ec 20 (10-byte clean prefix, next insn 4c 8b 41 10). Called only from ef960 (Wolf pad) and FUN_1407e6610
  (ComManipulator, enemies): filter rcx == module of the player, i.e. [[ChrIns+0x1FF8]+0x80]. At that point the stick is final and the
  module has not yet derived pressed/pending/requested, so inject there: OR bits into [mod+0x10] (u64 held) and write the stick to
  manip +0x10..0x18 and +0xC0..0xC8 (+0x438 = 1, mod+0x184 bit0 set). A post-hook on ef120 is too late for buttons (requests already
  built) but fine for the stick and the +0xF4/+0xF5 bytes.
- Button layout = ActionRequest bit (HKS ACTION_ARM_*) -> pad digital index (pad state byte +0x67+idx via FUN_14095fd80). Table at
  0x143b07230 (14 pairs, std::map built by FUN_140b25b20, read by FUN_140b258d0; loop in ef960 over bits 0..0x20):
  0 attack->14, 1 sub-attack/prosthetic (also sets 0x12 tool, 0x1e)->15, 2 guard->16, 3 grapple WIRE_SHOOT (also 0x1d)->17,
  4 jump->0x59, 5 SP_MOVE dash/sprint hold (also 0x1c)->0x5a, 6 change-style->0x5b, 7 use item->0x5c, 8 switch form->0x5d,
  9 weapon R->0x5e, 10 weapon L->0x5f, 11 change item->0x60, 12 wall hang->0x23, 31 eavesdrop->0x6a.
  Computed, not in the table: 13 backstep and 14 rolling/step from pad digital 0x36 (state+0x9d) with stick latch and timers
  (FUN_1407f1b70, 1407f.c:795; bit 14 also sets 0x17); 15 crouch from pad state+0x4d; 16/17 from FUN_1407ef480 (pad state +0x41/+0x43);
  0x13 quick-item slot (module+0x174 = slot, manip [+0xF4+0x17+slot] = 1); 0x19/0x1a attack variants (FUN_1407ed630);
  0x20 combat art = attack newly pressed while guard held (or attack held and 0x20 set last frame), no own button.
  Lock-on is not an ActionRequest bit (ChrIns+0x1070 flag, tools/rec/fields.py lock_flag).
  The physical pad/key names of the digital indexes were not found (virtual layer); injecting at action-bit level avoids needing them.

## B. ActionRequest module (ChrIns+0x1FF8 bag, +0x80; accessors FUN_1407e3ea0, FUN_140b36170)
- Layout (FUN_140b2c190 140b2.c:8805, FUN_140b2c380 :8289, FUN_140b2b6d0 :7641), 33 bits (0..0x20), all u64 masks:
  +0x10 held now, +0x18 held last frame, +0x20 pressed (held and not prev), +0x28 released, +0x30 REQUESTED (what HKS sees),
  +0x38 pending (latched press), +0xD0 accept mask, +0xE0 execute mask (both rebuilt per frame; copies +0x178/+0x1A8/+0x1B0),
  +0xE8 float[33] hold seconds, +0x16C float hold timer of the stick flag, +0x170 s32 id (-1), +0x174 quick-item slot,
  +0x184 u32 frame flags (bit0 = stick active), +0x188 flags (bit0 = drop all pending; HKS act 9101 ResetInputQueue),
  +0x190/+0x198/+0x1A0 copies of +0x20/+0x30/+0x38.
- Logic: pending |= pressed and accept; requested |= pending and execute (skipped for bits 0x10, 0x11, 7 when ChrCtrl+0x50 byte +0xF9 set).
- HKS env 1106 ActionRequest(idx) = case 0x452 of FUN_140b36410 -> FUN_140b2b6d0(mod,1,idx): bit idx of +0x30 (of +0x20 when the
  chr flag at bag[0]+0x10 bit0 is set; input-queue list when mod+0x1B8 is not 0). env 1107 = case 0x453: bit of +0x28 (released).
  env 1108 ActionDuration(idx) = case 0x454: float [mod+0xE8+4*idx] times 1000 as int (ms); idx above 0x20 gives 0.
- Action ids per button: the list in A. Names: reference/SekiroHKS/c0000_define.hks:4-21 (ATTACK 0, SUB_ATTACK 1, GUARD 2, WIRE_SHOOT 3,
  JUMP 4, SP_MOVE 5, USE_ITEM 7, CHANGE_WEAPON_L 10, WALL_HANG 12, BACKSTEP 13, ROLLING 14, CROUCH 15, SHINOBI_WEP_ACTION 18,
  EAVESDROP 31, SPECIAL_ATTACK 32) and c0000_transition.hks:433-440 (6 CHANGE_STYLE, 8 SWITCH_FORM, 9 WEAPON_R, 11 CHANGE_ITEM).
  Accept/execute flag history: docs/kb/input-buffer.md.

## C. HKS state callbacks per frame
- Class hkbScriptGenerator (vtable 0x142c49b50, ctor FUN_1413c1680). Each state node keeps Lua call strings, built in FUN_1411e4f20
  (1411e.c:6544-6566): onActivate() at node+0xA0, onDeactivate() at +0xC0, onUpdate() at +0xA8, each prefixed by the state name.
- Per-frame update = vtable +0xD0 FUN_1414210e0(node, ctx, float dt) (14142.c:611; bytes 40 53 48 83 ec 50 48 89 6c 24 60);
  activate = vtable +0x30 FUN_141420860 (14142.c:478); deactivate = vtable +0x48 FUN_141420a30 (14142.c:887); generate = +0xA8 FUN_141420ee0.
- All of them call ONE runner: FUN_1414212c0(node=rcx, scriptStrSlot=rdx, cacheHandle=r8d, ctx=r9, ...) (14142.c:255; bytes
  48 89 5c 24 08 48 89 74 24 10 57 = 10-byte clean prefix). HOOK THIS. kind = rdx - rcx (0xA0 activate, 0xA8 onUpdate, 0xB0 generate,
  0xB8 other, 0xC0 deactivate); call text = C string at (qword[rdx] with bit0 cleared), e.g. W_Event100_onUpdate(); state name =
  C string at (qword[rcx+0x48] with bit0 cleared). Active states for a frame = the onUpdate calls between two ticks (call path
  read from code with high confidence, not run live).
- Gap: fires for every character, not only Wolf. The node-to-character link is not traced (FUN_1411e4f20 reaches the character through
  param_2 -> +0xB8 -> FUN_1413a6af0); use a per-frame stamp plus state-name prefix, or trace ctx (r9) to the character first. Open.
- The behaviour graph active state id/name reachable from ChrIns: not found statically (open); the callback hook gives names directly.
