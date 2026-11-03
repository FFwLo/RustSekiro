//! The coach through a subscription CLI: `claude -p` (Claude Code headless) or `codex exec`,
//! so the game plan costs no API key.
//!
//! The child runs from its own temporary directory with no tools. Claude uses our instructions
//! (Claude by flag, Codex from a file) as the replacement system prompt and only the summary on stdin.
//! User settings and configured MCP servers are disabled. The answer is read from the
//! CLI's JSON envelope (`claude`) or its last-message file (`codex`). The child is killed
//! through its own handle at the deadline and when the game exits. Nothing the child prints is
//! ever echoed into a diagnostic, since it may contain the prompt.
//!
//! Flags come from `claude --help` and `codex exec --help` (read, never run, on 2026-10-11):
//! `-p`, `--model`, `--output-format json`, `--tools ""`, `--no-session-persistence`,
//! `--json-schema`, `--system-prompt`, `--strict-mcp-config`, `--mcp-config`,
//! `--setting-sources ""`, `--safe-mode`, `--effort low`; Codex `exec --ignore-user-config
//! -c model_reasoning_effort="medium" --skip-git-repo-check -s read-only -m --ephemeral
//! --output-schema -o -`. Claude's dynamic-section flag is ignored with --system-prompt.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::{CoachBackend, Failure, protocol};
use sekiro_sim::brain::GamePlan;

/// What a CLI coach call returned.
pub(crate) struct CliAnswer {
    pub plan: GamePlan,
    /// Input (all kinds), output and reasoning tokens, when the CLI reports them.
    pub tokens: [u64; 3],
    pub cost_usd: Option<f64>,
    pub cached_tokens: u64,
}

static RUNS: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Prompt<'a> {
    pub instructions: &'a str,
    pub input: &'a str,
}

/// A fresh empty directory under the system temp dir, removed when dropped (only the files this
/// run made in it).
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "sekiro-coach-{}-{}",
            std::process::id(),
            RUNS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // The directory only ever holds the schema and answer files this run wrote.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Programs to try: the override alone, else the CLI's name (and on Windows its npm shim).
fn programs(backend: CoachBackend, cmd: Option<&Path>) -> Vec<PathBuf> {
    if let Some(cmd) = cmd {
        return vec![cmd.to_path_buf()];
    }
    let name = backend.name();
    if cfg!(windows) {
        vec![name.into(), format!("{name}.cmd").into()]
    } else {
        vec![name.into()]
    }
}

/// Runs the coach CLI with `prompt` on stdin. `schema` is the game plan's JSON schema.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    backend: CoachBackend,
    model_and_effort: (&str, &str),
    cmd: Option<&Path>,
    prompt: &Prompt<'_>,
    schema: &Value,
    timeout: Duration,
    stopping: &AtomicBool,
    spent: &mut crate::TokenUsage,
) -> Result<CliAnswer, Failure> {
    let (model, effort) = model_and_effort;
    let instructions = prompt.instructions;
    let input = prompt.input;
    let scratch = Scratch::new().map_err(|_| Failure::CliFailed)?;
    let answer_file = scratch.0.join("answer.txt");
    let mut args: Vec<String> = Vec::new();
    match backend {
        CoachBackend::Claude => {
            let mcp_file = scratch.0.join("mcp.json");
            std::fs::write(&mcp_file, r#"{"mcpServers":{}}"#).map_err(|_| Failure::CliFailed)?;
            args.extend(
                [
                    "-p",
                    "--model",
                    model,
                    "--output-format",
                    "json",
                    "--no-session-persistence",
                    "--tools",
                    "",
                    "--json-schema",
                ]
                .map(String::from),
            );
            args.push(schema.to_string());
            args.extend(
                [
                    "--system-prompt",
                    instructions,
                    "--strict-mcp-config",
                    "--mcp-config",
                ]
                .map(String::from),
            );
            args.push(mcp_file.to_string_lossy().into_owned());
            args.extend(
                ["--setting-sources", "", "--safe-mode", "--effort", "low"].map(String::from),
            );
        }
        CoachBackend::Codex => {
            let schema_file = scratch.0.join("schema.json");
            std::fs::write(&schema_file, schema.to_string()).map_err(|_| Failure::CliFailed)?;
            // Our instructions replace Codex's coding system prompt, and nothing of the user's
            // (config, rules, AGENTS.md, memories, plugins, apps) is loaded: measured at ~5 s
            // and ~11k tokens per call with only game context (2026-10-11).
            let instructions_file = scratch.0.join("coach.md");
            std::fs::write(&instructions_file, instructions).map_err(|_| Failure::CliFailed)?;
            args.extend(
                [
                    "exec",
                    "--skip-git-repo-check",
                    "--ephemeral",
                    "--ignore-user-config",
                    "--ignore-rules",
                    "-s",
                    "read-only",
                    "-m",
                    model,
                    "-c",
                    &format!("model_reasoning_effort={}", toml_string(effort)),
                    "-c",
                    "project_doc_max_bytes=0",
                ]
                .map(String::from),
            );
            args.push("-c".into());
            args.push(format!(
                "model_instructions_file={}",
                toml_string(&instructions_file.to_string_lossy())
            ));
            for feature in [
                "memories",
                "apps",
                "plugins",
                "browser_use",
                "computer_use",
                "multi_agent",
            ] {
                args.push("--disable".into());
                args.push(feature.into());
            }
            args.push("--json".into());
            args.push("--output-schema".into());
            args.push(schema_file.to_string_lossy().into_owned());
            args.push("-o".into());
            args.push(answer_file.to_string_lossy().into_owned());
            args.push("-".into());
        }
        CoachBackend::Api => return Err(Failure::CliFailed),
    }
    let mut child = None;
    for program in programs(backend, cmd) {
        match Command::new(&program)
            .args(&args)
            .current_dir(&scratch.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => {
                child = Some(c);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Failure::CliFailed),
        }
    }
    let Some(mut child) = child else {
        return Err(Failure::CliMissing);
    };
    // Feed stdin and drain stdout on threads, so a full pipe never stalls the deadline loop.
    let mut stdin = child.stdin.take();
    let prompt = if backend == CoachBackend::Codex {
        format!("{instructions}\n\n{input}")
    } else {
        input.to_owned()
    };
    let writer = std::thread::spawn(move || {
        if let Some(mut stdin) = stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes());
        }
    });
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.take(4 * 1024 * 1024).read_to_end(&mut out);
        }
        out
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if stopping.load(Ordering::Acquire) || Instant::now() >= deadline {
            // Kill by our own handle, then reap it.
            let _ = child.kill();
            let _ = child.wait();
            // The helper threads end with the pipes; a grandchild holding one must not stall.
            return Err(if stopping.load(Ordering::Acquire) {
                Failure::Transport
            } else {
                Failure::Timeout
            });
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = writer.join();
    let out = reader.join().unwrap_or_default();
    // Account for reported usage even if the exit status or final plan is unusable.
    *spent = if backend == CoachBackend::Claude {
        serde_json::from_slice::<Value>(&out)
            .map(|v| claude_usage(&v))
            .unwrap_or_default()
    } else {
        let (tokens, cached_input_tokens) = codex_usage(&out);
        crate::TokenUsage {
            input_tokens: tokens[0],
            output_tokens: tokens[1],
            reasoning_tokens: tokens[2],
            cached_input_tokens,
        }
    };
    if !status.is_some_and(|s| s.success()) {
        return Err(Failure::CliFailed);
    }
    match backend {
        CoachBackend::Claude => parse_claude(&out),
        _ => {
            let text = std::fs::read_to_string(&answer_file).map_err(|_| Failure::InvalidAnswer)?;
            Ok(CliAnswer {
                plan: protocol::plan_from_text(&text)?,
                tokens: [
                    spent.input_tokens,
                    spent.output_tokens,
                    spent.reasoning_tokens,
                ],
                cached_tokens: spent.cached_input_tokens,
                cost_usd: None,
            })
        }
    }
}

