//! Core AIPathfind.cpp5923,9333,9561–9671, SH 2bfaea60.
use crate::collision_geometry;
use crate::skirmish::{
    Skirmish,
    movement::obstacles::OwnerId,
    partition::Identity,
    path_following::{Node, Path},
};

impl Skirmish {
    fn navigation_object_position(&self, id: Identity) -> [f32; 3] {
        match id {
            Identity::Actor(id) => self.object_position(id),
            Identity::Map(index) => self.layout.world_objects[index].position,
        }
    }

    fn navigation_object_geometry(&self, id: Identity) -> &collision_geometry::Geometry {
        match id {
            Identity::Actor(id) => self.body_geometry(id),
            Identity::Map(index) => &self.layout.world_objects[index].geometry,
        }
    }

    fn navigation_object_is_tall(&self, id: Identity) -> bool {
        let kinds = match id {
            Identity::Actor(id) => &self.catalog[self.entities[id].type_index].kind_of,
            Identity::Map(index) => &self.layout.world_objects[index].kind_of,
        };
        kinds.iter().any(|kind| kind == "AIRCRAFT_PATH_AROUND")
    }

    fn navigation_circle_radius(&self, id: Identity) -> f32 {
        let geometry = self.navigation_object_geometry(id);
        if matches!(geometry.shape, collision_geometry::Shape::Box) {
            let major_squared = geometry.major * geometry.major;
            let minor_squared = geometry.minor * geometry.minor;
            (major_squared + minor_squared).sqrt()
        } else {
            geometry.major
        }
    }

    fn closest_tall_building(&self, position: [f32; 3], range: f32) -> Option<Identity> {
        let mut closest = None;
        let mut squared = range * range;
        let mut last_ring = f64::INFINITY;
        for (ring, id) in self
            .partition
            .outward_world_range_rings([position[0], position[1]], range)
        {
            if ring > last_ring {
                break;
            }
            let other = self.navigation_object_position(id);
            let x = other[0] - position[0];
            let y = other[1] - position[1];
            let radius = self.navigation_circle_radius(id);
            let x_squared = x * x;
            let y_squared = y * y;
            let center_squared = x_squared + y_squared;
            let distance = if radius > 0.0 {
                let boundary = center_squared.sqrt() - radius;
                if boundary <= 0.0 {
                    0.0
                } else {
                    boundary * boundary
                }
            } else {
                center_squared
            };
            // Native distance predicate precedes the kind filter.
            if distance < squared && self.navigation_object_is_tall(id) {
                closest = Some(id);
                squared = distance;
                last_ring = ring;
            }
        }
        closest
    }

    fn clip_aircraft_circle(&self, from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
        let Some(first) = self.closest_tall_building(to, 100.0) else {
            return to;
        };
        let position = self.navigation_object_position(first);
        let radius = self.navigation_circle_radius(first) + 20.0;
        let mut adjusted = radial_offset(from, to, position, 100.0 + radius);
        if let Some(other) = self.closest_tall_building(adjusted, 100.0)
            && other != first
        {
            let radius = self.navigation_circle_radius(other) + 20.0;
            adjusted = radial_offset(
                from,
                adjusted,
                self.navigation_object_position(other),
                100.0 + radius,
            );
        }
        // Native ignoreBuilding is unused by circleClipsTallBuilding.
        adjusted
    }

    fn segment_tall_building(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        ignored: Option<Identity>,
    ) -> Option<Identity> {
        // iterateCellsAlongLine ignores worldToCell's overflow result, using
        // its floored and clamped coordinates even for off-map plane goals.
        let start = crate::Cell {
            x: clamped_path_coordinate(from[0], self.grid.width()),
            y: clamped_path_coordinate(from[1], self.grid.height()),
        };
        let end = crate::Cell {
            x: clamped_path_coordinate(to[0], self.grid.width()),
            y: clamped_path_coordinate(to[1], self.grid.height()),
        };
        for cell in self.grid.line_cells(start, end) {
            if !self.navigation.is_obstacle(cell)
                || matches!(
                    self.movement_terrain.path_kind(cell),
                    Some(crate::terrain::CellType::Rubble)
                )
            {
                continue;
            }
            let Some(owner) = self.navigation_obstacles.at(cell) else {
                continue;
            };
            let id = match owner.id {
                OwnerId::Actor(id) => Some(Identity::Actor(id)),
                OwnerId::Static(index) => self
                    .layout
                    .obstacle_objects
                    .get(index)
                    .copied()
                    .flatten()
                    .map(Identity::Map),
                OwnerId::Neutral(index) => {
                    self.registered_neutral_object(index).map(Identity::Actor)
                }
                OwnerId::Fence(_) => None,
            };
            if let Some(id) = id
                && Some(id) != ignored
                && self.navigation_object_is_tall(id)
            {
                return Some(id);
            }
        }
        None
    }

