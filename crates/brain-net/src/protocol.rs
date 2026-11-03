use sekiro_sim::brain::actions::PlanError;
use sekiro_sim::brain::coach::{CHECKPOINTS, GAME_PLAN_VERSION, KEEP};
use sekiro_sim::brain::{BossActions, GamePlan, Plan, SetOffer, Weighted};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Backend, Failure, HostedConfig};

/// The one question's name in both backends.
pub(crate) const QUESTION: &str = "boss_action";

pub(crate) fn instructions(actions: &BossActions) -> String {
    let mut text = format!(
        "You are {} fighting Wolf. Reward order: win and win faster (time_to_kill_wolf, lower is better); then break Wolf posture and land a hit right after (wolf_posture_breaks, hits_after_break); then damage to Wolf per minute (HP plus posture); then take little damage (boss_damage_taken_per_min, HP plus posture) and reduce Wolf perfect deflects, each of which builds your posture. engaged_time is information and only a small tiebreaker when losing while landing hits; survival time is never a goal. Stalling, running away or passivity counts as failure: a fight where no attack of yours started in the last 4 s more than 35% of the time failed, whatever else happened, plans without an attack are ignored, you must start an attack at least every 6 s and may back off or retreat only once per 5 s. Retreat chains are allowed as baits when followed by an attack, including turning to punish a chase. Movement tactics exist to set up attacks (baits, spacing, timing mixups), not to avoid fighting. Think like a swordsman: turning your back or retreating while the opponent is in sword range leaves you open, so tactics should set up attacks safely. Probe instead of repeating: try untried options, drop what keeps being deflected or guarded, bait his habits (stand still briefly, walk in slowly, strafe, mix timings) and punish them. Input: obs is a fair view of both fighters (plan is what runs now); tally counts this fight per action or tactic as [n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other]; memory holds every fight so far (the same counts, Wolf's habits, and trend: his deflect%, guard%, hits you landed, hp% lost, attacks per minute, damage per minute and passive percent, and kill seconds for information only, over the last fights, now = this fight so far); why is what triggered this question; break contexts include up to four viable_punishes rows [id,name,first seconds,seconds left,hits,window HP/posture,total HP/posture,stun_lock,guard posture after,recovery,evidence,conversion rate], with the same facts as the coach. You are asked only a few times a fight. The boss samples from your probabilities all fight, so spread them over several good options. Reflexes (guarding, deflecting, counters and reactions) are automatic. Choose lua to leave an act to your own AI. Options are single actions, movement tactics (distances from Wolf, left and right are yours) and plans of up to {} steps joined with > (consecutive attacks joined with +):",
        actions.name, actions.max_plan
    );
    for choice in actions.choices() {
        text.push_str(&format!("\n{}: {}", choice.value, choice.label));
    }
    text
}

