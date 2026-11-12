//! SH AIUpdate.cpp1002–1150 and AIStates.cpp1313–1540, ordinary primary clock.
use crate::skirmish::{Order, Skirmish, ai_goals::MainState, module_updates::FOREVER};

pub(in crate::skirmish) struct State {
    pub(in crate::skirmish) initialized: bool,
    pub(in crate::skirmish) sleep_until: u32,
}

impl Skirmish {
    /// `AIUpdate::onObjectCreated` runs after authored module constructors.
    pub(in crate::skirmish) fn initialize_primary_ai(&mut self, id: usize) {
        assert!(
            !self.entities[id].ai.primary.initialized,
            "AI primary machine already initialized"
        );
        self.entities[id].ai.primary.initialized = true;
        // AIStateMachine constructs these three nested machines before its
        // primary default state enters: attack-move, team follow, individual follow.
        self.reset_nested_attack_idle(id);
        self.reset_nested_attack_idle(id);
        self.reset_nested_attack_idle(id);
        self.enter_primary_idle(id);
    }

    pub(in crate::skirmish) fn enter_primary_idle(&mut self, id: usize) {
        let offset = self.random_between(60);
        let now = self.update_frame();
        let ai = &mut self.entities[id].ai;
        ai.idle.needs_init = true;
        ai.idle.sleep_offset = offset;
        ai.primary.sleep_until = 0;
        ai.mood.next_check = u64::from(now.wrapping_add(self.ai_rules.force_idle_frames));
        ai.mood.offset_check = true;
        self.ai_state_changed(id);
    }

