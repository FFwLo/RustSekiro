use sekiro_sim::tae::{TaeDb, TaeKind, sound_name};
fn main() {
    let cache = sekiro_sim::cache::root();
    let chr = std::env::args().nth(1).unwrap_or("c0000".into());
    let filter = std::env::args().nth(2).unwrap_or_default();
    let db = TaeDb::load_from_cache(&cache, &chr);
    let mut ids: Vec<i64> = db.anim_ids().collect();
    ids.sort();
    for id in ids {
        let name = sekiro_formats::tae::anim_name(id);
        if !name.contains(&filter) {
            continue;
        }
        let Some(a) = db.anim(&name) else { continue };
        let s: Vec<String> = a
            .events
            .iter()
            .filter_map(|e| match e.kind {
                TaeKind::Sound { kind, id } => {
                    Some(format!("{:.2}:{}", e.start, sound_name(kind, id)))
                }
                _ => None,
            })
            .collect();
        if !s.is_empty() {
            println!("{name} {s:?}");
        }
    }
}