/// Schemas verified against official documentation on 2026-10-10:
/// https://developers.openai.com/api/reference/resources/decisions/methods/create
/// https://developers.openai.com/api/docs/guides/decisions
/// https://developers.openai.com/api/reference/resources/responses/methods/create
/// Decisions takes text input and 2..=255 unique choices in one named question.
/// Its answers are typed (choice or refusal), with model and usage at the top level; a choice
/// answer carries `probabilities`, one `{value, probability}` per choice.
/// `excluded` plan texts are left out of the choices of a Responses request (it has one answer
/// and no probabilities to sample); a Decisions request offers every choice and the driver
/// penalises the repelled ones when sampling.
pub(crate) fn body(
    config: &HostedConfig,
    actions: &BossActions,
    input: &str,
    excluded: &[String],
) -> Value {
    let skip =
        |value: &str| config.backend == Backend::Responses && excluded.iter().any(|e| e == value);
    let mut choices: Vec<Value> = actions
        .choices()
        .into_iter()
        .filter(|c| !skip(&c.value))
        .map(|c| json!({"value": c.value, "description": c.label}))
        .collect();
    choices.push(json!({"value": "lua", "description": "leave it to his own AI"}));
    let instructions = instructions(actions);
    match config.backend {
        Backend::Decisions | Backend::Coach => json!({
            "model": config.model, "input": input,
            "questions": [{"type": "choice", "name": QUESTION,
                "instructions": instructions, "choices": choices}]
        }),
        Backend::Responses => {
            let values: Vec<_> = choices.iter().map(|c| c["value"].clone()).collect();
            json!({
                "model": config.model, "reasoning": {"effort": "none"},
                "instructions": instructions, "input": [{"role": "user", "content": input}],
                "text": {"format": {"type": "json_schema", "name": QUESTION, "strict": true,
                    "schema": {"type": "object", "properties": {"action": {"type": "string", "enum": values}},
                        "required": ["action"], "additionalProperties": false}}},
                "max_output_tokens": 16, "store": false
            })
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionAnswer {
    action: String,
}

pub(crate) fn choice(backend: Backend, response: &Value) -> Result<String, Failure> {
    match backend {
        Backend::Decisions | Backend::Coach => {
            let answers = response["answers"]
                .as_array()
                .ok_or(Failure::InvalidAnswer)?;
            if answers.len() != 1 || answers[0]["name"] != QUESTION {
                return Err(Failure::InvalidAnswer);
            }
            let answer = &answers[0];
            match answer["type"].as_str() {
                Some("refusal") => Err(Failure::Refusal),
                Some("choice") => answer["choice"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(Failure::InvalidAnswer),
                _ => Err(Failure::InvalidAnswer),
            }
        }
        Backend::Responses => {
            let output = response["output"]
                .as_array()
                .ok_or(Failure::InvalidAnswer)?;
            let mut texts = Vec::new();
            for item in output {
                if item["type"] != "message" || item["role"] != "assistant" {
                    continue;
                }
                if let Some(content) = item["content"].as_array() {
                    for part in content {
                        if part["type"] == "refusal" {
                            return Err(Failure::Refusal);
                        }
                        if part["type"] == "output_text" {
                            texts.push(part["text"].as_str().ok_or(Failure::InvalidAnswer)?);
                        }
                    }
                }
            }
            if response["status"] != "completed" {
                return Err(Failure::Incomplete);
            }
            if texts.len() != 1 {
                return Err(Failure::InvalidAnswer);
            }
            serde_json::from_str::<ActionAnswer>(texts[0])
                .map(|answer| answer.action)
                .map_err(|_| Failure::InvalidAnswer)
        }
    }
}

pub(crate) fn plan(value: &str, actions: &BossActions) -> Result<Option<Plan>, Failure> {
    if value == "lua" {
        return Ok(None);
    }
    match actions.parse(value) {
        Ok(plan) => Ok(Some(plan)),
        Err(PlanError::Empty | PlanError::TooLong(_)) => Err(Failure::InvalidPlan),
        Err(PlanError::Unknown(_) | PlanError::UnknownToken(_)) => Err(Failure::UnknownAction),
    }
}

/// The plan set behind a Decisions answer: its `probabilities`, each value read as a plan
/// (`lua` leaves the act to the Lua AI). Values that are not a valid plan are skipped; an
/// answer without probabilities gives an empty set (the chosen plan stands alone).
pub(crate) fn plan_set(backend: Backend, response: &Value, actions: &BossActions) -> Vec<Weighted> {
    if backend != Backend::Decisions {
        return Vec::new();
    }
    let Some(list) = response["answers"][0]["probabilities"].as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let weight = entry["probability"].as_f64()? as f32;
            let plan = plan(entry["value"].as_str()?, actions).ok()?;
            (weight.is_finite() && weight > 0.0).then_some(Weighted { plan, weight })
        })
        .collect()
}

// ---- the coach ----

/// The coach's structured output name.
pub(crate) const GAME_PLAN: &str = "game_plan";
/// The checkpoint chooser's question name.
pub(crate) const SET_QUESTION: &str = "plan_set";

/// What the coach is told: its job, the input, the game plan it writes and every choice it may
/// use, with labels.
pub(crate) fn coach_instructions(actions: &BossActions) -> String {
    let mut text = format!(
        "You are the coach of {name}, who fights the player (Wolf) again and again. After each fight you write the game plan for the next one. Reward order: win and win faster (time_to_kill_wolf, lower is better); then break Wolf posture and land a hit right after (wolf_posture_breaks, hits_after_break); then damage to Wolf per minute (HP plus posture); then take little damage (boss_damage_taken_per_min, HP plus posture) and reduce Wolf perfect deflects, each of which builds your posture. engaged_time is information and only a small tiebreaker when losing while landing hits; survival time is never a goal. Stalling, running away or passivity counts as failure: passive = the percent of the fight with no attack of yours started in the last 4 s, and a fight with passive above 35 failed regardless of every other number; if the last fight failed this way, make the next plan attack more. Retreat chains are allowed as baits when followed by an attack, including turning to punish a chase. Movement tactics exist to set up attacks (baits, spacing, timing mixups such as a hold or a slow walk-in before a strike), not to avoid fighting. Think like a swordsman: turning your back or retreating while the opponent is in sword range leaves you open, so tactics should set up attacks safely. The fighter enforces limits you cannot override: a plan in a set must contain an attack (plans without one are dropped), an attack must start at least every 6 s, and back_off, retreat or back-step plans run at most once per 5 s. Players learn patterns, so probe with untried tactics, exploit the habits you see and retire what keeps failing. Test one idea at a time and write down what you learn.\n\
Attack learning: attack_rates contains [samples, recent success share, severity-weighted failure share] after at least three uses. Mikiri, jump counters and other counters, deflects, fully guarded attacks, whiffs (including a dodged sweep), and HP/posture damage taken during an attack are failure evidence. Clean hits, actual HP/posture progress and breaks are success evidence; mixed outcomes retain both. Recent selected-player evidence dominates pooled evidence. Do not judge a move from one use or wait for perfect 10/10 failure: compare rates after a few uses and decide whether to reduce its weight, change its setup or explicitly avoid it. The driver compares per-context Thompson posteriors with exploration, never automatic outcome bans. Encode EVERY actionable lesson in notes/player_principles into plan weights, avoid/retry, counter_weights and checkpoint_prefs/when. The local chooser cannot interpret free prose: these structured fields are its executable form of your reasoning. Give explicit checkpoint preferences when your notes recommend a set at a checkpoint; when is a strong fallback preference.\n\
If phase is mid, this is a mid-fight review: keep what works, replace what fails, and return a full game plan. Its opener will be ignored; start_set or a matching when hint supplies the next set.\n\
How combat works: Posture is the real bar: breaking it opens a deathblow; the deathblow spends one enemy life. Guarding costs the guarder posture; deflects cost the attacker posture and less to the deflector. HP loss slows posture recovery. Pressure only pays off while the enemy can still act; stop planning during a deathblow opening or stagger. Attack until deflected, then answer the counter. Jump and stomp sweeps, Mikiri thrusts (or deflect), and dodge grabs.\n\
The moves table and move_sheet give first-hit time, reach, recovery, posture per hit and best combo posture/time totals; use wolf_posture to start a breaking combo with a fast hit left and choose viable follow-ups using the punish facts. Optimise wins, then fast kills, then converted breaks, then damage per minute. Write a short finisher set for wolf_posture_75. Check break_log and break_summary for breaks that did not convert. move_sheet hits are [seconds, guard posture, deflect posture, judge]; cancels are [start, end, flag, stateInfo]; duration/recovery are seconds and reach metres; projectiles are [launch, speed, life, radius, HP, guard posture, launch/parent travel, direct posture, deflect posture, parent launch]. break_conversion_delay is mean seconds to a landed conversion.\n\
Input includes actions: per-action [n, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other], including Lua choices; action_range: attack distance versus observed reach. Use under-used moves that work. boss_knowledge contains shared fights, player count, action/plan/break outcome counts, boss principles, bans and the compact best plan/result (boss win, seconds, Wolf damage/min, break conversion rate). player_profile contains only this player's fight count, habits, trend, notes, player principles and local bans/failures. Never infer this player's habits from shared evidence. on_wolf_break counts response outcomes. result and trend include wolf_posture_breaks, hits_after_break (landed hits within 2.5 s after the break, excluding the breaking blow), break_punish_rate (fraction of breaks followed by a landed hit), wolf_posture_peak (fraction of max), time_to_kill_wolf (seconds when you win). Input (compact JSON about the fight just over): fight = fights so far; result = Wolf's result (won = who won, secs, defl/guard = Wolf's perfect-deflect and guard percent, hits = hits you landed, hp/posture = percent of his bars lost, punishes = your actions he hit, apm, dpm = percent of a bar per minute, passive percent); trend = his deflect%, guard%, hits, hp%, kill seconds (information only), apm, dpm and passive over the last fights; plans = this fight per plan text, as [exchanges, hit, blocked, deflected, whiffed, punished, countered, mikiri, jump, other] (lua = your own AI's acts; exchanges include tactics, but only attacks can whiff); range = per-plan mean [attack-start distance, observed melee reach] in metres, including root travel, hit capsules and Wolf's radius; absent for projectile-only attacks; a radial envelope relative to the starting position, so distance above reach suggests insufficient range; target movement, facing and timing still affect contact; sets = this fight per set of the game plan played: secs and the same counts (opener = the opener, lua = own AI); wolf = his attacks this fight, same counts; overall = every fight per plan text; habits = how Wolf behaves; played = the game plan this fight ran; notes = your own notes from last time. With no played plan this fight was your plain AI, the baseline.\n\
Output: one game plan. on_wolf_break = 1 to 3 weighted plans, usually one fast attack, run locally immediately after Wolf posture breaks instead of the Lua follow-up; punish_preferences maps near, mid and far to ordered arrays of {{choice, reason}} (empty means none). viable_punishes supplies deterministic facts, not rankings: punish_options maps id to [name, whole move [HP, posture], recovery seconds]; bucket rows reference those ids and are [id, first seconds, seconds left, hits inside window, window [HP, posture], stun_lock (null if unknown), guard posture after window, evidence [uses, landed], conversion rate]. Prefer any viable option for its follow-ups, including weaker hits with stun-lock or guard pressure. The driver obeys the first currently viable preference; otherwise a per-punish Thompson bandit uses observed outcomes with a neutral prior. Fastest reachable is only the fallback when no option fits the data-derived window. player_principles = up to 6 lessons about this player, each at most 90 characters. boss_principles = up to 6 lessons about your tools true against any player; edit the current shared list, never include player habits. boss_bans = optional shared exclusions with choice and reason, based on shared evidence; avoid applies only to this player. A move that works should be repeated while it works; there is no variety penalty. lua must be a minority choice, total weight at most 25% of each set (the fighter caps it). opener = a plan run first in the fight (may be tactics such as hold3.5>walk_in4, or lua for none). sets = 2 to 4 named sets (name of letters, digits, _ or -, at most 24 characters), each with a one-line description for the fighter who picks the set (what it does and when it suits), 2 to 6 weighted plans from the choices below (weights relative), and when = the checkpoint it suits or null. start_set = the set played from the start. avoid = choices not to use, each with a short reason. retry = an optional list of banned choices to re-allow, each with a nonempty reason based on a new hypothesis. banned lists locally excluded plans; failures maps each plan to [uses, deflected_all, fatal, failed_attack_use], where fatal means a mark lost/death within 3 seconds after its last attack or a punished posture break. notes = at most 400 characters: your hypotheses about this player, kept and shown to you next time so your reasoning accumulates; revise them with the new evidence. checkpoint_prefs is optional in old plans; in structured output use an array of {{checkpoint, set, weight}} rows (empty for uniform priors), with weight from 0 to 1 and set naming an offered set. These are prior reward means with four pseudo-counts. The default local Thompson sampler picks a set at the checkpoints; Decisions is optional. Input checkpoints maps each phase/context to set names with [choices rewarded, mean reward], with reward from damage and posture breaks minus damage taken and counters; passive time never pays. The chooser picks which set to switch to at the checkpoints (start, wolf_posture_25, wolf_posture_50, wolf_posture_75 (each re-arms below 15% damage), wolf_broken, pressure, pressure_critical, half_hp (fallback), mark_lost, punished_twice, deflected_thrice, or keep), and your plans run locally through the contextual move fighter, so spread weights over good options and make the sets clearly different. pressure means about 6 Wolf hits to break, pressure_critical about 3 but at least 2; both re-arm above 70% remaining posture. mark_lost only fires with lives still left; checkpoints wait until the boss can act. half_hp is the backup event at 50% HP left. Lua decides when counters happen and which callbacks are legal. counter_weights maps counter animation ids to multipliers from 0 (veto) to 10, default 1. The counters map contains their outcome counts; countered means a successful Wolf Mikiri or sweep kick, not a whiff. boss_phase identifies the character phase planned for. Never invent counter candidates. Guarding, deflecting and other reactions stay automatic. Use only the choices listed; lua leaves an act to the boss's own AI. A choice is a single action, a movement tactic (distances from Wolf, left and right are the boss's) or a plan of up to {max} steps joined with > (consecutive attacks joined with +). Choices:",
        name = actions.name,
        max = actions.max_plan
    );
    for choice in actions.choices() {
        text.push_str(&format!("\n{}: {}", choice.value, choice.label));
    }
    text.push_str("\nlua: leave the act to the boss's own AI\nLocal move fighter: every two ordinary attacks, contextual Thompson sampling selects among eligible plans in the active set; Lua retains reactions/callbacks. Weight seeds priors. Compare mature moves (at least 3 uses) to the available arsenal, never a fixed failure percentage. Counters/Mikiri/jump counters/deflects/full guards/dodged sweeps/damage taken are failures; clean hits, progress and breaks are successes. Reward priority: win faster, break AND quickly connect during the opening, Wolf damage/min, little boss damage. A slow missed opening is penalised; passive time never pays. Push explore=high early or when Wolf adapts; lower it as kill/conversion/damage trends improve. explore=low|normal|high retains a probe floor. move_controls=[{plan,confidence:0..1,lock:bool}] raises a plan prior and reduces exploration; lock holds it for up to 3 eligible attacks, never bypassing limits. digest.moves is the whole-run arsenal table (percent rates; dg=deflected/guarded, counter=Mikiri/jump, progress=? for unknown historical flags, * for a since-v5 rate, recent/life=last-fight/lifetime clean-hit rate (- means no recent samples)); unused and worst rows lead. Flags overlap. digest.best/worst rank mature moves; digest.fights covers 10 completed fights: [kill seconds (-1 loss, -2 unknown), break conversion, damage/min, player first-deathblow (-1 unknown), passive share]. digest.trends gives known-value means in the same column order; digest.hypotheses.outcome is [kill,conversion,damage/min]. digest.lessons is your retained learning. Return up to 6 short lessons, with executable changes in controls/weights/avoids, not prose alone.");
    text
}

pub(crate) fn coach_instructions_for(actions: &BossActions, input: &str) -> String {
    let mut text = coach_instructions(actions);
    let review = serde_json::from_str::<Value>(input)
        .ok()
        .is_some_and(|v| v["digest"]["review"] == true);
    if review {
        text.push_str("\nREVIEW MODE: audit the current strategy against the whole-run moves table and last 10 fights. Compare played/overall/set outcomes to your previous notes/hypotheses and retained lessons. Is it working; what did we learn; which one hypothesis should we test next; should exploration rise or fall; which moves should retire or be promoted? Return the normal complete game plan plus short lessons. Keep successful hypotheses; replace unsupported ones. Validation and fallback are identical to a normal plan.");
    }
    text
}

/// Every value the coach may write as a plan (the choices and `lua`).
fn choice_values(actions: &BossActions) -> Vec<String> {
    let mut v: Vec<String> = actions.choices().into_iter().map(|c| c.value).collect();
    v.push("lua".into());
    v
}

/// The game plan's JSON schema for strict structured output: every property required, no extra
/// ones, choices enum-constrained through one shared definition. Length limits (set count,
/// notes) are enforced after the answer, by `GamePlan::validate`.
pub(crate) fn game_plan_schema(actions: &BossActions) -> Value {
    let counters = actions.counter_ids();
    let counter_properties: serde_json::Map<String, Value> = counters
        .iter()
        .map(|id| {
            (
                id.to_string(),
                json!({"type":"number", "minimum":0, "maximum":10}),
            )
        })
        .collect();
    let counter_keys: Vec<_> = counters.iter().map(i32::to_string).collect();
    let choice = json!({"$ref": "#/$defs/choice"});
    // Attack groups inside tactic plans also appear in the viable move sheet.
    // They need their own enum: a prefix such as Owl's 3000+3001 is not a whole-plan choice.
    let punish_ids: std::collections::BTreeSet<_> = actions
        .actions
        .iter()
        .map(|a| a.id.to_string())
        .chain(actions.combos.iter().map(|c| c.value()))
        .chain(
            actions
                .chains
                .iter()
                .flat_map(|chain| chain.split('>'))
                .filter(|group| group.split('+').all(|id| id.parse::<i32>().is_ok()))
                .map(str::to_owned),
        )
        .filter(|id| actions.parse(id).is_ok())
        .collect();
    let avoid = json!({"type": "object", "additionalProperties": false,
    "required": ["choice", "reason"], "properties": {
        "choice": {"type": "string", "enum": punish_ids}, "reason": {"type": "string"}
    }});
    let checkpoints: Vec<Value> = CHECKPOINTS
        .iter()
        .map(|c| json!(c))
        .chain(std::iter::once(Value::Null))
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["version", "opener", "sets", "start_set", "avoid", "retry", "notes", "on_wolf_break", "player_principles", "boss_principles", "boss_bans", "boss_phase", "counter_weights", "checkpoint_prefs", "explore", "move_controls", "lessons", "punish_preferences"],
        "$defs": {"choice": {"type": "string", "enum": choice_values(actions)}},
        "properties": {
            "punish_preferences": {"type": "object", "additionalProperties": false,
                "required": ["near", "mid", "far"], "properties": {
                    "near": {"type": "array", "items": avoid.clone()},
                    "mid": {"type": "array", "items": avoid.clone()},
                    "far": {"type": "array", "items": avoid.clone()}
                }},
            "explore":{"type":"string", "enum":["low","normal","high"]},
            "lessons":{"type":"array", "maxItems":6, "items":{"type":"string", "maxLength":120}},
            "move_controls":{"type":"array", "maxItems":32, "items":{
                "type":"object", "additionalProperties":false, "required":["plan","confidence","lock"],
                "properties":{"plan":choice, "confidence":{"type":"number","minimum":0,"maximum":1},"lock":{"type":"boolean"}}
            }},
            "checkpoint_prefs": {"type":"array", "items": {
                "type":"object", "additionalProperties":false,
                "required":["checkpoint", "set", "weight"],
                "properties": {
                    "checkpoint":{"type":"string", "enum":CHECKPOINTS},
                    "set":{"type":"string"},
                    "weight":{"type":"number", "minimum":0, "maximum":1}
                }
            }},
            "boss_phase": {"type":"string", "enum":[actions.chr]},
            "counter_weights": {"type":"object", "additionalProperties":false, "required":counter_keys, "properties":counter_properties},
            "version": {"type": "integer", "enum": [GAME_PLAN_VERSION]},
            "opener": choice,
            "sets": {"type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["name", "description", "plans", "when"],
                "properties": {
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "plans": {"type": "array", "items": {
                        "type": "object", "additionalProperties": false,
                        "required": ["plan", "weight"],
                        "properties": {"plan": choice, "weight": {"type": "number"}}}},
                    "when": {"type": ["string", "null"], "enum": checkpoints}
                }}},
            "start_set": {"type": "string"},
            "avoid": {"type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["choice", "reason"],
                "properties": {"choice": choice, "reason": {"type": "string"}}}},
            "retry": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["choice", "reason"], "properties": {"choice": choice, "reason": {"type": "string"}}}},
            "on_wolf_break": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["plan", "weight"], "properties": {"plan": choice, "weight": {"type": "number"}}}},
            "player_principles": {"type": "array", "items": {"type": "string"}},
            "boss_principles": {"type": "array", "items": {"type": "string"}},
            "boss_bans": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["choice", "reason"], "properties": {"choice": choice, "reason": {"type": "string"}}}},
            "notes": {"type": "string"}
        }
    })
}

