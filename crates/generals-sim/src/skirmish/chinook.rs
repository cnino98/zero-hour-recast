//! SH ChinookAIUpdate.cpp: authored `SupplyTruck` subclass and its primary graph.
use super::{Entity, Locomotor, Skirmish};
pub(in crate::skirmish) mod commands;
mod eligibility;
mod graph;
mod landing;
mod lift;
mod rappel;
pub use rappel::{RopeDrawable, RopeDrawables};
mod update;
pub use update::VisualEvent;
#[cfg(test)]
pub(in crate::skirmish) mod tests;

pub struct Rules {
    pub lift: f32,
    pub lift_damaged: f32,
    pub gravity: f32,
    pub damping: f32,
    pub speed_limit_z: f32,
    pub taxi: Locomotor,
    pub taxi_speed: f32,
    pub drop: DropRules,
    pub rotor_wash: String,
    pub upgraded_supply_boost: i32,
}

pub struct DropRules {
    pub rope_name: String,
    pub ropes: u32,
    pub width: f32,
    pub color: [f32; 3],
    pub wobble_length: f32,
    pub wobble_amplitude: f32,
    pub wobble_rate: f32,
    pub final_height: f32,
    pub drop_speed: f32,
    pub rappel_speed: f32,
    pub delay: [u32; 2],
    pub min_height: f32,
    pub wait_for_ropes: bool,
}

#[derive(Debug)]
pub(super) enum Flight {
    Flying,
    Landing([f32; 3]),
    Landed,
    TakingOff([f32; 3]),
    CombatDrop,
}

#[derive(Debug)]
pub(in crate::skirmish) enum Phase {
    Takeoff,
    Land,
    MoveLand,
    MoveEvac,
    LandEvac,
    EvacTakeoff,
    RecordCreation,
    MoveEvacExit,
    LandEvacExit,
    EvacExit,
    TakeoffExit,
    HeadOffMap,
    MoveCombat,
    CombatDrop,
}

pub(super) struct State {
    flight: Flight,
    phase: Option<Phase>,
    pub(super) height: f32,
    velocity: f32,
    acceleration: f32,
    pending: Option<super::ai_dispatch::stored_command::Command>,
    airfield: Option<usize>,
    original_position: [f32; 3],
    old_preferred_height: f32,
    combat_destination_z: f32,
    ropes: Vec<rappel::Rope>,
}

impl State {
    pub(super) const fn new(height: f32) -> Self {
        Self {
            flight: Flight::Flying,
            phase: None,
            height,
            velocity: 0.0,
            acceleration: 0.0,
            pending: None,
            airfield: None,
            original_position: [0.0; 3],
            old_preferred_height: 0.0,
            combat_destination_z: 0.0,
            ropes: Vec::new(),
        }
    }

    const fn transitioning(&self) -> bool {
        matches!(
            self.flight,
            Flight::Landing(_) | Flight::TakingOff(_) | Flight::CombatDrop
        )
    }
}

impl Entity {
    // ChinookAIUpdate::getBuildingToIgnoreDuringPathfind: only combat-drop states.
    pub(in crate::skirmish) fn chinook_path_ignored_building(&self) -> Option<usize> {
        if matches!(
            self.ai.command.main_state,
            super::ai_goals::MainState::Chinook
        ) && self
            .chinook
            .as_ref()
            .is_some_and(|state| matches!(state.phase, Some(Phase::MoveCombat | Phase::CombatDrop)))
        {
            self.ai.command.goal_object
        } else {
            None
        }
    }

    pub(in crate::skirmish) fn reset_chinook_dynamic_physics(&mut self) {
        if let Some(state) = &mut self.chinook {
            state.velocity = 0.0;
            state.acceleration = 0.0;
        }
    }

    pub(in crate::skirmish) fn chinook_physics(&self) -> Option<(f32, f32, f32)> {
        self.chinook
            .as_ref()
            .map(|state| (state.height, state.velocity, state.acceleration))
    }

    pub(in crate::skirmish) fn consume_chinook_acceleration(&mut self) {
        if let Some(state) = &mut self.chinook {
            state.acceleration = 0.0;
        }
    }

    pub(in crate::skirmish) fn set_chinook_physics(&mut self, height: f32, velocity: f32) {
        if let Some(state) = &mut self.chinook {
            state.height = height;
            state.velocity = velocity;
        }
    }

    /// Read-only Chinook flight state for native capture diagnostics.
    #[must_use]
    pub fn chinook_diagnostic(&self) -> String {
        self.chinook.as_ref().map_or_else(String::new, |state| {
            format!(
                "flight={:?} flight_phase={:?} pending={} combat_destination_z={:.3} attached_ropes={}",
                state.flight,
                state.phase,
                state.pending.is_some(),
                state.combat_destination_z,
                state.ropes.len()
            )
        })
    }

    #[must_use]
    pub fn landed(&self) -> bool {
        self.chinook
            .as_ref()
            .is_some_and(|state| matches!(state.flight, Flight::Landed))
    }

    pub(in crate::skirmish) fn chinook_idle(&self) -> bool {
        self.chinook.as_ref().is_none_or(|state| {
            state.pending.is_none() && (!self.landed() || self.containment.waiting.is_empty())
        })
    }

    #[must_use]
    pub fn flight_height(&self) -> Option<f32> {
        self.chinook
            .as_ref()
            .map(|state| state.height)
            .or_else(|| self.flight.as_ref().map(|state| state.height))
    }
}
