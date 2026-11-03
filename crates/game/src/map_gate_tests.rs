//! Installed-data gate. Baselines contain counts and MSB table indices only.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use sekiro_formats::msb::{self, PartKind};
use serde_json::{Value, json};

use super::visibility::{self, BlockRequest, BlockVisibility, VisibilityPart};

const MAPS: [&str; 10] = [
    "m10_00_00_00",
    "m11_00_00_00",
    "m11_01_00_00",
    "m11_02_00_00",
    "m13_00_00_00",
    "m15_00_00_00",
    "m17_00_00_00",
    "m20_00_00_00",
    "m25_00_00_00",
    "m89_00_00_99",
];
const KINDS: [PartKind; 8] = [
    PartKind::MapPiece,
    PartKind::Object,
    PartKind::Enemy,
    PartKind::Player,
    PartKind::Collision,
    PartKind::DummyObject,
    PartKind::DummyEnemy,
    PartKind::ConnectCollision,
];

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .unwrap()
}

/// Read only the GLB JSON chunk, never multi-megabyte vertex/index buffers.
fn glb_json(path: &Path) -> Option<Value> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0; 20];
    file.read_exact(&mut header).unwrap();
    assert_eq!(&header[..4], b"glTF");
    assert_eq!(u32::from_le_bytes(header[4..8].try_into().unwrap()), 2);
    assert_eq!(
        file.metadata().unwrap().len(),
        u32::from_le_bytes(header[8..12].try_into().unwrap()) as u64,
        "truncated GLB {}",
        path.display()
    );
    assert_eq!(&header[16..], b"JSON");
    let mut data = vec![0; u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize];
    file.read_exact(&mut data).unwrap();
    Some(serde_json::from_slice(&data).unwrap())
}

/// Independent translation of the documented executable branch, using MSB fields,
/// not exported JSON or the renderer's preprocessing/accessor functions.
fn exe_draw(part: &msb::Part) -> [u32; 8] {
    let d = part.display_data.as_ref().unwrap();
    let mut result = d.display_groups;
    if matches!(
        part.kind,
        PartKind::MapPiece | PartKind::Object | PartKind::Collision
    ) && d.condition1 == 0
        && d.condition2 != 0
    {
        for (i, word) in result.iter_mut().enumerate() {
            *word = 0;
            for bit in 0..32 {
                let source = if d.collision_mask[i] & (1 << bit) != 0 {
                    d.display_groups[i]
                } else {
                    d.draw_groups[i]
                };
                *word |= source & (1 << bit);
            }
        }
    }
    if part.kind == PartKind::MapPiece && result.iter().all(|&w| w == 0) {
        result.fill(u32::MAX);
    }
    result
}

fn exe_display(part: &msb::Part) -> [u32; 8] {
    let d = part.display_group.as_ref().unwrap();
    let mut result = d.disp_groups.map(|w| w as u32);
    if d.condition >= 0 {
        assert!(d.condition < 256);
        result[d.condition as usize / 32] |= 1 << (d.condition as usize % 32);
    }
    result
}

fn exe_targets<'a>(pattern: [u8; 4], inventory: &[&'a str]) -> Vec<&'a str> {
    if pattern == [255; 4] {
        return Vec::new();
    }
    inventory
        .iter()
        .copied()
        .filter(|id| {
            let fields: Vec<u8> = id[1..].split('_').map(|v| v.parse().unwrap()).collect();
            (0..4).all(|i| {
                if pattern[i] as i8 >= 0 {
                    pattern[i] == fields[i]
                } else {
                    i != 1 || fields[i] == 0
                }
            })
        })
        .collect()
}

#[test]
fn map_data_gate() {
    let started = std::time::Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cache = sekiro_sim::cache::root();
    let inventory: Vec<_> = MAPS
        .iter()
        .filter(|id| cache.join(format!("maps/{id}/layout.json")).is_file())
        .copied()
        .collect();
    for batch in MAPS.chunks(4) {
        std::thread::scope(|scope| {
            let workers: Vec<_> = batch
                .iter()
                .map(|&id| {
                    let cache = &cache;
                    let inventory = &inventory;
                    scope.spawn(move || check_map(root, cache, inventory, id))
                })
                .collect();
            for worker in workers {
                worker.join().unwrap();
            }
        });
    }
    eprintln!("map data gate: {:.2}s", started.elapsed().as_secs_f32());
}

