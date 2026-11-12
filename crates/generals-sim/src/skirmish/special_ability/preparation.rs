//! Native preparation can return early after setting its timer and before audio/status.
use super::{FIRING_A, FeedbackEffect, PackingState, PowerType, RAISING_FLAG, Skirmish, UNPACKING};
use crate::skirmish::object_sounds::Action;
use num_traits::ToPrimitive;

impl Skirmish {
    pub(super) fn start_native_preparation(&mut self, id: usize, ordinal: usize) {
        let frames = self.ability_rules(id, ordinal).timing.preparation;
        self.entities[id].behaviors.special_abilities[ordinal]
            .preparing
            .frames = frames;
        let kind = self.ability_kind(id, ordinal);
        let target = self.present_ability_target(id, ordinal);
        match kind {
            PowerType::SpecialMissileDefenderLaserGuidedMissiles => {
                if let Some(target) = target
                    && let Some(object) = self.create_ability_object(id, ordinal)
                    && !self.initialize_ability_laser(id, ordinal, object, Some(target))
                {
                    return;
                }
            }
            PowerType::SpecialInfantryCaptureBuilding => {
                if let Some(target) = target {
                    if self.team_membership.team_of(target) == self.team_membership.team_of(id) {
                        return;
                    }
                    if self.check_and_detonate_booby_trap(target, Some(id))
                        && self.entities[target].is_effectively_dead()
                    {
                        return;
                    }
                }
                self.ability_model_flags(id, RAISING_FLAG, UNPACKING);
                if self.entities[id].drawable.present {
                    self.entities[id].drawable.animation_completion =
                        Some((self.tick, frames, true));
                }
                if let Some(target) = target {
                    self.native_ability_feedback(
                        id,
                        target,
                        FeedbackEffect::Infiltration {
                            building_stolen_warning: true,
                        },
                    );
                }
            }
            PowerType::SpecialHackerDisableBuilding
            | PowerType::SpecialBlacklotusCaptureBuilding
            | PowerType::SpecialBlacklotusDisableVehicleHack
            | PowerType::SpecialBlacklotusStealCashHack => {
                if let Some(target) = target {
                    if self.object_relationship(id, target) == 2 {
                        return;
                    }
                    if let Some(object) = self.create_ability_object(id, ordinal) {
                        if !self.initialize_ability_laser(id, ordinal, object, Some(target)) {
                            return;
                        }
                        self.ability_model_flags(id, FIRING_A, UNPACKING);
                    }
                    self.native_ability_feedback(
                        id,
                        target,
                        FeedbackEffect::Infiltration {
                            building_stolen_warning: kind
                                == PowerType::SpecialBlacklotusCaptureBuilding,
                        },
                    );
                }
            }
            _ => {}
        }
        let power = self
            .ability_rules(id, ordinal)
            .power
            .as_ref()
            .expect("active native power")
            .name
            .clone();
        self.mark_special_power_triggered(id, &power, None);
        if self.entities[id].ai.registration.is_some() {
            self.ai_idle_from_ai(id);
        }
        self.entities[id].object_status |= 1 << 24;
        let sound = self.ability_rules(id, ordinal).feedback.sounds[2].clone();
        self.object_sound(
            id,
            sound,
            Action::PlayLoop(self.ability_loop_index(id, ordinal)),
        );
    }

    pub(super) fn continue_native_preparation(&mut self, id: usize, ordinal: usize) -> bool {
        if self.ability_rules(id, ordinal).approach.abort_range < 10_000_000.0
            && !self.ability_range(id, ordinal, true)
        {
            return false;
        }
        match self.ability_kind(id, ordinal) {
            PowerType::SpecialMissileDefenderLaserGuidedMissiles
            | PowerType::SpecialBlacklotusDisableVehicleHack => {
                let Some(target) = self.present_ability_target(id, ordinal) else {
                    return false;
                };
                if self.object_relationship(id, target) == 2 {
                    return false;
                }
                let objects = self.entities[id].behaviors.special_abilities[ordinal]
                    .objects
                    .clone();
                for object in objects {
                    if self.entities[object].exists()
                        && !self.initialize_ability_laser(id, ordinal, object, Some(target))
                    {
                        return false;
                    }
                }
            }
            PowerType::SpecialInfantryCaptureBuilding
            | PowerType::SpecialBlacklotusCaptureBuilding => {
                let Some(target) = self.present_ability_target(id, ordinal) else {
                    return false;
                };
                if self.object_relationship(id, target) == 2 {
                    return false;
                }
                if self.ability_rules(id, ordinal).preparation.capture_fx
                    && self.entities[target].drawable.present
                {
                    let total = crate::skirmish::progression::integer_real(
                        self.ability_rules(id, ordinal).timing.preparation.max(1),
                    );
                    let remaining = crate::skirmish::progression::integer_real(
                        self.entities[id].behaviors.special_abilities[ordinal]
                            .preparing
                            .frames,
                    );
                    let phase =
                        &mut self.entities[id].behaviors.special_abilities[ordinal].capture_flash;
                    let old = phase.to_i32().expect("native capture flash phase fits Int") & 1;
                    let increment = 1.0 - remaining / total;
                    *phase += increment / 3.0;
                    if old != 0
                        && (phase.to_i32().expect("native capture flash phase fits Int") & 1) == 0
                    {
                        self.native_ability_feedback(id, target, FeedbackEffect::CaptureFlash);
                        let sound = self
                            .ability_rules(id, ordinal)
                            .feedback
                            .capture_tick
                            .clone();
                        self.object_sound(target, sound, Action::Play);
                    }
                }
                if self.ability_kind(id, ordinal) == PowerType::SpecialInfantryCaptureBuilding {
                    let power = &self
                        .ability_rules(id, ordinal)
                        .power
                        .as_ref()
                        .expect("capture power")
                        .name;
                    if let Some(spm) = self.special_power_ordinal(id, power) {
                        self.recharge_special_power(id, spm);
                    }
                }
            }
            _ => {}
        }
        true
    }

