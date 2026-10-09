-- Shinobi live recorder: raw module blocks of Wolf + the 2 nearest enemies every tick -> binary log
-- args: seconds (default 330), hz (default 60), dir (default memscope-data/logs/rec_<time>)
-- Layout found 2026-10-08 (sekiro.exe base 0x140000000):
--   WorldChrMan = [0x143d7a1e0]; player = WorldChrMan+0x88 (PlayerIns); enemies (EnemyIns vt 0x142a27f28)
--   in the array at WorldChrMan+0x998; ChrIns+0x1FF8 = module bag; +0x2000 PlayerGameData;
--   +0x58 PadManipulator, +0x60 SprjChrTaeAnimEvent, +0x11D0 SpecialEffect.
-- Record: "<I4 d B B I2" (tick, clock ms, entity slot, block id, length) + raw bytes.
-- Slots: 0 Wolf, 1 lock-on target (else nearest), 2 nearest other enemy (re-picked every second). Block ids: see BLOCKS / index.txt.

local args = args or {}
local SECONDS = tonumber(args.seconds) or 330
local HZ = tonumber(args.hz) or 60
local stamp = os.date("%Y%m%d_%H%M%S")
local DIR = args.dir or ("C:\\DeadlockModding\\memscope-data\\logs\\rec_" .. stamp)
os.execute('mkdir "' .. DIR .. '" 2>nul')

local WCM_PTR = addr("0x143d7a1e0")
local ENEMY_VT = addr("0x142A27F28")

