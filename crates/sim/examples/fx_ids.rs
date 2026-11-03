//! `fx_ids <chr>...`: the FXR effect ids each character's TAE timelines spawn (events 96, 99,
//! 100, 118-121), with how many events use them. Run from the repository root.

use sekiro_formats::tae;
use sekiro_sim::tae::{TaeDb, TaeKind};
use std::collections::BTreeMap;

fn main() {
    for chr in std::env::args().skip(1) {
        let db = TaeDb::load_from_cache(&sekiro_sim::cache::root(), &chr);
        let mut ids: BTreeMap<(i32, i32), usize> = BTreeMap::new();
        for id in db.anim_ids() {
            let Some(anim) = db.anim(&tae::anim_name(id)) else {
                continue;
            };
            for e in anim.events.iter() {
                if let TaeKind::Effect(fx) = &e.kind {
                    *ids.entry((fx.event_type, fx.id)).or_default() += 1;
                }
            }
        }
        println!("{chr}: {} animations", db.len());
        for ((kind, id), n) in ids {
            println!("  type {kind} id {id}: {n}");
        }
    }
}
