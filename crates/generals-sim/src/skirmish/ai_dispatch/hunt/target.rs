//! `AI::findClosestEnemy(CAN_ATTACK)`, SH AI.cpp599–746 / `FASTER_GCO`.
use crate::skirmish::{CanAttackResult, Skirmish, ai_goals::COMMAND_FROM_AI};
use num_traits::ToPrimitive;

const RANGE: f32 = 9999.9;

impl Skirmish {
    pub(super) fn find_hunt_enemy(&mut self, source: usize, info: Option<usize>) -> Option<usize> {
        if !self.object_can_attack(source) {
            return None;
        }
        let position = self.entities[source].position;
        if self.native_default_attack_priority(info) {
            let mut closest = None;
            let mut best_squared = RANGE * RANGE;
            let mut last_ring = f64::INFINITY;
            for (ring, target) in self.partition.outward_range_rings(position, RANGE) {
                if ring > last_ring {
                    break;
                }
                if target == source {
                    continue;
                }
                let distance = self.hunt_distance_squared(source, target);
                if distance < best_squared && self.hunt_enemy_allowed(source, target) {
                    closest = Some(target);
                    best_squared = distance;
                    last_ring = ring;
                }
            }
            return closest;
        }
        // getClosestObjects calls filters before prepending each accepted node.
        let mut candidates = Vec::new();
        for (_, target) in self.partition.outward_range_rings(position, RANGE) {
            if target == source {
                continue;
            }
            let squared = self.hunt_distance_squared(source, target);
            if squared < RANGE * RANGE && self.hunt_enemy_allowed(source, target) {
                candidates.push((target, squared));
            }
        }
        candidates.reverse();
        candidates.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .expect("finite native world distance")
        });
        let distance_scale = self
            .opponent_rules
            .as_ref()
            .expect("loaded native AIData")
            .attack_priority_distance;
        let mut best = None;
        let mut effective = 0_i32;
        let mut actual = 0;
        for (target, squared) in candidates {
            let mut priority = self.native_attack_priority(info, target);
            if priority == 0 {
                continue;
            }
            // iterateContained(..., false) uses the actual virtual list, not
            // nested enclosed objects and not only passengers allowed to fire.
            for &rider in self.passengers(target) {
                priority = priority.max(self.native_attack_priority(info, rider));
            }
            let distance = squared.sqrt();
            let distance_modifier = (distance / distance_scale)
                .to_i32()
                .expect("native attack priority modifier");
            let modified = priority.wrapping_sub(distance_modifier).max(1);
            if modified > effective || modified == effective && priority > actual {
                best = Some(target);
                effective = modified;
                actual = priority;
            }
        }
        best
    }

    fn hunt_distance_squared(&self, source: usize, target: usize) -> f32 {
        let center_squared = crate::skirmish::movement::distance_squared(
            self.entities[source].position,
            self.entities[target].position,
        );
        let radius = self.object_radius(source) + self.object_radius(target);
        if radius > 0.0 {
            let distance = center_squared.sqrt() - radius;
            if distance <= 0.0 {
                0.0
            } else {
                distance * distance
            }
        } else {
            center_squared
        }
    }

    fn hunt_enemy_allowed(&mut self, source: usize, target: usize) -> bool {
        let actor = &self.entities[target];
        if actor.is_effectively_dead()
            || actor.is_off_map() != self.entities[source].is_off_map()
            || self.object_relationship(source, target) != 0
        {
            return false;
        }
        if !self.building_target_allowed(source, target) {
            return false;
        }
        if !matches!(
            self.native_weapon_attack_result(
                source,
                Some(target),
                self.object_position(target),
                false,
                COMMAND_FROM_AI
            ),
            CanAttackResult::Possible | CanAttackResult::PossibleAfterMoving
        ) {
            return false;
        }
        self.partition_stealth_visible(source, target)
    }
}
