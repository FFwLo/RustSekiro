-- Sv1 recorder v2: frames (Wolf + 2 enemies + globals) + events (SpEffects, slots, commands, markers) + hooks
-- Source of truth: shinobi-combat/tools/memscope/rec2.lua (`python -m tools.rec install` copies it to memscope).
-- args: dir (required: made by `python -m tools.rec new`), seconds (default 900), hz (default 60),
--       hooks (comma list from HOOKS below, default ""), raw (true = every block every tick; default: slow blocks every 6th tick)
-- Stops on: time up, "stop" line in <dir>/inbox.txt (F10 in the key helper), or the game closing.
-- Files: frames.bin ("<I4 d B B I2" tick, ms, slot, block id, len + bytes; slot 255 = globals),
--        events.jsonl, hooks.jsonl, manifest_lua.json. Sekiro 1.06, base 0x140000000.
-- Layout: WorldChrMan [0x143d7a1e0] (+0x88 player); EnemyIns vt 0x142a27f28; ChrIns +0x1FF8 module bag;
--   SpEffect list: [ChrIns+0x11D0]+0x8 head, entry +0x58 id, +0x60 param row, +0x70 next (FUN_140bfebf0).
--   Debug flags 0x143d7a366 (+0 player no death, +0xB all no death, +0x10 disable AI, +0x3 no goods consume);
--   game speed [0x143e9f748]+0x344; FieldArea [0x143d5c0a0] +0x30 ChrCam (SekiroTool Offsets.cs, patch 1.6.0).
local args = args or {}
local DIR = assert(args.dir, "args.dir missing: run `python -m tools.rec new` first")
local SECONDS = tonumber(args.seconds) or 900
local HZ = tonumber(args.hz) or 60
-- enemies recorded per tick (slot 1 = lock-on target, then nearest); args.enemies (default 6, was 2)
local ENEMIES = tonumber(args.enemies) or 6

