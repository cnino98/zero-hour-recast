//! `AssaultTransportAIUpdate` at SH 2bfaea60; inactive centroid code is omitted.
use super::{
    Order, Skirmish,
    ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_PLAYER},
    combat::attack::Goal,
};

struct Member {
    id: usize,
    new: bool,
}

pub(super) struct State {
    members: Vec<Member>,
    target: Option<usize>,
    attack_move: Option<[f32; 3]>,
    attack_object: bool,
    new_occupants: bool,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            members: Vec::new(),
            target: None,
            attack_move: None,
            attack_object: false,
            new_occupants: false,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_assault_module(&mut self, id: usize, registration: usize) {
        self.construct_ai_module(id, registration);
        assert!(self.entities[id].assault.is_none());
        self.entities[id].assault = Some(Box::new(State::new()));
    }

    pub(in crate::skirmish) fn update_assault_module(&mut self, id: usize) -> i32 {
        if self.entities[id].is_effectively_dead() {
            self.assault_final_orders(id);
            return i32::try_from(crate::skirmish::module_updates::FOREVER).expect("native sleep");
        }
        if !self.update_assault_transport(id) {
            self.update_ai_module(id);
        }
        1
    }

    pub(in crate::skirmish) fn assault_command_from(
        &mut self,
        id: usize,
        command: Order,
        world: Option<[f32; 3]>,
        source: i32,
        forced: bool,
    ) {
        if source == COMMAND_FROM_AI {
            return;
        }
        let Some(mut state) = self.entities[id].assault.take() else {
            return;
        };
        if matches!(command, Order::Idle) {
            state.target = None;
            self.retrieve_assault_members(id, &state);
        }
        let mut reset = State::new();
        match command {
            Order::AttackMove(_) => reset.attack_move = world,
            Order::Attack(_) if !forced => reset.attack_object = true,
            _ => {}
        }
        self.entities[id].assault = Some(Box::new(reset));
    }

    // Actual player/script commands call this; internal state transitions do not.
    pub(super) fn assault_command(&mut self, id: usize, command: Order) {
        let world = self.entities[id].ai.command.goal_position;
        let forced = self.entities[id].ai.attack_context.forced;
        self.assault_command_from(id, command, Some(world), COMMAND_FROM_PLAYER, forced);
    }

    pub(in crate::skirmish) fn begin_assault_target(&mut self, id: usize, target: Option<usize>) {
        if target.is_some()
            && let Some(state) = &mut self.entities[id].assault
        {
            state.target = target;
        }
    }

    // Returns true only for the native early isAttackPointless return, which
    // skips the base AI update for this frame.
    pub(super) fn update_assault_transport(&mut self, id: usize) -> bool {
        let Some(mut state) = self.entities[id].assault.take() else {
            return false;
        };
        self.refresh_assault_members(id, &mut state);
        let attacking = self.entities[id].weapon_set.status.attacking;
        if attacking && state.members.iter().all(|member| member.new) {
            self.ai_idle_from_ai(id);
            self.entities[id].assault = Some(state);
            return true;
        }
        let target = state.target.filter(|&target| {
            self.entities[target].exists() && !self.entities[target].is_effectively_dead()
        });
        if let Some(target) = target {
            self.order_assault_members(id, &state, target);
        } else if let Some(goal) = state.attack_move {
            if !matches!(self.entities[id].ai.order, Order::AttackMove(_)) {
                self.native_attack_move_from(id, goal, i32::MAX.unsigned_abs(), COMMAND_FROM_AI);
            }
        } else if state.attack_object {
            self.retrieve_assault_members(id, &state);
        }
        self.entities[id].assault = Some(state);
        false
    }

    fn refresh_assault_members(&mut self, carrier: usize, state: &mut State) {
        let mut index = 0;
        while index < state.members.len() {
            let id = state.members[index].id;
            if !self.entities[id].exists()
                || self.entities[id].is_effectively_dead()
                || self.entities[id].ai.command.last_command_source != COMMAND_FROM_AI
            {
                if self.entities[id].ai.registration.is_some() {
                    self.entities[id].ai.command.allowed_to_chase = false;
                }
                state.members.swap_remove(index);
            }
            // Native increments after swapping the last member into this slot.
            index += 1;
        }
        for index in 0..self.passengers(carrier).len() {
            let id = self.passengers(carrier)[index];
            if state.members.len() < 10 && !state.members.iter().any(|member| member.id == id) {
                if self.entities[id].ai.registration.is_some() {
                    self.entities[id].ai.command.allowed_to_chase = true;
                }
                state.members.push(Member {
                    id,
                    new: state.new_occupants,
                });
            }
        }
        if self.catalog[self.entities[carrier].type_index]
            .contain_family()
            .is_some()
        {
            state.new_occupants = true;
        }
    }

    fn order_assault_members(&mut self, carrier: usize, state: &State, target: usize) {
        let threshold = self.catalog[self.entities[carrier].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.assault_heal_ratio)
            .expect("assault AI module");
        for member in &state.members {
            let id = member.id;
            let actor = &self.entities[id];
            if !actor.exists() || actor.ai.registration.is_none() {
                continue;
            }
            let contained = actor.contained_by().is_some();
            let wounded = actor.health / actor.max_health < threshold;
            if contained
                && actor.health.partial_cmp(&actor.max_health) == Some(std::cmp::Ordering::Equal)
                && !member.new
            {
                self.native_ai_exit(id, carrier);
            }
            if !contained {
                if wounded {
                    if !matches!(self.entities[id].ai.order, Order::Enter(_)) {
                        self.native_enter_from(id, carrier, COMMAND_FROM_AI);
                    }
                } else if (matches!(self.entities[id].ai.order, Order::Idle)
                    || (!self.entities[id].ai.motion.is_moving()
                        && !self.entities[id].ai.motion.has_goal()))
                    && self.entities[id].ai.command.goal_object != Some(target)
                {
                    self.ai_attack_goal(id, Goal::Object(target), i32::MAX.unsigned_abs());
                }
            }
        }
    }

    fn retrieve_assault_members(&mut self, carrier: usize, state: &State) {
        for member in &state.members {
            let id = member.id;
            if self.entities[id].exists()
                && self.entities[id].ai.registration.is_some()
                && self.entities[id].contained_by().is_none()
                && !matches!(self.entities[id].ai.order, Order::Enter(_))
            {
                self.native_enter_from(id, carrier, COMMAND_FROM_AI);
            }
        }
    }

    pub(super) fn assault_final_orders(&mut self, id: usize) {
        let Some(state) = self.entities[id].assault.as_ref() else {
            return;
        };
        let target = state
            .target
            .filter(|&target| self.entities[target].exists());
        let attack_object = state.attack_object;
        let goal = state.attack_move;
        let members: Vec<_> = state.members.iter().map(|member| member.id).collect();
        for rider in members {
            if !self.entities[rider].exists() || self.entities[rider].ai.registration.is_none() {
                continue;
            }
            if attack_object && let Some(target) = target {
                self.native_attack_goal_from(
                    rider,
                    Goal::Object(target),
                    i32::MAX.unsigned_abs(),
                    COMMAND_FROM_PLAYER,
                    false,
                );
            } else if let Some(goal) = goal {
                self.native_attack_move_from(
                    rider,
                    goal,
                    i32::MAX.unsigned_abs(),
                    COMMAND_FROM_PLAYER,
                );
            }
            self.entities[rider].ai.command.allowed_to_chase = false;
        }
    }
}

#[cfg(test)]
mod tests;
