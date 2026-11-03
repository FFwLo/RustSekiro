//! DCX compression containers. Sekiro uses DFLT (zlib) and KRAK (Oodle Kraken).
//!
//! Oodle is not reimplemented: KRAK payloads are decoded with `oo2core_6_win64.dll` loaded from the
//! player's own game folder.

use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

use crate::reader::{Reader, Result, bail};

pub fn is_dcx(data: &[u8]) -> bool {
    data.starts_with(b"DCX\0")
}

/// Returns the payload unchanged when it is not DCX-wrapped.
pub fn decompress(data: &[u8], oodle: Option<&Oodle>) -> Result<Vec<u8>> {
    if !is_dcx(data) {
        return Ok(data.to_vec());
    }
    let mut r = Reader::at(data, 24);
    r.magic(b"DCS\0")?;
    let size = r.u32_be()? as usize;
    let compressed_size = r.u32_be()? as usize;
    r.magic(b"DCP\0")?;
    let codec = r.bytes(4)?;
    let payload = crate::reader::slice(data, 76, compressed_size)?;
    let out = match codec {
        b"DFLT" => {
            let mut out = Vec::with_capacity(size);
            flate2::read::ZlibDecoder::new(payload).read_to_end(&mut out)?;
            out
        }
        b"KRAK" => match oodle {
            Some(o) => o.decompress(payload, size)?,
            None => return bail("KRAK DCX needs the game's Oodle DLL"),
        },
        other => {
            return bail(format!(
                "unsupported DCX codec {:?}",
                String::from_utf8_lossy(other)
            ));
        }
    };
    if out.len() != size {
        return bail(format!("DCX size mismatch: {} != {size}", out.len()));
    }
    Ok(out)
}

/// Raw zlib payload (used by some binder entries without a DCX wrapper).
pub fn inflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data).read_to_end(&mut out)?;
    Ok(out)
}

type DecompressFn = unsafe extern "C" fn(
    *const u8,
    i64,
    *mut u8,
    i64,
    i32,
    i32,
    i32,
    *mut u8,
    i64,
    *mut u8,
    *mut u8,
    *mut u8,
    i64,
    i32,
) -> i64;

pub struct Oodle {
    _lib: libloading::Library,
    decompress: DecompressFn,
}

impl Oodle {
    pub fn load(game_dir: &Path) -> Result<&'static Oodle> {
        static INSTANCE: OnceLock<std::result::Result<Oodle, String>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| {
                let path = game_dir.join("oo2core_6_win64.dll");
                // SAFETY: loading the codec DLL shipped with the player's game install.
                unsafe {
                    let lib = libloading::Library::new(&path).map_err(|e| e.to_string())?;
                    let f: libloading::Symbol<DecompressFn> =
                        lib.get(b"OodleLZ_Decompress").map_err(|e| e.to_string())?;
                    let decompress = *f;
                    Ok(Oodle {
                        _lib: lib,
                        decompress,
                    })
                }
            })
            .as_ref()
            .map_err(|e| crate::Error::Format(format!("loading Oodle: {e}")))
    }

    pub fn decompress(&self, src: &[u8], size: usize) -> Result<Vec<u8>> {
        // Oodle may write past the requested size, so give it slack.
        let mut out = vec![0u8; size + 64];
        // SAFETY: buffers are sized per Oodle's contract; pointers outlive the call.
        let written = unsafe {
            (self.decompress)(
                src.as_ptr(),
                src.len() as i64,
                out.as_mut_ptr(),
                size as i64,
                1,
                0,
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                3,
            )
        };
        if written != size as i64 {
            return bail(format!("Oodle returned {written}, expected {size}"));
        }
        out.truncate(size);
        Ok(out)
    }
}