local WCM_PTR = addr("0x143d7a1e0")
local ENEMY_VT = addr("0x142A27F28")
local DBG = addr("0x143D7A366")
local FLIPPER = addr("0x143E9F748")
local FIELD_AREA = addr("0x143D5C0A0")
local GAMEDATA = addr("0x143D5AAC0")
-- docs/kb/rec-research/*.md (static RE 2026-10-21)
local WIRE_TGT = addr("0x143D8E058")   -- WireActionTarget: +0x10 grapple point, +0x28 handle
local LOCK_MAN = addr("0x143D78058")   -- LockTgtManImp: +0x10 candidate list
local DRAW_PARAM = addr("0x143D8DF18") -- CSWorldSceneDrawParamManager: [+0x20] area entries (GParam ids)

-- Per-character blocks {id, name, kind, offset, start, length, every n ticks (args.raw = every tick), only "wolf" / "enemy"}:
-- kind "mod" = module bag slot, "chr" = ChrIns field, "ptr" = object at [ChrIns+off], "ptr2" = [[ChrIns+a]+b]. Ids 1-22 = recorder.lua.
local BLOCKS = {
  {1, "chr_head", "chr", nil, 0x0, 0x100, 6},
  {2, "chr_lock", "chr", nil, 0x1000, 0x200, 1},
  {3, "physics", "mod", 0x68, 0x0, 0x120, 1},
  {4, "timeact", "mod", 0x10, 0x0, 0x200, 1},
  {5, "data", "mod", 0x18, 0x100, 0x180, 1},
  {6, "behavior", "mod", 0x28, 0x0, 0x200, 1},
  {7, "action_flag", "mod", 0x00, 0x0, 0x200, 1},
  {8, "action_request", "mod", 0x80, 0x0, 0x200, 1},
  {9, "throw", "mod", 0x88, 0x0, 0x100, 1},
  {10, "hitstop", "mod", 0x90, 0x0, 0x80, 1},
  {11, "damage", "mod", 0x98, 0x0, 0x200, 1},
  {12, "knockback", "mod", 0xA8, 0x0, 0x100, 6},
  {13, "autohoming", "mod", 0x108, 0x0, 0x100, 6},
  {14, "behavior_script", "mod", 0x08, 0x0, 0x100, 6},
  {15, "toughness", "mod", 0x48, 0x0, 0x80, 1},
  {16, "super_armor", "mod", 0x40, 0x0, 0x80, 1},
  {17, "sword_arts", "mod", 0xF8, 0x0, 0x100, 6},
  {18, "pad", "ptr", 0x58, 0x0, 0x450, 1, "wolf"}, -- PadManipulator 0x450 (rec-research/input-hks.md)
  {19, "tae_events", "ptr", 0x60, 0x0, 0x200, 6},
  {20, "speffect", "ptr", 0x11D0, 0x0, 0x100, 1},
  {21, "ai", "mod", 0x38, 0x0, 0x300, 6},
  -- AiThink = [[ChrIns+0x58]+0x340]: +0xB742 LastAct, +0xB744 LastKengekiAct (SekiroTool)
  {22, "ai_think", "ptr2", {0x58, 0x340}, 0xB740, 0x10, 1},
  {23, "resist", "mod", 0x20, 0x0, 0x40, 6},
  {24, "chr_flags", "chr", nil, 0x1F40, 0x10, 6},
  -- ChrIns +0x2fc root-motion scale (set by FUN_1407f0a70 from TAE 760 BoostRootMotionToReachTarget)
  {25, "chr_rm", "chr", nil, 0x2F0, 0x10, 1},
  -- rec-research/physics-traversal.md: velocity +0x160, floor contact +0xA2, last grounded pos +0x850, air time +0x8D0,
  -- floor material +0x940, slide +0xA50, wall +0xA84..; docking kind +0x10 (hang / wall hug / dive flags); ladder s8 +0x258
  {43, "physics_ext", "mod", 0x68, 0x120, 0xC70, 1, "wolf"},
  {44, "material", "mod", 0xA0, 0x0, 0x40, 1},
  {45, "fall", "mod", 0x70, 0x0, 0x40, 6},
  {46, "docking", "mod", 0x128, 0x0, 0x860, 1, "wolf"},
  {47, "wire", "mod", 0x130, 0x0, 0x40, 1, "wolf"},
  {40, "ladder", "ptr2", {0x50, 0x50}, 0x250, 0x2E0, 1, "wolf"},
  -- rec-research/ai-world-fx.md A: targeting system AiThink+0x7B30 (+0x158 alert state, +0x40 sight meters), home ChrIns+0xE0
  {41, "targeting", "ptr2", {0x58, 0x340}, 0x7B30, 0x170, 1, "enemy"},
  {42, "home", "chr", nil, 0xE0, 0x20, 30},
}
-- Global blocks (slot 255) {id, name, chain from an absolute pointer, start, length}
local GLOBALS = {
  {30, "chrcam", {FIELD_AREA, 0x30}, 0x0, 0x200},
  {31, "followcam", {FIELD_AREA, 0x30, 0x60}, 0x0, 0x100},
  {32, "flipper", {FLIPPER}, 0x340, 0x10},
  {33, "gamedata", {GAMEDATA}, 0x90, 0x10},
  {34, "debug_flags", nil, DBG, 0x20},
  -- current map block: [[[WorldChrMan]+0x20]+0x8] +0x8 = id 0xAABBCCDD -> mAA_BB_CC_DD (SekiroTool WorldBlockInfo/Id)
  {35, "worldblock", {WCM_PTR, 0x20, 0x8}, 0x0, 0x20},
  {36, "wire_target", {WIRE_TGT}, 0x0, 0x90},
  {37, "lockon", {LOCK_MAN}, 0x2830, 0x20},
  -- PlayerGameData: sen +0x7C, XP +0x160, attack power +0x48, resurrection +0x148..+0x168 (ai-world-fx.md D)
  {38, "pgd", {GAMEDATA, 0x8}, 0x0, 0x180, 6},
  -- area GParam entries, 4 x 0x118 (+0x18 current ids; low confidence)
  {39, "gparam", {DRAW_PARAM, 0x20}, 0x0, 0x460, 30},
}

-- Preferred: the me3 hook DLL (below, "me3 hook DLL") writes hooks.jsonl itself. These memscope hooks
-- (args.hooks) are the fallback when the game runs without it. All PRE hooks with no buffer capture; each must be proven alone first.
-- status: "ok" = ran without a crash under me3, "untested", "crashed" (date).
local HOOKS = {
  turn = {"0x1407dab40", "turn controller FUN_1407dab40", "ok 2026-10-15"},
  items = {"0x14079ab70", "AdjustItemCount(inv, item id rdx, delta r8) SekiroTool", "untested"},
  ai_goal = {"0x1405ce510", "AddSubGoal(goals, goal id rdx, life r8) SekiroTool", "untested"},
  music = {"0x1408cebe0", "StartMusic(this, id rdx) SekiroTool", "untested"},
  posture = {"0x1408439a0", "posture damage FUN_1408439a0", "crashed 2026-10-08 (post hook, before me3)"},
  guardcut = {"0x140840870", "guard posture cut FUN_140840870", "crashed 2026-10-08 (before me3)"},
}

local function valid(p) return p and p ~= 0 and isValidPointer(p) end
local function jstr(s) return '"' .. tostring(s):gsub('\\', '\\\\'):gsub('"', '\\"'):gsub('[%c]', ' ') .. '"' end

local f = assert(io.open(DIR .. "\\frames.bin", "wb"))
local ev = assert(io.open(DIR .. "\\events.jsonl", "w"))
local hk = nil
local t0 = clock()
local tick = 0
local function event(kind, body)
  ev:write(fmt('{"tick":%d,"ms":%.1f,"kind":"%s"%s}\n', tick, clock() - t0, kind, body and ("," .. body) or ""))
end

local function chain(list)
  local p = readPointer(list[1])
  for i = 2, #list do
    if not valid(p) then return nil end
    p = readPointer(p + list[i])
  end
  return valid(p) and p or nil
end

local function pos(chr)
  local bag = readPointer(chr + 0x1FF8)
  local ph = valid(bag) and readPointer(bag + 0x68)
  if not valid(ph) then return nil end
  return readFloat(ph + 0x80), readFloat(ph + 0x84), readFloat(ph + 0x88)
end

-- Enemy containers: any pointer in WorldChrMan[0..0x2000] whose target holds EnemyIns pointers (recorder.lua).
local enemy_list, last_scan = {}, -1e9
local function find_enemies()
  local wcm = readPointer(WCM_PTR)
  enemy_list = {}
  last_scan = clock()
  if not valid(wcm) then return end
  local seen = {}
  for off = 0, 0x2000, 8 do
    local p = readPointer(wcm + off)
    if valid(p) then
      for o2 = 0, 0x4000, 8 do
        local q = readPointer(p + o2)
        if valid(q) and not seen[q] and readPointer(q) == ENEMY_VT then seen[q] = true; enemy_list[#enemy_list + 1] = q end
      end
    end
  end
  -- ptr:handle:chr_id of every enemy, so hook lines (chr pointers / handles) can be named in ingest
  local chrs = {}
  for _, q in ipairs(enemy_list) do
    chrs[#chrs + 1] = fmt("%s:%d:%d", toHex(q), readInteger(q + 8) & 0xffffffff, readInteger(q + 0x68) or -1)
  end
  event("scan", fmt('"enemies":%d,"chrs":"%s"', #enemy_list, table.concat(chrs, ",")))
end

-- Slot 1 = lock-on target (PlayerIns +0x11E4 handle, ChrIns +0x8 handle), else nearest; slots 2..ENEMIES = next nearest.
local function pick_enemies(player)
  local px, py, pz = pos(player)
  if not px then return {} end
  local lock = readInteger(player + 0x11E4) & 0xffffffff
  local found, target = {}, nil
  for _, e in ipairs(enemy_list) do
    if valid(e) and readPointer(e) == ENEMY_VT then
      local x, y, z = pos(e)
      local hp = 0
      local bag = readPointer(e + 0x1FF8)
      local dm = valid(bag) and readPointer(bag + 0x18)
      if valid(dm) then hp = readInteger(dm + 0x130) or 0 end
      if x and hp > 0 then found[#found + 1] = {e, (x - px) ^ 2 + (y - py) ^ 2 + (z - pz) ^ 2} end
      if lock ~= 0xffffffff and (readInteger(e + 8) & 0xffffffff) == lock then target = e end
    end
  end
  if lock ~= 0xffffffff and not target and clock() - last_scan > 10000 then find_enemies() end
  table.sort(found, function(a, b) return a[2] < b[2] end)
  local out = {}
  if target then out[1] = target end
  for _, fe in ipairs(found) do
    if #out >= ENEMIES then break end
    if fe[1] ~= target then out[#out + 1] = fe[1] end
  end
  return out
end

local function put(slot, id, base, start, len)
  if not valid(base) then return end
  local bytes = readBytes(base + start, len)
  if not bytes or #bytes == 0 then return end
  f:write(string.pack("<I4dBBI2", tick, clock() - t0, slot, id, #bytes), string.char(table.unpack(bytes)))
end

local RAW = args.raw == true or args.raw == "true"
local function write_chr(slot, chr)
  for _, b in ipairs(BLOCKS) do
    if (RAW or b[7] == 1 or tick % b[7] == 0) and (b[8] == nil or (b[8] == "wolf") == (slot == 0)) then
    local base
    if b[3] == "chr" then base = chr
    elseif b[3] == "ptr" then base = readPointer(chr + b[4])
    elseif b[3] == "ptr2" then
      local p1 = readPointer(chr + b[4][1])
      base = valid(p1) and readPointer(p1 + b[4][2])
    else
      local bag = readPointer(chr + 0x1FF8)
      base = valid(bag) and readPointer(bag + b[4])
    end
    put(slot, b[1], base, b[5], b[6])
    end
  end
end

local function write_globals()
  for _, g in ipairs(GLOBALS) do
    if g[6] and not RAW and tick % g[6] ~= 0 then
    elseif g[3] == nil then put(255, g[1], g[4], 0, g[5])
    else put(255, g[1], chain(g[3]), g[4], g[5]) end
  end
end

-- SpEffects: per slot, the set of active ids; events on add / remove (entry +0x0..+0x58 raw on add).
local sp_prev = {}
for s = 0, ENEMIES do sp_prev[s + 1] = {} end
local function speffects(slot, chr)
  local se = readPointer(chr + 0x11D0)
  local now = {}
  if valid(se) then
    local e, n = readPointer(se + 0x8), 0
    while valid(e) and n < 96 do
      local id = readInteger(e + 0x58)
      if id then
        now[id] = true
        if not sp_prev[slot + 1][id] then
          event("sp_add", fmt('"slot":%d,"id":%d,"raw":"%s"', slot, id, readBytesHex and (readBytesHex(e, 0x58) or "") or ""))
        end
      end
      e = readPointer(e + 0x70)
      n = n + 1
    end
  end
  for id in pairs(sp_prev[slot + 1]) do
    if not now[id] then event("sp_del", fmt('"slot":%d,"id":%d', slot, id)) end
  end
  sp_prev[slot + 1] = now
end

-- Lock-on candidates (LockTgtManImp list: node +0x78 handle, +0x90 flags 0x10 candidate / 0x20 current, +0x80 next):
-- an event whenever the list changes
local lock_prev = ""
local function lock_cands()
  local mgr = readPointer(LOCK_MAN)
  if not valid(mgr) then return end
  local n, out = readPointer(mgr + 0x10), {}
  while valid(n) and #out < 32 do
    out[#out + 1] = fmt("%d:%d", readInteger(n + 0x78) & 0xffffffff, readByte(n + 0x90) or 0)
    n = readPointer(n + 0x80)
  end
  local s = table.concat(out, ",")
  if s ~= lock_prev then lock_prev = s; event("lock_cands", '"list":"' .. s .. '"') end
end

-- Inbox commands (one per line, written by `python -m tools.rec keys`): test-setup writes only.
local saved = {}
for _, o in ipairs({0x0, 0x3, 0xB, 0x10}) do saved[o] = readByte(DBG + o) end
local fl0 = readPointer(FLIPPER)
local speed0 = valid(fl0) and readFloat(fl0 + 0x344) or 1.0
local stop, inbox_pos, saved_pos = false, 0, nil
local dll = {} -- the me3 hook DLL's exported variables (filled below)
local DBG_CMD = {nodead = 0x0, noitems = 0x3, allnodead = 0xB, freezeai = 0x10}
local function player_pos_ptr()
  -- practice tool chain [[[WorldChrMan]+0x48]+0x28]+0x80 (pointers.rs `position`)
  local p = chain({WCM_PTR, 0x48, 0x28})
  return p and p + 0x80
end
local function command(line)
  local cmd, rest = line:match("^(%S+)%s*(.*)$")
  if not cmd then return end
  if cmd == "stop" then stop = true
  elseif cmd == "mark" then event("mark", '"text":' .. jstr(rest))
  elseif DBG_CMD[cmd] then writeByte(DBG + DBG_CMD[cmd], tonumber(rest) or 0)
  elseif cmd == "speed" then
    local fl = readPointer(FLIPPER)
    if valid(fl) then writeFloat(fl + 0x344, tonumber(rest) or 1.0) end
  elseif cmd == "savepos" then
    local p = player_pos_ptr()
    if p then saved_pos = {readFloat(p), readFloat(p + 4), readFloat(p + 8), readFloat(p + 12)} end
  elseif cmd == "play" then
    -- scripted input (tools/rec-hooks play.rs): hand the DLL the script path and start it
    if not (dll and dll.SV1REC_PLAY and dll.SV1REC_PLAYFILE) then event("play", '"err":"hook DLL without pad_play"'); return end
    local b = {string.byte(rest, 1, -1)}
    b[#b + 1] = 0
    writeBytes(dll.SV1REC_PLAYFILE, b)
    writeInteger(dll.SV1REC_PLAY, 1)
  elseif cmd == "loadpos" then
    local p = player_pos_ptr()
    if p and saved_pos then for i = 1, 4 do writeFloat(p + (i - 1) * 4, saved_pos[i]) end end
  else event("cmd_unknown", '"line":' .. jstr(line)); return end
  event("cmd", '"line":' .. jstr(line))
end
local function poll_inbox()
  local fi = io.open(DIR .. "\\inbox.txt", "r")
  if not fi then return end
  fi:seek("set", inbox_pos)
  for line in fi:lines() do command(line) end
  inbox_pos = fi:seek("end")
  fi:close()
end

-- Hooks
local hook_ids, hook_names = {}, {}
local want = {}
for name in tostring(args.hooks or ""):gmatch("[%w_]+") do want[#want + 1] = name end
local okd, dllbase = pcall(getModuleBase, "sv1_rec_hooks.dll")
if #want > 0 and okd and dllbase and dllbase ~= 0 then
  event("hook", '"name":"skipped: the me3 hook DLL is loaded and writes hooks.jsonl"')
  want = {}
end
if #want > 0 then
  pcall(destroyRingBuffer)
  createRingBuffer({entry_count = 4096, max_data_size = 16})
  hk = assert(io.open(DIR .. "\\hooks.jsonl", "w"))
  for _, name in ipairs(want) do
    local h = HOOKS[name]
    if h then
      local ok, err = pcall(hookFunction, addr(h[1]), {name = name, type = "pre", buffer_arg = -1, length_arg = -1, max_capture = 0, stack_args = {5, 6}})
      event("hook", fmt('"name":"%s","addr":"%s","ok":%s,"err":%s', name, h[1], ok and "true" or "false", jstr(err or "")))
    end
  end
  for k, v in pairs(listHooks() or {}) do
    if type(v) == "table" and v.name then hook_ids[#hook_ids + 1] = v.hook_id or k; hook_names[tostring(v.hook_id or k)] = v.name end
  end
end
local function drain_hooks()
  if not hk then return end
  for _, e in ipairs(readRingBuffer(1000) or {}) do
    local parts = {fmt('"tick":%d', tick)}
    local hn = e.name or hook_names[tostring(e.hook_id or e.id or "")]
    if hn and not e.name then parts[#parts + 1] = '"name":' .. jstr(hn) end
    for k, v in pairs(e) do
      if type(v) == "number" then parts[#parts + 1] = fmt('"%s":%s', k, math.type(v) == "integer" and tostring(v) or fmt("%.6g", v))
      elseif type(v) == "string" then parts[#parts + 1] = fmt('"%s":%s', k, jstr(v))
      elseif type(v) == "table" then
        local a = {}
        for i, x in ipairs(v) do a[i] = tostring(x) end
        parts[#parts + 1] = fmt('"%s":[%s]', k, table.concat(a, ","))
      end
    end
    hk:write("{" .. table.concat(parts, ",") .. "}\n")
  end
end

-- First enemy scan (2-4 s) before the clock starts, so tick 0 is not stretched
find_enemies()
t0 = clock()

-- me3 hook DLL (tools/rec-hooks, profile me3/sekiro-rec.me3): find its exported variables, hand it the folder.
local function export_of(base, name)
  local nt = base + readInteger(base + 0x3C)
  local dir = base + readInteger(nt + 0x88)
  local n, funcs, names, ords = readInteger(dir + 0x18), base + readInteger(dir + 0x1C), base + readInteger(dir + 0x20), base + readInteger(dir + 0x24)
  for i = 0, n - 1 do
    if readString(base + readInteger(names + i * 4), 64) == name then
      return base + readInteger(funcs + readUInt16(ords + i * 2) * 4)
    end
  end
end
local okb, dbase = pcall(getModuleBase, "sv1_rec_hooks.dll")
if okb and dbase and dbase ~= 0 then
  for _, n in ipairs({"SV1REC_ON", "SV1REC_TICK", "SV1REC_COUNT", "SV1REC_DIR", "SV1REC_PLAY", "SV1REC_PLAYFILE", "SV1REC_PLAYFRAME"}) do dll[n] = export_of(dbase, n) end
end
if dll.SV1REC_ON and dll.SV1REC_DIR and dll.SV1REC_TICK then
  local b = {string.byte(DIR, 1, -1)}
  b[#b + 1] = 0
  writeBytes(dll.SV1REC_DIR, b)
  writeInteger(dll.SV1REC_TICK, 0)
  writeInteger(dll.SV1REC_ON, 1)
  event("dll", fmt('"base":"%s","count0":%d', toHex(dbase), readInteger(dll.SV1REC_COUNT) or -1))
else
  dll = nil
  event("dll", '"base":"none"')
end

-- Main loop
event("start", fmt('"hz":%d,"seconds":%d,"hooks":%s,"speed0":%.3f', HZ, SECONDS, jstr(args.hooks or ""), speed0))
local period = 1000 / HZ
local slots, slow, missing, last_player = {}, 0, 0, nil
local ok, err = pcall(function()
  while clock() - t0 < SECONDS * 1000 and not stop do
    local t = clock()
    local wcm = readPointer(WCM_PTR)
    local player = valid(wcm) and readPointer(wcm + 0x88)
    if valid(player) then
      missing = 0
      if player ~= last_player then
        last_player = player
        event("player", fmt('"ptr":"%s","handle":%d', toHex(player), readInteger(player + 8) & 0xffffffff))
      end
      if tick > 0 and tick % (HZ * 30) == 0 or #enemy_list == 0 and tick % (HZ * 5) == 0 then find_enemies() end
      if tick % (HZ // 4) == 0 then
        local picked = pick_enemies(player)
        for s = 1, ENEMIES do
          if picked[s] ~= slots[s] then
            slots[s] = picked[s]
            local c = picked[s]
            event("slot", fmt('"slot":%d,"ptr":"%s","handle":%d,"chr_id":%d,"entity":%d', s, c and toHex(c) or "0",
              c and (readInteger(c + 8) & 0xffffffff) or -1, c and (readInteger(c + 0x68) or -1) or -1, c and (readInteger(c + 0x1A24) or -1) or -1))
            sp_prev[s + 1] = {}
          end
        end
      end
      local chrs = {player}
      for s = 1, ENEMIES do chrs[s + 1] = slots[s] end
      for s = 0, ENEMIES do
        local c = chrs[s + 1]
        if valid(c) then write_chr(s, c); speffects(s, c) end
      end
      write_globals()
      if tick % 3 == 0 then lock_cands() end
    else
      missing = missing + 1
      if missing == HZ * 2 then event("no_player") end
      if not isAttached() then break end
    end
    if dll then writeInteger(dll.SV1REC_TICK, tick) end
    if tick % 6 == 0 then poll_inbox() end
    if tick % 15 == 0 then drain_hooks(); f:flush(); ev:flush() end
    tick = tick + 1
    local spent = clock() - t
    if spent > period then slow = slow + 1 else sleep(math.floor(period - spent)) end
  end
end)

-- Cleanup: stop the DLL log, unhook, restore every switch we may have changed
local dll_count = -1
if dll then writeInteger(dll.SV1REC_ON, 0); dll_count = readInteger(dll.SV1REC_COUNT) or -1 end
drain_hooks()
for _, id in ipairs(hook_ids) do pcall(unhookFunction, id) end
if hk then pcall(destroyRingBuffer); hk:close() end
for o, v in pairs(saved) do if v then pcall(writeByte, DBG + o, v) end end
local fl = readPointer(FLIPPER)
if valid(fl) then pcall(writeFloat, fl + 0x344, speed0) end
event("end", fmt('"dll_calls":%d,"ticks":%d,"slow":%d,"seconds":%.1f,"ok":%s,"err":%s', dll_count, tick, slow, (clock() - t0) / 1000, ok and "true" or "false", jstr(err or "")))
ev:close()
f:close()
local mf = assert(io.open(DIR .. "\\manifest_lua.json", "w"))
local bl = {}
for _, b in ipairs(BLOCKS) do bl[#bl + 1] = fmt('"%d":{"name":"%s","kind":"%s","start":%d,"len":%d,"every":%d}', b[1], b[2], b[3], b[5], b[6], RAW and 1 or b[7]) end
for _, g in ipairs(GLOBALS) do bl[#bl + 1] = fmt('"%d":{"name":"%s","kind":"global","start":%d,"len":%d}', g[1], g[2], g[3] and g[4] or 0, g[5]) end
mf:write(fmt('{"game":"sekiro 1.06","hz":%d,"ticks":%d,"slow":%d,"seconds":%.1f,"hooks":%s,"blocks":{%s}}\n',
  HZ, tick, slow, (clock() - t0) / 1000, jstr(args.hooks or ""), table.concat(bl, ",")))
mf:close()
print(fmt("recorded %d ticks (%d slow) to %s %s", tick, slow, DIR, ok and "" or tostring(err)))
addResult("dir", DIR)
addResult("ticks", tick)
addResult("ok", ok)
