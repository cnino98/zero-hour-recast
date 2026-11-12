//! Helix command entry uses the authored native ability and shared timer.
use super::Skirmish;
use crate::skirmish::special_ability::PowerType;

impl Skirmish {
    fn helix_power(&self, id: usize) -> Option<&str> {
        self.entities.get(id)?;
        let ordinal = self.first_special_ability(id, PowerType::SpecialHelixNapalmBomb)?;
        self.catalog[self.entities[id].type_index]
            .behavior
            .special_abilities[ordinal]
            .power
            .as_ref()
            .map(|power| power.name.as_str())
    }

    pub fn helix_bomb_ready_frame(&mut self, id: usize) -> Option<u64> {
        let power = self.helix_power(id)?.to_owned();
        self.special_power_ready_frame(id, &power).map(u64::from)
    }

    pub fn helix_bomb_ready(&mut self, id: usize) -> bool {
        let eligible = self.entities.get(id).is_some_and(|actor| {
            actor.complete() && !self.object_disabled(id) && actor.contained_by().is_none()
        });
        if !eligible {
            return false;
        }
        let Some(power) = self.helix_power(id).map(str::to_owned) else {
            return false;
        };
        self.special_power_ready(id, &power)
    }

    pub fn helix_bomb_percent(&mut self, id: usize) -> Option<f32> {
        let power = self.helix_power(id)?.to_owned();
        Some(self.special_power_percent(id, &power))
    }

    /// Orders the authored persistent napalm ability at a world position.
    /// # Errors
    /// Rejects unavailable or recharging powers and off-map positions.
    pub fn helix_bomb(&mut self, id: usize, target: [f32; 2]) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.helix_bomb_ready(id) || self.cell_at(target).is_none() {
            return Err("Helix napalm bomb is unavailable");
        }
        let Some(power) = self.helix_power(id).map(str::to_owned) else {
            return Err("Helix napalm ability is unavailable");
        };
        let position = [target[0], target[1], self.ground_height(target)];
        if !self.special_power_at_location(id, &power, position, false) {
            return Err("Helix napalm ability rejected the command");
        }
        Ok(())
    }
}
