//! Source effects follow experience/skill awards and `TriggerSound`, in that order.
use super::{Effects, FeedbackEffect, PowerType, Skirmish};
use crate::skirmish::{combat::attack::Goal, object_sounds::Action, weapons::Lock};

impl Skirmish {
    pub(super) fn trigger_native_ability(&mut self, id: usize, ordinal: usize) {
        let effects = self.ability_rules(id, ordinal).effects;
        self.award_native_ability(id, effects);
        let sound = self.ability_rules(id, ordinal).feedback.sounds[3].clone();
        self.object_sound(id, sound, Action::Play);
        let mut lose_stealth = true;
        let target = self.present_ability_target(id, ordinal);
        match self.ability_kind(id, ordinal) {
            PowerType::SpecialMissileDefenderLaserGuidedMissiles => {
                if let Some(target) = target
                    && self.allocated_weapon(id, 1).is_some()
                {
                    if !matches!(self.entities[id].weapon_set.lock, Lock::Permanent) {
                        self.entities[id].weapon_set.current = 1;
                        self.entities[id].weapon_set.lock = Lock::Temporary;
                    }
                    if self.entities[id].ai.registration.is_some() {
                        self.ai_attack_goal(id, Goal::Object(target), i32::MAX.unsigned_abs());
                    }
                }
            }
            PowerType::SpecialHelixNapalmBomb => {
                self.create_ability_object(id, ordinal);
            }
            PowerType::SpecialTankhunterTntAttack
            | PowerType::SpecialTimedCharges
            | PowerType::SpecialBoobyTrap => {
                let Some(target) = target else {
                    return;
                };
                if self.ability_detonate_trap(id, target) {
                    return;
                }
                if self.ability_kind(id, ordinal) == PowerType::SpecialBoobyTrap
                    && self.entities[target].booby_trapped()
                {
                    return;
                }
                self.place_ability_charge(id, ordinal, target);
            }
            PowerType::SpecialHackerDisableBuilding
            | PowerType::SpecialBlacklotusDisableVehicleHack => {
                let Some(target) = target else {
                    return;
                };
                if self.object_relationship(id, target) == 2 {
                    return;
                }
                self.disable_from_native_ability(id, ordinal, target);
            }
            PowerType::SpecialInfantryCaptureBuilding
            | PowerType::SpecialBlacklotusCaptureBuilding => {
                let Some(target) = target else {
                    return;
                };
                if !self.capture_from_native_ability(id, ordinal, target) {
                    return;
                }
            }
            PowerType::SpecialBlacklotusStealCashHack => {
                let Some(target) = target else {
                    return;
                };
                self.steal_native_ability_cash(id, target);
            }
            PowerType::SpecialRemoteCharges => {
                if target.is_some_and(|target| self.ability_detonate_trap(id, target)) {
                    return;
                }
                let state = &self.entities[id].behaviors.special_abilities[ordinal];
                if state.target.is_none() && !state.target_position.iter().any(|&axis| axis != 0.0)
                {
                    let objects = state.objects.clone();
                    for object in objects {
                        if self.entities[object].exists()
                            && !self.catalog[self.entities[object].type_index]
                                .behavior
                                .sticky_bombs
                                .is_empty()
                        {
                            self.detonate_sticky_bomb(object, 0);
                            lose_stealth = false;
                        }
                    }
                } else {
                    let Some(target) = target else {
                        return;
                    };
                    self.place_ability_charge(id, ordinal, target);
                }
            }
            _ => {}
        }
        if effects.lose_stealth && lose_stealth {
            self.mark_detected(id, 0);
        }
    }

    fn award_native_ability(&mut self, id: usize, effects: Effects) {
        if effects.experience != 0 {
            self.add_unit_experience(id, effects.experience);
        }
        let skill = if effects.skill_points == -1 {
            effects.experience
        } else {
            effects.skill_points
        };
        if skill > 0 {
            self.add_capture_skill_points(
                self.entities[id].owner,
                u32::from_ne_bytes(skill.to_ne_bytes()),
            );
        }
    }

