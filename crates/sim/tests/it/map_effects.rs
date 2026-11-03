use sekiro_sim::{map_effects, world_events};

#[test]
fn installed_map_effects_resolve_model_parameters_and_story_commands() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("maps/m11_00_00_00/layout.json").exists() {
        return;
    }
    for map in world_events::SUPPORTED_MAPS {
        let state = world_events::first_visit(&cache, map).unwrap().unwrap();
        let catalog = map_effects::load(&cache, map, &state).unwrap();
        let braziers: Vec<_> = catalog
            .placements
            .iter()
            .filter(|p| p.param == Some(116000000))
            .collect();
        assert!(!braziers.is_empty());
        assert!(
            braziers
                .iter()
                .all(|p| p.effect == 839530 && p.dummy == Some(200))
        );
        assert!(
            catalog
                .placements
                .iter()
                .all(|p| p.position.iter().all(|x| x.is_finite()))
        );
        for placement in &catalog.placements {
            if let Some(write) = &placement.source {
                assert_eq!(write.entity, placement.entity);
                if write.command == 200903 {
                    assert!(state.bonfires.contains_key(&placement.entity));
                    continue;
                }
                assert!(state.effect_controls.iter().any(|c| c.event == write.event
                    && c.slot == write.slot
                    && c.instruction == write.instruction));
            }
        }
        assert!(catalog.placements.iter().any(|p| p.effect == 839500));
    }
}

/// The highest particle-drawing node of installed effect `id`, from the effect's origin (game
/// axes, y up): node offsets summed down the tree (the idol effects rotate only leaf nodes).
fn highest_drawn_node(cache: &std::path::Path, id: i32) -> glam::Vec3 {
    fn walk(node: &serde_json::Value, at: glam::Vec3, best: &mut Option<glam::Vec3>) {
        let config = &node["configs"][0];
        let offset = config["nodeTransform"]["offset"]
            .as_array()
            .map_or(glam::Vec3::ZERO, |o| {
                let f = |i: usize| o[i].as_f64().unwrap_or(0.0) as f32;
                glam::Vec3::new(f(0), f(1), f(2))
            });
        let at = at + offset;
        // 600s draw particles; 609 is the effect's point light.
        let drawn = config["appearance"]["type"]
            .as_i64()
            .is_some_and(|t| (600..609).contains(&t));
        if drawn && best.is_none_or(|b| at.y > b.y) {
            *best = Some(at);
        }
        for child in node["nodes"].as_array().into_iter().flatten() {
            walk(child, at, best);
        }
    }
    let path = std::fs::read_dir(cache.join("raw/sfx"))
        .unwrap()
        .flatten()
        .map(|d| d.path().join(format!("f{id:09}.fxr")))
        .find(|p| p.exists())
        .unwrap_or_else(|| panic!("FFX {id} not installed"));
    let fxr = sekiro_formats::fxr::parse(&std::fs::read(path).unwrap()).unwrap();
    let (tree, _) = sekiro_formats::fxr_json::to_json(&fxr);
    let mut best = None;
    walk(&tree["root"], glam::Vec3::ZERO, &mut best);
    best.unwrap_or_else(|| panic!("FFX {id} draws nothing"))
}

/// #154: an idol's idle effect follows its lit state. Unlit, its unlit idle (embers circling
/// it) shows; once Wolf lights it, or when the story has it lit, only the lit idle shows, whose
/// flame sits just above the idol's hands, at the idol's dummy 100 (its origin).
#[test]
fn every_idol_shows_its_flame_above_its_hands_once_lit() {
    let cache = sekiro_sim::cache::root();
    if !cache.join("maps/m11_00_00_00/layout.json").exists() {
        return;
    }
    for map in ["m11_00_00_00", "m11_01_00_00"] {
        let state = world_events::first_visit(&cache, map).unwrap().unwrap();
        let catalog = map_effects::load(&cache, map, &state).unwrap();
        let msb = sekiro_formats::msb::parse(
            &std::fs::read(cache.join(format!("raw/map/mapstudio/{map}.msb"))).unwrap(),
        )
        .unwrap();
        let mut idols = 0;
        for part in msb
            .parts
            .iter()
            .filter(|p| state.bonfires.contains_key(&p.entity_id))
        {
            idols += 1;
            let idle: Vec<_> = catalog
                .placements
                .iter()
                .filter(|p| p.entity == part.entity_id && p.idol.is_some())
                .collect();
            let shown = |found: bool| -> Vec<i32> {
                idle.iter()
                    .filter(|p| p.idol.unwrap().shows(found))
                    .map(|p| p.effect)
                    .collect()
            };
            let lit = shown(true);
            assert!(
                !lit.is_empty(),
                "{map} idol {}: no lit idle",
                part.entity_id
            );
            assert!(
                idle.iter()
                    .filter(|p| lit.contains(&p.effect))
                    .all(|p| p.idol.unwrap().lit),
                "{map} idol {}: a lit idol still shows its unlit idle {lit:?}",
                part.entity_id
            );
            if !idle[0].idol.unwrap().lit_at_start {
                let unlit = shown(false);
                assert!(!unlit.is_empty() && unlit.iter().all(|e| !lit.contains(e)));
            }
            let model = &msb.models[part.model_index as usize].name;
            let flver = sekiro_formats::flver::parse(
                &std::fs::read(cache.join(format!("raw/obj/{model}.objbnd.d/{model}.flver")))
                    .unwrap(),
            )
            .unwrap();
            let origin = glam::Vec3::new(-part.position[0], part.position[1], part.position[2]);
            for p in idle.iter().filter(|p| p.idol.unwrap().lit) {
                // Dummy 100 is the idol's origin; the flame's height is the effect's own.
                assert_eq!(p.dummy, Some(100));
                assert!(glam::Vec3::from_array(p.position).distance(origin) < 1e-3);
                let flame = highest_drawn_node(&cache, p.effect);
                let top = flver.bbox_max[1];
                assert!(
                    glam::Vec2::new(flame.x, flame.z).length() < 0.25
                        && flame.y > top - 0.1
                        && flame.y < top + 0.4,
                    "{map} idol {} ({model}, top {top:.2} m): FFX {} flame at {flame:.2?}",
                    part.entity_id,
                    p.effect
                );
            }
        }
        assert!(idols >= 7, "{map}: {idols} idols");
    }
}
