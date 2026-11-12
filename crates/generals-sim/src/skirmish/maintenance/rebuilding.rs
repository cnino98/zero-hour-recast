//! SH `RebuildHoleBehavior` / `RebuildHoleExposeDie` and `Object::onDie`'s reconstruction tail.
use super::super::{BodyKind, Skirmish};
use num_traits::ToPrimitive;

pub(super) struct Hole {
    spawner: Option<usize>,
    template: Option<usize>,
    worker_template: Option<usize>,
    worker: Option<usize>,
    foundation: Option<usize>,
    remaining: u32,
}

impl Skirmish {
    pub(in crate::skirmish) fn construct_rebuild_hole(&mut self, id: usize, ordinal: usize) {
        assert_eq!(self.entities[id].maintenance.holes.len(), ordinal);
        self.entities[id].maintenance.holes.push(Hole {
            spawner: None,
            template: None,
            worker_template: None,
            worker: None,
            foundation: None,
            remaining: 0,
        });
    }

    pub(in crate::skirmish) fn map_rebuild_hole(&self, prior: usize) -> Option<usize> {
        self.entities
            .iter()
            .enumerate()
            .rev()
            .find_map(|(id, actor)| {
                (actor.exists()
                    && actor
                        .maintenance
                        .holes
                        .first()
                        .is_some_and(|hole| hole.spawner == Some(prior)))
                .then_some(id)
            })
    }

    pub(in crate::skirmish) fn map_rebuilder(&self, building: usize) -> Option<usize> {
        self.entities
            .iter()
            .enumerate()
            .rev()
            .find_map(|(id, actor)| {
                (actor.exists()
                    && actor
                        .maintenance
                        .holes
                        .first()
                        .is_some_and(|hole| hole.foundation == Some(building)))
                .then_some(id)
            })
    }

    pub(in crate::skirmish) fn expose_rebuild_hole(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let rule = &self.catalog[actor.type_index].maintenance.expose_hole[ordinal];
        if !self.die_conditions_match(id, &rule.conditions)
            || actor.under_construction()
            || actor.owner == self.neutral_owner()
            || self.defeated[actor.owner]
        {
            return;
        }
        // Retained civilian players are active source Players, independently of lobby occupancy.
        let template = actor.type_index;
        let owner = actor.owner;
        let facing = actor.facing;
        let health = rule.health;
        let transfer = rule.transfer_attackers;
        let hole_type = self
            .catalog
            .iter()
            .position(|unit| unit.name == rule.template)
            .expect("validated rebuild-hole template");
        let team = self.team_membership.team_of(id);
        let hole = self.new_object_on_team(hole_type, owner, team);
        self.set_object_position(hole, self.object_position(id));
        self.set_object_orientation(hole, facing);
        self.copy_object_geometry(hole, id);
        self.transfer_crew_name(id, hole);
        self.restore_object_obstacle(hole);
        // BodyModule's default is SAME_CURRENTHEALTH, not FULLY_HEAL.
        if !matches!(self.catalog[hole_type].body_kind, BodyKind::Inactive) {
            self.entities[hole].max_health = health;
            self.entities[hole].damage_state.initial_health = health;
            if self.entities[hole].health > health {
                self.change_body_health(hole, health - self.entities[hole].health);
            }
        }
        self.start_rebuild_process(hole, template, id);
        if transfer {
            self.transfer_rebuild_attackers(id, hole);
        }
    }

    fn start_rebuild_process(&mut self, id: usize, template: usize, spawner: usize) {
        // getRebuildHoleBehaviorInterfaceFromObject returns the first authored instance.
        let Some(hole) = self.entities[id].maintenance.holes.first_mut() else {
            return;
        };
        hole.template = Some(template);
        hole.spawner = Some(spawner);
        let worker = hole.worker;
        let worker = worker.filter(|&id| self.entities[id].exists());
        self.restart_rebuild_worker(id, 0, worker);
    }

    fn restart_rebuild_worker(&mut self, id: usize, ordinal: usize, worker: Option<usize>) {
        if let Some(worker) = worker {
            self.destroy_maintenance_object(worker);
        }
        let frames = self.catalog[self.entities[id].type_index]
            .maintenance
            .rebuild[ordinal]
            .respawn_frames;
        let hole = &mut self.entities[id].maintenance.holes[ordinal];
        hole.worker = None;
        // Native Real -> UnsignedInt truncates; admission bounds the conversion domain.
        hole.remaining = frames.to_u32().expect("validated worker respawn duration");
        self.mask_object(id, false);
    }

    pub(in crate::skirmish) fn reconstruction_on_die(&mut self, id: usize) {
        if self.entities[id].reconstructing()
            && let Some(producer) = self.entities[id]
                .producer
                .filter(|&id| self.entities[id].exists())
        {
            self.start_rebuild_process(producer, self.entities[id].type_index, id);
            self.transfer_rebuild_attackers(id, producer);
        }
    }

