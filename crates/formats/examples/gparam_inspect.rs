//! Read-only summary of one GPARAM file: groups, params, the value ids each param carries,
//! and with a param-name filter the authored values.
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("path to a .gparam file");
    let group_filter = std::env::args().nth(2);
    let param_filter = std::env::args().nth(3);
    let data = std::fs::read(&path)?;
    let g = sekiro_formats::gparam::parse(&data)?;
    for group in &g.groups {
        if group_filter
            .as_ref()
            .is_some_and(|f| !group.name.contains(f.as_str()))
        {
            continue;
        }
        println!("GROUP {}", group.name);
        for p in &group.params {
            if let Some(f) = &param_filter {
                if !p.name.contains(f.as_str()) {
                    continue;
                }
                for e in &p.entries {
                    println!("  {} id={} t={} {:?}", p.name, e.id, e.time, e.value);
                }
                continue;
            }
            let mut ids: BTreeMap<i32, usize> = BTreeMap::new();
            for e in &p.entries {
                *ids.entry(e.id).or_default() += 1;
            }
            println!("  {} ids={ids:?}", p.name);
        }
    }
    Ok(())
}
