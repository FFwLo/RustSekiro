use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use sekiro_sim::brain::{
    BossActions, BossBrain, BrainReply, BrainRequest, CallStats, CoachReply, CoachRequest, Plan,
    RequestMode, SetOffer, Weighted,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{Backend, CoachBackend, ConfigError, HostedConfig, cli, protocol};

// ureq-proto Trace prints raw wire bytes regardless of a header's sensitive flag.
// Fail compilation if a future dependency change accidentally re-enables those traces.
const _: () = assert!(log::STATIC_MAX_LEVEL as usize <= log::LevelFilter::Debug as usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Timeout,
    Http(u16),
    Transport,
    MalformedJson,
    Refusal,
    InvalidAnswer,
    Incomplete,
    UnknownAction,
    InvalidPlan,
    Serialization,
    /// The coach CLI is not on PATH.
    CliMissing,
    /// The coach CLI ran and failed, or could not be started.
    CliFailed,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct TokenUsage {
    pub cached_input_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Reasoning tokens (billed as output; `usage.output_tokens_details.reasoning_tokens`).
    pub reasoning_tokens: u64,
}

impl TokenUsage {
    fn from_response(response: &Value) -> Self {
        Self {
            cached_input_tokens: response["usage"]["input_tokens_details"]["cached_tokens"]
                .as_u64()
                .or_else(|| response["usage"]["cached_input_tokens"].as_u64())
                .unwrap_or(0),
            input_tokens: response["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: response["usage"]["output_tokens"].as_u64().unwrap_or(0),
            reasoning_tokens: response["usage"]["output_tokens_details"]["reasoning_tokens"]
                .as_u64()
                .unwrap_or(0),
        }
    }
}

/// Totals of the coach's calls.
#[derive(Debug, Clone, Default)]
pub struct CoachStats {
    pub cached_input_tokens: u64,
    pub requests: u64,
    pub answered: u64,
    pub failed: BTreeMap<Failure, u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_ms: f64,
    pub max_ms: f64,
}

/// Totals of the calls. The plain fields count the per-fight calls (the checkpoint chooser, or
/// the old Decisions and Responses requests); `coach` counts the coach's.
#[derive(Debug, Clone, Default)]
pub struct HostedStats {
    pub cached_input_tokens: u64,
    pub costs: sekiro_sim::brain::cost::CostSnapshot,
    pub requests: u64,
    pub answered: u64,
    pub failed: BTreeMap<Failure, u64>,
    pub mean_round_trip_ms: f64,
    pub max_round_trip_ms: f64,
    pub total_round_trip_ms: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub coach: CoachStats,
    /// Logging failure does not change a valid answer into a gameplay failure.
    pub log_errors: u64,
}

#[derive(Clone, Default)]
pub struct StatsHandle {
    /// Budgeted calls are serialized across both workers. Simulation never takes this lock.
    gate: Arc<Mutex<()>>,
    budget_usd: Option<f64>,
    token_budget: Option<u64>,
    requests: Arc<AtomicU64>,
    coach_requests: Arc<AtomicU64>,
    completed: Arc<Mutex<HostedStats>>,
}

impl StatsHandle {
    fn exhausted(&self) -> bool {
        let mut s = self.completed.lock().unwrap_or_else(|e| e.into_inner());
        let t = &s.costs.totals;
        let reason = if self.token_budget.is_some_and(|n| t.tokens() >= n) {
            Some("token budget reached")
        } else if self.budget_usd.is_some_and(|n| t.billed_usd >= n) {
            Some("billed USD budget reached")
        } else if self.budget_usd.is_some() && t.unknown_billed > 0 {
            Some("billed cost unknown; USD budget cannot be enforced")
        } else {
            None
        };
        if let Some(reason) = reason {
            if s.costs.budget_notice.is_none() {
                let notice =
                    format!("Brain stopped: {reason}. Last plan this fight; own AI next fight.");
                eprintln!("{notice}");
                s.costs.budget_notice = Some(notice);
            }
            true
        } else {
            false
        }
    }
    fn record_cost(
        &self,
        kind: &str,
        model: &str,
        usage: TokenUsage,
        estimate: Option<f64>,
        billed: bool,
    ) {
        let mut s = self.completed.lock().unwrap_or_else(|e| e.into_inner());
        let add = |t: &mut sekiro_sim::brain::cost::CostTotals| {
            t.calls += 1;
            t.input += usage.input_tokens;
            t.cached += usage.cached_input_tokens;
            t.output += usage.output_tokens;
            t.reasoning += usage.reasoning_tokens;
            if billed {
                t.api_tokens += usage.input_tokens.saturating_add(usage.output_tokens);
            } else {
                t.plan_tokens += usage.input_tokens.saturating_add(usage.output_tokens);
            }
            t.billed_usd += if billed { estimate.unwrap_or(0.0) } else { 0.0 };
            t.unknown_billed += u64::from(billed && estimate.is_none());
        };
        add(&mut s.costs.totals);
        let rows = &mut s.costs.rows;
        let index = rows
            .iter()
            .position(|r| r.kind == kind && r.model == model)
            .unwrap_or_else(|| {
                rows.push(sekiro_sim::brain::cost::CostRow {
                    kind: kind.into(),
                    model: model.into(),
                    ..Default::default()
                });
                rows.len() - 1
            });
        add(&mut rows[index].totals);
        drop(s);
        self.exhausted();
    }
    pub fn snapshot(&self) -> HostedStats {
        let mut stats = self
            .completed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        stats.requests = self.requests.load(Ordering::Relaxed);
        stats.coach.requests = self.coach_requests.load(Ordering::Relaxed);
        stats
    }
}

struct Work {
    actions: &'static BossActions,
    id: u64,
    input: Result<String, Failure>,
    excluded: Vec<String>,
    /// Coach mode: the sets the chooser picks from (empty: the old per-act question).
    sets: Vec<SetOffer>,
}

struct CoachWork {
    id: u64,
    input: String,
}

pub struct HostedBrain {
    actions: &'static BossActions,
    backend: Backend,
    tx: Option<mpsc::Sender<Work>>,
    rx: mpsc::Receiver<BrainReply>,
    worker: Option<JoinHandle<()>>,
    coach_tx: Option<mpsc::Sender<CoachWork>>,
    coach_rx: mpsc::Receiver<CoachReply>,
    coach_worker: Option<JoinHandle<()>>,
    /// The coach talks to a CLI (killed at once on drop) rather than an HTTP call.
    coach_is_cli: bool,
    checkpoint_chooser: sekiro_sim::brain::bandit::CheckpointChooser,
    local_reply: Option<BrainReply>,
    stopping: Arc<AtomicBool>,
    stats: StatsHandle,
}

impl HostedBrain {
    pub fn new(config: HostedConfig, actions: &'static BossActions) -> Result<Self, ConfigError> {
        let needs_key = config.backend != Backend::Coach
            || config.coach_backend == CoachBackend::Api
            || config.checkpoint_chooser == sekiro_sim::brain::bandit::CheckpointChooser::Decisions;
        if config.model.trim().is_empty()
            || config.budget_usd.is_some_and(|n| !n.is_finite() || n < 0.0)
            || config.coach_model.trim().is_empty()
            || (needs_key && config.api_key.0.trim().is_empty())
            || config.api_key.0.chars().any(char::is_control)
            || config.timeout.is_zero()
            || config.coach_timeout.is_zero()
            || Instant::now().checked_add(config.timeout).is_none()
            || Instant::now().checked_add(config.coach_timeout).is_none()
            || !(config.base_url.starts_with("https://") || config.base_url.starts_with("http://"))
            || actions.actions.is_empty()
            || actions.actions.len() > 253
            || actions.max_plan == 0
            || actions
                .actions
                .iter()
                .enumerate()
                .any(|(i, a)| actions.actions[..i].iter().any(|b| b.id == a.id))
        {
            return Err(ConfigError::InvalidConfig);
        }
        let log = match &config.log_path {
            Some(path) => {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent).map_err(|_| ConfigError::LogOpen)?;
                }
                Some(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .map_err(|_| ConfigError::LogOpen)?,
                )
            }
            None => None,
        };
        let backend = config.backend;
        let checkpoint_chooser = config.checkpoint_chooser;
        let (tx, work) = mpsc::channel();
        let (reply, rx) = mpsc::channel();
        let (coach_tx, coach_work) = mpsc::channel();
        let (coach_reply, coach_rx) = mpsc::channel();
        let stats = StatsHandle {
            budget_usd: config.budget_usd,
            token_budget: config.token_budget,
            ..Default::default()
        };
        let stopping = Arc::new(AtomicBool::new(false));
        let coach_is_cli = backend == Backend::Coach && config.coach_backend != CoachBackend::Api;
        let coach_log = log.as_ref().and_then(|f| f.try_clone().ok());
        let coach_config = (backend == Backend::Coach).then(|| CoachConfig {
            prices: config.prices.clone(),
            backend: config.coach_backend,
            model: config.coach_model.clone(),
            effort: config.coach_effort.clone(),
            timeout: config.coach_timeout,
            max_output_tokens: config.coach_max_output_tokens,
            cmd: config.coach_cmd.clone(),
        });
        let coach_http = (backend == Backend::Coach && config.coach_backend == CoachBackend::Api)
            .then(|| (config.base_url.clone(), config.api_key.0.clone()));
        let worker_stats = stats.clone();
        let worker_stopping = stopping.clone();
        let worker = thread::Builder::new()
            .name(format!("boss-{}", backend.name()))
            // TLS setup and the handshake use large stack frames; 2 MB overflows on Windows.
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run(config, work, reply, worker_stats, worker_stopping, log))
            .map_err(|_| ConfigError::WorkerStart)?;
        let coach_worker = match coach_config {
            Some(cc) => {
                let s = stats.clone();
                let stop = stopping.clone();
                Some(
                    thread::Builder::new()
                        .name("boss-coach".into())
                        .stack_size(8 * 1024 * 1024)
                        .spawn(move || {
                            run_coach(
                                cc,
                                coach_http,
                                actions,
                                coach_work,
                                coach_reply,
                                s,
                                stop,
                                coach_log,
                            )
                        })
                        .map_err(|_| ConfigError::WorkerStart)?,
                )
            }
            None => None,
        };
        Ok(Self {
            actions,
            backend,
            tx: Some(tx),
            rx,
            worker: Some(worker),
            coach_tx: Some(coach_tx),
            coach_rx,
            coach_worker,
            coach_is_cli,
            checkpoint_chooser,
            local_reply: None,
            stopping,
            stats,
        })
    }

    pub fn stats(&self) -> HostedStats {
        self.stats.snapshot()
    }

    /// Retain before passing the controller to `Duel::set_brain` to inspect its telemetry.
    pub fn stats_handle(&self) -> StatsHandle {
        self.stats.clone()
    }
}

