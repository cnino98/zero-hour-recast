//! SH GenerateMinefieldBehavior.cpp: geometry, RNG and creation order.
use super::{GeneratorRules, Placement, Skirmish};
use crate::collision_geometry::{Geometry, Shape};
use crate::skirmish::{physical_ocl::transform, startup};
use std::f32::consts::PI;

impl Skirmish {
    fn place_generated_mine(
        &mut self,
        id: usize,
        ordinal: usize,
        template: usize,
        point: [f32; 3],
    ) -> Option<usize> {
        let ground = self.ground_height([point[0], point[1]]);
        let layer = self.movement_terrain.get_highest_layer_for_destination(
            [point[0], point[1], 99999.0],
            ground,
            false,
        );
        if layer == 1
            && (self
                .movement_terrain
                .parachute_water_height([point[0], point[1]], ground)
                .is_some()
                || self
                    .movement_terrain
                    .physics_surfaces([point[0], point[1]], ground)
                    .is_some_and(|surfaces| surfaces & 4 != 0))
        {
            return None;
        }
        let angle = startup::random_real(&mut self.random, -PI, PI);
        let mut geometry = copy_geometry(&self.catalog[template].geometry);
        let radius = circle_radius(&geometry);
        let rules = self.mine_generator_rules(id, ordinal);
        expand(&mut geometry, radius * -(1.0 - rules.skip_under_structure));
        let body = crate::collision_geometry::Body {
            geometry: &geometry,
            position: point,
            angle,
        };
        for other in self.native_potential_collisions(&body, false) {
            let structure = match other {
                crate::skirmish::partition::Identity::Actor(other) => {
                    self.attack_kind(other, "STRUCTURE")
                }
                crate::skirmish::partition::Identity::Map(other) => self.layout.world_objects
                    [other]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "STRUCTURE"),
            };
            if structure {
                return None;
            }
        }
        let owner = self.entities[id].owner;
        let team = self.team_membership.default_team(owner);
        let mine = self.new_object_on_team(template, owner, Some(team));
        self.set_object_position(mine, point);
        self.set_object_orientation(mine, angle);
        self.entities[mine].producer = Some(id);
        self.set_mine_scoot(mine, self.object_position(id), point);
        if self.mine_generator_rules(id, ordinal).upgradable {
            self.entities[id].mines.generators[ordinal].mines.push(mine);
        }
        Some(mine)
    }

    pub(in crate::skirmish) fn generate_minefield(&mut self, id: usize, ordinal: usize) {
        let type_index = self.entities[id].type_index;
        let state = &mut self.entities[id].mines.generators[ordinal];
        if state.generated {
            return;
        }
        state.generated = true;
        let rules = &self.catalog[type_index].behavior.mine_generators[ordinal];
        let name = if state.upgraded {
            &rules.upgraded_name
        } else {
            &rules.mine_name
        };
        let Some(template) = self.catalog.iter().position(|unit| &unit.name == name) else {
            return;
        };
        let target = state.target;
        let target = target.unwrap_or_else(|| self.object_position(id));
        let mut geometry = copy_geometry(self.body_geometry(id));
        let circular = rules.circular;
        let distance = rules.distance;
        match rules.placement {
            Placement::Smart { skip_interior } => {
                if !skip_interior {
                    geometry = copy_geometry(&self.catalog[template].geometry);
                    self.place_generated_mine(id, ordinal, template, target);
                }
                if circular {
                    make_circular(&mut geometry);
                }
                let radius = circle_radius(&self.catalog[template].geometry);
                let diameter = radius * 2.0;
                expand(&mut geometry, radius);
                loop {
                    self.minefield_border(id, ordinal, template, target, &geometry, circular);
                    expand(&mut geometry, diameter);
                    if circle_radius(&geometry) >= distance {
                        break;
                    }
                }
            }
            Placement::Border => {
                expand(&mut geometry, distance);
                self.minefield_border(id, ordinal, template, target, &geometry, circular);
            }
            Placement::Footprint => {
                expand(&mut geometry, distance);
                if circular {
                    make_circular(&mut geometry);
                }
                self.minefield_footprint(id, ordinal, template, target, &geometry);
            }
        }
        let effect = self.catalog[self.entities[id].type_index]
            .behavior
            .mine_generators[ordinal]
            .generation_fx
            .clone();
        if let Some(effect) = effect {
            self.object_fx(id, effect);
        }
    }

    fn minefield_border(
        &mut self,
        id: usize,
        ordinal: usize,
        template: usize,
        target: [f32; 3],
        geometry: &Geometry,
        circular: bool,
    ) {
        if matches!(geometry.shape, Shape::Box) && !circular {
            // makeCorner ignores its pos argument and uses the producer's full matrix.
            let (position, basis) = self.object_transform(id);
            let major = geometry.major;
            let minor = geometry.minor;
            let corners = [
                [major, minor, 0.0],
                [-major, minor, 0.0],
                [-major, -minor, 0.0],
                [major, -minor, 0.0],
            ]
            .map(|corner| {
                let offset = transform::vector(basis, corner);
                std::array::from_fn(|axis| offset[axis] + position[axis])
            });
            for edge in 0..4 {
                self.minefield_line(
                    id,
                    ordinal,
                    template,
                    corners[edge],
                    corners[(edge + 1) % 4],
                );
            }
        } else {
            self.minefield_circle(id, ordinal, template, target, geometry.major);
        }
    }

    fn minefield_line(
        &mut self,
        id: usize,
        ordinal: usize,
        template: usize,
        start: [f32; 3],
        end: [f32; 3],
    ) {
        let delta = [end[0] - start[0], end[1] - start[1]];
        let squared = delta.map(|value| value * value);
        let length = (squared[0] + squared[1]).sqrt();
        let radius = circle_radius(&self.catalog[template].geometry);
        let count = super::native_ceil(length / (radius * 2.0)).max(1);
        let increment = length / super::integer_real(count);
        let jitter = radius * self.mine_generator_rules(id, ordinal).jitter;
        // Every native rectangle edge skips the mine at its starting corner.
        let mut place = increment;
        loop {
            if place > length {
                break;
            }
            let mut point = [
                start[0] + place * delta[0] / length,
                start[1] + place * delta[1] / length,
                0.0,
            ];
            point[2] = self.ground_height([point[0], point[1]]);
            self.jitter_mine_position(&mut point, jitter);
            self.place_generated_mine(id, ordinal, template, point);
            place += increment;
        }
    }

    fn minefield_circle(
        &mut self,
        id: usize,
        ordinal: usize,
        template: usize,
        center: [f32; 3],
        radius: f32,
    ) {
        let circumference = 2.0 * PI * radius;
        let mine_radius = circle_radius(&self.catalog[template].geometry);
        let count = super::native_ceil(circumference / (mine_radius * 2.0)).max(1);
        let increment = (2.0 * PI) / super::integer_real(count);
        let half_increment = increment * 0.5;
        let full_turn = 2.0 * PI;
        let limit = full_turn - half_increment;
        let jitter = mine_radius * self.mine_generator_rules(id, ordinal).jitter;
        let mut angle = 0.0_f32;
        loop {
            if angle >= limit {
                break;
            }
            let offset = [radius * angle.cos(), radius * angle.sin()];
            let mut point = [center[0] + offset[0], center[1] + offset[1], 0.0];
            point[2] = self.ground_height([point[0], point[1]]);
            self.jitter_mine_position(&mut point, jitter);
            self.place_generated_mine(id, ordinal, template, point);
            angle += increment;
        }
    }

    fn minefield_footprint(
        &mut self,
        id: usize,
        ordinal: usize,
        template: usize,
        target: [f32; 3],
        geometry: &Geometry,
    ) {
        let area = match geometry.shape {
            Shape::Box => 4.0 * geometry.major * geometry.minor,
            Shape::Cylinder | Shape::Sphere => {
                PI * (circle_radius(geometry) * circle_radius(geometry))
            }
        };
        let count =
            super::native_ceil(self.mine_generator_rules(id, ordinal).density * area).max(1);
        let diameter = circle_radius(&self.catalog[template].geometry) * 2.0;
        let minimum_squared = diameter * diameter;
        let mut created = Vec::new();
        for _ in 0..count {
            let mut remaining = 100;
            let point = loop {
                let offset = random_footprint_offset(geometry, &mut self.random);
                let point = std::array::from_fn(|axis| offset[axis] + target[axis]);
                remaining -= 1;
                let too_close = created.iter().any(|&mine| {
                    let other = self.object_position(mine);
                    let dx = other[0] - point[0];
                    let dy = other[1] - point[1];
                    let squared = [dx * dx, dy * dy];
                    squared[0] + squared[1] < minimum_squared
                });
                if !too_close || remaining == 0 {
                    break point;
                }
            };
            // GeometryInfo's footprint test is deliberately unrotated.
            if inside_footprint(self.body_geometry(id), target, point) {
                continue;
            }
            if let Some(mine) = self.place_generated_mine(id, ordinal, template, point) {
                created.push(mine);
            }
        }
    }

    fn mine_generator_rules(&self, id: usize, ordinal: usize) -> &GeneratorRules {
        &self.catalog[self.entities[id].type_index]
            .behavior
            .mine_generators[ordinal]
    }

    fn jitter_mine_position(&mut self, position: &mut [f32; 3], amount: f32) {
        position[0] += startup::random_real(&mut self.random, -amount, amount);
        position[1] += startup::random_real(&mut self.random, -amount, amount);
    }
}

