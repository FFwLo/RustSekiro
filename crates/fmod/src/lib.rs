//! The game's own sound engine, driven from Rust: `fmodex64.dll` and `fmod_event64.dll` (FMOD Ex
//! 4.4 with its Event API) are loaded read-only from the player's Sekiro install, the game's
//! event projects (`.fev`) are loaded with their sample banks (`.fsb`), and sounds are played by
//! event name. FMOD then applies each event's own layering, variations, volume, pitch
//! randomisation, 3D distance rolloff and voice limits, as in the game.
//!
//! Event paths are `project/group/event`; the game names an event after its sound key, so the
//! TAE sound `c000004010` is `main/main/c000004010` and the Owl's `c506000000` is
//! `c5060/c5060/c506000000`. [`EventSystem::load_project`] lists every event of a project, so
//! [`EventSystem::play`] only needs the key.
//!
//! Facts about the runtime (init flags, the master volume of 0.7, event naming) were learnt from
//! FwLo's RustSekiro (`docs/kb/sound.md`, MIT); see `docs/AUDIO.md` and `CREDITS.md`.
//! Nothing is decrypted here: banks FMOD cannot open (the encrypted `s*`, `v*`, `x*`, `r*`
//! banks) make their events fail, and those are reported, not worked around.

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_char, c_void};
use std::fmt;
use std::path::{Path, PathBuf};

type Ptr = *mut c_void;

/// The default Steam install. Overridden by `--game <dir>` or the `SEKIRO_DIR` variable, the
/// same way as `sekiro-extract`.
pub const DEFAULT_GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro";

/// The Sekiro install folder: `--game <dir>` on the command line, else `SEKIRO_DIR`, else the
/// default Steam folder.
pub fn game_dir() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--game")
        && let Some(dir) = args.get(i + 1)
    {
        return PathBuf::from(dir);
    }
    std::env::var_os("SEKIRO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAME))
}

/// `FMOD_INIT_3D_RIGHTHANDED` in FMOD Ex 4.44. (0x4 is `FMOD_INIT_SOFTWARE_DISABLE`, which turns
/// the software mixer off so that no event can start; FwLo found this the hard way.)
const INIT_3D_RIGHTHANDED: u32 = 0x2;
/// `FMOD_OUTPUTTYPE_NOSOUND`: everything runs and mixes, nothing reaches a device.
const OUTPUT_NOSOUND: i32 = 2;
/// `FMOD_EVENT_DEFAULT`: an instance that loads its wave data and can be started.
const EVENT_DEFAULT: u32 = 0;
/// `FMOD_EVENT_INFOONLY`: a handle for queries only, nothing is loaded.
const EVENT_INFOONLY: u32 = 4;
/// `FMOD_EVENT_RESOURCE_STREAMS_AND_SAMPLES`: what `FMOD_EventGroup_LoadEventData` loads.
const RESOURCE_STREAMS_AND_SAMPLES: i32 = 0;
/// Software channels the event system mixes.
const MAX_CHANNELS: i32 = 128;

/// The game's master category volume at default options: the exe scales the master category by
/// its Master slot (1.0) times 0.7 (traced by FwLo, `docs/kb/sound.md`, 2026-10-09).
pub const MASTER_VOLUME: f32 = 0.7;

/// `FMOD_VECTOR`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FVec {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<[f32; 3]> for FVec {
    fn from([x, y, z]: [f32; 3]) -> Self {
        FVec { x, y, z }
    }
}

/// Why the event system could not start or a call failed. `code` is an `FMOD_RESULT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmodError {
    /// A DLL or one of its functions could not be loaded.
    Library(String),
    /// An FMOD call returned an error.
    Call { call: &'static str, code: i32 },
}

impl fmt::Display for FmodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FmodError::Library(e) => write!(f, "{e}"),
            FmodError::Call { call, code } => write!(f, "{call} failed (FMOD error {code})"),
        }
    }
}

impl std::error::Error for FmodError {}

fn check(call: &'static str, code: i32) -> Result<(), FmodError> {
    if code == 0 {
        Ok(())
    } else {
        Err(FmodError::Call { call, code })
    }
}

/// What [`EventSystem::play`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Played {
    Started,
    /// No loaded project has an event of that name.
    Unknown,
    /// The event exists but FMOD would not start it: `code` is the `FMOD_RESULT` (a bank it
    /// cannot open, or the event's playback limit right now).
    Failed(i32),
}