impl BossBrain for HostedBrain {
    fn checkpoint_chooser(&self) -> sekiro_sim::brain::bandit::CheckpointChooser {
        self.checkpoint_chooser
    }
    fn change_phase(&mut self, actions: &'static BossActions) {
        self.actions = actions;
    }
    fn costs(&self) -> sekiro_sim::brain::cost::CostSnapshot {
        self.stats.snapshot().costs
    }
    fn budget_exhausted(&self) -> bool {
        self.stats.exhausted()
    }
    fn name(&self) -> &'static str {
        self.backend.name()
    }

    /// A hosted model is asked at checkpoints and its answer, a plan set, is sampled locally.
    fn default_mode(&self) -> RequestMode {
        RequestMode::Checkpoints
    }

    fn coaches(&self) -> bool {
        self.backend == Backend::Coach
    }

    fn usage(&self) -> (u64, u64) {
        let stats = self.stats.snapshot();
        (stats.input_tokens, stats.output_tokens)
    }

    fn call_stats(&self) -> [CallStats; 2] {
        let s = self.stats.snapshot();
        let calls = s.answered + s.failed.values().sum::<u64>();
        let c = &s.coach;
        [
            CallStats {
                calls,
                failed: s.failed.values().sum(),
                input: s.input_tokens,
                output: s.output_tokens,
                reasoning: s.reasoning_tokens,
                total_ms: s.total_round_trip_ms,
                max_ms: s.max_round_trip_ms,
            },
            CallStats {
                calls: c.answered + c.failed.values().sum::<u64>(),
                failed: c.failed.values().sum(),
                input: c.input_tokens,
                output: c.output_tokens,
                reasoning: c.reasoning_tokens,
                total_ms: c.total_ms,
                max_ms: c.max_ms,
            },
        ]
    }

    fn submit(&mut self, request: &BrainRequest) {
        if self.backend == Backend::Coach
            && self.checkpoint_chooser != sekiro_sim::brain::bandit::CheckpointChooser::Decisions
        {
            self.local_reply = Some(BrainReply {
                id: request.id,
                ..Default::default()
            });
            return;
        }
        // Unbounded mpsc send never waits for the HTTP worker. No worker-held locks here.
        let input = match &request.brief {
            Some(brief) if !request.sets.is_empty() => Ok(brief.clone()),
            _ => serde_json::to_string(request).map_err(|_| Failure::Serialization),
        };
        self.stats.requests.fetch_add(1, Ordering::Relaxed);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Work {
                actions: self.actions,
                id: request.id,
                input,
                excluded: request.excluded.clone(),
                sets: request.sets.clone(),
            });
        }
    }

    fn poll(&mut self, _now: f32) -> Option<BrainReply> {
        if self.local_reply.is_some() {
            return self.local_reply.take();
        }
        self.rx.try_recv().ok()
    }

    fn submit_coach(&mut self, request: &CoachRequest, _now: f32) {
        self.stats.coach_requests.fetch_add(1, Ordering::Relaxed);
        if let Some(tx) = &self.coach_tx {
            let _ = tx.send(CoachWork {
                id: request.id,
                input: request.input.clone(),
            });
        }
    }

    fn poll_coach(&mut self, _now: f32) -> Option<CoachReply> {
        self.coach_rx.try_recv().ok()
    }
}

