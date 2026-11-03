//! Plays the game's own sounds.
//!
//! The simulation names the sounds ([`sekiro_sim::sound`]: TAE sound events and hit, block and
//! deflect sounds); [`emit`] turns one duel step into [`SoundCue`] messages and [`play_cues`]
//! plays them.
//!
//! - First choice: the game's own FMOD runtime from the install ([`sekiro_fmod`]), with the
//!   event projects of the fight (`main`, the enemy's `cXXXX`, the map area's) read from
//!   `cache/raw/sound`. Every cue is an FMOD event placed at the character, the listener is
//!   the camera, and FMOD applies the events' own variations, volumes, 3D rolloff and voice
//!   limits. A key that no project has is silent, as in the game.
//! - Fallback when the DLLs cannot be loaded: decoded samples in `cache/sound/<bank>/` (from
//!   `sekiro-extract sound`) as Bevy positional one-shots.
//!
//! Set `SEKIRO_NO_FMOD=1` to force the fallback.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, SpatialListener, Volume};
use bevy::prelude::*;
use sekiro_fmod::{EventSystem, Played};
use sekiro_sim::duel::{Duel, DuelReport};
use sekiro_sim::sound::{CueSource, HitSounds, tae_cues};

/// Height above a character's feet where its sounds play (about the chest; the TAE events'
/// dummy polys are not used yet).
const EMITTER_HEIGHT: f32 = 1.0;

/// One sound to play at a position: event keys to try in order.
#[derive(Message, Debug, Clone)]
pub struct SoundCue {
    pub keys: Vec<String>,
    pub position: Vec3,
    pub source: CueSource,
}

impl From<sekiro_sim::sound::SoundCue> for SoundCue {
    fn from(cue: sekiro_sim::sound::SoundCue) -> Self {
        Self {
            keys: cue.keys,
            position: Vec3::from_array(cue.position),
            source: cue.source,
        }
    }
}

fn actor_cues(
    actor: usize,
    character: &sekiro_sim::character::Character,
    sounds: &HitSounds,
    out: &mut MessageWriter<SoundCue>,
) {
    out.write_batch(
        tae_cues(actor, character, sounds.materials[actor])
            .into_iter()
            .map(SoundCue::from),
    );
}
/// Hit-sound tables and the decoded-sample fallback.
#[derive(Resource, Default)]
pub struct SoundLibrary {
    events: HashMap<String, Vec<(PathBuf, u32)>>,
    loaded: HashMap<PathBuf, Handle<AudioSource>>,
    missing: HashSet<String>,
    cache: PathBuf,
    /// Hit sounds for the NpcParam row they were loaded for.
    hit_sounds: HashMap<i32, Option<HitSounds>>,
    /// Project requests waiting to be consumed once, separate from the known project set.
    enemy_projects: HashSet<String>,
    known_projects: HashSet<String>,
    rng: u64,
    /// Every key played, in order (for logs and tests).
    pub played: Vec<String>,
}

impl SoundLibrary {
    fn require_project(&mut self, chr: &str) {
        if self.known_projects.insert(chr.to_owned()) {
            self.enemy_projects.insert(chr.to_owned());
        }
    }
    /// Reads every `cache/sound/*/events.json` (the fallback samples) when `decoded`.
    pub fn load(cache: &Path, decoded: bool) -> Self {
        let mut lib = SoundLibrary {
            cache: cache.to_path_buf(),
            rng: 0x9e37_79b9_7f4a_7c15,
            ..default()
        };
        if !decoded {
            return lib;
        }
        let Ok(dirs) = std::fs::read_dir(cache.join("sound")) else {
            warn!("no cache/sound: run `sekiro-extract sound` for the fallback audio");
            return lib;
        };
        for dir in dirs.flatten().map(|d| d.path()) {
            let Ok(text) = std::fs::read_to_string(dir.join("events.json")) else {
                continue;
            };
            let Ok(map) = serde_json::from_str::<HashMap<String, Vec<serde_json::Value>>>(&text)
            else {
                continue;
            };
            for (event, variations) in map {
                let list = lib.events.entry(event).or_default();
                for v in variations {
                    if let Some(file) = v.get("file").and_then(|f| f.as_str()) {
                        let weight = v.get("weight").and_then(|w| w.as_u64()).unwrap_or(100);
                        list.push((dir.join(file), weight.max(1) as u32));
                    }
                }
            }
        }
        info!(
            "sound: {} decoded events from cache/sound",
            lib.events.len()
        );
        lib
    }