    fn update_primary_idle(&mut self, id: usize) -> u32 {
        if self.entities[id].ai.idle.needs_init {
            self.entities[id].ai.idle.needs_init = false;
            if self.attack_ground_movement(id) {
                let position = self.object_position(id);
                if position.iter().any(|&coordinate| coordinate != 0.0) {
                    self.reserve_goal(id, self.entities[id].position);
                    if self.tick <= 1
                        && !self.entities[id].ai.motion.ultra_accurate()
                        && let Some(cell) = self.occupancy.goal_cell(id)
                    {
                        let point = self.path_position(id, cell);
                        let position = [point[0], point[1], position[2]];
                        self.entities[id].position = point;
                        self.entities[id].set_physical_position(position);
                        self.reserve_goal(id, point);
                    }
                }
            }
            self.entities[id].ai.motion.clear_goal();
            self.set_attack_current_victim(id, None);
        }
        let offset = self.entities[id].ai.idle.sleep_offset;
        self.entities[id].ai.idle.sleep_offset = 0;
        let mut sleep = 60 + offset;
        if self.attack_kind(id, "CAN_BE_REPULSED")
            && self.entities[id].ai_idle()
            && self.closest_repulsor(id).is_some()
        {
            self.enter_repulsor_state(id);
            return 1;
        }
        if !self.entities[id].is_effectively_dead()
            && self.object_disabled_mask(id)
                & ((1 << 1) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 10))
                == 0
        {
            self.acquire_target(id);
            if !matches!(self.entities[id].ai.order, Order::Idle) {
                return 1;
            }
        }
        let now = self.update_frame();
        let next = super::native_frame(self.entities[id].ai.mood.next_check);
        if next > now && next - now < sleep {
            sleep = next - now;
            self.entities[id].ai.idle.sleep_offset = offset;
        }
        sleep
    }

    /// Shared INITIAL callback; movement cleanup and turret updates precede the
    /// late DEAD transition, and all clocks compete with doLocomotor's result.
    pub(in crate::skirmish) fn update_ai_module(&mut self, id: usize) -> i32 {
        assert!(
            self.entities[id].ai.primary.initialized,
            "AI CREATE callback must initialize its primary machine"
        );
        self.update_deployment(id);
        self.dispatch_hacker_pending(id);
        self.update_jet_ai_wrapper(id);
        self.entities[id].ai.in_update = true;
        self.entities[id].ai.completed_waypoint = None;
        self.update_weapon_clocks(id);
        let now = self.update_frame();
        let deadline = self.entities[id].ai.primary.sleep_until;
        let mut sleep = if self.update_move_aside(id) {
            1
        } else if deadline != 0 && now < deadline {
            deadline - now
        } else {
            let sleep = self.update_primary_ai_state(id);
            self.entities[id].ai.primary.sleep_until = if sleep > 1 {
                now.wrapping_add(sleep)
            } else {
                0
            };
            sleep
        };
        self.finish_completed_movement(id);
        self.wake_path_request(id);
        sleep = sleep.min(self.ai_path_sleep_frames(id));
        if !self.entities[id].is_effectively_dead()
            && self.object_disabled_mask(id)
                & ((1 << 1) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 10))
                == 0
        {
            self.update_idle_turret(id);
            sleep = sleep.min(self.native_turret_sleep_frames(id));
        }
        if self.entities[id].is_ai_dead()
            && !matches!(self.entities[id].ai.command.main_state, MainState::Dead)
        {
            self.enter_primary_dead(id);
            sleep = 1;
        }
        if !self.attack_kind(id, "IMMOBILE") {
            let actor = &mut self.entities[id];
            let allow = actor
                .active_locomotor(&self.catalog[actor.type_index])
                .friction_when_airborne;
            actor.set_physics_airborne_friction(allow);
        }
        self.update_chinook_lift(id);
        self.update_flight(id);
        sleep = sleep.min(self.update_ground_locomotor(id));
        self.update_position_claim(id);
        self.observe_ground_partition(id);
        if self.entities[id].ai.completed_waypoint.is_some()
            || self.entities[id].aircraft.is_some()
            || self.entities[id].deployment.is_some()
            || self.catalog[self.entities[id].type_index]
                .behavior
                .hacking
                .is_some()
        {
            sleep = 1;
        }
        self.entities[id].ai.in_update = false;
        i32::try_from(sleep).expect("native AI sleep")
    }

    fn update_primary_ai_state(&mut self, id: usize) -> u32 {
        match self.entities[id].ai.command.main_state {
            MainState::Dead => {
                self.entities[id].ai.motion.clear_goal();
                self.dead_aircraft_goal(id);
                self.entities[id].set_effectively_dead(true);
                if self.attack_kind(id, "INFANTRY")
                    && self.catalog[self.entities[id].type_index]
                        .crush
                        .physics_module
                        .is_some()
                {
                    let [x, y, z] = self.entities[id].physics_velocity();
                    let squared = [x, y, z].map(|component| component * component);
                    let speed = (squared[0] + squared[1] + squared[2]).sqrt();
                    self.entities[id].scrub_physics_velocity_2d(speed * 0.8);
                }
                return 1;
            }
            MainState::Repulsors | MainState::WanderInPlace => {
                self.update_repulsor_state(id);
                return 1;
            }
            MainState::Wander | MainState::Panic => {
                self.update_waypoint_wander(id);
                return 1;
            }
            MainState::Busy => return FOREVER,
            MainState::Hunt => return self.update_hunt(id),
            MainState::Rappel => {
                self.update_rappel(id);
                return 1;
            }
            MainState::HackInternet => {
                self.update_hacking(id);
                return 1;
            }
            MainState::Jet => {
                self.update_aircraft(id);
                return 1;
            }
            MainState::Chinook => {
                self.update_chinook_state(id);
                return 1;
            }
            MainState::FaceObject { .. } | MainState::FacePosition { .. } => {
                return self.update_native_face(id);
            }
            MainState::Active => {}
        }
        if self.entities[id].contained_by().is_some() {
            self.update_contained(id);
            return 1;
        }
        let sleep = if matches!(self.entities[id].ai.order, Order::Idle) {
            self.update_primary_idle(id)
        } else {
            self.update_order(id);
            1
        };
        if self.entities[id].ai.dozer.is_none() {
            self.update_idle_repair(id);
            self.update_builder_weapon(id);
        }
        sleep
    }

    pub(in crate::skirmish) fn enter_primary_dead(&mut self, id: usize) {
        self.cancel_hacking(id);
        self.exit_hunt(id);
        self.exit_rappel(id);
        self.exit_native_follow_path(id);
        self.exit_chinook_phase(id);
        self.cancel_docking(id);
        self.exit_waypoint_state(id);
        self.exit_repulsor_state(id);
        self.exit_attack_move(id);
        self.exit_native_attack(id);
        self.container_notifications(id, Order::Idle);
        self.exit_aircraft_phase(id);
        self.clear_temporary(id);
        self.entities[id].ai.command_revision = self.entities[id]
            .ai
            .command_revision
            .checked_add(1)
            .expect("AI command revision overflow");
        let actor = &mut self.entities[id];
        actor.ai.command.main_state = MainState::Dead;
        actor.ai.primary.sleep_until = 0;
        if actor.ai.motion.is_moving() {
            actor.ai.motion.end_move();
        }
        actor.ai.order = Order::Idle;
        actor
            .drawable
            .model_conditions
            .set_flag(crate::skirmish::ModelConditions::MOVING, false);
        actor.weapon_set.status.condition = [None; 3];
        actor.weapon_set.status.pre_attack_animation = None;
    }
}

