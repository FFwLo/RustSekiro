//! `hkb-outline <behaviour.hkx> [depth]`: prints the behaviour graph tree (for `re/`, not git).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read");
    let depth = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(64);
    let graph = sekiro_formats::hkb::parse(&data).expect("parse behaviour graph");
    println!(
        "graph {:?}: {} nodes, {} effects, {} events, {} variables, {} animations",
        graph.name,
        graph.nodes.len(),
        graph.effects.len(),
        graph.events.len(),
        graph.variables.len(),
        graph.animation_names.len()
    );
    print!("{}", graph.outline(graph.root, depth));
}
