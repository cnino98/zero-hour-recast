//! SH AIStates.cpp: `AttackStateMachine` and `AIAttackState`/Aim/Fire lifetimes.
use crate::skirmish::{Order, Skirmish, ai_dispatch::WeaponStatus, locomotor, weapons::Lock};

mod movement;
#[cfg(test)]
mod tests;

// Goals are copied into the independently owned body and turret machines.
#[derive(Clone, Copy)]
pub(in crate::skirmish) enum Goal {
    Object(usize),
    Position([f32; 3]),
}

pub(in crate::skirmish) enum Phase {
    Aim,
    Fire,
    Pursue,
    Approach,
    Carrier,
    Immobile,
}

pub(in crate::skirmish) enum Return {
    Continue,
    Success,
    Failure,
}

pub(in crate::skirmish) struct State {
    pub goal: Goal,
    phase: Phase,
    locked_weapon: Option<(usize, u64)>,
    victim_team: Option<usize>,
    original_position: [f32; 3],
    forced: bool,
    set_orientation: bool,
    movement: Movement,
}

struct Movement {
    previous_position: [f32; 3],
    approach_frame: u32,
    waiting: bool,
    initial_approach: bool,
    initial_pursuit: bool,
    goal: [f32; 3],
}

impl Skirmish {
    /// `AIUpdateInterface::transferAttack` preserves primary and nested state lifetimes.
    pub(in crate::skirmish) fn transfer_native_attack(
        &mut self,
        id: usize,
        from: usize,
        to: usize,
    ) {
        if self.entities[id].ai.current_victim == Some(from) {
            self.entities[id].ai.current_victim = Some(to);
        }
        if self.entities[id].ai.command.goal_object == Some(from)
            && !self.entities[id].ai.command.locked
            && self.entities[from].exists()
        {
            self.set_ai_goal_object(id, to);
            if matches!(self.entities[id].ai.order, Order::Attack(target) if target == from) {
                self.entities[id].ai.order = Order::Attack(to);
                // AIAttackState copies its enclosing machine's goal before updating
                // the child; this changes no entry weapon, victim team or deadline.
                self.retarget_native_attack(id, Goal::Object(to));
            }
        }
        self.transfer_turret_targets(id, from, to);
    }

    /// A parent submachine can change its goal without re-entering `AIAttackState`.
    /// AIStates.cpp5668 preserves the entry weapon, original position and victim team.
    pub(in crate::skirmish) fn retarget_native_attack(&mut self, id: usize, goal: Goal) {
        if let Some(state) = &mut self.entities[id].ai.attack {
            state.goal = goal;
        }
    }

    pub(in crate::skirmish) fn update_native_attack(&mut self, id: usize, goal: Goal) -> bool {
        if self.entities[id]
            .ai
            .attack
            .as_ref()
            .is_some_and(|state| !same_goal(state.goal, goal))
        {
            self.exit_native_attack(id);
        }
        let revision = self.entities[id].ai.command_revision;
        let Some(mut state) = self.entities[id].ai.attack.take() else {
            return self.enter_native_attack(id, goal);
        };
        if !self.native_attack_outer_valid(id, &state) {
            self.entities[id].ai.attack = Some(state);
            self.exit_native_attack(id);
            return false;
        }
        if let Goal::Object(target) = state.goal {
            self.set_attack_current_victim(id, Some(target));
        }
        let result = match state.phase {
            Phase::Aim => self.update_attack_aim(id, &mut state),
            Phase::Fire => {
                // Payload callbacks can issue commands or delete the source. Keep
                // the old state owned by AI so those callbacks run its real exit.
                let goal = state.goal;
                let original = state.original_position;
                self.entities[id].ai.attack = Some(state);
                let result = self.update_attack_fire(id, goal, None, Some(original));
                if self.entities[id].ai.command_revision != revision {
                    return true;
                }
                let Some(retained) = self.entities[id].ai.attack.take() else {
                    return false;
                };
                state = retained;
                result
            }
            Phase::Pursue => self.update_attack_movement(id, &mut state, true),
            Phase::Approach => self.update_attack_movement(id, &mut state, false),
            Phase::Carrier => Return::Continue,
            Phase::Immobile => Return::Failure,
        };
        // transferAttack/continueAttackRange may change the primary goal during fire.
        if let Goal::Object(_) = state.goal
            && let Order::Attack(target) = self.entities[id].ai.order
        {
            state.goal = Goal::Object(target);
        }
        let active = self.entities[id].exists()
            && !self.entities[id].is_effectively_dead()
            && self.resolve_attack_transition(id, &mut state, result);
        self.entities[id].ai.attack = Some(state);
        if !active {
            self.exit_native_attack(id);
        }
        active
    }

