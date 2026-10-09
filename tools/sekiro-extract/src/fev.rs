//! FMOD Designer event projects (.fev) through the game's own fmod_event64.dll (FMOD Ex 4.4
//! Event API): load a project and list every event as "group/subgroup/event".

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;

type Ptr = *mut c_void;

pub fn list_events(sound_dir: &Path, dll: &Path, fev: &str) -> Result<Vec<String>, String> {
    unsafe {
        // fmod_event64.dll imports fmodex64.dll: load that first (same folder) so it resolves.
        let lib_ex = libloading::Library::new(dll.with_file_name("fmodex64.dll")).map_err(|e| format!("fmodex64: {e}"))?;
        let lib = libloading::Library::new(dll).map_err(|e| format!("fmod_event64: {e}"))?;
        macro_rules! sym {
            ($name:expr, $ty:ty) => {
                *lib.get::<$ty>($name).map_err(|e| format!("{}: {e}", String::from_utf8_lossy($name)))?
            };
        }
        let create = sym!(b"FMOD_EventSystem_Create", unsafe extern "system" fn(*mut Ptr) -> i32);
        let get_sys = sym!(b"FMOD_EventSystem_GetSystemObject", unsafe extern "system" fn(Ptr, *mut Ptr) -> i32);
        let init = sym!(b"FMOD_EventSystem_Init", unsafe extern "system" fn(Ptr, i32, u32, Ptr, u32) -> i32);
        let media = sym!(b"FMOD_EventSystem_SetMediaPath", unsafe extern "system" fn(Ptr, *const c_char) -> i32);
        let load = sym!(b"FMOD_EventSystem_Load", unsafe extern "system" fn(Ptr, *const c_char, Ptr, *mut Ptr) -> i32);
        let p_groups = sym!(b"FMOD_EventProject_GetNumGroups", unsafe extern "system" fn(Ptr, *mut i32) -> i32);
        let p_group = sym!(b"FMOD_EventProject_GetGroupByIndex", unsafe extern "system" fn(Ptr, i32, i32, *mut Ptr) -> i32);
        let g_groups = sym!(b"FMOD_EventGroup_GetNumGroups", unsafe extern "system" fn(Ptr, *mut i32) -> i32);
        let g_group = sym!(b"FMOD_EventGroup_GetGroupByIndex", unsafe extern "system" fn(Ptr, i32, i32, *mut Ptr) -> i32);
        let g_events = sym!(b"FMOD_EventGroup_GetNumEvents", unsafe extern "system" fn(Ptr, *mut i32) -> i32);
        let g_info = sym!(b"FMOD_EventGroup_GetInfo", unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char) -> i32);
        let g_event = sym!(b"FMOD_EventGroup_GetEventByIndex", unsafe extern "system" fn(Ptr, i32, u32, *mut Ptr) -> i32);
        let e_info = sym!(b"FMOD_Event_GetInfo", unsafe extern "system" fn(Ptr, *mut i32, *mut *const c_char, Ptr) -> i32);
        let set_output = *lib_ex.get::<unsafe extern "system" fn(Ptr, i32) -> i32>(b"FMOD_System_SetOutput").map_err(|e| e.to_string())?;

        let mut es: Ptr = std::ptr::null_mut();
        let r = create(&mut es);
        if r != 0 {
            return Err(format!("EventSystem_Create {r}"));
        }
        let mut sys: Ptr = std::ptr::null_mut();
        get_sys(es, &mut sys);
        set_output(sys, 2); // FMOD_OUTPUTTYPE_NOSOUND
        let r = init(es, 64, 0, std::ptr::null_mut(), 0);
        if r != 0 {
            return Err(format!("EventSystem_Init {r}"));
        }
        let dir = CString::new(format!("{}/", sound_dir.display())).unwrap();
        media(es, dir.as_ptr());
        let name = CString::new(fev).unwrap();
        let mut proj: Ptr = std::ptr::null_mut();
        let r = load(es, name.as_ptr(), std::ptr::null_mut(), &mut proj);
        if r != 0 {
            return Err(format!("EventSystem_Load {fev}: FMOD error {r}"));
        }
        let mut out = Vec::new();
        let cstr = |p: *const c_char| if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
        // Depth-first over groups.
        let mut stack: Vec<(Ptr, String)> = Vec::new();
        let mut n = 0;
        p_groups(proj, &mut n);
        for i in 0..n {
            let mut g: Ptr = std::ptr::null_mut();
            if p_group(proj, i, 0, &mut g) == 0 {
                stack.push((g, String::new()));
            }
        }
        while let Some((g, prefix)) = stack.pop() {
            let (mut idx, mut gname) = (0i32, std::ptr::null::<c_char>());
            g_info(g, &mut idx, &mut gname);
            let path = format!("{prefix}{}", cstr(gname));
            let mut ne = 0;
            g_events(g, &mut ne);
            for i in 0..ne {
                let mut ev: Ptr = std::ptr::null_mut();
                if g_event(g, i, 4, &mut ev) == 0 {
                    // FMOD_EVENT_INFOONLY
                    let (mut ei, mut en) = (0i32, std::ptr::null::<c_char>());
                    e_info(ev, &mut ei, &mut en, std::ptr::null_mut());
                    out.push(format!("{path}/{}", cstr(en)));
                }
            }
            let mut ng = 0;
            g_groups(g, &mut ng);
            for i in 0..ng {
                let mut sub: Ptr = std::ptr::null_mut();
                if g_group(g, i, 0, &mut sub) == 0 {
                    stack.push((sub, format!("{path}/")));
                }
            }
        }
        Ok(out)
    }
}