/// The full event path for an event `key` in group `group` of project `project`.
pub fn event_path(project: &str, group: &str, key: &str) -> String {
    format!("{project}/{group}/{key}")
}

macro_rules! api {
    ($($field:ident : $sym:literal => fn($($arg:ty),*);)*) => {
        struct Api {
            $($field: unsafe extern "system" fn($($arg),*) -> i32,)*
        }

        impl Api {
            /// Looks every function up in `ex` (fmodex64) or `ev` (fmod_event64).
            fn load(ex: &libloading::Library, ev: &libloading::Library) -> Result<Self, FmodError> {
                unsafe {
                    Ok(Api {
                        $($field: {
                            let name: &[u8] = $sym;
                            let lib = if name.starts_with(b"FMOD_System_") || name.starts_with(b"FMOD_Channel") { ex } else { ev };
                            *lib.get::<unsafe extern "system" fn($($arg),*) -> i32>(name)
                                .map_err(|e| FmodError::Library(format!(
                                    "{}: {e}",
                                    String::from_utf8_lossy(name)
                                )))?
                        },)*
                    })
                }
            }
        }
    };
}

api! {
    create: b"FMOD_EventSystem_Create" => fn(*mut Ptr);
    init: b"FMOD_EventSystem_Init" => fn(Ptr, i32, u32, Ptr, u32);
    release: b"FMOD_EventSystem_Release" => fn(Ptr);
    system_object: b"FMOD_EventSystem_GetSystemObject" => fn(Ptr, *mut Ptr);
    media_path: b"FMOD_EventSystem_SetMediaPath" => fn(Ptr, *const c_char);
    load: b"FMOD_EventSystem_Load" => fn(Ptr, *const c_char, Ptr, *mut Ptr);
    get_event: b"FMOD_EventSystem_GetEvent" => fn(Ptr, *const c_char, u32, *mut Ptr);
    update: b"FMOD_EventSystem_Update" => fn(Ptr);
    listener: b"FMOD_EventSystem_Set3DListenerAttributes"
        => fn(Ptr, i32, *const FVec, *const FVec, *const FVec, *const FVec);
    category_by_index: b"FMOD_EventSystem_GetCategoryByIndex" => fn(Ptr, i32, *mut Ptr);
    category_volume: b"FMOD_EventCategory_SetVolume" => fn(Ptr, f32);
    project_groups: b"FMOD_EventProject_GetNumGroups" => fn(Ptr, *mut i32);
    project_group: b"FMOD_EventProject_GetGroupByIndex" => fn(Ptr, i32, i32, *mut Ptr);
    group_info: b"FMOD_EventGroup_GetInfo" => fn(Ptr, *mut i32, *mut *const c_char);
    group_groups: b"FMOD_EventGroup_GetNumGroups" => fn(Ptr, *mut i32);
    group_group: b"FMOD_EventGroup_GetGroupByIndex" => fn(Ptr, i32, i32, *mut Ptr);
    group_events: b"FMOD_EventGroup_GetNumEvents" => fn(Ptr, *mut i32);
    group_event: b"FMOD_EventGroup_GetEventByIndex" => fn(Ptr, i32, u32, *mut Ptr);
    group_load: b"FMOD_EventGroup_LoadEventData" => fn(Ptr, i32, u32);
    group_free: b"FMOD_EventGroup_FreeEventData" => fn(Ptr, Ptr, i32);
    event_info: b"FMOD_Event_GetInfo" => fn(Ptr, *mut i32, *mut *const c_char, Ptr);
    event_3d: b"FMOD_Event_Set3DAttributes" => fn(Ptr, *const FVec, *const FVec, *const FVec);
    event_start: b"FMOD_Event_Start" => fn(Ptr);
    set_output: b"FMOD_System_SetOutput" => fn(Ptr, i32);
    channels_playing: b"FMOD_System_GetChannelsPlaying" => fn(Ptr, *mut i32);
    event_channel_group: b"FMOD_Event_GetChannelGroup" => fn(Ptr, *mut Ptr);
    group_channels: b"FMOD_ChannelGroup_GetNumChannels" => fn(Ptr, *mut i32);
    group_channel: b"FMOD_ChannelGroup_GetChannel" => fn(Ptr, i32, *mut Ptr);
    group_subgroups: b"FMOD_ChannelGroup_GetNumGroups" => fn(Ptr, *mut i32);
    group_subgroup: b"FMOD_ChannelGroup_GetGroup" => fn(Ptr, i32, *mut Ptr);
    channel_playing: b"FMOD_Channel_IsPlaying" => fn(Ptr, *mut i32);
    channel_audibility: b"FMOD_Channel_GetAudibility" => fn(Ptr, *mut f32);
    event_stop: b"FMOD_Event_Stop" => fn(Ptr, i32);
    event_properties: b"FMOD_Event_GetNumProperties" => fn(Ptr, *mut i32);
    event_property_info: b"FMOD_Event_GetPropertyInfo" => fn(Ptr, *mut i32, *mut *const c_char, *mut i32);
    event_property: b"FMOD_Event_GetPropertyByIndex" => fn(Ptr, i32, Ptr, i32);
}