fn check_map(root: &Path, cache: &Path, inventory: &[&str], id: &str) {
    let dir = cache.join(format!("maps/{id}"));
    if !dir.join("layout.json").is_file() {
        eprintln!("{id}: skipped (not extracted)");
        return;
    }
    let expected = read_json(&root.join(format!("tests/data/{id}.json")));
    let layout = read_json(&dir.join("layout.json"));
    assert_eq!(
        layout["layout_version"], 2,
        "{id}: refresh with sekiro-extract map {id} --layout-only"
    );
    let raw =
        msb::parse(&std::fs::read(cache.join(format!("raw/map/mapstudio/{id}.msb"))).unwrap())
            .unwrap();
    let rows = layout["parts"].as_array().unwrap();
    assert_eq!(rows.len(), raw.parts.len(), "{id}: missing placed parts");
    for ((row, part), index) in rows.iter().zip(&raw.parts).zip(0..) {
        assert_eq!(row["part_index"], index, "{id}");
        assert_eq!(row["name"], part.name, "{id}: part identity");
        assert_eq!(row["kind"], format!("{:?}", part.kind), "{id}: part kind");
        assert_eq!(row["model_index"], part.model_index, "{id}: model identity");
    }
    for (kind, number) in KINDS.iter().zip(expected[0].as_array().unwrap()) {
        let count = raw.parts.iter().filter(|p| p.kind == *kind).count();
        assert_eq!(json!(count), *number, "{id} {kind:?}: raw count changed");
        assert_eq!(
            layout["part_counts"][format!("{kind:?}")]
                .as_u64()
                .unwrap_or(0),
            count as u64,
            "{id}: exported count"
        );
    }
    let audit = read_json(&dir.join("material-audit.json"));
    let drawn_models: BTreeSet<_> = rows
        .iter()
        .filter(|r| r["render_status"]["asset"] == "exported")
        .map(|r| r["model"].as_str().unwrap())
        .collect();
    let mut absent_models = BTreeSet::new();
    let mut absent_uris = BTreeSet::new();
    // Enumerate each texture directory once; repeated filesystem probes for shared
    // images across thousands of materials dominate this otherwise CPU-only gate.
    let texture_files: BTreeSet<_> = std::fs::read_dir(dir.join("models/tex"))
        .unwrap()
        .map(|e| format!("tex/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    for model in &drawn_models {
        let path = layout["models"][model]["glb"].as_str().unwrap_or("");
        let Some(glb) = glb_json(&dir.join(path)) else {
            absent_models.insert(*model);
            continue;
        };
        if !glb["meshes"].as_array().is_some_and(|m| {
            m.iter()
                .any(|m| m["primitives"].as_array().is_some_and(|p| !p.is_empty()))
        }) {
            absent_models.insert(*model);
        }
        // Standard images plus every custom layer/fog URI, including extras.
        fn uris(value: &Value, out: &mut BTreeSet<String>) {
            match value {
                Value::Object(o) => {
                    for (k, v) in o {
                        if k == "uri"
                            && let Some(uri) = v.as_str()
                        {
                            out.insert(uri.to_owned());
                        } else {
                            uris(v, out);
                        }
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        uris(v, out);
                    }
                }
                _ => (),
            }
        }
        let mut referenced = BTreeSet::new();
        uris(&glb, &mut referenced);
        for uri in referenced {
            assert!(
                !uri.contains("..") && !Path::new(&uri).is_absolute(),
                "unsafe texture URI"
            );
            if !texture_files.contains(&uri) {
                absent_uris.insert(uri);
            }
        }
    }
    let missing_authored: BTreeSet<_> = audit["materials"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| drawn_models.contains(m["model"].as_str().unwrap()))
        .flat_map(|m| m["slots"].as_array().unwrap())
        .filter(|s| s["status"] == "missing")
        .map(|s| s["texture"].as_str().unwrap())
        .collect();
    let missing = [
        absent_models.len(),
        absent_uris.len(),
        missing_authored.len(),
    ];
    for (count, ceiling) in missing.iter().zip(expected[1].as_array().unwrap()) {
        assert!(
            *count <= ceiling.as_u64().unwrap() as usize,
            "{id}: missing count {count} exceeds {ceiling}; models {absent_models:?}, URIs {absent_uris:?}, authored {missing_authored:?}"
        );
    }
    let connections = layout["connections"].as_array().unwrap();
    assert_eq!(
        connections.len(),
        raw.parts.iter().filter(|p| p.connect.is_some()).count()
    );
    for part in raw.parts.iter().filter(|p| p.connect.is_some()) {
        let row = &rows[raw
            .parts
            .iter()
            .position(|p| std::ptr::eq(p, part))
            .unwrap()];
        let (index, pattern) = part.connect.unwrap();
        assert!(
            connections.iter().any(|c| c == row),
            "{id}: missing connection"
        );
        assert_eq!(row["connection"]["collision_index"], index);
        if index == -1 {
            assert!(row["connection"]["source_collision"].is_null());
        } else {
            let source = raw.collision_by_index(index).unwrap();
            assert_eq!(row["connection"]["source_collision"]["name"], source.name);
            let source = rows.iter().find(|r| r["name"] == source.name).unwrap();
            assert!(
                source["triangles"][1].as_u64().is_some_and(|n| n > 0),
                "{id}: connection source collision not exported"
            );
        }
        let targets: Vec<_> = if pattern == [255; 4] {
            Vec::new()
        } else {
            inventory
                .iter()
                .filter(|id| {
                    let fields: Vec<u8> = id[1..].split('_').map(|v| v.parse().unwrap()).collect();
                    (0..4).all(|i| {
                        if pattern[i] as i8 >= 0 {
                            pattern[i] == fields[i]
                        } else {
                            i != 1 || fields[i] == 0
                        }
                    })
                })
                .copied()
                .collect()
        };
        // Targets not extracted can be present in the raw inventory, but all listed
        // exported targets must exist and have their own collision asset.
        for target in row["connection"]["target_blocks"].as_array().unwrap() {
            let target = target.as_str().unwrap();
            if inventory.contains(&target) {
                assert!(targets.contains(&target), "{id}: incorrect target {target}");
                assert!(
                    cache.join(format!("maps/{target}/collision.bin")).is_file(),
                    "{id}: target collision absent"
                );
            } else {
                eprintln!("{id}: target {target} not extracted");
            }
        }
        for target in targets {
            assert!(
                row["connection"]["target_blocks"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(target)),
                "{id}: missing target {target}"
            );
        }
    }
    if visibility::supported(id) {
        let standing_names: BTreeSet<_> = expected[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| raw.parts[s[0].as_u64().unwrap() as usize].name.as_str())
            .collect();
        let mut wanted = BTreeSet::from([id]);
        for connection in &raw.parts {
            if let Some((index, pattern)) = connection.connect
                && index >= 0
                && standing_names.contains(raw.collision_by_index(index).unwrap().name.as_str())
            {
                wanted.extend(exe_targets(pattern, inventory));
            }
        }
        let neighbours: Vec<_> = inventory
            .iter()
            .filter(|map| wanted.contains(**map))
            .map(|&map| {
                (
                    map,
                    msb::parse(
                        &std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap(),
                    )
                    .unwrap(),
                    read_json(&cache.join(format!("maps/{map}/layout.json"))),
                )
            })
            .collect();
        let vm = visibility::VisibilityMap::from_layout(id, &layout)
            .unwrap()
            .unwrap();
        assert_eq!(expected[2].as_array().unwrap().len(), 10);
        for sample in expected[2].as_array().unwrap() {
            let standing = &raw.parts[sample[0].as_u64().unwrap() as usize];
            assert_eq!(standing.kind, PartKind::Collision);
            let mask = exe_display(standing);
            let requests = vm.requests([standing.name.as_str()], |_, _| true, |_| true);
            assert_eq!(
                requests[id].normal, mask,
                "{id}: collision display accessor"
            );
            let block = BlockVisibility {
                request: BlockRequest {
                    normal: mask,
                    ..Default::default()
                },
                draw: mask,
                resident: true,
                ready: true,
                groups_off: false,
            };
            let mut actual = BTreeSet::new();
            let mut oracle = BTreeSet::new();
            for (row, part) in rows
                .iter()
                .zip(&raw.parts)
                .filter(|(_, p)| matches!(p.kind, PartKind::MapPiece | PartKind::Object))
            {
                let vp = VisibilityPart::from_row(id, row).unwrap().unwrap();
                if visibility::decide(&vp, &block, true).visible {
                    actual.insert(&part.name);
                }
                if exe_draw(part).iter().zip(mask).any(|(a, b)| a & b != 0) {
                    oracle.insert(&part.name);
                }
            }
            assert_eq!(actual, oracle, "{id}: standing on {}", standing.name);
            assert_eq!(
                json!(actual.len()),
                sample[1],
                "{id}: visible count at {}",
                standing.name
            );
            // Independently derive attached connection requests from raw MSB indices,
            // signed target bytes and the connection's own display accessor.
            let mut raw_requests = BTreeMap::from([(id.to_string(), mask)]);
            for connection in &raw.parts {
                let Some((index, pattern)) = connection.connect else {
                    continue;
                };
                if index < 0 || raw.collision_by_index(index).unwrap().name != standing.name {
                    continue;
                }
                for target in exe_targets(pattern, inventory) {
                    let requested = raw_requests.entry(target.to_owned()).or_insert([0; 8]);
                    for (dst, src) in requested.iter_mut().zip(exe_display(connection)) {
                        *dst |= src;
                    }
                }
            }
            let chosen: BTreeMap<_, _> = requests
                .iter()
                .filter(|(map, _)| inventory.contains(&map.as_str()))
                .map(|(map, request)| (map.clone(), request.normal))
                .collect();
            assert_eq!(
                chosen, raw_requests,
                "{id}: connected requests at {}",
                standing.name
            );
            let mut actual_world = BTreeSet::new();
            let mut oracle_world = BTreeSet::new();
            for (map, parsed, exported) in &neighbours {
                let Some(&demand) = raw_requests.get(*map) else {
                    continue;
                };
                let block = BlockVisibility {
                    request: BlockRequest {
                        normal: chosen[*map],
                        ..Default::default()
                    },
                    draw: chosen[*map],
                    resident: true,
                    ready: true,
                    groups_off: false,
                };
                for (row, part) in exported["parts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(&parsed.parts)
                    .filter(|(_, p)| matches!(p.kind, PartKind::MapPiece | PartKind::Object))
                {
                    let source = visibility::SourceMasks::from_row(row).unwrap();
                    let vp = VisibilityPart::from_row(map, row)
                        .unwrap()
                        .unwrap_or_else(|| VisibilityPart {
                            map: (*map).to_owned(),
                            name: part.name.clone(),
                            entity_id: part.entity_id,
                            entity_groups: [0; 8],
                            effective: source.effective(row["kind"].as_str().unwrap()),
                            source,
                        });
                    if visibility::decide(&vp, &block, true).visible {
                        actual_world.insert((*map, &part.name));
                    }
                    if exe_draw(part).iter().zip(demand).any(|(a, b)| a & b != 0) {
                        oracle_world.insert((*map, &part.name));
                    }
                }
            }
            assert_eq!(
                actual_world, oracle_world,
                "{id}: connected visible set at {}",
                standing.name
            );
            assert_eq!(
                json!(actual_world.len()),
                sample[2],
                "{id}: connected visible count at {}",
                standing.name
            );
        }
    }
    eprintln!(
        "{id}: {} parts, {} connections, missing {missing:?}",
        rows.len(),
        connections.len()
    );
}
