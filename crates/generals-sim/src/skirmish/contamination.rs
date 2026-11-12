//! `FireOCLAfterWeaponCooldownUpdate`: shot history, upgrade admission and OCL lifetime.
mod firing;

use super::{
    Skirmish, hazards,
    progression::{integer_real, real_unsigned},
};

pub struct Rules {
    pub slot: usize,
    pub minimum_shots: u32,
    pub lifetime_per_second: u32,
    pub maximum_lifetime: u32,
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub field: hazards::Rules,
}

pub(super) struct State {
    valid: bool,
    shots: u32,
    start: u32,
}

impl State {
    const fn new() -> Self {
        Self {
            valid: false,
            shots: 0,
            start: 0,
        }
    }

    fn update(&mut self, rules: &Rules, now: u32, weapon: &Observation) -> Option<u32> {
        let valid = weapon.selected && weapon.upgrades;
        // An upgrade rejection suppresses the switch-away firing path only when
        // this module's slot is still selected (native short-circuit ordering).
        let can_fire = !weapon.selected || weapon.upgrades;
        let mut lifetime = None;
        if valid && weapon.last_shot == Some(now.wrapping_sub(1)) {
            self.shots = self.shots.wrapping_add(1);
            if self.shots == 1 {
                self.start = now;
            }
        } else if self.shots >= rules.minimum_shots
            && ((valid && weapon.possible_next_shot < now)
                || (!valid && can_fire && weapon.present))
        {
            let frames = now.wrapping_sub(self.start);
            let mut seconds = integer_real(frames) * (1.0 / 30.0);
            seconds *= integer_real(rules.lifetime_per_second) * 0.001;
            lifetime = Some(real_unsigned(seconds * 30.0).min(rules.maximum_lifetime));
            self.reset();
        }
        // The first validity edge intentionally discards a shot counted above.
        if valid != self.valid {
            self.valid = valid;
            self.reset();
        }
        lifetime
    }

    const fn reset(&mut self) {
        self.shots = 0;
        self.start = 0;
    }
}

struct Observation {
    selected: bool,
    present: bool,
    upgrades: bool,
    last_shot: Option<u32>,
    possible_next_shot: u32,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_contamination_module(
        &mut self,
        id: usize,
        ordinal: usize,
    ) {
        assert_eq!(self.entities[id].behaviors.contamination.len(), ordinal);
        self.entities[id].behaviors.contamination.push(State::new());
    }

    pub(in crate::skirmish) fn update_contamination_module(
        &mut self,
        id: usize,
        index: usize,
    ) -> i32 {
        let type_index = self.entities[id].type_index;
        let now = self.update_frame();
        let rules = &self.catalog[type_index].behavior.contamination[index];
        let actor = &self.entities[id];
        let has_upgrade = |name: &String| {
            self.object_has_upgrade(id, name) || self.has_upgrade(actor.owner, name)
        };
        let upgrades = (rules.triggers.is_empty()
            || if rules.requires_all {
                rules.triggers.iter().all(has_upgrade)
            } else {
                rules.triggers.iter().any(has_upgrade)
            })
            && !rules.conflicts.iter().any(has_upgrade);
        let state = &actor.weapon_set.weapons[rules.slot];
        let observation = Observation {
            selected: actor.weapon_slot() == rules.slot
                && self.allocated_weapon(id, rules.slot).is_some(),
            present: self.catalog[type_index]
                .weapon_template(actor.weapon_index(rules.slot))
                .is_some(),
            upgrades,
            last_shot: state
                .last_shot
                .map(|frame| u32::try_from(frame).expect("native frame")),
            possible_next_shot: u32::try_from(state.possible_next_shot).expect("native frame"),
        };
        if let Some(lifetime) =
            self.entities[id].behaviors.contamination[index].update(rules, now, &observation)
        {
            self.create_hazard(
                id,
                hazards::Origin::Cooldown(index),
                self.entities[id].owner,
                self.object_position(id),
                self.tick,
                Some(lifetime),
            );
        }
        1
    }
}

#[cfg(test)]
mod tests;
