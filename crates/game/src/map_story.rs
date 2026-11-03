//! Offline story input for the single map visibility decision.
//!
//! `load(cache, map, layout)` returns None outside Outskirts/Castle, preserving old maps.
//! `enabled(entity_id, name)` supplies the story gate for each real part.
//! Visibility combines this gate with masks/residency; this module never writes Visibility.
//! Named reference progression adds known boss defeats to the first-visit fixture.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use sekiro_sim::world_events::{self, PartState, WorldState};
use serde_json::Value;

pub const REFERENCE_STORY: &str = "genichiro-defeated";

#[derive(Debug, Resource, Default)]
pub struct MapStory {
    pub maps: HashMap<String, StoryMap>,
}

#[derive(Debug)]
pub struct StoryMap {
    pub state: WorldState,
    pub parts: Vec<PartState>,
    by_name: HashMap<String, usize>,
}

pub fn selection(args: &[String]) -> Option<String> {
    crate::arg_value(args, "--story").or_else(|| {
        args.iter()
            .any(|a| a == "--render-gate" || a == "--reference-gate")
            .then(|| REFERENCE_STORY.into())
    })
}

impl StoryMap {
    pub fn enabled(&self, entity_id: i32, name: &str) -> Option<bool> {
        let part = self.part(name)?;
        (part.entity_id == entity_id).then_some(part.enabled)
    }

    pub fn part(&self, name: &str) -> Option<&PartState> {
        self.by_name.get(name).map(|&i| &self.parts[i])
    }

    /// The story has switched this part off (unknown parts stay on).
    pub fn switched_off(&self, name: &str) -> bool {
        self.part(name).is_some_and(|p| !p.enabled)
    }
}

pub(crate) fn initial_state(
    cache: &Path,
    map: &str,
    story: Option<&str>,
) -> Result<Option<WorldState>> {
    let Some(mut fixture) = world_events::FirstVisit::for_map(map) else {
        return Ok(None);
    };
    if let Some(name) = story {
        match name {
            // Persistent writes of 11105800, then 11115800 through its final cutscene guard.
            "gyoubu-defeated" => fixture.flags.extend([9301, 11100800, 6801, 6078]),
            "genichiro-defeated" => fixture
                .flags
                .extend([9301, 11100800, 6801, 6078, 9303, 11110800, 6803, 9403]),
            _ => {}
        }
    }
    world_events::evaluate(cache, map, fixture)
        .map(Some)
        .map_err(anyhow::Error::msg)
}

/// [`load`] for each of a world's blocks, by block id.
pub fn load_blocks(cache: &Path, layouts: &BTreeMap<String, Value>) -> Result<MapStory> {
    let mut out = MapStory::default();
    for (block, layout) in layouts {
        if let Some(state) = load(cache, block, layout)? {
            out.maps.insert(block.clone(), state);
        }
    }
    Ok(out)
}

pub fn load(cache: &Path, map: &str, layout: &Value) -> Result<Option<StoryMap>> {
    // Diagnostic control for paired hole-sweep runs with identical loaded geometry.
    if std::env::args().any(|arg| arg == "--no-map-story") {
        return Ok(None);
    }
    let args: Vec<_> = std::env::args().collect();
    let Some(state) = initial_state(cache, map, selection(&args).as_deref())? else {
        return Ok(None);
    };
    anyhow::ensure!(
        layout["layout_version"].as_u64() == Some(2),
        "story state requires lossless layout v2"
    );
    layout["parts"]
        .as_array()
        .context("story state requires canonical parts")?;
    let parts = state.parts(layout);
    let mut by_name = HashMap::new();
    for (i, part) in parts.iter().enumerate() {
        anyhow::ensure!(
            by_name.insert(part.name.clone(), i).is_none(),
            "duplicate story part name {}",
            part.name
        );
    }
    Ok(Some(StoryMap {
        state,
        parts,
        by_name,
    }))
}
