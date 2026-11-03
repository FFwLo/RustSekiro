//! Hirata Father identity, separate from the castle Owl and his bird.

pub fn owns(spec: &crate::enemy::EnemySpec) -> bool {
    spec.chr == "c5060" && spec.npc_param == 50_601_010
}
