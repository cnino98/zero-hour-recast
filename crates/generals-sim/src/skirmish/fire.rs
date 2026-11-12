//! Original incendiary damage, burn timers and shared firestorm rules.
pub mod collide;
mod creation;
pub mod firestorm;
pub mod flammable;
pub mod geometry;
pub mod history;
mod objects;
pub mod weapon_update;
pub mod when_damaged;
pub(in crate::skirmish) use creation::Impact;
#[cfg(test)]
mod tests;
mod wander;

pub struct Rules {
    pub body: Body,
    pub ambient: String,
    pub lifetime: [u32; 2],
    pub initial_delay: u32,
    pub storm: Option<Storm>,
}

pub enum Body {
    Inactive,
    Immortal,
}

pub struct Storm {
    pub geometry: geometry::Rules,
    pub interval: f32,
    pub damage: f32,
    pub height: f32,
    pub particles: Vec<String>,
    pub particle_height: f32,
    pub effect: String,
    pub scorch: f32,
}

pub struct Weapon {
    pub impact: Option<Creation>,
    pub death: Option<Creation>,
    pub historic: Option<Historic>,
}

pub struct Historic {
    pub rules: history::Rules,
    pub creation: Creation,
}

#[derive(Clone)]
pub struct Creation {
    pub names: Vec<String>,
    pub count: u32,
    pub disposition: Disposition,
}

#[derive(Clone, Copy)]
pub enum Disposition {
    GroundAligned,
    ExistingWithVelocity,
}

pub struct State {
    created: u64,
    height: f32,
    dimensions: [f32; 3],
    started: Option<u64>,
    reversed: Option<u64>,
}

impl crate::skirmish::Entity {
    pub(in crate::skirmish) const fn set_fire_height(&mut self, height: f32) {
        if let Some(state) = &mut self.fire {
            state.height = height;
        }
    }

    pub(in crate::skirmish) fn fire_physics_asleep(&self, frame: u64) -> bool {
        self.fire
            .as_ref()
            .is_some_and(|state| frame <= state.created)
    }
    #[must_use]
    pub fn fire_started(&self) -> Option<u64> {
        self.fire.as_ref()?.started
    }

    #[must_use]
    pub fn fire_reversed(&self) -> Option<u64> {
        self.fire.as_ref()?.reversed
    }

    #[must_use]
    pub fn fire_radius(&self) -> Option<f32> {
        Some(self.fire.as_ref()?.dimensions[1])
    }

    pub(in crate::skirmish) fn fire_height(&self) -> Option<f32> {
        Some(self.fire.as_ref()?.height)
    }
}

fn frame(tick: u64) -> u32 {
    u32::try_from(tick).expect("native simulation frame fits unsigned 32-bit")
}