/// One loaded `.fev` project.
#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    /// Full paths of its events.
    pub events: Vec<String>,
}

/// The game's FMOD event system.
///
/// FMOD Ex calls must come from one thread: keep this on the main thread (a Bevy non-send
/// resource). Dropping it releases the system before the DLLs are unloaded.
pub struct EventSystem {
    api: Api,
    es: Ptr,
    sys: Ptr,
    projects: Vec<Project>,
    /// The `FMOD_EVENTPROJECT` of each of `projects`.
    handles: Vec<Ptr>,
    /// Event name (the sound key) to its full path, first project wins.
    index: HashMap<String, String>,
    /// Keys FMOD failed to start on their first try (unopenable bank), with the error.
    broken: HashMap<String, i32>,
    /// Keys that started at least once; a later failure is a passing playback limit.
    started: HashSet<String>,
    // Declared last so they unload after `Drop::drop` has released the system.
    _ev: libloading::Library,
    _ex: libloading::Library,
}

impl EventSystem {
    /// Loads the DLLs from `game_dir` and starts the event system with a 3D right-handed
    /// world (Bevy's axes) and the game's master volume. `silent` mixes without an output
    /// device (probes and tests).
    pub fn open(game_dir: &Path, silent: bool) -> Result<Self, FmodError> {
        // fmod_event64.dll imports fmodex64.dll: load that first, from the same folder.
        let open = |name: &str| unsafe {
            libloading::Library::new(game_dir.join(name))
                .map_err(|e| FmodError::Library(format!("{name}: {e}")))
        };
        let ex = open("fmodex64.dll")?;
        let ev = open("fmod_event64.dll")?;
        let api = Api::load(&ex, &ev)?;
        unsafe {
            let mut es: Ptr = std::ptr::null_mut();
            check("FMOD_EventSystem_Create", (api.create)(&mut es))?;
            let mut sys: Ptr = std::ptr::null_mut();
            (api.system_object)(es, &mut sys);
            if silent && !sys.is_null() {
                (api.set_output)(sys, OUTPUT_NOSOUND);
            }
            let r = (api.init)(
                es,
                MAX_CHANNELS,
                INIT_3D_RIGHTHANDED,
                std::ptr::null_mut(),
                0,
            );
            if r != 0 {
                (api.release)(es);
                return Err(FmodError::Call {
                    call: "FMOD_EventSystem_Init",
                    code: r,
                });
            }
            let mut out = EventSystem {
                api,
                es,
                sys,
                projects: Vec::new(),
                handles: Vec::new(),
                index: HashMap::new(),
                broken: HashMap::new(),
                started: HashSet::new(),
                _ev: ev,
                _ex: ex,
            };
            out.set_master_volume(MASTER_VOLUME);
            Ok(out)
        }
    }

    /// The folder FMOD reads `.fev` and `.fsb` files from.
    pub fn set_media_path(&mut self, dir: &Path) -> Result<(), FmodError> {
        let mut s = dir.to_string_lossy().replace('\\', "/");
        if !s.ends_with('/') {
            s.push('/');
        }
        let c = CString::new(s).map_err(|e| FmodError::Library(e.to_string()))?;
        check("FMOD_EventSystem_SetMediaPath", unsafe {
            (self.api.media_path)(self.es, c.as_ptr())
        })
    }

