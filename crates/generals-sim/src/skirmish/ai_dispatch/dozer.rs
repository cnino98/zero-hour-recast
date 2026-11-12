//! SH DozerAIUpdate.cpp: separate primary and persistent per-task action machines.
use crate::skirmish::{Order, Skirmish};

mod action;
mod construction;
mod tasks;

/// Native Dozer UI callbacks, retained until the presentation frame consumes them.
pub enum Feedback {
    Construction {
        frame: u64,
        owner: usize,
        builder: usize,
        builder_type: usize,
        object_type: usize,
        position: [f32; 3],
    },
    Repair {
        frame: u64,
    },
}

pub(in crate::skirmish) const BUILD: usize = 0;
pub(in crate::skirmish) const REPAIR: usize = 1;

struct Task {
    target: Option<usize>,
    frame: u32,
    docks: [Option<[f32; 3]>; 3],
}

impl Task {
    const fn new() -> Self {
        Self {
            target: None,
            frame: 0,
            docks: [None; 3],
        }
    }
}

enum Phase {
    Idle,
    Pick,
    Move,
    Action,
}

enum BuildPhase {
    Select,
    Moving,
    Building,
}

enum Result {
    Continue,
    Success,
    Failure,
}

pub(in crate::skirmish) struct State {
    tasks: [Task; 3],
    current: Option<usize>,
    previous: Option<(usize, usize)>,
    phase: Phase,
    build_phase: BuildPhase,
    goal_object: Option<usize>,
    goal_position: [f32; 3],
    idle_since: u32,
    revision: u64,
    building_sound: Option<(usize, String)>,
    rebuilding: bool,
}

impl State {
    fn new(frame: u32) -> Self {
        Self {
            tasks: std::array::from_fn(|_| Task::new()),
            current: None,
            previous: None,
            phase: Phase::Idle,
            build_phase: BuildPhase::Select,
            goal_object: None,
            goal_position: [0.0; 3],
            idle_since: frame,
            revision: 0,
            building_sound: None,
            rebuilding: false,
        }
    }

    fn most_recent(&self) -> Option<usize> {
        let mut frame = 0;
        let mut selected = None;
        for (index, task) in self.tasks.iter().enumerate() {
            if task.target.is_some() && task.frame > frame {
                frame = task.frame;
                selected = Some(index);
            }
        }
        selected
    }
}

impl crate::skirmish::Entity {
    /// Native `DozerAIInterface::getTaskTarget(BUILD)`, independent of movement.
    #[must_use]
    pub const fn build_task_target(&self) -> Option<usize> {
        if let Some(state) = &self.ai.dozer {
            return state.tasks[BUILD].target;
        }
        if let Order::Build(target) = self.ai.order {
            Some(target)
        } else {
            None
        }
    }

    /// Native `DozerAIInterface::getTaskTarget(REPAIR)`, independent of movement.
    #[must_use]
    pub const fn repair_task_target(&self) -> Option<usize> {
        if let Some(state) = &self.ai.dozer {
            return state.tasks[REPAIR].target;
        }
        if let Order::Repair(target) = self.ai.order {
            Some(target)
        } else {
            None
        }
    }

    pub(in crate::skirmish) fn has_pending_dozer_task(&self) -> bool {
        self.ai.dozer.as_ref().map_or(
            matches!(self.ai.order, Order::Build(_) | Order::Repair(_)),
            |state| state.tasks.iter().any(|task| task.target.is_some()),
        )
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn dozer_accepts_move_aside(
        &mut self,
        id: usize,
        other: usize,
    ) -> bool {
        let worker = self.worker_interface(id);
        let actor = &mut self.entities[id];
        let Some(state) = &actor.ai.dozer else {
            return true;
        };
        actor.drawable.model_conditions.constructing = false;
        worker
            || state.current.is_none()
            || self.entities.get(other).is_some_and(|actor| {
                self.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "DOZER")
            })
    }

