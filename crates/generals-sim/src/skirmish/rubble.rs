//! `ActiveBody::setCorrectDamageState`, native rubble cells and construction removal.
use super::Skirmish;
use crate::Grid;
use std::collections::BTreeSet;
#[cfg(test)]
mod tests;

/// `TerrainVisual::removeTreesAndPropsForConstruction` after object removal.
pub struct ConstructionClear {
    pub geometry: crate::collision_geometry::Geometry,
    pub position: [f32; 3],
    pub angle: f32,
}

impl Skirmish {
    pub(super) fn update_body_rubble(&mut self, id: usize) {
        let actor = &self.entities[id];
        if actor.alive()
            || !actor.exists()
            || self.catalog[actor.type_index]
                .maintenance
                .rubble_height
                .is_none()
        {
            return;
        }
        if self.set_rubble_geometry(id) {
            self.reclassify_rubble_obstacle(id);
            self.rebuild_navigation();
        }
    }

    #[must_use]
    pub fn retained_rubble(&self, id: usize) -> bool {
        self.entities.get(id).is_some_and(|actor| {
            !actor.alive()
                && actor.exists()
                && !actor.masked()
                && self.catalog[actor.type_index]
                    .maintenance
                    .rubble_height
                    .is_some()
        })
    }

    pub(super) fn navigation_for(&self, id: usize) -> &Grid {
        let actor = &self.entities[id];
        self.navigation_for_surfaces(actor.locomotor_surfaces(&self.catalog[actor.type_index]))
    }

    pub(super) fn navigation_for_type(&self, index: usize) -> &Grid {
        self.navigation_for_surfaces(self.catalog[index].locomotor_surfaces())
    }

    fn navigation_for_surfaces(&self, surfaces: u8) -> &Grid {
        if surfaces & 4 != 0 {
            return self.cliff_navigation.as_ref().unwrap_or(&self.navigation);
        }
        if surfaces & (16 | 8) == 0 {
            self.rubble_navigation.as_ref().unwrap_or(&self.navigation)
        } else {
            &self.navigation
        }
    }

    pub(super) fn rebuild_rubble(&mut self) {
        let cells: BTreeSet<_> = self.movement_terrain.rubble_cells().collect();
        self.rubble_navigation =
            (!cells.is_empty()).then(|| self.navigation.with_surface_barriers(cells));
    }

    pub(super) fn clear_rubble_for_construction(
        &mut self,
        index: usize,
        position: [f32; 2],
        angle: f32,
    ) {
        let position = [position[0], position[1], self.ground_height(position)];
        self.clear_removable_for_construction(index, position, angle);
    }

    pub(super) fn clear_removable_for_construction(
        &mut self,
        index: usize,
        position: [f32; 3],
        angle: f32,
    ) {
        let candidates =
            self.construction_collisions(&self.catalog[index].geometry, position, angle);
        let mut removed = false;
        for id in candidates {
            if !self.removable_for_construction(id) {
                continue;
            }
            let actor = &self.entities[id];
            let unit = &self.catalog[actor.type_index];
            if !unit.kind_of.iter().any(|name| name == "ALWAYS_SELECTABLE") {
                self.destroy_maintenance_object(id);
                removed = true;
            }
        }
        if removed {
            // BuildAssistant queues removable objects for destruction. Their
            // rubble cells remain until the destruction list is processed;
            // refreshing this view does not retire or mask those objects.
            self.rebuild_rubble();
        }
        let geometry = &self.catalog[index].geometry;
        self.construction_clears.push(ConstructionClear {
            geometry: crate::collision_geometry::Geometry {
                shape: geometry.shape,
                is_small: geometry.is_small,
                major: geometry.major,
                minor: geometry.minor,
                height: geometry.height,
            },
            position,
            angle,
        });
    }

    pub(super) fn removable_for_construction(&self, id: usize) -> bool {
        let actor = &self.entities[id];
        let flags = &self.catalog[actor.type_index].kind_of;
        actor.exists()
            && !flags.iter().any(|flag| flag == "INERT")
            && (!actor.alive()
                || flags
                    .iter()
                    .any(|flag| matches!(flag.as_str(), "SHRUBBERY" | "CLEARED_BY_BUILD")))
    }
}
