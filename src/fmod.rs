//! Sekiro's own sounds through the user's own FMOD runtime: fmodex64.dll + fmod_event64.dll from
//! the Sekiro install (FMOD Ex 4.4 Event API) load the game's event projects (extracted/sound/
//! main.fev, smain.fev and the enemy's cXXXX.fev with their .fsb banks) and play events by name,
//! so every sound gets the game's own variation, layering, volume and pitch rules.
//! Event paths are "project/group/event" with the TAE key as the event name, e.g.
//! "main/main/c000004010" (sekiro-extract fev-list prints them). Falls back to the PCM samples
//! (sound.rs) when the DLLs or an event are missing.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
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

/// A started FMOD event instance (see `Fmod::play_tracked`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EventHandle(Ptr);

// The handle is only an id: it is dereferenced by FMOD calls on the thread that owns the
// (NonSend) Fmod, so storing it in a Local is safe.
unsafe impl Send for EventHandle {}
unsafe impl Sync for EventHandle {}

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
    load: unsafe extern "system" fn(Ptr, *const c_char, Ptr, *mut Ptr) -> i32,
    /// The master category and its volume call (the Options sliders, set_mix).
    master: Ptr,
    set_category_volume: Option<unsafe extern "system" fn(Ptr, f32) -> i32>,
    /// The master's child categories by lower-case name (music, menu, se, voice, default), for
    /// the Options sliders (set_mix).
    mix: Vec<(String, Ptr)>,
    /// The mixer's reverb event (mixer/mixer/ntc_rev) once started (set_reverb).
    reverb: Ptr,
    /// The deathblow mixes' parameters (ntc_ninsatsu2, 1, 3: the exe's slot order) once their
    /// events run (set_deathblow_mix), and the values last set.
    ninsatsu: Option<[Ptr; 3]>,
    ninsatsu_last: [f32; 3],
    /// Projects tried and not loaded (no bank extracted), not tried again.
    failed: HashSet<String>,
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
            let mut master: Ptr = std::ptr::null_mut();
            let mut set_category_volume = None;
            if let (Ok(top), Ok(set_vol)) = (
                ev.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_EventSystem_GetCategoryByIndex"),
                ev.get::<unsafe extern "system" fn(Ptr, f32) -> i32>(b"FMOD_EventCategory_SetVolume"),
            ) {
                if top(es, -1, &mut master) == 0 && !master.is_null() {
                    set_vol(master, MASTER_VOLUME);
                    set_category_volume = Some(*set_vol);
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
                load,
                master,
                set_category_volume,
                mix: Vec::new(),
                reverb: std::ptr::null_mut(),
                ninsatsu: None,
                ninsatsu_last: [0.0; 3],
                failed: HashSet::new(),
            })
        }
    }

    /// The Options sound sliders (0-10). Ours, `volume`: the master category at MASTER_VOLUME *
    /// volume / 10. The game's (SoundMan update, loop at 0x14092f8f0 over the table built at
    /// 0x14092d489): five categories, each at its slot's value * (Menu: slot 5, else slot 6) *
    /// slot 1 InGameMaster (command 6, FUN_141c7b1d0): Default, Menu and SE take slot 3 Se, Music
    /// slot 2 Bgm, Voice slot 4 Voice; slots 2-4 = option bytes 4-6 / 10. gap: slots 1, 5 and 6
    /// (in-game master, menu, other menu) are taken as 1.0, their setters not traced.
    pub fn set_mix(&mut self, audio: &crate::config::AudioConfig) {
        let Some(set) = self.set_category_volume.filter(|_| !self.master.is_null()) else { return };
        if self.mix.is_empty() {
            unsafe {
                let l = &self._ev;
                if let (Ok(num), Ok(sub), Ok(info)) = (
                    l.get::<unsafe extern "system" fn(Ptr, *mut i32) -> i32>(b"FMOD_EventCategory_GetNumCategories"),
                    l.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_EventCategory_GetCategoryByIndex"),
                    l.get::<unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char) -> i32>(b"FMOD_EventCategory_GetInfo"),
                ) {
                    let mut n = 0;
                    num(self.master, &mut n);
                    for i in 0..n {
                        let (mut c, mut idx, mut name): (Ptr, i32, *const c_char) = (std::ptr::null_mut(), 0, std::ptr::null());
                        if sub(self.master, i, &mut c) == 0 && !c.is_null() && info(c, &mut idx, &mut name) == 0 && !name.is_null() {
                            self.mix.push((std::ffi::CStr::from_ptr(name).to_string_lossy().to_lowercase(), c));
                        }
                    }
                }
            }
        }
        let v = |x: f32| (x / 10.0).clamp(0.0, 1.0);
        unsafe {
            set(self.master, MASTER_VOLUME * v(audio.volume));
            for (name, c) in &self.mix {
                let slot = match name.as_str() {
                    "music" => audio.music,
                    "voice" => audio.voice,
                    "se" | "menu" | "default" => audio.se,
                    _ => continue,
                };
                set(*c, v(slot));
            }
        }
    }

    /// The space's reverb, the game's way: mixer.fev's ntc_rev event puts its effect chain (FMOD
    /// Echo -> Lowpass Simple -> SFX Reverb) on the Default category while it plays (seen in the
    /// DSP tree, test fmod_mixer_dsp); its parameter sd_reverb_size (0-8) is the space type
    /// (exe: the type eases to its target, FUN_14092fc30, and goes to the sound thread as
    /// "sd_reverb_size", FUN_1409312b0). gap: which other mixer events (ntc_main, ntc_battle_bgm
    /// ...) the game keeps running, and the easing time, are not traced; the value is set at once.
    pub fn set_reverb(&mut self, size: f32) {
        self.load_project("mixer");
        unsafe {
            if self.reverb.is_null() {
                let path = CString::new("mixer/mixer/ntc_rev").unwrap();
                let mut ev: Ptr = std::ptr::null_mut();
                if (self.get_event)(self.es, path.as_ptr(), 0, &mut ev) != 0 || ev.is_null() || (self.start)(ev) != 0 {
                    warn!("FMOD: mixer/mixer/ntc_rev not started: no reverb");
                    return;
                }
                self.reverb = ev;
            }
            let (Ok(get), Ok(set)) = (
                self._ev.get::<unsafe extern "system" fn(Ptr, *const c_char, *mut Ptr) -> i32>(b"FMOD_Event_GetParameter"),
                self._ev.get::<unsafe extern "system" fn(Ptr, f32) -> i32>(b"FMOD_EventParameter_SetValue"),
            ) else {
                return;
            };
            let name = CString::new("sd_reverb_size").unwrap();
            let mut prm: Ptr = std::ptr::null_mut();
            if get(self.reverb, name.as_ptr(), &mut prm) == 0 && !prm.is_null() {
                set(prm, size);
            }
        }
    }

    /// The deathblow mixes: mixer.fev's ntc_ninsatsu2 / 1 / 3 (the exe's slots 0-2, names at
    /// 0x143b139b8) each put an FMOD Lowpass Simple on ChannelGroup:Default whose cutoff follows
    /// the event's own 0-1 parameter of the same name: 22 kHz at 0; at 1 ntc_ninsatsu2 445 Hz,
    /// ntc_ninsatsu1 1332 Hz, ntc_ninsatsu3 10 Hz (test fmod_ninsatsu_mix). The exe sends the
    /// three values by name (FUN_140931320); here the three events run from the first non-zero
    /// call on (at 0 they change nothing).
    pub fn set_deathblow_mix(&mut self, v: [f32; 3]) {
        if self.ninsatsu.is_none() {
            if v == [0.0; 3] {
                return;
            }
            self.load_project("mixer");
            let Ok(get) = (unsafe { self._ev.get::<unsafe extern "system" fn(Ptr, *const c_char, *mut Ptr) -> i32>(b"FMOD_Event_GetParameter") }) else { return };
            let mut prms: [Ptr; 3] = [std::ptr::null_mut(); 3];
            for (i, k) in ["ntc_ninsatsu2", "ntc_ninsatsu1", "ntc_ninsatsu3"].iter().enumerate() {
                let path = CString::new(format!("mixer/mixer/{k}")).unwrap();
                let name = CString::new(*k).unwrap();
                let mut ev: Ptr = std::ptr::null_mut();
                unsafe {
                    if (self.get_event)(self.es, path.as_ptr(), 0, &mut ev) == 0 && !ev.is_null() && (self.start)(ev) == 0 {
                        get(ev, name.as_ptr(), &mut prms[i]);
                    }
                }
            }
            if prms.iter().any(|p| p.is_null()) {
                warn!("FMOD: the deathblow mixes (mixer/mixer/ntc_ninsatsu1-3) did not all start");
            }
            self.ninsatsu = Some(prms);
        }
        let Some(prms) = self.ninsatsu else { return };
        let Ok(set) = (unsafe { self._ev.get::<unsafe extern "system" fn(Ptr, f32) -> i32>(b"FMOD_EventParameter_SetValue") }) else { return };
        for i in 0..3 {
            if !prms[i].is_null() && v[i] != self.ninsatsu_last[i] {
                unsafe { set(prms[i], v[i]) };
            }
        }
        self.ninsatsu_last = v;
    }

    /// Loads project `<name>.fev` from the media path unless it is loaded (or failed before).
    pub fn load_project(&mut self, name: &str) {
        if self.projects.iter().any(|p| p == name) || self.failed.contains(name) {
            return;
        }
        let file = CString::new(format!("{name}.fev")).unwrap();
        let mut proj: Ptr = std::ptr::null_mut();
        match unsafe { (self.load)(self.es, file.as_ptr(), std::ptr::null_mut(), &mut proj) } {
            0 => {
                info!("FMOD: {name}.fev loaded");
                self.projects.push(name.to_string());
            }
            r => {
                warn!("FMOD: {name}.fev not loaded (error {r})");
                self.failed.insert(name.to_string());
            }
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

    /// Starts event `key` and returns its instance, to stop it later (a looping event tied to an
    /// FFX slot: it ends with the effect). None when nothing started.
    pub fn play_tracked(&mut self, key: &str, pos: Vec3) -> Option<EventHandle> {
        if self.missing.contains(key) {
            return None;
        }
        for p in &self.projects {
            let path = CString::new(format!("{p}/{p}/{key}")).unwrap();
            let mut ev: Ptr = std::ptr::null_mut();
            unsafe {
                if (self.get_event)(self.es, path.as_ptr(), 0, &mut ev) == 0 && !ev.is_null() {
                    let (pos, vel) = (FVec::from(pos), FVec::from(Vec3::ZERO));
                    (self.set_3d)(ev, &pos, &vel, std::ptr::null());
                    (self.start)(ev);
                    return Some(EventHandle(ev));
                }
            }
        }
        None
    }

    /// Moves a playing event (`play_tracked`) to `pos`.
    pub fn set_position(&mut self, ev: EventHandle, pos: Vec3) {
        let (pos, vel) = (FVec::from(pos), FVec::from(Vec3::ZERO));
        unsafe {
            (self.set_3d)(ev.0, &pos, &vel, std::ptr::null());
        }
    }

    /// Stops an event started by `play_tracked` (FMOD_Event_Stop, letting its fade-out play).
    pub fn stop(&mut self, ev: EventHandle) {
        unsafe {
            (self.event_stop)(ev.0, 0);
        }
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
        // Every enemy kind's bank (config enemy.chr and enemy.group, and the kinds the boss events
        // bring in when Combat is already loaded).
        let mut chrs: Vec<String> = match app.world().get_resource::<crate::data::Combat>() {
            Some(c) => c.kinds.iter().map(|k| k.foe.chr.clone()).collect(),
            None => app.world().get_resource::<crate::config::GameConfig>().map_or(vec!["c1020".to_string()], |c| {
                std::iter::once(c.enemy.chr.clone()).chain(c.enemy.group.iter().map(|g| g.chr.clone())).collect()
            }),
        };
        chrs.dedup();
        let mut banks = vec!["main", "smain"];
        banks.extend(chrs.iter().map(|c| c.as_str()));
        match Fmod::open(std::path::Path::new(&sekiro), &sound_dir, &banks, false) {
            Ok(f) => {
                // Swing, deflect, guard, flesh hit, armour, floor, enemy swing.
                let probe = ["c000004010", "c000004011", "c000006510", "s000003010", "c000001113", "c000001004", "c102004001"];
                let found: Vec<&str> = probe.iter().copied().filter(|k| f.has(k)).collect();
                info!("FMOD events: {:?}; probe {}/{} found {:?}", f.projects, found.len(), probe.len(), found);
                app.insert_non_send(f).add_systems(PostUpdate, (load_new_kinds, apply_volume, apply_reverb, boss_music, update_fmod).chain());
            }
            Err(e) => warn!("FMOD event system unavailable ({e}); using the PCM samples"),
        }
    }
}

/// An enemy kind brought in at runtime (the encounter picker, enemy.rs SpawnEncounter) gets its
/// chr bank, as the startup kinds do.
fn load_new_kinds(mut fmod: NonSendMut<Fmod>, combat: Option<Res<crate::data::Combat>>) {
    let Some(combat) = combat else { return };
    if !combat.is_changed() {
        return;
    }
    for k in &combat.kinds {
        fmod.load_project(&k.foe.chr);
    }
}

/// The Options sound sliders (config [audio]) on the categories, when they change.
fn apply_volume(mut fmod: NonSendMut<Fmod>, config: Res<crate::config::GameConfig>, mut last: Local<Option<crate::config::AudioConfig>>) {
    if last.as_ref() != Some(&config.audio) {
        fmod.set_mix(&config.audio);
        *last = Some(config.audio.clone());
    }
}

/// The arena's reverb (map SoundSpaceType; no map: 0, none) when it changes.
/// A global sound parameter easing to a whole-number target (exe FUN_14092eb30 sets the target,
/// FUN_14092fc30 eases it each frame): the first target is taken at once (1e-6 s), a new one is
/// reached in a straight line over the fade time, 2.6 s for sd_reverb_size (sound manager
/// +0x200, set by its constructor FUN_14092cec0; only the debug menu edits it).
#[derive(Default)]
pub(crate) struct ParamEase {
    target: Option<u8>,
    value: f32,
    left: f32,
}

impl ParamEase {
    pub(crate) const REVERB_FADE: f32 = 2.6;

    /// Sets the target; the new value when it changed this frame.
    pub(crate) fn step(&mut self, target: u8, fade: f32, dt: f32) -> Option<f32> {
        match self.target {
            None => {
                self.target = Some(target);
                self.value = target as f32;
                return Some(self.value);
            }
            Some(t) if t != target => {
                self.target = Some(target);
                self.left = fade;
            }
            _ => {}
        }
        if self.left <= 0.0 {
            return None;
        }
        let goal = target as f32;
        if dt < self.left {
            self.value += (goal - self.value) * dt / self.left;
            self.left -= dt;
        } else {
            self.value = goal;
            self.left = 0.0;
        }
        Some(self.value)
    }
}

fn apply_reverb(mut fmod: NonSendMut<Fmod>, map: Option<Res<crate::map::MapLighting>>, time: Res<Time>, mut ease: Local<ParamEase>) {
    let v = std::env::var("SHINOBI_REVERB").ok().and_then(|v| v.parse().ok()).unwrap_or(map.map_or(0, |m| m.sound_space));
    if ease.target != Some(v) {
        info!("FMOD: reverb sd_reverb_size -> {v}");
    }
    if let Some(x) = ease.step(v, ParamEase::REVERB_FADE, time.delta_secs()) {
        fmod.set_reverb(x);
    }
}

/// Listener at the camera, then FMOD's per-frame update.
/// A boss's music (tools/boss_bgm.py -> extracted/enemies/boss_bgm.json, keyed by NpcParam row):
/// its tracks in order, each the FMOD event "m<id>" of the area's sm<area>.fev and the boss-script
/// event flags it plays between (none: from the fight's start / until the defeat).
#[derive(serde::Deserialize, Clone, Debug)]
struct BgmEntry {
    bank: String,
    tracks: Vec<BgmTrack>,
}

#[derive(serde::Deserialize, Clone, Debug)]
struct BgmTrack {
    event: String,
    start: Option<i64>,
    stop: Option<i64>,
}

fn boss_bgm() -> &'static HashMap<i64, BgmEntry> {
    static BGM: std::sync::OnceLock<HashMap<i64, BgmEntry>> = std::sync::OnceLock::new();
    BGM.get_or_init(|| {
        let path = crate::paths::extracted().join("enemies/boss_bgm.json");
        let rows: HashMap<String, BgmEntry> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        rows.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect()
    })
}

