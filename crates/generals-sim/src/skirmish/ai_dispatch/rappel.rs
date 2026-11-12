//! SH AIStates.cpp409–656: `AIRappelState`, including hostile garrison entry.
use crate::skirmish::{DeathType, ModelConditions, Order, Skirmish, ai_goals::MainState};
use std::f32::consts::PI;

pub(in crate::skirmish) struct State {
    rate: f32,
    destination: f32,
    building: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn native_rappel_from(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
        source: i32,
    ) {
        if !self.native_ai_command_allowed(id) {
            return;
        }
        let position = self.clipped_ai_goal_position(id, position, source);
        if self
            .prepare_order_state(id, Order::Idle, false, Some(position), Some(source))
            .is_none()
        {
            return;
        }
        self.entities[id].ai.command.goal_object = target;
        self.entities[id].ai.blocking.frames = 0;
        self.entities[id].ai.blocking.blocked = false;
        self.entities[id].ai.blocking.stuck = false;
        self.entities[id].ai.command.main_state = MainState::Rappel;
        self.ai_state_changed(id);
        if !self.enter_rappel(id) {
            self.set_ai_order(id, Order::Idle);
        }
    }

    fn enter_rappel(&mut self, id: usize) -> bool {
        if !self.attack_kind(id, "CAN_RAPPEL") {
            return false;
        }
        self.entities[id]
            .drawable
            .model_conditions
            .set_flag(ModelConditions::RAPPELLING, true);
        self.reset_dynamic_physics(id);
        let position = self.object_position(id);
        let ground = self.ground_height(self.entities[id].position);
        let layer = self
            .movement_terrain
            .get_highest_layer_for_destination(position, ground, true);
        let mut destination = self
            .movement_terrain
            .bridge_height(layer, self.entities[id].position)
            .unwrap_or(ground);
        let target = self.entities[id].ai.command.goal_object.filter(|&target| {
            self.entities
                .get(target)
                .is_some_and(|actor| actor.exists() && !actor.is_effectively_dead())
                && self.attack_kind(target, "STRUCTURE")
        });
        if let Some(target) = target {
            destination += self.body_geometry_bounds(target).max_height;
        } else {
            self.entities[id].layer = layer;
        }
        let maximum = (self.gravity.abs() * 30.0) * 2.5;
        self.entities[id].ai.rappel = Some(State {
            rate: -self.entities[id].ai.command.desired_speed.min(maximum),
            destination,
            building: target.is_some(),
        });
        true
    }

    pub(in crate::skirmish) fn update_rappel(&mut self, id: usize) {
        if self.update_move_aside(id) {
            return;
        }
        let revision = self.entities[id].ai.command_revision;
        let mut state = self.entities[id]
            .ai
            .rappel
            .take()
            .expect("AIRappelState entry");
        let position = self.object_position(id);
        let target = self.entities[id].ai.command.goal_object.filter(|&target| {
            self.entities
                .get(target)
                .is_some_and(|actor| actor.exists() && !actor.is_effectively_dead())
        });
        if state.building && target.is_none() {
            state.building = false;
            state.destination = self.ground_height(self.entities[id].position);
        }
        self.entities[id].scrub_physics_velocity_2d(0.0);
        self.entities[id].scrub_physics_velocity_z(state.rate);
        if !state.building {
            state.destination = self
                .movement_terrain
                .bridge_height(self.entities[id].layer, self.entities[id].position)
                .unwrap_or_else(|| self.ground_height(self.entities[id].position));
        }
        let arrived = position[2] <= state.destination;
        let building = state.building;
        let destination = state.destination;
        self.entities[id].ai.rappel = Some(state);
        if !arrived {
            return;
        }
        self.set_object_position(id, [position[0], position[1], destination]);
        if building {
            self.finish_rappel_into_building(id, target.expect("retained rappel building"));
        }
        if self.entities[id].ai.command_revision == revision {
            self.set_ai_order(id, Order::Idle);
        }
    }

    fn finish_rappel_into_building(&mut self, id: usize, building: usize) {
        let mut killed = 0;
        while killed < 2 {
            let enemy = self.passengers(building).iter().copied().find(|&victim| {
                !self.entities[victim].is_effectively_dead()
                    && self.object_relationship(id, victim) == 0
            });
            let Some(enemy) = enemy else { break };
            if let Some(carrier) = self.entities[enemy].contained_by() {
                self.remove_passenger(enemy, carrier);
            }
            self.score_kill(id, enemy);
            self.kill_object(enemy, DeathType::Normal);
            killed += 1;
        }
        if killed > 0
            && let Some(effect) = self.catalog[self.entities[id].type_index]
                .combat_drop_kill_fx
                .clone()
        {
            self.object_fx(building, effect);
        }
        if killed == 2 {
            self.kill_object(id, DeathType::Normal);
        } else if self.native_container_admits(id, building, true) {
            self.add_to_contain(id, building);
        } else {
            self.rappel_full_building_exit(id, building);
        }
    }

    fn rappel_full_building_exit(&mut self, id: usize, building: usize) {
        let offset = self.object_radius(id).min(self.object_radius(building));
        let angle = crate::skirmish::startup::random_real(&mut self.random, PI, 2.0 * PI);
        let origin = self.object_position(building);
        let dx = offset * angle.cos();
        let dy = offset * angle.sin();
        let point = [origin[0] + dx, origin[1] + dy];
        let start = [point[0], point[1], self.ground_height(point)];
        let facing = self.entities[building].facing;
        self.set_object_position(id, start);
        self.set_object_orientation(id, facing);
        if let Some(end) = self.find_default_position_around(start, 200.0, Some(1.5 * PI)) {
            self.native_follow_path_from(
                id,
                vec![end],
                Some(building),
                crate::skirmish::ai_goals::COMMAND_FROM_AI,
                false,
            );
        }
    }

    pub(in crate::skirmish) fn exit_rappel(&mut self, id: usize) {
        if !matches!(self.entities[id].ai.command.main_state, MainState::Rappel) {
            return;
        }
        self.entities[id].ai.rappel = None;
        self.entities[id]
            .drawable
            .model_conditions
            .set_flag(ModelConditions::RAPPELLING, false);
        self.entities[id].ai.command.desired_speed = f32::MAX;
    }
}