-- {id, name, base kind, module offset or chr offset, start, length}
-- kind "mod" = module bag slot, "chr" = ChrIns field, "ptr" = object pointed to by a ChrIns field
local BLOCKS = {
  {1, "chr_head", "chr", nil, 0x0, 0x100},
  {2, "chr_lock", "chr", nil, 0x1000, 0x100},
  {3, "physics", "mod", 0x68, 0x0, 0x120},
  {4, "timeact", "mod", 0x10, 0x0, 0x200},
  {5, "data", "mod", 0x18, 0x100, 0x180},
  {6, "behavior", "mod", 0x28, 0x0, 0x200},
  {7, "action_flag", "mod", 0x00, 0x0, 0x200},
  {8, "action_request", "mod", 0x80, 0x0, 0x200},
  {9, "throw", "mod", 0x88, 0x0, 0x100},
  {10, "hitstop", "mod", 0x90, 0x0, 0x80},
  {11, "damage", "mod", 0x98, 0x0, 0x200},
  {12, "knockback", "mod", 0xA8, 0x0, 0x100},
  {13, "autohoming", "mod", 0x108, 0x0, 0x100},
  {14, "behavior_script", "mod", 0x08, 0x0, 0x100},
  {15, "toughness", "mod", 0x48, 0x0, 0x80},
  {16, "super_armor", "mod", 0x40, 0x0, 0x80},
  {17, "sword_arts", "mod", 0xF8, 0x0, 0x100},
  {18, "pad", "ptr", 0x58, 0x0, 0x200},
  {19, "tae_events", "ptr", 0x60, 0x0, 0x200},
  {20, "speffect", "ptr", 0x11D0, 0x0, 0x100},
  {21, "ai", "mod", 0x38, 0x0, 0x300},
  -- AiThink = [[ChrIns+0x58]+0x340] (SekiroTool offsets, patch 1.6.0): +0xB741 ForceAct, +0xB742 LastAct
  -- (the battle script's act), +0xB743 ForceKengekiAct, +0xB744 LastKengekiAct (sword-clash act).
  {22, "ai_think", "ptr2", {0x58, 0x340}, 0xB740, 0x10},
}

local f = assert(io.open(DIR .. "\\frames.bin", "wb"))
local idx = assert(io.open(DIR .. "\\index.txt", "w"))
idx:write("record <I4 d B B I2 + bytes; slot 0 = Wolf, 1-2 = nearest enemies\n")
for _, b in ipairs(BLOCKS) do idx:write(fmt("%d %s %s %s 0x%X 0x%X\n", b[1], b[2], b[3], type(b[4]) == "table" and fmt("0x%X>0x%X", b[4][1], b[4][2]) or (b[4] and fmt("0x%X", b[4]) or "-"), b[5], b[6])) end

local function valid(p) return p and p ~= 0 and isValidPointer(p) end
local function pos(chr)
  local bag = readPointer(chr + 0x1FF8)
  local ph = valid(bag) and readPointer(bag + 0x68)
  if not valid(ph) then return nil end
  return readFloat(ph + 0x80), readFloat(ph + 0x84), readFloat(ph + 0x88)
end

-- Enemy containers: WorldChrMan holds per-map chr sets 0x140 apart (+0x998 Ashina outskirts, +0xAD8/+0xC18/+0xD58
-- elsewhere); any pointer in WorldChrMan[0..0x2000] whose target holds EnemyIns pointers within 0x1000 bytes.
local containers, enemy_list, last_scan = {}, {}, -1e9
-- Full scan (slow, ~2-4 s): chr-set containers in WorldChrMan[0..0x2000] and every EnemyIns in them (0x4000 window).
local function find_containers()
  local wcm = readPointer(WCM_PTR)
  containers, enemy_list = {}, {}
  last_scan = clock()
  if not valid(wcm) then return end
  local seen = {}
  for off = 0, 0x2000, 8 do
    local p = readPointer(wcm + off)
    if valid(p) then
      local hit = false
      for o2 = 0, 0x4000, 8 do
        local q = readPointer(p + o2)
        if valid(q) and not seen[q] and readPointer(q) == ENEMY_VT then seen[q] = true; enemy_list[#enemy_list + 1] = q; hit = true end
      end
      if hit then containers[#containers + 1] = off end
    end
  end
  idx:write(fmt("scan: %d enemies, containers %s\n", #enemy_list, table.concat((function() local s = {} for i, o in ipairs(containers) do s[i] = fmt("0x%X", o) end return s end)(), " ")))
end

-- Slot 1 = the lock-on target (PlayerIns +0x11E4 handle, 0xFFFFFFFF = none; ChrIns +0x8 = handle),
-- slot 2 = the nearest other enemy; without a lock both are the nearest two. Uses the cached list.
local function nearest_enemies(player)
  local px, py, pz = pos(player)
  if not px then return {} end
  local lock = readInteger(player + 0x11E4) & 0xffffffff
  local found, target = {}, nil
  for _, e in ipairs(enemy_list) do
    if valid(e) and readPointer(e) == ENEMY_VT then
      local x, y, z = pos(e)
      if x then found[#found + 1] = {e, (x - px) ^ 2 + (y - py) ^ 2 + (z - pz) ^ 2} end
      if lock ~= 0xffffffff and (readInteger(e + 8) & 0xffffffff) == lock then target = e end
    end
  end
  if lock ~= 0xffffffff and not target and clock() - last_scan > 10000 then find_containers() end
  table.sort(found, function(a, b) return a[2] < b[2] end)
  local out = {}
  if target then out[1] = target end
  for _, fe in ipairs(found) do
    if #out >= 2 then break end
    if fe[1] ~= target then out[#out + 1] = fe[1] end
  end
  return out
end

local function write_block(tick, t, slot, b, chr)
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
  if not valid(base) then return end
  local bytes = readBytes(base + b[5], b[6])
  if not bytes or #bytes == 0 then return end
  f:write(string.pack("<I4dBBI2", tick, t, slot, b[1], #bytes), string.char(table.unpack(bytes)))
end

-- Optional posture hook (args.hook = true; CRASHED the game on its first call 2026-10-08 - keep off): FUN_1408439a0 calls -> hooks.txt (result = final posture damage).
local hookf, hid = nil, nil
if args.hook == true then
  pcall(destroyRingBuffer)
  createRingBuffer({entry_count = 512, max_data_size = 512})
  hookFunction(addr("0x1408439a0"), {name = "postureDmg", type = "post", buffer_arg = 1, length_arg = -1, max_capture = 512, stack_args = {5, 6}})
  for k, v in pairs(listHooks() or {}) do if v.name == "postureDmg" then hid = v.hook_id or k end end
  hookf = assert(io.open(DIR .. "\\hooks.txt", "w"))
end
local function drain_hooks(tick)
  if not hookf then return end
  for _, e in ipairs(readRingBuffer(200) or {}) do
    local ex = e.extra_args or {}
    hookf:write(fmt("tick=%d result=%s a3=%s def=%s atk=%s\n  %s\n", tick, tostring(e.result), toHex(e.arg3 or 0), toHex(ex[1] or 0), toHex(ex[2] or 0), e.data_hex or ""))
  end
end
local period = 1000 / HZ
local t0 = clock()
local tick, enemies, slow = 0, {}, 0
while clock() - t0 < SECONDS * 1000 do
  local t = clock()
  local wcm = readPointer(WCM_PTR)
  local player = valid(wcm) and readPointer(wcm + 0x88)
  if valid(player) then
    if tick % (HZ * 60) == 0 or #enemy_list == 0 and tick % (HZ * 5) == 0 then find_containers() end
    if tick % HZ == 0 then
      enemies = nearest_enemies(player)
      idx:write(fmt("tick %d enemies %s\n", tick, table.concat((function() local s = {} for i, e in ipairs(enemies) do s[i] = toHex(e) end return s end)(), " ")))
    end
    local chrs = {player, enemies[1], enemies[2]}
    for slot = 0, 2 do
      local c = chrs[slot + 1]
      if valid(c) then
        for _, b in ipairs(BLOCKS) do write_block(tick, t - t0, slot, b, c) end
      end
    end
  end
  if tick % 30 == 0 then drain_hooks(tick) end
  tick = tick + 1
  local spent = clock() - t
  if spent > period then slow = slow + 1 else sleep(math.floor(period - spent)) end
end
drain_hooks(tick)
if hookf then hookf:close(); if hid then unhookFunction(hid) end; pcall(destroyRingBuffer) end
f:close()
idx:write(fmt("done: %d ticks in %.1f s, %d slow ticks\n", tick, (clock() - t0) / 1000, slow))
idx:close()
print(fmt("recorded %d ticks (%d slow) to %s", tick, slow, DIR))
addResult("dir", DIR)
addResult("ticks", tick)
addResult("slow", slow)
