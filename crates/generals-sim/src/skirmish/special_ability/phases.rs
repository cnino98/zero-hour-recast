//! Native independent ability phases; primary AI continues to update throughout.
use super::{
    ABILITY_MODEL_FLAGS, FOREVER, Facing, PACKING, PackingState, PowerType, RAISING_FLAG, Skirmish,
    UNPACKING,
};
use crate::skirmish::{combat::attack::Goal, object_sounds::Action, startup};
use num_traits::ToPrimitive;

impl Skirmish {
    pub(in crate::skirmish) fn initiate_native_ability(
        &mut self,
        id: usize,
        ordinal: usize,
        power: &str,
        target: Option<usize>,
        position: Option<[f32; 3]>,
    ) -> bool {
        if self
            .ability_rules(id, ordinal)
            .power
            .as_ref()
            .is_none_or(|value| value.name != power)
        {
            return false;
        }
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        state.target = target;
        state.target_position = if target.is_some() {
            [0.0; 3]
        } else {
            position.unwrap_or([0.0; 3])
        };
        state.location_count = 0;
        state.preparing.frames = 0;
        state.animation_frames = 0;
        state.packing = PackingState::Packed;
        state.facing = Facing {
            initiated: false,
            complete: false,
        };
        state.preparing.within_start_range = false;
        self.ability_model_flags(id, 0, ABILITY_MODEL_FLAGS);
        if self.entities[id].ai.registration.is_none() {
            return false;
        }
        self.ai_idle_from_ai(id);
        let no_target = target.is_none() && position.is_none();
        let rules = self.ability_rules(id, ordinal);
        let unpacked = rules.timing.unpack == 0 || (no_target && rules.packing.skip_no_target);
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        state.no_target = no_target;
        if unpacked {
            state.packing = PackingState::Unpacked;
        }
        state.active = true;
        for kind in [
            PowerType::SpecialBlacklotusDisableVehicleHack,
            PowerType::SpecialBlacklotusStealCashHack,
            PowerType::SpecialBlacklotusCaptureBuilding,
            PowerType::SpecialRemoteCharges,
            PowerType::SpecialTimedCharges,
            PowerType::SpecialInfantryCaptureBuilding,
            PowerType::SpecialBoobyTrap,
        ] {
            if let Some(other) = self
                .first_special_ability(id, kind)
                .filter(|&other| other != ordinal)
            {
                self.exit_native_ability(id, other, false);
            }
        }
        self.wake_family_update(id, "SpecialAbilityUpdate", ordinal, 1);
        true
    }

    pub(in crate::skirmish) fn exit_native_ability(
        &mut self,
        id: usize,
        ordinal: usize,
        cleanup: bool,
    ) {
        self.ability_model_flags(id, 0, ABILITY_MODEL_FLAGS);
        self.entities[id].object_status &= !(1 << 24);
        self.stop_ability_loop(id, ordinal);
        self.end_native_preparation(id, ordinal);
        let rules = &self.ability_rules(id, ordinal).objects;
        if !rules.persistent || (cleanup && !rules.persist_on_death) {
            self.kill_ability_objects(id, ordinal);
        }
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        state.active = false;
        state.preparing.within_start_range = false;
        state.packing = PackingState::None;
    }