pub(super) const fn copy_geometry(source: &Geometry) -> Geometry {
    Geometry {
        shape: source.shape,
        is_small: source.is_small,
        major: source.major,
        minor: source.minor,
        height: source.height,
    }
}

pub(super) fn circle_radius(geometry: &Geometry) -> f32 {
    if matches!(geometry.shape, Shape::Box) {
        let major_squared = geometry.major * geometry.major;
        let minor_squared = geometry.minor * geometry.minor;
        (major_squared + minor_squared).sqrt()
    } else {
        geometry.major
    }
}

fn expand(geometry: &mut Geometry, radius: f32) {
    geometry.major += radius;
    geometry.minor += radius;
}

pub(super) fn clip_to_footprint(
    geometry: &Geometry,
    center: [f32; 3],
    mut point: [f32; 3],
) -> [f32; 3] {
    if matches!(geometry.shape, Shape::Box) {
        point[0] = point[0].clamp(center[0] - geometry.major, center[0] + geometry.major);
        point[1] = point[1].clamp(center[1] - geometry.minor, center[1] + geometry.minor);
    } else {
        let dx = point[0] - center[0];
        let dy = point[1] - center[1];
        let squared = [dx * dx, dy * dy];
        let radius = (squared[0] + squared[1]).sqrt();
        if radius > geometry.major {
            let ratio = geometry.major / radius;
            let offset = [dx * ratio, dy * ratio];
            point[0] = center[0] + offset[0];
            point[1] = center[1] + offset[1];
        }
    }
    point
}