    pub(super) fn end_native_preparation(&mut self, id: usize, ordinal: usize) {
        self.entities[id].object_status &= !(1 << 24);
        self.stop_ability_loop(id, ordinal);
        if matches!(
            self.ability_kind(id, ordinal),
            PowerType::SpecialMissileDefenderLaserGuidedMissiles
                | PowerType::SpecialHackerDisableBuilding
                | PowerType::SpecialBlacklotusDisableVehicleHack
                | PowerType::SpecialBlacklotusCaptureBuilding
                | PowerType::SpecialBlacklotusStealCashHack
                | PowerType::SpecialInfantryCaptureBuilding
        ) {
            self.kill_ability_objects(id, ordinal);
        }
    }

    pub(super) fn finish_native_ability(&mut self, id: usize, ordinal: usize) {
        let rules = self.ability_rules(id, ordinal);
        let range = rules.approach.flee_range;
        let flip = rules.packing.flip_after_unpack || rules.packing.flip_after_pack;
        let state = &self.entities[id].behaviors.special_abilities[ordinal];
        let valid = state.target.is_some() || state.target_position.iter().any(|&axis| axis != 0.0);
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        state.preparing.within_start_range = false;
        state.packing = PackingState::None;
        if range != 0.0 && valid {
            if self.entities[id].ai.registration.is_some() {
                let angle = self.entities[id].facing;
                let direction = [angle.cos() * range, angle.sin() * range, 0.0];
                let mut position = self.object_position(id);
                for axis in 0..3 {
                    if flip {
                        position[axis] += direction[axis];
                    } else {
                        position[axis] -= direction[axis];
                    }
                }
                if let Some(mine) = self.ability_flee_mine(id, position, range) {
                    let origin = self.object_position(mine);
                    let mut direction = [position[0] - origin[0], position[1] - origin[1]];
                    let squared = [direction[0] * direction[0], direction[1] * direction[1]];
                    let length = (squared[0] + squared[1]).sqrt();
                    if length != 0.0 {
                        direction[0] /= length;
                        direction[1] /= length;
                    }
                    let offset = [direction[0] * range, direction[1] * range];
                    position = [origin[0] + offset[0], origin[1] + offset[1], origin[2]];
                }
                if self.object_physics_mass(id).is_some() {
                    self.apply_physics_motive_force(id, [0.0; 3]);
                }
                self.construction_move(id, position);
                if let Some(target) = self.present_ability_target(id, ordinal) {
                    self.entities[id].ai.ignored_producer = Some(target);
                }
            }
        } else if self.entities[id].ai.registration.is_some() {
            self.ai_idle_from_ai(id);
        }
        self.exit_native_ability(id, ordinal, false);
    }

    fn ability_flee_mine(&self, id: usize, position: [f32; 3], range: f32) -> Option<usize> {
        let mut closest = None;
        let mut maximum = range * range;
        let mut found_ring = None;
        for (ring, mine) in self
            .partition
            .outward_range_rings([position[0], position[1]], range)
        {
            if found_ring.is_some_and(|found| ring > found) {
                break;
            }
            let actor = &self.entities[mine];
            if actor.owner != self.entities[id].owner
                || !self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|name| name == "MINE")
            {
                continue;
            }
            let point = self.object_position(mine);
            let delta = [point[0] - position[0], point[1] - position[1]];
            let axis_squared = [delta[0] * delta[0], delta[1] * delta[1]];
            let distance = axis_squared[0] + axis_squared[1];
            if distance < maximum {
                maximum = distance;
                closest = Some(mine);
                found_ring = Some(ring);
            }
        }
        closest
    }

    pub(super) fn ability_completion_voice(&mut self, id: usize, ordinal: usize) {
        let sound = self
            .ability_rules(id, ordinal)
            .feedback
            .completion_voice
            .clone();
        self.object_sound(id, sound, Action::Play);
    }
}
