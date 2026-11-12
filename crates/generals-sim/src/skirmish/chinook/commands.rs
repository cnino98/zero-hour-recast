//! `ChinookAIUpdate::aiDoCommand` stores native command arguments until flight settles.
use super::{Flight, Phase, Skirmish};
use crate::skirmish::{
    ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_PLAYER},
    combat::attack::Goal,
};

use crate::skirmish::ai_dispatch::stored_command::{Command, Kind};

impl Skirmish {
    /// True means the subclass consumed, retained, or rejected this command.
    /// False passes the original command to `SupplyTruckAIUpdate`'s dispatch.
    pub(in crate::skirmish) fn chinook_command(&mut self, id: usize, command: Command) -> bool {
        if self.entities[id].chinook.is_none() {
            return false;
        }
        if !matches!(command.kind, Kind::Evacuate | Kind::Exit(_)) {
            self.set_chinook_healing_airfield(id, None);
        }
        if !self.native_ai_command_allowed_for_sleep(
            id,
            matches!(command.kind, Kind::MoveEvenIfSleeping(_)),
        ) {
            return true;
        }
        if self.entities[id].chinook.as_ref().unwrap().transitioning() {
            self.entities[id].chinook.as_mut().unwrap().pending = Some(command);
            return true;
        }
        let landed = self.entities[id].landed();
        let next = match &command.kind {
            Kind::Enter(carrier) => {
                if *carrier == id || !self.action_enter_admits(id, *carrier, command.source, false)
                {
                    return true;
                }
                None
            }
            Kind::MoveEvacuate { position, exit } => {
                let current = self.object_position(id);
                let delta = [
                    current[0] - position[0],
                    current[1] - position[1],
                    current[2] - position[2],
                ];
                let squared = delta.map(|component| component * component);
                if landed && squared[0] + squared[1] + squared[2] > 9.0 {
                    Some(Phase::Takeoff)
                } else {
                    let phase = if *exit {
                        Phase::RecordCreation
                    } else {
                        Phase::MoveEvac
                    };
                    let position = *position;
                    // Selected PRESERVE_CHINOOK_PASSENGER_DUMPING=1 always
                    // enters the dumping chain, even without queued enter/exit.
                    self.set_chinook_state(id, phase, None, Some(position), COMMAND_FROM_AI);
                    return true;
                }
            }
            Kind::Exit(_) | Kind::Evacuate if !landed => Some(Phase::Land),
            Kind::Idle
            | Kind::Busy
            | Kind::FollowPath {
                exit_production: true,
                ..
            }
            | Kind::Exit(_)
            | Kind::Evacuate => None,
            _ if !matches!(
                self.entities[id].chinook.as_ref().unwrap().flight,
                Flight::Flying
            ) =>
            {
                Some(Phase::Takeoff)
            }
            _ => None,
        };
        if let Some(next) = next {
            self.entities[id].chinook.as_mut().unwrap().pending = Some(command);
            self.set_chinook_state(id, next, None, None, COMMAND_FROM_AI);
            return true;
        }
        self.entities[id].chinook.as_mut().unwrap().pending = None;
        false
    }

    pub(in crate::skirmish) fn chinook_attack_passengers(
        &mut self,
        id: usize,
        goal: Goal,
        shots: u32,
        source: i32,
        forced: bool,
    ) -> bool {
        if self.entities[id].chinook.is_none() {
            return true;
        }
        if !self.attack_kind(id, "CAN_ATTACK") {
            return false;
        }
        if source != COMMAND_FROM_PLAYER && source != 1 {
            return true;
        }
        for rider in self.passengers(id).to_vec() {
            if !self.container_fire_allowed(id, Some(rider))
                || !self.attack_kind(rider, "INFANTRY")
                || self.attack_kind(rider, "PORTABLE_STRUCTURE")
                    && self.portable_attack_disabled(rider)
                || self.entities[rider].ai.registration.is_none()
            {
                continue;
            }
            self.native_attack_goal_from(rider, goal, shots, source, forced);
        }
        if let Some(rider) = self.entities[id].attachment_child()
            && self.attack_kind(rider, "PORTABLE_STRUCTURE")
            && !self.portable_attack_disabled(rider)
            && self.entities[rider].ai.registration.is_some()
        {
            self.native_attack_goal_from(rider, goal, shots, source, forced);
        }
        true
    }

    fn portable_attack_disabled(&self, id: usize) -> bool {
        self.object_disabled_mask(id) & ((1 << 1) | (1 << 2) | (1 << 4) | (1 << 5)) != 0
    }

