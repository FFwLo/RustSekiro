//! Parses the player and soldier behaviour graphs. Skips when `cache/` has not been extracted.

use sekiro_formats::hkb::{self, NodeKind};

fn chr(path: &str) -> Option<Vec<u8>> {
    let p = sekiro_formats::cache::root().join("raw/chr").join(path);
    match std::fs::read(&p) {
        Ok(d) => Some(d),
        Err(_) => {
            eprintln!("skipping: {} not extracted", p.display());
            None
        }
    }
}

#[test]
fn player_graph() {
    let Some(data) = chr("c0000.behbnd.d/c0000.hkx") else {
        return;
    };
    let g = hkb::parse(&data).unwrap();
    assert_eq!(g.events.len(), 1908);
    assert_eq!(g.variables.len(), 311);
    assert!(g.nodes.len() > 5000);
    let clips = g
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Clip(_)))
        .count();
    let cmsgs = g
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Cmsg(_)))
        .count();
    assert_eq!((clips, cmsgs), (2566, 2041));

    // The guard-start event is a global wildcard of the deflect-guard machine.
    let ev = g.event_index("W_StandToDeflectGuard").unwrap() as i32;
    let (_, sm) = g
        .state_machines()
        .find(|(id, _)| g.nodes[*id].name == "DeflectGuard_SM")
        .unwrap();
    let t = sm
        .wildcard_transitions
        .iter()
        .find(|t| t.event == ev)
        .unwrap();
    let state = &sm.states[sm.state_index(t.to_state).unwrap()];
    assert_eq!(state.name, "StandToDeflectGuard");
    let NodeKind::Cmsg(c) = &g.nodes[state.generator.unwrap()].kind else {
        panic!("guard start is not a CMSG")
    };
    assert_eq!((c.anim_id, c.offset_type), (203000, 13));
    let NodeKind::Clip(clip) = &g.nodes[c.generators[0].unwrap()].kind else {
        panic!("CMSG child is not a clip")
    };
    assert_eq!(clip.animation, "a050_203000");
    assert_eq!(clip.anim_id(), Some((50, 203000)));
}

#[test]
fn soldier_graphs() {
    let Some(stub) = chr("c1010.behbnd.d/c1010.hkx") else {
        return;
    };
    let g = hkb::parse(&stub).unwrap();
    let reference = g.nodes.iter().find_map(|n| match &n.kind {
        NodeKind::BehaviorReference { behavior_name } => Some(behavior_name.clone()),
        _ => None,
    });
    assert_eq!(reference.as_deref(), Some("Behaviors\\c9997"));

    let shared = hkb::parse(&chr("c1010.behbnd.d/c9997.hkx").unwrap()).unwrap();
    assert_eq!(shared.events.len(), 1002);
    assert!(shared.state_machines().count() > 1);
}