impl Drop for HostedBrain {
    fn drop(&mut self) {
        // Do not drain an arbitrary queue on shutdown. Finish only the bounded in-flight call;
        // a coach CLI child is killed at once, an in-flight coach HTTP call (up to a minute) is
        // left to die with the process.
        self.stopping.store(true, Ordering::Release);
        self.tx.take();
        self.coach_tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.coach_worker.take()
            && self.coach_is_cli
        {
            let _ = worker.join();
        }
    }
}

fn http_failure(error: ureq::Error) -> Failure {
    match error {
        ureq::Error::Timeout(_) => Failure::Timeout,
        ureq::Error::StatusCode(code) => Failure::Http(code),
        _ => Failure::Transport,
    }
}

fn authorization(key: &crate::ApiKey) -> Result<ureq::http::HeaderValue, Failure> {
    let mut value = ureq::http::HeaderValue::from_str(&format!("Bearer {}", key.0))
        .map_err(|_| Failure::Transport)?;
    value.set_sensitive(true);
    Ok(value)
}

/// POSTs `body` and returns the parsed JSON response. Raw transport errors and response bodies
/// are deliberately never formatted or logged.
fn post(
    agent: &ureq::Agent,
    url: &str,
    key: &crate::ApiKey,
    body: Value,
) -> Result<Value, Failure> {
    let mut response = agent
        .post(url)
        .header("Authorization", authorization(key)?)
        .send_json(body)
        .map_err(http_failure)?;
    let raw = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(http_failure)?;
    serde_json::from_str(&raw).map_err(|_| Failure::MalformedJson)
}

