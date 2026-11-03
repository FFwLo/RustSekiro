//! Checks the MSB, BXF4 and hknp collision readers against the player's own unpacked game data
//! (`cache/raw/map`, area m11_00_00_00). Skips when `cache/` has not been extracted; the
//! collision test also needs the game install for Oodle and skips without it.

use std::path::{Path, PathBuf};

use sekiro_formats::{bxf4, dcx::Oodle, hknp, hkx::Compendium, msb};

const GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Sekiro";

fn map_dir() -> PathBuf {
    sekiro_formats::cache::root().join("raw/map")
}

fn read(path: &Path) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(d) => Some(d),
        Err(_) => {
            eprintln!("skipping: {} not extracted", path.display());
            None
        }
    }
}

#[test]
fn m11_msb_parts() {
    let Some(data) = read(&map_dir().join("mapstudio/m11_00_00_00.msb")) else {
        return;
    };
    let m = msb::parse(&data).unwrap();
    let count = |k: msb::PartKind| m.parts.iter().filter(|p| p.kind == k).count();
    assert_eq!(count(msb::PartKind::MapPiece), 4498);
    assert_eq!(count(msb::PartKind::Object), 1509);
    assert_eq!(count(msb::PartKind::Enemy), 144);
    assert_eq!(count(msb::PartKind::Collision), 166);
    assert_eq!(count(msb::PartKind::Player), 10);
    let soldiers: Vec<&msb::Part> = m
        .parts
        .iter()
        .filter(|p| p.kind == msb::PartKind::Enemy && m.model(p).unwrap().name == "c1010")
        .collect();
    assert_eq!(soldiers.len(), 43);
    // Every soldier has NPC and think params.
    for s in soldiers {
        let e = s.enemy.as_ref().unwrap();
        assert!(e.npc_param_id > 0 && e.think_param_id > 0, "{}", s.name);
    }
    let start = m.parts.iter().find(|p| p.name == "c0000_0000").unwrap();
    assert_eq!(start.position, [125.80372, -47.22, -193.07729]);
    assert_eq!(start.rotation, [0.0, 158.0, 0.0]);
    // Map piece mask blocks: 48 words (display, draw, collision mask).
    assert!(
        m.parts
            .iter()
            .filter(|p| p.kind == msb::PartKind::MapPiece)
            .all(|p| p.masks.len() == 48)
    );
}

#[test]
fn m11_collision_decodes() {
    let dir = map_dir().join("m11_00_00_00");
    let (Some(bhd), Some(bdt)) = (
        read(&dir.join("h11_00_00_00.hkxbhd")),
        read(&dir.join("h11_00_00_00.hkxbdt")),
    ) else {
        return;
    };
    let Ok(oodle) = Oodle::load(Path::new(GAME)) else {
        eprintln!("skipping: Oodle not found in {GAME}");
        return;
    };
    let files = bxf4::parse(&bhd, &bdt).unwrap();
    assert_eq!(files.len(), 174);
    let comp = files
        .iter()
        .find(|f| f.file_name().contains("compendium"))
        .unwrap();
    let comp = Compendium::parse(&comp.contents(Some(oodle)).unwrap()).unwrap();
    let f = files
        .iter()
        .find(|f| f.file_name() == "h11_00_00_00_000100.hkx.dcx")
        .unwrap();
    let (mesh, stats) = hknp::decode(&f.contents(Some(oodle)).unwrap(), Some(&comp)).unwrap();
    assert_eq!(stats.bodies, 1);
    assert_eq!(stats.sections, 131);
    assert_eq!(stats.primitives, 10656);
    // numPrimitiveKeys in the file: every primitive plus the second triangle of each quad.
    assert_eq!(mesh.triangle_count(), 13060);
    assert_eq!(stats.unmatched_material, 0);
    assert!(mesh.materials.iter().all(|&m| m != u32::MAX));
    // Vertices stay inside the mesh tree domain.
    for v in &mesh.vertices {
        assert!((11.5..=33.9).contains(&v[0]), "{v:?}");
        assert!((-85.9..=-27.4).contains(&v[1]), "{v:?}");
        assert!((-28.8..=-5.0).contains(&v[2]), "{v:?}");
    }
    let bytes = mesh.to_bytes().unwrap();
    assert_eq!(hknp::CollisionMesh::from_bytes(&bytes).unwrap(), mesh);
}

#[test]
fn m11_light_set_reads() {
    let path = map_dir().join("../param/drawparam/m11_00_0000.gparam");
    let Some(data) = read(&path) else {
        return;
    };
    let g = sekiro_formats::gparam::parse(&data).unwrap();
    assert_eq!(g.groups.len(), 23);
    let angle = g.param("LightSet", "Directional Light Angle0").unwrap();
    let a = angle.at(0, 12.0).unwrap();
    assert!(
        (a[0] + 1.361).abs() < 1e-3 && (a[1] - 2.094).abs() < 1e-3,
        "{a:?}"
    );
    let colour = g.param("LightSet", "Directional Light DiffColor0").unwrap();
    assert_eq!(colour.at(0, 18.0).unwrap().len(), 4);
}
