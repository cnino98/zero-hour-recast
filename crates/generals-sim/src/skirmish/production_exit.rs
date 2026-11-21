//! Selected ground exits from `DefaultProductionExitUpdate`,
//! `QueueProductionExitUpdate`, `SupplyCenterProductionExitUpdate` and
//! `AIUpdateInterface::privateFollowPath` at the pinned parent-module revision.
//! Ground exit delays/bursts retain the original module state and update rules.

use super::movement::{bounding_radius, distance_squared, footprints_overlap};
use super::{Footprint, Skirmish};

#[cfg(test)]
mod tests;

/// Authored `FactoryExitWidth` and `FactoryExtraBibWidth` placement footprint.
pub struct FactoryBib {
    pub exit_width: f32,
    pub extra_width: f32,
}

impl FactoryBib {
    pub const NONE: Self = Self {
        exit_width: 0.0,
        extra_width: 0.0,
    };
}

pub(super) struct QueueState {
    delay: u32,
    burst: u32,
}

impl QueueState {
    pub(super) fn new(exit: Option<&super::ProductionExit>) -> Self {
        Self {
            delay: 0,
            burst: exit
                .and_then(|exit| exit.queue.as_ref())
                .map_or(0, |queue| queue.initial_burst),
        }
    }

    pub(super) const fn free(&self) -> bool {
        self.burst > 0 || self.delay == 0
    }

    const fn update(&mut self) {
        if self.free() {
            self.delay = 0;
        } else {
            self.delay -= 1;
        }
    }

    pub(super) const fn exited(&mut self, delay: u32) {
        self.delay = delay;
        if self.burst > 0 {
            self.burst -= 1;
        }
    }
}

impl Skirmish {
    fn production_world_point(&self, id: usize, point: [f32; 3]) -> [f32; 3] {
        let (position, matrix) = self.object_transform(id);
        std::array::from_fn(|axis| {
            let products = [
                matrix[axis][0] * point[0],
                matrix[axis][1] * point[1],
                matrix[axis][2] * point[2],
            ];
            ((products[0] + products[1]) + products[2]) + position[axis]
        })
    }

    fn place_production_exit(&mut self, producer: usize, child: usize, mut point: [f32; 3]) {
        let actor = &self.entities[producer];
        let exit = self.catalog[actor.type_index]
            .behavior
            .production_exit
            .as_ref()
            .expect("Production exit");
        let default_exit = exit.extend_rally;
        let layer = actor.layer;
        let facing = actor.facing;
        let ground = self.ground_height([point[0], point[1]]);
        if let Some(queue) = &exit.queue {
            let airborne = point[2].partial_cmp(&ground) != Some(std::cmp::Ordering::Equal);
            if airborne && !queue.allow_airborne_creation {
                point[2] = ground;
            }
            self.set_object_position(child, point);
            self.set_object_orientation(child, facing);
            if airborne {
                self.production_exit_motive_force(producer, child);
            }
        } else {
            point[2] = if default_exit {
                self.movement_layer_height(layer, [point[0], point[1]])
            } else {
                ground
            };
            self.set_object_position(child, point);
            self.set_object_orientation(child, facing);
            if default_exit {
                self.entities[child].layer = layer;
            }
        }
    }

    fn production_exit_motive_force(&mut self, producer: usize, child: usize) {
        if self.object_physics_mass(producer).is_none() {
            return;
        }
        let Some(mass) = self.object_physics_mass(child) else {
            return;
        };
        let velocity = self.entities[producer].physics_velocity();
        self.apply_physics_motive_force(child, velocity.map(|component| component * mass));
        let offset = self.catalog[self.entities[child].type_index]
            .physics_motion_rules()
            .expect("Physics module data")
            .center_of_mass_offset;
        self.entities[child].set_physics_pitch_rate(offset * 0.04);
    }

    pub(super) fn update_queue_exit(&mut self, id: usize) -> i32 {
        self.entities[id].queue_exit.update();
        1
    }
    /// Model exit, unextended natural rally, and optional user rally in source XYZ.
    /// This is the building path used by `W3DWaypointBuffer::drawWaypoints`.
    #[must_use]
    pub fn producer_rally_path(&self, id: usize) -> Option<Vec<[f32; 3]>> {
        let building = self
            .entities
            .get(id)
            .filter(|building| building.complete())?;
        let points = if let Some(exit) = &self.catalog[building.type_index].behavior.production_exit
        {
            [
                self.production_world_point(id, exit.create),
                self.production_world_point(id, exit.rally),
            ]
        } else if self.is_tunnel(id) {
            self.container_rally_points(id)?
        } else {
            return None;
        };
        let mut path = points.to_vec();
        path.extend(building.rally_point);
        Some(path)
    }

    pub(super) fn exit_areas_clear(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
    ) -> bool {
        self.exit_area_failure(type_index, position, angle, computer, None)
            .is_none()
    }

    pub(super) fn exit_area_failure_ignoring_stealth(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        caster: usize,
    ) -> Option<super::PlacementFailure> {
        self.exit_area_failure(type_index, position, angle, false, Some(caster))
    }

