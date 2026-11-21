//! Native Sneak Attack placement from `ActionManager` and `BuildAssistant`.
use super::{Skirmish, movement::distance_squared, partition::Identity, visibility::Status};
use crate::{collision_geometry::Body, terrain::CellType};

/// `BuildAssistant::LegalBuildCode` failures used by original placement feedback.
pub enum PlacementFailure {
    Generic,
    RestrictedTerrain,
    NotFlatEnough,
    ObjectsInTheWay,
    TooCloseToSupplies,
    NoClearPath,
    Shroud,
}

impl Skirmish {
    pub(super) fn sneak_attack_location_valid(
        &self,
        caster: usize,
        reference: &str,
        position: [f32; 3],
    ) -> bool {
        let Some(kind) = self.catalog.iter().position(|unit| unit.name == reference) else {
            return false;
        };
        self.can_place_sneak_attack(
            caster,
            kind,
            position,
            self.catalog[kind].placement_angle,
            false,
        )
    }

    /// `InGameUI` previews at the displayed angle with hidden obstructions ignored.
    /// `PlaceEventTranslator` adds `FAIL_STEALTHED_WITHOUT_FEEDBACK` on release.
    #[must_use]
    pub fn can_place_sneak_attack(
        &self,
        caster: usize,
        kind: usize,
        position: [f32; 3],
        angle: f32,
        committing: bool,
    ) -> bool {
        self.sneak_attack_placement_failure(caster, kind, position, angle, committing)
            .is_none()
    }

    /// The original legal-build result determines the localized release feedback.
    #[must_use]
    pub fn sneak_attack_placement_failure(
        &self,
        caster: usize,
        kind: usize,
        position: [f32; 3],
        angle: f32,
        committing: bool,
    ) -> Option<PlacementFailure> {
        let Some(unit) = self.entities.get(caster) else {
            return Some(PlacementFailure::Generic);
        };
        let Some(template) = self.catalog.get(kind) else {
            return Some(PlacementFailure::Generic);
        };
        let point = [position[0], position[1]];
        if !position.into_iter().all(f32::is_finite)
            || !angle.is_finite()
            || self.cell_at(point).is_none()
            || point.iter().enumerate().any(|(axis, value)| {
                *value <= self.playable_extent[0][axis] || *value >= self.playable_extent[1][axis]
            })
        {
            return Some(PlacementFailure::RestrictedTerrain);
        }
        if !matches!(self.visibility_cell(unit.owner, point), Status::Clear) {
            return Some(PlacementFailure::Shroud);
        }
        let body = Body {
            geometry: &template.geometry,
            position,
            angle,
        };
        for other in self.native_potential_collisions(&body, false) {
            let failure =
                match other {
                    Identity::Actor(target) => {
                        self.sneak_attack_obstruction(caster, target, committing)
                    }
                    Identity::Map(target) => (!self.layout.world_objects[target]
                        .kind_of
                        .iter()
                        .any(|kind| {
                            matches!(
                                kind.as_str(),
                                "MINE" | "INERT" | "SHRUBBERY" | "CLEARED_BY_BUILD"
                            )
                        }))
                    .then_some(PlacementFailure::ObjectsInTheWay),
                };
            if failure.is_some() {
                return failure;
            }
        }
        if let Some(failure) = self.exit_area_failure_ignoring_stealth(kind, point, angle, caster) {
            return Some(failure);
        }
        if template.avoid_supply() && self.too_close_to_supply(&template.footprint(point, angle)) {
            return Some(PlacementFailure::TooCloseToSupplies);
        }
        if !self.build_site_reachable(caster, point) {
            return Some(PlacementFailure::NoClearPath);
        }
        self.native_build_terrain_failure(kind, position, angle)
    }

