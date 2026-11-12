//! Main AI state-machine goal snapshots, separate from nested movement goals.
//! SH `StateMachine::internalSetGoalObject` and `AIUpdateInterface::private*`.
use super::{Entity, Order, Skirmish};

pub(super) enum MainState {
    Active,
    Repulsors,
    WanderInPlace,
    Wander,
    Panic,
    Busy,
    Hunt,
    Rappel,
    Chinook,
    Jet,
    HackInternet,
    FaceObject { can_turn_in_place: bool },
    FacePosition { can_turn_in_place: bool },
    Dead,
}

/// Core GameCommon.h `CommandSourceType` values retained as native Int.
pub(in crate::skirmish) const COMMAND_FROM_PLAYER: i32 = 0;
pub(in crate::skirmish) const COMMAND_FROM_SCRIPT: i32 = 1;
pub(in crate::skirmish) const COMMAND_FROM_AI: i32 = 2;
pub(in crate::skirmish) const COMMAND_FROM_DOZER: i32 = 3;

pub(super) struct MoveDestination {
    pub(super) position: [f32; 3],
    pub(super) adjusts: bool,
    pub(super) waiting: bool,
    pub(super) path_goal: [f32; 3],
    pub(super) path_timestamp: u32,
}

pub(super) struct Command {
    pub(super) locked: bool,
    pub(super) main_state: MainState,
    pub(in crate::skirmish) last_command_source: i32,
    pub(super) from_ai: bool,
    pub(super) allowed_to_chase: bool,
    pub(super) ability_goal: Option<usize>,
    pub(super) goal_position: [f32; 3],
    pub(super) goal_object: Option<usize>,
    pub(super) move_to: MoveDestination,
    pub(super) wander: [super::ai_dispatch::WaypointState; 2],
    pub(super) repulsor: super::ai_dispatch::RepulsorState,
    pub(super) ai_group: Option<u32>,
    pub(super) formation_id: Option<u32>,
    pub(super) formation_offset: [f32; 2],
    pub(super) formation_speed: Option<f32>,
    pub(super) desired_speed: f32,
    pub(super) attack_approach: Option<super::ground_movement::Approach>,
    pub(super) attack_area: Option<super::ai::AttackAreaState>,
    pub(super) path_extra_distance: f32,
}

impl Command {
    pub(super) const fn new() -> Self {
        Self {
            locked: false,
            main_state: MainState::Active,
            last_command_source: COMMAND_FROM_AI,
            from_ai: true,
            allowed_to_chase: false,
            ability_goal: None,
            goal_position: [0.0; 3],
            goal_object: None,
            wander: [
                super::ai_dispatch::WaypointState::new(),
                super::ai_dispatch::WaypointState::new(),
            ],
            repulsor: super::ai_dispatch::RepulsorState::new(),
            move_to: MoveDestination {
                position: [0.0; 3],
                adjusts: true,
                waiting: false,
                path_goal: [0.0; 3],
                path_timestamp: 0,
            },
            ai_group: None,
            formation_id: None,
            formation_offset: [0.0; 2],
            formation_speed: None,
            desired_speed: f32::MAX,
            attack_approach: None,
            attack_area: None,
            path_extra_distance: 0.0,
        }
    }
}

impl Entity {
    /// Original main state-machine goal used by Listening Outpost route display.
    #[must_use]
    pub const fn ai_goal_position(&self) -> [f32; 3] {
        self.ai.command.goal_position
    }
}

impl Skirmish {
    /// `WeaponSet::getAbleToAttackObject` normal player relationship gate.
    #[must_use]
    pub fn player_attack_relationship(&self, id: usize, target: usize) -> bool {
        let Some(victim) = self.entities.get(target) else {
            return false;
        };
        if self.entities.get(id).is_none() {
            return false;
        }
        let relationship = self.object_relationship(id, target);
        relationship == 0
            || relationship != 2
                && (victim.script_targetable()
                    || self.catalog[victim.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "MINE"))
    }

    pub(super) fn attack_relationship(&self, id: usize, target: usize) -> bool {
        self.entities
            .get(id)
            .is_some_and(|actor| actor.ai.command.from_ai || actor.ai.attack_context.forced)
            || self.player_attack_relationship(id, target)
    }

