//! SH `DozerAIUpdate::construct` / `WorkerAIUpdate::construct`, `isRebuild` branch.
use super::{BUILD, Skirmish};
use crate::skirmish::{BodyKind, ConstructionState};

impl Skirmish {
    pub(in crate::skirmish) fn set_dozer_rebuilding(&mut self, id: usize, rebuilding: bool) {
        if let Some(state) = &mut self.entities[id].ai.dozer {
            state.rebuilding = rebuilding;
        }
    }

    pub(in crate::skirmish) fn dozer_is_rebuilding(&self, id: usize) -> bool {
        self.entities[id]
            .ai
            .dozer
            .as_ref()
            .is_some_and(|state| state.rebuilding)
    }

    pub(in crate::skirmish) fn construct_rebuilding_object(
        &mut self,
        worker: usize,
        template: usize,
        position: [f32; 3],
        angle: f32,
    ) -> usize {
        self.set_dozer_rebuilding(worker, true);
        let owner = self.entities[worker].owner;
        // Initial statuses precede constructors and CREATE, but the original body's
        // initial health remains intact until construct's internalChangeHealth call.
        let id = self.allocate_object(template, owner, true);
        self.entities[id].build_progress = 0.0;
        self.entities[id].collector_pending = false;
        self.entities[id].object_status |= (1 << 3) | (1 << 21);
        self.map_unit_produced(None, id);
        self.register_object_updates(id);
        self.entities[id].producer = Some(worker);
        self.entities[id].maintenance.builder = Some(worker);
        if self.worker_interface(worker) {
            self.worker_leaving_supply(worker);
        }
        self.set_object_position(id, position);
        self.set_object_orientation(id, angle);
        self.flatten_object_terrain(id);
        let ground = self.ground_height([position[0], position[1]]);
        self.set_object_position(id, [position[0], position[1], ground]);
        self.restore_object_obstacle(id);
        // Player::onStructureCreated is empty in the pinned source.
        self.entities[id].construction_percent = 0.0;
        if !matches!(
            self.catalog[self.entities[id].type_index].body_kind,
            BodyKind::Inactive
        ) {
            let change = -self.entities[id].health + 1.0;
            self.change_body_health(id, change);
        }
        self.entities[id].drawable.model_conditions.construction = ConstructionState::Awaiting;
        self.new_dozer_task(worker, BUILD, id);
        id
    }
}