    fn sneak_attack_obstruction(
        &self,
        caster: usize,
        target: usize,
        committing: bool,
    ) -> Option<PlacementFailure> {
        let hidden = self.ignores_stealth_for_placement(caster, target);
        if (!committing && hidden)
            || self.removable_for_construction(target)
            || self.attack_kind(target, "MINE")
            || self.attack_kind(target, "INERT")
        {
            return None;
        }
        if matches!(
            self.visibility_status(self.entities[caster].owner, target),
            Status::Fogged | Status::Shrouded
        ) {
            return Some(PlacementFailure::Shroud);
        }
        let relationship = self.object_relationship(caster, target);
        if relationship == 2 && self.object_busy(target) {
            return Some(PlacementFailure::ObjectsInTheWay);
        }
        if relationship == 0 || self.attack_kind(target, "IMMOBILE") || self.object_disabled(target)
        {
            return Some(if hidden {
                PlacementFailure::Generic
            } else {
                PlacementFailure::ObjectsInTheWay
            });
        }
        None
    }

    pub(super) fn ignores_stealth_for_placement(&self, caster: usize, target: usize) -> bool {
        let actor = &self.entities[target];
        self.object_relationship(caster, target) != 2
            && actor.stealthed()
            && !actor.detected()
            && !actor.has_native_disguise()
    }

    pub(super) fn native_build_terrain_clear(
        &self,
        kind: usize,
        world_position: [f32; 3],
        angle: f32,
    ) -> bool {
        self.native_build_terrain_failure(kind, world_position, angle)
            .is_none()
    }

    fn native_build_terrain_failure(
        &self,
        kind: usize,
        world_position: [f32; 3],
        angle: f32,
    ) -> Option<PlacementFailure> {
        let [world_x, world_y, _] = world_position;
        let position = [world_x, world_y];
        let ground = self.ground_height(position);
        if self
            .movement_terrain
            .get_layer_for_destination(world_position, ground)
            != 1
        {
            return Some(PlacementFailure::RestrictedTerrain);
        }
        let geometry = &self.catalog[kind].geometry;
        let half = if matches!(geometry.shape, crate::collision_geometry::Shape::Box) {
            [geometry.major, geometry.minor]
        } else {
            [geometry.major; 2]
        };
        let (sin, cos) = angle.sin_cos();
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for resolution in [30.0, 10.0] {
            let mut y = -half[1];
            loop {
                if y.partial_cmp(&(half[1] + resolution)) != Some(std::cmp::Ordering::Less) {
                    break;
                }
                if y > half[1] {
                    y = half[1];
                }
                let mut x = -half[0];
                loop {
                    if x.partial_cmp(&(half[0] + resolution)) != Some(std::cmp::Ordering::Less) {
                        break;
                    }
                    if x > half[0] {
                        x = half[0];
                    }
                    let left = x * cos;
                    let right = y * sin;
                    let bottom = x * sin;
                    let top = y * cos;
                    let point = [left - right + position[0], bottom + top + position[1]];
                    if matches!(geometry.shape, crate::collision_geometry::Shape::Box)
                        || distance_squared(point, position).sqrt() <= half[0]
                    {
                        let Some(cell) = self.cell_at(point) else {
                            return Some(PlacementFailure::RestrictedTerrain);
                        };
                        if self.movement_terrain.build_restricted(cell)
                            || self.movement_terrain.cell(cell).is_none_or(|cell| {
                                matches!(
                                    cell.kind,
                                    CellType::Water | CellType::Cliff | CellType::Impassable
                                )
                            })
                        {
                            return Some(PlacementFailure::RestrictedTerrain);
                        }
                        let height = self.ground_height(point);
                        low = low.min(height);
                        high = high.max(height);
                        let margin = self.rules.min_build_edge_distance;
                        if margin > 0.0
                            && (point[0] < self.playable_extent[0][0] + margin
                                || point[1] < self.playable_extent[0][1] + margin
                                || point[0] > self.playable_extent[1][0] - margin
                                || point[1] > self.playable_extent[1][1] - margin)
                        {
                            return Some(PlacementFailure::RestrictedTerrain);
                        }
                    }
                    x += resolution;
                }
                y += resolution;
            }
            if high - low > self.rules.allowed_build_height_variation {
                return Some(PlacementFailure::NotFlatEnough);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
