use sekiro_formats::{bxf4, dcx::Oodle, edge, msb};
use std::path::PathBuf;
#[test]
fn authored_grapple_census() {
    let cache = sekiro_formats::cache::root();
    for (id, objects, edges) in [("m11_01_00_00", 172, 224), ("m11_00_00_00", 237, 360)] {
        let path = cache.join(format!("raw/map/mapstudio/{id}.msb"));
        if !path.exists() {
            continue;
        }
        let map = msb::parse(&std::fs::read(path).unwrap()).unwrap();
        let mut counts = (0, 0);
        for part in map.parts.iter().filter(|p| p.kind == msb::PartKind::Object) {
            let model = map.model(part).unwrap();
            let path = cache.join(format!(
                "raw/obj/{}.objbnd.d/e{}.edge",
                model.name,
                model.name.trim_start_matches('o')
            ));
            if !path.exists() {
                continue;
            }
            let data = edge::parse(&std::fs::read(path).unwrap()).unwrap();
            let n = data.edges.iter().filter(|e| e.kind() == 1).count();
            if n > 0 {
                counts.0 += 1;
                counts.1 += n;
            }
        }
        assert_eq!(counts, (objects, edges));
        if id == "m11_00_00_00" {
            let game = std::env::var_os("SEKIRO_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro")
                });
            let oodle = Oodle::load(&game).unwrap();
            let base = cache.join(format!("raw/map/{id}/{id}"));
            let bhd = std::fs::read(base.with_extension("edgebhd")).unwrap();
            let bdt = std::fs::read(base.with_extension("edgebdt")).unwrap();
            let mut n = 0;
            for file in bxf4::parse(&bhd, &bdt).unwrap() {
                let data = edge::parse(&file.contents(Some(oodle)).unwrap()).unwrap();
                if data.edges.iter().any(|e| e.kind() == 1) {
                    assert_eq!(data.owner, 60000);
                    assert!(
                        !map.parts
                            .iter()
                            .any(|p| map.model(p).is_some_and(|m| m.name == "h060000"))
                    );
                    n += data.edges.iter().filter(|e| e.kind() == 1).count();
                }
            }
            assert_eq!(n, 4);
        }
    }
}