/// `POST /v1/responses` for the coach (verified against the Responses reference and the
/// structured outputs and reasoning guides on 2026-10-11, see `docs/BRAIN.md`).
pub(crate) fn coach_body(config: &HostedConfig, actions: &BossActions, input: &str) -> Value {
    json!({
        "model": config.coach_model,
        "reasoning": {"effort": config.coach_effort},
        "instructions": coach_instructions_for(actions, input),
        "input": [{"role": "user", "content": input}],
        "text": {"format": {"type": "json_schema", "name": GAME_PLAN, "strict": true,
            "schema": game_plan_schema(actions)}},
        "max_output_tokens": config.coach_max_output_tokens,
        "store": false
    })
}

/// The game plan inside a Responses answer (one assistant `output_text` holding the JSON).
pub(crate) fn coach_plan(response: &Value) -> Result<GamePlan, Failure> {
    let output = response["output"]
        .as_array()
        .ok_or(Failure::InvalidAnswer)?;
    let mut texts = Vec::new();
    for item in output {
        if item["type"] != "message" || item["role"] != "assistant" {
            continue;
        }
        for part in item["content"].as_array().into_iter().flatten() {
            if part["type"] == "refusal" {
                return Err(Failure::Refusal);
            }
            if part["type"] == "output_text" {
                texts.push(part["text"].as_str().ok_or(Failure::InvalidAnswer)?);
            }
        }
    }
    if response["status"] != "completed" {
        return Err(Failure::Incomplete);
    }
    if texts.len() != 1 {
        return Err(Failure::InvalidAnswer);
    }
    GamePlan::from_json(texts[0]).map_err(|_| Failure::InvalidAnswer)
}