/// The track a boss's music plays now: the game's common_func 20005830 / 20005831 (the arena's
/// InitializeCommonEvent): a track plays once its start flag is on until its stop flag is; the
/// two-track event switches to track 2 when the phase flag goes on (2010[8] region 2, 257) and
/// turns every BGM off at the defeat (2010[8] -1, 256). The latest track that holds wins. The
/// first track starts with the fight: its start flag is the end of the boss's intro (Gyoubu's
/// 11105801 waits for his entrance anim's event message 20 and 4 s), and Sv1's fights start
/// without intros.
fn bgm_track<'a>(entry: &'a BgmEntry, on: &HashSet<i64>) -> Option<&'a BgmTrack> {
    entry.tracks.iter().enumerate().rev().find(|(i, t)| (*i == 0 || t.start.is_none_or(|f| on.contains(&f))) && t.stop.is_none_or(|f| !on.contains(&f))).map(|(_, t)| t)
}

/// Plays the fought boss's music while the fight is on (playing or paused) and the boss script
/// has not handled his defeat; stops it (FMOD's own fade-out) on a track change, a death, the
/// title or the win. At the listener (the camera): the events' only parameter is "(distance)"
/// (0-10 m). gap: ordinary fights' battle music (common 410 / the arena's 戦闘 regions) and the
/// crossfade time of 2010[8] 257 are not done.
fn boss_music(
    mut fmod: NonSendMut<Fmod>,
    script: Option<Res<crate::enemy::BossScript>>,
    screen: Option<Res<State<crate::menu::Screen>>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut playing: Local<Option<(String, EventHandle)>>,
) {
    let fighting = screen.as_ref().is_none_or(|s| matches!(s.get(), crate::menu::Screen::Playing | crate::menu::Screen::Paused));
    let run = script.as_ref().and_then(|s| s.run.as_ref()).filter(|r| fighting && r.defeated.is_none());
    let want = run.and_then(|r| {
        let entry = boss_bgm().get(&r.row)?;
        let on: HashSet<i64> = r.flags_on().into_iter().collect();
        Some((entry.bank.as_str(), bgm_track(entry, &on)?.event.as_str()))
    });
    if playing.as_ref().map(|p| p.0.as_str()) != want.map(|w| w.1) {
        if let Some((key, ev)) = playing.take() {
            info!("FMOD: boss music {key} off");
            fmod.stop(ev);
        }
        if let Some((bank, key)) = want {
            fmod.load_project(bank);
            let at = camera.single().map_or(Vec3::ZERO, |c| c.translation());
            if let Some(ev) = fmod.play_tracked(key, at) {
                info!("FMOD: boss music {key} ({bank})");
                *playing = Some((key.to_string(), ev));
            }
        }
    }
    if let (Some((_, ev)), Ok(c)) = (playing.as_ref(), camera.single()) {
        let ev = *ev;
        fmod.set_position(ev, c.translation());
    }
}

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
mod ease_tests {
    use super::ParamEase;

