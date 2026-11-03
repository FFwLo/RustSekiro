//! `variables <behaviour.hkx> [name...]`: variable types, initial values and bounds.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let g = sekiro_formats::hkb::parse(&std::fs::read(&args[1]).unwrap()).unwrap();
    for v in &g.variables {
        if args.len() > 2 && !args[2..].iter().any(|a| v.name.contains(a.as_str())) {
            continue;
        }
        println!(
            "{:40} {:?} init {:?} bounds {:?}",
            v.name, v.ty, v.initial, v.bounds
        );
    }
}
