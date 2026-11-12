//! Native instant-construction collision query and friendly AI displacement.
use crate::{
    collision_geometry::{Body, Geometry, Shape, intersects},
    skirmish::{Skirmish, UnitAiUpdate, startup},
};
use num_traits::ToPrimitive;
use std::f32::consts::PI;

impl Skirmish {
    pub(in crate::skirmish) fn move_objects_for_construction(
        &mut self,
        owner: usize,
        type_index: usize,
        position: [f32; 3],
        angle: f32,
    ) -> bool {
        let game = self;
        let source = &game.catalog[type_index].geometry;
        let geometry = Geometry {
            shape: Shape::Box,
            is_small: matches!(source.shape, Shape::Box) && source.is_small,
            major: source.major,
            minor: if matches!(source.shape, Shape::Box) {
                source.minor
            } else {
                source.major
            },
            height: if matches!(source.shape, Shape::Box) {
                source.height
            } else {
                10.0
            },
        };
        // Native pow/sqrt promotes these Real inputs before rounding the radius.
        let major = f64::from(geometry.major);
        let minor = f64::from(geometry.minor);
        let major_squared = major * major;
        let minor_squared = minor * minor;
        let mut radius = (major_squared + minor_squared)
            .sqrt()
            .to_f32()
            .expect("validated construction radius");
        radius *= 1.4;
        let mut any_unmovables = false;
        for id in game.construction_collisions(&geometry, position, angle) {
            let unit = &game.catalog[game.entities[id].type_index];
            if unit
                .kind_of
                .iter()
                .any(|flag| matches!(flag.as_str(), "MINE" | "INERT" | "ALWAYS_SELECTABLE"))
                || game.removable_for_construction(id)
            {
                continue;
            }
            if game.enemies(owner, game.entities[id].owner)
                || matches!(unit.ai.update, UnitAiUpdate::Absent)
            {
                any_unmovables = true;
                continue;
            }
            let varied = startup::random_real(&mut game.random, 0.5, 1.5) * radius;
            let direction = startup::random_real(&mut game.random, -PI, PI);
            let (sin, cos) = direction.sin_cos();
            let left = varied * cos;
            let right = 0.0 * sin;
            let bottom = varied * sin;
            let top = 0.0 * cos;
            let destination = [
                position[0] + (left - right),
                position[1] + (bottom + top),
                position[2] + 0.0,
            ];
            // The special native command ignores sleep mood, retaining ordinary AI move guards.
            game.move_even_if_sleeping(id, destination);
        }
        !any_unmovables
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn construction_collisions(
        &self,
        geometry: &Geometry,
        position: [f32; 3],
        angle: f32,
    ) -> Vec<usize> {
        let maximum = sphere_radius(geometry) * 1.1;
        let maximum_squared = maximum * maximum;
        let query = Body {
            geometry,
            position,
            angle,
        };
        self.partition
            .fastest_range_objects([position[0], position[1]], maximum)
            .into_iter()
            .filter(|&id| {
                let actor = &self.entities[id];
                if !actor.exists() {
                    return false;
                }
                let target_position = self.object_position(id);
                let bounds = self.body_geometry_bounds(id);
                let horizontal = target_position[0] - position[0];
                let vertical = target_position[1] - position[1];
                let altitude = target_position[2] + bounds.center_height - position[2];
                let horizontal_squared = horizontal * horizontal;
                let vertical_squared = vertical * vertical;
                let altitude_squared = altitude * altitude;
                let distance = (horizontal_squared + vertical_squared + altitude_squared).sqrt();
                let boundary = (distance - bounds.sphere_radius).max(0.0);
                if boundary * boundary >= maximum_squared {
                    return false;
                }
                // PartitionFilterWouldCollide uses bottom positions even for spheres.
                if position[2] + max_height(geometry) < target_position[2]
                    || position[2] > target_position[2] + bounds.max_height
                {
                    return false;
                }
                intersects(
                    &query,
                    &Body {
                        geometry: self.body_geometry(id),
                        position: target_position,
                        angle: actor.facing,
                    },
                )
            })
            .collect()
    }
}

fn sphere_radius(geometry: &Geometry) -> f32 {
    match geometry.shape {
        Shape::Sphere => geometry.major,
        Shape::Cylinder => (geometry.height * 0.5).max(geometry.major),
        Shape::Box => {
            let half_height = geometry.height * 0.5;
            let major_squared = geometry.major * geometry.major;
            let minor_squared = geometry.minor * geometry.minor;
            let height_squared = half_height * half_height;
            (major_squared + minor_squared + height_squared).sqrt()
        }
    }
}

const fn max_height(geometry: &Geometry) -> f32 {
    if matches!(geometry.shape, Shape::Sphere) {
        geometry.major
    } else {
        geometry.height
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn instant_construction_moves_a_sleeping_friendly_ai_with_two_native_rng_draws() {
        let mut game = super::super::tests::game(Vec::new());
        let owner = game.map_side_state.owners["NPC"];
        let worker = game.spawn(1, owner, [350.0, 450.0], true);
        game.entities[worker].ai.mood.attitude = -2;
        game.update_ground_contacts();
        let mut expected = game.random;
        crate::skirmish::startup::random_real(&mut expected, 0.5, 1.5);
        crate::skirmish::startup::random_real(
            &mut expected,
            -std::f32::consts::PI,
            std::f32::consts::PI,
        );
        assert!(game.move_objects_for_construction(owner, 3, [350.0, 450.0, 0.0], 0.0));
        assert_eq!(game.random, expected);
        assert!(matches!(
            game.entities[worker].ai.order,
            crate::skirmish::Order::Move(_)
        ));
        assert_eq!(game.entities[worker].ai.mood.attitude, -2);
        assert_eq!(
            game.entities[worker].ai_goal_position()[2].to_bits(),
            0.0_f32.to_bits()
        );
    }
}
