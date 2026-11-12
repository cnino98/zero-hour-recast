//! Native jet death module state and physical OCL phases.
use super::physical_ocl;

mod runtime;
#[cfg(test)]
pub(in crate::skirmish) mod tests;

pub(in crate::skirmish) struct NativeState {
    roll: f32,
    dead_frame: u32,
    ground_frame: u32,
}

impl NativeState {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            roll: 0.0,
            dead_frame: 0,
            ground_frame: 0,
        }
    }
}

pub struct Jet {
    /// On-ground, initial, secondary, hit-ground, final, in native callback order.
    pub phases: [Phase; 5],
    pub secondary_delay: u32,
    pub final_delay: u32,
    pub roll_rate: f32,
    pub roll_delta: f32,
    pub pitch_rate: f32,
    pub fall_speed: f32,
    pub loop_sound: String,
}

#[derive(Clone)]
pub struct Phase {
    pub effect: String,
    pub objects: Option<physical_ocl::List>,
}
