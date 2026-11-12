//! SH AIDock.cpp: primary `AI_DOCK` and its eight-state nested machine.
use super::dock::{Notification, Position};
use crate::skirmish::{Order, Skirmish, ground_movement::MoveResult};

// Commands retain this identity; warehouses map to their actual Object instance.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::skirmish) enum DockTarget {
    Warehouse(usize),
    Center(usize),
    Repair(usize),
}

enum Stage {
    Approach,
    Waiting,
    Advance,
    Enter,
    Action,
    Process,
    Exit,
    Rally,
}

impl Stage {
    const fn movement_index(&self) -> Option<usize> {
        match self {
            Self::Approach => Some(0),
            Self::Advance => Some(1),
            Self::Enter => Some(2),
            Self::Action => Some(3),
            Self::Exit => Some(4),
            Self::Rally => Some(5),
            Self::Waiting | Self::Process => None,
        }
    }
}

pub(in crate::skirmish) struct Docking {
    pub target: DockTarget,
    pub repair_drone: Option<usize>,
    object: usize,
    slot: Option<usize>,
    stage: Stage,
    waiting_since: u32,
    next_action: u32,
    adjusts: [bool; 6],
}

impl Skirmish {
    pub(in crate::skirmish) fn dock_object(&self, target: DockTarget) -> Option<usize> {
        let object = match target {
            DockTarget::Warehouse(index) => self.supplies.get(index)?.object,
            DockTarget::Center(object) | DockTarget::Repair(object) => object,
        };
        self.entities
            .get(object)
            .filter(|actor| actor.exists() && actor.dock.is_some())
            .map(|_| object)
    }

    pub(in crate::skirmish) fn ai_dock(&mut self, id: usize, target: DockTarget, source: i32) {
        if !self.native_ai_command_allowed(id)
            || self.object_disabled(id)
            || self.attack_kind(id, "IMMOBILE")
        {
            return;
        }
        let Some(object) = self.dock_object(target) else {
            return;
        };
        if source == crate::skirmish::ai_goals::COMMAND_FROM_PLAYER {
            self.dozer_player_command(id);
        }
        self.set_ai_goal_object(id, object);
        let order = match target {
            DockTarget::Warehouse(index) => Order::Harvest(index),
            DockTarget::Center(_) => Order::ReturnSupply,
            DockTarget::Repair(object) => Order::RepairAt(object),
        };
        self.set_last_command_source(id, source);
        self.set_ai_order(id, order);
        self.entities[id].ai.command.from_ai = source == crate::skirmish::ai_goals::COMMAND_FROM_AI;
        if source == crate::skirmish::ai_goals::COMMAND_FROM_PLAYER {
            self.entities[id].ai.supply.preferred_dock = Some(target);
            self.finish_dozer_player_command(id);
        }
    }

    pub(in crate::skirmish) fn enter_docking(&mut self, id: usize, target: DockTarget) {
        let Some(object) = self.dock_object(target) else {
            self.set_ai_order(id, Order::Idle);
            return;
        };
        self.entities[id].ai.ignored_producer = Some(object);
        self.entities[id].ai.docking = Some(Docking {
            target,
            object,
            repair_drone: None,
            slot: None,
            stage: Stage::Approach,
            waiting_since: 0,
            next_action: 0,
            adjusts: [true; 6],
        });
        let result = self.enter_dock_stage(id);
        self.resolve_dock_transition(id, result);
    }

    pub(in crate::skirmish) fn update_docking(&mut self, id: usize, _target: DockTarget) {
        self.entities[id].ai.blocking.path_through_units = true;
        let state = self.entities[id]
            .ai
            .docking
            .as_ref()
            .expect("AI_DOCK eager entry");
        let object = state.object;
        let present = self.entities[object].exists();
        let result = match state.stage {
            Stage::Rally => self.update_internal_move(id),
            _ if !present => MoveResult::Failure,
            Stage::Waiting => self.wait_native_dock(id),
            Stage::Process => self.process_native_dock(id),
            Stage::Action if !self.native_dock_open(object) => {
                self.native_dock_cancel(object, id);
                MoveResult::Failure
            }
            Stage::Approach | Stage::Advance | Stage::Enter | Stage::Action | Stage::Exit => {
                self.update_internal_move(id)
            }
        };
        self.resolve_dock_transition(id, result);
    }

