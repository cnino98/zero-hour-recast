//! `GameLogic` map walls and `Pathfinder::classifyObjectFootprint(false)`.
use crate::skirmish::{DamageType, DeathType, Footprint, Skirmish};

impl Skirmish {
    fn wall_footprints(&self) -> Vec<Footprint> {
        self.movement_terrain
            .wall_piece_ids()
            .filter(|id| {
                self.entities
                    .get(*id)
                    .is_some_and(|actor| !actor.maintenance.deleted)
            })
            .map(|id| self.object_footprint(id))
            .collect()
    }

    pub(in crate::skirmish) fn allocate_map_wall_layer(&mut self) {
        let footprints = self.wall_footprints();
        self.movement_terrain
            .allocate_wall_layer(self.ai_rules.wall_height, &footprints);
    }

    pub(in crate::skirmish) fn remove_wall_footprint(&mut self, id: usize) {
        if !self.attack_kind(id, "WALK_ON_TOP_OF_WALL")
            || !self.movement_terrain.bridge_layer_active(15)
        {
            return;
        }
        self.movement_terrain.invalidate_wall_piece(id);
        let footprint = self.object_footprint(id);
        // Native object-list order is newest first. Damage callbacks can mutate
        // later victims; test each live layer/position at its turn in that order.
        for victim in (0..self.entities.len()).rev() {
            if !self.entities[victim].maintenance.deleted
                && self.entities[victim].layer == 15
                && crate::terrain::walls::contains(&footprint, self.entities[victim].position)
            {
                self.apply_weapon_hit(
                    victim,
                    victim,
                    999_999.0,
                    DamageType::Falling,
                    DeathType::Splatted,
                );
            }
        }
        let footprints = self.wall_footprints();
        self.movement_terrain.classify_wall_cells(&footprints);
    }
}

#[cfg(test)]
mod tests;
