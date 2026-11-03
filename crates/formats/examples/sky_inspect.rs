//! Read-only sky placement and material audit.
use sekiro_formats::{emevd, flver, gparam, msb, mtd};
use std::{error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let raw = Path::new("cache/raw");
    for e in fs::read_dir(raw.join("event"))? {
        let e = e?;
        let Ok(ev) = emevd::Emevd::parse(&fs::read(e.path())?) else {
            continue;
        };
        for event in ev.events {
            if event.instructions.iter().any(|i| {
                i.words()
                    .any(|w| (8904000..=8904300).contains(&w) || (8907000..=8907300).contains(&w))
                    || (i.bank == 2003 && (80..=100).contains(&i.id))
            }) {
                println!("EVENT {} {}", e.file_name().to_string_lossy(), event.id);
                for i in event.instructions {
                    println!(
                        "INS {}[{}] {:?}",
                        i.bank,
                        i.id,
                        i.words().collect::<Vec<_>>()
                    );
                }
            }
        }
    }
    let mut materials = Vec::new();
    for e in fs::read_dir(raw.join("mtd/allmaterialbnd.mtdbnd.d"))? {
        let e = e?;
        if let Ok(m) = mtd::parse(&fs::read(e.path())?)
            && m.shader.contains("Sky")
        {
            println!("MTD {} {m:?}", e.file_name().to_string_lossy());
            materials.push(e.file_name().to_string_lossy().to_lowercase());
        }
    }
    for id in [
        "m89_00_00_99",
        "m11_00_00_00",
        "m11_01_00_00",
        "m25_00_00_00",
        "m10_00_00_00",
    ] {
        let m = msb::parse(&fs::read(raw.join(format!("map/mapstudio/{id}.msb")))?)?;
        for p in &m.parts {
            let Some(model) = m.model(p) else { continue };
            if !model.name.starts_with("m9") {
                continue;
            }
            let digits = &model.name[1..];
            let path = raw.join(format!(
                "map/{id}/{id}_{digits}.mapbnd.d/{id}_{digits}.flver"
            ));
            let Ok(bytes) = fs::read(path) else { continue };
            let f = flver::parse(&bytes)?;
            if f.materials
                .iter()
                .any(|x| materials.iter().any(|n| x.mtd.to_lowercase().ends_with(n)))
            {
                println!(
                    "PART {id} {} {} pos={:?} rot={:?} gparam={:?} entity={} groups={:?} materials={:?}",
                    p.name,
                    model.name,
                    p.position,
                    p.rotation,
                    p.gparam,
                    p.entity_id,
                    p.entity_group_ids,
                    f.materials
                );
            }
        }
    }
    for e in fs::read_dir(raw.join("param/drawparam"))? {
        let e = e?;
        let n = e.file_name().to_string_lossy().into_owned();
        if !(n.starts_with("m89")
            || n.starts_with("s89")
            || n.starts_with("m11_0")
            || n.starts_with("m25")
            || n.starts_with("m10"))
        {
            continue;
        }
        let Ok(g) = gparam::parse(&fs::read(e.path())?) else {
            continue;
        };
        for group in g.groups {
            for p in group.params {
                if p.name.to_lowercase().contains("sky")
                    || group.name.to_lowercase().contains("sky")
                    || group.name.contains("Setting")
                {
                    println!("GPARAM {n} {} {p:?}", group.name);
                }
            }
        }
    }
    Ok(())
}
