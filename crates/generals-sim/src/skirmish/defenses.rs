//! Powered static weapons and the native Patriot assistance callback.
use super::{
    Skirmish, ai_dispatch::WeaponStatus, combat::attack::Goal, movement::distance_squared,
    weapons::Lock,
};

pub struct Assistance {
    pub requester: usize,
    pub assistant: usize,
    pub target: usize,
    pub frame: u64,
}

pub struct AssistedTargeting {
    pub clip_size: i32,
    pub weapon_slot: usize,
    pub lasers: [String; 2],
}

impl super::Entity {
    #[must_use]
    pub fn drawable_position(&self) -> Option<[f32; 3]> {
        self.drawable
            .present
            .then_some(self.drawable.transform_position)
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_assisted_targeting(&mut self, id: usize, ordinal: usize) {
        let names = &self.catalog[self.entities[id].type_index]
            .behavior
            .assisted_targeting[ordinal]
            .lasers;
        let templates = std::array::from_fn(|index| {
            self.catalog
                .iter()
                .position(|unit| unit.name == names[index])
        });
        assert_eq!(
            self.entities[id].behaviors.assisted_targeting.len(),
            ordinal
        );
        self.entities[id]
            .behaviors
            .assisted_targeting
            .push(templates);
    }

    /// `Player::doPowerDisable`: only POWERED objects brown out.
    #[must_use]
    pub fn has_operational_power(&self, id: usize) -> bool {
        self.entities
            .get(id)
            .is_some_and(|actor| actor.disabled_status & (1 << 6) == 0)
    }

    pub(in crate::skirmish) fn request_weapon_assistance_for(
        &mut self,
        id: usize,
        target: usize,
        range: f32,
    ) {
        let actor = &self.entities[id];
        if range == 0.0 {
            return;
        }
        let owner = actor.owner;
        let type_index = actor.type_index;
        let position = actor.position;
        // Player::iterateObjects visits prototype, instance and member order.
        for assistant in self.team_membership.player_objects(owner) {
            let other = &self.entities[assistant];
            let unit = &self.catalog[other.type_index];
            if assistant == id
                || !unit.equivalent_to(&self.catalog[type_index])
                || distance_squared(position, other.position) > range * range
            {
                continue;
            }
            let Some(rules) = unit.behavior.assisted_targeting.first() else {
                continue;
            };
            let slot = rules.weapon_slot;
            let count = u32::from_ne_bytes(rules.clip_size.to_ne_bytes());
            let Some(lasers) = self.entities[assistant]
                .behaviors
                .assisted_targeting
                .first()
                .copied()
            else {
                continue;
            };
            if !self.is_free_to_assist(assistant)
                || self.entities[assistant].ai.registration.is_none()
            {
                continue;
            }
            let allocated = self.allocated_weapon(assistant, slot).is_some();
            let other = &mut self.entities[assistant];
            if allocated && !matches!(other.weapon_set.lock, Lock::Permanent) {
                other.weapon_set.current = slot;
                other.weapon_set.lock = Lock::Temporary;
            }
            // Native assistAttack locks before AI onEnter chooses the weapon;
            // even an ignored AI command still produces its feedback lasers.
            if self.native_ai_command_allowed(assistant) {
                self.ai_attack_goal(assistant, Goal::Object(target), count);
            }
            for (template, from, to) in [(lasers[0], id, assistant), (lasers[1], assistant, target)]
            {
                if let Some(template) = template {
                    self.make_feedback_laser(template, assistant, from, to);
                }
            }
            self.presentation.assistance_events.push(Assistance {
                requester: id,
                assistant,
                target,
                frame: self.tick,
            });
        }
    }

    fn make_feedback_laser(&mut self, template: usize, assistant: usize, from: usize, to: usize) {
        let owner = self.entities[assistant].owner;
        let team = self.team_membership.default_team(owner);
        let laser = self.new_object_on_team(template, owner, Some(team));
        if self.catalog[self.entities[laser].type_index]
            .laser_update
            .is_none()
        {
            self.destroy_maintenance_object(laser);
            return;
        }
        self.set_object_position(laser, self.object_position(from));
        self.init_native_laser(
            laser,
            super::laser_update::Initialization {
                parent: Some(assistant),
                target: Some(to),
                start: Some(self.object_position(from)),
                end: Some(self.object_position(to)),
                bone: String::new(),
                size_delta: 0,
            },
        );
    }

    fn is_free_to_assist(&mut self, id: usize) -> bool {
        if !self.object_can_attack(id) {
            return false;
        }
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        matches!(
            actor.weapon_status(actor.weapon_slot(), unit, now),
            Some(WeaponStatus::Ready)
        )
    }
}

#[cfg(test)]
mod tests;