    pub(in crate::skirmish) fn enter_native_attack(&mut self, id: usize, goal: Goal) -> bool {
        if !self.attack_mood_allows(id)
            || !self.entities[id].complete()
            || self.attack_out_of_ammo(id)
        {
            return false;
        }
        let victim_team = match goal {
            Goal::Object(target) => {
                if !self
                    .entities
                    .get(target)
                    .is_some_and(|victim| victim.exists() && !victim.is_effectively_dead())
                {
                    self.notify_attack_victim_dead(id);
                    return false;
                }
                self.team_membership.team_of(target)
            }
            Goal::Position(_) => None,
        };
        if !self.choose_attack_weapon(id, goal) {
            return false;
        }
        let actor = &self.entities[id];
        let slot = actor.weapon_slot();
        let locked_weapon = (!matches!(actor.weapon_set.lock, Lock::None))
            .then_some((slot, actor.weapon_set.allocation_generation[slot]));
        let forced = actor.ai.attack_context.forced;
        let original_position = self.attack_goal_position(goal);
        self.entities[id].weapon_set.weapons[slot].max_shots = None;
        if self
            .allocated_weapon(id, slot)
            .is_some_and(|weapon| weapon.continue_attack_range > 0.0)
        {
            self.entities[id].ai.attack_context.ignoring_stealth = true;
        }
        let mut state = State {
            goal,
            phase: Phase::Aim,
            locked_weapon,
            victim_team,
            original_position,
            forced,
            set_orientation: false,
            movement: Movement {
                previous_position: [0.0; 3],
                approach_frame: 0_u32.wrapping_sub(10),
                waiting: false,
                initial_approach: true,
                initial_pursuit: true,
                goal: original_position,
            },
        };
        if let Goal::Object(target) = goal {
            self.begin_continuing_attack(id, target);
        }
        let result = self.enter_attack_phase(id, &mut state);
        let active = self.resolve_attack_transition(id, &mut state, result);
        self.entities[id].ai.attack = Some(state);
        if active {
            self.entities[id].weapon_set.status.attacking = true;
        } else {
            self.exit_native_attack(id);
        }
        active
    }

    fn native_attack_outer_valid(&mut self, id: usize, state: &State) -> bool {
        if self.attack_out_of_ammo(id) {
            return false;
        }
        if let Goal::Object(target) = state.goal {
            if !self
                .entities
                .get(target)
                .is_some_and(|victim| victim.exists() && !victim.is_effectively_dead())
            {
                self.notify_attack_victim_dead(id);
                return false;
            }
            if self.team_membership.team_of(target) != state.victim_team
                && self.object_relationship(id, target) != 0
            {
                if let Some(team) = self.team_membership.team_of(id)
                    && self.team_membership.common_target(team) == Some(target)
                {
                    self.team_membership.set_common_target(team, None);
                }
                self.entities[id].ai.command.goal_object = None;
                self.notify_attack_victim_dead(id);
                return false;
            }
        }
        if !self.choose_attack_weapon(id, state.goal) {
            return false;
        }
        let actor = &self.entities[id];
        let slot = actor.weapon_slot();
        state
            .locked_weapon
            .is_none_or(|locked| locked == (slot, actor.weapon_set.allocation_generation[slot]))
            && actor.weapon_set.weapons[slot].max_shots != Some(0)
    }

    fn choose_attack_weapon(&mut self, id: usize, goal: Goal) -> bool {
        let chosen = match goal {
            Goal::Object(target) => self.choose_weapon(id, target),
            Goal::Position(_) => {
                if matches!(self.entities[id].weapon_set.lock, Lock::None) {
                    self.entities[id].weapon_set.current = 0;
                }
                self.allocated_weapon(id, self.entities[id].weapon_slot())
                    .is_some()
            }
        };
        self.update_weapon_status(id);
        chosen
    }