    /// The first reverb is set at once; a change goes there in a straight line over 2.6 s.
    #[test]
    fn reverb_eases_over_its_fade() {
        let mut e = ParamEase::default();
        assert_eq!(e.step(2, 2.6, 0.1), Some(2.0));
        assert_eq!(e.step(2, 2.6, 0.1), None);
        let mut v = 0.0;
        for _ in 0..13 {
            v = e.step(6, 2.6, 0.1).unwrap();
        }
        assert!((v - 4.0).abs() < 1e-3, "{v}");
        for _ in 0..14 {
            v = e.step(6, 2.6, 0.1).unwrap_or(v);
        }
        assert_eq!(v, 6.0);
        assert_eq!(e.step(6, 2.6, 0.1), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Which of the Mortal Draw (a106) sounds keep playing: starts each (silent output), waits
    /// 6 s, prints whether FMOD still reports it playing (a loop).
    /// cargo test --release fmod_loops -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_loops() {
        let root = crate::paths::root();
        let mut f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &["main", "smain"], false).expect("fmod");
        let get_state = unsafe { *f._ev.get::<unsafe extern "system" fn(Ptr, *mut u32) -> i32>(b"FMOD_Event_GetState").unwrap() };
        let keys: Vec<String> = (440000..=440009)
            .map(|i| format!("s{i:09}"))
            .chain(["c000004320", "c000004103", "c000007550", "c000007551", "c000007552", "c000006501", "c000006503", "c000004003"].iter().map(|s| s.to_string()))
            .collect();
        let mut started = Vec::new();
        for k in &keys {
            if let Some(ev) = f.play_tracked(k, Vec3::ZERO) {
                started.push((k.clone(), ev));
            } else {
                println!("{k}: no event");
            }
        }
        for _ in 0..60 {
            unsafe { (f.update)(f.es) };
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        for (k, ev) in &started {
            let mut st = 0u32;
            unsafe { get_state(ev.0, &mut st) };
            println!("{k}: state {st:#x} {}", if st & 0x8 != 0 { "STILL PLAYING after 6 s (loop)" } else { "done" });
        }
    }

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

    /// The mixer project's events (multimix.mix: ntc_rev, ntc_main, ntc_ninsatsu1-3 ...) and
    /// their parameters (ntc_rev: sd_reverb_size).
    /// cargo test --release fmod_mixer -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_mixer() {
        let root = crate::paths::root();
        let f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &["mixer"], true).expect("fmod");
        for k in ["ntc_rev", "ntc_menu_rev", "ntc_through", "ntc_main", "ntc_enemy_rev", "ntc_ninsatsu1", "ntc_ninsatsu2", "ntc_ninsatsu3", "ntc_battle_bgm", "ntc_boss_bgm"] {
            println!("{k}: has {} {:?}", f.has(k), f.params_of(k));
        }
    }

    /// Which events take the exe's global parameters room / deep / sd_dimension (slots 1-3 of
    /// FUN_14092cec0): every event of the listed projects, parameter names counted.
    /// cargo test --release fmod_global_params -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_global_params() {
        let root = crate::paths::root();
        let projs = ["main", "mixer", "m11", "sm11", "xm11", "vm11", "c0000", "c1020"];
        let f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &projs, true).expect("fmod");
        unsafe {
            let l = &f._ev;
            let get_proj = l.get::<unsafe extern "system" fn(Ptr, *const c_char, *mut Ptr) -> i32>(b"FMOD_EventSystem_GetProject").unwrap();
            let n_ev = l.get::<unsafe extern "system" fn(Ptr, *mut i32) -> i32>(b"FMOD_EventProject_GetNumEvents").unwrap();
            let by_id = l.get::<unsafe extern "system" fn(Ptr, i32, u32, *mut Ptr) -> i32>(b"FMOD_EventProject_GetEventByProjectID").unwrap();
            let ev_info = l.get::<unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char, Ptr) -> i32>(b"FMOD_Event_GetInfo").unwrap();
            let num = l.get::<unsafe extern "system" fn(Ptr, *mut i32) -> i32>(b"FMOD_Event_GetNumParameters").unwrap();
            let by_index = l.get::<unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32>(b"FMOD_Event_GetParameterByIndex").unwrap();
            let info = l.get::<unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char) -> i32>(b"FMOD_EventParameter_GetInfo").unwrap();
            let range = l.get::<unsafe extern "system" fn(Ptr, *mut f32, *mut f32) -> i32>(b"FMOD_EventParameter_GetRange").unwrap();
            let cs = |p: *const c_char| if p.is_null() { String::new() } else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() };
            for p in projs {
                let name = CString::new(p).unwrap();
                let mut proj: Ptr = std::ptr::null_mut();
                if get_proj(f.es, name.as_ptr(), &mut proj) != 0 || proj.is_null() {
                    println!("{p}: not loaded");
                    continue;
                }
                let mut n = 0;
                n_ev(proj, &mut n);
                let mut counts: HashMap<String, usize> = HashMap::new();
                let mut hits = Vec::new();
                for id in 0..n {
                    let mut ev: Ptr = std::ptr::null_mut();
                    if by_id(proj, id, 4, &mut ev) != 0 || ev.is_null() {
                        continue;
                    }
                    let (mut idx, mut en) = (0, std::ptr::null());
                    ev_info(ev, &mut idx, &mut en, std::ptr::null_mut());
                    let mut np = 0;
                    num(ev, &mut np);
                    for i in 0..np {
                        let mut prm: Ptr = std::ptr::null_mut();
                        if by_index(ev, i, &mut prm) != 0 || prm.is_null() {
                            continue;
                        }
                        let (mut pi, mut pn, mut lo, mut hi) = (0, std::ptr::null(), 0.0, 0.0);
                        info(prm, &mut pi, &mut pn);
                        range(prm, &mut lo, &mut hi);
                        let pn = cs(pn);
                        *counts.entry(pn.clone()).or_default() += 1;
                        if matches!(pn.as_str(), "room" | "deep" | "sd_dimension") && hits.len() < 12 {
                            hits.push(format!("{} {pn} {lo}..{hi}", cs(en)));
                        }
                    }
                }
                let mut c: Vec<_> = counts.into_iter().collect();
                c.sort_by(|a, b| b.1.cmp(&a.1));
                println!("{p}: {n} events, params {:?}", &c[..c.len().min(15)]);
                println!("  {hits:?}");
            }
        }
    }

    /// Boss music follows the arena script's flags (boss_bgm.json): Gyoubu's second track at his
    /// phase flag (11105812: <= 1 bar, 3 s), Genichiro's Way of Tomoe track once his 0-bars event
    /// (11115820) sets 11115803 / 11115802.
    #[test]
    fn boss_music_follows_the_phase_flags() {
        let bgm = boss_bgm();
        let track = |row: i64, on: &[i64]| bgm_track(&bgm[&row], &on.iter().copied().collect()).map(|t| t.event.clone());
        assert_eq!(track(50800000, &[]).as_deref(), Some("m110000001"));
        assert_eq!(track(50800000, &[11105802]).as_deref(), Some("m110000002"));
        assert_eq!(track(71001000, &[]).as_deref(), Some("m110100001"));
        assert_eq!(track(71001000, &[11115803, 11115802]).as_deref(), Some("m110100002"));
        assert_eq!(track(71001000, &[11115803, 11115802, 11110800]), None);
    }

    /// The boss music banks: each boss_bgm.json track is an event of its sm<area>.fev.
    /// cargo test --release fmod_boss_bgm -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_boss_bgm() {
        let root = crate::paths::root();
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("extracted/enemies/boss_bgm.json")).unwrap()).unwrap();
        let banks: std::collections::BTreeSet<String> = json.as_object().unwrap().values().map(|e| e["bank"].as_str().unwrap().to_string()).collect();
        let names: Vec<&str> = banks.iter().map(String::as_str).collect();
        let f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &names, true).expect("fmod");
        println!("loaded {:?}", f.projects);
        let mut missing = Vec::new();
        for (row, e) in json.as_object().unwrap() {
            for t in e["tracks"].as_array().unwrap() {
                let k = t["event"].as_str().unwrap();
                if !f.has(k) {
                    missing.push(format!("{row} {k}"));
                }
                println!("{row} {k}: has {} {:?}", f.has(k), f.params_of(k));
            }
        }
        assert!(missing.is_empty(), "{missing:?}");
        // Starting one (mode 0 loads its data).
        for k in ["m110000001", "m110100001"] {
            let path = CString::new(format!("sm11/sm11/{k}")).unwrap();
            let mut ev: Ptr = std::ptr::null_mut();
            let r = unsafe { (f.get_event)(f.es, path.as_ptr(), 0, &mut ev) };
            println!("get_event {k}: {r} {}", !ev.is_null());
        }
    }

    /// Does a started mixer event put its effects (SFX Reverb ...) into FMOD's DSP network?
    /// Prints the DSP tree before and after starting each one.
    /// cargo test --release fmod_mixer_dsp -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_mixer_dsp() {
        let root = crate::paths::root();
        let f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &["mixer", "main"], true).expect("fmod");
        type Get = unsafe extern "system" fn(Ptr, *mut Ptr) -> i32;
        unsafe {
            let head: libloading::Symbol<Get> = f._ex.get(b"FMOD_System_GetDSPHead").unwrap();
            let n_in: libloading::Symbol<unsafe extern "system" fn(Ptr, *mut i32) -> i32> = f._ex.get(b"FMOD_DSP_GetNumInputs").unwrap();
            let input: libloading::Symbol<unsafe extern "system" fn(Ptr, i32, *mut Ptr, *mut Ptr) -> i32> = f._ex.get(b"FMOD_DSP_GetInput").unwrap();
            let info: libloading::Symbol<unsafe extern "system" fn(Ptr, *mut c_char, *mut u32, *mut i32, *mut i32, *mut i32) -> i32> = f._ex.get(b"FMOD_DSP_GetInfo").unwrap();
            let tree = |label: &str| {
                let mut h: Ptr = std::ptr::null_mut();
                head(f.sys, &mut h);
                let mut out = Vec::new();
                let mut stack = vec![(h, 0usize)];
                while let Some((d, depth)) = stack.pop() {
                    if d.is_null() || out.len() > 400 {
                        continue;
                    }
                    let mut name = [0 as c_char; 32];
                    let (mut v, mut ch, mut w, mut hh) = (0u32, 0, 0, 0);
                    info(d, name.as_mut_ptr(), &mut v, &mut ch, &mut w, &mut hh);
                    let name = std::ffi::CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned();
                    if depth <= 8 {
                        out.push(format!("{}{}", "  ".repeat(depth), name));
                    }
                    let mut n = 0;
                    let rn = n_in(d, &mut n);
                    let _ = rn;
                    for i in 0..n {
                        let mut c: Ptr = std::ptr::null_mut();
                        input(d, i, &mut c, std::ptr::null_mut());
                        stack.push((c, depth + 1));
                    }
                }
                println!("--- {label}: {} nodes\n{}", out.len(), out.join("\n"));
            };
            for _ in 0..3 {
                (f.update)(f.es);
            }
            tree("before");
            for k in ["ntc_main", "ntc_rev"] {
                let path = CString::new(format!("mixer/mixer/{k}")).unwrap();
                let mut ev: Ptr = std::ptr::null_mut();
                let r = (f.get_event)(f.es, path.as_ptr(), 0, &mut ev);
                let s = if ev.is_null() { -1 } else { (f.start)(ev) };
                for _ in 0..5 {
                    (f.update)(f.es);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                tree(&format!("after {k} (get {r} start {s})"));
            }
        }
    }

    /// What the deathblow mixes do: starts mixer/mixer/ntc_ninsatsu1-3 one at a time, sets their
    /// own 0-1 parameter to 0 then 1 and prints every DSP with its parameter values, and the
    /// categories' volumes.
    /// cargo test --release fmod_ninsatsu_mix -- --ignored --nocapture
    #[test]
    #[ignore]
    fn fmod_ninsatsu_mix() {
        let root = crate::paths::root();
        let f = Fmod::open(std::path::Path::new(&crate::paths::sekiro_dir()), &root.join("extracted/sound"), &["mixer", "main"], true).expect("fmod");
        type Get = unsafe extern "system" fn(Ptr, *mut Ptr) -> i32;
        unsafe {
            let head: libloading::Symbol<Get> = f._ex.get(b"FMOD_System_GetDSPHead").unwrap();
            let n_in: libloading::Symbol<unsafe extern "system" fn(Ptr, *mut i32) -> i32> = f._ex.get(b"FMOD_DSP_GetNumInputs").unwrap();
            let input: libloading::Symbol<unsafe extern "system" fn(Ptr, i32, *mut Ptr, *mut Ptr) -> i32> = f._ex.get(b"FMOD_DSP_GetInput").unwrap();
            let info: libloading::Symbol<unsafe extern "system" fn(Ptr, *mut c_char, *mut u32, *mut i32, *mut i32, *mut i32) -> i32> = f._ex.get(b"FMOD_DSP_GetInfo").unwrap();
            let n_par: libloading::Symbol<unsafe extern "system" fn(Ptr, *mut i32) -> i32> = f._ex.get(b"FMOD_DSP_GetNumParameters").unwrap();
            let par: libloading::Symbol<unsafe extern "system" fn(Ptr, i32, *mut f32, *mut c_char, i32) -> i32> = f._ex.get(b"FMOD_DSP_GetParameter").unwrap();
            let active: libloading::Symbol<unsafe extern "system" fn(Ptr, *mut i32) -> i32> = f._ex.get(b"FMOD_DSP_GetBypass").unwrap();
            let get_p = f._ev.get::<unsafe extern "system" fn(Ptr, *const c_char, *mut Ptr) -> i32>(b"FMOD_Event_GetParameter").unwrap();
            let set_p = f._ev.get::<unsafe extern "system" fn(Ptr, f32) -> i32>(b"FMOD_EventParameter_SetValue").unwrap();
            let cat_vol = f._ev.get::<unsafe extern "system" fn(Ptr, *mut f32) -> i32>(b"FMOD_EventCategory_GetVolume").unwrap();
            let cat = f._ev.get::<unsafe extern "system" fn(Ptr, *const c_char, *mut Ptr) -> i32>(b"FMOD_EventSystem_GetCategory").unwrap();
            let tree = |label: &str| {
                let mut h: Ptr = std::ptr::null_mut();
                head(f.sys, &mut h);
                let mut out = Vec::new();
                let mut stack = vec![(h, 0usize)];
                while let Some((d, depth)) = stack.pop() {
                    if d.is_null() || out.len() > 400 {
                        continue;
                    }
                    let mut name = [0 as c_char; 32];
                    let (mut v, mut ch, mut w, mut hh) = (0u32, 0, 0, 0);
                    info(d, name.as_mut_ptr(), &mut v, &mut ch, &mut w, &mut hh);
                    let name = std::ffi::CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned();
                    let mut np = 0;
                    n_par(d, &mut np);
                    let ps: Vec<String> = (0..np.min(12)).map(|i| { let mut x = 0f32; par(d, i, &mut x, std::ptr::null_mut(), 0); format!("{x:.2}") }).collect();
                    let mut by = 0;
                    active(d, &mut by);
                    if depth <= 8 && (np > 0 || depth <= 3) {
                        out.push(format!("{}{} bypass {} {:?}", "  ".repeat(depth), name, by, ps));
                    }
                    let mut n = 0;
                    n_in(d, &mut n);
                    for i in 0..n {
                        let mut c: Ptr = std::ptr::null_mut();
                        input(d, i, &mut c, std::ptr::null_mut());
                        stack.push((c, depth + 1));
                    }
                }
                let vols: Vec<String> = ["master", "master/music", "master/Menu", "master/Voice", "master/Default", "music", "Menu", "Voice", "Default"]
                    .iter()
                    .filter_map(|c| { let n = CString::new(*c).unwrap(); let mut p: Ptr = std::ptr::null_mut(); (cat(f.es, n.as_ptr(), &mut p) == 0 && !p.is_null()).then(|| { let mut v = 0f32; cat_vol(p, &mut v); format!("{c} {v:.2}") }) })
                    .collect();
                println!("--- {label}
{}
  categories: {vols:?}", out.join("
"));
            };
            let pump = || for _ in 0..5 { (f.update)(f.es); std::thread::sleep(std::time::Duration::from_millis(20)); };
            pump();
            tree("before");
            for k in ["ntc_ninsatsu1", "ntc_ninsatsu2", "ntc_ninsatsu3"] {
                let path = CString::new(format!("mixer/mixer/{k}")).unwrap();
                let mut ev: Ptr = std::ptr::null_mut();
                let r = (f.get_event)(f.es, path.as_ptr(), 0, &mut ev);
                let s = if ev.is_null() { -1 } else { (f.start)(ev) };
                let name = CString::new(k).unwrap();
                let mut prm: Ptr = std::ptr::null_mut();
                get_p(ev, name.as_ptr(), &mut prm);
                for v in [0.0f32, 1.0] {
                    if !prm.is_null() {
                        set_p(prm, v);
                    }
                    pump();
                    tree(&format!("{k} = {v} (get {r} start {s})"));
                }
                if !ev.is_null() {
                    let stop = f._ev.get::<unsafe extern "system" fn(Ptr, i32) -> i32>(b"FMOD_Event_Stop").unwrap();
                    stop(ev, 1);
                }
                pump();
            }
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
