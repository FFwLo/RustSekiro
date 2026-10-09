//! Sekiro's own sounds through the user's own FMOD runtime: fmodex64.dll + fmod_event64.dll from
//! the Sekiro install (FMOD Ex 4.4 Event API) load the game's event projects (extracted/sound/
//! main.fev, smain.fev and the enemy's cXXXX.fev with their .fsb banks) and play events by name,
//! so every sound gets the game's own variation, layering, volume and pitch rules.
//! Event paths are "project/group/event" with the TAE key as the event name, e.g.
//! "main/main/c000004010" (sekiro-extract fev-list prints them). Falls back to the PCM samples
//! (sound.rs) when the DLLs or an event are missing.

use bevy::prelude::*;
use std::collections::HashSet;
use std::ffi::{c_char, c_void, CString};

type Ptr = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct FVec {
    x: f32,
    y: f32,
    z: f32,
}

impl From<Vec3> for FVec {
    fn from(v: Vec3) -> Self {
        FVec { x: v.x, y: v.y, z: v.z }
    }
}

/// FMOD_INIT_3D_RIGHTHANDED (FMOD Ex 4.44: 0x2; 0x4 is FMOD_INIT_SOFTWARE_DISABLE, which turns
/// off the software mixer so no event can be created - every getEvent failed with error 16).
const INIT_3D_RIGHTHANDED: u32 = 0x2;

pub struct Fmod {
    _ex: libloading::Library,
    _ev: libloading::Library,
    es: Ptr,
    get_event: unsafe extern "system" fn(Ptr, *const c_char, u32, *mut Ptr) -> i32,
    set_3d: unsafe extern "system" fn(Ptr, *const FVec, *const FVec, *const FVec) -> i32,
    start: unsafe extern "system" fn(Ptr) -> i32,
    update: unsafe extern "system" fn(Ptr) -> i32,
    listener: unsafe extern "system" fn(Ptr, i32, *const FVec, *const FVec, *const FVec, *const FVec) -> i32,
    projects: Vec<String>,
    missing: HashSet<String>,
    /// fmodex64 low-level calls, for the sound map: which samples an event's channels play.
    sys: Ptr,
    get_channel: unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32,
    is_playing: unsafe extern "system" fn(Ptr, *mut i32) -> i32,
    current_sound: unsafe extern "system" fn(Ptr, *mut Ptr) -> i32,
    sound_name: unsafe extern "system" fn(Ptr, *mut c_char, i32) -> i32,
    event_stop: unsafe extern "system" fn(Ptr, i32) -> i32,
    event_volume: unsafe extern "system" fn(Ptr, f32) -> i32,
}

/// Master category volume at the default options: SoundMan Master 1.0 * 0.7 (exe command 10).
const MASTER_VOLUME: f32 = 0.7;

/// FMOD Ex FMOD_OUTPUTTYPE_NOSOUND: everything runs, nothing is heard (sound map).
const OUTPUT_NOSOUND: i32 = 2;

