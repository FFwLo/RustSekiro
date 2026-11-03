//! Read-only inspection of one installed FXR; never exports proprietary values into source.
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: fxr_dump <file.fxr>")?;
    let effect = sekiro_formats::fxr::parse(&std::fs::read(&path)?)?;
    println!(
        "FXR {}: {} states, {} nodes, {} configurations, {} actions, {} properties",
        effect.id,
        effect.states.len(),
        effect.nodes.len(),
        effect.configurations.len(),
        effect.actions.len(),
        effect.properties.len()
    );
    for (i, node) in effect.nodes.iter().enumerate() {
        println!(
            "node {i}: {} configs {:?} actions {:?} children {:?}",
            node.tag as u16, node.configurations, node.actions, node.children
        );
    }
    for (i, config) in effect.configurations.iter().enumerate() {
        println!(
            "configuration {i}: {} fields {:?} actions {:?}",
            config.tag as u16, config.fields, config.actions
        );
    }
    for (i, action) in effect.actions.iter().enumerate() {
        println!(
            "action {i}: {} variant {} fixed fields {:?} sequences {:?}",
            action.id(),
            action.variant,
            action.fields,
            action.sequences
        );
        for property in action.properties.iter().flat_map(|r| r.clone()) {
            let p = &effect.properties[property];
            println!(
                "  property {property}: {:#x} evaluator {} modifiers {:?} constant {:?} curve {:?} raw {:?}",
                p.tag,
                p.evaluator,
                p.modifiers,
                p.constant(),
                p.curve(),
                p.values
            );
        }
    }
    Ok(())
}