fn make_circular(geometry: &mut Geometry) {
    let radius = circle_radius(geometry);
    geometry.shape = Shape::Cylinder;
    geometry.is_small = false;
    geometry.height = 1.0;
    geometry.major = radius;
    geometry.minor = radius;
}

fn inside_footprint(geometry: &Geometry, center: [f32; 3], point: [f32; 3]) -> bool {
    let dx = point[0] - center[0];
    let dy = point[1] - center[1];
    if matches!(geometry.shape, Shape::Box) {
        center[0] - geometry.major <= point[0]
            && point[0] <= center[0] + geometry.major
            && center[1] - geometry.minor <= point[1]
            && point[1] <= center[1] + geometry.minor
    } else {
        let squared = [dx * dx, dy * dy];
        (squared[0] + squared[1]).sqrt() <= geometry.major
    }
}

fn random_footprint_offset(geometry: &Geometry, random: &mut [u32; 6]) -> [f32; 3] {
    let minor = if matches!(geometry.shape, Shape::Box) {
        geometry.minor
    } else {
        geometry.major
    };
    loop {
        let x = startup::random_real(random, -geometry.major, geometry.major);
        let y = startup::random_real(random, -minor, minor);
        let squared = [x * x, y * y];
        if matches!(geometry.shape, Shape::Box)
            || squared[0] + squared[1] <= geometry.major * geometry.major
        {
            return [x, y, 0.0];
        }
    }
}