impl Fmod {
    fn open(sekiro: &std::path::Path, sound_dir: &std::path::Path, fevs: &[&str], silent: bool) -> Result<Self, String> {
        unsafe {
            // fmod_event64.dll imports fmodex64.dll: load that first so it resolves.
            let ex = libloading::Library::new(sekiro.join("fmodex64.dll")).map_err(|e| format!("fmodex64.dll: {e}"))?;
            let ev = libloading::Library::new(sekiro.join("fmod_event64.dll")).map_err(|e| format!("fmod_event64.dll: {e}"))?;
            macro_rules! sym {
                ($name:expr, $ty:ty) => {
                    *ev.get::<$ty>($name).map_err(|e| format!("{}: {e}", String::from_utf8_lossy($name)))?
                };
            }
            let create = sym!(b"FMOD_EventSystem_Create", unsafe extern "system" fn(*mut Ptr) -> i32);
            let init = sym!(b"FMOD_EventSystem_Init", unsafe extern "system" fn(Ptr, i32, u32, Ptr, u32) -> i32);
            let media = sym!(b"FMOD_EventSystem_SetMediaPath", unsafe extern "system" fn(Ptr, *const c_char) -> i32);
            let load = sym!(b"FMOD_EventSystem_Load", unsafe extern "system" fn(Ptr, *const c_char, Ptr, *mut Ptr) -> i32);
            let get_event = sym!(b"FMOD_EventSystem_GetEvent", unsafe extern "system" fn(Ptr, *const c_char, u32, *mut Ptr) -> i32);
            let set_3d = sym!(b"FMOD_Event_Set3DAttributes", unsafe extern "system" fn(Ptr, *const FVec, *const FVec, *const FVec) -> i32);
            let start = sym!(b"FMOD_Event_Start", unsafe extern "system" fn(Ptr) -> i32);
            let update = sym!(b"FMOD_EventSystem_Update", unsafe extern "system" fn(Ptr) -> i32);
            let listener = sym!(
                b"FMOD_EventSystem_Set3DListenerAttributes",
                unsafe extern "system" fn(Ptr, i32, *const FVec, *const FVec, *const FVec, *const FVec) -> i32
            );
            macro_rules! exsym {
                ($name:expr, $ty:ty) => {
                    *ex.get::<$ty>($name).map_err(|e| format!("{}: {e}", String::from_utf8_lossy($name)))?
                };
            }
            let get_system = sym!(b"FMOD_EventSystem_GetSystemObject", unsafe extern "system" fn(Ptr, *mut Ptr) -> i32);
            let event_stop = sym!(b"FMOD_Event_Stop", unsafe extern "system" fn(Ptr, i32) -> i32);
            let event_volume = sym!(b"FMOD_Event_SetVolume", unsafe extern "system" fn(Ptr, f32) -> i32);
            let set_output = exsym!(b"FMOD_System_SetOutput", unsafe extern "system" fn(Ptr, i32) -> i32);
            let get_channel = exsym!(b"FMOD_System_GetChannel", unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32);
            let is_playing = exsym!(b"FMOD_Channel_IsPlaying", unsafe extern "system" fn(Ptr, *mut i32) -> i32);
            let current_sound = exsym!(b"FMOD_Channel_GetCurrentSound", unsafe extern "system" fn(Ptr, *mut Ptr) -> i32);
            let sound_name = exsym!(b"FMOD_Sound_GetName", unsafe extern "system" fn(Ptr, *mut c_char, i32) -> i32);
            let mut es: Ptr = std::ptr::null_mut();
            if create(&mut es) != 0 {
                return Err("FMOD_EventSystem_Create failed".into());
            }
            let mut sys: Ptr = std::ptr::null_mut();
            get_system(es, &mut sys);
            if std::env::var("FMOD_FILE_LOG").is_ok() {
                unsafe extern "system" fn on_open(name: *const c_char, _u: i32, _size: *mut u32, _h: *mut Ptr, _ud: *mut Ptr) -> i32 {
                    eprintln!("fmod open {}", unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy());
                    0
                }
                let attach = exsym!(b"FMOD_System_AttachFileSystem", unsafe extern "system" fn(Ptr, Ptr, Ptr, Ptr, Ptr) -> i32);
                let r = attach(sys, on_open as Ptr, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
                eprintln!("attach file log {r}");
            }
            if silent && !sys.is_null() {
                set_output(sys, OUTPUT_NOSOUND);
            }
            let r = init(es, 128, INIT_3D_RIGHTHANDED, std::ptr::null_mut(), 0);
            if r != 0 {
                return Err(format!("FMOD_EventSystem_Init: FMOD error {r}"));
            }
            let dir = CString::new(format!("{}/", sound_dir.display())).unwrap();
            media(es, dir.as_ptr());
            let mut projects = Vec::new();
            for &f in fevs {
                let name = CString::new(format!("{f}.fev")).unwrap();
                let mut proj: Ptr = std::ptr::null_mut();
                match load(es, name.as_ptr(), std::ptr::null_mut(), &mut proj) {
                    0 => projects.push(f.to_string()),
                    r => warn!("FMOD: {f}.fev not loaded (error {r})"),
                }
            }
            // The game's mix (SoundMan update FUN_14092f??? -> command 10, FUN_141c7b760): the master
            // category plays at the Master volume * 0.7; the SE / Music / Voice categories at their
            // option slider / 10 (GameDataMan options +0x50, bytes 4 / 5 / 6, default 10 each:
            // FUN_1407bc470) = 1.0. Without the 0.7 everything was ~3 dB louder than the game.
            if let (Ok(top), Ok(set_vol)) = (
                ev.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_EventSystem_GetCategoryByIndex"),
                ev.get::<unsafe extern "system" fn(Ptr, f32) -> i32>(b"FMOD_EventCategory_SetVolume"),
            ) {
                let mut master: Ptr = std::ptr::null_mut();
                if top(es, -1, &mut master) == 0 && !master.is_null() {
                    set_vol(master, MASTER_VOLUME);
                }
            }
            Ok(Fmod {
                _ex: ex,
                _ev: ev,
                es,
                get_event,
                set_3d,
                start,
                update,
                listener,
                projects,
                missing: HashSet::new(),
                sys,
                get_channel,
                is_playing,
                current_sound,
                sound_name,
                event_stop,
                event_volume,
            })
        }
    }

    /// Whether a loaded project has event `key` (FMOD_EVENT_INFOONLY: nothing plays).
    pub fn has(&self, key: &str) -> bool {
        self.projects.iter().any(|p| {
            let path = CString::new(format!("{p}/{p}/{key}")).unwrap();
            let mut ev: Ptr = std::ptr::null_mut();
            unsafe { (self.get_event)(self.es, path.as_ptr(), 4, &mut ev) == 0 && !ev.is_null() }
        })
    }

    /// Starts event `key` (e.g. "c000004010") at `pos`; false when no loaded project has it.
    /// An existing event FMOD declines right now (its max-playbacks limit) is skipped this time
    /// only - as in the game - and is not marked missing.
    pub fn play(&mut self, key: &str, pos: Vec3) -> bool {
        if self.missing.contains(key) {
            return false;
        }
        for p in &self.projects {
            let path = CString::new(format!("{p}/{p}/{key}")).unwrap();
            let mut ev: Ptr = std::ptr::null_mut();
            unsafe {
                if (self.get_event)(self.es, path.as_ptr(), 0, &mut ev) == 0 && !ev.is_null() {
                    let (pos, vel) = (FVec::from(pos), FVec::from(Vec3::ZERO));
                    (self.set_3d)(ev, &pos, &vel, std::ptr::null());
                    (self.start)(ev);
                    return true;
                }
            }
        }
        if self.has(key) {
            return true;
        }
        self.missing.insert(key.to_string());
        false
    }

    /// Category tree (name, volume, depth) of the loaded projects, for the mixing checks.
    pub fn categories(&self) -> Vec<(String, f32, usize)> {
        let mut out = Vec::new();
        unsafe {
            let l = &self._ev;
            let (Ok(top), Ok(info), Ok(vol), Ok(num), Ok(sub)) = (
                l.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_EventSystem_GetCategoryByIndex"),
                l.get::<unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char) -> i32>(b"FMOD_EventCategory_GetInfo"),
                l.get::<unsafe extern "system" fn(Ptr, *mut f32) -> i32>(b"FMOD_EventCategory_GetVolume"),
                l.get::<unsafe extern "system" fn(Ptr, *mut i32) -> i32>(b"FMOD_EventCategory_GetNumCategories"),
                l.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_EventCategory_GetCategoryByIndex"),
            ) else {
                return out;
            };
            fn walk(c: Ptr, depth: usize, out: &mut Vec<(String, f32, usize)>, info: &dyn Fn(Ptr, *mut i32, *mut *const c_char) -> i32, vol: &dyn Fn(Ptr, *mut f32) -> i32, num: &dyn Fn(Ptr, *mut i32) -> i32, sub: &dyn Fn(Ptr, i32, *mut Ptr) -> i32) {
                let (mut idx, mut name, mut v, mut n) = (0, std::ptr::null(), 0.0f32, 0);
                info(c, &mut idx, &mut name);
                vol(c, &mut v);
                let name = if name.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned() };
                out.push((name, v, depth));
                num(c, &mut n);
                for i in 0..n {
                    let mut s: Ptr = std::ptr::null_mut();
                    if sub(c, i, &mut s) == 0 && !s.is_null() {
                        walk(s, depth + 1, out, info, vol, num, sub);
                    }
                }
            }
            let mut master: Ptr = std::ptr::null_mut();
            if top(self.es, -1, &mut master) == 0 && !master.is_null() {
                walk(master, 0, &mut out, &|a, b, c| info(a, b, c), &|a, b| vol(a, b), &|a, b| num(a, b), &|a, b, c| sub(a, b, c));
            }
        }
        out
    }

    /// Event parameters (name, min, max, default value) of event `key`, for debugging which layer a
    /// parameter selects.
    pub fn params_of(&self, key: &str) -> Vec<(String, f32, f32, f32)> {
        let mut out = Vec::new();
        unsafe {
            let ev_lib = &self._ev;
            let (Ok(num), Ok(by_index), Ok(info), Ok(range), Ok(value)) = (
                ev_lib.get::<unsafe extern "system" fn(Ptr, *mut i32) -> i32>(b"FMOD_Event_GetNumParameters"),
                ev_lib.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_Event_GetParameterByIndex"),
                ev_lib.get::<unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char) -> i32>(b"FMOD_EventParameter_GetInfo"),
                ev_lib.get::<unsafe extern "system" fn(Ptr, *mut f32, *mut f32) -> i32>(b"FMOD_EventParameter_GetRange"),
                ev_lib.get::<unsafe extern "system" fn(Ptr, *mut f32) -> i32>(b"FMOD_EventParameter_GetValue"),
            ) else {
                return out;
            };
            for p in &self.projects {
                let path = CString::new(format!("{p}/{p}/{key}")).unwrap();
                let mut ev: Ptr = std::ptr::null_mut();
                if (self.get_event)(self.es, path.as_ptr(), 4, &mut ev) != 0 || ev.is_null() {
                    continue;
                }
                let mut n = 0;
                num(ev, &mut n);
                for i in 0..n {
                    let mut prm: Ptr = std::ptr::null_mut();
                    if by_index(ev, i, &mut prm) != 0 || prm.is_null() {
                        continue;
                    }
                    let (mut idx, mut name, mut lo, mut hi, mut v) = (0, std::ptr::null(), 0.0, 0.0, 0.0);
                    info(prm, &mut idx, &mut name);
                    range(prm, &mut lo, &mut hi);
                    value(prm, &mut v);
                    let name = if name.is_null() { String::new() } else { std::ffi::CStr::from_ptr(name).to_string_lossy().into_owned() };
                    out.push((name, lo, hi, v));
                }
                break;
            }
        }
        out
    }

    /// Sound map: starts event `key` `tries` times (muted) and returns the sample names its
    /// channels play (FMOD picks variants at random). None when no loaded project has the event.
    pub fn samples_of(&mut self, key: &str, tries: usize) -> Option<Vec<String>> {
        let mut out: Vec<String> = Vec::new();
        for _ in 0..tries {
            let ev = self.projects.iter().find_map(|p| {
                let path = CString::new(format!("{p}/{p}/{key}")).unwrap();
                let mut ev: Ptr = std::ptr::null_mut();
                unsafe { ((self.get_event)(self.es, path.as_ptr(), 0, &mut ev) == 0 && !ev.is_null()).then_some(ev) }
            })?;
            unsafe {
                (self.event_volume)(ev, 0.0);
                (self.start)(ev);
                for _ in 0..3 {
                    (self.update)(self.es);
                }
                for id in 0..128 {
                    let mut ch: Ptr = std::ptr::null_mut();
                    let mut playing = 0;
                    if (self.get_channel)(self.sys, id, &mut ch) != 0 || ch.is_null() || (self.is_playing)(ch, &mut playing) != 0 || playing == 0 {
                        continue;
                    }
                    let mut snd: Ptr = std::ptr::null_mut();
                    let mut name = [0 as c_char; 256];
                    if (self.current_sound)(ch, &mut snd) == 0 && !snd.is_null() && (self.sound_name)(snd, name.as_mut_ptr(), 255) == 0 {
                        let mut n = std::ffi::CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned();
                        // Names are stripped in the game's banks: identify the sample by its length
                        // (PCM samples; FMOD_TIMEUNIT_PCM = 2), matched to sound_pcm files.
                        if let Ok(get_len) = self._ex.get::<unsafe extern "system" fn(Ptr, *mut u32, u32) -> i32>(b"FMOD_Sound_GetLength") {
                            let mut len = 0u32;
                            if get_len(snd, &mut len, 2) == 0 {
                                n = format!("len{len}");
                            }
                        }
                        if !out.contains(&n) {
                            out.push(n);
                        }
                    }
                }
                (self.event_stop)(ev, 1);
                (self.update)(self.es);
            }
        }
        out.sort();
        Some(out)
    }
}

