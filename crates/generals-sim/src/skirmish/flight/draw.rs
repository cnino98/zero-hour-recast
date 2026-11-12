//! Original locomotor coefficients and `PhysicsBehavior` input for drawable banking.
use super::super::{Entity, UnitType};

pub struct Rules {
    pub stiffness: [f32; 2],
    pub damping: [f32; 2],
    pub axial_damping: f32,
    pub velocity: [f32; 3],
    pub acceleration: [f32; 2],
    pub pitch_limits: [f32; 2],
    pub rudder: [f32; 2],
    pub elevator: [f32; 2],
}

pub struct Motion<'a> {
    pub rules: &'a Rules,
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub motive: bool,
}

impl Entity {
    #[must_use]
    pub fn aircraft_draw_motion<'a>(&self, unit: &'a UnitType, frame: u64) -> Option<Motion<'a>> {
        if self.aircraft.is_none() && !unit.is_helix_container() {
            return None;
        }
        let rules = &self.active_locomotor(unit).flight.as_ref()?.draw;
        let state = self.flight.as_ref()?;
        Some(Motion {
            rules,
            velocity: state.velocity,
            acceleration: state.previous_acceleration,
            motive: state.motive_until > frame,
        })
    }
}
