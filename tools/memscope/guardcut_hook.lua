-- Guard cut check v2: PRE hook on FUN_140840870 (guard posture cut formula) - pre hooks jump (no misaligned call
-- like the post hook on FUN_1408439a0 that crashed). Logs integer args rcx/rdx/r8/r9 + stack args 5-8 per call.
-- args: seconds (default 60). Output: memscope-data/logs/guardcut_<time>.txt
local args = args or {}
local SECONDS = tonumber(args.seconds) or 60
local path = "C:\\DeadlockModding\\memscope-data\\logs\\guardcut_" .. os.date("%Y%m%d_%H%M%S") .. ".txt"
local f = assert(io.open(path, "w"))
pcall(destroyRingBuffer)
createRingBuffer({entry_count = 512, max_data_size = 64})
hookFunction(addr("0x140840870"), {name = "guardCut", type = "pre", buffer_arg = -1, length_arg = -1, max_capture = 0, stack_args = {5, 6, 7, 8}})
local hid = nil
for k, v in pairs(listHooks() or {}) do if v.name == "guardCut" then hid = v.hook_id or k end end
local t0, n = clock(), 0
local function drain()
  for _, e in ipairs(readRingBuffer(200) or {}) do
    n = n + 1
    local ex = e.extra_args or {}
    f:write(fmt("t=%.3f rcx=%s rdx=%s r8=%s r9=%s s5=%s s6=%s s7=%s s8=%s ret=%s\n", (clock() - t0) / 1000,
      toHex(e.arg0 or 0), toHex(e.arg1 or 0), toHex(e.arg2 or 0), toHex(e.arg3 or 0),
      toHex(ex[1] or 0), toHex(ex[2] or 0), toHex(ex[3] or 0), toHex(ex[4] or 0), toHex(e.return_addr or 0)))
  end
  f:flush()
end
local ok, err = pcall(function()
  while clock() - t0 < SECONDS * 1000 do
    sleep(250)
    drain()
  end
end)
drain()
if hid then unhookFunction(hid) end
pcall(destroyRingBuffer)
f:close()
print(fmt("%d calls logged to %s %s", n, path, ok and "" or tostring(err)))
addResult("calls", n)
addResult("path", path)
