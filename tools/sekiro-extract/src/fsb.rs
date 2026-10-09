//! FMOD FSB5 sound banks (Sekiro: sound/*.fsb, codec 15 = Vorbis) -> Ogg Vorbis files.
//!
//! FSB5 Vorbis stores raw Vorbis audio packets (u16 size prefix each) and only a CRC32
//! of the stream's setup header. FMOD rebuilds the header from a table compiled into
//! its own DLL; we read that table from the game's fmodex64.dll ({u64 ptr,
//! u32 size} entries pointing at complete "\x05vorbis" setup packets), synthesize the
//! identification/comment headers, and remux the packets into Ogg without re-encoding.
//! lewton decodes each packet only to count samples for the Ogg granule positions.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

pub struct Sample {
    pub name: String,
    pub rate: u32,
    pub channels: u8,
    pub vorbis_crc: Option<u32>,
    pub data: Vec<u8>,
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

/// FSB5 frequency index (sample header bits 1-4).
fn rate_of(idx: u64) -> u32 {
    match idx {
        1 => 8000,
        2 => 11000,
        3 => 11025,
        4 => 16000,
        5 => 22050,
        6 => 24000,
        7 => 32000,
        8 => 44100,
        9 => 48000,
        _ => 44100,
    }
}

pub fn read_fsb5(d: &[u8]) -> Result<Vec<Sample>, String> {
    if &d[0..4] != b"FSB5" {
        return Err("not FSB5 (encrypted bank?)".into());
    }
    let version = u32le(d, 4);
    let n = u32le(d, 8) as usize;
    let sample_headers = u32le(d, 12) as usize;
    let names_size = u32le(d, 16) as usize;
    let data_size = u32le(d, 20) as usize;
    let codec = u32le(d, 24);
    if codec != 15 {
        return Err(format!("codec {codec} is not Vorbis"));
    }
    let header_len = if version == 0 { 0x40 } else { 0x3C };
    let mut o = header_len;
    let mut heads = Vec::with_capacity(n);
    for _ in 0..n {
        let raw = u64::from_le_bytes(d[o..o + 8].try_into().unwrap());
        o += 8;
        let mut next = raw & 1;
        let mut rate = rate_of((raw >> 1) & 0xF);
        let mut channels = (((raw >> 5) & 1) + 1) as u8;
        let data_off = (((raw >> 6) & 0x0FFF_FFFF) * 16) as usize;
        let mut crc = None;
        while next != 0 {
            let c = u32le(d, o);
            o += 4;
            next = (c & 1) as u64;
            let size = ((c >> 1) & 0xFF_FFFF) as usize;
            let kind = c >> 25;
            let body = &d[o..o + size];
            match kind {
                1 => channels = body[0],
                2 => rate = u32le(body, 0),
                11 => crc = Some(u32le(body, 0)),
                _ => {}
            }
            o += size;
        }
        heads.push((rate, channels, data_off, crc));
    }
    let names_at = header_len + sample_headers;
    let data_at = names_at + names_size;
    let mut out = Vec::with_capacity(n);
    for (i, &(rate, channels, off, crc)) in heads.iter().enumerate() {
        let name = if names_size > 0 {
            let p = names_at + u32le(d, names_at + i * 4) as usize;
            let end = d[p..].iter().position(|&b| b == 0).map_or(d.len(), |e| p + e);
            String::from_utf8_lossy(&d[p..end]).into_owned()
        } else {
            format!("{i:05}")
        };
        let end = heads.get(i + 1).map_or(data_size, |h| h.2);
        out.push(Sample { name, rate, channels, vorbis_crc: crc, data: d[data_at + off..data_at + end].to_vec() });
    }
    Ok(out)
}

/// Vorbis setup headers keyed by CRC32, from FMOD's DLL.
pub fn setup_headers(dll: &[u8], wanted: &[u32]) -> HashMap<u32, Vec<u8>> {
    let pe = u32le(dll, 0x3c) as usize;
    let nsec = u16::from_le_bytes([dll[pe + 6], dll[pe + 7]]) as usize;
    let opt = u16::from_le_bytes([dll[pe + 20], dll[pe + 21]]) as usize;
    let base = u64::from_le_bytes(dll[pe + 48..pe + 56].try_into().unwrap());
    let secs: Vec<(u64, u64, u64)> = (0..nsec)
        .map(|i| {
            let s = pe + 24 + opt + i * 40;
            (u32le(dll, s + 12) as u64, u32le(dll, s + 16) as u64, u32le(dll, s + 20) as u64)
        })
        .collect();
    let file_off = |va: u64| -> Option<usize> {
        let rva = va.checked_sub(base)?;
        secs.iter().find(|(v, size, _)| rva >= *v && rva < v + size).map(|(v, _, raw)| (rva - v + raw) as usize)
    };
    let mut out = HashMap::new();
    for &crc in wanted {
        let needle = crc.to_le_bytes();
        let mut from = 0;
        while let Some(p) = dll[from..].windows(4).position(|w| w == needle).map(|p| p + from) {
            from = p + 4;
            // Table entries hold {u64 ptr, u32 size} blocks around the crc: the header is
            // the block before the crc, the block after it, or both concatenated
            // (header + continuation). Keep the first candidate lewton parses.
            let block = |ptr_at: usize| -> Option<Vec<u8>> {
                let ptr = u64::from_le_bytes(dll.get(ptr_at..ptr_at + 8)?.try_into().ok()?);
                let size = u32le(dll, ptr_at + 8) as usize;
                let fo = file_off(ptr)?;
                dll.get(fo..fo + size).map(|b| b.to_vec())
            };
            let before = if p >= 12 { block(p - 12) } else { None };
            let after = block(p + 4);
            let mut candidates = Vec::new();
            for (a, b) in [(&before, &None), (&after, &None), (&after, &before), (&before, &after)] {
                if let Some(a) = a {
                    if a.starts_with(b"vorbis") {
                        let mut v = a.clone();
                        if let Some(b) = b {
                            v.extend_from_slice(b);
                        }
                        candidates.push(v);
                    }
                }
            }
            if let Some(v) = candidates.into_iter().find(|v| parses(v)) {
                out.insert(crc, v);
                break;
            }
        }
    }
    out
}

/// Does this setup header parse completely (mono or stereo)?
fn parses(setup: &[u8]) -> bool {
    [1u8, 2].iter().any(|&ch| lewton::header::read_header_setup(setup, ch, (8, 11)).is_ok())
}

fn ident_header(channels: u8, rate: u32) -> Vec<u8> {
    let mut v = b"\x01vorbis".to_vec();
    v.extend_from_slice(&0u32.to_le_bytes());
    v.push(channels);
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&[0; 12]); // bitrate max / nominal / min
    v.push(0xB8); // block sizes 256 / 2048 (FSB5 Vorbis)
    v.push(1);
    v
}