    fn attack_out_of_ammo(&mut self, id: usize) -> bool {
        !self.attack_kind(id, "PROJECTILE") && self.native_actor_out_of_ammo(id)
    }

    pub(in crate::skirmish) fn attack_mood_allows(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        !self.computer_player(actor.owner)
            || actor.ai.mood.attitude != -2
            || self.attack_kind(id, "INFANTRY") && self.attack_kind(id, "IGNORED_IN_GUI")
    }

    pub(in crate::skirmish) fn attack_kind(&self, id: usize, kind: &str) -> bool {
        self.catalog[self.entities[id].type_index]
            .kind_of
            .iter()
            .any(|value| value == kind)
    }

    pub(in crate::skirmish) fn attack_goal_position(&self, goal: Goal) -> [f32; 3] {
        match goal {
            Goal::Object(target) => self.object_position(target),
            Goal::Position(position) => position,
        }
    }

    fn resolve_attack_transition(
        &mut self,
        id: usize,
        state: &mut State,
        mut result: Return,
    ) -> bool {
        // State::friend_checkForTransitions bounds recursive immediate transitions at twenty.
        for _ in 0..19 {
            let next = match result {
                Return::Success => match state.phase {
                    Phase::Aim => Some(Phase::Fire),
                    Phase::Fire | Phase::Approach => Some(Phase::Aim),
                    Phase::Pursue => Some(Phase::Approach),
                    Phase::Carrier | Phase::Immobile => return false,
                },
                Return::Failure => match state.phase {
                    Phase::Fire => Some(Phase::Aim),
                    Phase::Pursue => Some(Phase::Approach),
                    Phase::Aim | Phase::Approach | Phase::Carrier | Phase::Immobile => {
                        return false;
                    }
                },
                Return::Continue => {
                    match state.phase {
                        Phase::Aim | Phase::Fire => {
                            if self.attack_out_of_range(id, state.goal) {
                                Some(self.attack_chase_phase(id, state.goal))
                            } else if matches!(state.goal, Goal::Object(_)) {
                                let target = match state.goal {
                                    Goal::Object(target) => target,
                                    Goal::Position(_) => unreachable!(),
                                };
                                if !state.forced && self.attack_wants_crush(id, target) {
                                    Some(self.attack_chase_phase(id, state.goal))
                                } else if !self.attack_continued_legal(id, state.goal, state.forced)
                                {
                                    return false;
                                } else if state.forced && self.attack_wants_crush(id, target) {
                                    Some(self.attack_chase_phase(id, state.goal))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        Phase::Carrier => {
                            // inWeaponRangeObject succeeds for a null object goal.
                            if matches!(state.goal, Goal::Position(_))
                                || !self.attack_out_of_range(id, state.goal)
                            {
                                Some(Phase::Aim)
                            } else {
                                None
                            }
                        }
                        Phase::Pursue | Phase::Approach => None,
                        Phase::Immobile => return false,
                    }
                }
            };
            let Some(next) = next else {
                return true;
            };
            self.exit_attack_phase(id, state);
            state.phase = next;
            result = self.enter_attack_phase(id, state);
        }
        false
    }

    fn attack_chase_phase(&self, id: usize, goal: Goal) -> Phase {
        if self.attack_kind(id, "IMMOBILE") {
            return Phase::Immobile;
        }
        if self.attack_kind(id, "PORTABLE_STRUCTURE") && self.attack_kind(id, "CAN_ATTACK") {
            Phase::Carrier
        } else if matches!(goal, Goal::Object(_)) {
            Phase::Pursue
        } else {
            Phase::Approach
        }
    }

    fn enter_attack_phase(&mut self, id: usize, state: &mut State) -> Return {
        match state.phase {
            Phase::Aim => {
                if let Goal::Object(target) = state.goal {
                    if !self.prepare_garrison_attack(id, target) {
                        return Return::Failure;
                    }
                    self.add_attack_targeter(target, id);
                }
                let turret = self.current_weapon_turret(id);
                self.set_attack_turret_targets(id, state.goal, state.forced);
                if turret.is_none() && !self.catalog[self.entities[id].type_index].turrets_linked {
                    let actor = &self.entities[id];
                    if actor
                        .active_weapon(&self.catalog[actor.type_index])
                        .contact()
                        && self.attack_goal_in_range(id, state.goal)
                        && !self.attack_goal_prevents_aim(id, state.goal)
                    {
                        return Return::Success;
                    }
                }
                state.set_orientation = false;
                self.entities[id].weapon_set.status.aiming = true;
                Return::Continue
            }
            Phase::Fire => self.enter_attack_fire(id, state.goal),
            Phase::Pursue | Phase::Approach => {
                if self.attack_kind(id, "IMMOBILE") {
                    return Return::Failure;
                }
                self.enter_attack_movement(id, state)
            }
            Phase::Carrier => Return::Continue,
            Phase::Immobile => Return::Failure,
        }
    }

    fn exit_attack_phase(&mut self, id: usize, state: &mut State) {
        match state.phase {
            Phase::Aim => {
                if state.set_orientation {
                    self.entities[id].ai.motion.clear_goal();
                }
                self.entities[id].weapon_set.status.aiming = false;
            }
            Phase::Fire => self.exit_attack_fire(id),
            Phase::Pursue | Phase::Approach => self.exit_attack_movement(id, state),
            Phase::Carrier | Phase::Immobile => {}
        }
    }

    fn update_attack_aim(&mut self, id: usize, state: &mut State) -> Return {
        let slot = self.entities[id].weapon_slot();
        if self.allocated_weapon(id, slot).is_none() {
            return Return::Failure;
        }
        if let Goal::Object(target) = state.goal
            && !self
                .entities
                .get(target)
                .is_some_and(|victim| victim.exists() && !victim.is_effectively_dead())
        {
            return Return::Failure;
        }
        if let Some(turret) = self.current_weapon_turret(id) {
            self.set_native_turret_target(id, turret, Some(state.goal), state.forced, false);
            if self.turret_rules(id, turret).turn_rate != 0.0 {
                return Return::Continue;
            }
        }
        let position = self.attack_goal_position(state.goal);
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let tolerance = actor.active_weapon(unit).aim_delta.max(0.035);
        let relative =
            super::relative_angle(actor.position, actor.facing, [position[0], position[1]]);
        let can_turn = actor.active_locomotor(unit).ground.as_ref().map_or_else(
            || {
                actor
                    .active_locomotor(unit)
                    .flight
                    .as_ref()
                    .is_some_and(|rules| rules.min_speed == 0.0)
            },
            |rules| rules.min_speed == 0.0,
        );
        if can_turn {
            if relative.abs() > tolerance {
                let desired = self.entities[id].facing + relative;
                self.entities[id].ai.motion.request_angle(desired);
                state.set_orientation = true;
            }
        } else if actor.active_locomotor(unit).flight.is_some() {
            self.aim_moving_flight(id, position);
        } else {
            let delta = [
                position[0] - actor.position[0],
                position[1] - actor.position[1],
            ];
            let squared = delta.map(|component| component * component);
            let distance = (squared[0] + squared[1]).sqrt();
            self.entities[id]
                .ai
                .motion
                .request_path(locomotor::GroundGoal {
                    height: position[2],
                    position: [position[0], position[1]],
                    distance,
                    desired_speed: f32::MAX,
                    no_slow_down: false,
                    ultra_accurate: false,
                });
        }
        if relative.abs() < tolerance {
            if let Goal::Object(target) = state.goal {
                self.add_attack_targeter(target, id);
            }
            if !self.attack_goal_prevents_aim(id, state.goal) {
                return Return::Success;
            }
        }
        if self.object_disabled_mask(id) & (1 << 3) != 0
            && !self.attack_goal_in_range(id, state.goal)
        {
            return Return::Failure;
        }
        Return::Continue
    }

    pub(in crate::skirmish) fn attack_goal_prevents_aim(&self, id: usize, goal: Goal) -> bool {
        match goal {
            Goal::Object(target) => self.prevents_attack_aim(target, id),
            Goal::Position(_) => false,
        }
    }

    pub(in crate::skirmish) fn enter_attack_fire(&mut self, id: usize, goal: Goal) -> Return {
        if !self.attack_mood_allows(id) {
            return Return::Failure;
        }
        let target = match goal {
            Goal::Object(target) => Some(target),
            Goal::Position(_) => None,
        };
        if let Some(target) = target
            && self.team_attacks_common_target(id)
            && self.team_attack_target(id).is_none()
            && let Some(team) = self.team_membership.team_of(id)
        {
            self.set_team_attack_target(team, Some(target));
        }
        self.entities[id].weapon_set.status.firing = true;
        self.prefire_native_weapon(id, target);
        Return::Continue
    }

    pub(in crate::skirmish) fn update_attack_fire(
        &mut self,
        id: usize,
        goal: Goal,
        turret: Option<usize>,
        original_position: Option<[f32; 3]>,
    ) -> Return {
        if let Goal::Object(target) = goal
            && !self
                .entities
                .get(target)
                .is_some_and(|victim| victim.exists() && !victim.is_effectively_dead())
        {
            return Return::Failure;
        }
        let actor = &self.entities[id];
        let slot = actor.weapon_slot();
        let index = actor.type_index;
        let now = self.update_frame();
        match self.entities[id].weapon_status(slot, &self.catalog[index], now) {
            Some(WeaponStatus::PreAttack) => return Return::Continue,
            Some(WeaponStatus::Ready) => {}
            _ => return Return::Failure,
        }
        if turret
            .is_some_and(|turret| self.turret_rules(id, turret).controlled_slots & (1 << slot) == 0)
        {
            return Return::Failure;
        }
        let revision = self.entities[id].ai.command_revision;
        // setFiringConditionForCurrentWeapon precedes payload callbacks/FX.
        self.entities[id].weapon_set.status.condition[slot] =
            Some(["FIRING_A", "FIRING_B", "FIRING_C"][slot]);
        match goal {
            Goal::Object(target) => {
                self.fire_weapon(id, target);
                if self.entities[id].ai.command_revision != revision || !self.entities[id].exists()
                {
                    return Return::Continue;
                }
                if let Some(original) = original_position {
                    self.continue_disarm_attack(id, target, original);
                }
                self.reset_garrison_aim(id);
            }
            Goal::Position(position) => {
                if self.catalog[index].turrets_linked {
                    for linked in 0..3 {
                        if self.allocated_weapon(id, linked).is_some() {
                            let binding = crate::skirmish::WeaponBinding {
                                type_index: index,
                                weapon_index: self.entities[id].weapon_index(linked),
                                weapon_slot: linked,
                            };
                            let target = crate::skirmish::damage::WeaponTarget {
                                object: None,
                                position,
                                projectile_position: position,
                                scatter_radius: 0.0,
                            };
                            let reloaded = self.fire_allocated_weapon_payload(id, binding, target);
                            if self.entities[id].ai.command_revision != revision
                                || !self.entities[id].exists()
                            {
                                return Return::Continue;
                            }
                            if reloaded
                                && matches!(self.entities[id].weapon_set.lock, Lock::Temporary)
                            {
                                self.entities[id].weapon_set.lock = Lock::None;
                            }
                            self.notify_firing_tracker_shot(id, binding.weapon_index, None);
                        }
                    }
                } else {
                    self.fire_position_weapon(id, position);
                }
            }
        }
        self.entities[id].ai.attack_context.ignoring_stealth = false;
        Return::Success
    }

    pub(in crate::skirmish) fn exit_attack_fire(&mut self, id: usize) {
        let actor = &mut self.entities[id];
        actor.weapon_set.status.firing = false;
        actor.ai.attack_context.ignoring_stealth = false;
        let slot = actor.weapon_slot();
        actor.weapon_set.weapons[slot].pre_attack = None;
    }

    pub(in crate::skirmish) fn exit_native_attack(&mut self, id: usize) {
        let Some(mut state) = self.entities[id].ai.attack.take() else {
            return;
        };
        self.exit_attack_phase(id, &mut state);
        let actor = &mut self.entities[id];
        actor.weapon_set.status.firing = false;
        actor.weapon_set.status.aiming = false;
        actor.weapon_set.status.attacking = false;
        actor.ai.attack_context.ignoring_stealth = false;
        for weapon in &mut actor.weapon_set.weapons {
            weapon.leech_range_active = false;
        }
        self.set_attack_current_victim(id, None);
        for turret in 0..2 {
            if self.has_native_turret(id, turret) {
                self.set_native_turret_target(id, turret, None, false, false);
            }
        }
        self.entities[id].ai.command.goal_object = None;
    }

    pub(in crate::skirmish) fn set_attack_current_victim(
        &mut self,
        id: usize,
        target: Option<usize>,
    ) {
        if target.is_none()
            && let Some(previous) = self.entities[id].ai.current_victim
        {
            self.remove_attack_targeter(previous, id);
        }
        self.entities[id].ai.current_victim = target;
    }

    pub(in crate::skirmish) fn attack_continued_legal(
        &mut self,
        id: usize,
        goal: Goal,
        forced: bool,
    ) -> bool {
        let target = match goal {
            Goal::Object(target) => Some(target),
            Goal::Position(_) => None,
        };
        self.object_can_attack(id)
            && matches!(
                self.attack_target_result(
                    id,
                    target,
                    self.attack_goal_position(goal),
                    forced,
                    true,
                    None
                ),
                super::targeting::Result::Possible | super::targeting::Result::PossibleAfterMoving
            )
    }

    pub(in crate::skirmish) fn attack_goal_in_range(&self, id: usize, goal: Goal) -> bool {
        match goal {
            Goal::Object(target) => self.within_attack_range(id, target),
            Goal::Position(position) => {
                let actor = &self.entities[id];
                let unit = &self.catalog[actor.type_index];
                let weapon = actor.active_weapon(unit);
                let squared = super::circle_distance_squared(
                    actor.position,
                    [position[0], position[1]],
                    self.object_radius(id),
                );
                let range = actor.weapon_range(unit, self.passenger_bonus_flags(id));
                squared >= weapon.min_range * weapon.min_range && squared <= range * range
            }
        }
    }

    pub(in crate::skirmish) fn attack_ground_movement(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        // JetAIUpdate overrides the base query even while taxiing.
        if unit.aircraft.is_some() {
            return false;
        }
        if actor.unmanned() && self.attack_kind(id, "PRODUCED_AT_HELIPAD") {
            return true;
        }
        if actor.locomotor_surfaces(unit) == 8 || actor.ai.surface_motion.is_empty() {
            return false;
        }
        if actor.active_locomotor(unit).legal_surfaces & 8 != 0 {
            return false;
        }
        if self.object_disabled_mask(id) & (1 << 3) != 0 {
            return false;
        }
        !(self.height_above_ground(id) > 0.0 && actor.physics_allows_fall())
    }

    pub(in crate::skirmish) fn attack_out_of_range(&self, id: usize, goal: Goal) -> bool {
        let actor = &self.entities[id];
        let object = matches!(goal, Goal::Object(_));
        if object && actor.weapon_set.weapons[actor.weapon_slot()].leech_range_active {
            return false;
        }
        if !self.attack_goal_in_range(id, goal) {
            return true;
        }
        let weapon = actor.active_weapon(&self.catalog[actor.type_index]);
        if object && weapon.contact() {
            return false;
        }
        let on_ground = self.attack_ground_movement(id)
            || self.attack_kind(id, "IMMOBILE")
            || self.attack_kind(id, "SPAWNS_ARE_THE_WEAPONS")
            || actor.contained_by().is_some_and(|carrier| {
                self.attack_kind(carrier, "STRUCTURE")
                    || !self.entities[carrier].is_airborne_target()
            });
        let target = match goal {
            Goal::Object(target) => Some(target),
            Goal::Position(_) => None,
        };
        on_ground
            && target.is_none_or(|target| self.height_above_ground(target) <= -9.0 * self.gravity)
            && self.attack_view_blocked(
                id,
                target,
                self.object_position(id),
                self.attack_goal_position(goal),
            )
    }

    fn attack_wants_crush(&self, id: usize, target: usize) -> bool {
        self.ai_rules.crushes_infantry
            && self.computer_player(self.entities[id].owner)
            && self.entities[target].contained_by().is_none()
            && self.current_weapon_turret(id).is_some()
            && !self.entities[id].unmanned()
            && self.can_crush(id, target)
            && !self.attack_kind(id, "DONT_AUTO_CRUSH_INFANTRY")
    }
}

fn same_goal(left: Goal, right: Goal) -> bool {
    match (left, right) {
        (Goal::Object(left), Goal::Object(right)) => left == right,
        (Goal::Position(left), Goal::Position(right)) => left
            .iter()
            .zip(right)
            .all(|(a, b)| a.partial_cmp(&b) == Some(std::cmp::Ordering::Equal)),
        _ => false,
    }
}
