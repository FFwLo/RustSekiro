//! Character draw masks: which `#NN#` variant mesh groups of an exported model are visible.
//!
//! Enemy GLBs contain every variant mesh (hair, hats, sheathed or drawn swords). Their scene
//! extras carry `drawMask` (NpcParam `modelDispMask0..31`) and `combatDrawMask` (with the
//! weapon-draw TAE event applied). A [`DrawMask`] on the entity that spawns the scene picks one,
//! and TAE `ChangeChrDrawMask` events can change it at runtime with [`DrawMask::apply_event`].
//! Mesh primitives are matched by their material name, `m<mesh> <flver material> | ...`, where a
//! FLVER material named `#NN#...` belongs to group NN; meshes without a group always show.

use bevy::gltf::{GltfMaterialName, GltfSceneExtras};
use bevy::prelude::*;

pub const GROUPS: usize = 32;

/// Which initial mask to take from the scene extras.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSource {
    /// `drawMask`: the NpcParam look (weapons sheathed).
    Default,
    /// `combatDrawMask`: weapons drawn.
    Combat,
}

#[derive(Component, Debug)]
pub struct DrawMask {
    pub source: MaskSource,
    groups: Option<[bool; GROUPS]>,
    dirty: bool,
}

impl DrawMask {
    pub fn with_groups(groups: [bool; GROUPS]) -> Self {
        Self {
            source: MaskSource::Combat,
            groups: Some(groups),
            dirty: true,
        }
    }

    pub fn new(source: MaskSource) -> Self {
        Self {
            source,
            groups: None,
            dirty: true,
        }
    }

    /// Applies a TAE `ChangeChrDrawMask` event: per group 0 hides, 1 shows, anything else keeps.
    #[allow(dead_code)]
    pub fn apply_event(&mut self, values: &[u8]) {
        let Some(groups) = self.groups.as_mut() else {
            return;
        };
        for (g, v) in groups.iter_mut().zip(values) {
            match v {
                0 => *g = false,
                1 => *g = true,
                _ => {}
            }
        }
        self.dirty = true;
    }
}

/// The `#NN#` group of a material name exported as `m<mesh> <flver material> | ...`.
fn group_of(material: &str) -> Option<usize> {
    let flver_name = material.split_once(' ')?.1;
    let rest = flver_name.strip_prefix('#')?;
    rest[..rest.find('#')?].parse().ok()
}

fn parse_mask(extras: &str, key: &str) -> Option<[bool; GROUPS]> {
    let value: serde_json::Value = serde_json::from_str(extras).ok()?;
    let list = value.get(key)?.as_array()?;
    let mut out = [false; GROUPS];
    for (o, v) in out.iter_mut().zip(list) {
        *o = v.as_bool()?;
    }
    Some(out)
}

/// Resolves masks from the scene extras once the scene exists, and updates mesh visibility
/// whenever a mask changes.
pub fn apply_draw_masks(
    mut roots: Query<(Entity, &mut DrawMask)>,
    children: Query<&Children>,
    extras: Query<&GltfSceneExtras>,
    mut meshes: Query<(&GltfMaterialName, &mut Visibility)>,
) {
    for (root, mut mask) in &mut roots {
        if mask.groups.is_none() {
            let key = match mask.source {
                MaskSource::Default => "drawMask",
                MaskSource::Combat => "combatDrawMask",
            };
            let Some(found) = children
                .iter_descendants(root)
                .find_map(|e| extras.get(e).ok())
            else {
                continue; // scene not spawned yet
            };
            match parse_mask(&found.value, key) {
                Some(groups) => mask.groups = Some(groups),
                None => {
                    // No mask in this model (the player exports only what shows).
                    mask.groups = Some([true; GROUPS]);
                }
            }
        }
        if !mask.dirty {
            continue;
        }
        let groups = mask.groups.expect("resolved above");
        let mut seen = false;
        for e in children.iter_descendants(root) {
            if let Ok((name, mut vis)) = meshes.get_mut(e) {
                seen = true;
                let show = group_of(name).is_none_or(|g| groups.get(g).copied().unwrap_or(false));
                *vis = if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
        // Mesh entities can appear a frame after the extras; retry until they do.
        mask.dirty = !seen;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_from_material_names() {
        assert_eq!(
            group_of("m0 #00# | tsuka | c1010_katana_rope_e_decal"),
            Some(0)
        );
        assert_eq!(
            group_of("m42 #28#kimono_cloth | c1010_sode_cloth | x"),
            Some(28)
        );
        assert_eq!(
            group_of("m26 c9500_body | body001 | c1019_body_merge_sss"),
            None
        );
    }
}
