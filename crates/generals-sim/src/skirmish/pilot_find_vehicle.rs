//! `PilotFindVehicleUpdate.cpp` 79–185, SH 2bfaea60.
use super::{Skirmish, ai_goals::COMMAND_FROM_AI, movement::distance_squared};

pub struct Rules {
    pub scan_frames: u32,
    pub scan_range: f32,
    pub min_health: f32,
}

pub(super) struct State {
    did_move_to_base: bool,
}

pub(super) fn validate(unit: &super::UnitType) -> Result<(), String> {
    if !unit.behavior.pilot_find_vehicle.is_empty() {
        if unit.behavior.veterancy_crates.is_empty() {
            return Err(format!(
                "{}: PilotFindVehicleUpdate requires its actual veterancy collide consumer",
                unit.name
            ));
        }
        if unit.behavior_modules.iter().any(|module| {
            module.interfaces & 0x10 != 0
                && !matches!(
                    module.family.as_str(),
                    "VeterancyCrateCollide" | "PhysicsBehavior" | "SquishCollide"
                )
        }) {
            return Err(format!(
                "{}: PilotFindVehicleUpdate has an untranslated collide admission query",
                unit.name
            ));
        }
        if unit.behavior.pilot_find_vehicle.iter().any(|rule| {
            !rule.scan_range.is_finite() || rule.scan_range < 0.0 || !rule.min_health.is_finite()
        }) {
            return Err(format!(
                "{}: PilotFindVehicleUpdate requires represented finite scan values",
                unit.name
            ));
        }
    }
    Ok(())
}

impl Skirmish {
    pub(super) fn construct_pilot_find_vehicle(&mut self, id: usize, ordinal: usize) {
        assert_eq!(
            self.entities[id].behaviors.pilot_find_vehicle.len(),
            ordinal
        );
        self.entities[id].behaviors.pilot_find_vehicle.push(State {
            did_move_to_base: false,
        });
    }

    pub(super) fn update_pilot_find_vehicle(&mut self, id: usize, ordinal: usize) -> i32 {
        let actor = &self.entities[id];
        if !self.computer_player(actor.owner) || actor.ai.registration.is_none() {
            return i32::from_ne_bytes(super::module_updates::FOREVER.to_ne_bytes());
        }
        let sleep = i32::from_ne_bytes(
            self.catalog[actor.type_index].behavior.pilot_find_vehicle[ordinal]
                .scan_frames
                .to_ne_bytes(),
        );
        if !actor.ai_idle() {
            return sleep;
        }
        if let Some(target) = self.scan_pilot_vehicle(id, ordinal) {
            self.native_enter_from(id, target, COMMAND_FROM_AI);
            self.entities[id].behaviors.pilot_find_vehicle[ordinal].did_move_to_base = false;
        } else if !self.entities[id].behaviors.pilot_find_vehicle[ordinal].did_move_to_base
            && let Some(position) = self.pilot_ai_base_center(self.entities[id].owner)
        {
            self.native_move_from(id, position, COMMAND_FROM_AI);
            self.entities[id].behaviors.pilot_find_vehicle[ordinal].did_move_to_base = true;
        }
        sleep
    }

    fn scan_pilot_vehicle(&self, id: usize, ordinal: usize) -> Option<usize> {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.pilot_find_vehicle[ordinal];
        let mut candidates = Vec::new();
        for target in self
            .partition
            .fastest_range_objects(actor.position, rules.scan_range)
        {
            let other = &self.entities[target];
            let squared = distance_squared(actor.position, other.position);
            if squared < rules.scan_range * rules.scan_range
                && other.owner == actor.owner
                && other.is_off_map() == actor.is_off_map()
                && !other.is_effectively_dead()
                && self.catalog[other.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "VEHICLE")
            {
                candidates.push((target, squared));
            }
        }
        // SimpleObjectIterator's merge sort is stable, including equal distances.
        candidates.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .expect("finite native center distance")
        });
        candidates.into_iter().find_map(|(target, _)| {
            let other = &self.entities[target];
            let body = &self.catalog[other.type_index].body_kind;
            ((body.is_active() || matches!(body, super::BodyKind::Inactive))
                && other.health >= other.max_health * rules.min_health
                && self.catalog[actor.type_index]
                    .behavior
                    .veterancy_crates
                    .iter()
                    .enumerate()
                    .any(|(ordinal, _)| self.veterancy_crate_valid(id, ordinal, target)))
            .then_some(target)
        })
    }
}
