//! `sekiro-rs`: the Bevy app.
//!
//! Modes:
//! - `sekiro-rs --sweep --map <id> [--spacing 80] [--views 4] [--objects]`:
//!   offscreen visual-completeness survey; see `docs/MAP-COMPLETENESS-SWEEP.md`.
//! - `sekiro-rs --viewer <file.glb> [--screenshot <out.png>] [--yaw <deg>] [--pitch <deg>]
//!   [--distance <m>] [--height <m>] [--hide m12,hair] [--only m68,m69]`: inspect an exported
//!   model in bind pose; `--hide`/`--only` filter primitives by material name, `--combat` shows
//!   enemies with weapons drawn.
//! - `sekiro-rs --arena owl|genichiro [--full-map] [--script FILE ...]`: Wolf against a boss where the
//!   game places him (`sekiro_sim::arena`); a play mode, see `play.rs`.
//! - `sekiro-rs --map <id> [--screenshot <out.png>] [--at <s>] [--camera x,y,z,yaw,pitch]
//!   [--start <n>] [--collision] [--no-pieces] [--objects] [--hide m9,m8]
//!   [--groups draw|display|off] [--uncapped]`: fly through an area exported by
//!   `sekiro-extract map`.
//!
//! Map lighting: `--story NAME` selects map event states; `--hour H` overrides their clock.
//! `--no-env-probe` for hemisphere fallback,
//! `--probe-sky` to show the environment probe capture as the sky (debug only).
//! `--no-grading`, `--no-bloom`, `--no-auto-exposure` disable individual map post effects.
//! `--native-tone-reinhard` / `--native-tone-filmic` override the authored/default sensitometric operator.
//! `--visual-stats` logs periodic map frame times in viewer and play modes.
//! `--perf` logs VRAM and asset bytes every five seconds; `--perf-seconds N` self-exits.
//! `--vram-budget GB` simulates a smaller card (GiB), reserving 20% for headroom.
//! `--profile-systems` also logs map-system CPU timers and render-schedule/pass diagnostics.
//! `--no-fxr` disables authored FXR playback and keeps the existing effect stand-ins.
//! `--no-cloth` disables the Havok cloth simulation (characters keep their rigid skin).
//! `--fxr-preview <id>` repeats an installed effect between the actors every two seconds.

mod arena;
mod arena_b;
mod audio;
mod brain_overlay;
mod brain_stage;
mod bullet_fx;
mod change;
mod character;
mod cloth;
mod combat_hud;
mod death_screen;
mod deathblow_demo;
mod decals;
mod draw_mask;
mod fxr;
mod fxr_tree;
mod game_ui;
mod input_replay;
pub mod lighting;
mod lighting_selection;
mod lightning_fx;
pub mod map;
mod map_fog_surface;
mod map_fx;
mod map_grass;
mod material_residency;
mod menu;
mod mesh_residency;
mod perf_log;
mod pipeline_warmup;
mod play;
mod presentation;
#[cfg(feature = "profile-spans")]
mod span_profile;
mod split_sun_cascades;
mod texture_residency;
mod viewer;
mod vram;

use std::path::PathBuf;

