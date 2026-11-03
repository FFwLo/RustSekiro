//! `msb_enemies <map id> [model prefix]`: lists the enemy parts a map layout places, with their
//! NpcParam and NpcThinkParam rows. Run from the repo root, for example
//! `cargo run -p sekiro-sim --example msb_enemies -- m11_01_00_00 c50`.
use sekiro_formats::msb::{self, PartKind};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map = args.get(1).map_or("m11_00_00_00", String::as_str);
    let prefix = args.get(2).map_or("", String::as_str);
    let path = format!("cache/raw/map/mapstudio/{map}.msb");
    let msb = msb::parse(&std::fs::read(&path).expect("map layout")).expect("parse");
    for part in &msb.parts {
        if part.kind != PartKind::Enemy {
            continue;
        }
        let model = msb.model(part).map_or("?", |m| m.name.as_str());
        if !model.starts_with(prefix) {
            continue;
        }
        let Some(e) = &part.enemy else { continue };
        println!(
            "{:<16} {model} npc={} think={} entity={} pos=[{:.2}, {:.2}, {:.2}] rot_y={:.1} \
             init={} event_flag={} collision={}",
            part.name,
            e.npc_param_id,
            e.think_param_id,
            part.entity_id,
            part.position[0],
            part.position[1],
            part.position[2],
            part.rotation[1],
            e.chara_init_id,
            e.event_flag_id,
            e.collision_part,
        );
    }
}