    fn capture_from_native_ability(&mut self, id: usize, ordinal: usize, target: usize) -> bool {
        if self.ability_detonate_trap(id, target) {
            return false;
        }
        if self.team_membership.team_of(id) == self.team_membership.team_of(target) {
            return false;
        }
        if self.is_garrison(target) {
            self.remove_garrison_contents_exposed(target, true);
        } else {
            self.native_ability_feedback(id, target, FeedbackEffect::BuildingStolen);
            let owner = self.entities[id].owner;
            let team = self.team_membership.default_team(owner);
            self.defect_to_team(target, team, 1);
            if self.ability_kind(id, ordinal) == PowerType::SpecialBlacklotusCaptureBuilding {
                let name = &self
                    .ability_rules(id, ordinal)
                    .power
                    .as_ref()
                    .expect("capture power")
                    .name;
                if let Some(spm) = self.special_power_ordinal(id, name) {
                    self.recharge_special_power(id, spm);
                }
            }
            self.players[owner].academy.buildings_captured = self.players[owner]
                .academy
                .buildings_captured
                .wrapping_add(1);
        }
        true
    }

    fn steal_native_ability_cash(&mut self, id: usize, target: usize) {
        // PRESERVE_HARDCODED_BLACK_LOTUS_CASH_HACK is true in the pinned
        // profile even though RETAIL_COMPATIBLE_CRC is false.
        let owner = self.entities[id].owner;
        let victim = self.entities[target].owner;
        let cash = self.players[victim].cash.min(1000);
        if cash > 0 {
            self.players[victim].cash -= cash;
            self.deposit_native_money(owner, cash);
            self.players[owner].statistics.money_earned = self.players[owner]
                .statistics
                .money_earned
                .wrapping_add(cash);
            self.native_ability_feedback(id, target, FeedbackEffect::CashStolen(cash));
        }
    }

    fn ability_detonate_trap(&mut self, id: usize, target: usize) -> bool {
        self.check_and_detonate_booby_trap(target, Some(id))
            && (self.entities[target].is_effectively_dead()
                || self.entities[id].is_effectively_dead())
    }

    fn place_ability_charge(&mut self, id: usize, ordinal: usize, target: usize) {
        if let Some(charge) = self.create_ability_object(id, ordinal) {
            if self.catalog[self.entities[charge].type_index]
                .behavior
                .sticky_bombs
                .is_empty()
            {
                self.kill_ability_objects(id, ordinal);
                return;
            }
            self.init_sticky_bomb(charge, 0, Some(target), Some(id), None);
        }
    }

    fn disable_from_native_ability(&mut self, id: usize, ordinal: usize, target: usize) {
        let duration = self.ability_rules(id, ordinal).timing.effect;
        self.set_hacked_until(target, self.update_frame().wrapping_add(duration));
        let geometry = self.body_geometry(target);
        let area = if matches!(geometry.shape, crate::collision_geometry::Shape::Box) {
            (4.0 * geometry.major) * geometry.minor
        } else {
            std::f32::consts::PI * (geometry.major * geometry.major)
        };
        let small = area < 300.0
            && self.catalog[self.entities[target].type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "STRUCTURE");
        if small {
            let state = &mut self.entities[id].behaviors.special_abilities[ordinal];
            state.preparing.disable_particles = !state.preparing.disable_particles;
        }
        if self.entities[id].behaviors.special_abilities[ordinal]
            .preparing
            .disable_particles
            && let Some(name) = self
                .ability_rules(id, ordinal)
                .feedback
                .disable_particle
                .clone()
        {
            self.native_ability_feedback(
                id,
                target,
                FeedbackEffect::DisableParticles {
                    name,
                    lifetime: duration.wrapping_mul(if small { 2 } else { 1 }),
                    geometry: {
                        let geometry = self.body_geometry(target);
                        crate::collision_geometry::Geometry {
                            is_small: geometry.is_small,
                            shape: geometry.shape,
                            major: geometry.major,
                            minor: geometry.minor,
                            height: geometry.height,
                        }
                    },
                },
            );
        }
    }
}

#[cfg(test)]
#[path = "../hacker_disable/effect_tests.rs"]
mod hacker_disable_tests;
