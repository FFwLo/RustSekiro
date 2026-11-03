//! The game's virtual file system: `Data1..Data5` BHD/BDT archive pairs, addressed by path hash.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};

use crate::bhd::{self, BhdEntry};
use crate::dcx::{self, Oodle};
use crate::reader::{Result, bail};

pub const ARCHIVES: [&str; 5] = ["Data1", "Data2", "Data3", "Data4", "Data5"];

pub struct Located {
    pub archive: usize,
    pub entry: BhdEntry,
}

pub struct Vfs {
    game_dir: PathBuf,
    bdts: Vec<Mutex<File>>,
    entries: HashMap<u32, Located>,
    names: HashMap<u32, String>,
    oodle: Option<&'static Oodle>,
}

impl Vfs {
    /// Opens the archives. `keys` maps archive name (`Data1`) to its PEM public key;
    /// `dictionary` is the list of known virtual paths.
    pub fn open(
        game_dir: &Path,
        keys: &HashMap<String, String>,
        dictionary: &[String],
    ) -> Result<Self> {
        let mut bdts = Vec::new();
        let mut entries = HashMap::new();
        for (index, name) in ARCHIVES.iter().enumerate() {
            let key = keys
                .get(*name)
                .ok_or_else(|| crate::Error::Format(format!("no key for {name}")))?;
            let raw = std::fs::read(game_dir.join(format!("{name}.bhd")))?;
            let header = if raw.starts_with(b"BHD5") {
                raw
            } else {
                bhd::decrypt(&raw, key)?
            };
            for entry in bhd::parse(&header)? {
                entries.insert(
                    entry.hash,
                    Located {
                        archive: index,
                        entry,
                    },
                );
            }
            bdts.push(Mutex::new(File::open(
                game_dir.join(format!("{name}.bdt")),
            )?));
        }
        let names = dictionary
            .iter()
            .filter(|p| p.starts_with('/'))
            .map(|p| (bhd::path_hash(p), p.clone()))
            .collect();
        let oodle = Oodle::load(game_dir).ok();
        Ok(Self {
            game_dir: game_dir.to_path_buf(),
            bdts,
            entries,
            names,
            oodle,
        })
    }

    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    pub fn oodle(&self) -> Option<&'static Oodle> {
        self.oodle
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All entries with their dictionary path when known.
    pub fn entries(&self) -> impl Iterator<Item = (u32, Option<&str>, &Located)> {
        self.entries
            .iter()
            .map(|(h, l)| (*h, self.names.get(h).map(String::as_str), l))
    }

    /// Known virtual paths that exist in the archives.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries
            .keys()
            .filter_map(|h| self.names.get(h).map(String::as_str))
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(&bhd::path_hash(path))
    }

    /// Reads a file by virtual path (for example `/chr/c1010.chrbnd.dcx`), decrypted and
    /// DCX-decompressed.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let hash = bhd::path_hash(path);
        let Some(located) = self.entries.get(&hash) else {
            return bail(format!("{path} not found in archives"));
        };
        let raw = self.read_raw(located)?;
        dcx::decompress(&raw, self.oodle)
    }

    /// Reads and decrypts an entry, without DCX decompression.
    pub fn read_raw(&self, located: &Located) -> Result<Vec<u8>> {
        let e = &located.entry;
        let mut data = vec![0u8; e.padded_size as usize];
        {
            let mut file = self.bdts[located.archive].lock().unwrap();
            file.seek(SeekFrom::Start(e.offset))?;
            file.read_exact(&mut data)?;
        }
        if let Some(aes) = &e.aes {
            let cipher = Aes128::new(GenericArray::from_slice(&aes.key));
            for &(start, end) in &aes.ranges {
                let end = (end as usize).min(data.len());
                for block in data[start as usize..end].as_chunks_mut::<16>().0 {
                    cipher.decrypt_block(GenericArray::from_mut_slice(block));
                }
            }
        }
        if e.size > 0 && (e.size as usize) < data.len() {
            data.truncate(e.size as usize);
        }
        Ok(data)
    }
}

/// Parses `ArchiveKeys.cs` (UXM) and returns the Sekiro archive keys.
pub fn parse_uxm_keys(source: &str) -> HashMap<String, String> {
    let section = source
        .split("SekiroKeys")
        .nth(1)
        .and_then(|s| s.split("SekiroBonusKeys").next())
        .unwrap_or("");
    let mut keys = HashMap::new();
    let mut rest = section;
    while let Some(start) = rest.find("[\"") {
        let after = &rest[start + 2..];
        let Some(name_end) = after.find('"') else {
            break;
        };
        let name = &after[..name_end];
        let Some(pem_start) = after.find("-----BEGIN") else {
            break;
        };
        let Some(pem_len) = after[pem_start..].find("-----END RSA PUBLIC KEY-----") else {
            break;
        };
        let pem_end = pem_start + pem_len + "-----END RSA PUBLIC KEY-----".len();
        keys.insert(name.to_string(), after[pem_start..pem_end].to_string());
        rest = &after[pem_end..];
    }
    keys
}
