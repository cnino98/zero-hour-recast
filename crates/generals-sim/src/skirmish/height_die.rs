//! HeightDieUpdate.cpp: polling delay, containment, direction and post-kill particles.
use super::Skirmish;
use crate::collision_geometry::Shape;

pub struct Rules {
    pub target_height: f32,
    pub includes_structures: bool,
    pub only_moving_down: bool,
    pub particle_height: f32,
    pub snap_to_ground: bool,
    pub initial_delay: u32,
}

pub(super) struct State {
    has_died: bool,
    particles_destroyed: bool,
    last_position: [f32; 3],
    earliest_death: u32,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            has_died: false,
            particles_destroyed: false,
            last_position: [-1.0; 3],
            earliest_death: u32::MAX,
        }
    }
}

impl Skirmish {
    pub(super) fn construct_height_die(&mut self, id: usize) {
        self.entities[id].behaviors.height_die.push(State::new());
    }

    pub(super) fn update_height_die(&mut self, id: usize, ordinal: usize) -> i32 {
        let now = self.update_frame();
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.height_die[ordinal];
        let state = &mut actor.behaviors.height_die[ordinal];
        if state.earliest_death == u32::MAX {
            state.earliest_death = now.wrapping_add(rules.initial_delay);
        }
        if state.earliest_death > now {
            return 1;
        }
        let position = self.object_position(id);
        if self.entities[id].contained_by().is_some() {
            self.entities[id].behaviors.height_die[ordinal].last_position = position;
            return 1;
        }
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.height_die[ordinal];
        let state = &actor.behaviors.height_die[ordinal];
        let mut direction_ok = true;
        if !state.has_died {
            if rules.only_moving_down && position[2] >= state.last_position[2] {
                direction_ok = false;
            }
            let (ground, target) = self.height_die_target(id, rules);
            if position[2] < target && direction_ok {
                if rules.snap_to_ground || position[2] < ground {
                    self.set_object_position(id, [position[0], position[1], ground]);
                }
                self.kill_object(id, super::DeathType::Normal);
                self.entities[id].behaviors.height_die[ordinal].has_died = true;
            }
        }
        // Native pos points at the Object's coordinate, including a death snap.
        let position = self.object_position(id);
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.height_die[ordinal];
        let state = &mut actor.behaviors.height_die[ordinal];
        if !state.particles_destroyed
            && position[2] < rules.particle_height
            && (state.has_died || direction_ok)
        {
            self.presentation
                .weapon_fx
                .push(super::WeaponFxEvent::DestroyAttachedParticles(id));
            state.particles_destroyed = true;
        }
        state.last_position = position;
        1
    }

    fn height_die_target(&self, id: usize, rules: &Rules) -> (f32, f32) {
        let position = self.object_position(id);
        let point = [position[0], position[1]];
        let mut ground = self.ground_height(point);
        if rules.includes_structures {
            let layer = self
                .movement_terrain
                .get_highest_layer_for_destination(position, ground, false);
            if layer != 1 {
                ground = ground.max(self.movement_layer_height(layer, point));
            }
        }
        let mut target = ground + rules.target_height;
        if rules.includes_structures {
            let geometry = self.body_geometry(id);
            let radius = if matches!(geometry.shape, Shape::Box) {
                let mut squared = geometry.major * geometry.major;
                let minor_squared = geometry.minor * geometry.minor;
                squared += minor_squared;
                squared.sqrt()
            } else {
                geometry.major
            };
            let us = self.body_geometry_bounds(id);
            let mut tallest = 0.0_f32;
            // Maximum height is order-independent. The partition owns membership;
            // boundary-sphere distance remains the original strict 3D predicate.
            for other in self.partition.outward_objects(point) {
                if other == id
                    || !self.catalog[self.entities[other].type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "STRUCTURE")
                {
                    continue;
                }
                let theirs = self.object_position(other);
                let bounds = self.body_geometry_bounds(other);
                let x = theirs[0] - position[0];
                let y = theirs[1] - position[1];
                let z = (theirs[2] + bounds.center_height) - (position[2] + us.center_height);
                let mut squared = x * x;
                let y_squared = y * y;
                squared += y_squared;
                let z_squared = z * z;
                squared += z_squared;
                let total_radius = us.sphere_radius + bounds.sphere_radius;
                let distance_squared = if total_radius > 0.0 {
                    let distance = (squared.sqrt() - total_radius).max(0.0);
                    distance * distance
                } else {
                    squared
                };
                if distance_squared < radius * radius {
                    tallest = tallest.max(bounds.max_height);
                }
            }
            if tallest > rules.target_height {
                target = tallest + ground;
            }
        }
        (ground, target)
    }
}

#[cfg(test)]
mod tests;
