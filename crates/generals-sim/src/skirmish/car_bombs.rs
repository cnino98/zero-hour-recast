//! `ConvertToCarBombCrateCollide` and its explicit `AI_ENTER` goal, SH 2bfaea60.
use super::{Entity, Order, Skirmish};

#[cfg(test)]
mod tests;

pub struct Rules {
    pub effect: String,
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
}

pub struct Event {
    pub frame: u64,
    pub source: usize,
    pub target: usize,
}

pub(super) struct State {
    pub vision: f32,
    pub shroud: f32,
}

impl Entity {
    #[must_use]
    pub const fn car_bomb(&self) -> bool {
        self.car_bomb.is_some()
    }
}

impl Skirmish {
    #[must_use]
    pub fn can_make_car_bomb(&self, id: usize, target: usize) -> bool {
        let Some(actor) = self.entities.get(id) else {
            return false;
        };
        actor.complete()
            && !actor.masked()
            && !actor.hacked()
            && !actor.paralyzed()
            && self.car_bomb_accepts(id, target)
            && self.maintenance_target_visible(actor.owner, target)
    }

    /// Uses the original enter-object command; conversion occurs at contact.
    /// # Errors
    /// Rejects unavailable drivers, hidden targets, and invalid car-bomb recipients.
    pub fn make_car_bomb(&mut self, id: usize, target: usize) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if !self.can_make_car_bomb(id, target) {
            return Err("Choose a visible vehicle that can become a car bomb");
        }
        self.begin_container_enter(id, target);
        Ok(())
    }

    pub(super) fn car_bomb_accepts(&self, id: usize, target: usize) -> bool {
        let (Some(actor), Some(car)) = (self.entities.get(id), self.entities.get(target)) else {
            return false;
        };
        let driver = &self.catalog[actor.type_index];
        let Some(rules) = &driver.behavior.car_bomb_converter else {
            return false;
        };
        let unit = &self.catalog[car.type_index];
        id != target
            && actor.alive()
            && car.alive()
            && !car.masked()
            && (car.owner != self.neutral_owner() || self.civilian_objects.contains(&target))
            && !car.car_bomb()
            && self.object_position(id)[2] <= self.ground_height(actor.position)
            && rules
                .required
                .iter()
                .all(|kind| unit.kind_of.contains(kind))
            && !rules
                .forbidden
                .iter()
                .any(|kind| unit.kind_of.contains(kind))
            && !unit
                .kind_of
                .iter()
                .any(|kind| matches!(kind.as_str(), "AIRCRAFT" | "BOAT" | "PARACHUTE"))
            && unit
                .weapon_set_rules
                .replacements
                .iter()
                .any(|set| set.flags == 8)
            && car
                .weapon_set
                .template_set
                .is_none_or(|set| unit.weapon_set_rules.replacements[set].flags & 8 == 0)
    }

    pub(super) fn car_bomb_contact(&mut self, id: usize, target: usize) -> bool {
        if !matches!(self.entities[id].ai.order, Order::Enter(goal) if goal == target)
            || !self.car_bomb_accepts(id, target)
        {
            return false;
        }
        let actor = &self.entities[id];
        let driver = &self.catalog[actor.type_index];
        let owner = actor.owner;
        let level = actor.veterancy;
        let sight = State {
            vision: actor.vision_range(driver),
            shroud: actor.shroud_range(driver),
        };
        let effect = driver
            .behavior
            .car_bomb_converter
            .as_ref()
            .expect("converter")
            .effect
            .clone();
        self.object_fx(target, effect);
        self.entities[target].car_bomb = Some(sight);
        self.replace_weapon_set(target);
        self.refund_production_on_death(target);
        self.transfer_maintenance_owner(target, owner);
        self.civilian_objects.remove(&target);
        self.set_veterancy(target, level);
        self.transfer_crew_name(id, target);
        self.observe_visibility(target);
        self.presentation.car_bomb_events.push(Event {
            frame: self.tick,
            source: id,
            target,
        });
        // CrateCollide destroys the driver; this must not run OnDie or detonate him.
        self.destroy_maintenance_object(id);
        self.entities[id].maintenance.masked = true;
        self.observe_visibility(id);
        true
    }
}