    fn exit_area_failure(
        &self,
        type_index: usize,
        position: [f32; 2],
        angle: f32,
        computer: bool,
        ignore_stealth_for: Option<usize>,
    ) -> Option<super::PlacementFailure> {
        // GeneralsMD/.../BuildAssistant.cpp isLocationClearOfObjects, exit-area
        // checks, including the original computer player's minimum extra bib.
        let unit = &self.catalog[type_index];
        let shape = unit.footprint(position, angle);
        let mut extra = unit.factory_bib.extra_width;
        let mut exit_width = unit.factory_bib.exit_width;
        if computer && extra < 30.0 {
            extra = 30.0;
            exit_width = (exit_width - extra).max(0.0);
        }
        let bounds = bib_bounds(&shape, extra, extra);
        let exit = exit_area(&shape, exit_width);
        let range = 2.0 * (shape.major + shape.minor);
        let obstruction =
            |other: &Footprint, other_exit: f32, other_bib: f32, shrouded: bool, immobile: bool| {
                if (distance_squared(shape.position, other.position).sqrt()
                    - bounding_radius(other))
                .max(0.0)
                    >= range
                {
                    return None;
                }
                // Preserve the pinned source's box minor expansion by myExtraWidth.
                let other_bounds = bib_bounds(other, other_bib, extra);
                let other_exit = exit_area(other, other_exit);
                if footprints_overlap(&bounds, &other_bounds) {
                    return Some(super::PlacementFailure::ObjectsInTheWay);
                }
                if !immobile {
                    return None;
                }
                if exit
                    .as_ref()
                    .is_some_and(|exit| footprints_overlap(exit, &other_bounds))
                    || other_exit
                        .as_ref()
                        .is_some_and(|exit| footprints_overlap(exit, &bounds))
                    || exit
                        .as_ref()
                        .zip(other_exit.as_ref())
                        .is_some_and(|(left, right)| footprints_overlap(left, right))
                {
                    return Some(if shrouded {
                        super::PlacementFailure::Shroud
                    } else {
                        super::PlacementFailure::ObjectsInTheWay
                    });
                }
                None
            };
        for (id, entity) in self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, entity)| entity.alive())
        {
            let other = &self.catalog[entity.type_index];
            if !other.kind_of.iter().any(|kind| kind == "STRUCTURE")
                || ignore_stealth_for.is_some_and(|caster| {
                    self.ignores_stealth_for_placement(caster, id)
                        || self.removable_for_construction(id)
                })
            {
                continue;
            }
            let shrouded = ignore_stealth_for.is_some_and(|caster| {
                matches!(
                    self.visibility_status(self.entities[caster].owner, id),
                    super::visibility::Status::Fogged | super::visibility::Status::Shrouded
                )
            });
            let failure = obstruction(
                &other.footprint(entity.position, entity.facing),
                other.factory_bib.exit_width,
                other.factory_bib.extra_width,
                shrouded,
                other.kind_of.iter().any(|kind| kind == "IMMOBILE"),
            );
            if failure.is_some() {
                return failure;
            }
        }
        self.layout
            .obstacles
            .iter()
            .find_map(|other| obstruction(other, 0.0, 0.0, false, true))
    }

    pub(super) fn spawn_produced_unit(
        &mut self,
        producer: usize,
        type_index: usize,
        free_spawn: bool,
    ) -> Option<usize> {
        self.spawn_produced_unit_at(producer, type_index, free_spawn, 0)
    }

    pub(in crate::skirmish) fn spawn_produced_unit_at(
        &mut self,
        producer: usize,
        type_index: usize,
        free_spawn: bool,
        ordinal: usize,
    ) -> Option<usize> {
        if self.catalog[self.entities[producer].type_index]
            .parking
            .is_some()
        {
            return self.spawn_parking_unit(producer, type_index, free_spawn, ordinal);
        }
        let building = &self.entities[producer];
        let exit = self.catalog[building.type_index]
            .behavior
            .production_exit
            .as_ref()?;
        if exit.queue.is_some() && !building.queue_exit.free() {
            return None;
        }
        let type_index = self.resolve_variation(type_index);
        let building = &self.entities[producer];
        let exit = self.catalog[building.type_index]
            .behavior
            .production_exit
            .as_ref()?;
        let delay = exit.queue.as_ref().map(|queue| queue.delay_frames);
        let create = self.production_world_point(producer, exit.create);
        let local_rally = natural_rally(exit);
        let mut rally = self.production_world_point(producer, local_rally);
        if exit.snap_rally {
            let horizontal = self.snap_unit_position(type_index, [rally[0], rally[1]])?;
            rally[0] = horizontal[0];
            rally[1] = horizontal[1];
            rally[2] = self.ground_height(horizontal);
        }
        self.cell_at([create[0], create[1]])?;
        self.cell_at([rally[0], rally[1]])?;
        let owner = building.owner;
        // Default/Queue exits adjust the user's final rally; SupplyCenter appends
        // its raw rally. Only the first exit segment uses computeQuickPath.
        // QueueProductionExit duplicates its natural destination without a user
        // rally. AIFollowPath later adjusts that segment only if it is not skipped.
        let user_rally = building.rally_point;
        let adjust_rally = exit.extend_rally;
        let spawned = self.new_object(type_index, owner, true);
        // SpawnBehavior notifies before installing the exit; ProductionUpdate
        // does so afterwards. The distinction matters for AI docking orders.
        if free_spawn {
            self.opponent_unit_produced(producer, spawned);
            self.map_unit_produced(Some(producer), spawned);
        }
        self.place_production_exit(producer, spawned, create);
        // Default/QueueProductionExitUpdate registers the new object before
        // adjustDestination, which includes other units' current goal claims.
        self.update_position_claim(spawned);
        let user_rally = user_rally.map_or_else(
            || delay.map(|_| rally),
            |point| {
                if adjust_rally {
                    self.attack_ground_movement(spawned)
                        .then(|| self.adjusted_destination_world(spawned, point))
                        .flatten()
                } else {
                    Some(point)
                }
            },
        );
        self.entities[spawned].producer = Some(producer);
        let mut points = vec![rally];
        points.extend(user_rally);
        self.native_follow_path_from(
            spawned,
            points,
            Some(producer),
            crate::skirmish::ai_goals::COMMAND_FROM_AI,
            true,
        );
        if self.catalog[self.entities[producer].type_index].kind == super::Kind::SupplyCenter {
            self.entities[spawned].ai.supply.force_wanting = true;
        }
        if !free_spawn {
            self.opponent_unit_produced(producer, spawned);
            self.map_unit_produced(Some(producer), spawned);
            self.native_build_complete(spawned);
        }
        self.record_built(spawned);
        if let Some(delay) = delay {
            self.entities[producer].queue_exit.exited(delay);
        }
        Some(spawned)
    }
}