/// What a per-fight call produced.
struct Answer {
    plan: Option<Plan>,
    set: Vec<Weighted>,
    /// The chosen value (a choice, or a set name in coach mode).
    value: String,
    /// Coach mode: the set chosen.
    choice: Option<String>,
}

fn call(
    agent: &ureq::Agent,
    config: &HostedConfig,
    actions: &BossActions,
    work: &Work,
    input: &str,
    usage: &mut TokenUsage,
) -> Result<Answer, Failure> {
    let url = format!(
        "{}/v1/{}",
        config.base_url.trim_end_matches('/'),
        config.backend.endpoint()
    );
    if !work.sets.is_empty() {
        let body = protocol::chooser_body(config, actions, input, &work.sets);
        let response = post(agent, &url, &config.api_key, body)?;
        *usage = TokenUsage::from_response(&response);
        let value = protocol::chooser_choice(&response, &work.sets)?;
        return Ok(Answer {
            plan: None,
            set: Vec::new(),
            choice: Some(value.clone()),
            value,
        });
    }
    let body = protocol::body(config, actions, input, &work.excluded);
    let response = post(agent, &url, &config.api_key, body)?;
    *usage = TokenUsage::from_response(&response);
    let value = protocol::choice(config.backend, &response)?;
    let plan = protocol::plan(&value, actions)?;
    let set = protocol::plan_set(config.backend, &response, actions);
    Ok(Answer {
        plan,
        set,
        value,
        choice: None,
    })
}