    pub(in crate::skirmish) fn update_special_ability(&mut self, id: usize, ordinal: usize) -> i32 {
        self.validate_ability_objects(id, ordinal);
        if self.entities[id].is_effectively_dead() {
            self.exit_native_ability(id, ordinal, true);
            return self.ability_sleep(id, ordinal);
        }
        if !self.entities[id].behaviors.special_abilities[ordinal].active {
            return self.ability_sleep(id, ordinal);
        }
        if self.entities[id].ai.registration.is_none() || !self.entities[id].last_command_from_ai()
        {
            self.exit_native_ability(id, ordinal, false);
            return self.ability_sleep(id, ordinal);
        }
        if self.entities[id].moving()
            && self.entities[id].special_ability_in_use(ordinal)
            && !self.entities[id].behaviors.special_abilities[ordinal]
                .facing
                .initiated
            && matches!(
                self.ability_kind(id, ordinal),
                PowerType::SpecialInfantryCaptureBuilding
                    | PowerType::SpecialBlacklotusCaptureBuilding
            )
        {
            self.exit_native_ability(id, ordinal, false);
            return self.ability_sleep(id, ordinal);
        }
        if self.process_ability_packing(id, ordinal) {
            return self.ability_sleep(id, ordinal);
        }
        let power = self
            .ability_rules(id, ordinal)
            .power
            .as_ref()
            .expect("active native power")
            .name
            .clone();
        let Some(spm) = self
            .special_power_ordinal(id, &power)
            .filter(|_| !self.ability_target_abort(id, ordinal))
        else {
            self.ai_idle_from_ai(id);
            self.exit_native_ability(id, ordinal, false);
            return self.ability_sleep(id, ordinal);
        };
        if self.entities[id].behaviors.special_abilities[ordinal]
            .preparing
            .frames
            > 0
        {
            self.update_native_preparation(id, ordinal, spm, &power);
        } else if self.ability_start_range(id, ordinal) {
            self.entities[id].behaviors.special_abilities[ordinal]
                .preparing
                .within_start_range = true;
            if !self.native_ability_is_facing(id, ordinal)
                && self.native_ability_needs_face(id, ordinal)
            {
                self.start_native_ability_facing(id, ordinal);
                return self.ability_sleep(id, ordinal);
            }
            if self.ability_needs_packing(id, ordinal, false) {
                self.start_native_ability_packing(id, ordinal, false, false);
                return self.ability_sleep(id, ordinal);
            }
            if matches!(
                self.entities[id].behaviors.special_abilities[ordinal].packing,
                PackingState::Unpacked
            ) {
                self.start_native_preparation(id, ordinal);
                if self.entities[id].behaviors.special_abilities[ordinal]
                    .preparing
                    .frames
                    == 0
                {
                    self.trigger_native_ability(id, ordinal);
                    let persistent = self.ability_rules(id, ordinal).timing.persistent;
                    let recharge = self
                        .ability_rules(id, ordinal)
                        .preparation
                        .persistence_recharges;
                    if persistent > 0 && recharge {
                        self.entities[id].behaviors.special_abilities[ordinal]
                            .preparing
                            .frames = persistent;
                        self.recharge_special_power(id, spm);
                        return self.ability_sleep(id, ordinal);
                    }
                    self.complete_native_preparation(id, ordinal, true);
                }
            }
        } else if self.entities[id].ai_idle() {
            self.approach_native_ability(id, ordinal);
        }
        self.ability_sleep(id, ordinal)
    }

    fn update_native_preparation(&mut self, id: usize, ordinal: usize, spm: usize, power: &str) {
        let rules = self.ability_rules(id, ordinal);
        let ready = rules.timing.persistent == 0
            || !rules.preparation.persistence_recharges
            || (self.special_power_ready(id, power)
                && self
                    .special_power_ready_frame(id, power)
                    .is_some_and(|frame| frame < self.update_frame()));
        if ready {
            self.entities[id].behaviors.special_abilities[ordinal]
                .preparing
                .frames -= 1;
        }
        if self.entities[id].behaviors.special_abilities[ordinal]
            .preparing
            .frames
            == 0
        {
            self.trigger_native_ability(id, ordinal);
            if self.ability_rules(id, ordinal).timing.persistent > 0 {
                self.entities[id].behaviors.special_abilities[ordinal]
                    .preparing
                    .frames = self.ability_rules(id, ordinal).timing.persistent;
                if self
                    .ability_rules(id, ordinal)
                    .preparation
                    .persistence_recharges
                {
                    self.recharge_special_power(id, spm);
                }
            } else {
                self.complete_native_preparation(id, ordinal, true);
            }
        } else if !self.continue_native_preparation(id, ordinal) {
            self.complete_native_preparation(id, ordinal, false);
        }
    }

    fn complete_native_preparation(&mut self, id: usize, ordinal: usize, success: bool) {
        self.end_native_preparation(id, ordinal);
        if self.ability_needs_packing(id, ordinal, true) {
            self.start_native_ability_packing(id, ordinal, true, success);
        } else {
            self.finish_native_ability(id, ordinal);
        }
    }

    fn ability_sleep(&self, id: usize, ordinal: usize) -> i32 {
        if self.entities[id].behaviors.special_abilities[ordinal].active
            || self.ability_rules(id, ordinal).preparation.always_validate
        {
            1
        } else {
            i32::try_from(FOREVER).expect("native forever")
        }
    }

    pub(super) fn ability_needs_packing(&self, id: usize, ordinal: usize, packing: bool) -> bool {
        let state = &self.entities[id].behaviors.special_abilities[ordinal];
        let rules = self.ability_rules(id, ordinal);
        let phase = if packing {
            matches!(state.packing, PackingState::Unpacked)
        } else {
            matches!(state.packing, PackingState::Packed)
        };
        phase
            && !(rules.packing.skip_no_target && state.no_target)
            && if packing {
                rules.timing.pack != 0
            } else {
                rules.timing.unpack != 0
            }
    }

