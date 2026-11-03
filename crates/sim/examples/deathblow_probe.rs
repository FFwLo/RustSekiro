//! `deathblow_probe <graph.hkx> <state>...`: prints the animation ids a behaviour-graph state's
//! CMSG plays, e.g. `TrunkCollapseFront` or `ThrowDef12000` in c9997.hkx.
use sekiro_formats::hkb::{self, NodeKind};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let g = hkb::parse(&std::fs::read(&args[1]).expect("graph file")).expect("graph");
    for (_, sm) in g.state_machines() {
        for s in &sm.states {
            if !args[2..].contains(&s.name) {
                continue;
            }
            let Some(gn) = s.generator else { continue };
            if let NodeKind::Cmsg(c) = &g.nodes[gn].kind {
                println!(
                    "{}: anim_id {} offset_type {}",
                    s.name, c.anim_id, c.offset_type
                );
            }
        }
    }
}
