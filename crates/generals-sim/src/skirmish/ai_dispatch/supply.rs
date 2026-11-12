//! SH SupplyTruckAIUpdate.cpp: the independent five-state supply machine.
use crate::skirmish::{Skirmish, ai_goals::COMMAND_FROM_PLAYER, economy::DockTarget};

enum Phase {
    Busy,
    Idle,
    Wanting,
    Regrouping,
    Docking,
}

enum WorkerMode {
    Dozer,
    Supply,
}

enum Result {
    Continue,
    Success,
    Failure,
}

pub(in crate::skirmish) struct State {
    pub preferred_dock: Option<DockTarget>,
    pub force_wanting: bool,
    pub force_busy: bool,
    phase: Option<Phase>,
    worker: Option<WorkerMode>,
}

impl State {
    pub const fn new() -> Self {
        Self {
            preferred_dock: None,
            force_wanting: false,
            force_busy: false,
            phase: None,
            worker: None,
        }
    }

    pub const fn constructed(&self) -> bool {
        self.phase.is_some()
    }

    pub const fn ferrying(&self) -> bool {
        matches!(self.phase, Some(Phase::Wanting | Phase::Docking))
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_supply_module(&mut self, id: usize, registration: usize) {
        self.construct_ai_module(id, registration);
        // The constructor cannot see its own AI interface during Busy::onEnter.
        self.entities[id].ai.supply.phase = Some(Phase::Busy);
        self.entities[id].cargo = 0;
    }

    pub(in crate::skirmish) fn construct_worker_module(&mut self, id: usize, registration: usize) {
        self.construct_dozer_module(id, registration);
        let state = &mut self.entities[id].ai.supply;
        state.phase = Some(Phase::Busy);
        state.worker = Some(WorkerMode::Dozer);
        self.entities[id].cargo = 0;
    }

    pub(in crate::skirmish) fn update_worker_module(&mut self, id: usize) -> i32 {
        self.update_ai_module(id);
        if self.entities[id].is_effectively_dead() {
            return 1;
        }
        self.resolve_worker_mode(id);
        // The zero-argument ActAs*::onExit methods in SH do not override State's
        // onExit(StateExitType); native mode transitions do not reset either brain.
        if matches!(self.entities[id].ai.supply.worker, Some(WorkerMode::Dozer)) {
            self.update_dozer_tasks(id);
        } else {
            self.update_supply_machine(id);
            self.set_mine_clearing(id, true);
        }
        1
    }

    fn resolve_worker_mode(&mut self, id: usize) {
        let actor = &self.entities[id];
        let wants_supply = actor.ai.supply.force_wanting || actor.ai.docking.is_some();
        let mode = actor.ai.supply.worker.as_ref().expect("Worker constructor");
        let next = match mode {
            WorkerMode::Dozer if wants_supply => Some(WorkerMode::Supply),
            WorkerMode::Supply
                if !wants_supply && matches!(actor.ai.supply.phase, Some(Phase::Busy)) =>
            {
                Some(WorkerMode::Dozer)
            }
            _ => None,
        };
        if let Some(next) = next {
            self.entities[id].ai.supply.worker = Some(next);
        }
    }

    pub(in crate::skirmish) fn worker_leaving_supply(&mut self, id: usize) {
        if matches!(self.entities[id].ai.supply.worker, Some(WorkerMode::Supply)) {
            self.ai_idle_from_ai(id);
            self.entities[id].ai.supply.worker = Some(WorkerMode::Dozer);
            self.resolve_worker_mode(id);
        }
    }

    pub(in crate::skirmish) fn worker_interface(&self, id: usize) -> bool {
        self.entities[id].ai.supply.worker.is_some()
    }

    pub(in crate::skirmish) fn update_supply_module(&mut self, id: usize) -> i32 {
        self.update_supply_machine(id);
        // Unlike Dozer and Worker, SupplyTruck updates its own machine first.
        self.update_ai_module(id).min(1)
    }

    pub(in crate::skirmish) fn update_supply_machine(&mut self, id: usize) {
        let result = match self.entities[id]
            .ai
            .supply
            .phase
            .as_ref()
            .expect("SupplyTruck constructor")
        {
            Phase::Wanting => self.supply_wanting_update(id),
            Phase::Regrouping if self.entities[id].ai_idle() => Result::Success,
            Phase::Busy | Phase::Idle | Phase::Regrouping | Phase::Docking => Result::Continue,
        };
        self.resolve_supply_transitions(id, result);
    }

    fn resolve_supply_transitions(&mut self, id: usize, mut result: Result) {
        for _ in 0..19 {
            let state = &self.entities[id].ai.supply;
            let phase = state.phase.as_ref().expect("SupplyTruck machine");
            let idle = self.entities[id].ai_idle();
            let docking = self.entities[id].ai.docking.is_some();
            let busy = !idle && !docking;
            let next = match result {
                Result::Success => Some(match phase {
                    Phase::Regrouping => Phase::Wanting,
                    Phase::Busy | Phase::Idle | Phase::Wanting | Phase::Docking => Phase::Busy,
                }),
                Result::Failure => Some(match phase {
                    Phase::Wanting => Phase::Regrouping,
                    Phase::Busy | Phase::Idle | Phase::Regrouping | Phase::Docking => Phase::Busy,
                }),
                Result::Continue => match phase {
                    Phase::Busy if idle => Some(Phase::Idle),
                    Phase::Idle | Phase::Docking if state.force_busy => Some(Phase::Busy),
                    Phase::Idle if state.force_wanting => Some(Phase::Wanting),
                    Phase::Busy | Phase::Idle | Phase::Wanting if docking => Some(Phase::Docking),
                    Phase::Regrouping
                        if self.entities[id].ai.command.last_command_source
                            == COMMAND_FROM_PLAYER =>
                    {
                        Some(Phase::Busy)
                    }
                    Phase::Docking if idle && self.supply_available(id) => Some(Phase::Wanting),
                    Phase::Idle | Phase::Wanting | Phase::Docking if busy => Some(Phase::Busy),
                    _ => None,
                },
            };
            let Some(next) = next else {
                return;
            };
            self.entities[id].ai.supply.phase = Some(next);
            result = self.enter_supply_phase(id);
        }
        panic!("SupplyTruck exceeded native immediate-transition limit");
    }

    fn enter_supply_phase(&mut self, id: usize) -> Result {
        match self.entities[id]
            .ai
            .supply
            .phase
            .as_ref()
            .expect("SupplyTruck machine")
        {
            Phase::Busy => self.entities[id].ai.supply.force_busy = false,
            Phase::Idle => self.worker_leaving_supply(id),
            Phase::Wanting | Phase::Docking => self.entities[id].ai.supply.force_wanting = false,
            Phase::Regrouping => return self.enter_supply_regroup(id),
        }
        Result::Continue
    }

    pub(in crate::skirmish) fn supply_available(&self, id: usize) -> bool {
        self.entities[id].chinook.is_none()
            || self.catalog[self.entities[id].type_index]
                .contain_family()
                .is_some()
                && !self.container_waiting(id)
                && self.passengers(id).is_empty()
                && !matches!(
                    self.catalog[self.entities[id].type_index].contain_family(),
                    Some("OverlordContain" | "HelixContain")
                )
    }

    fn supply_wanting_update(&mut self, id: usize) -> Result {
        assert!(
            self.supply_available(id),
            "Wanting requires available SupplyTruck interface"
        );
        let target = if self.entities[id].cargo > 0 {
            self.closest_supply_center(id).map(DockTarget::Center)
        } else {
            self.best_supply(id).map(DockTarget::Warehouse)
        };
        let Some(target) = target else {
            return Result::Failure;
        };
        self.ai_dock(id, target, crate::skirmish::ai_goals::COMMAND_FROM_AI);
        Result::Success
    }

    fn enter_supply_regroup(&mut self, id: usize) -> Result {
        self.entities[id].ai.ignored_producer = None;
        let destination = ["CASH_GENERATOR", "COMMANDCENTER", "STRUCTURE"]
            .into_iter()
            .find_map(|kind| self.closest_supply_regroup_object(id, kind));
        let Some(destination) = destination else {
            return Result::Failure;
        };
        let actor = &self.entities[id];
        let target = &self.entities[destination];
        let dx = actor.position[0] - target.position[0];
        let dy = actor.position[1] - target.position[1];
        let xx = dx * dx;
        let yy = dy * dy;
        let radii = self.object_radius(id) + self.object_radius(destination);
        let distance = ((xx + yy).sqrt() - radii).max(0.0);
        if distance * distance < 225.0 {
            return Result::Continue;
        }
        let Some(position) = self.supply_regroup_position(destination) else {
            return Result::Failure;
        };
        self.construction_move(id, position);
        Result::Continue
    }

    fn closest_supply_regroup_object(&self, id: usize, kind: &str) -> Option<usize> {
        let actor = &self.entities[id];
        let origin = self.object_position(id);
        let mut closest = None;
        let mut closest_distance = f32::MAX;
        for target in self.team_membership.player_objects(actor.owner) {
            if !self.attack_kind(target, kind) {
                continue;
            }
            let delta = std::array::from_fn::<_, 3, _>(|axis| {
                self.object_position(target)[axis] - origin[axis]
            });
            let squared = delta.map(|component| component * component);
            let distance = squared[0] + squared[1];
            if distance < closest_distance {
                closest_distance = distance;
                closest = Some(target);
            }
        }
        closest
    }
}
