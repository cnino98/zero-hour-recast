//! SH `NeutronBlastBehavior`: a dormant update and ordered death callback.
use super::{DeathType, Skirmish, TerrainDecal};

pub struct Rules {
    pub radius: f32,
    pub affect_airborne: bool,
    pub affect_allies: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn neutron_blast_on_die(&mut self, source: usize, ordinal: usize) {
        let rules = &self.catalog[self.entities[source].type_index]
            .behavior
            .neutron_blast[ordinal];
        let radius = rules.radius;
        let hit_air = rules.affect_airborne;
        let allies = rules.affect_allies;
        let position = self.entities[source].position;
        let squared_radius = radius * radius;
        // PartitionManager materializes distance/Alive/SameMapStatus before any
        // onDie callback can mutate another candidate. Air/alliance checks follow.
        let targets: Vec<_> = self
            .partition
            .fastest_range_objects(position, radius)
            .into_iter()
            .filter(|&id| {
                let actor = &self.entities[id];
                let dx = actor.position[0] - position[0];
                let dy = actor.position[1] - position[1];
                let xx = dx * dx;
                let yy = dy * dy;
                !actor.is_effectively_dead()
                    && actor.is_off_map() == self.entities[source].is_off_map()
                    && xx + yy < squared_radius
            })
            .collect();
        for id in targets {
            if id == source
                || (!hit_air
                    && (self.attack_kind(id, "AIRCRAFT") || self.entities[id].is_airborne_target()))
            {
                continue;
            }
            // Object::getRelationship hides either end's undetected defection
            // before asking the live teams for their relationship (Object.cpp1593).
            let protected_ally = self.team_membership.team_of(source).is_some()
                && !self.entities[source].helpers.defection.undetected
                && (self.entities[id].helpers.defection.undetected
                    || self.object_relationship(source, id) == 2);
            if !allies && protected_ally {
                continue;
            }
            self.neutron_blast_object(id);
        }
    }

    fn neutron_blast_object(&mut self, id: usize) {
        if self.attack_kind(id, "INFANTRY") {
            self.kill_object(id, DeathType::Normal);
        }
        self.kill_all_contained(id);
        if self.attack_kind(id, "VEHICLE") && !self.attack_kind(id, "DRONE") {
            if self.attack_kind(id, "CLIFF_JUMPER") {
                self.kill_object(id, DeathType::Normal);
            } else {
                self.set_unmanned(id);
                if self.entities[id].ai.registration.is_some() {
                    self.ai_idle_from_ai(id);
                }
                self.deselect_dead_object(id);
                if self.entities[id].drawable.present {
                    self.entities[id].drawable.terrain_decal = TerrainDecal::None;
                }
                self.transfer_maintenance_owner(id, self.neutral_owner());
            }
        }
    }
}

#[cfg(test)]
mod tests;