/// Pulls the game plan out of free text from a CLI: the JSON object between the first `{` and
/// the last `}` (a model may wrap it in a code fence).
pub(crate) fn plan_from_text(text: &str) -> Result<GamePlan, Failure> {
    let start = text.find('{').ok_or(Failure::InvalidAnswer)?;
    let end = text.rfind('}').ok_or(Failure::InvalidAnswer)?;
    if end < start {
        return Err(Failure::InvalidAnswer);
    }
    GamePlan::from_json(&text[start..=end]).map_err(|_| Failure::InvalidAnswer)
}

/// Instructions of the checkpoint chooser.
pub(crate) fn chooser_instructions(actions: &BossActions) -> String {
    format!(
        "{}: follow coach notes, intent, principles and newest lessons; confidence=[plan,confidence,lock]. Adapt the coach's plan to the moment; keep if it fits. Break viable_punishes are coach facts; use follow-ups and guard pressure. win fast; break+hit; damage/min; low boss damage. Passive>35% fails. Counters/Mikiri/jump/deflect/guard fail. Compare arsenal rates after 3 uses. Guarding without threat fails. Local moves; choose set.",
        actions.chr
    )
}

/// The checkpoint chooser's Decisions body: one choice question over the set names and `keep`.
pub(crate) fn chooser_body(
    config: &HostedConfig,
    actions: &BossActions,
    input: &str,
    sets: &[SetOffer],
) -> Value {
    let mut choices: Vec<Value> = sets
        .iter()
        .map(|s| json!({"value": s.name, "description": s.description}))
        .collect();
    choices.push(json!({"value": KEEP, "description": "stay on the current set"}));
    json!({
        "model": config.model, "input": input,
        "questions": [{"type": "choice", "name": SET_QUESTION,
            "instructions": chooser_instructions(actions), "choices": choices}]
    })
}