    pub(in crate::skirmish) fn construct_dozer_module(&mut self, id: usize, registration: usize) {
        self.construct_ai_module(id, registration);
        self.entities[id].ai.dozer = Some(State::new(self.update_frame()));
        self.set_mine_clearing(id, true);
    }

    pub(in crate::skirmish) fn update_dozer_module(&mut self, id: usize) -> i32 {
        self.update_ai_module(id);
        if self.entities[id].is_effectively_dead() {
            return 1;
        }
        self.update_dozer_tasks(id);
        1
    }

    pub(in crate::skirmish) fn update_dozer_tasks(&mut self, id: usize) {
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        if let Some(task) = state.current {
            let target = state.tasks[task]
                .target
                .filter(|&target| self.entities[target].exists());
            let invalid = match task {
                BUILD => target.is_none(),
                REPAIR => target.is_none_or(|target| {
                    !self.repair_target(
                        id,
                        target,
                        self.entities[id].ai.command.last_command_source != 0,
                    )
                }),
                _ => false,
            };
            if invalid {
                self.cancel_dozer_task(id, task, false);
            }
        } else if !self.worker_interface(id) {
            self.set_mine_clearing(id, true);
        }
        self.update_dozer_machine(id);
    }

    fn reset_dozer_machine(&mut self, id: usize) {
        self.idle_workers.retain(|&(worker, _)| worker != id);
        let now = self.update_frame();
        let state = self.entities[id]
            .ai
            .dozer
            .as_mut()
            .expect("Dozer constructor");
        state.current = None;
        state.phase = Phase::Idle;
        state.idle_since = now;
        state.revision = state
            .revision
            .checked_add(1)
            .expect("Dozer state revision overflow");
        self.set_mine_clearing(id, true);
    }

    fn update_dozer_machine(&mut self, id: usize) {
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        let revision = state.revision;
        let result = match state.phase {
            Phase::Idle => {
                self.update_dozer_idle(id);
                Result::Continue
            }
            Phase::Pick => self.pick_dozer_action_position(id),
            Phase::Move => self.move_dozer_action_position(id),
            Phase::Action => self.perform_dozer_action(id),
        };
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        if state.revision != revision {
            return;
        }
        match state.phase {
            Phase::Idle => {
                let task = state.most_recent();
                if self.entities[id].ai_idle()
                    && let Some(task) = task
                {
                    self.idle_workers.retain(|&(worker, _)| worker != id);
                    let state = self.entities[id]
                        .ai
                        .dozer
                        .as_mut()
                        .expect("Dozer constructor");
                    state.current = Some(task);
                    state.phase = Phase::Pick;
                    state.goal_object = None;
                }
            }
            Phase::Pick => match result {
                Result::Success => {
                    self.entities[id]
                        .ai
                        .dozer
                        .as_mut()
                        .expect("Dozer constructor")
                        .phase = Phase::Move;
                }
                Result::Failure => self.reset_dozer_machine(id),
                Result::Continue => {}
            },
            Phase::Move => {
                let state = self.entities[id]
                    .ai
                    .dozer
                    .as_mut()
                    .expect("Dozer constructor");
                match result {
                    Result::Success => {
                        state.phase = Phase::Action;
                        state.build_phase = BuildPhase::Select;
                    }
                    Result::Failure => state.phase = Phase::Pick,
                    Result::Continue => {}
                }
            }
            Phase::Action => {
                if !matches!(result, Result::Continue) {
                    self.reset_dozer_machine(id);
                }
            }
        }
    }