    /// The hit sounds for the enemy of NpcParam row `npc_param`, loaded on first use.
    fn hit_sounds(&mut self, npc_param: i32) -> Option<&HitSounds> {
        self.hit_sounds
            .entry(npc_param)
            .or_insert_with(|| HitSounds::load_from_cache(&self.cache, npc_param))
            .as_ref()
    }

    fn pick(&mut self, event: &str) -> Option<PathBuf> {
        let list = self.events.get(event)?;
        let total: u32 = list.iter().map(|(_, w)| w).sum();
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let mut r = (self.rng % total.max(1) as u64) as u32;
        for (path, w) in list {
            if r < *w {
                return Some(path.clone());
            }
            r -= w;
        }
        list.first().map(|(p, _)| p.clone())
    }
}

/// The sound projects a fight on map `map` loads at start, in lookup order: the common
/// project, the map area's (`m11_01_00_00` gives `m11`, `sm11`, `xm11`, `vm11`, `rm11`), then
/// the common SFX project. The enemy's own project is added when the fight starts.
pub fn start_projects(map: Option<&str>) -> Vec<String> {
    let mut projects = vec!["main".to_string()];
    if let Some(area) = map.and_then(|m| m.get(..3)).filter(|a| a.starts_with('m')) {
        for prefix in ["", "s", "x", "v", "r"] {
            projects.push(format!("{prefix}{area}"));
        }
    }
    projects.push("smain".into());
    projects
}

/// The game's FMOD event system and what it has loaded.
pub struct FmodAudio {
    system: EventSystem,
    loaded: HashSet<String>,
    /// Keys already reported as missing or failing.
    reported: HashSet<String>,
    population_resident: HashSet<String>,
    population_required: HashSet<String>,
    pending_cues: Vec<SoundCue>,
}

impl FmodAudio {
    /// Starts FMOD from the install and loads the projects of [`start_projects`] and the
    /// enemy's own (`enemy`), so no project loads mid-fight.
    fn open(cache: &Path, map: Option<&str>, enemy: Option<&str>) -> Result<Self, String> {
        let game = sekiro_fmod::game_dir();
        let mut system = EventSystem::open(&game, false)
            .map_err(|e| format!("{e} (install {})", game.display()))?;
        system
            .set_media_path(&cache.join("raw/sound"))
            .map_err(|e| e.to_string())?;
        let mut audio = FmodAudio {
            system,
            loaded: HashSet::new(),
            reported: HashSet::new(),
            population_resident: HashSet::new(),
            population_required: HashSet::new(),
            pending_cues: Vec::new(),
        };
        for p in start_projects(map) {
            audio.load(&p);
        }
        if let Some(chr) = enemy {
            audio.load(chr);
        }
        if audio.system.projects().iter().all(|p| p.name != "main") {
            return Err("main.fev did not load (run `sekiro-extract unpack`)".into());
        }
        Ok(audio)
    }

    /// Loads project `name` once. The samples of the common, enemy and map projects are read
    /// now, so no sound waits for the disk the first time it plays.
    fn load(&mut self, name: &str) {
        self.load_project(name, true);
    }

    fn load_project(&mut self, name: &str, warm_samples: bool) {
        if !self.loaded.insert(name.to_string()) {
            return;
        }
        match self.system.load_project(&format!("{name}.fev")) {
            Ok(n) => {
                let preload = warm_samples
                    && (name == "main" || name.starts_with('c') || name.starts_with('m'));
                let note = match preload.then(|| self.system.preload(name)) {
                    Some((_, Some(code))) => {
                        format!(", samples not preloaded (FMOD error {code})")
                    }
                    Some(_) => ", samples preloaded".into(),
                    None => String::new(),
                };
                info!("FMOD: {name}.fev, {n} events{note}");
            }
            Err(e) => warn!("FMOD: {name}.fev not loaded: {e}"),
        }
    }

    /// Plays the first of `keys` that a project has; the key and what FMOD did.
    fn play(&mut self, keys: &[String], position: Vec3) -> Option<(String, Played)> {
        let key = keys.iter().find(|k| self.system.has(k))?;
        Some((key.clone(), self.system.play(key, position.to_array())))
    }

