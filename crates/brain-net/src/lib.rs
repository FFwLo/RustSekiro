//! Hosted boss choices. Only the worker threads perform HTTP or run CLIs; the simulation never
//! leaves its thread.

mod cli;
mod config;
mod prices;
mod protocol;
mod worker;

pub use config::{ApiKey, Backend, CoachBackend, ConfigError, HostedConfig};
pub use prices::PriceTable;
pub use worker::{CoachStats, Failure, HostedBrain, HostedStats, StatsHandle, TokenUsage};

use sekiro_sim::brain::{BossActions, BossBrain, BrainKind, LuaBrain};

/// Whether `--brain <name>` is a hosted model (which remembers across sessions by default).
pub fn is_hosted(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "decisions" | "responses" | "coach"
    )
}

/// The coach options of the game CLI (`--coach`, `--coach-model`, `--coach-effort`).
#[derive(Debug, Clone, Default)]
pub struct CoachCli {
    pub checkpoint: Option<String>,
    pub budget_usd: Option<f64>,
    pub token_budget: Option<u64>,
    /// `codex` (default), `claude` or `api`.
    pub backend: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl CoachCli {
    fn resolved_backend(&self) -> Result<CoachBackend, String> {
        match self.backend.as_deref() {
            None => Ok(CoachBackend::Codex),
            Some(b) => CoachBackend::by_name(b)
                .ok_or_else(|| format!("unknown --coach {b}: known are claude, codex, api")),
        }
    }
}

/// Game CLI boundary for the enemy `chr` (the hosted brain offers that enemy's actions).
/// Configuration or worker startup failure leaves the Lua AI in control. Constructing a
/// controller does not send a request; only `submit` does.
pub fn from_cli(
    name: &str,
    model: Option<String>,
    chr: &str,
) -> Result<Box<dyn BossBrain>, String> {
    from_cli_with(name, model, chr, &CoachCli::default())
}

/// Like [`from_cli`], with the coach options for `--brain coach`.
pub fn from_cli_with(
    name: &str,
    model: Option<String>,
    chr: &str,
    coach: &CoachCli,
) -> Result<Box<dyn BossBrain>, String> {
    let backend = match name.to_ascii_lowercase().as_str() {
        "decisions" => Backend::Decisions,
        "responses" => Backend::Responses,
        "coach" => Backend::Coach,
        _ => {
            return BrainKind::by_name(name)
                .map(|kind| kind.build(chr))
                .map_err(|_| {
                    "unknown brain: known are lua, scripted, decisions, responses, coach".into()
                });
        }
    };
    let coach_backend = coach.resolved_backend()?;
    let checkpoint = sekiro_sim::brain::bandit::CheckpointChooser::by_name(
        coach.checkpoint.as_deref().unwrap_or("local"),
    )
    .ok_or_else(|| "unknown --checkpoint: known are local, decisions, off".to_string())?;
    let Some(actions) = BossActions::for_chr(chr) else {
        eprintln!("--brain {backend}: no action list for {chr}; falling back to Lua AI");
        return Ok(Box::new(LuaBrain));
    };
    let result = checkpoint_config(backend, coach_backend, checkpoint, || {
        HostedConfig::from_env(backend)
    })
    .and_then(|mut config| {
        config.checkpoint_chooser = checkpoint;
        if backend == Backend::Coach {
            config.checkpoints =
                checkpoint == sekiro_sim::brain::bandit::CheckpointChooser::Decisions;
        }
        config.budget_usd = coach.budget_usd;
        config.token_budget = coach.token_budget;
        config.prices = PriceTable::load()?;
        if let Some(model) = model {
            config.model = model;
        }
        if backend == Backend::Coach {
            config.coach_backend = coach_backend;
            config.coach_model = coach
                .model
                .clone()
                .unwrap_or_else(|| coach_backend.default_model().to_string());
            if let Some(effort) = &coach.effort {
                config.coach_effort = effort.clone();
            } else if coach_backend == CoachBackend::Codex {
                config.coach_effort = "medium".into();
            } else {
                config.coach_effort = "low".into();
            }
            config.coach_timeout = coach_backend.timeout();
            config.coach_cmd = std::env::var_os("SEKIRO_COACH_CMD").map(Into::into);
            eprintln!(
                "coach: {} (model {}{}), checkpoints on {}",
                coach_backend.name(),
                config.coach_model,
                if coach_backend != CoachBackend::Claude {
                    format!(", effort {}", config.coach_effort)
                } else {
                    String::new()
                },
                match checkpoint {
                    sekiro_sim::brain::bandit::CheckpointChooser::Local => "local",
                    sekiro_sim::brain::bandit::CheckpointChooser::Off => "off",
                    sekiro_sim::brain::bandit::CheckpointChooser::Decisions => &config.model,
                }
            );
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        config.log_path = Some(format!("logs/brain-{stamp}.jsonl").into());
        HostedBrain::new(config, actions)
    });
    Ok(fallback(backend, result))
}

fn checkpoint_config(
    backend: Backend,
    coach: CoachBackend,
    checkpoint: sekiro_sim::brain::bandit::CheckpointChooser,
    credentials: impl FnOnce() -> Result<HostedConfig, ConfigError>,
) -> Result<HostedConfig, ConfigError> {
    if backend != Backend::Coach
        || coach == CoachBackend::Api
        || checkpoint == sekiro_sim::brain::bandit::CheckpointChooser::Decisions
    {
        credentials()
    } else {
        Ok(HostedConfig::new(backend, ApiKey::new("")))
    }
}

fn fallback(backend: Backend, result: Result<HostedBrain, ConfigError>) -> Box<dyn BossBrain> {
    match result {
        Ok(brain) => Box::new(brain),
        Err(error) => {
            eprintln!("--brain {backend}: {error}; falling back to Lua AI");
            Box::new(LuaBrain)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_and_off_cli_coaches_never_read_credentials() {
        use sekiro_sim::brain::bandit::CheckpointChooser;
        for coach in [CoachBackend::Codex, CoachBackend::Claude] {
            for checkpoint in [CheckpointChooser::Local, CheckpointChooser::Off] {
                let config = checkpoint_config(Backend::Coach, coach, checkpoint, || {
                    panic!("credential access")
                })
                .unwrap();
                assert_eq!(config.backend, Backend::Coach);
            }
        }
        assert_eq!(
            CheckpointChooser::by_name("decisions"),
            Some(CheckpointChooser::Decisions)
        );
        assert!(matches!(
            checkpoint_config(
                Backend::Coach,
                CoachBackend::Codex,
                CheckpointChooser::Decisions,
                || Err(ConfigError::MissingKey)
            ),
            Err(ConfigError::MissingKey)
        ));
        assert!(matches!(
            checkpoint_config(
                Backend::Coach,
                CoachBackend::Api,
                CheckpointChooser::Local,
                || Err(ConfigError::MissingKey)
            ),
            Err(ConfigError::MissingKey)
        ));
    }

    #[test]
    fn game_cli_defaults_to_codex() {
        assert_eq!(
            CoachCli::default().resolved_backend().unwrap(),
            CoachBackend::Codex
        );
    }

    #[test]
    fn missing_key_falls_back_without_opening_real_credential_sources() {
        for backend in [Backend::Decisions, Backend::Responses, Backend::Coach] {
            let brain = fallback(backend, Err(ConfigError::MissingKey));
            assert_eq!(brain.name(), "lua");
            assert!(!brain.plans());
        }
    }
}