    pub(in crate::skirmish) fn native_evacuate_from(&mut self, id: usize, source: i32) {
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: Kind::Evacuate,
            },
        ) || !self.native_ai_command_allowed(id)
        {
            return;
        }
        for rider in self.passengers(id).to_vec() {
            if self.entities[rider].ai.registration.is_some() {
                self.native_exit_from(rider, id, source);
            }
        }
    }

    pub(in crate::skirmish) fn native_move_evacuate_from(
        &mut self,
        id: usize,
        position: [f32; 3],
        exit: bool,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: Kind::MoveEvacuate { position, exit },
            },
        ) {
            return;
        }
        // The selected Chinook override always consumes this command.
        assert!(
            self.entities[id].chinook.is_some(),
            "Chinook move-and-evacuate virtual dispatch"
        );
    }

    pub(in crate::skirmish) fn native_combat_drop_from(
        &mut self,
        id: usize,
        target: Option<usize>,
        mut position: [f32; 3],
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: Kind::CombatDrop { target, position },
            },
        ) {
            return;
        }
        if source == COMMAND_FROM_PLAYER
            && target.is_some_and(|target| !self.action_enter_admits(id, target, source, true))
        {
            return;
        }
        if target.is_none() {
            position = self
                .find_default_position_around(position, self.object_radius(id) * 100.0, None)
                .unwrap_or(position);
        }
        self.set_chinook_state(id, Phase::MoveCombat, target, Some(position), source);
    }

    pub(in crate::skirmish) fn native_chinook_repair_from(
        &mut self,
        id: usize,
        target: usize,
        source: i32,
    ) {
        if self.native_subclass_command(
            id,
            Command {
                source,
                kind: Kind::Repair(target),
            },
        ) {
            return;
        }
        if matches!(
            self.entities[id].chinook.as_ref().unwrap().flight,
            Flight::Landing(_) | Flight::Landed
        ) || !self.chinook_repair_admits(id, target, source)
        {
            return;
        }
        self.set_chinook_healing_airfield(id, Some(target));
        let position = self.object_position(target);
        let position = self
            .find_default_position_around(position, self.object_radius(target) * 100.0, None)
            .unwrap_or(position);
        self.set_chinook_state(id, Phase::MoveLand, None, Some(position), source);
    }

    #[must_use]
    pub fn combat_drop_ready(&self, id: usize) -> bool {
        self.entities
            .get(id)
            .is_some_and(|actor| actor.chinook.is_some())
            && self
                .passengers(id)
                .iter()
                .any(|&rider| self.attack_kind(rider, "CAN_RAPPEL"))
    }

    #[must_use]
    pub fn can_combat_drop_into(&self, id: usize, target: usize) -> bool {
        self.entities
            .get(id)
            .is_some_and(|actor| actor.chinook.is_some())
            && self.action_enter_admits(id, target, COMMAND_FROM_PLAYER, true)
    }

    /// Original object/location Combat Drop command, with unprojected XYZ.
    /// # Errors
    /// Rejects an unavailable Chinook, nonfinite coordinates or an invalid object target.
    pub fn combat_drop(
        &mut self,
        id: usize,
        target: Option<usize>,
        position: [f32; 3],
    ) -> Result<(), &'static str> {
        self.ready_actor(id)?;
        if self.entities[id].chinook.is_none()
            || position.iter().any(|value| !value.is_finite())
            || target.is_some_and(|target| !self.can_combat_drop_into(id, target))
        {
            return Err("Chinook cannot combat drop at this target");
        }
        let position = target.map_or(position, |target| self.object_position(target));
        self.native_combat_drop_from(id, target, position, COMMAND_FROM_PLAYER);
        Ok(())
    }

    /// `AIGroup::groupEvacuate` chooses the airborne move-and-evacuate command.
    /// # Errors
    /// Rejects unavailable actors and unsupported containers.
    pub fn group_evacuate(&mut self, id: usize) -> Result<(), &'static str> {
        let actor = self.ready_actor(id)?;
        let unit = &self.catalog[actor.type_index];
        if actor.chinook.is_none()
            || self.height_above_ground(id)
                <= actor.active_locomotor(unit).airborne_targeting_height
        {
            return self.evacuate(id);
        }
        self.native_move_evacuate_from(id, self.object_position(id), false, COMMAND_FROM_PLAYER);
        Ok(())
    }
}
