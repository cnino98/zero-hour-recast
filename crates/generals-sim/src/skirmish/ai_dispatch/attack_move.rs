//! SH AIStates.cpp1994,3519–3693: primary Move and independent nested Attack.
use crate::skirmish::{
    ModelConditions, Order, Skirmish, ai_goals::COMMAND_FROM_AI, combat::attack::Goal,
    ground_movement::MoveResult,
};

pub(in crate::skirmish) struct State {
    active: bool,
    source: i32,
    retries: u8,
    sleep_until: u32,
}
impl State {
    pub(super) const fn new() -> Self {
        Self {
            active: false,
            source: COMMAND_FROM_AI,
            retries: 5,
            sleep_until: 0,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn native_attack_move_from(
        &mut self,
        id: usize,
        mut position: [f32; 3],
        shots: u32,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            crate::skirmish::ai_dispatch::stored_command::Command {
                source,
                kind: crate::skirmish::ai_dispatch::stored_command::Kind::AttackMove {
                    position,
                    shots,
                },
            },
        ) {
            return;
        }
        // Assault's aiDoCommand override observes even commands rejected by the base.
        self.assault_command_from(
            id,
            Order::AttackMove([position[0], position[1]]),
            Some(position),
            source,
            false,
        );
        if !self.native_ai_command_allowed(id)
            || self.entities[id].is_ai_dead()
            || self.attack_kind(id, "IMMOBILE")
            || self.object_disabled(id)
        {
            return;
        }
        position = self.clipped_ai_goal_position(id, position, source);
        self.set_order_state_from(
            id,
            Order::AttackMove([position[0], position[1]]),
            false,
            Some(position),
            Some(source),
        );
        let slot = self.entities[id].weapon_slot();
        if self.allocated_weapon(id, slot).is_some() {
            self.entities[id].weapon_set.weapons[slot].max_shots = Some(shots);
        }
    }

    pub(in crate::skirmish) fn reset_nested_attack_idle(&mut self, id: usize) {
        // AIStates.cpp3528/4393: attack-move and both attack-follow children
        // enter Idle during construction. Their parents skip child updates while
        // idle and reset Idle before entering, so no child offset survives to
        // an update. The onEnter RNG and mood reset remain observable.
        self.random_between(60);
        let now = self.update_frame();
        let ai = &mut self.entities[id].ai;
        ai.mood.next_check = u64::from(now.wrapping_add(self.ai_rules.force_idle_frames));
        ai.mood.offset_check = true;
    }

    pub(in crate::skirmish) fn enter_attack_move(&mut self, id: usize) -> bool {
        self.exit_native_attack(id);
        self.reset_nested_attack_idle(id);
        let actor = &mut self.entities[id];
        actor.ai.attack_move.active = true;
        actor.ai.attack_move.source = actor.ai.command.last_command_source;
        actor.ai.attack_move.retries = 5;
        actor.ai.attack_move.sleep_until = 0;
        actor.ai.ai_target = None;
        actor.ai.command.move_to.position = actor.ai.command.goal_position;
        actor.ai.command.move_to.adjusts = true;
        !matches!(self.enter_primary_ground_move(id), MoveResult::Failure)
    }

    pub(in crate::skirmish) fn retarget_attack_move_goal(
        &mut self,
        id: usize,
        target: usize,
    ) -> bool {
        if !self.entities[id].ai.attack_move.active {
            return false;
        }
        self.entities[id].ai.ai_target = Some(target);
        true
    }

    pub(in crate::skirmish) fn exit_attack_move(&mut self, id: usize) {
        if !self.entities[id].ai.attack_move.active {
            return;
        }
        self.exit_native_attack(id);
        self.reset_nested_attack_idle(id);
        self.entities[id].ai.attack_move.active = false;
        self.entities[id].ai.ai_target = None;
    }

    pub(in crate::skirmish) fn update_attack_move(&mut self, id: usize, _goal: [f32; 2]) {
        if self.entities[id].aircraft.is_some() && self.jet_out_of_special_reload_ammo(id) {
            self.set_ai_order(id, Order::Idle);
            return;
        }
        let mut repath = false;
        let force = if let Some(target) = self.entities[id].ai.ai_target {
            self.entities[id].ai.motion.clear_goal();
            self.entities[id]
                .drawable
                .model_conditions
                .set_flag(ModelConditions::MOVING, false);
            let revision = self.entities[id].ai.command_revision;
            let active = self.update_attack(id, target);
            if self.entities[id].ai.command_revision != revision || active {
                return;
            }
            self.entities[id].ai.ai_target = None;
            self.reset_nested_attack_idle(id);
            let source = self.entities[id].ai.attack_move.source;
            self.set_last_command_source(id, source);
            self.entities[id].ai.command.from_ai = source == COMMAND_FROM_AI;
            repath = true;
            true
        } else {
            false
        };
        // checkForCrateToPickup clears m_crateCreated before findObjectByID in
        // this pinned source, so its pickup branch is unreachable (AIUpdate893).
        if let Some(target) = self.mood_target(id, false, force) {
            self.entities[id].ai.motion.end_move();
            self.entities[id].ai.motion.movement_active = false;
            self.entities[id].ai.attack_context.forced = false;
            if self.enter_native_attack(id, Goal::Object(target)) {
                self.entities[id].ai.ai_target = Some(target);
            } else {
                self.reset_nested_attack_idle(id);
            }
            self.set_last_command_source(id, COMMAND_FROM_AI);
            self.entities[id].ai.command.from_ai = true;
            return;
        }
        let now = self.update_frame();
        let deadline = self.entities[id].ai.attack_move.sleep_until;
        if deadline > now {
            return;
        }
        repath |= deadline == now;
        if repath {
            self.entities[id].ai.command.move_to.position =
                self.entities[id].ai.command.goal_position;
            self.entities[id].ai.command.move_to.adjusts = true;
            self.enter_primary_ground_move(id);
            let movement = &mut self.entities[id].ai.command.move_to;
            movement.path_goal = [-100.0; 3];
            movement.path_timestamp = 0_u32.wrapping_sub(10);
        }
        let result = self.update_primary_ground_move(id);
        if matches!(result, MoveResult::Continue) {
            return;
        }
        let actor = &self.entities[id];
        let goal = actor.ai.command.move_to.path_goal;
        let delta = [actor.position[0] - goal[0], actor.position[1] - goal[1]];
        let squared = delta.map(|value| value * value);
        if actor.ai.attack_move.retries == 0
            || squared[0] + squared[1] < (8.0 * crate::skirmish::CELL_SIZE).powi(2)
        {
            self.set_ai_order(id, Order::Idle);
        } else {
            let state = &mut self.entities[id].ai.attack_move;
            state.retries -= 1;
            state.sleep_until = now.wrapping_add(90);
        }
    }

    pub(in crate::skirmish) fn jet_out_of_special_reload_ammo(&mut self, id: usize) -> bool {
        let now = self.update_frame();
        let mut special = 0;
        let mut empty = 0;
        for slot in 0..3 {
            if !self.allocated_weapon(id, slot).is_some_and(|weapon| {
                matches!(weapon.reload, crate::skirmish::ClipReload::ReturnToBase)
            }) {
                continue;
            }
            special += 1;
            let actor = &mut self.entities[id];
            if matches!(
                actor.weapon_status(slot, &self.catalog[actor.type_index], now),
                Some(super::WeaponStatus::OutOfAmmo)
            ) {
                empty += 1;
            }
        }
        special > 0 && empty == special
    }
}

#[cfg(test)]
mod tests;
