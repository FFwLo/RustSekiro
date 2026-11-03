//! Recording presets always migrate and save in an isolated, retained temporary directory.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sekiro_sim::brain::memory::LayerPaths;

#[derive(Debug)]
pub struct Stage {
    pub brain: &'static str,
    pub memory: Option<PathBuf>,
}

pub fn load(name: &str, chr: &str, player: &str, datasets: &Path) -> Result<Stage> {
    if name == "lua" {
        return Ok(Stage {
            brain: "lua",
            memory: None,
        });
    }
    let dataset = match name {
        "early" => "owl-2026-10-11-golden",
        "alive" => "owl-2026-10-11-alive",
        "outsmarted" => "owl-2026-10-11-outsmarted",
        _ => bail!("unknown --brain-stage {name}; choose lua, early, alive or outsmarted"),
    };
    if chr != "c5060" {
        bail!("--brain-stage {name} requires Owl (--arena owl or --enemy owl)");
    }
    let source = datasets.join(dataset).join("c5060.json");
    let bytes = std::fs::read(&source).with_context(|| {
        format!(
            "--brain-stage {name}: missing or unreadable dataset {}",
            source.display()
        )
    })?;
    let dir = std::env::temp_dir().join(format!(
        "sekiro-brain-stage-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&dir)?;
    let legacy = dir.join("c5060.json");
    std::fs::write(&legacy, bytes)?;
    let default = LayerPaths::new(&legacy, "default");
    let (mut boss, profile) = default.load(chr).with_context(|| {
        format!(
            "--brain-stage {name}: invalid snapshot {}",
            source.display()
        )
    })?;
    // Reassign the snapshot to the chosen player without counting its evidence twice.
    if player != "default" {
        boss.players.remove("default");
        boss.update(player, &profile);
        std::fs::write(&default.boss, serde_json::to_vec_pretty(&boss)?)?;
    }
    LayerPaths::new(&legacy, player).save(&boss, &profile)?;
    eprintln!(
        "brain stage {name}: temporary memory {} (retained after exit)",
        dir.display()
    );
    Ok(Stage {
        brain: "coach",
        memory: Some(legacy),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_recording_snapshots_migrate_only_in_temporary_copies() {
        let datasets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../logs/datasets");
        for (name, folder) in [
            ("early", "golden"),
            ("alive", "alive"),
            ("outsmarted", "outsmarted"),
        ] {
            let source = datasets.join(format!("owl-2026-10-11-{folder}/c5060.json"));
            if !source.exists() {
                continue;
            }
            let bytes = std::fs::read(&source).unwrap();
            let original: sekiro_sim::brain::memory::Memory =
                serde_json::from_slice(&bytes).unwrap();
            let stage = load(name, "c5060", "recording", &datasets).unwrap();
            assert_eq!(stage.brain, "coach");
            let paths = LayerPaths::new(stage.memory.as_ref().unwrap(), "recording");
            let (boss, profile) = paths.load("c5060").unwrap();
            assert_eq!(boss.evidence.fights, original.fights);
            assert_eq!(boss.players.len(), 1);
            assert_eq!(profile.fights, original.fights);
            assert_eq!(profile.habits, original.habits);
            assert_eq!(profile.coach.plan, original.coach.plan);
            assert_eq!(std::fs::read(&source).unwrap(), bytes);
        }
    }

    #[test]
    fn lua_needs_no_dataset_and_bad_presets_fail() {
        let absent = Path::new("nonexistent-stage-datasets");
        assert!(
            load("lua", "c5060", "default", absent)
                .unwrap()
                .memory
                .is_none()
        );
        assert!(load("unknown", "c5060", "default", absent).is_err());
        assert!(load("early", "c1010", "default", absent).is_err());
        assert!(
            load("early", "c5060", "default", absent)
                .unwrap_err()
                .to_string()
                .contains("missing or unreadable")
        );
    }

    #[test]
    fn stage_migration_and_writes_leave_source_unchanged() {
        let root = std::env::temp_dir().join(format!(
            "sekiro-stage-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dataset = root.join("owl-2026-10-11-golden");
        std::fs::create_dir_all(&dataset).unwrap();
        let source = dataset.join("c5060.json");
        let mut memory = sekiro_sim::brain::memory::Memory::new("c5060");
        memory.fights = 17;
        memory.habits.jumps = 42;
        memory.save(&source).unwrap();
        let before = std::fs::read(&source).unwrap();
        let stage = load("early", "c5060", "clip", &root).unwrap();
        let paths = LayerPaths::new(stage.memory.as_ref().unwrap(), "clip");
        let (boss, mut profile) = paths.load("c5060").unwrap();
        assert_eq!(boss.evidence.fights, 17);
        assert_eq!(boss.players.len(), 1);
        assert_eq!(profile.fights, 17);
        assert_eq!(profile.habits.jumps, 42);
        profile.fights += 1;
        paths.save(&boss, &profile).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert_eq!(std::fs::read_dir(&dataset).unwrap().count(), 1);
    }
}