#[cfg(test)]
mod tests {
    use crate::skirmish::{Order, ai_goals::MainState, tests::game};

    #[test]
    fn object_command_enters_aim_immediately_and_stop_runs_its_exit() {
        let mut game = game();
        let actor = game.spawn(5, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [330.0, 300.0], true);
        game.entities[actor].facing = 0.0;
        game.attack(actor, target).unwrap();
        assert!(game.entities[actor].attacking());
        assert!(game.entities[actor].weapon_set.status.aiming);
        assert!(game.entities[actor].ai.attack.is_some());
        game.stop(actor).unwrap();
        assert!(game.entities[actor].ai.attack.is_none());
        assert!(!game.entities[actor].attacking());
        assert!(!game.entities[actor].weapon_set.status.aiming);
        assert!(game.entities[actor].ai.current_victim.is_none());
    }

    #[test]
    fn idle_uses_a_retained_primary_clock_and_a_command_wakes_it() {
        let (mut game, actor) = crate::skirmish::ground_movement::tests::moving_tank();
        let kind = game.entities[actor].type_index;
        game.catalog[kind].ai.acquisition = 0;
        game.tick = 3;
        let sleep = game.update_ai_module(actor);
        assert!(sleep > 1);
        let deadline = game.entities[actor].ai.primary.sleep_until;
        let random = game.random;
        game.tick += 1;
        game.update_ai_module(actor);
        assert_eq!(game.entities[actor].ai.primary.sleep_until, deadline);
        assert_eq!(game.random, random);
        let target = game.spawn(kind, 1, [330.0, 150.0], true);
        game.attack(actor, target).unwrap();
        assert_eq!(game.entities[actor].ai.primary.sleep_until, 0);
        assert_eq!(game.update_ai_module(actor), 1);
    }

    #[test]
    fn deleting_an_attacker_cancels_preattack_and_target_lifetime() {
        let mut game = game();
        game.catalog[5].weapon.pre_attack_frames = 9;
        let actor = game.spawn(5, 0, [300.0, 300.0], true);
        let target = game.spawn(5, 1, [330.0, 300.0], true);
        game.entities[actor].facing = 0.0;
        game.attack(actor, target).unwrap();
        game.tick += 1;
        game.update_ai_module(actor);
        assert!(
            game.entities[actor].weapon_set.weapons[0]
                .pre_attack
                .is_some()
        );
        game.destroy_maintenance_object(actor);
        assert!(game.entities[actor].ai.attack.is_none());
        assert!(
            game.entities[actor].weapon_set.weapons[0]
                .pre_attack
                .is_none()
        );
        assert!(!game.entities[actor].attacking());
    }

    #[test]
    fn death_transition_is_late_and_locks_out_later_commands() {
        let mut game = game();
        let actor = game.spawn(5, 0, [300.0, 300.0], true);
        game.mark_ai_dead(actor);
        assert!(!matches!(
            game.entities[actor].ai.command.main_state,
            MainState::Dead
        ));
        game.update_ai_module(actor);
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::Dead
        ));
        game.set_order(actor, Order::Move([400.0, 300.0]));
        assert!(matches!(
            game.entities[actor].ai.command.main_state,
            MainState::Dead
        ));
        assert!(!game.entities[actor].ai.motion.has_goal());
    }
}
