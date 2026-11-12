//! Source contact/LOS range gates use partition membership, not weapon legality.
use super::{PowerType, Skirmish};
use crate::{
    collision_geometry::{Body, Shape},
    skirmish::partition::Identity,
};

impl Skirmish {
    pub(super) fn ability_start_range(&self, id: usize, ordinal: usize) -> bool {
        if self.entities[id].behaviors.special_abilities[ordinal]
            .preparing
            .within_start_range
        {
            return true;
        }
        self.ability_range(id, ordinal, false)
    }

    pub(super) fn ability_range(&self, id: usize, ordinal: usize, abort: bool) -> bool {
        let state = &self.entities[id].behaviors.special_abilities[ordinal];
        let rules = self.ability_rules(id, ordinal);
        let target = self.present_ability_target(id, ordinal);
        let distance = if state.target.is_some() {
            target.map_or(0.0, |target| {
                self.ability_distance(id, Some(target), [0.0; 3])
            })
        } else if state.target_position.iter().any(|&axis| axis != 0.0) {
            self.ability_distance(id, None, state.target_position)
        } else {
            return true;
        };
        let authored = if abort {
            rules.approach.abort_range
        } else {
            rules.approach.start_range
        };
        if !matches!(
            distance.partial_cmp(&(authored * authored)),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ) {
            return false;
        }
        let undersized = rules.approach.start_range - 2.5;
        let range = if 0.0 > undersized { 0.0 } else { undersized };
        if range == 0.0 && state.target.is_some() {
            return target.is_some_and(|target| {
                self.native_potential_collisions(
                    &Body {
                        geometry: self.body_geometry(id),
                        position: self.object_position(id),
                        angle: 0.0,
                    },
                    false,
                )
                .contains(&Identity::Actor(target))
            });
        }
        if abort || !rules.approach.requires_los {
            return true;
        }
        let Some(target) = target.filter(|&target| target != id) else {
            return false;
        };
        // iterateObjectsInRange uses strict radius and skips the source Object.
        self.partition
            .outward_range_rings(self.entities[id].position, range)
            .iter()
            .any(|&(_, other)| other == target)
            && distance < range * range
            && self.ability_line_of_sight(id, target)
            && (self.height_above_ground(target) > -9.0 * self.gravity
                || !self.attack_view_blocked(
                    id,
                    Some(target),
                    self.object_position(id),
                    self.object_position(target),
                ))
    }

    fn ability_distance(&self, id: usize, target: Option<usize>, position: [f32; 3]) -> f32 {
        let position = target.map_or(position, |target| self.object_position(target));
        let source = self.object_position(id);
        let delta = [position[0] - source[0], position[1] - source[1]];
        let axis_squared = [delta[0] * delta[0], delta[1] * delta[1]];
        let squared = axis_squared[0] + axis_squared[1];
        let radius = |object| {
            let geometry = self.body_geometry(object);
            if matches!(geometry.shape, Shape::Box) {
                let extent_squared = [
                    geometry.major * geometry.major,
                    geometry.minor * geometry.minor,
                ];
                (extent_squared[0] + extent_squared[1]).sqrt()
            } else {
                geometry.major
            }
        };
        let total = radius(id) + target.map_or(0.0, radius);
        if total > 0.0 {
            let distance = squared.sqrt() - total;
            if distance <= 0.0 {
                0.0
            } else {
                distance * distance
            }
        } else {
            squared
        }
    }

    pub(super) fn ability_target_abort(&self, id: usize, ordinal: usize) -> bool {
        let Some(target) = self.present_ability_target(id, ordinal) else {
            return false;
        };
        let victim = &self.entities[target];
        if victim.is_effectively_dead() {
            return true;
        }
        let kind = self.ability_kind(id, ordinal);
        let hidden = victim.stealthed() && !victim.detected();
        let not_prepared = self.entities[id].behaviors.special_abilities[ordinal]
            .preparing
            .frames
            != 0;
        match kind {
            PowerType::SpecialInfantryCaptureBuilding
            | PowerType::SpecialBlacklotusCaptureBuilding
            | PowerType::SpecialHackerDisableBuilding => {
                self.team_membership.team_of(target) == self.team_membership.team_of(id)
                    || hidden && not_prepared
            }
            PowerType::SpecialBlacklotusStealCashHack | PowerType::SpecialBoobyTrap => {
                hidden && not_prepared
            }
            PowerType::SpecialRemoteCharges | PowerType::SpecialTimedCharges => {
                !self.ability_needs_packing(id, ordinal, false) && hidden && not_prepared
            }
            PowerType::SpecialMissileDefenderLaserGuidedMissiles => {
                self.catalog[victim.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "STRUCTURE")
                    || hidden
            }
            PowerType::SpecialBlacklotusDisableVehicleHack => hidden,
            _ => false,
        }
    }
}