    fn update_dozer_idle(&mut self, id: usize) {
        let idle = self.entities[id].ai_idle();
        let marked = self.idle_workers.iter().any(|&(worker, _)| worker == id);
        if idle && !marked && !self.entities[id].is_effectively_dead() {
            self.idle_workers.push((id, self.entities[id].owner));
            self.set_mine_clearing(id, true);
        }
        if marked && (!idle || self.entities[id].is_effectively_dead()) {
            self.idle_workers.retain(|&(worker, _)| worker != id);
        }
        let now = self.update_frame();
        let type_index = self.entities[id].type_index;
        let state = self.entities[id]
            .ai
            .dozer
            .as_mut()
            .expect("Dozer constructor");
        if !idle {
            state.idle_since = now;
        }
        let rules = self.catalog[type_index].maintenance.idle_repair.as_ref();
        let Some(rules) = rules else {
            return;
        };
        let elapsed =
            crate::skirmish::progression::integer_real(now.wrapping_sub(state.idle_since));
        if elapsed <= rules.frames || state.tasks.iter().any(|task| task.target.is_some()) {
            return;
        }
        state.idle_since = now;
        if let Some(target) = self.idle_repair_target(id) {
            self.begin_repair(id, target, true).ok();
        } else {
            self.set_mine_clearing(id, true);
            let rules = self.catalog[self.entities[id].type_index]
                .maintenance
                .idle_repair
                .as_ref()
                .expect("Dozer rules");
            let range = if self.computer_player(self.entities[id].owner) {
                rules.range * self.ai_rules.dozer_bored_radius_modifier
            } else {
                rules.range
            };
            self.bored_disarm(id, range);
        }
    }

    pub(in crate::skirmish) fn dozer_player_command(&mut self, id: usize) {
        let Some(state) = &self.entities[id].ai.dozer else {
            return;
        };
        let task = state.current;
        self.entities[id].drawable.model_conditions.constructing = false;
        if let Some(task) = task {
            self.cancel_dozer_task(id, task, false);
        }
    }

    pub(in crate::skirmish) fn finish_dozer_player_command(&mut self, id: usize) {
        if self.entities[id].ai.dozer.is_some() {
            self.reset_dozer_machine(id);
        }
        self.worker_drop_mine_clearing_cargo(id);
    }

    pub(in crate::skirmish) fn worker_drop_mine_clearing_cargo(&mut self, id: usize) {
        let actor = &self.entities[id];
        if self.worker_interface(id)
            && actor.attacking()
            && actor
                .active_weapon(&self.catalog[actor.type_index])
                .anti_mask
                & 0x10
                != 0
        {
            self.entities[id].cargo = 0;
        }
    }

    pub(in crate::skirmish) fn dozer_disabled_edge(&mut self, id: usize, disabled: bool) {
        let Some(state) = &self.entities[id].ai.dozer else {
            return;
        };
        if disabled {
            if let Some(task) = state.current {
                let remember = self.object_disabled_mask(id)
                    & ((1 << 1) | (1 << 2) | (1 << 6) | (1 << 10))
                    != 0;
                self.cancel_dozer_task(id, task, remember);
            }
        } else {
            let previous = self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .previous
                .take();
            if let Some((task, target)) = previous
                && self.entities[target].exists()
                && (task != BUILD || self.entities[target].under_construction())
            {
                self.new_dozer_task(id, task, target);
            }
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .previous = None;
        }
    }

    pub(in crate::skirmish) fn cancel_all_dozer_tasks(&mut self, id: usize) {
        let Some(state) = &mut self.entities[id].ai.dozer else {
            return;
        };
        state.previous = None;
        for task in 0..3 {
            self.finish_dozer_task(id, task, false);
            self.dozer_idle_command(id);
        }
        self.reset_dozer_machine(id);
    }

    pub(in crate::skirmish) fn delete_dozer(&mut self, id: usize) {
        for task in 0..3 {
            if self.entities[id]
                .ai
                .dozer
                .as_ref()
                .expect("Dozer constructor")
                .tasks[task]
                .target
                .is_some()
            {
                self.cancel_dozer_task(id, task, false);
            }
        }
        self.idle_workers.retain(|&(worker, _)| worker != id);
    }

    fn dozer_idle_command(&mut self, id: usize) {
        self.ai_idle_from_ai(id);
    }
}

#[cfg(test)]
mod tests;
