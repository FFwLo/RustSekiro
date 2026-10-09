//! sekiro-dearxan <sekiro.exe> <out.exe>
//!
//! Produces an analysis copy of sekiro.exe for Ghidra:
//!  1. SteamStub 3.1 (x64) is unwrapped: .text is AES-256-CBC decrypted and the
//!     original entry point restored.
//!  2. Arxan is neutered statically with tremwil/dearxan: every stub is analysed,
//!     encrypted code regions are written back decrypted, and each stub entry is
//!     hooked to dearxan's fix-up code (placed in a new ".dearxan" section).
//!     This repeats until no new stubs appear (decryption can reveal more).
//!  3. The result is written in memory layout (raw offset = RVA), which Ghidra
//!     loads like a normal PE.
//!
//! For local reverse engineering only; the output is game-derived.

use aes::cipher::{BlockDecrypt, BlockDecryptMut, KeyInit, KeyIvInit, generic_array::GenericArray};
use dearxan::analysis::analyze_all_stubs;
use dearxan::patch::ArxanPatch;
use pelite::pe64::{Pe, PeFile, PeView};
use std::collections::HashSet;

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(d[o..o + 2].try_into().unwrap())
}
fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn u64_at(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}
fn put_u32(d: &mut [u8], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

struct Section {
    header: usize,
    va: u32,
    vsize: u32,
    raw: u32,
    rawsize: u32,
}

fn sections(d: &[u8]) -> (usize, Vec<Section>) {
    let pe = u32_at(d, 0x3C) as usize;
    let n = u16_at(d, pe + 6) as usize;
    let opt = u16_at(d, pe + 20) as usize;
    let first = pe + 24 + opt;
    let secs = (0..n)
        .map(|i| {
            let h = first + i * 40;
            Section { header: h, va: u32_at(d, h + 12), vsize: u32_at(d, h + 8), raw: u32_at(d, h + 20), rawsize: u32_at(d, h + 16) }
        })
        .collect();
    (pe, secs)
}

fn rva_to_off(secs: &[Section], rva: u32) -> usize {
    let s = secs.iter().find(|s| rva >= s.va && rva < s.va + s.vsize.max(s.rawsize)).expect("rva outside sections");
    (rva - s.va + s.raw) as usize
}

/// SteamStub's rolling xor: each dword is xored with the previous *encrypted* dword.
fn steam_xor(data: &mut [u8], mut key: u32) -> u32 {
    let mut start = 0;
    if key == 0 {
        key = u32_at(data, 0);
        start = 4;
    }
    for o in (start..data.len() - 3).step_by(4) {
        let v = u32_at(data, o);
        put_u32(data, o, v ^ key);
        key = v;
    }
    key
}

/// Unwraps SteamStub 3.1 x64 in place. Layout of the 0xF0-byte header follows
/// the public format documentation (Steamless, Variant31.x64 SteamStubHeader).
fn unwrap_steamstub(file: &mut [u8]) {
    let (pe, secs) = sections(file);
    let ep = u32_at(file, pe + 24 + 16);
    let hdr_off = rva_to_off(&secs, ep) - 0xF0;
    let mut h = file[hdr_off..hdr_off + 0xF0].to_vec();
    steam_xor(&mut h, 0);
    assert_eq!(u32_at(&h, 4), 0xC0DE_C0DF, "not a SteamStub 3.1 header");
    let oep = u64_at(&h, 0x20);
    let app_id = u32_at(&h, 0x38);
    let flags = u32_at(&h, 0x3C);
    let code_va = u64_at(&h, 0x48) as u32;
    let key: [u8; 32] = h[0x58..0x78].try_into().unwrap();
    let iv_enc: [u8; 16] = h[0x78..0x88].try_into().unwrap();
    let stolen: [u8; 16] = h[0x88..0x98].try_into().unwrap();
    println!("SteamStub 3.1: app {app_id}, flags {flags:#x}, OEP rva {oep:#x}, code {code_va:#x}");
    if flags & 0x4 == 0 {
        // IV is itself AES-256-ECB encrypted with the same key.
        let ecb = aes::Aes256::new(GenericArray::from_slice(&key));
        let mut iv = GenericArray::clone_from_slice(&iv_enc);
        ecb.decrypt_block(&mut iv);
        let s = secs.iter().find(|s| s.va == code_va).expect("code section");
        let raw = s.raw as usize;
        let len = s.rawsize as usize;
        // Ciphertext = 16 stolen bytes + the section's raw data.
        let mut buf = Vec::with_capacity(len + 16);
        buf.extend_from_slice(&stolen);
        buf.extend_from_slice(&file[raw..raw + len]);
        let usable = buf.len() / 16 * 16;
        let mut dec = cbc::Decryptor::<aes::Aes256>::new(GenericArray::from_slice(&key), &iv);
        for block in buf[..usable].chunks_exact_mut(16) {
            dec.decrypt_block_mut(GenericArray::from_mut_slice(block));
        }
        file[raw..raw + len].copy_from_slice(&buf[..len]);
        println!("decrypted {} bytes of code", len);
    }
    put_u32(file, pe + 24 + 16, oep as u32);
}

/// Copies the file into its in-memory layout (sections at their RVAs).
fn map_image(file: &[u8]) -> Vec<u8> {
    let (pe, secs) = sections(file);
    let size_of_image = u32_at(file, pe + 24 + 56) as usize;
    let size_of_headers = u32_at(file, pe + 24 + 60) as usize;
    let mut img = vec![0u8; size_of_image];
    img[..size_of_headers].copy_from_slice(&file[..size_of_headers]);
    for s in &secs {
        let n = s.rawsize.min(s.vsize) as usize;
        img[s.va as usize..s.va as usize + n].copy_from_slice(&file[s.raw as usize..s.raw as usize + n]);
    }
    img
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().collect();
    // sekiro-dearxan disasm <mapped image (output of this tool)> <hex va> [count]
    if args.len() >= 4 && args[1] == "disasm" {
        let img = std::fs::read(&args[2]).expect("read image");
        let va = u64::from_str_radix(args[3].trim_start_matches("0x"), 16).unwrap();
        let count: usize = args.get(4).and_then(|c| c.parse().ok()).unwrap_or(40);
        disasm(&img, 0x1_4000_0000, va, count);
        return;
    }
    if args.len() != 3 {
        eprintln!("usage: sekiro-dearxan <sekiro.exe> <out.exe>");
        std::process::exit(2);
    }
    let mut file = std::fs::read(&args[1]).expect("read exe");
    let has_bind = sections(&file).1.iter().any(|s| &file[s.header..s.header + 5] == b".bind");
    if has_bind {
        unwrap_steamstub(&mut file);
    } else {
        println!("no .bind section: SteamStub already removed (e.g. by Steamless)");
    }
    // Sanity check that the code now looks like code.
    {
        let (_, secs) = sections(&file);
        let t = &secs[0];
        let code = &file[t.raw as usize..(t.raw + t.rawsize) as usize];
        let prologues = code.windows(5).filter(|w| *w == [0x48, 0x89, 0x5C, 0x24, 0x08]).count();
        println!("code check: {prologues} `mov [rsp+8], rbx` prologues");
        assert!(prologues > 1000, "SteamStub decryption produced garbage");
    }

    let mut img = map_image(&file);
    let base = PeFile::from_bytes(&file).unwrap().optional_header().ImageBase;
    let mut hooks: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut seen = HashSet::new();
    for pass in 1..=8 {
        let view = PeView::from_bytes(&img).expect("mapped view");
        let results = analyze_all_stubs(view);
        let (ok, errs): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);
        let stubs: Vec<_> = ok.into_iter().map(Result::unwrap).filter(|s| seen.insert(s.test_rsp_va)).collect();
        println!("pass {pass}: {} new stubs, {} analysis errors", stubs.len(), errs.len());
        for e in errs.iter().take(5) {
            println!("  error: {}", e.as_ref().unwrap_err());
        }
        if stubs.is_empty() {
            break;
        }
        let patches = ArxanPatch::build_from_stubs(view, Some(base), stubs.iter()).expect("patch generation");
        let mut writes = 0usize;
        for p in patches {
            match p {
                ArxanPatch::Write { va, bytes } => {
                    let rva = (va - base) as usize;
                    img[rva..rva + bytes.len()].copy_from_slice(&bytes);
                    writes += bytes.len();
                }
                ArxanPatch::JmpHook { target, pic } => hooks.push((target, pic)),
            }
        }
        println!("  wrote {writes} decrypted bytes, {} hooks so far", hooks.len());
    }

    write_output(&args[2], img, &hooks, base);
}

