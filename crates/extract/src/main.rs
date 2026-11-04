//! `sekiro-extract`: reads the player's Sekiro install and writes derived data to `cache/`.
//! The install is only ever read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rayon::prelude::*;
use sekiro_formats::{bnd4, dvd::Vfs};

mod anims;
mod maps;
mod menu;
mod models;
mod params;
mod sound;

const DEFAULT_GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro";

#[derive(Parser)]
#[command(about = "Extract data from your own Sekiro install into cache/")]
struct Cli {
    /// Sekiro install folder (read only). Also settable with the SEKIRO_DIR environment variable.
    #[arg(long, env = "SEKIRO_DIR", default_value = DEFAULT_GAME)]
    game: PathBuf,
    /// Output cache folder.
    #[arg(long, default_value = "cache")]
    cache: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Extract the five wire HUD sprites to cache/menu/wire.
    Menu,
    /// Unpack every archive entry to cache/raw, decompressed, with binders expanded alongside.
    Unpack {
        /// Only unpack paths containing this substring.
        #[arg(long)]
        filter: Option<String>,
    },
    /// List known archive paths.
    List {
        #[arg(long)]
        filter: Option<String>,
    },
    /// Convert character models in cache/raw to skinned, textured GLB files in cache/models.
    Models {
        /// Model ids, optionally with an NpcParam row (c7100:71001000). Defaults to the placed roster.
        ids: Vec<String>,
        /// Print which FLVER mesh each glTF primitive came from.
        #[arg(long)]
        verbose: bool,
    },
    /// Print a summary of a FLVER or TPF file.
    ModelInfo {
        path: PathBuf,
        /// For a TPF: also decode every texture to PNG in this folder.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// Decode cache/raw PARAM binders with Paramdex defs into cache/params/<Table>.json.
    Params,
    /// Decode Havok skeletons, animation clips and TAE events into cache/anim/<chr>/.
    Anims {
        /// Character ids (e.g. c0000 c1010). Defaults to Wolf and the placed roster's animation donors.
        chrs: Vec<String>,
    },
    /// Export one map area (e.g. m11_00_00_00) to cache/maps/<id>: map piece GLBs, layout JSON
    /// and the hit collision mesh. Needs the game folder for Oodle (read only).
    Map {
        id: String,
        /// Only print a summary of the MSB.
        #[arg(long)]
        summary: bool,
        /// Refresh complete placement metadata using existing geometry and material audit.
        #[arg(long, conflicts_with = "summary")]
        layout_only: bool,
        /// Refresh only these placed models and their textures, preserving layout and ancillary data.
        #[arg(long, num_args = 1.., conflicts_with_all = ["summary", "layout_only"])]
        models: Vec<String>,
        /// Rewrite only the placed objects' collision (`object_collision.bin`) from cache/raw.
        #[arg(long, conflicts_with_all = ["summary", "layout_only", "models"])]
        object_collision: bool,
        /// Rewrite only the fall-prevention assist walls (`fall_assist.bin`) from cache/raw.
        /// Needs the game folder for Oodle (read only).
        #[arg(long, conflicts_with_all = ["summary", "layout_only", "models", "object_collision"])]
        fall_assist: bool,
        /// Rewrite only the low-resolution hit collision characters and the camera collide
        /// with (`collision_lo.bin`) from cache/raw. Needs the game folder for Oodle (read only).
        #[arg(
            long,
            conflicts_with_all = ["summary", "layout_only", "models", "object_collision", "fall_assist"]
        )]
        lo_collision: bool,
    },
    /// Decode FSB5 sound banks in cache/raw/sound to WAV in cache/sound/<bank>/ with an
    /// events.json per bank. Needs the Vorbis headers from tools/sound/vorbis_setups.py.
    Sound {
        /// Banks to decode (e.g. c1010 main). Defaults to main, c1010 and m11.
        banks: Vec<String>,
        /// Decode every unencrypted bank (about 2.3 GB of WAV).
        #[arg(long)]
        all: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    fill_refs(&cli.cache)?;
    match &cli.command {
        Command::Models { ids, verbose } => return models::export(&cli.cache, ids, *verbose),
        Command::ModelInfo { path, dump } => return models::info(path, dump.as_deref()),
        Command::Params => return params::export(&cli.cache),
        Command::Anims { chrs } => return anims::export(&cli.cache, chrs),
        Command::Sound { banks, all } => return sound::export(&cli.cache, banks, *all),
        Command::Map {
            id,
            summary,
            layout_only,
            models,
            object_collision,
            fall_assist,
            lo_collision,
        } => {
            return if *summary {
                maps::summary(&cli.cache, id)
            } else if *lo_collision {
                maps::export_lo_collision(&cli.game, &cli.cache, id)
            } else if *fall_assist {
                maps::export_fall_assist(&cli.game, &cli.cache, id)
            } else if *object_collision {
                maps::export_object_collision(&cli.cache, id)
            } else if *layout_only {
                maps::refresh_layout(&cli.cache, id)
            } else {
                maps::export(&cli.game, &cli.cache, id, models)
            };
        }
        _ => {}
    }
    let vfs = open_vfs(&cli.game, &cli.cache)?;
    match cli.command {
        Command::Menu => menu::export(&vfs, &cli.cache)?,
        Command::List { filter } => {
            let mut paths: Vec<_> = vfs
                .paths()
                .filter(|p| filter.as_deref().is_none_or(|f| p.contains(f)))
                .collect();
            paths.sort_unstable();
            for p in &paths {
                println!("{p}");
            }
            eprintln!("{} of {} entries named", paths.len(), vfs.len());
        }
        Command::Unpack { filter } => unpack(&vfs, &cli.cache.join("raw"), filter.as_deref())?,
        Command::Models { .. }
        | Command::ModelInfo { .. }
        | Command::Params
        | Command::Anims { .. }
        | Command::Map { .. }
        | Command::Sound { .. } => unreachable!(),
    }
    Ok(())
}