fn comment_header() -> Vec<u8> {
    let vendor = b"sekiro-extract";
    let mut v = b"\x03vorbis".to_vec();
    v.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    v.extend_from_slice(vendor);
    v.extend_from_slice(&0u32.to_le_bytes());
    v.push(1);
    v
}

/// One sample -> an Ogg Vorbis file.
pub fn to_ogg(s: &Sample, setup: &[u8]) -> Result<Vec<u8>, String> {
    use lewton::header::{read_header_comment, read_header_ident, read_header_setup};
    use ogg::writing::{PacketWriteEndInfo, PacketWriter};
    let ident = ident_header(s.channels, s.rate);
    let comment = comment_header();
    let id = read_header_ident(&ident).map_err(|e| format!("ident: {e:?}"))?;
    read_header_comment(&comment).map_err(|e| format!("comment: {e:?}"))?;
    let setup_h = read_header_setup(setup, s.channels, (id.blocksize_0, id.blocksize_1)).map_err(|e| format!("setup: {e:?}"))?;

    let mut packets = Vec::new();
    let mut o = 0;
    while o + 2 <= s.data.len() {
        let len = u16::from_le_bytes([s.data[o], s.data[o + 1]]) as usize;
        o += 2;
        if len == 0 || o + len > s.data.len() {
            break;
        }
        packets.push(&s.data[o..o + len]);
        o += len;
    }

    let mut buf = Cursor::new(Vec::new());
    {
        let mut w = PacketWriter::new(&mut buf);
        let serial = 0x5EC1_0000;
        w.write_packet(ident.clone().into_boxed_slice(), serial, PacketWriteEndInfo::EndPage, 0).map_err(|e| e.to_string())?;
        w.write_packet(comment.into_boxed_slice(), serial, PacketWriteEndInfo::NormalPacket, 0).map_err(|e| e.to_string())?;
        w.write_packet(setup.to_vec().into_boxed_slice(), serial, PacketWriteEndInfo::EndPage, 0).map_err(|e| e.to_string())?;
        let mut pwr = lewton::audio::PreviousWindowRight::new();
        let mut granule = 0u64;
        for (i, p) in packets.iter().enumerate() {
            let decoded: Vec<Vec<i16>> =
                lewton::audio::read_audio_packet_generic(&id, &setup_h, p, &mut pwr).map_err(|e| format!("packet {i}: {e:?}"))?;
            granule += decoded.first().map_or(0, |c| c.len()) as u64;
            let end = if i + 1 == packets.len() { PacketWriteEndInfo::EndStream } else { PacketWriteEndInfo::NormalPacket };
            w.write_packet(p.to_vec().into_boxed_slice(), serial, end, granule).map_err(|e| e.to_string())?;
        }
    }
    Ok(buf.into_inner())
}

