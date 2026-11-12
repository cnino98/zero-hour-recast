//! SH AIUpdate.cpp3648 / AIStates.cpp5796,7146: Hunt owns `AttackThenIdle`.
use crate::skirmish::{
    Order, Skirmish,
    ai_goals::{COMMAND_FROM_AI, MainState},
    combat::attack::Goal,
    weapons::Lock,
};

mod target;
#[cfg(test)]
mod tests;

pub(in crate::skirmish) struct State {
    next_scan: u32,
    goal: Option<usize>,
    inner: Inner,
}

enum Inner {
    Attack,
    Idle { initialized: bool, sleep_until: u32 },
}

impl Skirmish {
    pub(in crate::skirmish) fn ai_hunt_from_ai(&mut self, id: usize) {
        self.native_hunt_from(id, COMMAND_FROM_AI);
    }

    pub(in crate::skirmish) fn native_hunt_from(&mut self, id: usize, source: i32) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::Hunt,
            },
        ) {
            return;
        }
        if self.entities[id].ai.registration.is_none() {
            return;
        }
        if self.entities[id].ai.dozer.is_some() {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        if !self.native_ai_command_allowed(id)
            || self.attack_kind(id, "IMMOBILE")
            || self.object_disabled_mask(id) != 0
            || self.attack_kind(id, "PROJECTILE")
        {
            return;
        }
        if source == crate::skirmish::ai_goals::COMMAND_FROM_PLAYER {
            self.dozer_player_command(id);
        }
        self.clear_ai_goal(id);
        if self
            .prepare_order_state(id, Order::Idle, false, None, None)
            .is_none()
        {
            // clear/setState are refused under the native machine lock, but
            // privateHunt still records the command source between those calls.
            if self.entities[id].ai.command.locked {
                self.set_last_command_source(id, source);
                self.entities[id].ai.command.from_ai = source == COMMAND_FROM_AI;
                if source == crate::skirmish::ai_goals::COMMAND_FROM_PLAYER {
                    self.finish_dozer_player_command(id);
                }
            }
            return;
        }
        self.set_last_command_source(id, source);
        self.entities[id].ai.command.from_ai = source == COMMAND_FROM_AI;
        self.entities[id].ai.command.main_state = MainState::Hunt;
        let offset = self.random_between(30);
        self.entities[id].ai.hunt = Some(State {
            next_scan: self.update_frame().wrapping_add(offset),
            goal: None,
            inner: Inner::Attack,
        });
        // The default Attack performs its ordinary mutable ammo query before
        // discovering the null goal (AIAttackState::onEnter5507–5542).
        if self.attack_mood_allows(id) && self.entities[id].complete() {
            self.native_actor_out_of_ammo(id);
        }
        // AttackThenIdle's DEFAULT is Attack. Its null goal fails onEnter,
        // then enters DO_NOT_LOOK_FOR_TARGETS Idle, after Hunt's scan draw.
        self.enter_hunt_idle(id);
        self.ai_state_changed(id);
        if source == crate::skirmish::ai_goals::COMMAND_FROM_PLAYER {
            self.finish_dozer_player_command(id);
        }
    }

    fn enter_hunt_idle(&mut self, id: usize) {
        self.exit_native_attack(id);
        self.enter_primary_idle(id);
        self.entities[id]
            .ai
            .hunt
            .as_mut()
            .expect("Hunt machine")
            .inner = Inner::Idle {
            initialized: false,
            sleep_until: 0,
        };
    }

    pub(in crate::skirmish) fn retarget_hunt_goal(
        &mut self,
        id: usize,
        target: Option<usize>,
    ) -> bool {
        let Some(state) = &mut self.entities[id].ai.hunt else {
            return false;
        };
        state.goal = target;
        true
    }

    pub(in crate::skirmish) fn exit_hunt(&mut self, id: usize) {
        if self.entities[id].ai.hunt.take().is_some() {
            self.exit_native_attack(id);
            if matches!(self.entities[id].weapon_set.lock, Lock::Temporary) {
                self.entities[id].weapon_set.lock = Lock::None;
            }
        }
    }

    pub(in crate::skirmish) fn update_hunt(&mut self, id: usize) -> u32 {
        if self.update_move_aside(id) {
            return 1;
        }
        let now = self.update_frame();
        if !self.scan_hunt_victim(id, now) {
            return 1;
        }
        self.entities[id].ai.command.locked = true;
        self.update_hunt_inner(id, now);
        self.entities[id].ai.command.locked = false;
        // Hunt must scan each frame even if its inner Idle is sleeping.
        1
    }

    fn scan_hunt_victim(&mut self, id: usize, now: u32) -> bool {
        if now
            >= self.entities[id]
                .ai
                .hunt
                .as_ref()
                .expect("Hunt state")
                .next_scan
        {
            if self.native_actor_out_of_ammo(id) && !self.attack_kind(id, "PROJECTILE") {
                self.set_ai_order(id, Order::Idle);
                return false;
            }
            // AIUpdate::checkForCrateToPickup clears the ID before looking it
            // up (893), so the pinned implementation cannot return that crate.
            self.entities[id]
                .ai
                .hunt
                .as_mut()
                .expect("Hunt state")
                .next_scan = now.wrapping_add(30);
            let common = self.team_attacks_common_target(id);
            let info = self.entities[id].ai.attack_priority;
            let team_victim = if common {
                self.team_attack_target(id)
            } else {
                None
            };
            let victim = if team_victim.is_some() && info.is_none() {
                team_victim
            } else {
                let mut victim = self.find_hunt_enemy(id, info);
                if victim.is_none() && self.players[self.entities[id].owner].units_should_hunt {
                    victim = self.find_hunt_enemy(id, None);
                }
                if common {
                    if let Some(team_victim) = team_victim.filter(|_| info.is_some())
                        && (victim.is_none()
                            || self.native_attack_priority(info, team_victim)
                                >= victim
                                    .map_or(0, |victim| self.native_attack_priority(info, victim)))
                    {
                        victim = Some(team_victim);
                    }
                    if let Some(team) = self.team_membership.team_of(id) {
                        self.set_team_attack_target(team, victim);
                    }
                }
                victim
            };
            self.entities[id].ai.hunt.as_mut().expect("Hunt state").goal = victim;
            if matches!(
                self.entities[id]
                    .ai
                    .hunt
                    .as_ref()
                    .expect("Hunt state")
                    .inner,
                Inner::Idle { .. }
            ) {
                if let Some(victim) = victim {
                    self.entities[id]
                        .ai
                        .hunt
                        .as_mut()
                        .expect("Hunt state")
                        .inner = Inner::Attack;
                    let active = self.enter_native_attack(id, Goal::Object(victim));
                    if self.entities[id].ai.hunt.is_none() {
                        return false;
                    }
                    if !active {
                        self.enter_hunt_idle(id);
                    }
                } else if !self.players[self.entities[id].owner].units_should_hunt {
                    self.set_ai_order(id, Order::Idle);
                    return false;
                }
            }
        }
        true
    }

    fn update_hunt_inner(&mut self, id: usize, now: u32) {
        let state = self.entities[id].ai.hunt.as_ref().expect("Hunt state");
        match state.inner {
            Inner::Attack => {
                let goal = state.goal;
                let active = goal.is_some_and(|victim| {
                    self.retarget_native_attack(id, Goal::Object(victim));
                    self.update_native_attack(id, Goal::Object(victim))
                });
                if self.entities[id].ai.hunt.is_none() {
                    return;
                }
                if !active {
                    self.enter_hunt_idle(id);
                }
            }
            Inner::Idle {
                initialized,
                sleep_until,
            } => {
                if now < sleep_until {
                    return;
                }
                if !initialized {
                    // AIIdle::doInitIdleState: the outer machine is Hunt, so
                    // ai->isIdle() is false and the ground-grid snap is skipped.
                    self.entities[id].ai.motion.clear_goal();
                    self.set_attack_current_victim(id, None);
                    self.entities[id].ai.idle.needs_init = false;
                }
                let offset = self.entities[id].ai.idle.sleep_offset;
                self.entities[id].ai.idle.sleep_offset = 0;
                self.entities[id]
                    .ai
                    .hunt
                    .as_mut()
                    .expect("Hunt state")
                    .inner = Inner::Idle {
                    initialized: true,
                    sleep_until: now.wrapping_add(60 + offset),
                };
            }
        }
    }
}

impl Skirmish {
    /// Player.cpp1975: the native flag persists independently of each AI machine.
    pub(in crate::skirmish) fn set_player_units_should_hunt(
        &mut self,
        owner: usize,
        hunt: bool,
        source: i32,
    ) {
        self.players[owner].units_should_hunt = hunt;
        // getMostValuableLocation writes a local position which this source
        // function never reads. It has no mutation, RNG or callback side effects.
        for id in self.team_membership.player_objects(owner) {
            if ["DOZER", "HARVESTER", "IGNORES_SELECT_ALL"]
                .iter()
                .any(|kind| self.attack_kind(id, kind))
            {
                continue;
            }
            self.entities[id].ai.command.ai_group = None;
            if self.entities[id].ai.registration.is_some() {
                if hunt {
                    self.native_hunt_from(id, source);
                } else {
                    self.native_idle_from(id, source);
                }
            }
        }
    }
}
