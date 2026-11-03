use sekiro_formats::hkb::{self, NodeKind};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let g = hkb::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    let ev = |e: i32| {
        usize::try_from(e)
            .ok()
            .and_then(|e| g.events.get(e))
            .cloned()
            .unwrap_or("-".into())
    };
    for (id, sm) in g.state_machines() {
        for s in &sm.states {
            if !args[2..].contains(&s.name) {
                continue;
            }
            println!("{} / {} (id {})", g.nodes[id].name, s.name, s.id);
            for t in &s.transitions {
                println!(
                    "  local {} -> {} flags {:#x}",
                    ev(t.event),
                    t.to_state,
                    t.flags
                );
            }
            if let Some(gn) = s.generator {
                let n = &g.nodes[gn];
                if let NodeKind::Cmsg(c) = &n.kind {
                    println!(
                        "  cmsg {} end_type {} end_event {} check_slot {} change {} user_data {}",
                        n.name,
                        c.anime_end_event_type,
                        ev(c.end_event),
                        c.check_anim_end_slot,
                        c.change_type_of_selected_index_after_activate,
                        n.user_data
                    );
                }
            }
        }
    }
    let mut hist = std::collections::BTreeMap::<(i64, String), usize>::new();
    for n in &g.nodes {
        if let NodeKind::Cmsg(c) = &n.kind {
            *hist
                .entry((c.anime_end_event_type, ev(c.end_event)))
                .or_default() += 1;
        }
    }
    for (k, v) in hist.iter().take(40) {
        println!("end_type {} end_event {} : {v}", k.0, k.1);
    }
}
