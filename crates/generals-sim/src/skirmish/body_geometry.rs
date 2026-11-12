//! `ActiveBody::setCorrectDamageState`, `Object::setGeometryInfoZ` and
//! `GeometryInfo::setMaxHeightAbovePosition`, SH 2bfaea60.
use super::{GeometryBounds, Skirmish};
use crate::collision_geometry::{Geometry, Shape};

pub(super) struct State {
    geometry: Geometry,
    bounds: GeometryBounds,
    no_collisions: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn copy_object_geometry(&mut self, target: usize, source: usize) {
        let geometry = self.body_geometry(source);
        let bounds = self.body_geometry_bounds(source);
        let state = State {
            geometry: Geometry {
                shape: geometry.shape,
                is_small: geometry.is_small,
                major: geometry.major,
                minor: geometry.minor,
                height: geometry.height,
            },
            bounds: GeometryBounds {
                center_height: bounds.center_height,
                sphere_radius: bounds.sphere_radius,
                max_height: bounds.max_height,
            },
            no_collisions: self.entities[target]
                .body_geometry
                .as_ref()
                .is_some_and(|state| state.no_collisions),
        };
        self.entities[target].body_geometry = Some(state);
        self.rebuild_object_partition_geometry(target);
    }

    /// Current native `GeometryInfo`, including instance changes and rubble.
    #[must_use]
    pub fn body_geometry(&self, id: usize) -> &Geometry {
        let actor = &self.entities[id];
        actor
            .body_geometry
            .as_ref()
            .map_or(&self.catalog[actor.type_index].geometry, |state| {
                &state.geometry
            })
    }

    /// Bounds cached with the current native `GeometryInfo`.
    #[must_use]
    pub fn body_geometry_bounds(&self, id: usize) -> &GeometryBounds {
        let actor = &self.entities[id];
        actor
            .body_geometry
            .as_ref()
            .map_or(&self.catalog[actor.type_index].geometry_bounds, |state| {
                &state.bounds
            })
    }

    pub(super) fn body_no_collisions(&self, id: usize) -> bool {
        self.body_collision_status(id) || self.catalog[self.entities[id].type_index].no_collide()
    }

    pub(super) fn body_collision_status(&self, id: usize) -> bool {
        // ActiveBody sets this status with rubble geometry and never clears it on healing.
        self.entities[id].object_status & (1 << 5) != 0
            || self.entities[id]
                .projectile_flight
                .as_ref()
                .is_some_and(super::projectiles::Flight::no_collisions)
            || self.entities[id]
                .body_geometry
                .as_ref()
                .is_some_and(|state| state.no_collisions)
    }

    pub(super) fn set_rubble_geometry(&mut self, id: usize) -> bool {
        let actor = &self.entities[id];
        let height = self.catalog[actor.type_index]
            .maintenance
            .rubble_height
            .expect("STRUCTURE rubble height");
        let old = self.body_geometry(id);
        let changed = actor.body_geometry.is_none()
            || if matches!(old.shape, Shape::Sphere) {
                old.major.to_bits() != height.to_bits()
            } else {
                old.height.to_bits() != height.to_bits()
            };
        self.entities[id].body_geometry = Some(with_height(old, height));
        // Z-only mutation deliberately retains the native partition cells.
        changed
    }

    pub(in crate::skirmish) fn set_dynamic_geometry(&mut self, id: usize, dimensions: [f32; 3]) {
        let old = self.body_geometry(id);
        let [height, major, minor] = dimensions;
        // GeometryInfo::set normalizes the irrelevant dimensions by shape.
        let (height, minor) = match old.shape {
            Shape::Sphere => (major, major),
            Shape::Cylinder => (height, major),
            Shape::Box => (height, minor),
        };
        let geometry = Geometry {
            shape: old.shape,
            is_small: old.is_small,
            major,
            minor,
            height,
        };
        let no_collisions = self.entities[id]
            .body_geometry
            .as_ref()
            .is_some_and(|state| state.no_collisions);
        let mut state = with_height(&geometry, height);
        state.no_collisions = no_collisions;
        self.entities[id].body_geometry = Some(state);
        self.rebuild_object_partition_geometry(id);
    }
}

fn with_height(old: &Geometry, height: f32) -> State {
    let mut geometry = Geometry {
        shape: old.shape,
        is_small: old.is_small,
        major: old.major,
        minor: old.minor,
        height: old.height,
    };
    let bounds = if matches!(geometry.shape, Shape::Sphere) {
        geometry.major = height;
        GeometryBounds {
            center_height: 0.0,
            sphere_radius: height,
            max_height: height,
        }
    } else {
        geometry.height = height;
        let center_height = height * 0.5;
        let sphere_radius = if matches!(geometry.shape, Shape::Box) {
            let major_squared = geometry.major * geometry.major;
            let minor_squared = geometry.minor * geometry.minor;
            let height_squared = center_height * center_height;
            (major_squared + minor_squared + height_squared).sqrt()
        } else if center_height < geometry.major {
            geometry.major
        } else {
            center_height
        };
        GeometryBounds {
            center_height,
            sphere_radius,
            max_height: height,
        }
    };
    State {
        geometry,
        bounds,
        no_collisions: true,
    }
}

#[cfg(test)]
mod tests;