/// Character ids placed by extracted maps; animation defaults include each row's donor.
pub(crate) fn roster_ids(cache: &Path, donors: bool) -> Result<Vec<String>> {
    let params: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cache.join("params/NpcParam.json"))?)?;
    let rows: HashMap<i64, &serde_json::Value> = params["rows"]
        .as_array()
        .context("NpcParam rows")?
        .iter()
        .filter_map(|r| Some((r["id"].as_i64()?, r)))
        .collect();
    let mut ids = std::collections::BTreeSet::from(["c0000".to_string()]);
    for dir in std::fs::read_dir(cache.join("maps"))? {
        let path = dir?.path().join("layout.json");
        if !path.exists() {
            continue;
        }
        let layout: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        for p in layout["enemies"].as_array().into_iter().flatten() {
            let Some(chr) = p["model"].as_str().filter(|c| c.starts_with('c')) else {
                continue;
            };
            let donor = p["npc_param"]
                .as_i64()
                .and_then(|id| rows.get(&id))
                .and_then(|r| r["normalChangeAnimChrId"].as_i64())
                .filter(|id| *id > 0);
            ids.insert(if donors {
                donor.map_or_else(|| chr.into(), |id| format!("c{id:04}"))
            } else {
                chr.into()
            });
        }
    }
    Ok(ids.into_iter().collect())
}

/// RSekiro's own extractor folder: its archive keys (`keys/`), file names and Paramdex defs
/// (`defs/`), so both extractors share one set and nothing has to be downloaded.
fn rsekiro_tool() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/sekiro-extract")
}

