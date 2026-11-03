//! Placed enemies and their resource donors, read from extractor output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::enemy::EnemySpec;

#[derive(Debug, Clone)]
pub struct Placement {
    pub map: String,
    pub part: String,
    pub entity: i32,
    pub npc: i32,
    pub think: i32,
}

#[derive(Debug)]
pub struct Registry {
    pub placed: BTreeMap<String, Vec<Placement>>,
    npc: BTreeMap<i32, Value>,
    think: BTreeMap<i32, Value>,
    bundles: BTreeMap<String, Vec<String>>,
    names: BTreeMap<i32, String>,
}

fn json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The rows of `params/{table}.json` by id.
pub fn rows(cache: &Path, table: &str) -> Result<BTreeMap<i32, Value>, String> {
    let value = json(&cache.join(format!("params/{table}.json")))?;
    Ok(value["rows"]
        .as_array()
        .ok_or_else(|| format!("{table}: missing rows"))?
        .iter()
        .filter_map(|r| Some((r["id"].as_i64()? as i32, r.clone())))
        .collect())
}

impl Registry {
    /// The registry of the extracted data at `cache`, read once per process and shared after
    /// that: reading it parses every map's `layout.json` (several hundred MB of JSON, about 3 s),
    /// and duels, arenas and encounters each ask for it. A failed read is not kept, so a later
    /// call tries again.
    pub fn load(cache: &Path) -> Result<Arc<Self>, String> {
        static LOADED: Mutex<BTreeMap<PathBuf, Arc<Registry>>> = Mutex::new(BTreeMap::new());
        let key = std::fs::canonicalize(cache).unwrap_or_else(|_| cache.to_path_buf());
        // Held while reading, so callers racing on a cold cache wait for one read.
        let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(registry) = loaded.get(&key) {
            return Ok(registry.clone());
        }
        let registry = Arc::new(Self::read(cache)?);
        loaded.insert(key, registry.clone());
        Ok(registry)
    }

    fn read(cache: &Path) -> Result<Self, String> {
        let mut placed: BTreeMap<String, Vec<Placement>> = BTreeMap::new();
        let dirs = std::fs::read_dir(cache.join("maps")).map_err(|e| e.to_string())?;
        for dir in dirs {
            let dir = dir.map_err(|e| e.to_string())?.path();
            let path = dir.join("layout.json");
            if !path.exists() {
                continue;
            }
            let layout = json(&path)?;
            for p in layout["enemies"].as_array().into_iter().flatten() {
                let Some(chr) = p["model"].as_str() else {
                    continue;
                };
                if !chr.starts_with('c') || chr == "c0000" {
                    continue;
                }
                placed.entry(chr.into()).or_default().push(Placement {
                    map: dir.file_name().unwrap().to_string_lossy().into_owned(),
                    part: p["name"].as_str().unwrap_or_default().into(),
                    entity: p["entity_id"].as_i64().unwrap_or(-1) as i32,
                    npc: p["npc_param"].as_i64().unwrap_or(-1) as i32,
                    think: p["think_param"].as_i64().unwrap_or(-1) as i32,
                });
            }
        }
        for placements in placed.values_mut() {
            placements.sort_by(|a, b| (&a.map, &a.part).cmp(&(&b.map, &b.part)));
        }
        let mut bundles = BTreeMap::new();
        for dir in std::fs::read_dir(cache.join("raw/script")).map_err(|e| e.to_string())? {
            let dir = dir.map_err(|e| e.to_string())?.path();
            if !dir.is_dir() {
                continue;
            }
            let files = std::fs::read_dir(&dir)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect();
            bundles.insert(
                dir.file_name().unwrap().to_string_lossy().into_owned(),
                files,
            );
        }
        Ok(Self {
            placed,
            npc: rows(cache, "NpcParam")?,
            think: rows(cache, "NpcThinkParam")?,
            bundles,
            names: npc_names(cache)?,
        })
    }

    /// Resolve the exact placed think row, including its own map's script bundle first.
    pub fn placement_spec(&self, chr: &str, p: &Placement) -> Result<EnemySpec, String> {
        let row = self
            .npc
            .get(&p.npc)
            .ok_or_else(|| format!("{}: missing NpcParam {}", p.part, p.npc))?;
        let think = self
            .think
            .get(&p.think)
            .ok_or_else(|| format!("{}: missing NpcThinkParam {}", p.part, p.think))?;
        let logic = think["logicId"].as_i64().ok_or("missing logicId")?;
        let battle = think["battleGoalID"]
            .as_i64()
            .ok_or("missing battleGoalID")?;
        let has = |files: &Vec<String>| {
            files.contains(&format!("{logic:06}_logic.lua"))
                && files.contains(&format!("{battle:06}_battle.lua"))
        };
        let own = format!("{}.luabnd.d", p.map);
        let bundle = self
            .bundles
            .get(&own)
            .filter(|f| has(f))
            .map(|_| own.clone())
            .or_else(|| {
                self.bundles
                    .iter()
                    .find(|(_, f)| has(f))
                    .map(|(name, _)| name.clone())
            })
            .ok_or_else(|| format!("{}: think {} has no extracted Lua bundle", p.part, p.think))?;
        Ok(EnemySpec {
            chr: chr.into(),
            npc_param: p.npc,
            think: p.think,
            ai_bundle: bundle,
            starts_in_battle: (0..32)
                .any(|i| row[format!("spEffectID{i}")].as_i64() == Some(200004)),
        })
    }