    fn transfer_rebuild_attackers(&mut self, previous: usize, next: usize) {
        for id in (0..self.entities.len()).rev() {
            if self.entities[id].exists() && self.entities[id].ai.registration.is_some() {
                self.transfer_native_attack(id, previous, next);
            }
        }
    }

    pub(in crate::skirmish) fn update_rebuild_hole(&mut self, id: usize, ordinal: usize) -> i32 {
        let hole = &self.entities[id].maintenance.holes[ordinal];
        let mut worker = hole.worker.filter(|&id| self.entities[id].exists());
        if hole.worker.is_some() && worker.is_none() {
            self.restart_rebuild_worker(id, ordinal, None);
        }
        let foundation_id = self.entities[id].maintenance.holes[ordinal].foundation;
        let mut foundation = foundation_id.filter(|&id| self.entities[id].exists());
        if foundation_id.is_some() && foundation.is_none() {
            self.restart_rebuild_worker(id, ordinal, worker);
            self.entities[id].maintenance.holes[ordinal].foundation = None;
            // The local native worker pointer survives deferred destroyObject this frame.
        }
        let hole = &mut self.entities[id].maintenance.holes[ordinal];
        if worker.is_none() && hole.remaining > 0 {
            hole.remaining -= 1;
            if hole.remaining == 0 {
                worker = Some(self.create_rebuild_worker(id, ordinal, &mut foundation));
            }
        }
        let actor = &self.entities[id];
        if actor.health < actor.max_health {
            let regeneration =
                self.catalog[actor.type_index].maintenance.rebuild[ordinal].regeneration;
            let amount = (regeneration / 30.0) * actor.max_health;
            self.apply_healing(id, Some(id), amount);
        }
        if let Some(foundation) = foundation.filter(|&id| !self.entities[id].under_construction()) {
            self.transfer_crew_name(id, foundation);
            if let Some(worker) = worker {
                self.destroy_maintenance_object(worker);
            }
            self.destroy_maintenance_object(id);
        }
        1
    }

    fn create_rebuild_worker(
        &mut self,
        id: usize,
        ordinal: usize,
        foundation: &mut Option<usize>,
    ) -> usize {
        let hole = &self.entities[id].maintenance.holes[ordinal];
        let worker_type = hole.worker_template.unwrap_or_else(|| {
            let name = &self.catalog[self.entities[id].type_index]
                .maintenance
                .rebuild[ordinal]
                .worker;
            self.catalog
                .iter()
                .position(|unit| &unit.name == name)
                .expect("validated rebuild worker")
        });
        self.entities[id].maintenance.holes[ordinal].worker_template = Some(worker_type);
        let worker = self.new_object_on_team(
            worker_type,
            self.entities[id].owner,
            self.team_membership.team_of(id),
        );
        self.set_object_position(worker, self.object_position(id));
        self.entities[id].maintenance.holes[ordinal].worker = Some(worker);
        if self.entities[worker].ai.registration.is_none() {
            return worker;
        }
        let building = if let Some(building) = *foundation {
            self.set_last_command_source(worker, crate::skirmish::ai_goals::COMMAND_FROM_AI);
            // Source aiResumeConstruction can decline an already staffed construction site.
            let _ = self.dozer_resume_command(worker, building);
            building
        } else {
            let template = self.entities[id].maintenance.holes[ordinal]
                .template
                .expect("started rebuild process");
            self.construct_rebuilding_object(
                worker,
                template,
                self.object_position(id),
                self.entities[id].facing,
            )
        };
        *foundation = Some(building);
        self.transfer_rebuild_attackers(id, building);
        self.entities[id].maintenance.holes[ordinal].foundation = Some(building);
        self.entities[building].producer = Some(id);
        self.entities[worker].maintenance.instance_flags |= super::INSTANCE_UNSELECTABLE;
        self.mask_object(id, true);
        self.transfer_rebuild_bombs(id, building);
        worker
    }

    pub(in crate::skirmish) fn rebuild_hole_on_die(&mut self, id: usize, ordinal: usize) {
        self.destroy_rebuild_worker(id, ordinal);
        self.destroy_maintenance_object(id);
    }

    fn destroy_rebuild_worker(&mut self, id: usize, ordinal: usize) {
        if let Some(worker) = self.entities[id].maintenance.holes[ordinal]
            .worker
            .filter(|&id| self.entities[id].exists())
        {
            self.destroy_maintenance_object(worker);
            self.entities[id].maintenance.holes[ordinal].worker = None;
        }
    }

    pub(in crate::skirmish) fn destroy_rebuild_workers(&mut self, id: usize) {
        for ordinal in 0..self.entities[id].maintenance.holes.len() {
            self.destroy_rebuild_worker(id, ordinal);
        }
    }
}

#[cfg(test)]
mod tests;