/// Warms the route to the host before the first decision: resolves its name and opens a TCP
/// connection, then closes it. Nothing is sent, so no API request is made and no key leaves the
/// machine. (ureq has no public way to pool a connection it did not open itself, so the TLS
/// handshake still happens on the first real request; this takes the DNS lookup and the first
/// route set-up out of it.)
fn warm_up(base_url: &str, timeout: std::time::Duration) {
    use std::net::ToSocketAddrs;
    let rest = base_url.split_once("//").map_or(base_url, |(_, r)| r);
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    let port = if base_url.starts_with("https://") {
        443
    } else {
        80
    };
    let target = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:{port}")
    };
    if let Some(addr) = target.to_socket_addrs().ok().and_then(|mut a| a.next()) {
        let _ = std::net::TcpStream::connect_timeout(&addr, timeout);
    }
}

/// Appends one JSON line to the telemetry log; false when the write failed.
fn log_line(log: &mut Option<File>, line: &Value) -> bool {
    let failed = log.as_mut().is_some_and(|file| {
        serde_json::to_writer(&mut *file, line).is_err()
            || file.write_all(b"\n").is_err()
            || file.flush().is_err()
    });
    if failed {
        *log = None;
    }
    failed
}

#[allow(clippy::too_many_arguments)]
fn annotate(
    line: &mut Value,
    system: &str,
    model: &str,
    operation: &str,
    usage: TokenUsage,
    estimate: Option<f64>,
    billed: bool,
    phase: &str,
    ms: f64,
) {
    let fields = json!({
        "gen_ai.system": system, "gen_ai.request.model": model,
        "gen_ai.operation.name": operation,
        "gen_ai.usage.input_tokens": usage.input_tokens,
        "gen_ai.usage.output_tokens": usage.output_tokens,
        "sekiro.usage.cached_input_tokens": usage.cached_input_tokens,
        "sekiro.usage.reasoning_tokens": usage.reasoning_tokens,
        "sekiro.usd_estimate": if billed { estimate } else { None }, "sekiro.billed": billed,
        "sekiro.tokens.proxy": !billed,
        "sekiro.latency_ms": ms, "sekiro.kind": line["kind"], "sekiro.phase": phase,
        "usd_estimate": if billed { estimate } else { None }, "billed": billed, "phase": phase
    });
    line.as_object_mut()
        .expect("telemetry object")
        .extend(fields.as_object().expect("fields").clone());
}

