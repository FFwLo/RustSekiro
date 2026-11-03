//! Selection only: event-authored story environments and collision GPARAM keys.
use std::{collections::BTreeMap, path::Path};

// Alias avoids Bevy 0.20 prelude ContextExt ambiguity.
use anyhow::{Context as ContextExt, Result};
use bevy::prelude::*;
use sekiro_formats::{
    emedf,
    emevd::{Emevd, Instruction},
    gparam::{self, Gparam},
};
use sekiro_sim::{
    arena::{ArenaSpec, scene::SceneState},
    events::{EventEntry, EventHost, EventScripts},
    story::StoryTime,
};
use serde_json::Value;

#[path = "lighting/maps/m10.rs"]
mod m10;
#[path = "lighting/maps/m11_00.rs"]
mod m11_00;
#[path = "lighting/maps/m11_01.rs"]
mod m11_01;
#[path = "lighting/maps/m11_02.rs"]
mod m11_02;
#[path = "lighting/maps/m13.rs"]
mod m13;
#[path = "lighting/maps/m15.rs"]
mod m15;
#[path = "lighting/maps/m17.rs"]
mod m17;
#[path = "lighting/maps/m20.rs"]
mod m20;
#[path = "lighting/maps/m25.rs"]
mod m25;
#[path = "lighting/maps/m89.rs"]
mod m89;

const MAPS: [&str; 10] = [
    m10::PROFILE,
    m11_00::PROFILE,
    m11_01::PROFILE,
    m11_02::PROFILE,
    m13::PROFILE,
    m15::PROFILE,
    m17::PROFILE,
    m20::PROFILE,
    m25::PROFILE,
    m89::PROFILE,
];

/// All four independent fields, even when the installed map only uses zero for three.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionSelection(pub [i32; 4]);

impl RegionSelection {
    pub fn enter(&mut self, config: [i32; 4]) {
        for (current, next) in self.0.iter_mut().zip(config) {
            if next >= 0 {
                *current = next;
            }
        }
    }
}

#[derive(Debug)]
pub struct Selection {
    pub fixed_environment: bool,
    pub hour: f32,
    pub environment: u32,
    pub sub_id: u8,
    pub source: String,
    pub sky: Option<String>,
    pub flags: Vec<i32>,
}

impl Selection {
    /// Sets the clock (`--hour`, or an arena's story hour) in place of the story's own. The hour
    /// lands in [`crate::lighting::MapLighting::hour`], which every later LightSet, fog and post
    /// rebuild reads, so collision row changes at doorways keep it (#59). Unless the map's
    /// environment is fixed, the probes follow the hour, and an explicit override or a changed
    /// hour chooses its matching ordinary sky.
    pub fn override_hour(&mut self, hour: f32, explicit: bool) {
        let hour_changed = hour != self.hour;
        self.hour = hour;
        if !self.fixed_environment {
            self.environment = crate::lighting::nearest_variant(hour);
            if hour_changed || explicit {
                self.sky = None;
            }
        }
    }
}

fn profile(id: &str) -> Result<Value> {
    MAPS.iter()
        .map(|s| serde_json::from_str::<Value>(s).expect("lighting profile JSON"))
        .find(|p| id.get(..6) == p["map"].as_str())
        .with_context(|| format!("no lighting profile for {id}"))
}

struct MapHost {
    area: u8,
    block: u8,
}
impl EventHost for MapHost {
    fn encounter_condition(&self, i: &Instruction) -> Option<bool> {
        (i.code() == (3, 8)).then(|| {
            (i.u8_at(2) == Some(self.area) && i.u8_at(3) == Some(self.block))
                == (i.u8_at(1) == Some(1))
        })
    }
    fn has_sp_effect(&self, _: i32, _: i32) -> Option<bool> {
        None
    }
    fn has_event_message(&self, _: i32, _: i32) -> Option<bool> {
        None
    }
    fn is_dead(&self, _: i32) -> Option<bool> {
        None
    }
    fn hp(&self, _: i32) -> Option<i32> {
        None
    }
}

fn script(cache: &Path, map: &str) -> Result<Emevd> {
    let path = cache.join(format!("raw/event/{map}.emevd"));
    Ok(Emevd::parse(
        &std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
    )?)
}

/// Resolve a sub-ID against the installed catalog, never substitute a nearest number.
/// Families without a 00 file must be unique or explicitly documented in the map profile.
pub fn sub_file(cache: &Path, id: &str, sub: u8) -> Result<String> {
    let p = profile(id)?;
    if let Some(stem) = p["sub_files"][sub.to_string()].as_str() {
        anyhow::ensure!(
            cache
                .join(format!("raw/param/drawparam/{stem}.gparam"))
                .is_file(),
            "missing {stem}"
        );
        return Ok(stem.into());
    }
    let base = format!("{}_00{sub:02}", &id[..6]);
    let dir = cache.join("raw/param/drawparam");
    if dir.join(format!("{base}.gparam")).is_file() {
        return Ok(base);
    }
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{}_", &id[..6]))
            && name.ends_with(&format!("{sub:02}.gparam"))
        {
            candidates.push(name.trim_end_matches(".gparam").to_string());
        }
    }
    anyhow::ensure!(
        candidates.len() == 1,
        "{id} sub-ID {sub}: ambiguous/missing GPARAM families {candidates:?}"
    );
    Ok(candidates.remove(0))
}

