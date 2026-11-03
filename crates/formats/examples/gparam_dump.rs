//! Read-only draw-param and MSB facts for the visual baseline.
use std::{collections::BTreeSet, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = std::env::args()
        .nth(1)
        .map_or_else(sekiro_formats::cache::root, PathBuf::from);
    for id in ["m11_00_00_00", "m11_01_00_00", "m25_00_00_00"] {
        println!("MAP {id}");
        let msb = sekiro_formats::msb::parse(&std::fs::read(
            cache.join(format!("raw/map/mapstudio/{id}.msb")),
        )?)?;
        let ids: BTreeSet<_> = msb.parts.iter().filter_map(|p| p.gparam).collect();
        println!("MSB gparam [light, fog, scattering, env]: {ids:?}");
        let mut paths: Vec<_> = std::fs::read_dir(cache.join("raw/param/drawparam"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()?;
        paths.retain(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with(&format!("{}_", &id[..6])) && n.ends_with(".gparam")
            })
        });
        paths.sort();
        for path in paths {
            println!("FILE {}", path.file_name().unwrap().to_string_lossy());
            let g = sekiro_formats::gparam::parse(&std::fs::read(&path)?)?;
            let banks: BTreeSet<_> = g
                .groups
                .iter()
                .flat_map(|g| &g.params)
                .filter(|p| p.name.contains("LightBankId"))
                .flat_map(|p| &p.entries)
                .filter_map(|e| match e.value {
                    sekiro_formats::gparam::Value::Int(i) => Some(i),
                    _ => None,
                })
                .collect();
            println!("Distinct LightBankId: {banks:?}");
            for group in &g.groups {
                for param in &group.params {
                    for entry in &param.entries {
                        println!(
                            "{} / {} / id={} / hour={} = {:?}",
                            group.name, param.name, entry.id, entry.time, entry.value
                        );
                    }
                }
            }
            for hour in [0., 2., 6., 12., 18., 22.] {
                println!("HOUR {hour}");
                for group in &g.groups {
                    if [
                        "LightSet",
                        "VolumetricFog",
                        "ColorGrading",
                        "Bloom",
                        "Tone Map",
                    ]
                    .iter()
                    .any(|s| group.name.contains(s))
                    {
                        for p in &group.params {
                            println!("{} / {} / id=0 = {:?}", group.name, p.name, p.at(0, hour));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