/// Appends a ".dearxan" section holding the hook bodies, writes `jmp rel32` at
/// each stub, and lays the file out with raw offset = RVA for Ghidra.
fn write_output(path: &str, mut img: Vec<u8>, hooks: &[(u64, Vec<u8>)], base: u64) {
    let (pe, secs) = sections(&img);
    let align = 0x1000usize;
    let new_va = img.len().div_ceil(align) * align;
    let mut body = Vec::new();
    let mut targets = Vec::new();
    for (target, pic) in hooks {
        targets.push((*target, new_va + body.len()));
        body.extend_from_slice(pic);
        while body.len() % 16 != 0 {
            body.push(0xCC);
        }
    }
    let new_size = body.len().div_ceil(align).max(1) * align;
    img.resize(new_va + new_size, 0);
    img[new_va..new_va + body.len()].copy_from_slice(&body);
    for (target, at) in targets {
        let rva = (target - base) as usize;
        let rel = at as i64 - (rva as i64 + 5);
        img[rva] = 0xE9;
        img[rva + 1..rva + 5].copy_from_slice(&(rel as i32).to_le_bytes());
    }

    // Section headers: raw = VA so the file is its own memory image.
    for s in &secs {
        put_u32(&mut img, s.header + 20, s.va);
        put_u32(&mut img, s.header + 16, s.vsize.div_ceil(align as u32) * align as u32);
    }
    let last = secs.last().unwrap().header + 40;
    let size_of_headers = u32_at(&img, pe + 24 + 60) as usize;
    assert!(last + 40 <= size_of_headers, "no room for a new section header");
    let mut h = [0u8; 40];
    h[..8].copy_from_slice(b".dearxan");
    h[8..12].copy_from_slice(&(new_size as u32).to_le_bytes());
    h[12..16].copy_from_slice(&(new_va as u32).to_le_bytes());
    h[16..20].copy_from_slice(&(new_size as u32).to_le_bytes());
    h[20..24].copy_from_slice(&(new_va as u32).to_le_bytes());
    h[36..40].copy_from_slice(&0x6000_0020u32.to_le_bytes()); // code | execute | read
    img[last..last + 40].copy_from_slice(&h);
    let n = u16_at(&img, pe + 6) + 1;
    img[pe + 6..pe + 8].copy_from_slice(&n.to_le_bytes());
    put_u32(&mut img, pe + 24 + 36, align as u32); // FileAlignment = SectionAlignment
    let total = img.len() as u32;
    put_u32(&mut img, pe + 24 + 56, total); // SizeOfImage
    std::fs::write(path, &img).expect("write output");
    println!("wrote {path} ({} MB, {} stub hooks)", img.len() / 1_000_000, hooks.len());
}

fn disasm(img: &[u8], base: u64, va: u64, count: usize) {
    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
    let rva = (va - base) as usize;
    let mut dec = Decoder::with_ip(64, &img[rva..], va, DecoderOptions::NONE);
    let mut fmt = IntelFormatter::new();
    let mut out = String::new();
    for _ in 0..count {
        if !dec.can_decode() {
            break;
        }
        let ins = dec.decode();
        out.clear();
        fmt.format(&ins, &mut out);
        let start = (ins.ip() - base) as usize;
        let bytes: String = img[start..start + ins.len()].iter().map(|b| format!("{b:02x}")).collect();
        println!("{:x}  {:<24} {out}", ins.ip(), bytes);
    }
}