    /// Loads a project: `name` is a file name in the media path (`main.fev`) or a full path.
    /// Returns its number of events.
    pub fn load_project(&mut self, name: &str) -> Result<usize, FmodError> {
        let c = CString::new(name).map_err(|e| FmodError::Library(e.to_string()))?;
        let mut project: Ptr = std::ptr::null_mut();
        check("FMOD_EventSystem_Load", unsafe {
            (self.api.load)(self.es, c.as_ptr(), std::ptr::null_mut(), &mut project)
        })?;
        let stem = Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut events = Vec::new();
        unsafe {
            let mut n = 0;
            (self.api.project_groups)(project, &mut n);
            for i in 0..n {
                let mut group: Ptr = std::ptr::null_mut();
                if (self.api.project_group)(project, i, 0, &mut group) == 0 && !group.is_null() {
                    self.walk_group(group, &stem, &mut events);
                }
            }
        }
        for path in &events {
            if let Some((_, key)) = path.rsplit_once('/') {
                self.index
                    .entry(key.to_string())
                    .or_insert_with(|| path.clone());
            }
        }
        let count = events.len();
        self.handles.push(project);
        self.projects.push(Project { name: stem, events });
        Ok(count)
    }

    /// Collects the full paths of the events in `group` and its subgroups.
    unsafe fn walk_group(&self, group: Ptr, parent: &str, out: &mut Vec<String>) {
        unsafe {
            let (mut index, mut name) = (0, std::ptr::null());
            (self.api.group_info)(group, &mut index, &mut name);
            let path = format!("{parent}/{}", c_str(name));
            let mut n = 0;
            (self.api.group_events)(group, &mut n);
            for i in 0..n {
                let mut ev: Ptr = std::ptr::null_mut();
                if (self.api.group_event)(group, i, EVENT_INFOONLY, &mut ev) == 0 && !ev.is_null() {
                    let (mut index, mut name) = (0, std::ptr::null());
                    (self.api.event_info)(ev, &mut index, &mut name, std::ptr::null_mut());
                    out.push(format!("{path}/{}", c_str(name)));
                }
            }
            let mut n = 0;
            (self.api.group_groups)(group, &mut n);
            for i in 0..n {
                let mut sub: Ptr = std::ptr::null_mut();
                if (self.api.group_group)(group, i, 0, &mut sub) == 0 && !sub.is_null() {
                    self.walk_group(sub, &path, out);
                }
            }
        }
    }

    /// Loads the sample data of every event of project `name` now, so the first time each
    /// plays does not wait for the disk. Returns the groups that loaded and the first error.
    pub fn preload(&mut self, name: &str) -> (usize, Option<i32>) {
        self.preload_resource(name, RESOURCE_STREAMS_AND_SAMPLES)
    }

    fn preload_resource(&mut self, name: &str, resource: i32) -> (usize, Option<i32>) {
        let Some(i) = self.projects.iter().position(|p| p.name == name) else {
            return (0, None);
        };
        let project = self.handles[i];
        let (mut loaded, mut error) = (0, None);
        unsafe {
            let mut n = 0;
            (self.api.project_groups)(project, &mut n);
            for g in 0..n {
                let mut group: Ptr = std::ptr::null_mut();
                if (self.api.project_group)(project, g, 0, &mut group) != 0 || group.is_null() {
                    continue;
                }
                match (self.api.group_load)(group, resource, EVENT_DEFAULT) {
                    0 => loaded += 1,
                    r => {
                        error.get_or_insert(r);
                    }
                }
            }
        }
        (loaded, error)
    }

    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// Release a dormant character's event data while retaining its project metadata.
    pub fn free_samples(&mut self, name: &str) -> Result<(), FmodError> {
        let Some(i) = self.projects.iter().position(|p| p.name == name) else {
            return Ok(());
        };
        let mut first = None;
        unsafe {
            let mut n = 0;
            check(
                "FMOD_EventProject_GetNumGroups",
                (self.api.project_groups)(self.handles[i], &mut n),
            )?;
            for g in 0..n {
                let mut group = std::ptr::null_mut();
                let result = check(
                    "FMOD_EventProject_GetGroupByIndex",
                    (self.api.project_group)(self.handles[i], g, 0, &mut group),
                )
                .and_then(|()| {
                    check(
                        "FMOD_EventGroup_FreeEventData",
                        (self.api.group_free)(group, std::ptr::null_mut(), 1),
                    )
                });
                if let Err(error) = result {
                    first.get_or_insert(error);
                }
            }
        }
        first.map_or(Ok(()), Err)
    }
    /// The full path of event `key`, if a loaded project has it.
    pub fn path_of(&self, key: &str) -> Option<&str> {
        self.index.get(key).map(String::as_str)
    }

