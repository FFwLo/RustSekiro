use sekiro_formats::{bxf4, dcx::Oodle, edge};
use std::path::PathBuf;

#[test]
fn installed_map_edge_counts() {
    let cache = sekiro_formats::cache::root().join("raw/map");
    for (map, hang, cover) in [("m11_01_00_00", 963, 1326), ("m11_00_00_00", 584, 974)] {
        let base = cache.join(map).join(map);
        if !base.with_extension("edgebhd").exists() {
            continue;
        }
        let game = std::env::var_os("SEKIRO_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro")
            });
        let oodle = Oodle::load(&game).unwrap();
        let bhd = std::fs::read(base.with_extension("edgebhd")).unwrap();
        let bdt = std::fs::read(base.with_extension("edgebdt")).unwrap();
        let mut counts = [0; 4];
        for file in bxf4::parse(&bhd, &bdt).unwrap() {
            let data = file.contents(Some(oodle)).unwrap();
            for e in edge::parse(&data).unwrap().edges {
                counts[e.kind() as usize] += 1;
            }
        }
        assert_eq!((counts[2], counts[3]), (hang, cover), "{map}");
    }
}

#[test]
fn rejects_malformed_records() {
    let mut data = vec![0; 80];
    data[0] = 4;
    data[4] = 1;
    data[68] = 2;
    data[69] = 1;
    let parsed = edge::parse(&data).unwrap();
    assert_eq!(parsed.edges[0].kind(), 2);
    assert_eq!(parsed.edges[0].variation(), 1);
    assert!(edge::parse(&data[..79]).is_err());
    data[0] = 3;
    assert!(edge::parse(&data).is_err());
}