/// sounds <extracted dir> <fmodex64.dll>: every extracted/sound/*.fsb -> extracted/sound_ogg/<name>.ogg
pub fn export_all(root: &Path, dll_path: &Path) {
    let dll = std::fs::read(dll_path).expect("fmodex64.dll");
    let out_dir = root.join("sound_ogg");
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(root.join("sound")).unwrap().flatten().map(|e| e.path()).collect();
    files.sort();
    for path in files.into_iter().filter(|p| p.extension().is_some_and(|e| e == "fsb")) {
        let d = std::fs::read(&path).unwrap();
        let samples = match read_fsb5(&d) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                continue;
            }
        };
        let crcs: Vec<u32> = samples.iter().filter_map(|s| s.vorbis_crc).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        let headers = setup_headers(&dll, &crcs);
        let (mut ok, mut failed) = (0, 0);
        for s in &samples {
            let Some(setup) = s.vorbis_crc.and_then(|c| headers.get(&c)) else {
                failed += 1;
                continue;
            };
            match to_ogg(s, setup) {
                Ok(bytes) => {
                    std::fs::write(out_dir.join(format!("{}.ogg", s.name)), bytes).unwrap();
                    ok += 1;
                }
                Err(e) => {
                    if failed < 12 {
                        eprintln!("{}: {}: {e}", path.display(), s.name);
                    }
                    failed += 1;
                }
            }
        }
        println!("{}: {ok} sounds, {failed} failed ({} setup headers found of {})", path.display(), headers.len(), crcs.len());
    }
}