/// The chosen set name (or `keep`) of a Decisions answer, checked against the offers.
pub(crate) fn chooser_choice(response: &Value, sets: &[SetOffer]) -> Result<String, Failure> {
    let answers = response["answers"]
        .as_array()
        .ok_or(Failure::InvalidAnswer)?;
    if answers.len() != 1 || answers[0]["name"] != SET_QUESTION {
        return Err(Failure::InvalidAnswer);
    }
    match answers[0]["type"].as_str() {
        Some("refusal") => Err(Failure::Refusal),
        Some("choice") => {
            let value = answers[0]["choice"]
                .as_str()
                .ok_or(Failure::InvalidAnswer)?;
            if value == KEEP || sets.iter().any(|s| s.name == value) {
                Ok(value.to_owned())
            } else {
                Err(Failure::UnknownAction)
            }
        }
        _ => Err(Failure::InvalidAnswer),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn review_instruction_block_uses_the_same_normal_schema() {
        let normal = super::coach_instructions_for(
            &sekiro_sim::brain::GENICHIRO,
            r#"{"digest":{"review":false}}"#,
        );
        let review = super::coach_instructions_for(
            &sekiro_sim::brain::GENICHIRO,
            r#"{"digest":{"review":true}}"#,
        );
        assert!(!normal.contains("REVIEW MODE"));
        assert!(review.contains("REVIEW MODE"));
        for field in ["move_controls", "explore", "lessons"] {
            assert!(
                super::game_plan_schema(&sekiro_sim::brain::GENICHIRO)["properties"]
                    .get(field)
                    .is_some()
            );
        }
    }
    use super::*;
    use crate::ApiKey;
    use sekiro_sim::brain::actions::{ActionKind, BossAction};
    use sekiro_sim::brain::{OWL, SOLDIER, Tactic};

    /// A boss small enough to print whole in the docs: one attack, a back step, two tactics
    /// and one combined plan.
    pub(crate) const DOC_BOSS: BossActions = BossActions {
        chr: "c0000",
        name: "a doc example boss",
        actions: &[
            BossAction {
                id: 3005,
                label: "front kick",
                kind: ActionKind::Attack,
                approach: 2.8,
                pressure: None,
            },
            BossAction {
                id: 5201,
                label: "back step",
                kind: ActionKind::Step,
                approach: 0.0,
                pressure: None,
            },
        ],
        reflex_acts: &[],
        max_plan: 3,
        combos: &[],
        back_steps: &[5201],
        scripted: [&[3005], &[3005]],
        chains: &["walk_in4>3005"],
        tactics: &[Tactic::Hold(2.0), Tactic::WalkIn(4.0)],
    };

    #[test]
    fn coach_effort_is_configurable_and_checkpoints_are_in_the_schema() {
        let mut config = HostedConfig::new(Backend::Coach, ApiKey::new("fake"));
        config.coach_effort = "high".into();
        let b = coach_body(&config, &DOC_BOSS, "{}");
        assert_eq!(b["reasoning"]["effort"], "high");
        let schema = b["text"]["format"]["schema"].to_string();
        for field in ["player_principles", "boss_principles", "boss_bans"] {
            assert!(schema.contains(field));
        }
        assert!(schema.contains("pressure") && schema.contains("pressure_critical"));
        assert!(!schema.contains("posture_half") && !schema.contains("posture_critical"));
        let instructions = coach_instructions(&DOC_BOSS);
        let combat = instructions
            .split_once("If phase is mid, this is a mid-fight review: keep what works, replace what fails, and return a full game plan. Its opener will be ignored; start_set or a matching when hint supplies the next set.\n\
How combat works: ")
            .unwrap()
            .1
            .split('\n')
            .next()
            .unwrap();
        assert!(combat.chars().count() <= 700);
        for fact in [
            "Posture is the real bar",
            "Guarding costs the guarder",
            "deflects cost the attacker",
            "HP loss slows",
            "enemy can still act",
            "Jump",
            "Mikiri",
            "dodge grabs",
        ] {
            assert!(combat.contains(fact), "missing {fact}: {combat}");
        }
    }

    #[test]
    fn documented_requests_match_wire_bodies() {
        let docs = include_str!("../BRAIN.md").replace("\r\n", "\n");
        for (backend, heading) in [
            (Backend::Decisions, "### Decisions request"),
            (Backend::Responses, "### Responses request"),
        ] {
            let section = docs.split_once(heading).unwrap().1;
            let example = section
                .split_once("```json\n")
                .unwrap()
                .1
                .split_once("\n```")
                .unwrap()
                .0;
            let documented: Value = serde_json::from_str(example).unwrap();
            let config = HostedConfig::new(backend, ApiKey::new("test-key-not-real"));
            assert_eq!(
                documented,
                body(
                    &config,
                    &DOC_BOSS,
                    r#"{"id":1,"obs":{},"tally":{},"memory":{}}"#,
                    &[]
                )
            );
        }
    }

    #[test]
    fn coach_punish_preferences_and_decisions_break_facts_are_preserved() {
        let schema = game_plan_schema(&sekiro_sim::brain::GENICHIRO);
        let prefs = &schema["properties"]["punish_preferences"];
        assert_eq!(prefs["required"], json!(["near", "mid", "far"]));
        assert_eq!(
            prefs["properties"]["near"]["items"]["required"],
            json!(["choice", "reason"])
        );
        let owl = game_plan_schema(&sekiro_sim::brain::OWL);
        assert!(owl["properties"]["punish_preferences"]["properties"]["near"]["items"]["properties"]["choice"]["enum"]
            .as_array().unwrap().iter().any(|id| id == "3000+3001"));
        let parsed = GamePlan::from_json(r#"{"punish_preferences":{"near":[{"choice":"3000","reason":"guard follow-up"}],"mid":[],"far":[]}}"#).unwrap()
            .validate(&sekiro_sim::brain::GENICHIRO).plan;
        assert_eq!(parsed.punish_preferences["near"][0].choice, "3000");
        let input = r#"{"why":"wolf_broken","viable_punishes":[["3000","slash",0.7,0.6,1,[64,18],[128,36],true,20,0.5,[2,1],0.5]]}"#;
        let config = HostedConfig::new(Backend::Decisions, ApiKey::new("test-key-not-real"));
        let body = chooser_body(&config, &sekiro_sim::brain::GENICHIRO, input, &[]);
        assert_eq!(body["input"], input);
        assert!(
            body["questions"][0]["instructions"]
                .as_str()
                .unwrap()
                .contains("viable_punishes")
        );
    }

    #[test]
    fn plans_validate_every_component_and_length() {
        assert_eq!(
            plan("3000+3001+3002", &OWL).unwrap().unwrap().actions(),
            [3000, 3001, 3002]
        );
        assert_eq!(plan("3000+9999+3002", &OWL), Err(Failure::UnknownAction));
        assert_eq!(plan("3000+3001+3002+3000", &OWL), Err(Failure::InvalidPlan));
        for invalid in [
            "",
            "+3000",
            "3000+",
            "03000",
            "lua+3000",
            "3000++3001",
            ">3000",
            "walk_in5>3000",
            "hold2+3000",
            "walk_in4>hold2>3000>3001",
        ] {
            assert!(plan(invalid, &OWL).is_err(), "{invalid}");
        }
        let p = plan("walk_in4>hold2>3005", &OWL).unwrap().unwrap();
        assert_eq!(p.value(), "walk_in4>hold2>3005");
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn probabilities_become_a_plan_set() {
        let response = json!({"answers": [{"name": "boss_action", "type": "choice",
            "choice": "3000", "probabilities": [
                {"value": "3000", "probability": 0.5},
                {"value": "walk_in4>3005", "probability": 0.3},
                {"value": "lua", "probability": 0.15},
                {"value": "9999", "probability": 0.04},
                {"value": "3005", "probability": 0.0}]}]});
        let set = plan_set(Backend::Decisions, &response, &OWL);
        let values: Vec<_> = set
            .iter()
            .map(|w| (w.plan.as_ref().map(Plan::value), w.weight))
            .collect();
        assert_eq!(
            values,
            [
                (Some("3000".to_string()), 0.5),
                (Some("walk_in4>3005".to_string()), 0.3),
                (None, 0.15),
            ]
        );
        assert!(plan_set(Backend::Responses, &response, &OWL).is_empty());
        let bare =
            json!({"answers": [{"name": "boss_action", "type": "choice", "choice": "3000"}]});
        assert!(plan_set(Backend::Decisions, &bare, &OWL).is_empty());
    }

    #[test]
    fn both_bosses_offer_tactics_within_the_api_limit() {
        for table in [&OWL, &SOLDIER] {
            let choices = table.choices();
            // Plus `lua`: 2 to 255 unique values.
            assert!(
                (2..=254).contains(&(choices.len() + 1)),
                "{}",
                choices.len()
            );
            let mut seen = std::collections::HashSet::new();
            for c in &choices {
                assert!(seen.insert(c.value.clone()), "duplicate {}", c.value);
                let plan = table
                    .parse(&c.value)
                    .unwrap_or_else(|e| panic!("{}: {e}", c.value));
                assert_eq!(plan.value(), c.value, "canonical text form");
                assert!(table.act_for_plan(&plan, 0.5).is_ok(), "{}", c.value);
            }
            for t in Tactic::ALL {
                assert!(seen.contains(&t.token()));
            }
            eprintln!(
                "{}: {} choices with lua, instructions {} bytes",
                table.chr,
                choices.len() + 1,
                instructions(table).len()
            );
        }
    }

    #[test]
    fn the_soldier_gets_his_own_vocabulary_and_instructions() {
        let config = HostedConfig::new(Backend::Decisions, ApiKey::new("test-key-not-real"));
        let b = body(&config, &SOLDIER, "{}", &[]);
        let q = &b["questions"][0];
        assert_eq!(q["name"], QUESTION);
        let values: Vec<_> = q["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["value"].as_str().unwrap().to_string())
            .collect();
        for want in [
            "3013",
            "3000+3001",
            "3005+3007",
            "hold2",
            "walk_in4>3013",
            "lua",
        ] {
            assert!(values.iter().any(|v| v == want), "{want}");
        }
        assert_eq!(values.last().map(String::as_str), Some("lua"));
        let text = q["instructions"].as_str().unwrap();
        assert!(
            text.starts_with("You are an Ashina soldier with a one-handed sword fighting Wolf.")
        );
        assert!(text.contains("\n3013: perilous thrust"), "{text}");
        assert!(
            text.contains("\n3005+3007: kick, then rising slash"),
            "{text}"
        );
        assert!(text.contains("Reward order: win and win faster"));
        assert!(!text.contains("Owl"));
        assert_eq!(
            plan("3005+3007", &SOLDIER).unwrap().unwrap().actions(),
            [3005, 3007]
        );
        assert_eq!(plan("3031", &SOLDIER), Err(Failure::UnknownAction));
        assert_eq!(plan("3004", &SOLDIER), Err(Failure::UnknownAction));
    }

    #[test]
    fn responses_requests_leave_out_repelled_plans() {
        let config = HostedConfig::new(Backend::Responses, ApiKey::new("test-key-not-real"));
        let excluded = vec!["3003".to_string(), "walk_in4>3013".to_string()];
        let b = body(&config, &SOLDIER, "{}", &excluded);
        let values = b["text"]["format"]["schema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap();
        assert!(!values.iter().any(|v| v == "3003" || v == "walk_in4>3013"));
        assert!(values.iter().any(|v| v == "3000"));
        // Decisions offers every choice and lets the sampler penalise.
        let config = HostedConfig::new(Backend::Decisions, ApiKey::new("test-key-not-real"));
        let b = body(&config, &SOLDIER, "{}", &excluded);
        assert!(
            b["questions"][0]["choices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["value"] == "3003")
        );
    }
}
