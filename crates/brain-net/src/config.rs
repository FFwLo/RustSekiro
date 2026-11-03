use std::fmt;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Duration;

/// Never expose credentials through Debug, Display, errors or telemetry.
pub struct ApiKey(pub(crate) String);

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Decisions,
    Responses,
    /// A coach between fights (Responses, reasoning, structured game plan) and the Decisions
    /// API choosing among the plan's sets at the checkpoints.
    Coach,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Decisions => "decisions",
            Self::Responses => "responses",
            Self::Coach => "coach",
        }
    }

    /// The endpoint of the per-request calls (the coach's own call is always Responses).
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Decisions | Self::Coach => "decisions",
            Self::Responses => "responses",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub struct HostedConfig {
    pub checkpoint_chooser: sekiro_sim::brain::bandit::CheckpointChooser,
    pub budget_usd: Option<f64>,
    pub token_budget: Option<u64>,
    pub prices: crate::PriceTable,
    pub backend: Backend,
    pub model: String,
    pub base_url: String,
    pub timeout: Duration,
    pub log_path: Option<PathBuf>,
    /// Resolve the host and open (then close) a TCP connection when the worker starts, so the
    /// first decision does not pay for the DNS lookup. Sends nothing.
    pub warm_up: bool,
    pub api_key: ApiKey,
    /// Coach backend: the reasoning model, its effort, the deadline of one coach call and the
    /// output cap (reasoning tokens count against it).
    pub coach_model: String,
    pub coach_effort: String,
    pub coach_timeout: Duration,
    pub coach_max_output_tokens: u32,
    /// How the coach is run (`--coach`): through a subscription CLI or the Responses API.
    pub coach_backend: CoachBackend,
    /// Overrides the CLI program of the `claude` and `codex` coaches (`SEKIRO_COACH_CMD`; the
    /// tests point it at a fake).
    pub coach_cmd: Option<PathBuf>,
    /// Legacy veto for explicit Decisions checkpoint calls; local/off never sends HTTP.
    pub checkpoints: bool,
}

/// Who writes the game plan between fights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoachBackend {
    /// Claude Code headless (`claude -p`), on the user's subscription.
    Claude,
    /// Codex CLI (`codex exec`), on the user's subscription.
    Codex,
    /// The Responses API with an API key.
    Api,
}

impl CoachBackend {
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "api" => Some(Self::Api),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Api => "api",
        }
    }

    /// The model used when `--coach-model` is not given.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Claude => "claude-haiku-5-5",
            Self::Codex => "gpt-6.1-sol",
            Self::Api => "gpt-6-luna",
        }
    }

    /// The deadline of one coach call.
    pub fn timeout(self) -> Duration {
        match self {
            Self::Claude => Duration::from_secs(120),
            Self::Codex => Duration::from_secs(60),
            Self::Api => Duration::from_secs(45),
        }
    }
}

impl fmt::Debug for HostedConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Public configuration can accidentally contain the credential too.
        f.debug_struct("HostedConfig")
            .field("backend", &self.backend)
            .field("model", &self.redact(&self.model))
            .field("base_url", &self.redact(&self.base_url))
            .field("timeout", &self.timeout)
            .field(
                "log_path",
                &self
                    .log_path
                    .as_ref()
                    .map(|p| self.redact(&p.to_string_lossy())),
            )
            .field("api_key", &self.api_key)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    MissingKey,
    InvalidConfig,
    LogOpen,
    WorkerStart,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingKey => "OPENAI_API_KEY is missing; set it in the environment or working directory .env",
            Self::InvalidConfig => "invalid hosted configuration: use a nonempty model, HTTP(S) base URL, positive finite timeout, and a nonempty key without control characters",
            Self::LogOpen => "cannot open hosted brain JSONL log",
            Self::WorkerStart => "cannot start hosted brain worker",
        })
    }
}

impl std::error::Error for ConfigError {}

impl HostedConfig {
    pub fn new(backend: Backend, api_key: ApiKey) -> Self {
        Self {
            checkpoint_chooser: sekiro_sim::brain::bandit::CheckpointChooser::Local,
            budget_usd: None,
            token_budget: None,
            prices: crate::PriceTable::default(),
            backend,
            model: "gpt-6-luna".into(),
            base_url: "https://api.openai.com".into(),
            timeout: Duration::from_millis(3000),
            log_path: None,
            warm_up: true,
            api_key,
            coach_model: "gpt-6.1-sol".into(),
            coach_effort: "medium".into(),
            coach_timeout: Duration::from_secs(45),
            coach_max_output_tokens: 6000,
            coach_backend: CoachBackend::Codex,
            coach_cmd: None,
            checkpoints: true,
        }
    }

    /// Reads no other dotenv settings. Never includes file contents in an error.
    pub fn from_env(backend: Backend) -> Result<Self, ConfigError> {
        let key = read_key(std::env::var("OPENAI_API_KEY").ok(), || {
            std::fs::File::open(".env")
                .ok()
                .and_then(|file| key_from_dotenv(BufReader::new(file)))
        })?;
        Ok(Self::new(backend, ApiKey::new(key)))
    }

    pub(crate) fn redact(&self, value: &str) -> String {
        if self.api_key.0.is_empty() {
            value.to_owned()
        } else {
            value.replace(&self.api_key.0, "***")
        }
    }
}

fn read_key(
    env: Option<String>,
    dotenv: impl FnOnce() -> Option<String>,
) -> Result<String, ConfigError> {
    env.filter(|key| !key.trim().is_empty())
        .or_else(dotenv)
        .ok_or(ConfigError::MissingKey)
}

pub(crate) fn key_from_dotenv(reader: impl BufRead) -> Option<String> {
    for line in reader.lines() {
        let line = line.ok()?;
        if let Some(value) = line.trim().strip_prefix("OPENAI_API_KEY=") {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_sources_are_lazy_and_dotenv_reads_only_the_named_line() {
        let fake = "test-key-not-real";
        let key = read_key(Some(fake.into()), || {
            panic!("dotenv must not open with an environment key")
        })
        .unwrap();
        assert!(key == fake);
        for text in [
            "OTHER=ignored\nOPENAI_API_KEY=test-key-not-real\nOTHER=ignored\n",
            "OPENAI_API_KEY=\"test-key-not-real\"\n",
            "OPENAI_API_KEY='test-key-not-real'\n",
        ] {
            assert!(read_key(None, || key_from_dotenv(text.as_bytes())).unwrap() == fake);
        }
        let error = read_key(None, || key_from_dotenv(b"OTHER=ignored\n".as_slice())).unwrap_err();
        assert_eq!(error, ConfigError::MissingKey);
        assert!(!error.to_string().contains(fake));
        assert_eq!(key_from_dotenv(b"OPENAI_API_KEY=\"\"\n".as_slice()), None);
        assert_eq!(
            key_from_dotenv(b"# OPENAI_API_KEY=ignored\n".as_slice()),
            None
        );
    }
}
