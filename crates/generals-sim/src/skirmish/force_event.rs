//! Drain the exact Entity-only `PhysicsBehavior::applyForce` event after its caller.
pub(super) struct PhysicsControl {
    pub(super) turning: bool,
    pub(super) airborne_friction: bool,
    /// Adapter event from applyForce to the registered Physics wake handle.
    pub(super) force_wake: bool,
}

impl PhysicsControl {
    pub(super) const fn new() -> Self {
        Self {
            turning: false,
            airborne_friction: false,
            force_wake: false,
        }
    }
}

impl super::Skirmish {
    pub(super) fn drain_ai_physics_force_wake(&mut self, id: usize) {
        if std::mem::take(&mut self.entities[id].physics_control.force_wake) {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }
}
