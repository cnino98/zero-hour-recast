//! `DozerAIUpdate` newTask/cancelTask and persistent dock selection.
use super::{BUILD, REPAIR, Skirmish, Task};

impl Skirmish {
    pub(in crate::skirmish) fn new_dozer_task(&mut self, id: usize, task: usize, target: usize) {
        if !self.entities[target].exists() {
            return;
        }
        if self.worker_interface(id) {
            self.entities[id].ai.supply.preferred_dock = None;
        }
        if task == BUILD || task == REPAIR {
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
            let position = self.dozer_dock_position(id, target);
            if task == BUILD {
                self.entities[target].maintenance.builder = Some(id);
            }
            let center = self.object_position(target);
            let delta = [position[0] - center[0], position[1] - center[1]];
            let squared = delta.map(|value| value * value);
            let length = (squared[0] + squared[1]).sqrt();
            let offset = if length == 0.0 {
                [0.0; 2]
            } else {
                delta.map(|value| value / length * 50.0)
            };
            let end = if self.worker_interface(id) {
                position
            } else {
                [
                    position[0] + offset[0],
                    position[1] + offset[1],
                    position[2],
                ]
            };
            self.entities[id]
                .ai
                .dozer
                .as_mut()
                .expect("Dozer constructor")
                .tasks[task]
                .docks = [Some(position), Some(position), Some(end)];
        }
        let now = self.update_frame();
        let state = self.entities[id]
            .ai
            .dozer
            .as_mut()
            .expect("Dozer constructor");
        state.tasks[task].target = Some(target);
        state.tasks[task].frame = now;
        self.reset_dozer_machine(id);
        self.worker_leaving_supply(id);
    }

    pub(super) fn cancel_dozer_task(&mut self, id: usize, task: usize, remember: bool) {
        let state = self.entities[id]
            .ai
            .dozer
            .as_mut()
            .expect("Dozer constructor");
        state.previous = if remember {
            assert!(state.previous.is_none(), "Dozer already remembers a task");
            state.tasks[task].target.map(|target| (task, target))
        } else {
            None
        };
        self.finish_dozer_task(id, task, false);
        self.dozer_idle_command(id);
        self.reset_dozer_machine(id);
    }

    pub(super) fn finish_dozer_task(&mut self, id: usize, task: usize, completed: bool) {
        if task == BUILD || task == REPAIR {
            self.entities[id].drawable.model_conditions.constructing = false;
        }
        let state = self.entities[id]
            .ai
            .dozer
            .as_mut()
            .expect("Dozer constructor");
        state.tasks[task] = Task::new();
        if completed {
            state.previous = None;
        }
    }

    pub(in crate::skirmish) fn dozer_repair_command(
        &mut self,
        id: usize,
        target: usize,
        from_ai: bool,
    ) -> Result<(), &'static str> {
        self.entities[id].drawable.model_conditions.constructing = false;
        let state = self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor");
        if state.current == Some(REPAIR) && state.tasks[REPAIR].target == Some(target) {
            return Ok(());
        }
        if state.current.is_none() {
            self.dozer_idle_command(id);
        }
        if !self.repair_target(id, target, from_ai) || !self.repair_benefactor(id, target) {
            return Err("Builder cannot repair this object");
        }
        self.new_dozer_task(id, REPAIR, target);
        Ok(())
    }

    pub(in crate::skirmish) fn dozer_resume_command(
        &mut self,
        id: usize,
        target: usize,
    ) -> Result<(), &'static str> {
        self.entities[id].drawable.model_conditions.constructing = false;
        if self.entities[id]
            .ai
            .dozer
            .as_ref()
            .expect("Dozer constructor")
            .current
            .is_none()
        {
            self.dozer_idle_command(id);
        }
        if let Some(builder) = self.entities[target].maintenance.builder
            && self.entities[builder].exists()
            && !self.entities[builder].is_effectively_dead()
            && let Some(state) = &self.entities[builder].ai.dozer
            && state.current == Some(BUILD)
            && state.tasks[BUILD].target == Some(target)
        {
            return Err("Another builder is actively constructing this object");
        }
        self.new_dozer_task(id, BUILD, target);
        Ok(())
    }
}