/// Decodes every subsound of each extracted/sound/*.fsb with the game's own FMOD Ex
/// (fmodex64.dll, loaded at runtime) and writes extracted/sound_pcm/<name>.pcm:
///   "SPCM" u32 rate, u16 channels, u16 bits (16), then interleaved i16 samples.
/// This handles every Vorbis setup header (some are stored split in the DLL) and is
/// the reference decode. Encrypted banks (smain.fsb) are skipped.
pub fn decode_with_fmod(root: &Path, dll_path: &Path) -> Result<(), String> {
    use std::ffi::{c_char, c_void, CString};
    type Ptr = *mut c_void;
    unsafe {
        let lib = libloading::Library::new(dll_path).map_err(|e| e.to_string())?;
        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                *lib.get::<$ty>($name).map_err(|e| e.to_string())?
            };
        }
        let create = sym!(b"FMOD_System_Create", unsafe extern "system" fn(*mut Ptr) -> i32);
        let set_output = sym!(b"FMOD_System_SetOutput", unsafe extern "system" fn(Ptr, i32) -> i32);
        let init = sym!(b"FMOD_System_Init", unsafe extern "system" fn(Ptr, i32, u32, Ptr) -> i32);
        let create_sound = sym!(b"FMOD_System_CreateSound", unsafe extern "system" fn(Ptr, *const c_char, u32, Ptr, *mut Ptr) -> i32);
        let num_sub = sym!(b"FMOD_Sound_GetNumSubSounds", unsafe extern "system" fn(Ptr, *mut i32) -> i32);
        let get_sub = sym!(b"FMOD_Sound_GetSubSound", unsafe extern "system" fn(Ptr, i32, *mut Ptr) -> i32);
        let get_format = sym!(b"FMOD_Sound_GetFormat", unsafe extern "system" fn(Ptr, *mut i32, *mut i32, *mut i32, *mut i32) -> i32);
        let get_defaults = sym!(b"FMOD_Sound_GetDefaults", unsafe extern "system" fn(Ptr, *mut f32, *mut f32, *mut f32, *mut i32) -> i32);
        let get_length = sym!(b"FMOD_Sound_GetLength", unsafe extern "system" fn(Ptr, *mut u32, u32) -> i32);
        let get_name = sym!(b"FMOD_Sound_GetName", unsafe extern "system" fn(Ptr, *mut c_char, i32) -> i32);
        let seek = sym!(b"FMOD_Sound_SeekData", unsafe extern "system" fn(Ptr, u32) -> i32);
        let read = sym!(b"FMOD_Sound_ReadData", unsafe extern "system" fn(Ptr, Ptr, u32, *mut u32) -> i32);
        let release = sym!(b"FMOD_Sound_Release", unsafe extern "system" fn(Ptr) -> i32);

        let mut sys: Ptr = std::ptr::null_mut();
        let r = create(&mut sys);
        if r != 0 {
            return Err(format!("FMOD_System_Create {r}"));
        }
        set_output(sys, 2); // FMOD_OUTPUTTYPE_NOSOUND: no audio device needed
        let r = init(sys, 16, 0, std::ptr::null_mut());
        if r != 0 {
            return Err(format!("FMOD_System_Init {r}"));
        }
        const FMOD_SOFTWARE: u32 = 0x40;
        const FMOD_OPENONLY: u32 = 0x2000;
        const PCMBYTES: u32 = 4;
        let out_dir = root.join("sound_pcm");
        std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
        let mut files: Vec<_> = std::fs::read_dir(root.join("sound")).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files.into_iter().filter(|p| p.extension().is_some_and(|e| e == "fsb")) {
            if std::fs::read(&path).map(|d| !d.starts_with(b"FSB")).unwrap_or(true) {
                eprintln!("{}: encrypted, skipped", path.display());
                continue;
            }
            let cpath = CString::new(path.to_string_lossy().as_bytes()).unwrap();
            let mut snd: Ptr = std::ptr::null_mut();
            let r = create_sound(sys, cpath.as_ptr(), FMOD_SOFTWARE | FMOD_OPENONLY, std::ptr::null_mut(), &mut snd);
            if r != 0 {
                eprintln!("{}: FMOD_System_CreateSound {r}", path.display());
                continue;
            }
            let mut n = 0;
            num_sub(snd, &mut n);
            let (mut ok, mut failed) = (0, 0);
            // FMOD can crash the process on a few subsounds: progress is recorded so a
            // rerun resumes after the one that crashed (tools/extract.ps1 loops).
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            let progress = out_dir.join(format!(".progress_{stem}"));
            let start: i32 = std::fs::read_to_string(&progress).ok().and_then(|s| s.trim().parse().ok()).map_or(0, |i: i32| i + 1);
            if start >= n {
                release(snd);
                continue;
            }
            for i in start..n {
                std::fs::write(&progress, i.to_string()).map_err(|e| e.to_string())?;
                let mut sub: Ptr = std::ptr::null_mut();
                if get_sub(snd, i, &mut sub) != 0 || sub.is_null() {
                    failed += 1;
                    continue;
                }
                let mut name = [0 as c_char; 256];
                get_name(sub, name.as_mut_ptr(), 255);
                let name = std::ffi::CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned();
                let (mut ty, mut fmt, mut ch, mut bits) = (0, 0, 0, 0);
                get_format(sub, &mut ty, &mut fmt, &mut ch, &mut bits);
                let (mut freq, mut vol, mut pan, mut prio) = (0.0f32, 0.0f32, 0.0f32, 0);
                get_defaults(sub, &mut freq, &mut vol, &mut pan, &mut prio);
                let mut len = 0u32;
                get_length(sub, &mut len, PCMBYTES);
                let mut buf = vec![0u8; len as usize];
                seek(sub, 0);
                let mut got = 0u32;
                let r = read(sub, buf.as_mut_ptr() as Ptr, len, &mut got);
                // FMOD_ERR_FILE_EOF (r == 0x13 in Ex 4.44) after a full read is fine.
                if got == 0 || (r != 0 && got < len / 2) {
                    if failed < 5 {
                        eprintln!("{}: {name}: read {r} ({got}/{len} bytes, fmt {fmt}, bits {bits})", path.display());
                    }
                    failed += 1;
                    // A failed read can leave the shared stream unusable: reopen the bank.
                    release(snd);
                    snd = std::ptr::null_mut();
                    if create_sound(sys, cpath.as_ptr(), FMOD_SOFTWARE | FMOD_OPENONLY, std::ptr::null_mut(), &mut snd) != 0 {
                        break;
                    }
                    continue;
                }
                buf.truncate(got as usize);
                // PCMFLOAT (5) -> i16.
                let pcm: Vec<u8> = if fmt == 5 {
                    buf.chunks_exact(4)
                        .flat_map(|c| (((f32::from_le_bytes(c.try_into().unwrap())).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
                        .collect()
                } else {
                    buf
                };
                let mut out = b"SPCM".to_vec();
                out.extend_from_slice(&(freq as u32).to_le_bytes());
                out.extend_from_slice(&(ch as u16).to_le_bytes());
                out.extend_from_slice(&16u16.to_le_bytes());
                out.extend_from_slice(&pcm);
                std::fs::write(out_dir.join(format!("{name}.pcm")), out).map_err(|e| e.to_string())?;
                ok += 1;
            }
            release(snd);
            println!("{}: {ok} sounds decoded by FMOD, {failed} failed", path.display());
        }
    }
    Ok(())
}
