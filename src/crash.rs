//! A native crash (an access violation in FMOD, a driver, unsafe code) ends the process with no
//! panic and no log line. This unhandled-exception filter writes the exception code, the module
//! and offset it happened at, the thread, and a backtrace to stderr and sv1.log first, then lets
//! Windows end the process as before.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        next: *const ExceptionRecord,
        address: *const c_void,
        params: u32,
        info: [usize; 15],
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *const ExceptionRecord,
        context: *const c_void,
    }

    type Filter = unsafe extern "system" fn(*const ExceptionPointers) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(filter: Option<Filter>) -> Option<Filter>;
        fn GetModuleHandleExW(flags: u32, addr: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
        fn GetCurrentThreadId() -> u32;
    }

    /// GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | _UNCHANGED_REFCOUNT.
    const FROM_ADDRESS: u32 = 0x4 | 0x2;
    /// EXCEPTION_CONTINUE_SEARCH: Windows ends the process as it would have.
    const CONTINUE_SEARCH: i32 = 0;

    fn module_of(addr: *const c_void) -> String {
        let mut module = std::ptr::null_mut();
        if unsafe { GetModuleHandleExW(FROM_ADDRESS, addr, &mut module) } == 0 {
            return format!("{addr:?} (no module)");
        }
        let mut name = [0u16; 260];
        let n = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&name[..n.min(name.len())]);
        let file = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
        format!("{file}+0x{:x}", addr as usize - module as usize)
    }

    unsafe extern "system" fn filter(ptrs: *const ExceptionPointers) -> i32 {
        let rec = unsafe { ptrs.as_ref().and_then(|p| p.record.as_ref()) };
        let Some(rec) = rec else { return CONTINUE_SEARCH };
        // Thread id, not std::thread::current(): a crash at exit comes after the thread's
        // locals are gone, where that panics.
        let mut text = format!("CRASH 0x{:08X} at {} on thread {}", rec.code, module_of(rec.address), unsafe { GetCurrentThreadId() });
        if rec.code == 0xC000_0005 && rec.params >= 2 {
            let op = match rec.info[0] {
                0 => "reading",
                1 => "writing",
                _ => "executing",
            };
            text += &format!(" ({op} 0x{:x})", rec.info[1]);
        }
        text += &format!("\n{}", std::backtrace::Backtrace::force_capture());
        eprintln!("{text}");
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(crate::host_app::log_path()) {
            use std::io::Write;
            let _ = writeln!(f, "{text}");
        }
        CONTINUE_SEARCH
    }

    pub fn install() {
        unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    }
}

/// Installs the crash report (Windows only).
pub fn install() {
    #[cfg(windows)]
    imp::install();
}