pub struct FmodPlugin;

impl Plugin for FmodPlugin {
    fn build(&self, app: &mut App) {
        let sekiro = crate::paths::sekiro_dir();
        let sound_dir = crate::paths::root().join("extracted/sound");
        let chr = app.world().get_resource::<crate::config::GameConfig>().map_or("c1020".to_string(), |c| c.enemy.chr.clone());
        match Fmod::open(std::path::Path::new(&sekiro), &sound_dir, &["main", "smain", chr.as_str()], false) {
            Ok(f) => {
                // Swing, deflect, guard, flesh hit, armour, floor, enemy swing.
                let probe = ["c000004010", "c000004011", "c000006510", "s000003010", "c000001113", "c000001004", "c102004001"];
                let found: Vec<&str> = probe.iter().copied().filter(|k| f.has(k)).collect();
                info!("FMOD events: {:?}; probe {}/{} found {:?}", f.projects, found.len(), probe.len(), found);
                app.insert_non_send(f).add_systems(PostUpdate, update_fmod);
            }
            Err(e) => warn!("FMOD event system unavailable ({e}); using the PCM samples"),
        }
    }
}

/// Listener at the camera, then FMOD's per-frame update.
fn update_fmod(mut fmod: NonSendMut<Fmod>, camera: Query<&GlobalTransform, With<Camera3d>>) {
    if let Ok(cam) = camera.single() {
        let t = cam.compute_transform();
        let (pos, vel, fwd, up) = (FVec::from(t.translation), FVec::from(Vec3::ZERO), FVec::from(*t.forward()), FVec::from(*t.up()));
        unsafe {
            (fmod.listener)(fmod.es, 0, &pos, &vel, &fwd, &up);
        }
    }
    unsafe {
        (fmod.update)(fmod.es);
    }
    let _ = &mut fmod.missing;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// cargo test --release fmod_categories -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_categories() {
        let root = crate::paths::root();
        let f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &["main", "smain", "c1020"], true).expect("fmod");
        for (n, v, d) in f.categories() {
            println!("{}{} {:.3}", "  ".repeat(d), n, v);
        }
        for k in ["c000004010", "c000004011", "c000006510", "s000003010", "c000001004", "c000001113", "z199999980", "z100000103", "c102004001"] {
            println!("{k}: {:?}", f.params_of(k));
        }
    }

    /// Sound map (auto debug): every TAE PlaySound event of every exported anim, keyed the way
    /// sound.rs keys it, played through the game's FMOD (silent) to list the samples it uses.
    /// Writes extracted/sound_map.txt and prints the suspicious ones (fire, magic, ...).
    /// cargo test --release sound_map -- --ignored --nocapture
    #[test]
    #[ignore]
    fn sound_map() {
        let root = crate::paths::root();
        let text = std::fs::read_to_string(root.join("extracted/combat_data.json")).expect("combat_data.json");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut chars: Vec<(String, bool, &serde_json::Value)> = vec![("c0000".into(), true, &v["player"])];
        for (k, c) in v["enemies"].as_object().unwrap() {
            chars.push((k.clone(), false, c));
        }
        // event key -> uses ("c0000 a000_000010 f3 PlaySound_CenterBody")
        let mut uses: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (chr, player, c) in &chars {
            for (anim, a) in c["anims"].as_object().unwrap() {
                for e in a["events"].as_array().unwrap() {
                    let kind = e["type"].as_i64().unwrap_or(0);
                    if !(128..=132).contains(&kind) || e["args"]["StateInfo"].as_i64().unwrap_or(0) != 0 {
                        continue;
                    }
                    let (Some(id), Some(letter)) = (
                        e["args"]["SoundID"].as_i64(),
                        e["args"]["SoundType"].as_str().and_then(|t| t.split_once('(')).and_then(|(_, r)| r.chars().next()),
                    ) else {
                        continue;
                    };
                    let key = match letter {
                        'x' => format!("c{:09}", id / 1000 * 1000 + 4),
                        'b' => format!("c{:09}", id + if *player { 113 } else { 108 }),
                        _ => format!("{letter}{id:09}"),
                    };
                    uses.entry(key).or_default().push(format!("{chr} {anim} f{} {}", e["startFrame"].as_f64().unwrap_or(0.0), e["name"].as_str().unwrap_or("")));
                }
            }
        }
        let sekiro = crate::paths::sekiro_dir();
        let mut f = Fmod::open(std::path::Path::new(&sekiro), &root.join("extracted/sound"), &["mixer", "main", "smain", "c1020", "c1010"], false).expect("FMOD");
        for k in ["c000004010", "c000004200", "c000004210", "c000004220", "c000009012", "c000400011", "c000001004", "c000002004", "c000000113", "c000001113"] {
            println!("params {k}: {:?}", f.params_of(k));
        }
        for k in ["s000101001", "s000101011", "s000101031", "s000101101", "s000132001", "s000103001", "z100000103", "z000000013", "z000000108", "z030000109", "z000000110", "c000001001", "c000002001", "c000003001", "c000001003", "c000001005", "c000001006", "c000001016", "c000001028", "c000011001", "c000001000", "z199999980", "z999999960", "z999999970", "z200000101", "s000252001", "s000253001", "c000000108", "c000000114", "c000001108", "c000001114", "c000002108", "c000002114", "c000000113", "c000001113", "c000002113"] {
            println!("has {k}: {}", f.has(k));
        }
        for k in ["s000003010", "s000003000", "s000003001"] {
            println!("samples {k}: {:?}", f.samples_of(k, 8));
        }
        let mut out = String::new();
        let mut flagged = Vec::new();
        for (key, u) in &uses {
            let samples = f.samples_of(key, 6);
            let line = match &samples {
                Some(s) => format!("{key} [{}] x{}", s.join(", "), u.len()),
                None => format!("{key} MISSING x{}", u.len()),
            };
            out += &line;
            out += "\n";
            for x in u.iter().take(12) {
                out += &format!("    {x}\n");
            }
            let bad = ["fire", "flame", "burn", "torch", "magic", "explo", "ignit"];
            if samples.iter().flatten().any(|s| bad.iter().any(|b| s.to_lowercase().contains(b))) {
                flagged.push(format!("{line}\n    {}", u.join("\n    ")));
            }
        }
        std::fs::write(root.join("extracted/sound_map.txt"), &out).unwrap();
        println!("{} sound keys -> extracted/sound_map.txt", uses.len());
        for l in &flagged {
            println!("FLAG {l}");
        }
    }
    /// Every event of every loaded project -> the samples it plays (lengths matched to
    /// extracted/sound_pcm names): extracted/sound_events.txt.
    /// cargo test --release sound_events -- --ignored --nocapture
    #[test]
    #[ignore]
    fn sound_events() {
        let root = crate::paths::root();
        let dir = root.join("extracted/sound_pcm");
        let mut by_len: std::collections::HashMap<u32, Vec<String>> = std::collections::HashMap::new();
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let d = std::fs::read(e.path()).unwrap_or_default();
            if d.len() < 12 || &d[..4] != b"SPCM" {
                continue;
            }
            let ch = u16::from_le_bytes([d[8], d[9]]).max(1) as usize;
            let frames = ((d.len() - 12) / 2 / ch) as u32;
            by_len.entry(frames).or_default().push(e.file_name().to_string_lossy().trim_end_matches(".pcm").to_string());
        }
        let sekiro = crate::paths::sekiro_dir();
        let mut f = Fmod::open(std::path::Path::new(&sekiro), &root.join("extracted/sound"), &["main", "smain", "c1020", "c1010"], false).expect("FMOD");
        let mut out = String::new();
        for proj in ["main", "smain", "c1020", "c1010"] {
            let fev = std::fs::read(root.join(format!("extracted/sound/{proj}.fev"))).unwrap_or_default();
            let pat = format!("/{proj}/");
            let mut names: Vec<String> = Vec::new();
            let mut i = 0;
            while let Some(p) = fev[i..].windows(pat.len()).position(|w| w == pat.as_bytes()) {
                let s0 = i + p + pat.len();
                let end = fev[s0..].iter().position(|&b| !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b' ')).map_or(fev.len(), |e| s0 + e);
                let name = String::from_utf8_lossy(&fev[s0..end]).to_string();
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
                i = end.max(s0 + 1);
            }
            for n in names {
                let Some(samples) = f.samples_of(&n, 6) else { continue };
                let named: Vec<String> = samples
                    .iter()
                    .map(|s| s.strip_prefix("len").and_then(|l| l.parse::<u32>().ok()).and_then(|l| by_len.get(&l)).map_or(s.clone(), |v| v.join("|")))
                    .collect();
                out += &format!("{proj}/{n}: {}
", named.join(", "));
            }
        }
        std::fs::write(root.join("extracted/sound_events.txt"), &out).unwrap();
        println!("{} events -> extracted/sound_events.txt", out.lines().count());
    }

}
