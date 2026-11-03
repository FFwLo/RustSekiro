//! `sm_bindings <behaviour.hkx>`: state machines whose members are bound to variables.
use sekiro_formats::hkb::{self, NodeKind};
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let g = hkb::parse(&std::fs::read(path).unwrap()).unwrap();
    for (id, sm) in g.state_machines() {
        let n = &g.nodes[id];
        if n.bindings.is_empty() {
            continue;
        }
        let b: Vec<String> = n
            .bindings
            .iter()
            .map(|b| {
                format!(
                    "{} <- {}",
                    b.member_path, g.variables[b.variable as usize].name
                )
            })
            .collect();
        println!(
            "{} start={} mode={}: {}",
            n.name,
            sm.start_state_id,
            sm.start_state_mode,
            b.join(", ")
        );
    }
    let _ = NodeKind::Other { children: vec![] };
}
