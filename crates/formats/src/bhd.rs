//! BHD5 archive headers (`DataN.bhd`), RSA-encrypted with a public key per archive.

use num_bigint::BigUint;

use crate::reader::{Reader, Result, bail};

#[derive(Debug, Clone)]
pub struct BhdEntry {
    pub hash: u32,
    pub padded_size: u64,
    pub offset: u64,
    pub size: u64,
    pub aes: Option<AesInfo>,
}

#[derive(Debug, Clone)]
pub struct AesInfo {
    pub key: [u8; 16],
    /// Byte ranges (start, end) within the entry that are encrypted.
    pub ranges: Vec<(u64, u64)>,
}

/// The FromSoftware path hash used by BHD5 entries in Sekiro (32-bit, multiplier 37).
pub fn path_hash(path: &str) -> u32 {
    path.chars()
        .map(|c| {
            if c == '\\' {
                '/'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .fold(0u32, |h, c| h.wrapping_mul(37).wrapping_add(c as u32))
}

/// Decrypts an RSA-encrypted header with the archive's public key (PKCS#1 PEM).
pub fn decrypt(encrypted: &[u8], pem: &str) -> Result<Vec<u8>> {
    let (modulus, exponent) = parse_pem(pem)?;
    let block = modulus.bits().div_ceil(8) as usize;
    if !encrypted.len().is_multiple_of(block) {
        return bail("encrypted header is not a whole number of RSA blocks");
    }
    let mut out = Vec::with_capacity(encrypted.len());
    for chunk in encrypted.chunks(block) {
        let value = BigUint::from_bytes_be(chunk);
        let plain = value.modpow(&exponent, &modulus).to_bytes_be();
        // Each block decrypts to block - 1 bytes, left-padded with zeros.
        let width = block - 1;
        if plain.len() > width {
            return bail("RSA block decrypted wider than expected");
        }
        out.resize(out.len() + width - plain.len(), 0);
        out.extend_from_slice(&plain);
    }
    Ok(out)
}

pub fn parse(data: &[u8]) -> Result<Vec<BhdEntry>> {
    let mut r = Reader::new(data);
    r.magic(b"BHD5")?;
    if r.u8()? != 0xFF {
        return bail("expected little-endian BHD5");
    }
    r.skip(3);
    if r.i32()? != 1 {
        return bail("unsupported BHD5 version");
    }
    let _file_size = r.u32()?;
    let bucket_count = r.u32()? as usize;
    let bucket_offset = r.offset32()?;
    let mut entries = Vec::new();
    for b in 0..bucket_count {
        let mut br = Reader::at(data, bucket_offset + b * 8);
        let count = br.u32()? as usize;
        let offset = br.offset32()?;
        for e in 0..count {
            let mut er = Reader::at(data, offset + e * 40);
            let hash = er.u32()?;
            let padded_size = er.u32()? as u64;
            let file_offset = er.u64()?;
            let _sha_offset = er.u64()?;
            let aes_offset = er.u64()? as usize;
            let size = er.u64()?;
            let aes = if aes_offset != 0 {
                let mut ar = Reader::at(data, aes_offset);
                let key: [u8; 16] = ar.bytes(16)?.try_into().unwrap();
                let range_count = ar.u32()? as usize;
                let mut ranges = Vec::with_capacity(range_count);
                for _ in 0..range_count {
                    let start = ar.i64()?;
                    let end = ar.i64()?;
                    if start >= 0 && end > start {
                        ranges.push((start as u64, end as u64));
                    }
                }
                Some(AesInfo { key, ranges })
            } else {
                None
            };
            entries.push(BhdEntry {
                hash,
                padded_size,
                offset: file_offset,
                size,
                aes,
            });
        }
    }
    Ok(entries)
}

fn parse_pem(pem: &str) -> Result<(BigUint, BigUint)> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----") && !l.is_empty())
        .collect();
    let der = base64_decode(&body)?;
    // PKCS#1 RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    let (tag, seq, _) = der_item(&der, 0)?;
    if tag != 0x30 {
        return bail("RSA key is not a DER sequence");
    }
    let (t1, modulus, next) = der_item(seq, 0)?;
    let (t2, exponent, _) = der_item(seq, next)?;
    if t1 != 2 || t2 != 2 {
        return bail("RSA key integers missing");
    }
    Ok((
        BigUint::from_bytes_be(modulus),
        BigUint::from_bytes_be(exponent),
    ))
}

fn der_item(data: &[u8], mut pos: usize) -> Result<(u8, &[u8], usize)> {
    let tag = *data
        .get(pos)
        .ok_or(crate::Error::Format("truncated DER".into()))?;
    let mut len = *data
        .get(pos + 1)
        .ok_or(crate::Error::Format("truncated DER".into()))? as usize;
    pos += 2;
    if len & 0x80 != 0 {
        let n = len & 0x7F;
        len = 0;
        for i in 0..n {
            len = (len << 8) | *data.get(pos + i).unwrap_or(&0) as usize;
        }
        pos += n;
    }
    let body = crate::reader::slice(data, pos, len)?;
    Ok((tag, body, pos + len))
}

fn base64_decode(text: &str) -> Result<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|&c| c != b'=') {
        let v = ALPHABET
            .iter()
            .position(|&a| a == c)
            .ok_or(crate::Error::Format("invalid base64".into()))? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_hash_is_case_and_slash_insensitive() {
        assert_eq!(
            path_hash("/chr/c1010.anibnd.dcx"),
            path_hash("\\CHR\\C1010.ANIBND.DCX")
        );
        assert_eq!(path_hash("a"), 'a' as u32);
        assert_eq!(path_hash("ab"), 'a' as u32 * 37 + 'b' as u32);
    }

    #[test]
    fn base64_round_trip() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
    }
}