    pub(crate) fn population_banks_pending(&self) -> bool {
        self.population_required
            .iter()
            .any(|chr| !self.population_resident.contains(chr))
    }

    fn cue_ready(&self, cue: &SoundCue) -> bool {
        let bank = cue
            .keys
            .iter()
            .find_map(|key| self.system.path_of(key))
            .and_then(|path| path.split('/').next());
        bank.is_none_or(|bank| {
            !self.population_required.contains(bank) || self.population_resident.contains(bank)
        })
    }
    /// Check residency before cue playback, loading/freeing data only on activation changes.
    /// Never open dormant banks or first-load wave data as many actors start events together.
    pub(crate) fn population_banks<'a>(&mut self, required: impl Iterator<Item = &'a str>) {
        let required = required.collect::<std::collections::BTreeSet<_>>();
        let leaving = self
            .population_resident
            .iter()
            .filter(|chr| !required.contains(chr.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for chr in leaving {
            self.population_resident.remove(&chr);
            match self.system.free_samples(&chr) {
                Ok(()) => {
                    info!("FMOD: {chr} dormant, event data released");
                }
                Err(error) => {
                    if self.reported.insert(format!("free:{chr}")) {
                        warn!("FMOD: {chr} release: {error}");
                    }
                }
            }
        }
        self.population_required = required.iter().map(|chr| (*chr).to_owned()).collect();
        for chr in required {
            if self.population_resident.insert(chr.to_owned()) {
                let result = self.system.preload(chr);
                info!("FMOD: {chr} activated, event data preload={result:?}");
                break; // Bound activation warmup to one kind per Update frame.
            }
        }
    }
}

pub struct GameAudioPlugin {
    pub cache: PathBuf,
    /// The map being played (`m11_01_00_00`), for its area's sound projects.
    pub map: Option<String>,
    /// The enemy's character id (`c5060`) and NpcParam row: its sound project and the hit-sound
    /// tables load at start too, not on the fight's first step (about 90 ms, measured).
    pub enemy: Option<(String, i32)>,
}

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        let fmod = if std::env::var_os("SEKIRO_NO_FMOD").is_some() {
            Err("SEKIRO_NO_FMOD is set".to_string())
        } else {
            FmodAudio::open(
                &self.cache,
                self.map.as_deref(),
                self.enemy.as_ref().map(|(chr, _)| chr.as_str()),
            )
        };
        let use_fmod = fmod.is_ok();
        match fmod {
            Ok(f) => {
                info!("sound: the game's FMOD event system");
                app.insert_non_send(f).add_systems(
                    PostUpdate,
                    update_fmod.after(bevy::transform::TransformSystems::Propagate),
                );
            }
            Err(e) => {
                warn!("sound: FMOD unavailable ({e}); falling back to the decoded samples");
                app.add_systems(Update, attach_listener);
            }
        }
        let level = volume_arg(&std::env::args().collect::<Vec<_>>());
        let mut library = SoundLibrary::load(&self.cache, !use_fmod);
        if let Some((_, npc_param)) = &self.enemy {
            library.hit_sounds(*npc_param);
        }
        app.insert_resource(library)
            .insert_resource(MasterVolume {
                level,
                shown_until: 0.0,
                applied: None,
            })
            .add_message::<SoundCue>()
            .add_systems(Startup, spawn_volume_readout)
            .add_systems(Update, (adjust_volume, play_cues).chain());
    }
}

/// Prepare sound tables and project metadata behind the loading screen.
/// Loading every character's event data exhausts FMOD resources (error 43), preventing
/// even Wolf's footsteps from starting. Warm only active kinds and evict them on dormancy.
pub(crate) fn prepare_population(app: &mut App, foes: &[(String, i32, bool)]) {
    {
        let mut library = app.world_mut().resource_mut::<SoundLibrary>();
        for (chr, npc, _) in foes {
            library.hit_sounds(*npc);
            library.require_project(chr);
        }
    }
    let mut projects = app
        .world_mut()
        .resource_mut::<SoundLibrary>()
        .enemy_projects
        .drain()
        .collect::<Vec<_>>();
    projects.sort(); // Preserve lookup order when projects share an event key.
    if let Some(mut fmod) = app.world_mut().get_non_send_mut::<FmodAudio>() {
        for chr in projects {
            fmod.load_project(&chr, false);
        }
        // The loading screen may warm all initially active kinds; Update warms only one.
        loop {
            fmod.population_banks(
                foes.iter()
                    .filter(|(_, _, active)| *active)
                    .map(|(chr, _, _)| chr.as_str()),
            );
            if !fmod.population_banks_pending() {
                break;
            }
        }
    }
}

