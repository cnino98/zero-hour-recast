//! `PartitionManager::iteratePotentialCollisions` and raw boundary distances.
use super::Identity;
use crate::{
    collision_geometry::{Body, Geometry, Shape, would_collide},
    skirmish::Skirmish,
};

impl Skirmish {
    /// The point query has no source Object, so it retains the source itself,
    /// dead objects, disabled collision bodies and every registered map object.
    pub(in crate::skirmish) fn native_potential_collisions(
        &self,
        body: &Body<'_>,
        use_2d: bool,
    ) -> Vec<Identity> {
        let maximum = bounds(body.geometry).1 * 1.1;
        let maximum_squared = maximum * maximum;
        let mut result = Vec::new();
        for (_, identity) in self
            .partition
            .outward_world_range_rings([body.position[0], body.position[1]], maximum)
        {
            let other = match identity {
                Identity::Actor(id) => Body {
                    geometry: self.body_geometry(id),
                    position: self.object_position(id),
                    angle: self.entities[id].facing,
                },
                Identity::Map(id) => {
                    let object = &self.layout.world_objects[id];
                    Body {
                        geometry: &object.geometry,
                        position: object.position,
                        angle: object.angle,
                    }
                }
            };
            // getClosestObjects receives the supplied position with no Object:
            // only the candidate contributes its circle/sphere radius.
            let distance = boundary_distance_squared(
                body.position,
                None,
                other.position,
                Some(other.geometry),
                use_2d,
            );
            if distance < maximum_squared && would_collide(body, &other) {
                result.push(identity);
            }
        }
        // SimpleObjectIterator::insert prepends and this query does not sort.
        result.reverse();
        result
    }
}

fn boundary_distance_squared(
    first: [f32; 3],
    first_geometry: Option<&Geometry>,
    second: [f32; 3],
    second_geometry: Option<&Geometry>,
    use_2d: bool,
) -> f32 {
    let delta = [
        second[0] - first[0],
        second[1] - first[1],
        if use_2d {
            0.0
        } else {
            (second[2] + second_geometry.map_or(0.0, |geometry| bounds(geometry).0))
                - (first[2] + first_geometry.map_or(0.0, |geometry| bounds(geometry).0))
        },
    ];
    let squares = delta.map(|value| value * value);
    let actual_squared = if use_2d {
        squares[0] + squares[1]
    } else {
        squares[0] + squares[1] + squares[2]
    };
    let radius = |geometry: &Geometry| {
        if use_2d {
            circle_radius(geometry)
        } else {
            bounds(geometry).1
        }
    };
    let total_radius = first_geometry.map_or(0.0, radius) + second_geometry.map_or(0.0, radius);
    if total_radius > 0.0 {
        let distance = actual_squared.sqrt() - total_radius;
        if distance <= 0.0 {
            0.0
        } else {
            distance * distance
        }
    } else {
        actual_squared
    }
}

fn circle_radius(geometry: &Geometry) -> f32 {
    if matches!(geometry.shape, Shape::Box) {
        let squares = [
            geometry.major * geometry.major,
            geometry.minor * geometry.minor,
        ];
        (squares[0] + squares[1]).sqrt()
    } else {
        geometry.major
    }
}

fn bounds(geometry: &Geometry) -> (f32, f32) {
    if matches!(geometry.shape, Shape::Sphere) {
        return (0.0, geometry.major);
    }
    let center = geometry.height * 0.5;
    let radius = if matches!(geometry.shape, Shape::Box) {
        let squares = [
            geometry.major * geometry.major,
            geometry.minor * geometry.minor,
            center * center,
        ];
        (squares[0] + squares[1] + squares[2]).sqrt()
    } else if center < geometry.major {
        geometry.major
    } else {
        center
    };
    (center, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{DeathType, tests};

    #[test]
    fn contact_query_keeps_dead_and_no_collision_objects_and_positive_sphere_height() {
        let mut game = tests::game();
        let source = game.spawn(5, 0, [300.0, 300.0], true);
        let other = game.spawn(5, 1, [300.0, 300.0], true);
        game.kill_object(other, DeathType::Normal);
        game.set_dynamic_geometry(source, [5.0, 10.0, 10.0]);
        game.set_dynamic_geometry(other, [5.0, 10.0, 10.0]);
        game.update_ground_contacts();
        let geometry = Geometry {
            is_small: false,
            shape: Shape::Sphere,
            major: 10.0,
            minor: 10.0,
            height: 10.0,
        };
        let body = Body {
            geometry: &geometry,
            position: [300.0, 300.0, 7.0],
            angle: 0.0,
        };
        // Ordinary sphere contact extends below its canonical position. Native
        // WouldCollide rejects the lower body before running the shape test.
        let actor = Body {
            geometry: game.body_geometry(other),
            position: game.object_position(other),
            angle: 0.0,
        };
        assert!(crate::collision_geometry::intersects(&body, &actor));
        assert!(
            !game
                .native_potential_collisions(&body, false)
                .contains(&Identity::Actor(other))
        );
        let body = Body {
            position: [300.0, 300.0, 0.0],
            ..body
        };
        let found = game.native_potential_collisions(&body, false);
        assert!(found.contains(&Identity::Actor(source)));
        assert!(found.contains(&Identity::Actor(other)));
        game.destroy_maintenance_object(other);
        game.process_update_destruction();
        assert!(
            !game
                .native_potential_collisions(&body, false)
                .contains(&Identity::Actor(other))
        );
    }

    #[test]
    fn boundary_distance_uses_circle_and_geometry_center_distinct_from_sphere() {
        let geometry = Geometry {
            is_small: false,
            shape: Shape::Box,
            major: 3.0,
            minor: 4.0,
            height: 100.0,
        };
        assert_eq!(
            boundary_distance_squared([0.0; 3], None, [8.0, 0.0, 0.0], Some(&geometry), true)
                .to_bits(),
            9.0_f32.to_bits()
        );
        assert_eq!(
            boundary_distance_squared(
                [0.0; 3],
                Some(&geometry),
                [13.0, 0.0, 0.0],
                Some(&geometry),
                true
            )
            .to_bits(),
            9.0_f32.to_bits()
        );
        let distance =
            boundary_distance_squared([0.0; 3], None, [8.0, 0.0, 0.0], Some(&geometry), false);
        assert!(distance > 0.0 && distance < 1.0);
    }
}