// Default/Queue getNaturalRallyPoint uses Vector3 reciprocal normalization.
fn natural_rally(exit: &super::ProductionExit) -> [f32; 3] {
    let mut point = exit.rally;
    if exit.extend_rally {
        let squared = point.map(|component| component * component);
        let length_squared = (squared[0] + squared[1]) + squared[2];
        if length_squared != 0.0 {
            let inverse_length = 1.0 / length_squared.sqrt();
            for coordinate in &mut point {
                let normalized = *coordinate * inverse_length;
                let extension = normalized * 20.0;
                *coordinate += extension;
            }
        }
    }
    point
}

fn bib_bounds(shape: &Footprint, major_extra: f32, minor_extra: f32) -> Footprint {
    let mut bounds = shape.clone();
    bounds.major += major_extra;
    bounds.minor = if matches!(shape.shape, crate::collision_geometry::Shape::Box) {
        bounds.minor + minor_extra
    } else {
        bounds.major
    };
    bounds.shape = crate::collision_geometry::Shape::Box;
    bounds
}

fn exit_area(shape: &Footprint, width: f32) -> Option<Footprint> {
    if width <= 0.0 {
        return None;
    }
    let major = width / 2.0;
    Some(Footprint {
        position: transform_point(shape.position, shape.angle, [shape.major + major, 0.0]),
        angle: shape.angle,
        major,
        minor: if matches!(shape.shape, crate::collision_geometry::Shape::Box) {
            shape.minor
        } else {
            shape.major
        },
        shape: shape.shape,
    })
}

pub(super) fn transform_point(center: [f32; 2], angle: f32, local: [f32; 2]) -> [f32; 2] {
    let (sine, cosine) = angle.sin_cos();
    let products = [
        cosine * local[0],
        sine * local[1],
        sine * local[0],
        cosine * local[1],
    ];
    [
        center[0] + (products[0] - products[1]),
        center[1] + (products[2] + products[3]),
    ]
}

impl Skirmish {
    pub(super) fn exit_delivered_unit(&mut self, producer: usize, rider: usize) {
        let building = &self.entities[producer];
        let exit = self.catalog[building.type_index]
            .behavior
            .production_exit
            .as_ref()
            .expect("spawn rally exit");
        let create = self.production_world_point(producer, exit.create);
        let local = natural_rally(exit);
        let mut rally = self.production_world_point(producer, local);
        if exit.snap_rally {
            let horizontal = self
                .snap_unit_position(self.entities[rider].type_index, [rally[0], rally[1]])
                .expect("authored production natural rally");
            rally[0] = horizontal[0];
            rally[1] = horizontal[1];
            rally[2] = self.ground_height(horizontal);
        }
        let user_rally = building.rally_point;
        self.place_production_exit(producer, rider, create);
        self.update_position_claim(rider);
        let user_rally = user_rally.and_then(|goal| {
            self.attack_ground_movement(rider)
                .then(|| self.adjusted_destination_world(rider, goal))
                .flatten()
        });
        let mut points = vec![rally];
        points.extend(user_rally);
        self.native_follow_path_from(
            rider,
            points,
            Some(producer),
            crate::skirmish::ai_goals::COMMAND_FROM_AI,
            true,
        );
    }
}