    fn resolve_dock_transition(&mut self, id: usize, mut result: MoveResult) {
        for _ in 0..19 {
            let Some(state) = self.entities[id].ai.docking.as_ref() else {
                return;
            };
            let next = match result {
                MoveResult::Continue => {
                    if matches!(state.stage, Stage::Waiting)
                        && self.native_dock_clear_to_advance(state.object, id, state.slot)
                    {
                        Some(Stage::Advance)
                    } else {
                        return;
                    }
                }
                MoveResult::Success => match state.stage {
                    Stage::Approach | Stage::Advance => Some(Stage::Waiting),
                    Stage::Waiting => Some(Stage::Enter),
                    Stage::Enter => Some(Stage::Action),
                    Stage::Action => Some(Stage::Process),
                    Stage::Process => Some(Stage::Exit),
                    Stage::Exit => Some(Stage::Rally),
                    Stage::Rally => None,
                },
                MoveResult::Failure => match state.stage {
                    Stage::Enter | Stage::Action | Stage::Process => Some(Stage::Exit),
                    Stage::Approach
                    | Stage::Advance
                    | Stage::Waiting
                    | Stage::Exit
                    | Stage::Rally => None,
                },
            };
            let Some(next) = next else {
                // Normal state exit precedes the primary machine's halt/reset.
                self.exit_dock_stage(id);
                self.set_ai_order(id, Order::Idle);
                return;
            };
            self.exit_dock_stage(id);
            self.entities[id]
                .ai
                .docking
                .as_mut()
                .expect("Dock state transition")
                .stage = next;
            result = self.enter_dock_stage(id);
        }
        panic!("AIDockMachine immediate-transition limit");
    }

    fn enter_dock_stage(&mut self, id: usize) -> MoveResult {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        let object = state.object;
        if !self.entities[object].exists() {
            return MoveResult::Failure;
        }
        if !matches!(
            state.stage,
            Stage::Waiting | Stage::Process | Stage::Exit | Stage::Rally
        ) && !self.native_dock_open(object)
        {
            self.native_dock_cancel(object, id);
            return MoveResult::Failure;
        }
        let goal = match state.stage {
            Stage::Approach | Stage::Advance => {
                let position = if matches!(state.stage, Stage::Approach) {
                    self.native_dock_reserve(object, id)
                } else {
                    let slot = state.slot.expect("Advance reserved approach");
                    self.native_dock_advance(object, id, slot)
                };
                let Some((slot, position)) = position else {
                    return MoveResult::Failure;
                };
                self.entities[id]
                    .ai
                    .docking
                    .as_mut()
                    .expect("Dock state")
                    .slot = Some(slot);
                self.entities[id].ai.ignored_producer = None;
                position
            }
            Stage::Waiting => {
                let now = self.update_frame();
                self.entities[id]
                    .ai
                    .docking
                    .as_mut()
                    .expect("Dock state")
                    .waiting_since = now;
                return MoveResult::Continue;
            }
            Stage::Process => {
                self.schedule_dock_action(id);
                return MoveResult::Continue;
            }
            Stage::Rally => {
                let actor = &self.entities[object];
                if self.catalog[actor.type_index]
                    .neutral_tech
                    .repair_dock
                    .is_none()
                    || self.catalog[actor.type_index]
                        .behavior
                        .production_exit
                        .is_none()
                {
                    return MoveResult::Success;
                }
                let Some(point) = actor.rally_point else {
                    return MoveResult::Success;
                };
                point
            }
            Stage::Enter | Stage::Action | Stage::Exit => {
                let kind = match state.stage {
                    Stage::Enter => Position::Enter,
                    Stage::Action => Position::Action,
                    Stage::Exit => Position::Exit,
                    _ => unreachable!("Dock movement stage"),
                };
                let passthrough = self.catalog[self.entities[object].type_index]
                    .supply
                    .dock
                    .as_ref()
                    .expect("Dock data")
                    .allows_passthrough;
                let position = self.native_dock_position(object, id, &kind);
                let state = self.entities[id].ai.docking.as_mut().expect("Dock state");
                if matches!(state.stage, Stage::Enter) {
                    state.slot = None;
                }
                if passthrough {
                    if !matches!(state.stage, Stage::Enter) {
                        state.adjusts[state.stage.movement_index().expect("movement stage")] =
                            false;
                    }
                    self.entities[id].ai.ignored_producer = Some(object);
                }
                position
            }
        };
        self.enter_dock_movement(id, goal)
    }

