-- Warp Wolf to an idol and, optionally, to an exact spot after the load (boss arenas, test spots).
-- Source of truth: shinobi-combat/tools/memscope/warp.lua. Sekiro 1.06, base 0x140000000.
-- args: idol (idol id, e.g. 1101951 Outskirts Wall - Gate Path; ids and boss spots in SekiroTool's
--       Resources.resx "Warps", MIT), x, y, z, yaw (optional landing spot, radians),
--       nodeath (default true: debug flag player no death stays on so a test never costs a death).
-- The warp is the game's own idol travel, FUN_14068e870(0, idol, 0) (SekiroTool Functions.Warp);
-- the landing spot is written to the physics module after the new area is loaded.
local args = args or {}
local IDOL = assert(tonumber(args.idol), "args.idol missing")
local WARP = addr("0x14068E870")
local WORLD_CHR_MAN = addr("0x143D7A1E0")
local DEBUG_FLAGS = addr("0x143D7A366") -- +0 player no death, +0xB all no death, +0x10 disable AI

local function physics()
  local wcm = readPointer(WORLD_CHR_MAN)
  local p = wcm and readPointer(wcm + 0x88)
  local bag = p and readPointer(p + 0x1FF8)
  return bag and readPointer(bag + 0x68)
end

local function pos(ph)
  return readFloat(ph + 0x80), readFloat(ph + 0x84), readFloat(ph + 0x88)
end

if args.nodeath ~= false and args.nodeath ~= "false" then
  writeByte(DEBUG_FLAGS, 1)
end

local before = physics()
local bx, by, bz = pos(before)
allowUnsafeCodeExecution(true)
call(WARP, {0, IDOL, 0})

-- Loaded = a player physics module again whose position moved away from the old spot.
local t0, ph = clock(), nil
repeat
  sleep(250)
  ph = physics()
  local moved = false
  if ph then
    local x, y, z = pos(ph)
    moved = math.abs(x - bx) + math.abs(y - by) + math.abs(z - bz) > 0.5
  end
until (ph and moved) or clock() - t0 > 30000
assert(ph, "no player after 30 s: the warp did not finish loading")
sleep(1500) -- let the area settle before moving Wolf

if args.x then
  -- While Wolf plays the idol arrival the game puts him back each frame (seen 2026-11-03), so
  -- write, wait, and repeat until the spot holds (up to 20 s).
  local tx, ty, tz = tonumber(args.x), tonumber(args.y), tonumber(args.z)
  local held, t1 = false, clock()
  repeat
    for _ = 1, 10 do
      writeFloat(ph + 0x80, tx)
      writeFloat(ph + 0x84, ty)
      writeFloat(ph + 0x88, tz)
      if args.yaw then writeFloat(ph + 0x74, tonumber(args.yaw)) end
      sleep(16)
    end
    sleep(700)
    local x, y, z = pos(ph)
    held = math.abs(x - tx) + math.abs(z - tz) < 1.0 and math.abs(y - ty) < 3.0
  until held or clock() - t1 > 20000
  assert(held, "the landing spot did not hold after 20 s")
end

local x, y, z = pos(ph)
setResult({idol = IDOL, x = x, y = y, z = z, yaw = readFloat(ph + 0x74), seconds = (clock() - t0) / 1000})
