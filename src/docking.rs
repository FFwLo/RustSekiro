//! Wall hug (cover) and ledge hang: Wolf docked to a map edge (`map::LedgeEdges`, kind 3 hug /
//! kind 2 hang). HKS c0000_transition.lua BEH_A_COVER_START / GROUND_HANG_START /
//! AIR_HANG_START pick the start; the c0000.hkx CustomDockingGenerator nodes hold Wolf at the
//! edge with the behavior variables DockingTranslationOffset_Cover (0, 0, 0.7), _Hang
//! (0, 1.6, -0.7), _HangStart (0, 0, -0.4), _HangBackStart (0, 0, 0.4), _HangLeftStart
//! (-0.4, 0, 0) and _HangRightStart (0.4, 0, 0) (their initial values, hkbBehaviorGraphData
//! variableInitialValues).

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    /// Which side of each hug / hang edge of the gate map the wall is on: rays at 1 m (hug) /
    /// below the edge (hang) out to both sides of the a-b segment's middle, and where c lies.
    #[test]
    #[ignore]
    fn edge_wall_side_probe() {
        let dir = crate::paths::extracted();
        let id = "m11_01_00_00";
        let Some(t) = std::fs::read(dir.join(format!("map_{id}.hit"))).ok().and_then(|d| crate::map::Terrain::from_hit(&d)) else { return };
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("map_{id}.json"))).unwrap()).unwrap();
        for e in crate::map::load_ledges(&json) {
            let mid = (e.a + e.b) * 0.5;
            let along = (e.b - e.a).with_y(0.0).normalize();
            // Left of a->b seen from above (+Y up).
            let left = Vec3::Y.cross(along);
            let probe = |h: f32| {
                let o = mid + Vec3::Y * h;
                (t.raycast(o, left, 1.5), t.raycast(o, -left, 1.5))
            };
            let floor = t.floor(mid.x, mid.z, mid.y + 0.5);
            let below = t.floor(mid.x + left.x * 0.5, mid.z + left.z * 0.5, mid.y - 0.1).map(|f| mid.y - f);
            let below_r = t.floor(mid.x - left.x * 0.5, mid.z - left.z * 0.5, mid.y - 0.1).map(|f| mid.y - f);
            println!(
                "kind {} var {} len {:.2} mid {:.2?} floor {:?} c.left {:.2} c.y {:.2} | ray L/R at +1 {:?} at +0.3 {:?} at -0.5 {:?} | drop L {:?} R {:?}",
                e.kind,
                e.variation,
                e.a.distance(e.b),
                mid,
                floor.map(|f| mid.y - f),
                (e.c - mid).dot(left),
                e.c.y - mid.y,
                probe(1.0),
                probe(0.3),
                probe(-0.5),
                below,
                below_r
            );
        }
    }
}