    pub fn has(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// Keys that exist but could not be started on their first try, with the FMOD error.
    pub fn broken(&self) -> &HashMap<String, i32> {
        &self.broken
    }

    /// Starts event `key` at `position` (world space, metres).
    pub fn play(&mut self, key: &str, position: [f32; 3]) -> Played {
        let Some(path) = self.index.get(key) else {
            return Played::Unknown;
        };
        if let Some(&code) = self.broken.get(key) {
            return Played::Failed(code);
        }
        let c = CString::new(path.as_str()).expect("event paths have no NUL");
        let mut ev: Ptr = std::ptr::null_mut();
        let mut r = unsafe { (self.api.get_event)(self.es, c.as_ptr(), EVENT_DEFAULT, &mut ev) };
        if r == 0 && !ev.is_null() {
            let (pos, vel) = (FVec::from(position), FVec::default());
            unsafe {
                (self.api.event_3d)(ev, &pos, &vel, std::ptr::null());
                r = (self.api.event_start)(ev);
            }
        } else if r == 0 {
            r = -1;
        }
        if r == 0 {
            self.started.insert(key.to_string());
            Played::Started
        } else {
            // Resource exhaustion and playback limits are transient, even on the first play.
            // Only remember file/format errors, so a busy mixer cannot permanently mute a cue.
            if !self.started.contains(key) && unopenable_bank(r) {
                self.broken.insert(key.to_string(), r);
            }
            Played::Failed(r)
        }
    }

    /// Puts the listener (the camera) at `position` looking along `forward` with `up`.
    pub fn set_listener(&mut self, position: [f32; 3], forward: [f32; 3], up: [f32; 3]) {
        let (p, v, f, u) = (
            FVec::from(position),
            FVec::default(),
            FVec::from(forward),
            FVec::from(up),
        );
        unsafe {
            (self.api.listener)(self.es, 0, &p, &v, &f, &u);
        }
    }

    /// Scales the master category (all sounds).
    pub fn set_master_volume(&mut self, volume: f32) {
        unsafe {
            let mut master: Ptr = std::ptr::null_mut();
            if (self.api.category_by_index)(self.es, -1, &mut master) == 0 && !master.is_null() {
                (self.api.category_volume)(master, volume);
            }
        }
    }

    /// Runs FMOD's per-frame work: 3D positions, voice limits, finished events.
    pub fn update(&mut self) -> i32 {
        unsafe { (self.api.update)(self.es) }
    }

    /// Diagnostics: starts event `key` at `position`, lets FMOD mix a few updates and returns
    /// the summed audibility of its channels (volume after the event's levels, 3D rolloff and
    /// the categories; 1.0 is full scale), then stops it. Measure with nothing else playing.
    pub fn audibility(&mut self, key: &str, position: [f32; 3]) -> Option<f32> {
        let path = CString::new(self.index.get(key)?.as_str()).ok()?;
        unsafe {
            let mut ev: Ptr = std::ptr::null_mut();
            if (self.api.get_event)(self.es, path.as_ptr(), EVENT_DEFAULT, &mut ev) != 0
                || ev.is_null()
            {
                return None;
            }
            let (pos, vel) = (FVec::from(position), FVec::default());
            (self.api.event_3d)(ev, &pos, &vel, std::ptr::null());
            if (self.api.event_start)(ev) != 0 {
                return None;
            }
            for _ in 0..3 {
                (self.api.update)(self.es);
            }
            // The event's own channels hang below its channel group.
            let mut sum = 0.0;
            let mut group: Ptr = std::ptr::null_mut();
            if (self.api.event_channel_group)(ev, &mut group) == 0 && !group.is_null() {
                sum = self.group_audibility(group, 0);
            }
            (self.api.event_stop)(ev, 1);
            (self.api.update)(self.es);
            Some(sum)
        }
    }

    /// Summed audibility of the playing channels in `group` and its subgroups.
    unsafe fn group_audibility(&self, group: Ptr, depth: u32) -> f32 {
        let mut sum = 0.0;
        unsafe {
            let mut n = 0;
            (self.api.group_channels)(group, &mut n);
            for i in 0..n {
                let mut ch: Ptr = std::ptr::null_mut();
                let mut playing = 0;
                if (self.api.group_channel)(group, i, &mut ch) == 0
                    && !ch.is_null()
                    && (self.api.channel_playing)(ch, &mut playing) == 0
                    && playing != 0
                {
                    let mut a = 0.0;
                    (self.api.channel_audibility)(ch, &mut a);
                    sum += a;
                }
            }
            if depth < 4 {
                let mut n = 0;
                (self.api.group_subgroups)(group, &mut n);
                for i in 0..n {
                    let mut sub: Ptr = std::ptr::null_mut();
                    if (self.api.group_subgroup)(group, i, &mut sub) == 0 && !sub.is_null() {
                        sum += self.group_audibility(sub, depth + 1);
                    }
                }
            }
        }
        sum
    }

    /// Diagnostics: the designer properties of event `key` (3D distances, levels, limits) as
    /// (name, value) text.
    pub fn properties(&self, key: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let Some(path) = self
            .index
            .get(key)
            .and_then(|p| CString::new(p.as_str()).ok())
        else {
            return out;
        };
        unsafe {
            let mut ev: Ptr = std::ptr::null_mut();
            if (self.api.get_event)(self.es, path.as_ptr(), EVENT_INFOONLY, &mut ev) != 0
                || ev.is_null()
            {
                return out;
            }
            let mut n = 0;
            (self.api.event_properties)(ev, &mut n);
            for i in 0..n {
                let (mut index, mut name, mut kind) = (i, std::ptr::null(), 0);
                if (self.api.event_property_info)(ev, &mut index, &mut name, &mut kind) != 0 {
                    continue;
                }
                // FMOD_EVENTPROPERTY_TYPE: 0 int, 1 float, 2 string.
                let mut raw = [0u8; 8];
                if (self.api.event_property)(ev, i, raw.as_mut_ptr() as Ptr, 0) != 0 {
                    continue;
                }
                let value = match kind {
                    0 => i32::from_le_bytes(raw[..4].try_into().unwrap()).to_string(),
                    1 => f32::from_le_bytes(raw[..4].try_into().unwrap()).to_string(),
                    _ => c_str(usize::from_le_bytes(raw) as *const c_char),
                };
                out.push((c_str(name), value));
            }
        }
        out
    }

    /// Channels mixing right now.
    pub fn channels_playing(&self) -> i32 {
        let mut n = 0;
        if !self.sys.is_null() {
            unsafe {
                (self.api.channels_playing)(self.sys, &mut n);
            }
        }
        n
    }
}

impl Drop for EventSystem {
    fn drop(&mut self) {
        unsafe {
            (self.api.release)(self.es);
        }
    }
}

fn c_str(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

fn unopenable_bank(result: i32) -> bool {
    // FMOD Ex: FILE_BAD, FILE_EOF, FILE_NOTFOUND, FORMAT.
    matches!(result, 19 | 22 | 23 | 25)
}

/// The projects a fight against `enemy` (such as `c5060`) on map area `area` (such as `m11`)
/// loads, in lookup order: the common project (Wolf and shared sounds), the enemy's, the area's
/// sound projects, then the common SFX project. Each is skipped when missing.
pub fn fight_projects(enemy: &str, area: &str) -> Vec<String> {
    let mut out = vec!["main".to_string(), enemy.to_string()];
    for prefix in ["", "s", "x", "v", "r"] {
        out.push(format!("{prefix}{area}"));
    }
    out.push("smain".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_failures_do_not_poison_event_keys() {
        assert!(!unopenable_bank(43), "resource exhaustion can recover");
        assert!(!unopenable_bank(83), "an event playback limit can recover");
        assert!(unopenable_bank(23));
        assert!(unopenable_bank(25));
    }

    #[test]
    fn event_paths() {
        assert_eq!(
            event_path("main", "main", "c000004010"),
            "main/main/c000004010"
        );
        assert_eq!(
            event_path("c5060", "c5060", "c506000000"),
            "c5060/c5060/c506000000"
        );
    }

    #[test]
    fn owl_fight_projects() {
        assert_eq!(
            fight_projects("c5060", "m11"),
            [
                "main", "c5060", "m11", "sm11", "xm11", "vm11", "rm11", "smain"
            ]
        );
    }
}
