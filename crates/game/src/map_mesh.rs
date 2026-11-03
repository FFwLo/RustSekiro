//! Shared source-asset eligibility and lossless static placement merging.
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

pub(crate) fn opaque(alpha: AlphaMode) -> bool {
    matches!(
        alpha,
        AlphaMode::Opaque | AlphaMode::Mask(_) | AlphaMode::AlphaToCoverage
    )
}

/// Reuse indices, UVs and other unchanged attributes for repeated placements of a mesh.
pub(crate) fn merge_meshes<'a>(
    inputs: impl IntoIterator<Item = (&'a Mesh, Transform)>,
) -> Option<Mesh> {
    let mut combined: Option<Mesh> = None;
    let mut scratch: Option<(&Mesh, Mesh)> = None;
    for (source, transform) in inputs {
        if scratch
            .as_ref()
            .is_none_or(|(previous, _)| !std::ptr::eq(*previous, source))
        {
            let mut mesh = source.clone();
            let indices = mesh.indices().map_or_else(
                || (0..mesh.count_vertices() as u32).collect(),
                |indices| indices.iter().map(|i| i as u32).collect(),
            );
            mesh.insert_indices(bevy::mesh::Indices::U32(indices));
            scratch = Some((source, mesh));
        } else {
            let (_, mesh) = scratch.as_mut().unwrap();
            for attribute in [
                Mesh::ATTRIBUTE_POSITION,
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_TANGENT,
            ] {
                match (mesh.attribute_mut(attribute), source.attribute(attribute)) {
                    (
                        Some(VertexAttributeValues::Float32x3(target)),
                        Some(VertexAttributeValues::Float32x3(values)),
                    ) => target.copy_from_slice(values),
                    (
                        Some(VertexAttributeValues::Float32x4(target)),
                        Some(VertexAttributeValues::Float32x4(values)),
                    ) => target.copy_from_slice(values),
                    _ => {}
                }
            }
        }
        let (_, mesh) = scratch.as_mut().unwrap();
        mesh.transform_by(transform);
        if let Some(combined) = &mut combined {
            if let Err(error) = combined.merge(mesh) {
                warn!("static mesh merge layout mismatch: {error:?}");
                return None;
            }
        } else {
            combined = Some(mesh.clone());
        }
    }
    combined
}