fn run(
    config: HostedConfig,
    work: mpsc::Receiver<Work>,
    reply: mpsc::Sender<BrainReply>,
    stats: StatsHandle,
    stopping: Arc<AtomicBool>,
    mut log: Option<File>,
) {
    if config.backend == Backend::Coach
        && config.checkpoint_chooser != sekiro_sim::brain::bandit::CheckpointChooser::Decisions
    {
        // No TLS agent, DNS warmup, or request processing for a local/off chooser.
        return;
    }
    // Keep one pooled rustls agent for the entire fight. Disable redirects (no credential
    // forwarding or extra provider calls) and environment proxies (loopback tests stay local).
    if config.warm_up && config.checkpoints {
        warm_up(&config.base_url, config.timeout);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .max_redirects(0)
        .proxy(None)
        .build()
        .into();
    let mut total_ms = 0.0;
    let mut completed = 0_u64;
    for work in work {
        if stopping.load(Ordering::Acquire) {
            break;
        }
        if !work.sets.is_empty() && !config.checkpoints {
            // No API key: the plan's opener and start set play; no switching.
            let sent = reply.send(BrainReply {
                id: work.id,
                ..BrainReply::default()
            });
            if sent.is_err() {
                break;
            }
            continue;
        }
        let _gate = (stats.budget_usd.is_some() || stats.token_budget.is_some())
            .then(|| stats.gate.lock().unwrap_or_else(|e| e.into_inner()));
        if stats.exhausted() {
            let _ = reply.send(BrainReply {
                id: work.id,
                ..Default::default()
            });
            continue;
        }
        let start = Instant::now();
        let mut usage = TokenUsage::default();
        let result = match &work.input {
            Ok(input) => call(&agent, &config, work.actions, &work, input, &mut usage),
            Err(f) => Err(*f),
        };
        let round_trip_ms = start.elapsed().as_secs_f64() * 1000.0;
        let (answer, failure) = match result {
            Ok(a) => (Some(a), None),
            Err(reason) => (None, Some(reason)),
        };
        let chosen = answer.as_ref().map(|a| config.redact(&a.value));
        let estimate = config
            .prices
            .estimate(&config.model, config.backend.endpoint(), usage);
        let mut line = json!({
            "id": work.id, "kind": "decide", "backend": config.backend.name(),
            "model": config.redact(&config.model),
            "round_trip_ms": round_trip_ms, "status": if failure.is_some() {"failed"} else {"answered"},
            "failure": failure, "chosen": chosen, "usage": usage
        });
        annotate(
            &mut line,
            "openai",
            &config.redact(&config.model),
            config.backend.endpoint(),
            usage,
            estimate,
            true,
            "mid",
            round_trip_ms,
        );
        let log_failed = log_line(&mut log, &line);
        stats.record_cost(
            "decide",
            &config.redact(&config.model),
            usage,
            estimate,
            true,
        );
        completed += 1;
        total_ms += round_trip_ms;
        {
            let mut stats = stats.completed.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(reason) = failure {
                *stats.failed.entry(reason).or_default() += 1;
            } else {
                stats.answered += 1;
            }
            stats.mean_round_trip_ms = total_ms / completed as f64;
            stats.total_round_trip_ms = total_ms;
            stats.max_round_trip_ms = stats.max_round_trip_ms.max(round_trip_ms);
            stats.input_tokens += usage.input_tokens;
            stats.cached_input_tokens += usage.cached_input_tokens;
            stats.output_tokens += usage.output_tokens;
            stats.reasoning_tokens += usage.reasoning_tokens;
            stats.log_errors += u64::from(log_failed);
        }
        let brain_reply = match answer {
            Some(a) => BrainReply {
                id: work.id,
                plan: a.plan,
                set: a.set,
                choice: a.choice,
            },
            None => BrainReply {
                id: work.id,
                ..BrainReply::default()
            },
        };
        if reply.send(brain_reply).is_err() {
            break;
        }
    }
}

/// How the coach thread runs a call.
struct CoachConfig {
    prices: crate::PriceTable,
    backend: CoachBackend,
    model: String,
    effort: String,
    timeout: std::time::Duration,
    max_output_tokens: u32,
    cmd: Option<std::path::PathBuf>,
}

/// What a coach call returned.
struct CoachAnswer {
    plan: sekiro_sim::brain::GamePlan,
    usage: TokenUsage,
    cost_usd: Option<f64>,
}

/// A short, payload-free reason for the console and the reply.
fn describe(backend: CoachBackend, failure: Failure) -> String {
    match failure {
        Failure::CliMissing => format!(
            "the {0} CLI was not found on PATH (install it, set SEKIRO_COACH_CMD, or use --coach api)",
            backend.name()
        ),
        Failure::CliFailed => format!("the {} CLI failed", backend.name()),
        other => serde_json::to_value(other).map_or_else(
            |_| "failed".into(),
            |v| v.to_string().trim_matches('"').to_string(),
        ),
    }
}

fn api_coach(
    agent: &ureq::Agent,
    config: &CoachConfig,
    base_url: &str,
    key: &crate::ApiKey,
    actions: &BossActions,
    input: &str,
    usage: &mut TokenUsage,
) -> Result<CoachAnswer, Failure> {
    let url = format!("{}/v1/responses", base_url.trim_end_matches('/'));
    let probe = HostedConfig {
        coach_model: config.model.clone(),
        coach_effort: config.effort.clone(),
        coach_max_output_tokens: config.max_output_tokens,
        ..HostedConfig::new(Backend::Coach, crate::ApiKey::new("unused"))
    };
    let body = protocol::coach_body(&probe, actions, input);
    let response = post(agent, &url, key, body)?;
    *usage = TokenUsage::from_response(&response);
    match protocol::coach_plan(&response) {
        Ok(plan) => Ok(CoachAnswer {
            plan,
            usage: *usage,
            cost_usd: None,
        }),
        // Tokens were spent even when the answer is unusable.
        Err(f) => Err(f),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_coach(
    config: CoachConfig,
    http: Option<(String, String)>,
    actions: &'static BossActions,
    work: mpsc::Receiver<CoachWork>,
    reply: mpsc::Sender<CoachReply>,
    stats: StatsHandle,
    stopping: Arc<AtomicBool>,
    mut log: Option<File>,
) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .max_redirects(0)
        .proxy(None)
        .build()
        .into();
    for work in work {
        let phase = serde_json::from_str::<serde_json::Value>(&work.input)
            .ok()
            .and_then(|v| v["boss_phase"].as_str().map(str::to_owned));
        let actions = phase
            .as_deref()
            .and_then(BossActions::for_chr)
            .unwrap_or(actions);
        let schema = protocol::game_plan_schema(actions);
        if stopping.load(Ordering::Acquire) {
            break;
        }
        let _gate = (stats.budget_usd.is_some() || stats.token_budget.is_some())
            .then(|| stats.gate.lock().unwrap_or_else(|e| e.into_inner()));
        if stats.exhausted() {
            let _ = reply.send(CoachReply {
                id: work.id,
                error: Some("session budget exhausted".into()),
                ..Default::default()
            });
            continue;
        }
        let start = Instant::now();
        let mut spent = TokenUsage::default();
        let result: Result<CoachAnswer, Failure> = match (&http, config.backend) {
            (Some((base, key)), CoachBackend::Api) => {
                let key = crate::ApiKey::new(key.clone());
                api_coach(
                    &agent,
                    &config,
                    base,
                    &key,
                    actions,
                    &work.input,
                    &mut spent,
                )
            }
            (_, backend) => {
                let prompt = format!(
                    "Input (fight review):\n{}\n\nReply with only the game plan as one JSON object matching the schema, no other text.",
                    work.input
                );
                cli::run(
                    backend,
                    (&config.model, &config.effort),
                    config.cmd.as_deref(),
                    &cli::Prompt {
                        input: &prompt,
                        instructions: &protocol::coach_instructions_for(actions, &work.input),
                    },
                    &schema,
                    config.timeout,
                    &stopping,
                    &mut spent,
                )
                .map(|a| CoachAnswer {
                    plan: a.plan,
                    usage: TokenUsage {
                        cached_input_tokens: a.cached_tokens,
                        input_tokens: a.tokens[0],
                        output_tokens: a.tokens[1],
                        reasoning_tokens: a.tokens[2],
                    },
                    cost_usd: a.cost_usd,
                })
            }
        };
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let (plan, failure, usage, cost) = match result {
            Ok(a) => (Some(a.plan), None, a.usage, a.cost_usd),
            Err(f) => (None, Some(f), spent, None),
        };
        let billed = config.backend == CoachBackend::Api;
        let estimate = billed
            .then(|| config.prices.estimate(&config.model, "responses", usage))
            .flatten();
        let phase = serde_json::from_str::<Value>(&work.input)
            .ok()
            .and_then(|v| v["phase"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "post".into());
        let mut line = json!({
            "id": work.id, "kind": "coach", "phase": serde_json::from_str::<serde_json::Value>(&work.input).ok().and_then(|v| v.get("phase").cloned()), "backend": config.backend.name(),
            "model": config.model, "round_trip_ms": ms,
            "status": if failure.is_some() {"failed"} else {"answered"},
            "failure": failure, "usage": usage, "cost_usd": if billed { cost } else { None }
        });
        annotate(
            &mut line,
            if config.backend == CoachBackend::Claude {
                "anthropic"
            } else {
                "openai"
            },
            &config.model,
            "responses",
            usage,
            estimate,
            billed,
            &phase,
            ms,
        );
        let log_failed = log_line(&mut log, &line);
        stats.record_cost("coach", &config.model, usage, estimate, billed);
        {
            let mut s = stats.completed.lock().unwrap_or_else(|e| e.into_inner());
            let c = &mut s.coach;
            match failure {
                Some(f) => *c.failed.entry(f).or_default() += 1,
                None => c.answered += 1,
            }
            c.input_tokens += usage.input_tokens;
            c.cached_input_tokens += usage.cached_input_tokens;
            c.output_tokens += usage.output_tokens;
            c.reasoning_tokens += usage.reasoning_tokens;
            c.total_ms += ms;
            c.max_ms = c.max_ms.max(ms);
            s.log_errors += u64::from(log_failed);
        }
        let sent = reply.send(CoachReply {
            id: work.id,
            plan,
            error: failure.map(|f| describe(config.backend, f)),
            tokens: [
                usage.input_tokens,
                usage.output_tokens,
                usage.reasoning_tokens,
            ],
            latency_ms: ms.round() as u32,
        });
        if sent.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_debug_is_sensitive() {
        let key = crate::ApiKey::new("test-key-not-real");
        let value = authorization(&key).unwrap();
        assert!(value.is_sensitive());
        assert!(!format!("{value:?}").contains(&key.0));
    }
}
