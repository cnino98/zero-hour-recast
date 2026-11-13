//! Native `AIUpdate` interface state, state-machine continuations and locomotion.
use crate::grid::Cell;
use crate::skirmish::{
    AiRules, ConstructionPhase, Order, Skirmish, UnitType, ai, ai_goals, blocking, builder_disarm,
    economy, locomotor, path_following, path_service, turret,
};

/// State for the actual AI module. A native object has one AI interface.
/// Registration index is its authored UPDATE identity, never its Entity index.
pub struct State {
    pub(in crate::skirmish) registration: Option<usize>,
    pub(in crate::skirmish) completed_waypoint: Option<i32>,
    pub(in crate::skirmish) follow_waypoint: Option<super::follow_waypoint::State>,
    pub(in crate::skirmish) current_victim: Option<usize>,
    pub(in crate::skirmish) ai_dead: bool,
    pub(in crate::skirmish) in_update: bool,
    pub(in crate::skirmish) primary: super::primary::State,
    pub(in crate::skirmish) recruitable: bool,
    pub(in crate::skirmish) idle: Idle,
    pub(in crate::skirmish) hunt: Option<super::hunt::State>,
    pub(in crate::skirmish) rappel: Option<super::rappel::State>,
    pub(in crate::skirmish) dozer: Option<super::dozer::State>,
    pub(in crate::skirmish) command_revision: u64,
    pub(in crate::skirmish) command: ai_goals::Command,
    pub(in crate::skirmish) motion: locomotor::GroundMotion,
    pub(in crate::skirmish) surface_locomotor: usize,
    pub(in crate::skirmish) locomotor_set: u8,
    pub(in crate::skirmish) surface_motion: Vec<locomotor::SavedGroundState>,
    pub(in crate::skirmish) blocking: blocking::Blocking,
    pub(in crate::skirmish) path_request: path_service::State,
    pub order: Order,
    pub(in crate::skirmish) waypoints: ai::Waypoints,
    pub(in crate::skirmish) ai_target: Option<usize>,
    pub(in crate::skirmish) attack_context: builder_disarm::Attack,
    pub(in crate::skirmish) mood: ai::Mood,
    pub(in crate::skirmish) guard: ai::GuardState,
    pub(in crate::skirmish) attack_priority: Option<usize>,
    pub(in crate::skirmish) path: crate::grid::Route,
    pub(in crate::skirmish) path_goal: Option<Cell>,
    pub(in crate::skirmish) path_endpoint: Option<[f32; 2]>,
    pub(in crate::skirmish) ground_path: Option<path_following::Path>,
    pub(in crate::skirmish) construction_dock: Option<[f32; 2]>,
    pub(in crate::skirmish) construction_phase: ConstructionPhase,
    pub(in crate::skirmish) action_frames: f32,
    pub(in crate::skirmish) docking: Option<economy::Docking>,
    pub(in crate::skirmish) supply: economy::SupplyAi,
    pub turret: turret::State,
    pub secondary_turret: Option<turret::State>,
    pub(in crate::skirmish) attack: Option<crate::skirmish::combat::attack::State>,
    pub(in crate::skirmish) attack_move: super::attack_move::State,
    pub(in crate::skirmish) ignored_producer: Option<usize>,
}

pub(in crate::skirmish) struct Idle {
    pub(in crate::skirmish) needs_init: bool,
    pub(in crate::skirmish) sleep_offset: u32,
}

impl State {
    pub(in crate::skirmish) fn new(
        unit: &UnitType,
        tick: u64,
        position: [f32; 2],
        rules: &AiRules,
    ) -> Self {
        Self {
            registration: None,
            completed_waypoint: None,
            follow_waypoint: None,
            current_victim: None,
            ai_dead: false,
            in_update: false,
            primary: super::primary::State {
                initialized: false,
                sleep_until: 0,
            },
            recruitable: true,
            idle: Idle {
                needs_init: false,
                sleep_offset: 0,
            },
            hunt: None,
            rappel: None,
            dozer: None,
            command_revision: 0,
            command: ai_goals::Command::new(),
            motion: locomotor::GroundMotion::new(tick),
            surface_locomotor: 0,
            locomotor_set: locomotor::set::NORMAL,
            surface_motion: Vec::new(),
            blocking: blocking::Blocking::new(),
            path_request: path_service::State::new(),
            order: Order::Idle,
            waypoints: ai::Waypoints::new(),
            ai_target: None,
            attack_context: builder_disarm::Attack::new(),
            mood: ai::Mood::new(tick + u64::from(rules.force_idle_frames)),
            guard: ai::GuardState::new(position),
            attack_priority: None,
            path: crate::grid::Route::new(),
            path_goal: None,
            path_endpoint: None,
            ground_path: None,
            construction_dock: None,
            construction_phase: ConstructionPhase::Approach,
            action_frames: 0.0,
            docking: None,
            supply: economy::SupplyAi::new(),
            turret: turret::State::new(unit.turret.as_ref()),
            secondary_turret: unit
                .secondary_turret
                .as_ref()
                .map(|rules| turret::State::new(Some(rules))),
            attack: None,
            attack_move: super::attack_move::State::new(),
            ignored_producer: None,
        }
    }
}

impl Skirmish {
    /// AI constructor's NONE wake, after this actual module's handle allocation.
    pub(in crate::skirmish) fn construct_ai_module(&mut self, id: usize, registration: usize) {
        assert!(
            self.entities[id].ai.registration.is_none(),
            "multiple native AI interfaces"
        );
        self.entities[id].ai.registration = Some(registration);
        self.entities[id].ai.mood.next_check = 0;
        self.entities[id].ai.mood.offset_check = false;
        self.construct_ai_turret(id);
        self.initialize_locomotor_set(id);
        self.wake_update(id, registration, 1);
    }

    /// `AIStateMachine` notifyStateChanged -> wakeUpNow. Native ignores this
    /// inside its callback; the outer callback combines the retained child clocks.
    pub(in crate::skirmish) fn ai_state_changed(&mut self, id: usize) {
        self.wake_ai_now(id);
    }

    pub(in crate::skirmish) fn wake_ai_now(&mut self, id: usize) {
        if !self.entities[id].ai.in_update
            && let Some(registration) = self.entities[id].ai.registration
            && self.update_remaining_sleep(id, registration) > 1
        {
            self.wake_update(id, registration, 1);
        }
    }
}

#[cfg(test)]
mod tests;