/// Default master volume (the game at full volume was reported too loud).
pub const DEFAULT_VOLUME: f32 = 0.5;
/// One press of a volume key.
const VOLUME_STEP: f32 = 0.1;
/// How long the readout stays after a change, in seconds.
const READOUT_SECONDS: f64 = 1.5;

/// The player's master volume, 0 to 1, on top of the game's own mix.
#[derive(Resource, Debug)]
pub struct MasterVolume {
    pub level: f32,
    /// Time (seconds since start) until which the readout shows.
    shown_until: f64,
    /// The level last handed to the sound system.
    applied: Option<f32>,
}

/// `--volume <0..1>` from the command line, else [`DEFAULT_VOLUME`].
fn volume_arg(args: &[String]) -> f32 {
    args.windows(2)
        .find(|w| w[0] == "--volume")
        .and_then(|w| w[1].parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .map_or(DEFAULT_VOLUME, |v| v.clamp(0.0, 1.0))
}

/// `level` moved by `steps` presses, kept on the 10% grid and within 0 to 1.
fn step_volume(level: f32, steps: i32) -> f32 {
    ((level / VOLUME_STEP).round() + steps as f32).clamp(0.0, 1.0 / VOLUME_STEP) * VOLUME_STEP
}

#[derive(Component)]
struct VolumeReadout;

fn spawn_volume_readout(mut commands: Commands) {
    commands.spawn((
        VolumeReadout,
        Text::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(22.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            right: px(16),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Minus / `[` lower and equals / `]` raise the volume by 10%; applies it to FMOD's master
/// category (times the game's own 0.7) or to the fallback's global volume, and shows the level
/// for a moment.
fn adjust_volume(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    mut volume: ResMut<MasterVolume>,
    mut global: ResMut<bevy::audio::GlobalVolume>,
    fmod: Option<NonSendMut<FmodAudio>>,
    mut readout: Query<(&mut Text, &mut Visibility), With<VolumeReadout>>,
) {
    let down = [
        KeyCode::Minus,
        KeyCode::NumpadSubtract,
        KeyCode::BracketLeft,
    ]
    .iter()
    .filter(|k| keys.just_pressed(**k))
    .count() as i32;
    let up = [KeyCode::Equal, KeyCode::NumpadAdd, KeyCode::BracketRight]
        .iter()
        .filter(|k| keys.just_pressed(**k))
        .count() as i32;
    let now = time.elapsed_secs_f64();
    if up != down {
        volume.level = step_volume(volume.level, up - down);
        volume.shown_until = now + READOUT_SECONDS;
    }
    if volume.applied != Some(volume.level) {
        let level = volume.level;
        volume.applied = Some(level);
        if let Some(mut fmod) = fmod {
            fmod.system
                .set_master_volume(sekiro_fmod::MASTER_VOLUME * level);
        }
        *global = bevy::audio::GlobalVolume::new(Volume::Linear(level));
        info!("sound: volume {:.0}%", level * 100.0);
    }
    for (mut text, mut vis) in &mut readout {
        if now < volume.shown_until {
            text.0 = format!("Volume {:.0}%", volume.level * 100.0);
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// Puts the fallback's spatial listener on the 3D camera.
fn attach_listener(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<SpatialListener>)>,
) {
    for e in &cameras {
        commands.entity(e).insert(SpatialListener::new(0.3));
    }
}

/// The listener follows the camera, then FMOD does its frame's work.
fn update_fmod(mut fmod: NonSendMut<FmodAudio>, camera: Query<&GlobalTransform, With<Camera3d>>) {
    let _timer = crate::perf_log::SystemTimer::start("audio FMOD");
    if let Ok(cam) = camera.single() {
        let t = cam.compute_transform();
        fmod.system.set_listener(
            t.translation.to_array(),
            t.forward().to_array(),
            t.up().to_array(),
        );
    }
    let result = fmod.system.update();
    if result != 0 && fmod.reported.insert(format!("update:{result}")) {
        warn!("FMOD update result={result}");
    }
}

/// Plays queued cues: through FMOD when it runs, else as decoded positional one-shots.
pub fn play_cues(
    mut commands: Commands,
    mut cues: MessageReader<SoundCue>,
    mut lib: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    fmod: Option<NonSendMut<FmodAudio>>,
) {
    let _timer = crate::perf_log::SystemTimer::start("audio cues");
    if let Some(mut fmod) = fmod {
        for chr in lib.enemy_projects.drain() {
            fmod.load(&chr);
        }
        let waiting = std::mem::take(&mut fmod.pending_cues);
        for cue in waiting.into_iter().chain(cues.read().cloned()) {
            if !fmod.cue_ready(&cue) {
                fmod.pending_cues.push(cue);
                continue;
            }
            let position = cue.position + Vec3::Y * EMITTER_HEIGHT;
            let played = fmod.play(&cue.keys, position);

            match played {
                Some((key, Played::Started)) => lib.played.push(key),
                Some((key, Played::Failed(code))) => {
                    if fmod.reported.insert(key.clone()) {
                        info!("sound {key} ({:?}): FMOD error {code}", cue.source);
                    }
                }
                Some((_, Played::Unknown)) | None => {
                    let key = cue.keys.join("|");
                    if fmod.reported.insert(key.clone()) {
                        info!("sound {key} ({:?}): no such event", cue.source);
                    }
                }
            }
        }
        return;
    }
    for cue in cues.read() {
        let Some((key, path)) = cue
            .keys
            .iter()
            .find_map(|k| Some((k.clone(), lib.pick(k)?)))
        else {
            let key = cue.keys.join("|");
            if lib.missing.insert(key.clone()) {
                info!("sound {key}: not in the decoded banks");
            }
            continue;
        };
        let handle = match lib.loaded.get(&path) {
            Some(h) => h.clone(),
            None => match std::fs::read(&path) {
                Ok(bytes) => {
                    let h = sources.add(AudioSource {
                        bytes: Arc::from(bytes),
                    });
                    lib.loaded.insert(path.clone(), h.clone());
                    h
                }
                Err(e) => {
                    warn!("sound {}: {e}", path.display());
                    continue;
                }
            },
        };
        lib.played.push(key);
        commands.spawn((
            AudioPlayer(handle),
            PlaybackSettings::DESPAWN
                .with_spatial(true)
                .with_volume(Volume::Linear(0.8)),
            Transform::from_translation(cue.position + Vec3::Y * EMITTER_HEIGHT),
        ));
    }
}

/// The cues of one duel step: TAE sound events of both characters and the sounds of the hits.
pub fn emit_population(
    duel: &Duel,
    population: &sekiro_sim::duel::population::Population,
    lib: &mut SoundLibrary,
    out: &mut MessageWriter<SoundCue>,
) {
    let _timer = crate::perf_log::SystemTimer::start("audio emit");
    let row = population.selected.map_or(duel.spec.npc_param, |i| {
        population.foes[i].duel.spec.npc_param
    });
    if let Some(hs) = lib.hit_sounds(row) {
        actor_cues(0, &duel.player, hs, out);
    }
    for foe in population.foes.iter().filter(|f| f.stepped) {
        let Some(hs) = lib.hit_sounds(foe.duel.spec.npc_param) else {
            continue;
        };
        actor_cues(1, &foe.duel.enemy, hs, out);
        for (hit, res) in &foe.report.hits {
            out.write_batch(hs.cues(hit, res).into_iter().map(SoundCue::from));
        }
    }
}

/// The cues of one duel step: TAE sounds of both characters and resolved hit sounds.
pub fn emit(
    duel: &Duel,
    report: &DuelReport,
    lib: &mut SoundLibrary,
    out: &mut MessageWriter<SoundCue>,
) {
    let _timer = crate::perf_log::SystemTimer::start("audio emit");
    lib.require_project(&duel.spec.chr);
    let Some(hs) = lib.hit_sounds(duel.spec.npc_param) else {
        return;
    };
    actor_cues(0, &duel.player, hs, out);
    actor_cues(1, &duel.enemy, hs, out);
    for (hit, res) in &report.hits {
        out.write_batch(hs.cues(hit, res).into_iter().map(SoundCue::from));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_project_requests_do_not_reload_each_frame() {
        let mut library = SoundLibrary::default();
        for _ in 0..43 {
            library.require_project("c1010");
        }
        assert_eq!(
            library.enemy_projects.drain().collect::<Vec<_>>(),
            ["c1010"]
        );
        for _ in 0..600 {
            library.require_project("c1010");
        }
        assert!(library.enemy_projects.is_empty());
        library.require_project("c1020");
        assert_eq!(
            library.enemy_projects.drain().collect::<Vec<_>>(),
            ["c1020"]
        );
    }

    #[test]
    fn population_preparation_keeps_wolf_audio_playable() {
        let cache = sekiro_sim::cache::root();
        let game = sekiro_fmod::game_dir();
        if !cache.join("raw/sound/c5020.fev").is_file() || !game.join("fmodex64.dll").is_file() {
            return;
        }
        let mut system = EventSystem::open(&game, true).unwrap();
        system.set_media_path(&cache.join("raw/sound")).unwrap();
        let mut fmod = FmodAudio {
            system,
            loaded: HashSet::new(),
            reported: HashSet::new(),
            population_resident: HashSet::new(),
            population_required: HashSet::new(),
            pending_cues: Vec::new(),
        };
        for name in start_projects(Some("m11_00_00_00")) {
            fmod.load(&name);
        }
        let mut app = App::new();
        app.insert_resource(SoundLibrary { cache, ..default() })
            .insert_non_send(fmod);
        let foes = [
            "c1010", "c1020", "c1100", "c1120", "c1150", "c1180", "c1190", "c1240", "c1350",
            "c1360", "c5020",
        ]
        .map(|chr| (chr.to_owned(), 10100000, matches!(chr, "c1010" | "c1150")));
        prepare_population(&mut app, &foes);
        let mut fmod = app.world_mut().non_send_mut::<FmodAudio>();
        assert_eq!(fmod.system.projects().len(), 18);
        assert_eq!(fmod.population_resident.len(), 2);
        fmod.population_banks(["c1010"].into_iter());
        assert!(!fmod.population_resident.contains("c1150"));
        fmod.population_banks(["c1010", "c1150"].into_iter());
        assert_eq!(fmod.population_resident.len(), 2);
        // One new kind warms each frame; retain the next kind's first cue until ready.
        fmod.population_banks(["c1010"].into_iter());
        fmod.population_banks(["c1010", "c1150", "c1180"].into_iter());
        assert_eq!(fmod.population_resident.len(), 2);
        let key = fmod
            .system
            .projects()
            .iter()
            .find(|p| p.name == "c1180")
            .unwrap()
            .events[0]
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        let first_cue = SoundCue {
            keys: vec![key],
            position: Vec3::ZERO,
            source: CueSource::Tae,
        };
        assert!(!fmod.cue_ready(&first_cue));
        fmod.pending_cues.push(first_cue);
        fmod.population_banks(["c1010", "c1150", "c1180"].into_iter());
        assert!(fmod.cue_ready(&fmod.pending_cues[0]));
        assert_eq!(
            fmod.pending_cues.len(),
            1,
            "first cue is retained until ready"
        );
        assert_eq!(
            fmod.system.channels_playing(),
            0,
            "preparation must not start events"
        );
        for key in ["c000001001", "c000004010"] {
            assert_eq!(fmod.system.play(key, [0.0; 3]), Played::Started, "{key}");
        }
    }

    #[test]
    fn projects_follow_the_map_area() {
        assert_eq!(
            start_projects(Some("m11_01_00_00")),
            ["main", "m11", "sm11", "xm11", "vm11", "rm11", "smain"]
        );
        assert_eq!(start_projects(None), ["main", "smain"]);
    }

    #[test]
    fn volume_flag_and_steps() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(volume_arg(&args(&["sekiro-rs", "--arena", "owl"])), 0.5);
        assert_eq!(volume_arg(&args(&["x", "--volume", "0.3"])), 0.3);
        assert_eq!(volume_arg(&args(&["x", "--volume", "7"])), 1.0);
        assert_eq!(volume_arg(&args(&["x", "--volume", "loud"])), 0.5);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(near(step_volume(0.5, 1), 0.6));
        assert!(near(step_volume(0.5, -2), 0.3));
        assert!(near(step_volume(0.95, 1), 1.0));
        assert!(near(step_volume(1.0, 1), 1.0));
        assert!(near(step_volume(0.1, -3), 0.0));
        assert!(near(step_volume(0.33, 0), 0.3));
    }
}
