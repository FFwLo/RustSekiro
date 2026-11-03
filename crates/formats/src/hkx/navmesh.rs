//! Havok AI navigation meshes (`hkaiNavMesh`): one per file of a map's `nvmhktbnd`
//! (`n<area>_<part>.hkx`), in the map's own space, which the MSB's map offset event places in the
//! world (the same transform its collision parts carry).

use super::tagfile::TagFile;
use crate::Result;
use crate::reader::bail;

/// A navigation mesh's walkable faces.
#[derive(Debug, Clone, Default)]
pub struct NavMesh {
    /// Vertex positions, in FromSoftware's (left-handed) map space.
    pub vertices: Vec<[f32; 3]>,
    /// Each face's corners as vertex indices, in edge order. Faces are convex polygons.
    pub faces: Vec<Vec<u32>>,
}

impl NavMesh {
    /// The first `hkaiNavMesh` of a tagfile.
    pub fn from_tagfile(file: &TagFile) -> Result<Self> {
        let nav = file.first("hkaiNavMesh")?;
        let raw = nav.get("vertices")?.array_bytes()?;
        if raw.len() % 16 != 0 {
            return bail(format!("navmesh vertex array of {} bytes", raw.len()));
        }
        // hkVector4: x, y, z, w.
        let vertices: Vec<[f32; 3]> = raw
            .as_chunks::<16>()
            .0
            .iter()
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap());
                [f(0), f(1), f(2)]
            })
            .collect();
        let edges = nav
            .get("edges")?
            .elements()?
            .iter()
            .map(|e| e.get("a")?.int())
            .collect::<Result<Vec<i64>>>()?;
        let faces = nav
            .get("faces")?
            .elements()?
            .iter()
            .map(|f| {
                let start = f.get("startEdgeIndex")?.int()?;
                let count = f.get("numEdges")?.int()?;
                let (Ok(start), Ok(count)) = (usize::try_from(start), usize::try_from(count))
                else {
                    return bail(format!("navmesh face edges {start} + {count}"));
                };
                let Some(starts) = edges.get(start..start + count) else {
                    return bail(format!(
                        "navmesh face edges {start} + {count} past {}",
                        edges.len()
                    ));
                };
                starts
                    .iter()
                    .map(|&a| match u32::try_from(a) {
                        Ok(a) if (a as usize) < vertices.len() => Ok(a),
                        _ => bail(format!("navmesh edge vertex {a} of {}", vertices.len())),
                    })
                    .collect()
            })
            .collect::<Result<Vec<Vec<u32>>>>()?;
        Ok(Self { vertices, faces })
    }

    /// Parses a navmesh `.hkx` (a tagfile without a compendium).
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_tagfile(&TagFile::parse(data, None)?)
    }
}