    fn avoid_aircraft_segment(
        &self,
        from: [f32; 3],
        to: &mut [f32; 3],
        ignored: Option<Identity>,
    ) -> Option<[[f32; 3]; 3]> {
        let mut start = from;
        for _ in 0..2 {
            let building = self.segment_tall_building(start, *to, ignored)?;
            let center = self.navigation_object_position(building);
            let radius = self.navigation_circle_radius(building) + 20.0;
            if inside_tall_radius(*to, center, radius) {
                move_outside_tall_radius(to, center, radius);
                continue;
            }
            if inside_tall_radius(start, center, radius) {
                move_outside_tall_radius(&mut start, center, radius);
            }
            let middle = radial_offset(start, *to, center, radius);
            return Some([
                radial_offset(start, middle, center, radius),
                middle,
                radial_offset(middle, *to, center, radius),
            ]);
        }
        None
    }

    pub(in crate::skirmish) fn native_aircraft_path(
        &self,
        id: usize,
        to: [f32; 3],
        ignored: Option<Identity>,
    ) -> Path {
        let ignored = ignored.or_else(|| {
            self.entities[id]
                .chinook_path_ignored_building()
                .map(Identity::Actor)
        });
        let actor = &self.entities[id];
        let mut start = self.object_position(id);
        let unit = &self.catalog[actor.type_index];
        let aircraft = unit.kind_of.iter().any(|kind| kind == "AIRCRAFT")
            && !unit.kind_of.iter().any(|kind| kind == "PROJECTILE");
        let wings = aircraft
            && actor
                .active_locomotor(&self.catalog[actor.type_index])
                .flight
                .as_ref()
                .is_some_and(|rules| {
                    matches!(rules.appearance, crate::skirmish::flight::Appearance::Wings)
                });
        let destination = if wings {
            self.clip_aircraft_circle(start, to)
        } else {
            to
        };
        start[2] = to[2];
        let mut positions = vec![start, destination];
        let mut current = 0;
        let mut limit = 20;
        while aircraft && current + 1 < positions.len() {
            let from = positions[current];
            if let Some(inserted) =
                self.avoid_aircraft_segment(from, &mut positions[current + 1], ignored)
            {
                let insertion = current + 1;
                positions.splice(insertion..insertion, inserted);
                // Native curNode=newNode2, then advances to newNode3.
                current += 2;
            }
            current += 1;
            limit -= 1;
            if limit < 0 {
                break;
            }
        }
        Path::new(
            positions
                .into_iter()
                .map(|position| Node {
                    position,
                    layer: 1,
                    previous_layer: 1,
                    can_optimize: false,
                })
                .collect(),
        )
    }
}

fn clamped_path_coordinate(value: f32, extent: u16) -> u16 {
    let coordinate = (value / 10.0).floor().clamp(0.0, f32::from(extent - 1));
    num_traits::ToPrimitive::to_u16(&coordinate).expect("finite clamped path coordinate")
}

fn radial_offset(from: [f32; 3], to: [f32; 3], center: [f32; 3], radius: f32) -> [f32; 3] {
    let x = to[0] - from[0];
    let y = to[1] - from[1];
    let cross_x = x * (center[1] - from[1]);
    let cross_y = y * (center[0] - from[0]);
    let cross = cross_x - cross_y;
    let mut normal = if cross > 0.0 { [y, -x] } else { [-y, x] };
    let squared = normal.map(|component| component * component);
    let length = (squared[0] + squared[1]).sqrt();
    if length != 0.0 {
        normal[0] /= length;
        normal[1] /= length;
    }
    let offset_x = normal[0] * radius;
    let offset_y = normal[1] * radius;
    [center[0] + offset_x, center[1] + offset_y, center[2]]
}

fn inside_tall_radius(position: [f32; 3], center: [f32; 3], radius: f32) -> bool {
    let x = position[0] - center[0];
    let y = position[1] - center[1];
    let x_squared = x * x;
    let y_squared = y * y;
    f64::from((x_squared + y_squared).sqrt()) <= f64::from(radius) * 0.98
}

fn move_outside_tall_radius(position: &mut [f32; 3], center: [f32; 3], radius: f32) {
    let mut delta = [position[0] - center[0], position[1] - center[1]];
    let initial_squared = delta.map(|component| component * component);
    if (initial_squared[0] + initial_squared[1]).sqrt() < 0.1 {
        delta[0] = 1.0;
    }
    let x_squared = delta[0] * delta[0];
    let y_squared = delta[1] * delta[1];
    let length = (x_squared + y_squared).sqrt();
    delta[0] /= length;
    delta[1] /= length;
    delta[0] *= radius;
    delta[1] *= radius;
    position[0] = center[0] + delta[0];
    position[1] = center[1] + delta[1];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_map_segment_goals_use_source_floor_and_extent_clamping() {
        assert_eq!(clamped_path_coordinate(-0.1, 40), 0);
        assert_eq!(clamped_path_coordinate(19.9, 40), 1);
        assert_eq!(clamped_path_coordinate(2000.0, 40), 39);
    }

    #[test]
    fn detour_uses_building_height_and_native_right_side_for_zero_cross() {
        let center = [50.0, 0.0, 37.0];
        let point = radial_offset([0.0; 3], [100.0, 0.0, 0.0], center, 40.0);
        assert_eq!(
            point.map(f32::to_bits),
            [50.0_f32, 40.0, 37.0].map(f32::to_bits)
        );
        let mut inside = center;
        move_outside_tall_radius(&mut inside, center, 40.0);
        assert_eq!(
            inside.map(f32::to_bits),
            [90.0_f32, 0.0, 37.0].map(f32::to_bits)
        );
    }
}
