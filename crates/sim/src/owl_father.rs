//! Hirata Father identity, separate from the castle Owl and his bird.

pub fn owns(spec: &crate::enemy::EnemySpec) -> bool {
    spec.chr == "c5060" && spec.npc_param == 50_601_010
}

pub fn bird_entity(spec: &crate::enemy::EnemySpec) -> i32 {
    if spec.think == 50_630_000 {
        1_000_991
    } else {
        1_000_901
    }
}

/// The c5060 external move sheet describes castle Owl, not either Hirata battle goal.
pub fn keep_lua(spec: &crate::enemy::EnemySpec, brain: &str, log: bool) -> bool {
    owns(spec) && brain == "scripted" && !log
}
