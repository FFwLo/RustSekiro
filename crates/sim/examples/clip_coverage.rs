//! `clip_coverage`: how the player graph's clip names resolve to extracted animations.
//! Run from the repository root.
use sekiro_formats::hkb::{self, NodeKind};
use sekiro_formats::tae;
use std::collections::BTreeSet;

fn main() {
    let g = hkb::parse(&std::fs::read("cache/raw/chr/c0000.behbnd.d/c0000.hkx").unwrap()).unwrap();
    let names: BTreeSet<String> = g
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Clip(c) => Some(c.animation.clone()),
            _ => None,
        })
        .collect();
    let parsed = sekiro_sim::tae::read_tae_dir("cache/raw/chr/c0000.anibnd.d".as_ref());
    let exists = |n: &str| std::path::Path::new(&format!("cache/anim/c0000/{n}.bin")).exists();
    let (mut direct, mut alias, mut no_tae, mut dangling) = (0, 0, Vec::new(), Vec::new());
    for n in &names {
        if exists(n) {
            direct += 1;
            continue;
        }
        let Some(id) = tae::anim_id(n) else {
            dangling.push(format!("{n} (unparsable)"));
            continue;
        };
        match parsed.get(&id) {
            None => no_tae.push(n.clone()),
            Some(a) => {
                let src = tae::anim_name(a.hkx_source);
                if exists(&src) {
                    alias += 1;
                } else {
                    dangling.push(format!("{n} -> {src}"));
                }
            }
        }
    }
    println!(
        "{} clip names: {direct} direct, {alias} via TAE alias, {} with no TAE entry, {} dangling",
        names.len(),
        no_tae.len(),
        dangling.len()
    );
    println!(
        "no TAE entry (first 30): {:?}",
        &no_tae[..no_tae.len().min(30)]
    );
    println!(
        "dangling (first 30): {:?}",
        &dangling[..dangling.len().min(30)]
    );
}