/// Reads Claude Code's `--output-format json` envelope: `result` (text) or `structured_output`
/// (when a schema was given) hold the plan; `usage` is recorded as a plan proxy.
pub(crate) fn parse_claude(out: &[u8]) -> Result<CliAnswer, Failure> {
    let envelope: Value = serde_json::from_slice(out).map_err(|_| Failure::MalformedJson)?;
    if envelope["is_error"] == true {
        return Err(Failure::CliFailed);
    }
    let plan = match &envelope["structured_output"] {
        Value::Object(_) => GamePlan::from_json(&envelope["structured_output"].to_string())
            .map_err(|_| Failure::InvalidAnswer)?,
        _ => protocol::plan_from_text(envelope["result"].as_str().ok_or(Failure::InvalidAnswer)?)?,
    };
    let usage = claude_usage(&envelope);
    Ok(CliAnswer {
        plan,
        tokens: [usage.input_tokens, usage.output_tokens, 0],
        cost_usd: None,
        cached_tokens: usage.cached_input_tokens,
    })
}

fn claude_usage(envelope: &Value) -> crate::TokenUsage {
    let n = |k: &str| envelope["usage"][k].as_u64().unwrap_or(0);
    crate::TokenUsage {
        input_tokens: n("input_tokens")
            .saturating_add(n("cache_creation_input_tokens"))
            .saturating_add(n("cache_read_input_tokens")),
        output_tokens: n("output_tokens"),
        cached_input_tokens: n("cache_read_input_tokens"),
        reasoning_tokens: 0,
    }
}

/// Each completed turn has its own usage; item events and printed totals are not usage.
fn codex_usage(out: &[u8]) -> ([u64; 3], u64) {
    let mut tokens = [0; 3];
    let mut cached = 0;
    for line in out.split(|b| *b == b'\n') {
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if event["type"] != "turn.completed" {
            continue;
        }
        let u = &event["usage"];
        tokens[0] += u["input_tokens"].as_u64().unwrap_or(0);
        tokens[1] += u["output_tokens"].as_u64().unwrap_or(0);
        tokens[2] += u["reasoning_tokens"]
            .as_u64()
            .or_else(|| u["output_tokens_details"]["reasoning_tokens"].as_u64())
            .unwrap_or(0);
        cached += u["cached_input_tokens"].as_u64().unwrap_or(0);
    }
    (tokens, cached)
}

/// A TOML basic string, for a `-c key=value` override.
fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_usage_sums_completed_turns_and_ignores_other_events_and_text() {
        let stream = br#"{"type":"turn.started"}
{"type":"item.completed","usage":{"input_tokens":999999}}
{"type":"turn.completed","usage":{"input_tokens":10000,"cached_input_tokens":2000,"output_tokens":1000,"output_tokens_details":{"reasoning_tokens":600}}}
tokens used
11,000
{"type":"turn.completed","usage":{"input_tokens":500,"output_tokens":100,"reasoning_tokens":50}}
{"type":"turn.failed"}
"#;
        assert_eq!(codex_usage(stream), ([10500, 1100, 650], 2000));
        assert_eq!(codex_usage(b"unknown event\n"), ([0, 0, 0], 0));
    }
}
