//! Hall bell interaction geometry from the placed object and installed action parameters.

use crate::{Character, InputFrame};
use glam::Mat4;
use serde_json::Value;
use std::path::Path;

pub struct Bell {
    pub point: [f32; 3],
    /// Its ActionButtonParam cylinder about `point`.
    pub reach: crate::items::Reach,
    pub player_animation: i32,
    pressed: bool,
    pub activated: bool,
    pub available: bool,
}

fn row(cache: &Path, table: &str, id: i32) -> Result<Value, String> {
    let path = cache.join(format!("params/{table}.json"));
    let json: Value = serde_json::from_slice(
        &std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    json["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["id"].as_i64() == Some(i64::from(id)))
        .cloned()
        .ok_or_else(|| format!("missing {table} row {id}"))
}

fn node_world(model: &sekiro_formats::flver::Flver, index: i16) -> Option<Mat4> {
    if index < 0 {
        return Some(Mat4::IDENTITY);
    }
    let mut chain = Vec::new();
    let mut at = index;
    while at >= 0 {
        let node = model.nodes.get(at as usize)?;
        chain.push(node.local_matrix());
        if chain.len() > model.nodes.len() {
            return None;
        }
        at = node.parent;
    }
    Some(
        chain
            .into_iter()
            .rev()
            .fold(Mat4::IDENTITY, |world, local| world * local),
    )
}

impl Bell {
    pub fn load(cache: &Path) -> Result<Self, String> {
        let raw = std::fs::read(cache.join("raw/map/MapStudio/m20_00_00_00.msb"))
            .map_err(|e| e.to_string())?;
        let map = sekiro_formats::msb::parse(&raw).map_err(|e| e.to_string())?;
        let part = map
            .parts
            .iter()
            .find(|p| p.entity_id == 2_001_800)
            .ok_or("missing MSB bell object 2001800")?;
        let model = map.model(part).ok_or("bell has no model")?;
        let path = cache.join(format!(
            "raw/obj/{}.objbnd.d/{}.flver",
            model.name, model.name
        ));
        let flver = sekiro_formats::flver::parse(&std::fs::read(&path).map_err(|e| {
            format!(
                "missing bell FLVER {}: {e}; extract with sekiro-extract unpack obj",
                path.display()
            )
        })?)
        .map_err(|e| e.to_string())?;
        let action = row(cache, "ObjActParam", 205101)?;
        let button = row(
            cache,
            "ActionButtonParam",
            action["actionButtonParamId"]
                .as_i64()
                .ok_or("bell has no action button")? as i32,
        )?;
        let dummy = flver
            .dummies
            .iter()
            .find(|d| i64::from(d.reference_id) == button["dummyPoly1"].as_i64().unwrap_or(-1))
            .ok_or("bell action dummy is missing")?;
        let local = node_world(&flver, dummy.parent_bone)
            .ok_or("bell dummy parent chain is invalid")?
            .transform_point3(dummy.position.into());
        let point = part.matrix().transform_point3(local);
        Ok(Self {
            point: [-point.x, point.y, point.z],
            reach: crate::items::Reach::from_button(&button).ok_or("bell reach absent")?,
            player_animation: action["playerAnimId"]
                .as_i64()
                .ok_or("bell player animation absent")? as i32,
            pressed: false,
            activated: false,
            available: false,
        })
    }

    pub fn step(&mut self, player: &Character, input: &InputFrame, player_height: f32) {
        let pressed = input.buttons.action && !self.pressed;
        self.pressed = input.buttons.action;
        self.available = self
            .reach
            .distance(self.point, player.body.position, player_height)
            .is_some();
        self.activated = pressed && self.available;
    }
}
