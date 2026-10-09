-- Guard posture check: hooks FUN_1408439a0 (defender posture damage, returns int) for N seconds and logs each call
-- args: seconds (default 60). Output: memscope-data/logs/posture_hook_<time>.txt
-- Per call: result (final posture damage), arg0 = attack data struct (first 0x200 bytes captured:
-- +0x34/+0x38 base stamina damage, +0x188 rate, +0x2c attribute, +0x1d0/+0x1d4), arg3 = guarded flag,
-- stack args 5/6 = defender chr / attacker chr. Compare result with base x 3.75 to isolate the x0.74 term.
local args = args or {}
local SECONDS = tonumber(args.seconds) or 60
local path = "C:\\DeadlockModding\\memscope-data\\logs\\posture_hook_" .. os.date("%Y%m%d_%H%M%S") .. ".txt"
local f = assert(io.open(path, "w"))
pcall(destroyRingBuffer)
createRingBuffer({entry_count = 512, max_data_size = 512})
local h = hookFunction(addr("0x1408439a0"), {name = "postureDmg", type = "post", buffer_arg = 1, length_arg = -1, max_capture = 512, stack_args = {5, 6}})
local hid = 1
for k, v in pairs(listHooks() or {}) do if v.name == "postureDmg" then hid = v.hook_id or k end end
local t0, n = clock(), 0
local function drain()
  for _, e in ipairs(readRingBuffer(200) or {}) do
    n = n + 1
    local ex = e.extra_args or {}
    f:write(fmt("t=%.3f result=%s a0=%s a1=%s a2=%s a3=%s def=%s atk=%s\n", (clock() - t0) / 1000, tostring(e.result),
      toHex(e.arg0 or 0), toHex(e.arg1 or 0), toHex(e.arg2 or 0), toHex(e.arg3 or 0), toHex(ex[1] or 0), toHex(ex[2] or 0)))
    f:write("  struct " .. (e.data_hex or "") .. "\n")
  end
end
while clock() - t0 < SECONDS * 1000 do
  sleep(250)
  drain()
end
drain()
unhookFunction(hid)
pcall(destroyRingBuffer)
f:close()
print(fmt("%d calls logged to %s", n, path))
addResult("calls", n)
addResult("path", path)
