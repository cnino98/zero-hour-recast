//! `DeployStyleAIUpdate`, SH 2bfaea60, `RETAIL_COMPATIBLE_CRC=0`.
use super::{Entity, Order, Skirmish, ai::GuardPhase, combat::attack::Goal};

pub struct Rules {
    pub pack: u32,
    pub unpack: u32,
    pub manual: bool,
    pub turret_only_deployed: bool,
    pub center_before_packing: bool,
    pub sounds: [String; 2],
}

#[derive(Clone, Copy)]
enum Phase {
    Move,
    Deploy,
    Attack,
    Undeploy,
    Align,
}

pub(super) struct State {
    phase: Phase,
    deadline: u32,
}

impl Entity {
    #[must_use]
    pub fn deployment_condition(&self) -> Option<&'static str> {
        self.deployment.as_ref()?;
        let flags = &self.drawable.model_conditions;
        if flags.has_flag(1 << 74) {
            Some("UNPACKING")
        } else if flags.has_flag(1 << 73) {
            Some("PACKING")
        } else if flags.has_flag(1 << 75) {
            Some("DEPLOYED")
        } else {
            None
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_deployment_module(
        &mut self,
        id: usize,
        registration: usize,
    ) {
        self.construct_ai_module(id, registration);
        self.entities[id].deployment = Some(State {
            phase: Phase::Move,
            deadline: 0,
        });
    }

    /// The subclass runs before `AIUpdate`'s primary, turret and locomotor clocks.
    pub(super) fn update_deployment(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(state) = &actor.deployment else {
            return;
        };
        let now = self.update_frame();
        let moving = actor.ai.path_request.pending.is_some() || actor.ai.ground_path.is_some();
        let guard_idle = matches!(actor.ai.order, Order::Guard(_))
            && matches!(actor.ai.guard.phase, GuardPhase::Idle);
        let weapon = self.weapon_slot_present(id, actor.weapon_slot());
        let attacking = actor.ai.attack.is_some();
        let victim = actor
            .ai
            .current_victim
            .filter(|&target| self.entities[target].exists());
        let goal = victim.map(Goal::Object).or_else(|| {
            (actor.weapon_set.status.attacking && actor.ai.current_victim.is_none())
                .then_some(Goal::Position(actor.ai.command.goal_position))
        });
        let in_range =
            attacking && weapon && goal.is_some_and(|goal| self.attack_goal_in_range(id, goal));
        if state.deadline != 0 && now >= state.deadline {
            match state.phase {
                Phase::Deploy => self.change_deployment(id, Phase::Attack, false),
                Phase::Undeploy => self.change_deployment(id, Phase::Move, false),
                _ => {}
            }
        }
        let state = self.entities[id]
            .deployment
            .as_ref()
            .expect("DeployStyle constructor");
        let transition = if !moving && (in_range || guard_idle) {
            match state.phase {
                Phase::Move => Some((Phase::Deploy, false)),
                Phase::Undeploy if state.deadline != 0 => Some((Phase::Deploy, true)),
                Phase::Align => Some((Phase::Attack, false)),
                _ => None,
            }
        } else if moving {
            let turret = if weapon {
                self.current_weapon_turret(id)
            } else {
                None
            };
            let center = self.catalog[self.entities[id].type_index]
                .deployment
                .as_ref()
                .expect("DeployStyle data")
                .center_before_packing;
            match state.phase {
                Phase::Attack if turret.is_some() && center => Some((Phase::Align, false)),
                Phase::Attack => Some((Phase::Undeploy, false)),
                Phase::Deploy if state.deadline != 0 => Some((Phase::Undeploy, true)),
                Phase::Align
                    if turret.is_some_and(|which| self.native_turret_is_natural(id, which)) =>
                {
                    Some((Phase::Undeploy, false))
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some((phase, reverse)) = transition {
            self.change_deployment(id, phase, reverse);
        }
        let state = self.entities[id]
            .deployment
            .as_ref()
            .expect("DeployStyle constructor");
        match state.phase {
            Phase::Move if moving => self.entities[id]
                .drawable
                .model_conditions
                .set_flag(super::ModelConditions::MOVING, true),
            Phase::Deploy | Phase::Undeploy | Phase::Align => {
                let rules = self.catalog[self.entities[id].type_index]
                    .deployment
                    .as_ref()
                    .expect("DeployStyle data");
                if rules.manual && !matches!(state.phase, Phase::Align) {
                    let left = state.deadline.wrapping_sub(now);
                    let frame = if matches!(state.phase, Phase::Deploy) {
                        rules.pack.wrapping_sub(left)
                    } else {
                        left
                    };
                    self.set_drawable_animation_frame(id, frame);
                }
                self.set_temporary_busy(id);
                self.entities[id].ai.motion.clear_goal();
            }
            _ => {}
        }
    }

    fn change_deployment(&mut self, id: usize, phase: Phase, reverse: bool) {
        let now = self.update_frame();
        let rules = self.catalog[self.entities[id].type_index]
            .deployment
            .as_ref()
            .expect("DeployStyle data");
        let state = self.entities[id]
            .deployment
            .as_mut()
            .expect("DeployStyle constructor");
        state.phase = phase;
        let left = state.deadline.wrapping_sub(now);
        state.deadline = match phase {
            Phase::Deploy | Phase::Undeploy if reverse => {
                now.wrapping_add(rules.unpack).wrapping_sub(left)
            }
            Phase::Deploy => now.wrapping_add(rules.unpack),
            Phase::Undeploy => now.wrapping_add(rules.pack),
            _ => 0,
        };
        let manual_frame = (rules.manual && reverse).then(|| {
            if matches!(phase, Phase::Deploy) {
                rules.unpack.wrapping_sub(left)
            } else {
                left
            }
        });
        let only_deployed = rules.turret_only_deployed;
        let sound = match phase {
            Phase::Deploy => Some(rules.sounds[0].clone()),
            Phase::Undeploy => Some(rules.sounds[1].clone()),
            _ => None,
        };
        let (set, clear) = match phase {
            Phase::Deploy => (1 << 74, 1 << 73),
            Phase::Undeploy => (1 << 73, (1 << 74) | (1 << 75)),
            Phase::Move => (0, 1 << 73),
            Phase::Attack => (1 << 75, 1 << 74),
            Phase::Align => (0, 0),
        };
        self.ability_model_flags(id, set, clear);
        if matches!(phase, Phase::Undeploy) {
            self.entities[id].object_status &= !(1 << 44);
        }
        if matches!(phase, Phase::Attack) {
            self.entities[id].object_status |= 1 << 44;
        }
        if let Some(frame) = manual_frame {
            self.set_drawable_animation_frame(id, frame);
        }
        if self.weapon_slot_present(id, self.entities[id].weapon_slot())
            && let Some(which) = self.current_weapon_turret(id)
        {
            match phase {
                Phase::Align => self.center_native_turret(id, which),
                Phase::Attack if only_deployed => self.set_native_turret_enabled(id, which, true),
                Phase::Undeploy if only_deployed => {
                    self.set_native_turret_enabled(id, which, false);
                }
                _ => {}
            }
        }
        if let Some(sound) = sound {
            self.object_sound(id, sound, super::object_sounds::Action::Play);
        }
    }

    fn set_drawable_animation_frame(&mut self, id: usize, frame: u32) {
        if self.entities[id].drawable.present {
            self.entities[id].drawable.animation_frame =
                Some((self.tick, i32::from_ne_bytes(frame.to_ne_bytes())));
        }
    }
}

#[cfg(test)]
mod tests;