pub fn select(
    cache: &Path,
    id: &str,
    story: Option<&str>,
    arena: Option<&ArenaSpec>,
) -> Result<Selection> {
    let p = profile(id)?;
    let name = story.unwrap_or(p["default"].as_str().context("map default")?);
    let states = p["states"].as_array().context("map states")?;
    let state = states
        .iter()
        .find(|s| s["name"].as_str() == Some(name))
        .with_context(|| {
            format!(
                "--story {name} for {id}: choose {}",
                states
                    .iter()
                    .filter_map(|s| s["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut flags: Vec<i32> = state["flags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .map(|f| f as i32)
        .collect();
    if story.is_none()
        && let Some(arena) = arena
    {
        flags = arena.beaten.to_vec();
        if let Some(boss) = sekiro_sim::arena::boss_events(&arena.enemy_spec()) {
            flags.extend(boss.flags);
        }
    }
    for rule in p["seed_rules"].as_array().into_iter().flatten() {
        if rule["when"].as_array().is_some_and(|required| {
            required
                .iter()
                .all(|f| f.as_i64().is_some_and(|f| flags.contains(&(f as i32))))
        }) {
            flags.extend(
                rule["set"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_i64)
                    .map(|f| f as i32),
            );
        }
    }
    let area = id[1..3].parse()?;
    let state = if story.is_none() && arena.is_some() {
        states
            .iter()
            .find(|s| {
                s["when_flags"].as_array().is_some_and(|required| {
                    !required.is_empty()
                        && required
                            .iter()
                            .all(|f| f.as_i64().is_some_and(|f| flags.contains(&(f as i32))))
                })
            })
            .unwrap_or(state)
    } else {
        state
    };
    let apply_preset = story.is_some() || arena.is_none() || !state["when_flags"].is_null();
    let block = id[4..6].parse()?;
    let common = script(cache, "common")?;
    let mut events = EventScripts::new(
        &common,
        &[EventEntry {
            id: 100,
            from_label: None,
        }],
    )
    .map_err(anyhow::Error::msg)?;
    // Event 820 raises the cumulative clock before event 100 selects the environment.
    let time = StoryTime::from_flags(|f| flags.contains(&f));
    let variant = super::lighting::nearest_variant(time.hour()) as usize;
    let count = match variant {
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        _ => 0,
    };
    flags.extend(sekiro_sim::story::TIME_FLAGS[..count].iter());
    events.flags.extend(flags.iter().copied());
    events.step(0., &MapHost { area, block });
    let mut scene = events.cast_scene;
    // Presets reference original instruction locations, not copied parameter values.
    if apply_preset {
        for command in state["commands"].as_array().into_iter().flatten() {
            let event = command[0].as_i64().context("lighting event reference")?;
            let index = command[1]
                .as_u64()
                .context("lighting instruction reference")? as usize;
            let source_map = state["event_map"].as_str().unwrap_or(id);
            let file = script(cache, source_map)?;
            let ins = file
                .event(event)
                .and_then(|e| e.instructions.get(index))
                .context("lighting instruction missing")?;
            anyhow::ensure!(
                matches!(ins.code(), (2003, 80 | 81 | 88) | (2012, 1)),
                "lighting reference changed: {event}:{index}"
            );
            let args: Vec<_> = emedf::decode(ins)
                .context("lighting instruction layout")?
                .1
                .into_iter()
                .map(|(_, value)| value)
                .collect();
            scene.instruction(ins, &args);
        }
    }
    let environment = scene
        .environment
        .context("event 100 did not select an environment")? as u32;
    let mut hour = StoryTime::from_variant(environment as i32)
        .context("unknown authored environment")?
        .hour();
    if let Some(lighting) = scene.lighting.and_then(StoryTime::from_variant) {
        hour = lighting.hour();
    }
    if story.is_none()
        && let Some(arena) = arena
    {
        hour = arena.story_time(&arena.enemy_spec()).hour();
    }
    let sub_id = scene
        .gparam_sub_ids
        .get(&(area as i16, block as i16))
        .map_or(0, |s| s.0);
    let source = if sub_id == 0
        && let Some(source) = p["base_file"].as_str()
    {
        source.into()
    } else if !state["commands"].is_null()
        && apply_preset
        && let Some(source) = p["event_sub_files"][sub_id.to_string()].as_str()
    {
        source.into()
    } else {
        sub_file(cache, id, sub_id)?
    };
    let sky = sky_model(cache, &scene)?;
    Ok(Selection {
        fixed_environment: p["fixed_environment"].as_bool().unwrap_or(false),
        hour,
        environment,
        sub_id,
        source,
        sky,
        flags,
    })
}

fn sky_model(cache: &Path, scene: &SceneState) -> Result<Option<String>> {
    let file = cache.join("raw/map/mapstudio/m89_00_00_99.msb");
    let msb = sekiro_formats::msb::parse(&std::fs::read(file)?)?;
    Ok(scene
        .enabled_order
        .iter()
        .rev()
        .filter(|group| scene.enabled.get(group) == Some(&true))
        .find_map(|group| {
            msb.parts
                .iter()
                .find(|p| p.entity_group_ids.contains(group))
                .and_then(|p| msb.model(p))
                .map(|m| m.name.clone())
        }))
}

/// Stem of the family file that holds layer `index` of the block whose layer 0 is `stem`.
///
/// A block loads ten files `m<area>_<block>_<d><sub>`; the executable derives the number of the
/// file for index `d` as `(d - base / 100) * 100 + base` (`ed5cd0`), where `base` is the number
/// of the layer-0 file.
/// Layers 1 to 4 take `d` from the four signed bytes of the collision `SceneGparam` struct, and
/// a byte outside `0..=9` leaves the layer empty.
pub fn family_stem(stem: &str, index: i8) -> Option<String> {
    let (prefix, number) = stem.split_at_checked(stem.len().checked_sub(4)?)?;
    let valid = prefix.len() == 7
        && prefix.starts_with('m')
        && prefix.ends_with('_')
        && stem.bytes().filter(u8::is_ascii_digit).count() == 8;
    let base: i32 = number.parse().ok().filter(|_| valid)?;
    let d = i32::from(index);
    let number = (d - base / 100) * 100 + base;
    ((0..=9).contains(&d) && (0..10_000).contains(&number)).then(|| format!("{prefix}{number:04}"))
}

fn read_gparam(cache: &Path, stem: &str) -> Result<Option<Gparam>> {
    let path = cache.join(format!("raw/param/drawparam/{stem}.gparam"));
    path.is_file()
        .then(|| Ok(gparam::parse(&std::fs::read(&path)?)?))
        .transpose()
}

/// The engine layer stack for one map: `default.gparam` and `mapbase.gparam` under the block's
/// own file and the four collision-selected overlay files (priorities in [`gparam::priority`]).
/// `MapOverride_NNNN` (priority 10) is not applied: the block field that selects it was not
/// recovered, and the installed files carry no LightSet rows.
#[derive(Default)]
struct LayerFiles {
    defaults: Option<Gparam>,
    mapbase: Option<Gparam>,
    family: BTreeMap<String, Gparam>,
}

impl LayerFiles {
    fn load(cache: &Path, stems: &[&str]) -> Result<Self> {
        let mut family = BTreeMap::new();
        for stem in stems {
            for d in 0..=9 {
                if let Some(name) = family_stem(stem, d)
                    && !family.contains_key(&name)
                    && let Some(g) = read_gparam(cache, &name)?
                {
                    family.insert(name, g);
                }
            }
        }
        Ok(Self {
            defaults: read_gparam(cache, "default")?,
            mapbase: read_gparam(cache, "mapbase")?,
            family,
        })
    }

    /// The resolved stack for `own` (the file `stem`) with the given collision overlay bytes.
    fn compose(&self, stem: &str, own: &Gparam, overlays: [i8; 4]) -> Gparam {
        // Diagnostic `SEKIRO_ABLATE=nolayers` restores the single-file lookup.
        if crate::lighting::ablate("nolayers") {
            return own.clone();
        }
        let mut layers: Vec<(u8, &Gparam)> = Vec::new();
        if let Some(g) = &self.mapbase {
            layers.push((gparam::priority::MAP_BASE, g));
        }
        layers.push((gparam::priority::BLOCK_LAYER, own));
        for (i, &d) in overlays.iter().enumerate() {
            if let Some(g) = family_stem(stem, d).and_then(|name| self.family.get(&name)) {
                layers.push((gparam::priority::BLOCK_LAYER + 1 + i as u8, g));
            }
        }
        gparam::layer(&layers, self.defaults.as_ref())
    }
}

/// Loaded once per map. Runtime event overrides use the same resolver as initial selection.
#[derive(Resource)]
pub struct LightingSelection {
    pub initial: Selection,
    pub area: (i16, i16),
    files: BTreeMap<u8, (String, Gparam)>,
    base: Gparam,
    current: Option<u8>,
    pinned: bool,
    layers: LayerFiles,
    /// Overlay layers 1 to 4 chosen by the collision under the focus (-1: none), and the ones
    /// last resolved into the lighting.
    overlays: [i8; 4],
    applied_overlays: [i8; 4],
}

impl LightingSelection {
    pub fn load(cache: &Path, id: &str, initial: Selection, pinned: bool) -> Result<Self> {
        let area = (id[1..3].parse()?, id[4..6].parse()?);
        let mut subs = std::collections::BTreeSet::from([initial.sub_id]);
        for map in ["common", id] {
            if !cache.join(format!("raw/event/{map}.emevd")).is_file() {
                continue;
            }
            for event in script(cache, map)?.events {
                for i in event.instructions {
                    if i.code() == (2003, 88)
                        && i.i32_at(0) == Some(i32::from(area.0))
                        && i.i32_at(4) == Some(i32::from(area.1))
                        && let Some(sub) = i.u8_at(8)
                    {
                        subs.insert(sub);
                    }
                }
            }
        }
        let mut files = BTreeMap::new();
        let profile = profile(id)?;
        for sub in subs {
            let source = if let Some(source) = profile["event_sub_files"][sub.to_string()].as_str()
            {
                source.into()
            } else if sub == initial.sub_id {
                initial.source.clone()
            } else {
                sub_file(cache, id, sub)?
            };
            let g = gparam::parse(&std::fs::read(
                cache.join(format!("raw/param/drawparam/{source}.gparam")),
            )?)?;
            files.insert(sub, (source, g));
        }
        let base = gparam::parse(&std::fs::read(
            cache.join(format!("raw/param/drawparam/{}.gparam", initial.source)),
        )?)?;
        let stems: Vec<&str> = files
            .values()
            .map(|(source, _)| source.as_str())
            .chain([initial.source.as_str()])
            .collect();
        let layers = LayerFiles::load(cache, &stems)?;
        Ok(Self {
            initial,
            area,
            files,
            base,
            current: None,
            pinned,
            layers,
            overlays: [-1; 4],
            applied_overlays: [-1; 4],
        })
    }

    /// Resolves the initial file through the layer stack. Pinned or substituted files (explicit
    /// `--gparam`, cinematic banks) are used as they are.
    pub fn resolve_initial(&self, stem: &str, plain: Gparam) -> Gparam {
        if self.pinned || stem != self.initial.source {
            plain
        } else {
            self.layers.compose(stem, &plain, [-1; 4])
        }
    }

    /// The overlay layers of the collision under the focus; applied on the next `sync_events`.
    pub fn set_overlays(selection: &mut Mut<Self>, overlays: [i8; 4]) {
        crate::change::set(selection, |s| &mut s.overlays, overlays);
    }

    pub fn apply_sub(&mut self, requested: Option<u8>, light: &mut crate::lighting::MapLighting) {
        if self.pinned || (requested == self.current && self.overlays == self.applied_overlays) {
            return;
        }
        let Some(sub) = requested else {
            light.gparam = self
                .layers
                .compose(&self.initial.source, &self.base, self.overlays);
            light.source.clone_from(&self.initial.source);
            light.applied = None;
            self.current = None;
            self.applied_overlays = self.overlays;
            return;
        };
        if let Some((source, g)) = self.files.get(&sub) {
            light.gparam = self.layers.compose(source, g, self.overlays);
            light.source.clone_from(source);
            light.applied = None;
            self.current = Some(sub);
            self.applied_overlays = self.overlays;
            info!(
                "event lighting: {source}, sub-ID {sub}, overlays {:?}",
                self.overlays
            );
        } else {
            warn!("event lighting: unavailable sub-ID {sub}");
        }
    }
}

pub fn sync_events(
    map_scene: Res<MapEventScene>,
    mut selection: ResMut<LightingSelection>,
    mut light: ResMut<crate::lighting::MapLighting>,
) {
    let scene = &map_scene.0;
    let sub = scene.gparam_sub_ids.get(&selection.area).map(|s| s.0);
    selection.apply_sub(sub, &mut light);
}

#[derive(Resource, Default)]
pub struct MapEventScene(pub SceneState);

#[derive(Resource)]
pub struct MapStoryEvents {
    cache: std::path::PathBuf,
    map: String,
    scripts: EventScripts,
}

impl MapStoryEvents {
    pub fn install(app: &mut App, cache: &Path, id: &str, story: Option<&str>) -> Result<()> {
        app.init_resource::<MapEventScene>();
        let p = profile(id)?;
        let name = story.unwrap_or(p["default"].as_str().context("map default")?);
        let state = p["states"]
            .as_array()
            .context("map states")?
            .iter()
            .find(|s| s["name"].as_str() == Some(name))
            .context("map story state")?;
        let entries: Vec<_> = state["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .map(|id| EventEntry {
                id,
                from_label: None,
            })
            .collect();
        if entries.is_empty() {
            return Ok(());
        }
        let mut scripts =
            EventScripts::new(&script(cache, id)?, &entries).map_err(anyhow::Error::msg)?;
        scripts.flags.extend(
            app.world()
                .resource::<LightingSelection>()
                .initial
                .flags
                .iter()
                .copied(),
        );
        app.insert_resource(Self {
            cache: cache.into(),
            map: id.into(),
            scripts,
        })
        .add_systems(Update, tick_map_events);
        Ok(())
    }
}

struct PositionHost<'a> {
    cache: &'a Path,
    map: &'a str,
    position: [f32; 3],
}
impl EventHost for PositionHost<'_> {
    fn has_sp_effect(&self, _: i32, _: i32) -> Option<bool> {
        None
    }
    fn has_event_message(&self, _: i32, _: i32) -> Option<bool> {
        None
    }
    fn is_dead(&self, _: i32) -> Option<bool> {
        None
    }
    fn hp(&self, _: i32) -> Option<i32> {
        None
    }
    fn inside_area(&self, entity: i32, area: i32) -> Option<bool> {
        (entity == sekiro_sim::events::PLAYER_ENTITY)
            .then(|| {
                sekiro_sim::arena_c::regions::contains(self.cache, self.map, area, self.position)
            })
            .flatten()
    }
}

fn tick_map_events(
    mut events: ResMut<MapStoryEvents>,
    mut scene: ResMut<MapEventScene>,
    time: Res<Time>,
    focus: Query<&Transform, With<crate::map::MapFocus>>,
) {
    let Some(focus) = focus.iter().next() else {
        return;
    };
    let MapStoryEvents {
        cache,
        map,
        scripts,
    } = &mut *events;
    scripts.step(
        time.delta_secs(),
        &PositionHost {
            cache,
            map,
            position: focus.translation.to_array(),
        },
    );
    scene.0.clone_from(&scripts.cast_scene);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cache() -> std::path::PathBuf {
        sekiro_sim::cache::root()
    }
    #[test]
    fn reference_progression_uses_installed_boss_and_clock_events() {
        let cache = cache();
        for (map, event, flags) in [
            ("m11_00_00_00", 11105800, &[9301, 11100800, 6801, 6078][..]),
            ("m11_01_00_00", 11115800, &[9303, 11110800, 6803, 9403][..]),
        ] {
            let file = script(&cache, map).unwrap();
            for &flag in flags {
                assert!(file.event(event).unwrap().instructions.iter().any(|i| {
                    i.code() == (2003, 2) && i.i32_at(0) == Some(flag) && i.u8_at(4) == Some(1)
                }));
            }
        }
        for map in ["m11_00_00_00", "m11_01_00_00"] {
            for (name, flags, hour, environment, sky) in [
                ("gyoubu-defeated", vec![9301], 6., 1, "m900000"),
                ("genichiro-defeated", vec![9301, 9303], 12., 2, "m900010"),
            ] {
                let selected = select(&cache, map, Some(name), None).unwrap();
                assert_eq!(
                    (selected.hour, selected.environment, selected.sub_id),
                    (hour, environment, 0)
                );
                assert_eq!(selected.source, format!("{}_0000", &map[..6]));
                assert_eq!(selected.sky.as_deref(), Some(sky));
                assert!(flags.iter().all(|f| selected.flags.contains(f)));
                let state = crate::map::story::initial_state(&cache, map, Some(name))
                    .unwrap()
                    .unwrap();
                assert!(flags.iter().all(|f| state.flags.contains(f)));
                if name == "genichiro-defeated" {
                    assert!(
                        state.flags.contains(&9403),
                        "post-fight cutscene must remain completed"
                    );
                }
                println!(
                    "{map} {name}: {selected:?}; {} visibility writes, {} unsupported",
                    state.visibility.len(),
                    state.unsupported.len()
                );
            }
        }
    }
    #[test]
    fn defaults_follow_installed_common_event() {
        for (map, hour, environment) in [
            ("m10_00", 0., 0),
            ("m11_00", 6., 1),
            ("m11_01", 6., 1),
            ("m11_02", 0., 0),
            ("m13_00", 6., 1),
            ("m15_00", 6., 1),
            ("m17_00", 6., 1),
            ("m20_00", 6., 1),
            ("m25_00", 22., 4),
        ] {
            let selection = select(&cache(), &format!("{map}_00_00"), None, None).unwrap();
            assert_eq!(
                (selection.hour, selection.environment),
                (hour, environment),
                "{map}"
            );
            assert_eq!(selection.source, format!("{map}_0000"));
            if map == "m11_00" {
                // The fresh menu's start is the real game's opening (#47): before Gyoubu, no story-time flag.
                assert_eq!(
                    (selection.sub_id, selection.sky.as_deref()),
                    (0, Some("m900000"))
                );
                assert!(
                    !selection
                        .flags
                        .iter()
                        .any(|f| *f == 9301 || (9800..=9803).contains(f))
                );
            }
        }
    }
    #[test]
    fn invasion_and_storm_use_authored_sub_ids() {
        for map in ["m11_00", "m11_01", "m11_02"] {
            let s = select(&cache(), &format!("{map}_00_00"), Some("invasion"), None).unwrap();
            assert_eq!((s.environment, s.sub_id), (4, 20));
            assert_eq!(s.source, format!("{map}_0020"));
        }
        let s = select(&cache(), "m11_02_00_00", Some("sword-saint-storm"), None).unwrap();
        assert_eq!(s.source, "m11_02_0930");
        let clear = select(&cache(), "m11_02_00_00", Some("sword-saint-defeated"), None).unwrap();
        assert_eq!(clear.source, "m11_02_0920");
        for (map, sub, source) in [("m11_00", 30, "m11_00_0030"), ("m11_01", 10, "m11_01_0010")] {
            let s = select(&cache(), &format!("{map}_00_00"), Some("tomoe"), None).unwrap();
            assert_eq!((s.hour, s.sub_id, s.source.as_str()), (18., sub, source));
        }
    }
    #[test]
    #[ignore = "slow: audit every boss arena story preset"]
    fn boss_arenas_keep_their_story_state() {
        for arena in sekiro_sim::arena::ARENAS {
            let s = select(&cache(), arena.map, None, Some(arena)).unwrap();
            assert_eq!(
                s.hour,
                arena.story_time(&arena.enemy_spec()).hour(),
                "{}",
                arena.name
            );
            if arena.map.starts_with("m10") {
                assert_eq!(s.environment, 0);
            }
        }
    }

    #[test]
    fn castle_boss_arenas_keep_their_story_state() {
        let morning = select(
            &cache(),
            "m11_01_00_00",
            None,
            Some(ArenaSpec::by_name("genichiro").unwrap()),
        )
        .unwrap();
        let evening = select(
            &cache(),
            "m11_01_00_00",
            None,
            Some(ArenaSpec::by_name("owl").unwrap()),
        )
        .unwrap();
        assert_eq!((morning.hour, evening.hour), (6., 18.));
    }
    #[test]
    fn region_switch_preserves_all_independent_collision_ids() {
        let mut r = RegionSelection::default();
        r.enter([110, 7, 9, 12]);
        assert_eq!(r.0, [110, 7, 9, 12]);
        r.enter([0, 0, 0, 0]);
        assert_eq!(r.0, [0; 4]);
        r.enter([-1; 4]);
        assert_eq!(r.0, [0; 4]);
    }

    #[test]
    fn map_sky_override_wins_over_the_common_sky() {
        let s = select(&cache(), "m20_00_00_00", Some("illusion"), None).unwrap();
        let msb = sekiro_formats::msb::parse(
            &std::fs::read(cache().join("raw/map/mapstudio/m89_00_00_99.msb")).unwrap(),
        )
        .unwrap();
        let part = msb
            .parts
            .iter()
            .find(|p| p.entity_group_ids.contains(&8_907_085))
            .unwrap();
        assert_eq!(
            s.sky.as_deref(),
            Some(msb.model(part).unwrap().name.as_str())
        );
    }
    #[test]
    #[ignore = "slow: audit every installed map story preset"]
    fn all_presets_resolve() {
        for json in MAPS.iter().take(9) {
            let p: Value = serde_json::from_str(json).unwrap();
            let map = format!("{}_00_00", p["map"].as_str().unwrap());
            for state in p["states"].as_array().unwrap() {
                select(&cache(), &map, state["name"].as_str(), None).unwrap();
            }
        }
    }

    fn drawparam(stem: &str) -> Gparam {
        read_gparam(&cache(), stem).unwrap().unwrap()
    }

    fn value(g: &Gparam, name: &str, row: i32, hour: f32) -> Vec<f32> {
        g.param("LightSet", name)
            .and_then(|p| p.at(g.light_row(row), hour))
            .unwrap()
    }

    #[test]
    fn family_files_follow_the_executable_numbering() {
        assert_eq!(
            family_stem("m11_00_0000", 5).as_deref(),
            Some("m11_00_0500")
        );
        assert_eq!(
            family_stem("m11_00_0000", 0).as_deref(),
            Some("m11_00_0000")
        );
        assert_eq!(
            family_stem("m11_02_0930", 5).as_deref(),
            Some("m11_02_0530")
        );
        assert_eq!(
            family_stem("m11_02_0930", 9).as_deref(),
            Some("m11_02_0930")
        );
        assert_eq!(
            family_stem("m11_00_0020", 3).as_deref(),
            Some("m11_00_0320")
        );
        assert_eq!(family_stem("m11_00_0000", -1), None);
        assert_eq!(family_stem("m11_00_0000", 10), None);
        assert_eq!(family_stem("s89_00_0099", 1), None);
        assert_eq!(family_stem("default", 1), None);
    }

    #[test]
    fn map_override_files_parse_and_carry_no_light_rows() {
        // Game id 3 (value ids without times): the files exist, but hold no LightSet rows.
        for i in 0..4 {
            let g = drawparam(&format!("mapoverride_{i:04}"));
            assert!(g.rows("LightSet").is_empty(), "mapoverride_{i:04}");
            assert!(g.groups.len() == if i == 0 { 0 } else { 25 }, "{i}");
        }
        let env = drawparam("mapoverride_0001");
        let ids: Vec<i32> = env
            .param("Env Map", "EnvMapId")
            .unwrap()
            .entries
            .iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, (0..=16).collect::<Vec<_>>());
    }

    #[test]
    fn mapbase_and_row_defaults_layer_under_the_ashina_block_file() {
        let library = LayerFiles::load(&cache(), &["m11_00_0000"]).unwrap();
        let own = drawparam("m11_00_0000");
        let base = drawparam("mapbase");
        let defaults = drawparam("default");
        let g = library.compose("m11_00_0000", &own, [-1; 4]);
        assert!(g.is_layered());
        // The block's own row-100 sun beats the map base.
        assert_eq!(
            value(&g, "Directional Light DiffColor0", 100, 12.),
            value(&own, "Directional Light DiffColor0", 100, 12.)
        );
        assert_ne!(
            value(&g, "Directional Light DiffColor0", 100, 12.),
            value(&base, "Directional Light DiffColor0", 100, 12.)
        );
        // Fields the block file never authors for row 100 come from the map base for that row,
        // not from the block's row 0 and not from the shared default.
        for name in [
            "Hemi Color Up",
            "Hemi Color Down",
            "Sharpness0",
            "SsaoWeight0",
        ] {
            let name = format!("Directional Light {name}");
            assert_eq!(
                value(&g, &name, 100, 18.),
                value(&base, &name, 100, 18.),
                "{name}"
            );
        }
        assert_ne!(
            value(&g, "Directional Light Hemi Color Up", 100, 18.),
            value(&g, "Directional Light Hemi Color Up", 0, 18.),
            "row 100 must not inherit the block's row 0"
        );
        assert_eq!(value(&g, "Directional Light Sharpness0", 100, 12.), [1.]);
        // The block's row 0 still wins for row 0.
        assert_eq!(
            value(&g, "Directional Light Hemi Color Up", 0, 18.),
            value(&own, "Directional Light Hemi Color Up", 0, 18.)
        );
        // Rows only the map base registers exist for every block, but row 140 does not.
        assert!(g.rows("LightSet").contains(&130));
        assert_eq!(g.light_row(130), 130);
        assert_eq!(g.light_row(140), gparam::DEFAULT_ROW);
        assert_eq!(
            value(&g, "Directional Light DiffColor0", 140, 12.),
            value(&defaults, "Directional Light DiffColor0", 0, 12.)
        );
    }

    #[test]
    fn collision_overlays_win_over_the_block_file_by_priority() {
        let stem = "m11_00_0000";
        let library = LayerFiles::load(&cache(), &[stem]).unwrap();
        let own = drawparam(stem);
        let plain = library.compose(stem, &own, [-1; 4]);
        // Layer 2 = file 5 authors a row-100 second light that the block file lacks.
        let layered = library.compose(stem, &own, [-1, -1, 5, -1]);
        let overlay = drawparam("m11_00_0500");
        let angle = "Directional Light Angle1";
        assert_ne!(
            value(&plain, angle, 100, 12.),
            value(&layered, angle, 100, 12.)
        );
        assert_eq!(
            value(&layered, angle, 100, 12.),
            value(&overlay, angle, 100, 12.)
        );
        // Overlay 1 authors a different tone-map exposure bank than the base file.
        let exposure = |g: &Gparam| {
            g.param("Tone Map", "ToneMap-Exposure")
                .and_then(|p| p.at(0, 12.))
                .unwrap()
        };
        let overlay1 = library.compose(stem, &own, [1, -1, -1, -1]);
        assert_eq!(exposure(&overlay1), exposure(&drawparam("m11_00_0100")));
        // The collision order matters: a later layer beats an earlier one.
        let both = library.compose(stem, &own, [1, 5, -1, -1]);
        assert_eq!(
            exposure(&both),
            exposure(&drawparam("m11_00_0500")),
            "layer 2 outranks layer 1"
        );
    }

    #[test]
    fn every_ashina_collision_resolves_to_registered_rows_with_complete_records() {
        for map in ["m11_00_00_00", "m11_01_00_00", "m11_02_00_00"] {
            let stem = format!("{}_0000", &map[..6]);
            let library = LayerFiles::load(&cache(), &[stem.as_str()]).unwrap();
            let own = drawparam(&stem);
            let layout: Value = serde_json::from_slice(
                &std::fs::read(cache().join(format!("maps/{map}/layout.json"))).unwrap(),
            )
            .unwrap();
            for part in layout["collisions"].as_array().unwrap() {
                let layers = [0, 1, 2, 3].map(|i| {
                    part["scene_gparam"]["event_ids"][i]
                        .as_i64()
                        .map_or(-1, |v| v as i8)
                });
                let g = library.compose(&stem, &own, layers);
                let row = part["light_set"].as_i64().unwrap() as i32;
                // Every field of every light slot has a value for the row, from some layer.
                for name in [
                    "Directional Light DiffColor0",
                    "Directional Light Angle0",
                    "Directional Light Hemi Color Up",
                    "Directional Light Sharpness1",
                    "Directional Light SsaoWeight0",
                ] {
                    assert!(
                        g.param("LightSet", name)
                            .is_some_and(|p| p.entries.iter().any(|e| e.id == g.light_row(row))),
                        "{map} {} row {row} {name}",
                        part["name"]
                    );
                }
            }
        }
    }

    #[test]
    fn event_sub_id_reset_restores_the_initial_story_file() {
        let s = select(&cache(), "m11_02_00_00", None, None).unwrap();
        let mut library = LightingSelection::load(&cache(), "m11_02_00_00", s, false).unwrap();
        let (source, gparam) = library.files[&0].clone();
        let mut light = crate::lighting::MapLighting {
            source,
            gparam,
            hour: 0.,
            light_set: 0,
            applied: Some(0),
        };
        library.apply_sub(Some(30), &mut light);
        assert_eq!(light.source, "m11_02_0930");
        library.apply_sub(None, &mut light);
        assert_eq!(light.source, "m11_02_0000");
        assert_eq!(light.applied, None);
    }

    /// `--hour 4` on the Outskirts, then the walk out of the start hall, back in and out again:
    /// the collision LightSet row flips 110 inside, 100 outside (#59), and an event sub-ID
    /// rebuilds the bank on the way. Every rebuild must light the scene at 4 h, never at the
    /// story's 6 h morning.
    #[test]
    fn light_set_changes_keep_the_hour_override() {
        use crate::lighting::{FillLight, LightSet, MapLighting, Sun, apply_lighting};
        let map = "m11_00_00_00";
        let mut selected = select(&cache(), map, None, None).unwrap();
        assert_eq!(selected.hour, 6., "the story's own morning");
        selected.override_hour(4., true);
        assert_eq!(selected.hour, 4.);
        assert_eq!(selected.environment, crate::lighting::nearest_variant(4.));
        assert_eq!(
            selected.sky, None,
            "an explicit hour picks its ordinary sky"
        );
        let library = LightingSelection::load(&cache(), map, selected, false).unwrap();
        let source = library.initial.source.clone();
        let gparam = library.resolve_initial(&source, library.base.clone());
        let mut app = App::new();
        app.insert_resource(MapLighting {
            gparam,
            hour: library.initial.hour,
            light_set: 0,
            applied: None,
            source,
        })
        .insert_resource(library)
        .init_resource::<MapEventScene>()
        .init_resource::<GlobalAmbientLight>()
        .init_resource::<ClearColor>()
        .add_systems(Update, (sync_events, apply_lighting).chain());
        let sun = app
            .world_mut()
            .spawn((Sun, DirectionalLight::default(), Transform::default()))
            .id();
        app.world_mut()
            .spawn((FillLight, DirectionalLight::default(), Transform::default()));
        let mut seen = std::collections::BTreeSet::new();
        for (row, sub) in [
            (110, None),
            (100, None),
            (110, None),
            (100, Some(20)),
            (100, None),
        ] {
            let area = app.world().resource::<LightingSelection>().area;
            let mut scene = app.world_mut().resource_mut::<MapEventScene>();
            scene.0.gparam_sub_ids.clear();
            if let Some(sub) = sub {
                scene.0.gparam_sub_ids.insert(area, (sub, 0.));
            }
            app.world_mut().resource_mut::<MapLighting>().light_set = row;
            app.update();
            let light = app.world().resource::<MapLighting>();
            assert_eq!(
                (light.light_set, light.hour),
                (row, 4.),
                "row {row}, sub {sub:?}"
            );
            seen.insert(light.source.clone());
            let at = |hour| LightSet::from_gparam(&light.gparam, row, hour).sun_color;
            let color = app.world().get::<DirectionalLight>(sun).unwrap().color;
            assert_eq!(color, at(4.), "row {row}, sub {sub:?}");
            assert_ne!(color, at(6.), "row {row}, sub {sub:?} lit at 6 h");
        }
        assert_eq!(seen.len(), 2, "the event sub-ID rebuilt the bank: {seen:?}");
    }

    /// A collision overlay byte 9 on m25 layers `m25_00_0900`, which authors other `ShadowParam`
    /// cut points (5/25 m against the base stack's 10/50 m). The overlay must rebuild the sun's
    /// cascades, a `--shadow` distance must survive it, and a LightSet row change with the same
    /// values must leave the cascade config untouched (#65).
    #[test]
    fn light_file_switch_rebuilds_the_sun_cascades() {
        use crate::lighting::{FillLight, MapLighting, ShadowSettings, Sun, apply_lighting};
        use bevy::light::CascadeShadowConfig;
        let map = "m25_00_00_00";
        let none: [String; 0] = [];
        let shadow_arg = ["--shadow".to_string(), "4,250".to_string()];
        for args in [&none[..], &shadow_arg[..]] {
            let selected = select(&cache(), map, None, None).unwrap();
            let library = LightingSelection::load(&cache(), map, selected, false).unwrap();
            let source = library.initial.source.clone();
            let hour = library.initial.hour;
            let gparam = library.resolve_initial(&source, library.base.clone());
            let settings = ShadowSettings::from_args(&gparam, hour, args).unwrap();
            let start = settings.cascade_bounds();
            assert_eq!(&start[..2], [10., 50.], "the base file's cut points");
            let mut app = App::new();
            app.insert_resource(MapLighting {
                gparam,
                hour,
                light_set: 0,
                applied: None,
                source,
            })
            .insert_resource(library)
            .init_resource::<MapEventScene>()
            .init_resource::<GlobalAmbientLight>()
            .init_resource::<ClearColor>()
            .insert_resource(settings)
            .add_systems(Update, (sync_events, apply_lighting).chain());
            let sun = app
                .world_mut()
                .spawn((
                    Sun,
                    DirectionalLight::default(),
                    CascadeShadowConfig {
                        bounds: start.clone(),
                        ..default()
                    },
                    Transform::default(),
                ))
                .id();
            app.world_mut()
                .spawn((FillLight, DirectionalLight::default(), Transform::default()));
            let cascades = |app: &App| {
                let config = app.world().entity(sun).get_ref::<CascadeShadowConfig>();
                let config = config.unwrap();
                (config.bounds.clone(), config.last_changed())
            };
            let set_overlays = |app: &mut App, overlays: [i8; 4]| {
                let mut selection = app.world_mut().resource_mut::<LightingSelection>();
                LightingSelection::set_overlays(&mut selection, overlays);
            };
            app.update();
            let (bounds, tick) = cascades(&app);
            assert_eq!(bounds, start, "{args:?}");
            // Same light file, another LightSet row: same values, no rebuild.
            app.world_mut().resource_mut::<MapLighting>().light_set = 100;
            app.update();
            assert_eq!(cascades(&app), (start.clone(), tick), "{args:?}");
            // The collision under the focus asks for overlay 9: m25_00_0900.
            set_overlays(&mut app, [9, -1, -1, -1]);
            app.update();
            let light = app.world().resource::<MapLighting>();
            let expected = ShadowSettings::from_args(&light.gparam, light.hour, args)
                .unwrap()
                .cascade_bounds();
            let (switched, _) = cascades(&app);
            assert_eq!(switched, expected, "{args:?}");
            assert_eq!(&switched[..2], [5., 25.], "{args:?}");
            if !args.is_empty() {
                assert_eq!(switched.last(), Some(&250.), "--shadow distance survives");
                assert_eq!(app.world().resource::<ShadowSettings>().distance, 250.);
            }
            // And back to the base stack.
            set_overlays(&mut app, [-1; 4]);
            app.update();
            assert_eq!(cascades(&app).0, start, "{args:?}");
        }
    }
}
