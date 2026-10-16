"""Decoded fields of the recorder's raw blocks: the one place to add a field once it is understood.

Each field: name -> (block name, offset inside the block, type, note). Offsets are relative to the
block's own start (see tools/memscope/rec2.lua BLOCKS: e.g. the data block starts at module +0x100,
so HP at ChrDataModule +0x130 is +0x30 here). `verified` = confirmed in a live recording.
"""

# (block, offset, type, verified, source / note)
FIELDS = {
    # physics module (ChrIns +0x1FF8 bag +0x68)
    "x": ("physics", 0x80, "f32", True, "SekiroTool ChrPhysicsOffsets X; Sekiro y is up"),
    "y": ("physics", 0x84, "f32", True, "height"),
    "z": ("physics", 0x88, "f32", True, ""),
    "yaw": ("physics", 0x74, "f32", True, "radians"),
    # TimeAct module (+0x10): current anim (face layer a000_79xxxx filtered in ingest)
    "anim": ("timeact", 0x100, "i32", True, "movement session 2026-10-08"),
    # data module (+0x18), block starts at +0x100
    "hp": ("data", 0x30, "i32", True, "ChrDataOffsets Hp 0x130"),
    "hp_max": ("data", 0x34, "i32", True, "MaxHp 0x134"),
    "posture": ("data", 0x48, "i32", True, "remaining posture 0x148 (counts down on damage)"),
    "posture_max": ("data", 0x4C, "i32", True, "MaxPosture 0x14C"),
    # pad (PlayerIns +0x58 PadManipulator for Wolf; ComManipulator for enemies)
    "btn_attack": ("pad", 0xF4, "u8", True, "rec_windows.py"),
    "btn_guard": ("pad", 0xF5, "u8", True, "rec_windows.py"),
    # AiThink [[ChrIns+0x58]+0x340], block starts at +0xB740
    "ai_force_act": ("ai_think", 0x1, "u8", False, "ForceAct 0xB741"),
    "ai_act": ("ai_think", 0x2, "u8", False, "LastAct 0xB742 (battle script act)"),
    "ai_kengeki": ("ai_think", 0x4, "u8", False, "LastKengekiAct 0xB744 (sword clash act)"),
    # ChrIns +0x1000.. block
    "lock_flag": ("chr_lock", 0x70, "u8", True, "ChrIns +0x1070 lock-on byte (move_log.lua; 0/1 toggles live 2026-10-15)"),
    "lock_target": ("chr_lock", 0x1E4, "u32", False, "PlayerIns +0x11E4 target handle? stayed 0xFFFFFFFF while locked (rec_20261015_191200) - use lock_flag"),
    "handle": ("chr_head", 0x8, "u32", True, "ChrIns +0x8"),
    # resist module (+0x20)
    "poison": ("resist", 0x10, "f32", False, "ChrResistOffsets"),
    "burn": ("resist", 0x18, "f32", False, ""),
    "shock": ("resist", 0x20, "f32", False, ""),
}

# Global blocks (slot 255)
GLOBAL_FIELDS = {
    "game_speed": ("flipper", 0x4, "f32", False, "SprjFlipperImp +0x344"),
    "igt": ("gamedata", 0xC, "u32", False, "GameDataMan +0x9C (ms)"),
    "dbg_nodead": ("debug_flags", 0x0, "u8", True, "DebugFlags +0x0 player no death"),
    "dbg_no_goods": ("debug_flags", 0x3, "u8", True, "DebugFlags +0x3 player no goods consume (infinite items)"),
    "dbg_no_emblems": ("debug_flags", 0x4, "u8", True, "DebugFlags +0x4 player no emblem consume (2026-10-15 rec: set by SekiroTool)"),
    "dbg_freezeai": ("debug_flags", 0x10, "u8", False, "DebugFlags +0x10"),
    "map_block": ("worldblock", 0x8, "u32", True, "[[WCM+0x20]+0x8]+0x8, 0x0A000000 = m10_00_00_00 (live 2026-10-15)"),
    # ChrCam: CSCam layout like SekiroTool's DebugCamCoords (free cam +0x40 position), verified on
    # rec_20261015_193824 (4-5 m behind Wolf, forward points at him, fov 0.7505):
    # +0x10 / +0x20 / +0x30 matrix rows (right, up, forward), +0x40 position, +0x50 fov (radians)
    "cam_x": ("chrcam", 0x40, "f32", True, "ChrCam +0x40 (CSCam position, check: a few m from Wolf)"),
    "cam_y": ("chrcam", 0x44, "f32", True, ""),
    "cam_z": ("chrcam", 0x48, "f32", True, ""),
    "cam_fwd_x": ("chrcam", 0x30, "f32", True, "matrix row 3 = forward"),
    "cam_fwd_y": ("chrcam", 0x34, "f32", True, ""),
    "cam_fwd_z": ("chrcam", 0x38, "f32", True, ""),
    "cam_fov": ("chrcam", 0x50, "f32", True, "radians"),
    "cam_follow_yaw": ("followcam", 0x10, "f32", False, "SekiroTool comment: ChrExFollowCam +0x10 yaw"),
    "cam_follow_pitch": ("followcam", 0x30, "f32", False, "SekiroTool comment: +0x30 pitch"),
}

TYPES = {"i32": ("<i", 4), "u32": ("<I", 4), "f32": ("<f", 4), "u8": ("<B", 1), "u16": ("<H", 2),
         "i16": ("<h", 2), "u64": ("<Q", 8)}

FACE_LAYER = range(790000, 800000)  # a000_79xxxx AddBlendFace anims interleave in TimeAct +0x100


def map_name(i):
    if i is None:
        return "-"
    return f"m{(i >> 24) & 0xFF:02d}_{(i >> 16) & 0xFF:02d}_{(i >> 8) & 0xFF:02d}_{i & 0xFF:02d}"


def anim_name(a):
    if a is None or a < 0:
        return "-"
    return f"a{a // 1000000:03d}_{a % 1000000:06d}"
