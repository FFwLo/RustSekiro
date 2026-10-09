-- Sv1 AI runtime.
--
-- Runs FromSoftware's own (decompiled) enemy AI scripts, loaded at runtime from the
-- user's extracted/ai_src, on a small reimplementation of the engine side:
--   * the goal tree (Activate / Update / Terminate / Interupt, life, subgoal queues),
--   * the native goals the scripts bottom out in (CommonAttack, MoveToSomewhere,
--     SidewayMove, LeaveTarget, SpinStep, Wait, ...),
--   * the `ai` object the scripts query (distances, SpEffects, timers, numbers, ...).
--
-- Rust fills W (world snapshot) before each AI_Tick and reads CMD (one action
-- request) afterwards. Engine rule for requests (from the NPC TAEs): an action can
-- start when the current anim is free or has its AI cancel flag active:
--   23 combo attack (goals with ENABLE_COMBO_ATK_CANCEL), 86 attack, 79 step, 78 move.

W = {}
CMD = {}
AI_LOG = {}

local SUCCESS, CONTINUE, FAILED = 1, 0, -1
local DEFS = {}      -- goal id -> { flags = {}, tbl = table goal | nil, fn = {activate,...} | nil }
local NATIVE = {}    -- goal id -> { activate, update, terminate }
local WARNED = {}
local ROOT = nil
local TOP_QUEUE = {} -- AddTopGoal requests
local REPLAN = false
local INTERRUPT = nil -- { kind, sp }
local AI = {}

