//! Independently switched event collision, sharing the arena's query world.
use super::*;

impl CollisionWorld {
    /// Adds an event part that `who` ([`crate::hit_filter::mask`] bits of its hit filter)
    /// collides with while it is on.
    pub fn append_event_part(&mut self, entity: i32, mesh: &CollisionMesh, who: u8, on: bool) {
        let range = self.append(mesh, who);
        self.add_event_part(entity, range, who, on);
    }

    pub fn set_event_part(&self, entity: i32, on: bool) {
        for (id, _, _, state) in &self.event_parts {
            if *id == entity {
                state.store(on, Ordering::Relaxed);
            }
        }
    }
}