    fn enter_dock_movement(&mut self, id: usize, goal: [f32; 3]) -> MoveResult {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        let index = state.stage.movement_index().expect("movement stage");
        self.entities[id].ai.command.move_to.adjusts = state.adjusts[index];
        self.entities[id].ai.command.move_to.position = goal;
        let result = self.enter_primary_ground_move(id);
        let adjusts = self.entities[id].ai.command.move_to.adjusts;
        self.entities[id]
            .ai
            .docking
            .as_mut()
            .expect("Dock state")
            .adjusts[index] = adjusts;
        result
    }

    fn wait_native_dock(&mut self, id: usize) -> MoveResult {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        let object = state.object;
        if !self.native_dock_open(object) {
            self.native_dock_cancel(object, id);
            return MoveResult::Failure;
        }
        if self.native_dock_clear_to_enter(object, id) {
            return MoveResult::Success;
        }
        if state.waiting_since.wrapping_add(900) < self.update_frame() {
            MoveResult::Failure
        } else {
            MoveResult::Continue
        }
    }

    fn exit_dock_stage(&mut self, id: usize) {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        let object = state.object;
        let movement = state.stage.movement_index().is_some();
        if self.entities[object].exists() {
            if matches!(state.stage, Stage::Exit) {
                self.native_dock_notify(object, id, &Notification::Exit);
            } else if !matches!(state.stage, Stage::Process | Stage::Rally) {
                if self.native_dock_open(object) {
                    let notification = match state.stage {
                        Stage::Approach | Stage::Advance => Some(Notification::Approach),
                        Stage::Enter => Some(Notification::Enter),
                        Stage::Action => Some(Notification::Action),
                        _ => None,
                    };
                    if let Some(notification) = notification {
                        self.native_dock_notify(object, id, &notification);
                    }
                } else {
                    self.native_dock_cancel(object, id);
                }
            }
        }
        if movement {
            self.entities[id].ai.motion.end_move();
            self.entities[id].ai.motion.movement_active = false;
        }
    }

    pub(in crate::skirmish) fn cancel_docking(&mut self, id: usize) {
        let Some(state) = &self.entities[id].ai.docking else {
            return;
        };
        self.native_dock_cancel(state.object, id);
        // StateMachine::halt clears the current state without invoking onExit.
        self.entities[id].ai.docking = None;
        self.entities[id].ai.blocking.path_through_units = false;
        self.entities[id].ai.ignored_producer = None;
    }

    fn schedule_dock_action(&mut self, id: usize) {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        let rules = &self.catalog[self.entities[id].type_index].supply;
        let seconds = if self.entities[id].ai.supply.constructed() {
            match state.target {
                DockTarget::Warehouse(_) => rules.gather_seconds,
                DockTarget::Center(_) => rules.dropoff_seconds,
                DockTarget::Repair(_) => 0.0,
            }
        } else {
            0.0
        };
        let delay =
            num_traits::ToPrimitive::to_u32(&(seconds * 30.0).ceil()).expect("Dock action delay");
        let next = self.update_frame().wrapping_add(delay);
        self.entities[id]
            .ai
            .docking
            .as_mut()
            .expect("Dock state")
            .next_action = next;
    }

    fn process_native_dock(&mut self, id: usize) -> MoveResult {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        if self.update_frame() < state.next_action {
            return MoveResult::Continue;
        }
        let target = state.target;
        let object = state.object;
        self.schedule_dock_action(id);
        self.find_dock_drone(id);
        let continuing = self.native_dock_open(object)
            && match target {
                DockTarget::Warehouse(source) => self.native_warehouse_action(source, id),
                DockTarget::Center(center) => self.native_supply_center_action(center, id),
                DockTarget::Repair(pad) => self.repair_dock_action(id, pad),
            };
        if continuing {
            MoveResult::Continue
        } else {
            MoveResult::Success
        }
    }

    fn find_dock_drone(&mut self, id: usize) {
        let state = self.entities[id].ai.docking.as_ref().expect("Dock state");
        if state
            .repair_drone
            .is_some_and(|drone| self.entities[drone].exists())
        {
            return;
        }
        let drone = self
            .team_membership
            .player_objects(self.entities[id].owner)
            .into_iter()
            .find(|&drone| {
                self.attack_kind(drone, "DRONE")
                    && self.entities[drone].drone_producer() == Some(id)
            });
        self.entities[id]
            .ai
            .docking
            .as_mut()
            .expect("Dock state")
            .repair_drone = drone;
    }

    pub(in crate::skirmish::economy) fn dock_slot(
        &self,
        id: usize,
        target: DockTarget,
    ) -> Option<usize> {
        let object = self.dock_object(target)?;
        self.native_dock_available_slot(object, id)
    }
}