local function log(msg)
    AI_LOG[#AI_LOG + 1] = msg
end

local function warn_once(key)
    if not WARNED[key] then
        WARNED[key] = true
        log("stub: " .. key)
    end
end

local NIL_FLAGS = {}
local function flags(id)
    if id == nil then return NIL_FLAGS end
    DEFS[id] = DEFS[id] or { flags = {} }
    return DEFS[id].flags
end

-- Registration functions called at the top level of the scripts.
function RegisterTableGoal(id, name)
    Goal = { __id = id, __name = name }
    flags(id)
    DEFS[id].tbl = Goal
end
function RegisterTableLogic(id)
    Logic = { __id = id }
end
-- Engine hooks used by the game's own common_table_ai_common.lua, which replaces the
-- two functions above and keeps its tables in g_GoalTable / g_LogicTable.
function REGISTER_GOAL(id, name)
    flags(id)
    DEFS[id].name = name
end
function REGISTER_LOGIC_FUNC() end
function REGISTER_GOAL_NO_UPDATE(id, v) flags(id).no_update = v end
function REGISTER_GOAL_NO_INTERUPT(id, v) flags(id).no_interrupt = v end
function REGISTER_GOAL_NO_SUB_GOAL(id, v) flags(id).no_sub = v end
function REGISTER_GOAL_UPDATE_TIME(id, a, b) end
function REGISTER_DBG_GOAL_PARAM() end
function ENABLE_COMBO_ATK_CANCEL(id) flags(id).combo = true end
function REGISTER_GOAL_USE_AVOID_CHR() end

-- Function-style goals: GOAL_COMMON_X = id with X_Activate / X_Update / X_Terminate / X_Interupt.
function AI_BindFunctionGoals()
    for id, tbl in pairs(rawget(_G, "g_GoalTable") or {}) do
        flags(id)
        DEFS[id].tbl = tbl
        tbl.__name = DEFS[id].name
    end
    for name, id in pairs(_G) do
        if type(name) == "string" and type(id) == "number" and name:sub(1, 5) == "GOAL_" then
            local short = name:gsub("^GOAL_COMMON_", ""):gsub("^GOAL_", "")
            local act = rawget(_G, short .. "_Activate")
            if act ~= nil or NATIVE[id] ~= nil then
                flags(id)
                DEFS[id].name = DEFS[id].name or name
            end
            if act ~= nil then
                DEFS[id].fn = {
                    activate = act,
                    update = rawget(_G, short .. "_Update"),
                    terminate = rawget(_G, short .. "_Terminate"),
                    interrupt = rawget(_G, short .. "_Interupt"),
                }
            end
        end
    end
end

---------------------------------------------------------------------------
-- Goal objects
---------------------------------------------------------------------------
local G = {}
G.__index = G
setmetatable(G, { __index = function(_, k)
    return function()
        warn_once("goal:" .. k)
        return 0
    end
end })

local function new_goal(id, life, ...)
    return setmetatable({
        id = id, life = life or -1, p = table.pack(...), subs = {}, started = false,
        t = 0, had_sub = false, num = {}, timing = {}, parent_combo = false, st = {},
    }, G)
end

function G:GetParam(i)
    -- Unset goal params read as 0, like the engine (e.g. EndureAttack's 5th param).
    local v = self.p[i + 1]
    if v == nil then return 0 end
    return v
end
function G:GetLife() return self.life end
function G:GetLifeRemain() return self.life - self.t end
function G:GetSubGoalNum() return #self.subs end
function G:SetNumber(i, v) self.num[i] = v end
function G:GetNumber(i) return self.num[i] or 0 end
function G:AddGoalScopedTeamRecord() end
function G:SetFailedEndOption() end
function G:SetLifeEndSuccess() self.life_success = true end
function G:SetManagementGoal() end
function G:SetTargetRange() return self end

local function child(self, id, life, ...)
    local g = new_goal(id, life, ...)
    local def = DEFS[self.id]
    g.parent_combo = def ~= nil and def.flags.combo == true
    self.had_sub = true
    return g
end

function G:AddSubGoal(id, life, ...)
    local g = child(self, id, life, ...)
    self.subs[#self.subs + 1] = g
    return g
end
function G:AddSubGoal_Front(id, life, ...)
    local g = child(self, id, life, ...)
    table.insert(self.subs, 1, g)
    return g
end

-- TimingSetTimer(timer, time, when) / TimingSetNumber(slot, value, when): applied on
-- activation (AI_TIMING_SET__ACTIVATE) or on success (UPDATE_SUCCESS).
function G:TimingSetTimer(id, time, when)
    self.timing[#self.timing + 1] = { "timer", id, time, when }
    return self
end
function G:TimingSetNumber(id, v, when)
    self.timing[#self.timing + 1] = { "number", id, v, when }
    return self
end

local function apply_timing(g, when)
    for _, t in ipairs(g.timing) do
        -- The scripts pass an undefined UPDATE_SUCCESS (nil) for "on success".
        if (t[4] or AI_TIMING_SET__UPDATE_SUCCESS or 1) == when then
            if t[1] == "timer" then AI:SetTimer(t[2], t[3]) else AI:SetNumber(t[2], t[3]) end
        end
    end
end

local function call_def(def, what, g, ...)
    if def.tbl ~= nil then
        local f = rawget(def.tbl, what)
        if f ~= nil then return f(def.tbl, AI, g, ...) end
    elseif def.fn ~= nil then
        local f = def.fn[what:lower()]
        if what == "Interrupt" then f = def.fn.interrupt end
        if f ~= nil then return f(AI, g, ...) end
    end
    return nil
end

local terminate

function G:ClearSubGoal()
    for _, s in ipairs(self.subs) do terminate(s) end
    self.subs = {}
end

terminate = function(g)
    if not g.started or g.terminated then return end
    g.terminated = true
    for _, s in ipairs(g.subs) do terminate(s) end
    local nat = NATIVE[g.id]
    if nat ~= nil then
        if nat.terminate then nat.terminate(g) end
    else
        local def = DEFS[g.id]
        if def ~= nil then call_def(def, "Terminate", g) end
    end
end

local function tick(g, dt)
    local nat = NATIVE[g.id]
    local def = DEFS[g.id]
    if not g.started then
        g.started = true
        apply_timing(g, AI_TIMING_SET__ACTIVATE or 0)
        if nat ~= nil then
            nat.activate(g)
        elseif def ~= nil then
            call_def(def, "Activate", g)
        else
            warn_once("goal id " .. tostring(g.id))
            return FAILED
        end
    end
    g.t = g.t + dt
    if #g.subs > 0 then
        local s = g.subs[1]
        local r = tick(s, dt)
        if r == SUCCESS then
            apply_timing(s, AI_TIMING_SET__UPDATE_SUCCESS or 1)
            terminate(s)
            table.remove(g.subs, 1)
        elseif r == FAILED then
            terminate(s)
            g:ClearSubGoal()
            return FAILED
        end
        if #g.subs > 0 then return CONTINUE end
    end
    if g.life >= 0 and g.t >= g.life then
        -- Life over: actions that already started run to their own end; an action
        -- that never got to start fails (dropping the rest of its combo).
        if nat == nil or not nat.ignore_life then return SUCCESS end
        if not g.busy then return FAILED end
    end
    if nat ~= nil then return nat.update(g, dt) end
    if def == nil then return FAILED end
    if def.flags.no_update then
        return (#g.subs == 0) and SUCCESS or CONTINUE
    end
    local r = call_def(def, "Update", g, dt) or CONTINUE
    if r == CONTINUE and g.had_sub and #g.subs == 0 then return SUCCESS end
    return r
end

local function active_ids(g, out)
    out[g.id] = true
    for _, s in ipairs(g.subs) do active_ids(s, out) end
    return out
end

---------------------------------------------------------------------------
-- World helpers
---------------------------------------------------------------------------
local function can_act(flag)
    return W.free or (W.flags ~= nil and W.flags[flag] == true)
end

local function dist() return W.dist or 99 end

-- Signed angle (deg) from self's facing to the target, positive to the right.
local function to_target_angle() return W.angle or 0 end

local function dir_center(dir_type)
    if dir_type == AI_DIR_TYPE_B then return 180
    elseif dir_type == AI_DIR_TYPE_L then return -90
    elseif dir_type == AI_DIR_TYPE_R then return 90 end
    return 0
end

local function angle_in(center, width, a)
    local d = (a - center + 540) % 360 - 180
    return math.abs(d) <= width / 2
end

---------------------------------------------------------------------------
-- Native goals
---------------------------------------------------------------------------
local function attack_native(param_ofs)
    return {
        ignore_life = true,
        activate = function(g)
            g.ez = g:GetParam(param_ofs)
            g.succ = g:GetParam(param_ofs + 2) or 9999
            g.turn_time = g:GetParam(param_ofs + 4) or 0
            g.front = g:GetParam(param_ofs + 5) or 90
            g.phase = "wait"
            g.busy = false
        end,
        update = function(g, dt)
            if g.phase == "wait" then
                local flag = g.parent_combo and 23 or 86
                if not can_act(flag) then return CONTINUE end
                if W.free and g.turn_time > 0 and g.t < g.turn_time and not angle_in(0, g.front, to_target_angle()) then
                    CMD.turn = true
                    return CONTINUE
                end
                CMD.anim = g.ez
                g.phase = "requested"
                g.busy = true
                return CONTINUE
            elseif g.phase == "requested" then
                if W.ez == g.ez then
                    g.phase = "run"
                elseif W.ez_failed == g.ez then
                    return FAILED
                else
                    CMD.anim = g.ez
                end
                return CONTINUE
            end
            if W.ez ~= g.ez then return FAILED end
            if W.t > 0.1 and W.flags[23] then
                if dist() <= g.succ then return SUCCESS end
            end
            if W.anim_done then
                return (dist() <= g.succ) and SUCCESS or FAILED
            end
            return CONTINUE
        end,
    }
end

NATIVE[GOAL_COMMON_CommonAttack or 2200] = attack_native(0)

-- SpinStep(ezState, target, ?, dirType, dist): a step anim, AI step flag 79.
NATIVE[GOAL_COMMON_SpinStep or 2020] = {
    ignore_life = true,
    activate = function(g) g.ez = g:GetParam(0); g.phase = "wait"; g.busy = false end,
    update = function(g, dt)
        if g.phase == "wait" then
            if not can_act(79) then return CONTINUE end
            CMD.anim = g.ez
            g.phase = "requested"
            g.busy = true
            return CONTINUE
        elseif g.phase == "requested" then
            if W.ez == g.ez then g.phase = "run" elseif W.ez_failed == g.ez then return FAILED else CMD.anim = g.ez end
            return CONTINUE
        end
        if W.ez ~= g.ez then return FAILED end
        if W.anim_done or (W.t > 0.1 and (W.flags[86] or W.flags[78])) then return SUCCESS end
        return CONTINUE
    end,
}

local function move_goal(kind)
    return {
        activate = function(g)
            g.start_bearing = W.bearing or 0
        end,
        update = function(g, dt)
            local p = g.p
            if kind == "approach" then
                -- MoveToSomewhere(target, dirType, dist, turnTarget, walk, ...)
                if dist() <= (p[3] or 0) then return SUCCESS end
                if not can_act(78) then return CONTINUE end
                CMD.move = "F"
                CMD.walk = p[5] == true
            elseif kind == "leave" then
                -- LeaveTarget(target, dist, turnTarget, walk, guard)
                if dist() >= (p[2] or 0) then return SUCCESS end
                if not can_act(78) then return CONTINUE end
                CMD.move = "B"
                CMD.walk = true
            elseif kind == "sideway" then
                -- SidewayMove(target, right(1)/left(0), angle, turnTarget, walk, guard)
                local moved = math.abs(((W.bearing or 0) - g.start_bearing + 540) % 360 - 180)
                if moved >= (p[3] or 45) then return SUCCESS end
                if not can_act(78) then return CONTINUE end
                CMD.move = (p[2] == 1) and "R" or "L"
                CMD.walk = true
            elseif kind == "keep" then
                -- KeepDist(target, min, max, ...)
                local d = dist()
                if d >= (p[2] or 0) and d <= (p[3] or 99) then return SUCCESS end
                if not can_act(78) then return CONTINUE end
                CMD.move = (d < (p[2] or 0)) and "B" or "F"
                CMD.walk = true
            end
            CMD.face = true
            return CONTINUE
        end,
    }
end

NATIVE[GOAL_COMMON_MoveToSomewhere or 2019] = move_goal("approach")
NATIVE[GOAL_COMMON_LeaveTarget or 2016] = move_goal("leave")
NATIVE[GOAL_COMMON_SidewayMove or 2017] = move_goal("sideway")
NATIVE[GOAL_COMMON_KeepDist or 2018] = move_goal("keep")

-- Wait(life, target, ...): stand, facing the target when it is the enemy.
NATIVE[GOAL_COMMON_Wait or 2000] = {
    activate = function(g) end,
    update = function(g, dt)
        if g:GetParam(0) == TARGET_ENE_0 and W.free then CMD.turn = true end
        return CONTINUE
    end,
}

-- Guard(life, ezState, target, ...): hold the guard anim for the goal's life.
NATIVE[GOAL_COMMON_Guard or 2101] = {
    activate = function(g) g.ez = g:GetParam(0); g.req = false end,
    update = function(g, dt)
        if not g.req and g.ez ~= nil and g.ez > 0 and can_act(86) then
            CMD.anim = g.ez
            g.req = true
        end
        return CONTINUE
    end,
}

---------------------------------------------------------------------------
-- The ai object
---------------------------------------------------------------------------
setmetatable(AI, { __index = function(_, k)
    return function()
        warn_once("ai:" .. k)
        return 0
    end
end })

local TIMERS, NUMBERS, SNUM, ID_TIMERS, ATK_PASSED = {}, {}, {}, {}, {}
local OBSERVED = {}

local function sp_of(target)
    if target == TARGET_SELF then return W.sp_self or {} end
    return W.sp_target or {}
end

function AI:GetDist(t) if t == TARGET_SELF then return 0 end return dist() end
function AI:GetDistYSigned(t) return 0 end
function AI:GetDistAtoB() return dist() end
function AI:GetMapHitRadius(t) return W.hit_radius or 0.5 end
function AI:GetRandam_Int(a, b)
    a, b = math.floor(a), math.floor(b)
    -- Empty range (all act weights 0): return the minimum, so no act is picked.
    if b < a then return a end
    return math.random(a, b)
end
function AI:GetRandam_Float(a, b) return a + (b - a) * math.random() end
function AI:HasSpecialEffectId(t, id) return sp_of(t)[id] == true end
function AI:HasSpecialEffectAttribute(t, a) return false end
function AI:IsTargetGuard(t) return W.target_guard == true end
function AI:GetSp(t) return W.sp or 0 end
function AI:GetSpRate(t) return W.sp_rate or 1 end
function AI:GetHpRate(t) if t == TARGET_SELF then return W.hp_rate or 1 end return W.target_hp_rate or 1 end
function AI:GetHp(t) return W.hp or 1 end
function AI:GetNinsatsuNum() return W.ninsatsu or 1 end
function AI:GetNpcThinkParamID() return W.think_id or 0 end
function AI:GetExcelParam(k) return 0 end
function AI:GetTeamOrder(k) return 0 end
function AI:GetEventRequest() return 0 end
function AI:GetToTargetAngle(t) return to_target_angle() end
function AI:IsInsideTarget(t, dir_type, angle)
    return angle_in(dir_center(dir_type), angle, to_target_angle())
end
function AI:IsInsideTargetEx(t, t2, dir_type, angle, d)
    return angle_in(dir_center(dir_type), angle, to_target_angle()) and dist() <= d
end
function AI:IsInsideTargetRegion() return false end
function AI:IsVisibleCurrTarget() return true end
function AI:IsVisibleTarget() return true end
function AI:IsExistMeshOnLine() return true end
function AI:GetExistMeshOnLineDistSpecifyAngleEx(t, angle, d) return d end
function AI:GetExistMeshOnLineDistSpecifyAngle(t, angle, d) return d end
function AI:CheckDoesExistPathWithSetPoint() return true end
function AI:IsLadderAct() return false end
function AI:IsBattleState() return true end
function AI:IsFindState() return false end
function AI:IsCautionState() return false end
function AI:IsSearchTarget() return true end
function AI:GetCurrTargetType() return AI_TARGET_TYPE__NORMAL_ENEMY or 3 end
function AI:IsLookToTarget() return angle_in(0, 20, to_target_angle()) end
function AI:TurnTo(t) CMD.turn = true end
function AI:RequestEmergencyQuickTurn() end
function AI:SetAIPredictionMoveTargetSpecifyTargetDir() end
function AI:PrintText() end
function AI:DbgGetForceActIdx() return 0 end
function AI:DbgGetForceKengekiActIdx() return 0 end
function AI:DbgSetLastActIdx(i) W.last_act = i end
function AI:DbgSetLastKengekiActIdx(i) W.last_kengeki = i end
function AI:GetOddsParam() return 0 end
function AI:GetOddsParamIdOffset() return 0 end
function AI:GetEzStateAnimId(ez) return ez end
function AI:DoEzAction(t, ez) CMD.anim = ez end

function AI:SetTimer(i, t) TIMERS[i] = t end
function AI:GetTimer(i) return TIMERS[i] or 0 end
function AI:IsFinishTimer(i) return (TIMERS[i] or 0) <= 0 end
function AI:StartIdTimer(i) ID_TIMERS[i] = 0 end
function AI:GetIdTimer(i) return ID_TIMERS[i] or 9999 end
function AI:SetNumber(i, v) NUMBERS[i] = v end
function AI:GetNumber(i) return NUMBERS[i] or 0 end
function AI:SetStringIndexedNumber(k, v) SNUM[k] = v end
function AI:GetStringIndexedNumber(k) return SNUM[k] or 0 end
function AI:StartAttackPassedTimer(ez) ATK_PASSED[ez] = 0 end
function AI:GetAttackPassedTime(ez) return ATK_PASSED[ez] or 9999 end
function AI:IsFinishAttackCoolTime() return true end
function AI:IsStartAttack() return false end

function AI:AddObserveSpecialEffectAttribute(t, id) OBSERVED[id] = t end
function AI:DeleteObserve(slot) end
function AI:AddObserveArea() end
function AI:AddObserveRegion() end
function AI:AddObserveChrDmyArea() end
function AI:GetAreaObserveSlot() return false end
function AI:IsInsideObserve() return false end

function AI:IsInterupt(kind) return INTERRUPT ~= nil and INTERRUPT.kind == kind end
function AI:GetSpecialEffectActivateInterruptType(i) return INTERRUPT ~= nil and INTERRUPT.sp or 0 end
function AI:Replanning() REPLAN = true end
function AI:AddTopGoal(id, life, ...)
    local g = new_goal(id, life, ...)
    TOP_QUEUE[#TOP_QUEUE + 1] = g
    return g
end
function AI:GetTopGoal() return ROOT end
function AI:IsActiveGoal(id) return ROOT ~= nil and active_ids(ROOT, {})[id] == true end
function AI:HasGoal(id) return AI:IsActiveGoal(id) end

---------------------------------------------------------------------------
-- Tick
---------------------------------------------------------------------------
local function reset_root()
    if ROOT ~= nil then terminate(ROOT) end
    ROOT = nil
end

local function interrupt_goals(kind, sp)
    if ROOT == nil then
        ROOT = new_goal(W.battle_goal, -1)
    end
    INTERRUPT = { kind = kind, sp = sp }
    local handled = false
    local def = DEFS[ROOT.id]
    if def ~= nil and not def.flags.no_interrupt then
        handled = call_def(def, "Interrupt", ROOT, ROOT) == true
    end
    INTERRUPT = nil
    -- A handled interrupt replaces the plan: a fresh root does not Activate on top of it.
    if handled then ROOT.started = true end
    return handled
end

-- Called by Rust every fixed frame.
function AI_Tick(dt)
    CMD = {}
    for k, v in pairs(TIMERS) do TIMERS[k] = v - dt end
    for k, v in pairs(ID_TIMERS) do ID_TIMERS[k] = v + dt end
    for k, v in pairs(ATK_PASSED) do ATK_PASSED[k] = v + dt end
    if W.ez_started ~= nil then
        AI:StartAttackPassedTimer(W.ez_started)
    end
    -- An anim the AI did not request replaced its action (reaction, guard, deflect):
    -- the plan is dropped and rebuilt, which is where Kengeki_Activate sees the clash.
    if W.interrupted then
        reset_root()
    end
    -- The player's attack notify (TAE flag 63): Goal.Interrupt -> Goal.Parry.
    if W.parry_timing then
        interrupt_goals(INTERUPT_ParryTiming or 24, 0)
    end
    if W.use_item then
        interrupt_goals(INTERUPT_UseItem or 12, 0)
    end
    for _, id in ipairs(W.sp_new or {}) do
        if OBSERVED[id] ~= nil then
            interrupt_goals(INTERUPT_ActivateSpecialEffect or 43, id)
        end
    end
    if REPLAN then
        REPLAN = false
        reset_root()
    end
    if ROOT == nil then
        TOP_QUEUE = {}
        ROOT = new_goal(W.battle_goal, -1)
    end
    local r = tick(ROOT, dt)
    if r ~= CONTINUE then
        reset_root()
    end
    if REPLAN then
        REPLAN = false
        reset_root()
    end
end

function AI_Describe()
    local parts = {}
    local g = ROOT
    while g ~= nil do
        local def = DEFS[g.id]
        local name = (def and (def.name or (def.tbl and rawget(def.tbl, "__name")))) or tostring(g.id)
        name = name:gsub("^GOAL_COMMON_", ""):gsub("^GOAL_", "")
        local ez = rawget(g, "ez")
        parts[#parts + 1] = name .. (ez and ("(" .. tostring(ez) .. ")") or "")
        g = rawget(g, "subs")[1]
    end
    return table.concat(parts, " > ")
end