    fn process_ability_packing(&mut self, id: usize, ordinal: usize) -> bool {
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        if state.animation_frames == 0 {
            return false;
        }
        state.animation_frames -= 1;
        if state.animation_frames == 0 {
            self.ability_model_flags(id, 0, PACKING | UNPACKING);
            let rules = self.ability_rules(id, ordinal).packing;
            let state = &self.entities[id].behaviors.special_abilities[ordinal];
            if matches!(state.packing, PackingState::Unpacking) {
                if rules.flip_after_unpack {
                    self.set_object_orientation(
                        id,
                        self.entities[id].facing + std::f32::consts::PI,
                    );
                }
                self.entities[id].behaviors.special_abilities[ordinal].packing =
                    PackingState::Unpacked;
            } else if matches!(state.packing, PackingState::Packing) {
                if rules.flip_after_pack {
                    self.set_object_orientation(
                        id,
                        self.entities[id].facing + std::f32::consts::PI,
                    );
                }
                self.entities[id].behaviors.special_abilities[ordinal].packing =
                    PackingState::Packed;
                self.finish_native_ability(id, ordinal);
                return true;
            }
            return false;
        }
        let rules = self.ability_rules(id, ordinal);
        if rules.effects.lose_stealth
            && self.entities[id].behaviors.special_abilities[ordinal].animation_frames
                < rules.timing.unstealth
        {
            self.mark_detected(id, 0);
        }
        true
    }

    fn start_native_ability_packing(
        &mut self,
        id: usize,
        ordinal: usize,
        packing: bool,
        success: bool,
    ) {
        let timing = self.ability_rules(id, ordinal).timing;
        let variation = startup::random_real(
            &mut self.random,
            1.0 - timing.variation,
            1.0 + timing.variation,
        );
        let frames = (crate::skirmish::progression::integer_real(if packing {
            timing.pack
        } else {
            timing.unpack
        }) * variation)
            .to_u32()
            .expect("native unsigned animation frame conversion");
        let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
        state.packing = if packing {
            PackingState::Packing
        } else {
            PackingState::Unpacking
        };
        state.animation_frames = frames;
        if packing {
            self.ability_model_flags(id, PACKING, UNPACKING | RAISING_FLAG);
        } else {
            self.ability_model_flags(id, UNPACKING, PACKING);
        }
        let sound = self.ability_rules(id, ordinal).feedback.sounds[usize::from(!packing)].clone();
        self.object_sound(id, sound, Action::Play);
        if self.entities[id].drawable.present {
            self.entities[id].drawable.animation_completion = Some((self.tick, frames, true));
        }
        self.ai_busy_from_ai(id);
        if packing && success {
            self.ability_completion_voice(id, ordinal);
        }
    }

    fn native_ability_is_facing(&mut self, id: usize, ordinal: usize) -> bool {
        if self.entities[id].ai.registration.is_none() {
            return true;
        }
        let facing = &self.entities[id].behaviors.special_abilities[ordinal].facing;
        if !facing.complete && facing.initiated {
            if self.entities[id].ai_idle() {
                self.entities[id].behaviors.special_abilities[ordinal]
                    .facing
                    .complete = true;
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    fn native_ability_needs_face(&self, id: usize, ordinal: usize) -> bool {
        let state = &self.entities[id].behaviors.special_abilities[ordinal];
        self.entities[id].ai.registration.is_some()
            && self.ability_rules(id, ordinal).approach.face_target
            && (!state.facing.initiated || !state.facing.complete)
    }

    fn start_native_ability_facing(&mut self, id: usize, ordinal: usize) {
        let target = self.present_ability_target(id, ordinal);
        if self.entities[id].ai.registration.is_none() {
            return;
        }
        self.ai_idle_from_ai(id);
        if self.object_physics_mass(id).is_some() {
            self.reset_dynamic_physics(id);
        }
        self.entities[id].behaviors.special_abilities[ordinal]
            .facing
            .initiated = true;
        if let Some(target) = target {
            self.ai_face_from_ai(id, Goal::Object(target));
        } else {
            let position = self.entities[id].behaviors.special_abilities[ordinal].target_position;
            if position.iter().any(|&axis| axis != 0.0) {
                self.ai_face_from_ai(id, Goal::Position(position));
            }
        }
    }

    fn approach_native_ability(&mut self, id: usize, ordinal: usize) {
        if let Some(target) = self.present_ability_target(id, ordinal) {
            self.entities[id].ai.ignored_producer = Some(target);
            self.native_move_to_object_from(id, target, crate::skirmish::ai_goals::COMMAND_FROM_AI);
        } else {
            let position = self.entities[id].behaviors.special_abilities[ordinal].target_position;
            if position.iter().any(|&axis| axis != 0.0) {
                self.construction_move(id, position);
            }
        }
    }

    pub(in crate::skirmish) fn ability_model_flags(&mut self, id: usize, set: u128, clear: u128) {
        if self.entities[id].drawable.present {
            let flags = &mut self.entities[id].drawable.model_conditions.flags;
            *flags = (*flags & !clear) | set;
        }
    }

    pub(super) fn stop_ability_loop(&mut self, id: usize, ordinal: usize) {
        let sound = self.ability_rules(id, ordinal).feedback.sounds[2].clone();
        self.object_sound(
            id,
            sound,
            Action::StopLoop(self.ability_loop_index(id, ordinal)),
        );
    }
}