    pub fn entity(&self, map: &str, entity: i32) -> Result<(EnemySpec, &Placement), String> {
        for (chr, placements) in &self.placed {
            if let Some(p) = placements
                .iter()
                .find(|p| p.map == map && p.entity == entity)
            {
                let mut spec = self.resolve(&format!("{chr}:{}", p.npc))?;
                // The two placed Isshins share NpcParam but have distinct authored Lua goals.
                if chr == "c5400" {
                    spec.think = p.think;
                }
                return Ok((spec, p));
            }
        }
        Err(format!("{map}: no placed entity {entity}"))
    }

    pub fn display_name(&self, chr: &str, row: i32) -> Option<&str> {
        // The Hall's c1000 row carries the monkeys' event-authored bar, not c1000's name.
        if chr == "c1000" && row == 12_600_050 {
            return self.names.get(&901_260).map(String::as_str);
        }
        let number = chr.strip_prefix('c')?.parse::<i32>().ok()?;
        let name_id = self.npc.get(&row)?["nameId"].as_i64().unwrap_or(0) as i32;
        self.names
            .get(&name_id)
            .or_else(|| self.names.get(&(900_000 + number)))
            .or_else(|| self.names.get(&(910_000 + number)))
            .map(String::as_str)
    }

    /// Map-authored boss-bar text can differ between placements sharing NpcParam.
    pub fn boss_name(&self, id: i32) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    pub fn resolve(&self, name: &str) -> Result<EnemySpec, String> {
        let (chr, requested) = name
            .split_once(':')
            .map_or((name, None), |(c, r)| (c, Some(r)));
        let placements = self
            .placed
            .get(chr)
            .ok_or_else(|| format!("{chr}: not in the placed roster"))?;
        let requested = requested
            .map(|r| r.parse::<i32>().map_err(|e| e.to_string()))
            .transpose()?;
        let mut counts = BTreeMap::<i32, usize>::new();
        for p in placements {
            *counts.entry(p.npc).or_default() += 1;
        }
        let npc = requested.unwrap_or_else(|| {
            *counts
                .iter()
                .max_by_key(|&(id, count)| (count, std::cmp::Reverse(id)))
                .unwrap()
                .0
        });
        for p in placements.iter().filter(|p| p.npc == npc) {
            if let Ok(spec) = self.placement_spec(chr, p) {
                return Ok(spec);
            }
        }
        Err(format!(
            "{chr}:{npc}: no placed think row with available Lua scripts"
        ))
    }
}

/// The little-endian SDT FMG v2: range records index an array of UTF-16 offsets.
fn npc_names(cache: &Path) -> Result<BTreeMap<i32, String>, String> {
    let dir = cache.join("raw/msg/engus/item.msgbnd.d");
    let Some(path) = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("NPC"))
        })
    else {
        return Ok(BTreeMap::new());
    };
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let u32_at = |at: usize| {
        bytes
            .get(at..at + 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    if bytes.get(..4) != Some(&[0, 0, 2, 0]) {
        return Err(format!("{}: unsupported FMG header", path.display()));
    }
    let groups = u32_at(12).ok_or("FMG range count missing")? as usize;
    let count = u32_at(16).ok_or("FMG string count missing")? as usize;
    let offsets = u32_at(24).ok_or("FMG offsets missing")? as usize;
    let mut names = BTreeMap::new();
    for group in 0..groups {
        let at = 40 + group * 16;
        let index = u32_at(at).ok_or("FMG range index missing")? as usize;
        let first = u32_at(at + 4).ok_or("FMG range start missing")?;
        let last = u32_at(at + 8).ok_or("FMG range end missing")?;
        if last < first || index + (last - first) as usize >= count {
            return Err("FMG invalid range".into());
        }
        for id in first..=last {
            let at = offsets + (index + (id - first) as usize) * 8;
            let start = bytes
                .get(at..at + 8)
                .and_then(|b| b.try_into().ok())
                .map(u64::from_le_bytes)
                .ok_or("FMG offset missing")? as usize;
            if start == 0 {
                continue;
            }
            let text = bytes
                .get(start..)
                .ok_or("FMG string out of bounds")?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .take_while(|&c| c != 0)
                .collect::<Vec<_>>();
            names.insert(
                id as i32,
                String::from_utf16(&text).map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(names)
}