/// The params and anims exports and the game (sim's map effects) read the Paramdex defs and the
/// TAE template from `cache/refs`: copy RSekiro's there when nothing was fetched.
fn fill_refs(cache: &Path) -> Result<()> {
    let defs = rsekiro_tool().join("defs");
    for (from, to) in [("paramdef", "paramdex/Defs"), ("names", "paramdex/Names")] {
        let to = cache.join("refs").join(to);
        if to.exists() {
            continue;
        }
        std::fs::create_dir_all(&to)?;
        for entry in std::fs::read_dir(defs.join(from))? {
            let entry = entry?;
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    let tae = cache.join("refs/TAE.Template.SDT.xml");
    if !tae.exists() {
        std::fs::copy(defs.join("TAE.Template.SDT.xml"), tae)?;
    }
    Ok(())
}

fn open_vfs(game: &Path, cache: &Path) -> Result<Vfs> {
    let refs = cache.join("refs");
    let keys_dir = rsekiro_tool().join("keys");
    let keys: HashMap<_, _> = match std::fs::read_to_string(refs.join("ArchiveKeys.cs")) {
        Ok(src) => sekiro_formats::dvd::parse_uxm_keys(&src),
        Err(_) => sekiro_formats::dvd::ARCHIVES
            .iter()
            .map(|name| Ok((name.to_string(), std::fs::read_to_string(keys_dir.join(format!("{name}.pem")))?)))
            .collect::<std::io::Result<_>>()
            .with_context(|| format!("no archive keys in cache/refs/ArchiveKeys.cs or {}", keys_dir.display()))?,
    };
    let fetched = refs.join("SekiroDictionary.txt");
    let names = if fetched.exists() { fetched } else { keys_dir.join("SekiroDictionary.txt") };
    let dictionary: Vec<String> = std::fs::read_to_string(names)?
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    Ok(Vfs::open(game, &keys, &dictionary)?)
}

fn unpack(vfs: &Vfs, out: &Path, filter: Option<&str>) -> Result<()> {
    let mut paths: Vec<String> = vfs
        .paths()
        .filter(|p| filter.is_none_or(|f| p.contains(f)))
        .map(str::to_string)
        .collect();
    paths.sort_unstable();
    let done = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let total = paths.len();
    paths.par_iter().for_each(|path| {
        if let Err(e) = unpack_one(vfs, out, path) {
            failed.fetch_add(1, Ordering::Relaxed);
            eprintln!("FAILED {path}: {e:#}");
        }
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(500) || n == total {
            eprintln!("{n}/{total}");
        }
    });
    let unnamed = vfs.len() - vfs.paths().count();
    eprintln!(
        "unpacked {} files ({} failed); {unnamed} archive entries have no known name",
        total,
        failed.load(Ordering::Relaxed)
    );
    Ok(())
}

fn unpack_one(vfs: &Vfs, out: &Path, path: &str) -> Result<()> {
    let data = vfs.read(path)?;
    let rel = path.trim_start_matches('/');
    let rel = rel.strip_suffix(".dcx").unwrap_or(rel);
    let dest = out.join(rel);
    std::fs::create_dir_all(dest.parent().unwrap())?;
    if bnd4::is_bnd4(&data) {
        expand_binder(
            vfs,
            &data,
            &dest.with_extension(format!(
                "{}.d",
                dest.extension().and_then(|e| e.to_str()).unwrap_or("")
            )),
        )?;
    }
    std::fs::write(&dest, &data)?;
    Ok(())
}

/// Writes each binder member into `dir`, recursing into nested binders.
fn expand_binder(vfs: &Vfs, data: &[u8], dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for file in bnd4::parse(data)? {
        let contents = file.contents(vfs.oodle())?;
        let name = file.file_name();
        let name = if name.is_empty() {
            format!("{}", file.id)
        } else {
            name.to_string()
        };
        let dest = dir.join(name.strip_suffix(".dcx").unwrap_or(&name));
        if bnd4::is_bnd4(&contents) {
            expand_binder(
                vfs,
                &contents,
                &dest.with_extension(format!(
                    "{}.d",
                    dest.extension().and_then(|e| e.to_str()).unwrap_or("")
                )),
            )?;
        }
        std::fs::write(&dest, &contents)?;
    }
    Ok(())
}
