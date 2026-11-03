//! `tae_probe <anim>...`: prints the TAE events of player animations (or of `$CHR`), with SpEffect behaviour
//! reference ids resolved through SpEffectParam. Run from the repository root.

use sekiro_formats::param::ParamSet;
use sekiro_formats::tae::{self, Template};
use std::collections::HashMap;

fn main() {
    let chr = std::env::var("CHR").unwrap_or_else(|_| "c0000".into());
    let dir = std::path::PathBuf::from(format!("cache/raw/chr/{chr}.anibnd.d"));
    let template =
        Template::parse_xml(&std::fs::read_to_string("cache/refs/TAE.Template.SDT.xml").unwrap())
            .unwrap();
    let t0 = std::time::Instant::now();
    let params = ParamSet::load(
        "cache/raw/param/gameparam/gameparam.parambnd.d".as_ref(),
        "cache/refs/paramdex/Defs".as_ref(),
    )
    .unwrap();
    eprintln!("params loaded in {:?}", t0.elapsed());
    let sp = params.table("SpEffectParam").unwrap();
    let mut anims: HashMap<i64, tae::TaeAnimation> = HashMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("tae") {
            continue;
        }
        let cat = tae::category_from_file_name(&p.file_name().unwrap().to_string_lossy());
        let t = tae::parse(&std::fs::read(&p).unwrap()).unwrap();
        for a in t.animations {
            anims.insert(tae::full_id(cat, a.id), a);
        }
    }
    for name in std::env::args().skip(1) {
        let id = tae::anim_id(&name).unwrap();
        let Some(a) = anims.get(&id) else {
            println!("{name}: no TAE entry");
            continue;
        };
        println!(
            "{name} (hkx {}):",
            tae::anim_name(tae::full_id(id / 1_000_000, a.hkx_source()))
        );
        for ev in &a.events {
            let d = template.decode(ev);
            let label = d.as_ref().map(|d| d.name.clone()).unwrap_or_default();
            let fields: Vec<String> = d
                .as_ref()
                .map(|d| {
                    d.fields
                        .iter()
                        .map(|f| match &f.label {
                            Some(l) => format!("{}={l}", f.name),
                            None => format!("{}={:?}", f.name, f.value),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut extra = String::new();
            if (ev.event_type == 66 || ev.event_type == 67)
                && let Some(spid) = d
                    .as_ref()
                    .and_then(|d| d.field("SpEffectID"))
                    .and_then(|v| v.as_int())
                && let Some(row) = sp.find(spid as i32)
            {
                extra = format!(
                    " -> behaviorRefId {} stateInfo {} endurance {}",
                    row.int("behaviorRefId").unwrap_or(-1),
                    row.int("stateInfo").unwrap_or(-1),
                    row.f32("effectEndurance").unwrap_or(-1.0)
                );
            }
            println!(
                "  {:4} {:6.3}-{:6.3} {label} {}{extra}",
                ev.event_type,
                ev.start,
                ev.end,
                fields.join(" ")
            );
        }
    }
}
