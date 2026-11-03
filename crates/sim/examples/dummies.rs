//! `dummies <file.flver>...`: dummy poly reference ids with their attach and parent nodes.
fn main() {
    for path in std::env::args().skip(1) {
        let f = sekiro_formats::flver::parse(&std::fs::read(&path).unwrap()).unwrap();
        let name = |i: i16| {
            usize::try_from(i)
                .ok()
                .and_then(|i| f.nodes.get(i))
                .map_or("-".to_string(), |n| n.name.clone())
        };
        let mut ids: Vec<String> = f
            .dummies
            .iter()
            .map(|d| {
                format!(
                    "{}@{}/{}",
                    d.reference_id,
                    name(d.attach_bone),
                    name(d.parent_bone)
                )
            })
            .collect();
        ids.sort();
        println!("{path}: {} dummies: {}", f.dummies.len(), ids.join(" "));
    }
}
