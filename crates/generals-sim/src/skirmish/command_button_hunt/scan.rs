//! `scanClosestTarget`: center-distance iteration, bounding-sphere priority metric.
use super::{Command, Skirmish};
use crate::skirmish::{
    ai_goals::COMMAND_FROM_AI, movement::distance_squared, special_ability::PowerType,
};
use num_traits::ToPrimitive;

impl Skirmish {
    pub(super) fn scan_command_button_target(
        &mut self,
        id: usize,
        ordinal: usize,
        button: usize,
    ) -> Option<usize> {
        let definition = &self.command_buttons.buttons[button];
        let enter = matches!(
            definition.command,
            Command::Hijack | Command::CarBomb | Command::Sabotage
        );
        let power = definition
            .power
            .as_ref()
            .map(|power| (power.name.clone(), power.kind));
        if !enter && power.is_none() {
            return None;
        }
        let capture = power
            .as_ref()
            .is_some_and(|(_, kind)| *kind == PowerType::SpecialInfantryCaptureBuilding);
        let relation = u8::from(matches!(definition.command, Command::CarBomb));
        let range = self.catalog[self.entities[id].type_index]
            .behavior
            .command_button_hunts[ordinal]
            .scan_range;
        let position = self.entities[id].position;
        let mut candidates = Vec::new();
        for (_, target) in self.partition.outward_range_rings(position, range) {
            let actor = &self.entities[target];
            let squared = distance_squared(position, actor.position);
            if squared < range * range
                && !actor.is_effectively_dead()
                && actor.is_off_map() == self.entities[id].is_off_map()
                && self.partition_stealth_visible(id, target)
                && (capture || self.object_relationship(id, target) == relation)
            {
                candidates.push((target, squared));
            }
        }
        candidates.reverse();
        candidates.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .expect("finite native world distance")
        });
        if let Some((name, kind)) =
            power.filter(|(name, _)| self.special_power_ordinal(id, name).is_some())
        {
            self.best_hunt_power_target(id, range, &name, kind, candidates)
        } else if enter {
            candidates.into_iter().find_map(|(target, _)| {
                self.hunt_enter_allowed(id, target, button)
                    .then_some(target)
            })
        } else {
            None
        }
    }

    fn best_hunt_power_target(
        &mut self,
        id: usize,
        range: f32,
        name: &str,
        kind: PowerType,
        candidates: Vec<(usize, f32)>,
    ) -> Option<usize> {
        let capture = kind == PowerType::SpecialInfantryCaptureBuilding;
        let vehicle_hack = kind == PowerType::SpecialBlacklotusDisableVehicleHack;
        let info = self.entities[id].ai.attack_priority;
        let distance_scale = self
            .opponent_rules
            .as_ref()
            .expect("loaded native AIData")
            .attack_priority_distance;
        let (mut best, mut effective, mut actual) = (None, 0, 0);
        for (target, squared) in candidates {
            if vehicle_hack && self.object_disabled(target) {
                continue;
            }
            if capture
                && (self.entities[id].owner == self.entities[target].owner
                    || self.object_relationship(id, target) == 2)
            {
                continue;
            }
            if !self.can_special_power_at_object_from(id, name, kind, target, COMMAND_FROM_AI) {
                continue;
            }
            // The native isPlaceExplosive assignments are inside isCaptureBuilding
            // and cannot be true for either charges enum; no mine exclusion runs.
            let radius = self.object_radius(id) + self.object_radius(target);
            let boundary_squared = if radius > 0.0 {
                let distance = squared.sqrt() - radius;
                if distance <= 0.0 {
                    0.0
                } else {
                    distance * distance
                }
            } else {
                squared
            };
            let distance = boundary_squared.sqrt();
            let priority = info.map_or_else(
                || (range - distance).to_i32().expect("native hunt priority"),
                |_| self.native_attack_priority(info, target),
            );
            if priority == 0 {
                continue;
            }
            let modifier = (distance / distance_scale)
                .to_i32()
                .expect("native attack priority modifier");
            let adjusted = priority.wrapping_sub(modifier).max(1);
            if adjusted > effective || adjusted == effective && priority > actual {
                best = Some(target);
                effective = adjusted;
                actual = priority;
            }
        }
        best
    }

    fn hunt_enter_allowed(&self, id: usize, target: usize, button: usize) -> bool {
        if self.entities[target].is_effectively_dead()
            || self.action_target_shrouded(id, target, COMMAND_FROM_AI)
        {
            return false;
        }
        match self.command_buttons.buttons[button].command {
            Command::Hijack => {
                self.object_relationship(id, target) == 0
                    && self.attack_kind(target, "VEHICLE")
                    && !self.attack_kind(target, "AIRCRAFT")
                    && !self.attack_kind(target, "DRONE")
                    && self.hijacker_accepts(id, target)
            }
            Command::Sabotage => {
                self.object_relationship(id, target) == 0 && self.sabotage_accepts(id, target)
            }
            Command::CarBomb => self.car_bomb_accepts(id, target),
            _ => false,
        }
    }
}