/// Controller construction is deferred until play, so menu validation and tests
/// never start workers or open hosted telemetry files.
pub struct BrainOptions {
    name: String,
    model: Option<String>,
    chr: String,
    coach: sekiro_brain_net::CoachCli,
}
impl BrainOptions {
    fn build(self) -> anyhow::Result<Box<dyn sekiro_sim::brain::BossBrain>> {
        sekiro_brain_net::from_cli_with(&self.name, self.model, &self.chr, &self.coach)
            .map_err(|e| anyhow::anyhow!("--brain: {e}"))
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn arg_list(args: &[String], name: &str) -> Vec<String> {
    arg_value(args, name)
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default()
}

fn arg_f32(args: &[String], name: &str, default: f32) -> anyhow::Result<f32> {
    match arg_value(args, name) {
        Some(v) => v.parse().map_err(|e| anyhow::anyhow!("{name} {v}: {e}")),
        None => Ok(default),
    }
}

fn dispatch() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    vram::validate_args(&args)?;
    let cache = arg_value(&args, "--cache").map(PathBuf::from);
    if args.iter().any(|a| a == "--cache")
        && cache
            .as_ref()
            .is_none_or(|v| v.as_os_str().to_string_lossy().starts_with("--"))
    {
        anyhow::bail!("--cache requires a directory");
    }
    sekiro_sim::cache::init(cache.as_deref());
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--sweep" | "--render-gate" | "--reference-gate"))
    {
        return map::sweep::run(&args);
    }
    if args.len() == 1 || args.iter().any(|a| a == "--menu") {
        return menu::run(&args);
    }
    for flag in [
        "--attack-power",
        "--progression",
        "--brain-stage",
        "--window-size",
        "--hour",
        "--story",
        "--present-mode",
    ] {
        if args.iter().any(|a| a == flag)
            && arg_value(&args, flag).is_none_or(|v| v.starts_with("--"))
        {
            anyhow::bail!("{flag} requires a value");
        }
    }
    if let Some(mode) = arg_value(&args, "--present-mode") {
        anyhow::ensure!(
            matches!(mode.as_str(), "immediate" | "mailbox" | "fifo"),
            "--present-mode wants immediate, mailbox or fifo"
        );
    }
    let _attack_power = sekiro_sim::progression::AttackPower::from_options(
        arg_value(&args, "--attack-power").as_deref(),
        arg_value(&args, "--progression").as_deref(),
    )
    .map_err(anyhow::Error::msg)?;
    if let Some(glb) = arg_value(&args, "--viewer") {
        let options = viewer::Options {
            glb: PathBuf::from(glb),
            screenshot: arg_value(&args, "--screenshot").map(PathBuf::from),
            yaw_degrees: arg_f32(&args, "--yaw", 180.0)?,
            pitch_degrees: arg_f32(&args, "--pitch", 8.0)?,
            distance: arg_f32(&args, "--distance", 3.4)?,
            height: arg_f32(&args, "--height", 0.95)?,
            hide: arg_list(&args, "--hide"),
            only: arg_list(&args, "--only"),
            combat: args.iter().any(|a| a == "--combat"),
        };
        return viewer::run(options);
    }
    if let Some(id) = arg_value(&args, "--map")
        && !args.iter().any(|a| a == "--play")
    {
        return map::run(map::Options {
            cache: sekiro_sim::cache::root(),
            id,
            screenshot: arg_value(&args, "--screenshot").map(PathBuf::from),
            at: arg_f32(&args, "--at", 2.0)?,
            camera: map::parse_camera(arg_value(&args, "--camera").as_deref())?,
            collision: args.iter().any(|a| a == "--collision"),
            no_pieces: args.iter().any(|a| a == "--no-pieces"),
            start: arg_f32(&args, "--start", 0.0)? as usize,
            hide: arg_list(&args, "--hide"),
            uncapped: args.iter().any(|a| a == "--uncapped"),
            groups: map::GroupMode::parse(arg_value(&args, "--groups").as_deref())?,
            objects: args.iter().any(|a| a == "--objects"),
            hour: arg_f32(&args, "--hour", f32::NAN)?,
            gparam: arg_value(&args, "--gparam"),
        });
    }
    // `--arena <name>` (a boss arena, see `sekiro_sim::arena`) is a play mode; a bare `--arena`
    // is the presentation demo below.
    let boss_arena = match arg_value(&args, "--arena").filter(|v| !v.starts_with("--")) {
        Some(name) => Some(
            sekiro_sim::arena::ArenaSpec::by_name(&name)
                .map_err(|e| anyhow::anyhow!("--arena: {e}"))?,
        ),
        None => None,
    };
    if args.iter().any(|a| a == "--play") || boss_arena.is_some() {
        return play::run(play_options(&args)?);
    }
    if args.iter().any(|a| a == "--arena") {
        return arena::run(arena::Options {
            cache: sekiro_sim::cache::root(),
            screenshot: arg_value(&args, "--screenshot").map(PathBuf::from),
            at: arg_f32(&args, "--at", 3.0)?,
            deathblow: args.iter().any(|a| a == "--deathblow"),
        });
    }
    eprintln!(
        "usage: sekiro-rs --play [--attack-power 1..99] [--progression castle-roof] [--script FILE|builtin] [--shots step:path,...] [--exit]"
    );
    eprintln!(
        "                 [--map <id> [--start <n> | --spawn x,y,z,yaw,0 | --at-idol <entity>]] [--kit <json>] [--no-enemy] [--lock-on]"
    );
    eprintln!(
        "                 [--checkpoint local|decisions|off] [--brain-moves local|off] [--brain-move-every 1|2|3] [--coach-review-every <n>] (local defaults; review 5, 0 disables)"
    );
    eprintln!(
        "                 [--simple-ai] [--passive] [--god] [--immortal] [--force-act <id>] [--enemy cXXXX[:npcrow]|soldier|owl] [--brain lua|scripted|decisions|responses|coach] [--brain-model <name>] [--coach claude|codex|api] [--coach-model <name>] [--coach-effort <level>] [--coach-interval <secs>] [--brain-log] [--brain-memory off|on|<path>] [--player <name>]"
    );
    eprintln!(
        "       sekiro-rs --arena owl|genichiro [--full-map] [--genichiro-phase ashina|tomoe] [--play options] (a boss on his own map)"
    );
    eprintln!(
        "                 [--brain-overlay] [--brain-stage lua|early|alive|outsmarted] [--window-size WIDTHxHEIGHT] (F8 toggles overlay)"
    );
    eprintln!("usage: sekiro-rs --arena [--deathblow] [--screenshot <out.png> --at <seconds>]");
    eprintln!("       sekiro-rs --sweep --map <id> [--spacing 80] [--views 4] [--objects]");
    eprintln!("       sekiro-rs --map <id> [--start <n>] [--camera x,y,z,yaw,pitch] [--collision]");
    eprintln!("                 [--screenshot <out.png> --at <seconds>] (see src/map.rs)");
    eprintln!("       sekiro-rs --viewer <file.glb> [--screenshot <out.png>] [--yaw <deg>]");
    eprintln!("                 [--pitch <deg>] [--distance <m>] [--height <m>]");
    std::process::exit(2);
}

fn play_options(args: &[String]) -> anyhow::Result<play::Options> {
    play_options_with(args, std::path::Path::new("logs/datasets"))
}

fn play_options_with(args: &[String], datasets: &std::path::Path) -> anyhow::Result<play::Options> {
    let cache =
        sekiro_sim::cache::resolve(arg_value(args, "--cache").map(PathBuf::from).as_deref());
    let attack_power = sekiro_sim::progression::AttackPower::from_options(
        arg_value(args, "--attack-power").as_deref(),
        arg_value(args, "--progression").as_deref(),
    )
    .map_err(anyhow::Error::msg)?;
    let boss_arena = arg_value(args, "--arena")
        .filter(|v| !v.starts_with("--"))
        .map(|name| sekiro_sim::arena::ArenaSpec::by_name(&name).map_err(anyhow::Error::msg))
        .transpose()?;
    // Without a map the fight is on the flat floor, which has no player starts, idols or
    // coordinates to spawn at: say so instead of ignoring the flag.
    let map_id = arg_value(args, "--map");
    for flag in ["--start", "--at-idol"] {
        anyhow::ensure!(
            map_id.is_some() || !args.iter().any(|a| a == flag),
            "{flag} needs --map <id>"
        );
    }
    anyhow::ensure!(
        map_id.is_some() || boss_arena.is_some() || !args.iter().any(|a| a == "--spawn"),
        "--spawn needs --map <id> or --arena <name>"
    );
    let screenshots = arg_list(args, "--shots")
        .into_iter()
        .map(|s| {
            let (step, path) = s
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("--shots wants step:path, got {s}"))?;
            Ok((step.parse()?, PathBuf::from(path)))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let phase_enemy = match arg_value(args, "--genichiro-phase").as_deref() {
        None | Some("ashina") => None,
        Some("tomoe") => Some("c7110:71100000".to_owned()),
        Some(_) => anyhow::bail!("--genichiro-phase wants ashina or tomoe"),
    };
    let mut opponent = match (
        phase_enemy.or_else(|| arg_value(args, "--enemy")),
        boss_arena,
    ) {
        (Some(name), _) => sekiro_sim::enemy::EnemySpec::from_cache(&cache, &name)
            .map_err(|e| anyhow::anyhow!("--enemy: {e}"))?,
        (None, arena) => sekiro_sim::enemy::EnemySpec::from_cache(
            &cache,
            arena.map_or("soldier", |arena| arena.enemy),
        )
        .map_err(|e| anyhow::anyhow!("--enemy: {e}"))?,
    };
    if let Some(arena) = boss_arena {
        sekiro_sim::arenas_a::select_placed_think(arena, &mut opponent);
    }
    let player = sekiro_sim::brain::memory::player_name(
        &arg_value(args, "--player").unwrap_or_else(|| "default".into()),
    )
    .map_err(anyhow::Error::msg)?;
    let stage = arg_value(args, "--brain-stage")
        .map(|name| brain_stage::load(&name, &opponent.chr, &player, datasets))
        .transpose()?;
    let brain_name = stage
        .as_ref()
        .map(|s| s.brain.to_string())
        .unwrap_or_else(|| arg_value(args, "--brain").unwrap_or_else(|| "lua".into()));
    let coach_backend = arg_value(args, "--coach").unwrap_or_else(|| "codex".into());
    let checkpoint = arg_value(args, "--checkpoint").unwrap_or_else(|| "local".into());
    let brain_moves = arg_value(args, "--brain-moves").unwrap_or_else(|| "local".into());
    anyhow::ensure!(
        matches!(brain_moves.as_str(), "local" | "off"),
        "--brain-moves wants local or off"
    );
    let review_every = arg_value(args, "--coach-review-every")
        .unwrap_or_else(|| sekiro_sim::brain::move_bandit::REVIEW_EVERY.to_string())
        .parse::<u32>()?;
    let move_every = arg_value(args, "--brain-move-every")
        .unwrap_or_else(|| sekiro_sim::brain::move_bandit::MOVE_EVERY.to_string())
        .parse::<u32>()?;
    anyhow::ensure!(
        (1..=3).contains(&move_every),
        "--brain-move-every wants 1, 2 or 3"
    );
    anyhow::ensure!(
        sekiro_sim::brain::bandit::CheckpointChooser::by_name(&checkpoint).is_some(),
        "unknown --checkpoint {checkpoint}: known are local, decisions, off"
    );
    if sekiro_brain_net::is_hosted(&brain_name) {
        anyhow::ensure!(
            sekiro_brain_net::CoachBackend::by_name(&coach_backend).is_some(),
            "unknown --coach {coach_backend}"
        );
    } else {
        sekiro_sim::brain::BrainKind::by_name(&brain_name)
            .map_err(|e| anyhow::anyhow!("--brain: {e}"))?;
    }
    let brain_label = if brain_name.eq_ignore_ascii_case("coach") {
        let backend = sekiro_brain_net::CoachBackend::by_name(&coach_backend)
            .ok_or_else(|| anyhow::anyhow!("unknown --coach {coach_backend}"))?;
        format!(
            "coach - {}",
            arg_value(args, "--coach-model").unwrap_or_else(|| backend.default_model().into())
        )
    } else {
        brain_name.clone()
    };
    let window_size = arg_value(args, "--window-size")
        .map(|s| -> anyhow::Result<(u32, u32)> {
            let (w, h) = s
                .split_once('x')
                .ok_or_else(|| anyhow::anyhow!("--window-size wants WIDTHxHEIGHT"))?;
            let size = (w.parse()?, h.parse()?);
            anyhow::ensure!(
                size.0 >= 800 && size.1 >= 600,
                "--window-size minimum is 800x600"
            );
            Ok(size)
        })
        .transpose()?
        .unwrap_or((1600, 900));
    // Sekiro's time of day follows the story, not a clock: an arena defaults to the hour of the
    // light, probes and sky the game's own flags select at that point (`--hour` overrides).
    let default_hour = match boss_arena {
        Some(arena)
            if arg_value(args, "--hour").is_none() && arg_value(args, "--story").is_none() =>
        {
            let time = arena.story_time(&opponent);
            eprintln!(
                "arena {}: story time {} ({} h), set --hour to override",
                arena.name,
                time.name(),
                time.hour()
            );
            time.hour()
        }
        _ => f32::NAN,
    };
    Ok(play::Options {
        map_enemies: args.iter().any(|a| a == "--map-enemies"),
        brain_overlay: args.iter().any(|a| a == "--brain-overlay"),
        brain_label,
        window_size,
        attack_power,
        cache,
        script: arg_value(args, "--script"),
        input: arg_value(args, "--input"),
        screenshots,
        exit_after_script: args.iter().any(|a| a == "--exit"),
        enemy: !args.iter().any(|a| a == "--no-enemy" || a == "--passive"),
        enemy_present: !args
            .iter()
            .any(|a| a == "--no-enemy" || a == "--map-enemies"),
        arena: boss_arena,
        full_map: args.iter().any(|a| a == "--full-map"),
        hide: arg_list(args, "--hide"),
        objects: args.iter().any(|a| a == "--objects"),
        lock_on: args.iter().any(|a| a == "--lock-on"),
        simple_ai: args.iter().any(|a| a == "--simple-ai"),
        god: args.iter().any(|a| a == "--god"),
        immortal: args.iter().any(|a| a == "--immortal"),
        force_act: arg_value(args, "--force-act")
            .map(|v| v.parse())
            .transpose()
            .map_err(|e| anyhow::anyhow!("--force-act wants an action id: {e}"))?,
        break_posture: args.iter().any(|a| a == "--break-posture"),
        brain: BrainOptions {
            name: brain_name.clone(),
            model: arg_value(args, "--brain-model"),
            chr: opponent.chr.clone(),
            coach: sekiro_brain_net::CoachCli {
                checkpoint: Some(checkpoint),
                budget_usd: arg_value(args, "--brain-budget")
                    .map(|v| v.parse::<f64>())
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("--brain-budget wants USD: {e}"))?,
                token_budget: arg_value(args, "--brain-token-budget")
                    .map(|v| v.parse::<u64>())
                    .transpose()
                    .map_err(|e| {
                        anyhow::anyhow!("--brain-token-budget wants a token count: {e}")
                    })?,
                backend: arg_value(args, "--coach"),
                model: arg_value(args, "--coach-model"),
                effort: arg_value(args, "--coach-effort"),
            },
        },
        coach_interval: arg_f32(args, "--coach-interval", 30.0)?.max(0.0),
        brain_moves: brain_moves == "local",
        brain_move_every: move_every,
        coach_review_every: review_every,
        player,
        brain_log: args.iter().any(|a| a == "--brain-log"),
        brain_memory: if let Some(stage) = stage {
            stage.memory
        } else {
            sekiro_sim::brain::memory::resolve_for(
                arg_value(args, "--brain-memory").as_deref(),
                &opponent.chr,
                &brain_name.to_ascii_lowercase(),
                sekiro_brain_net::is_hosted(&brain_name),
            )
        },
        opponent,
        map: map_id,
        start: arg_f32(args, "--start", 0.0)? as usize,
        spawn: map::parse_camera(arg_value(args, "--spawn").as_deref())?
            .map(|[x, y, z, yaw, _]| [x, y, z, yaw]),
        enemy_distance: arg_f32(args, "--enemy-distance", 5.0)?,
        hour: arg_f32(args, "--hour", default_hour)?,
        gparam: arg_value(args, "--gparam"),
        at_idol: arg_value(args, "--at-idol")
            .map(|v| v.parse())
            .transpose()
            .map_err(|e| anyhow::anyhow!("--at-idol wants an idol entity id: {e}"))?,
        kit: arg_value(args, "--kit")
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(|e| anyhow::anyhow!("--kit wants a kit as JSON: {e}"))?,
    })
}

/// When this process started, for load timings measured from launch.
fn process_start() -> std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
}

/// Seconds since this process started.
fn since_start() -> f32 {
    process_start().elapsed().as_secs_f32()
}

pub fn run() -> anyhow::Result<()> {
    process_start();
    dispatch()?;
    menu::relaunch()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Map-only placement flags without `--map` used to be ignored silently on the flat floor.
    #[test]
    fn placement_flags_without_a_map_fail_clearly() {
        for (extra, message) in [
            (&["--start", "3"][..], "--start needs --map <id>"),
            (&["--at-idol", "1111950"], "--at-idol needs --map <id>"),
            (
                &["--spawn", "0,0,0,0,0"],
                "--spawn needs --map <id> or --arena <name>",
            ),
        ] {
            let args: Vec<String> = ["sekiro-rs", "--play"]
                .iter()
                .chain(extra)
                .map(|s| s.to_string())
                .collect();
            let error = super::play_options_with(&args, std::path::Path::new("logs/datasets"))
                .err()
                .map(|e| e.to_string());
            assert_eq!(error.as_deref(), Some(message), "{extra:?}");
        }
    }
}
