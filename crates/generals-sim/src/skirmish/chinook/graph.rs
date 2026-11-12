//! `ChinookAIStateMachine`'s fourteen registered primary states.
use super::{Flight, Phase, Skirmish};
use crate::skirmish::{Order, ai_goals::MainState, ground_movement::MoveResult, locomotor};

impl Phase {
    const fn moving(&self) -> bool {
        matches!(
            self,
            Self::MoveLand | Self::MoveEvac | Self::MoveEvacExit | Self::MoveCombat
        )
    }

    const fn landing(&self) -> bool {
        matches!(self, Self::Land | Self::LandEvac | Self::LandEvacExit)
    }

    const fn vertical(&self) -> bool {
        self.landing() || matches!(self, Self::Takeoff | Self::TakeoffExit)
    }

    const fn next(&self) -> Option<Self> {
        match self {
            Self::MoveLand => Some(Self::Land),
            Self::MoveEvac => Some(Self::LandEvac),
            Self::LandEvac => Some(Self::EvacTakeoff),
            Self::EvacTakeoff => Some(Self::Takeoff),
            Self::RecordCreation => Some(Self::MoveEvacExit),
            Self::MoveEvacExit => Some(Self::LandEvacExit),
            Self::LandEvacExit => Some(Self::EvacExit),
            Self::EvacExit => Some(Self::TakeoffExit),
            Self::TakeoffExit => Some(Self::HeadOffMap),
            Self::MoveCombat => Some(Self::CombatDrop),
            Self::Takeoff | Self::Land | Self::HeadOffMap | Self::CombatDrop => None,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn set_chinook_state(
        &mut self,
        id: usize,
        phase: Phase,
        target: Option<usize>,
        position: Option<[f32; 3]>,
        source: i32,
    ) {
        let position = position.map(|position| self.clipped_ai_goal_position(id, position, source));
        if self
            .prepare_order_state(id, Order::Idle, false, position, Some(source))
            .is_none()
        {
            return;
        }
        self.entities[id].ai.command.goal_object = target;
        self.entities[id].ai.command.main_state = MainState::Chinook;
        self.ai_state_changed(id);
        self.enter_chinook_phase(id, phase);
    }

    fn enter_chinook_phase(&mut self, id: usize, phase: Phase) {
        self.entities[id]
            .chinook
            .as_mut()
            .expect("Chinook constructor")
            .phase = Some(phase);
        let phase = self.entities[id]
            .chinook
            .as_ref()
            .unwrap()
            .phase
            .as_ref()
            .unwrap();
        let result = if phase.vertical() {
            self.enter_chinook_vertical(id);
            MoveResult::Continue
        } else if phase.moving() {
            if matches!(phase, Phase::MoveCombat) {
                self.enter_chinook_combat_move(id);
            }
            let goal = self.entities[id]
                .ai
                .command
                .goal_object
                .filter(|&target| {
                    self.entities
                        .get(target)
                        .is_some_and(crate::skirmish::Entity::exists)
                })
                .map_or_else(
                    || self.entities[id].ai_goal_position(),
                    |target| self.object_position(target),
                );
            self.entities[id].ai.command.move_to.position = goal;
            self.enter_primary_ground_move(id)
        } else {
            match phase {
                Phase::RecordCreation => {
                    let position = self.object_position(id);
                    self.entities[id]
                        .chinook
                        .as_mut()
                        .unwrap()
                        .original_position = position;
                    MoveResult::Success
                }
                Phase::EvacTakeoff | Phase::EvacExit => {
                    for rider in self.passengers(id).to_vec() {
                        self.remove_passenger(rider, id);
                    }
                    self.activate_object_team(id);
                    MoveResult::Success
                }
                Phase::HeadOffMap => {
                    self.entities[id].object_status |= 1 << 37;
                    let position = self.entities[id]
                        .chinook
                        .as_ref()
                        .unwrap()
                        .original_position;
                    self.construction_move(id, position);
                    self.entities[id].ai.motion.set_allow_invalid_position(true);
                    MoveResult::Continue
                }
                Phase::CombatDrop => self.enter_chinook_combat_drop(id),
                _ => unreachable!("moving and vertical entries handled above"),
            }
        };
        if self.entities[id].exists() {
            self.finish_chinook_phase(id, &result);
        }
    }

    pub(in crate::skirmish) fn update_chinook_state(&mut self, id: usize) {
        if self.update_move_aside(id) {
            return;
        }
        let phase = self.entities[id]
            .chinook
            .as_ref()
            .unwrap()
            .phase
            .as_ref()
            .unwrap();
        let result = if phase.vertical() {
            if self.entities[id].is_effectively_dead() {
                MoveResult::Failure
            } else {
                let (Flight::Landing(goal) | Flight::TakingOff(goal)) =
                    self.entities[id].chinook.as_ref().unwrap().flight
                else {
                    unreachable!("vertical state owns flight destination");
                };
                self.set_flight_explicit_goal(id, goal);
                let position = self.object_position(id);
                let delta = [
                    position[0] - goal[0],
                    position[1] - goal[1],
                    position[2] - goal[2],
                ];
                let squared = delta.map(|component| component * component);
                if squared[0] + squared[1] + squared[2] <= 9.0 {
                    MoveResult::Success
                } else {
                    MoveResult::Continue
                }
            }
        } else if phase.moving() {
            let combat = matches!(phase, Phase::MoveCombat);
            let result = self.update_primary_ground_move(id);
            if combat
                && !matches!(result, MoveResult::Continue)
                && (self.object_position(id)[2]
                    - self.entities[id]
                        .chinook
                        .as_ref()
                        .unwrap()
                        .combat_destination_z)
                    .abs()
                    > 3.0
            {
                MoveResult::Continue
            } else {
                result
            }
        } else if matches!(phase, Phase::CombatDrop) {
            self.update_chinook_combat_drop(id)
        } else if matches!(phase, Phase::HeadOffMap) {
            let point = self.entities[id].position;
            if (0..2).any(|axis| {
                point[axis] < self.border_extent[0][axis]
                    || point[axis] > self.border_extent[1][axis]
            }) {
                self.destroy_maintenance_object(id);
                MoveResult::Success
            } else {
                MoveResult::Continue
            }
        } else {
            MoveResult::Success
        };
        if self.entities[id].exists() {
            self.finish_chinook_phase(id, &result);
        }
    }

    fn finish_chinook_phase(&mut self, id: usize, result: &MoveResult) {
        if matches!(result, MoveResult::Continue) {
            return;
        }
        let next = self.entities[id]
            .chinook
            .as_ref()
            .unwrap()
            .phase
            .as_ref()
            .and_then(Phase::next);
        self.exit_chinook_phase(id);
        if matches!(result, MoveResult::Success)
            && let Some(next) = next
        {
            self.enter_chinook_phase(id, next);
        } else {
            self.set_ai_order(id, Order::Idle);
        }
    }

    fn enter_chinook_vertical(&mut self, id: usize) {
        let landing = self.entities[id]
            .chinook
            .as_ref()
            .unwrap()
            .phase
            .as_ref()
            .unwrap()
            .landing();
        self.entities[id].chinook.as_mut().unwrap().flight = if landing {
            Flight::Landing([0.0; 3])
        } else {
            Flight::TakingOff([0.0; 3])
        };
        if landing {
            self.entities[id].cargo = 0;
        }
        self.entities[id].scrub_physics_velocity_2d(0.0);
        self.choose_locomotor_set(id, locomotor::set::NORMAL);
        self.set_flight_precision(id, true, true);
        let original = self.object_position(id);
        let ground = self.ground_height(self.entities[id].position);
        let layer = self
            .movement_terrain
            .get_highest_layer_for_destination(original, ground, true);
        let mut goal = original;
        goal[2] = self
            .movement_terrain
            .bridge_height(layer, self.entities[id].position)
            .unwrap_or(ground);
        if landing {
            if let Some(found) =
                self.find_default_position_around(goal, self.object_radius(id) * 100.0, None)
            {
                let point = self
                    .adjust_chinook_landing(id, [found[0], found[1]])
                    .unwrap_or([found[0], found[1]]);
                goal = [point[0], point[1], found[2]];
            }
            let point = [goal[0], goal[1]];
            let ground = self.ground_height(point);
            let layer = self.movement_terrain.get_highest_layer_for_destination(
                [goal[0], goal[1], original[2]],
                ground,
                true,
            );
            goal[2] = self
                .movement_terrain
                .bridge_height(layer, point)
                .unwrap_or(ground);
            self.entities[id].layer = layer;
        } else {
            goal[2] += self.flight_preferred_height(id);
            self.entities[id].layer = 1;
        }
        self.entities[id].chinook.as_mut().unwrap().flight = if landing {
            Flight::Landing(goal)
        } else {
            Flight::TakingOff(goal)
        };
    }

    fn enter_chinook_combat_move(&mut self, id: usize) {
        self.entities[id].ai.motion.set_ultra_accurate(true);
        let old = self.flight_preferred_height(id);
        let target = self.entities[id].ai.command.goal_object.filter(|&target| {
            self.entities
                .get(target)
                .is_some_and(|actor| actor.exists() && !actor.is_effectively_dead())
                && self.attack_kind(target, "STRUCTURE")
        });
        let (height, point) = target.map_or_else(
            || (old, self.entities[id].ai_goal_position()),
            |target| {
                let building_height = self.body_geometry_bounds(target).max_height;
                let min = self.catalog[self.entities[id].type_index]
                    .chinook
                    .as_ref()
                    .unwrap()
                    .drop
                    .min_height;
                (
                    (building_height + min).max(old),
                    self.object_position(target),
                )
            },
        );
        self.set_flight_preferred_height(id, height);
        let destination = self.ground_height([point[0], point[1]]) + height;
        let state = self.entities[id].chinook.as_mut().unwrap();
        state.old_preferred_height = old;
        state.combat_destination_z = destination;
    }

    pub(in crate::skirmish) fn exit_chinook_phase(&mut self, id: usize) {
        let Some(phase) = self.entities[id]
            .chinook
            .as_mut()
            .and_then(|state| state.phase.take())
        else {
            return;
        };
        if phase.vertical() {
            self.entities[id].chinook.as_mut().unwrap().flight = if phase.landing() {
                Flight::Landed
            } else {
                Flight::Flying
            };
            self.set_flight_precision(id, false, false);
            if !self.entities[id].is_effectively_dead() {
                self.set_flight_lift_limit(id, 99999.0);
            }
            if phase.landing() {
                self.choose_locomotor_set(id, locomotor::set::TAXI);
            } else {
                self.entities[id].layer = 1;
            }
        } else if matches!(phase, Phase::MoveCombat) {
            let old = self.entities[id]
                .chinook
                .as_ref()
                .unwrap()
                .old_preferred_height;
            self.set_flight_preferred_height(id, old);
            self.entities[id].ai.motion.set_ultra_accurate(false);
        } else if matches!(phase, Phase::CombatDrop) {
            self.exit_chinook_combat_drop(id);
        }
        if phase.moving() {
            self.entities[id].ai.motion.end_move();
        }
        // HeadOffMap's no-argument onExit does not override State::onExit in SH.
    }
}