    pub(super) fn copy_attack_command(&mut self, parent: usize, child: usize) {
        self.entities[child].ai.command.from_ai = self.entities[parent].ai.command.from_ai;
        self.entities[child].ai.command.last_command_source =
            self.entities[parent].ai.command.last_command_source;
        self.entities[child].ai.attack_context.forced =
            self.entities[parent].ai.attack_context.forced;
    }

    pub(super) fn set_ai_goal_position(&mut self, id: usize, position: [f32; 3]) {
        if self.entities[id].ai.command.locked {
            return;
        }
        self.entities[id].ai.command.goal_position = position;
    }

    pub(super) fn set_ai_ground_goal(&mut self, id: usize, position: [f32; 2]) {
        self.set_ai_goal_position(id, [position[0], position[1], self.ground_height(position)]);
    }

    pub(super) fn set_ai_goal_object(&mut self, id: usize, target: usize) {
        if self.entities[id].ai.command.locked {
            return;
        }
        self.entities[id].ai.command.goal_object = Some(target);
        self.set_ai_goal_position(id, self.object_position(target));
    }

    pub(super) fn clear_ai_goal(&mut self, id: usize) {
        if self.entities[id].ai.command.locked {
            return;
        }
        self.entities[id].ai.command.goal_object = None;
        self.set_ai_goal_position(id, [0.0; 3]);
    }

    pub(super) fn record_ai_order_goal(&mut self, id: usize, order: Order) {
        if self.entities[id].ai.command.locked {
            return;
        }
        match order {
            Order::Move(position)
            | Order::AttackMove(position)
            | Order::FireAt { position, .. } => {
                self.entities[id].ai.command.goal_object = None;
                self.set_ai_ground_goal(id, position);
                if matches!(order, Order::Move(_)) {
                    self.entities[id].ai.command.move_to.adjusts = true;
                    self.entities[id].ai.command.move_to.position =
                        self.entities[id].ai.command.goal_position;
                }
            }
            Order::Exit(position) => {
                self.entities[id].ai.command.goal_object = None;
                self.set_ai_goal_position(id, position);
            }
            Order::Attack(target) | Order::Enter(target) | Order::RepairAt(target) => {
                self.set_ai_goal_object(id, target);
            }
            Order::CleanHazard(target) => {
                if let Some(field) = self.hazards.iter().find(|field| field.id == target) {
                    self.set_ai_goal_position(id, field.position);
                }
            }
            // These commands clear the main machine. Guard's saved center and
            // special-ability targets belong to independent update/state data.
            Order::Guard(_) | Order::AttackArea | Order::FollowPath | Order::HackInternet => {
                self.clear_ai_goal(id);
            }
            Order::ExitContainer => {
                self.clear_ai_goal(id);
                if let Some(carrier) = self.entities[id].containment.exit_at {
                    self.set_ai_goal_object(id, carrier);
                }
            }
            // Internal state completion retains the goal. Explicit aiIdle
            // callbacks clear it separately. Dozer and supply task machines
            // replace it only when they issue their actual move/dock callback.
            Order::Idle
            | Order::Build(_)
            | Order::Repair(_)
            | Order::Harvest(_)
            | Order::ReturnSupply => {}
        }
    }
}

#[cfg(test)]
mod tests;

impl Skirmish {
    /// `AIUpdate::setGoalPositionClipped` preserves Z and only bounds PLAYER XY.
    pub(in crate::skirmish) fn clipped_ai_goal_position(
        &self,
        id: usize,
        mut position: [f32; 3],
        source: i32,
    ) -> [f32; 3] {
        if source == COMMAND_FROM_PLAYER {
            let mut fudge = self.layout.partition_cell_size * 0.5;
            let actor = &self.entities[id];
            if self.attack_kind(id, "AIRCRAFT")
                && self.height_above_ground(id) > -9.0 * self.gravity
                && let Some(flight) = &actor
                    .active_locomotor(&self.catalog[actor.type_index])
                    .flight
            {
                fudge = fudge.max(flight.preferred_height);
            }
            for (axis, coordinate) in position.iter_mut().enumerate().take(2) {
                if *coordinate < self.playable_extent[0][axis] + fudge {
                    *coordinate = self.playable_extent[0][axis] + fudge;
                }
                if *coordinate > self.playable_extent[1][axis] - fudge {
                    *coordinate = self.playable_extent[1][axis] - fudge;
                }
            }
        }
        position
    }
}
