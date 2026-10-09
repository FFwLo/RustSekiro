//! BHD5 archive headers (Sekiro uses the DS3 layout) and BDT data reading.
//!
//! The .bhd is RSA-encrypted with a public key per archive (keys/DataN.pem,
//! the same public keys UXM ships). Each file entry can carry an AES-128-ECB
//! key plus a list of byte ranges that are encrypted.

use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use base64::Engine;
use num_bigint::BigUint;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::bin::Reader;

pub struct Entry {
    pub hash: u32,
    pub padded_size: u32,
    pub unpadded_size: u64,
    pub offset: u64,
    pub aes: Option<([u8; 16], Vec<(i64, i64)>)>,
}

/// FromSoftware path hash (pre-Elden Ring): lowercase, '/' separators,
/// leading '/', h = h * 37 + c.
pub fn path_hash(path: &str) -> u32 {
    let mut p = path.trim().replace('\\', "/").to_lowercase();
    if !p.starts_with('/') {
        p.insert(0, '/');
    }
    p.bytes().fold(0u32, |h, c| h.wrapping_mul(37).wrapping_add(c as u32))
}

fn parse_public_key(pem: &str) -> (BigUint, BigUint) {
    let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let der = base64::engine::general_purpose::STANDARD.decode(b64.trim()).expect("bad key base64");
    // PKCS#1 RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    let mut pos = 0;
    let mut read_tlv = |expect: u8| -> (usize, usize) {
        assert_eq!(der[pos], expect, "unexpected DER tag");
        pos += 1;
        let mut len = der[pos] as usize;
        pos += 1;
        if len & 0x80 != 0 {
            let n = len & 0x7f;
            len = der[pos..pos + n].iter().fold(0, |a, &b| (a << 8) | b as usize);
            pos += n;
        }
        let start = pos;
        if expect != 0x30 {
            pos += len;
        }
        (start, len)
    };
    read_tlv(0x30);
    let (ns, nl) = read_tlv(0x02);
    let (es, el) = read_tlv(0x02);
    (BigUint::from_bytes_be(&der[ns..ns + nl]), BigUint::from_bytes_be(&der[es..es + el]))
}

/// Raw RSA "decrypt" with the public key, block by block (BouncyCastle RsaEngine semantics:
/// 256-byte input blocks, 255-byte output blocks, left-padded with zeros).
fn rsa_decrypt(data: &[u8], pem: &str) -> Vec<u8> {
    let (n, e) = parse_public_key(pem);
    let in_size = (n.bits() as usize + 7) / 8;
    let out_size = (n.bits() as usize - 1) / 8;
    let mut out = Vec::with_capacity(data.len());
    for block in data.chunks(in_size) {
        let m = BigUint::from_bytes_be(block).modpow(&e, &n).to_bytes_be();
        out.extend(std::iter::repeat_n(0u8, out_size.saturating_sub(m.len())));
        out.extend_from_slice(&m[m.len().saturating_sub(out_size)..]);
    }
    out
}

pub fn read_bhd(bhd_path: &Path, pem: &str) -> Vec<Entry> {
    let raw = std::fs::read(bhd_path).expect("read bhd");
    let data = if &raw[0..4] == b"BHD5" { raw } else { rsa_decrypt(&raw, pem) };
    assert_eq!(&data[0..4], b"BHD5", "{} did not decrypt to BHD5", bhd_path.display());
    let r = Reader::new(&data);
    let bucket_count = r.u32(0x10) as usize;
    let buckets_offset = r.u32(0x14) as usize;
    let mut entries = Vec::new();
    for b in 0..bucket_count {
        let base = buckets_offset + b * 8;
        let count = r.u32(base) as usize;
        let headers = r.u32(base + 4) as usize;
        for i in 0..count {
            let h = headers + i * 0x28;
            let sha_off = r.u64(h + 0x10) as usize;
            let _ = sha_off;
            let aes_off = r.u64(h + 0x18) as usize;
            let aes = (aes_off != 0).then(|| {
                let mut key = [0u8; 16];
                key.copy_from_slice(&data[aes_off..aes_off + 16]);
                let n = r.u32(aes_off + 16) as usize;
                let ranges = (0..n)
                    .map(|k| {
                        let o = aes_off + 20 + k * 16;
                        (r.u64(o) as i64, r.u64(o + 8) as i64)
                    })
                    .collect();
                (key, ranges)
            });
            entries.push(Entry {
                hash: r.u32(h),
                padded_size: r.u32(h + 4),
                offset: r.u64(h + 8),
                unpadded_size: r.u64(h + 0x20),
                aes,
            });
        }
    }
    entries
}

pub fn read_entry(bdt: &mut File, e: &Entry) -> Vec<u8> {
    let mut buf = vec![0u8; e.padded_size as usize];
    bdt.seek(SeekFrom::Start(e.offset)).unwrap();
    bdt.read_exact(&mut buf).unwrap();
    if let Some((key, ranges)) = &e.aes {
        let cipher = Aes128::new(GenericArray::from_slice(key));
        for &(start, end) in ranges {
            if start == -1 || end == -1 || start == end {
                continue;
            }
            for block in buf[start as usize..end as usize].chunks_exact_mut(16) {
                cipher.decrypt_block(GenericArray::from_mut_slice(block));
            }
        }
    }
    if e.unpadded_size > 0 && (e.unpadded_size as usize) < buf.len() {
        buf.truncate(e.unpadded_size as usize);
    }
    buf
}
