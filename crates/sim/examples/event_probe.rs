use sekiro_formats::hkb::{self, NodeKind};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let g = hkb::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    for ev_name in &args[2..] {
        let Some(ev) = g.event_index(ev_name) else {
            println!("{ev_name}: no such event");
            continue;
        };
        println!("event {ev_name} = {ev}");
        for (id, sm) in g.state_machines() {
            let name = &g.nodes[id].name;
            for t in &sm.wildcard_transitions {
                if t.event == ev as i32 {
                    let to = sm
                        .states
                        .iter()
                        .find(|s| s.id == t.to_state)
                        .map(|s| s.name.as_str())
                        .unwrap_or("?");
                    let fx = t
                        .effect
                        .map(|e| {
                            format!(
                                "{} {:?} dur {:?}",
                                g.effects[e].type_name, g.effects[e].name, g.effects[e].duration
                            )
                        })
                        .unwrap_or_default();
                    println!(
                        "  wildcard in {name}: -> {} {to} nested {} flags {:#x} {fx}",
                        t.to_state, t.to_nested_state, t.flags
                    );
                }
            }
            for s in &sm.states {
                for t in &s.transitions {
                    if t.event == ev as i32 {
                        println!(
                            "  from {name}/{}: -> {} nested {} flags {:#x}",
                            s.name, t.to_state, t.to_nested_state, t.flags
                        );
                    }
                }
            }
        }
    }
    let n_local: usize = g
        .state_machines()
        .map(|(_, sm)| sm.states.iter().map(|s| s.transitions.len()).sum::<usize>())
        .sum();
    println!("state-local transitions total: {n_local}");
    let mut ctypes = std::collections::BTreeMap::<String, usize>::new();
    for n in &g.nodes {
        if let NodeKind::Other { .. } = n.kind {
            *ctypes.entry(n.type_name.clone()).or_default() += 1;
        }
    }
    println!("other node types: {ctypes:?}");
    for (i, fx) in g.effects.iter().enumerate() {
        println!(
            "effect {i}: {} {:?} dur {:?} choices {:?} sel {:?} bind {:?}",
            fx.type_name, fx.name, fx.duration, fx.choices, fx.selected_index, fx.bindings
        );
    }
}
