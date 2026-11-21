//! SH `NeutronMissileUpdate` and `NeutronMissileSlowDeathBehavior`.
use super::{
    DamageType, DeathType, ModelConditions, ProjectileFxStage, Skirmish, WeaponBinding,
    death_lifecycle::Derived, physical_ocl::transform, radius_decal, startup,
};
use num_traits::ToPrimitive;
use std::f32::consts::FRAC_PI_2;

pub struct Rules {
    pub initial_distance: f32,
    pub turn_rate: f32,
    pub damping: f32,
    pub speed: f32,
    pub approach_height: f32,
    pub rise_frames: u32,
    pub rise_height: f32,
    pub rise_acceleration: f32,
    pub jitter: f32,
    pub launch_fx: String,
    pub ignition_fx: String,
    pub decal: radius_decal::Template,
    pub decal_radius: f32,
    pub exhaust: String,
}

pub struct DeathRules {
    pub scorch_size: f32,
    pub effect: String,
    pub blasts: Vec<Blast>,
}

pub struct Blast {
    pub delay: f32,
    pub scorch_delay: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub max_damage: f32,
    pub min_damage: f32,
    pub topple_speed: f32,
}

struct Launch {
    launcher: usize,
    binding: WeaponBinding,
    barrel: usize,
}

struct Flight {
    velocity: [f32; 3],
    distance_left: f32,
    launch_frame: u32,
    launch_height: f32,
    reached_approach: bool,
}

#[derive(Default)]
pub(in crate::skirmish) struct State {
    launch: Option<Launch>,
    flight: Option<Flight>,
    target: [f32; 3],
    activation_frame: Option<u32>,
    completed_blasts: u16,
    completed_scorches: u16,
    scorch_placed: bool,
}

impl Skirmish {
    pub(in crate::skirmish) fn prepare_neutron_missile(
        &mut self,
        id: usize,
        launcher: usize,
        binding: WeaponBinding,
        barrel: usize,
        destination: [f32; 3],
        target: Option<usize>,
    ) {
        let target = target.map_or(destination, |victim| self.object_position(victim));
        let actor = &mut self.entities[id];
        let state = &mut actor.neutron_missile;
        state.launch = Some(Launch {
            launcher,
            binding,
            barrel,
        });
        state.target = target;
        let rules = self.catalog[actor.type_index]
            .behavior
            .neutron_missile
            .as_ref()
            .expect("bound NeutronMissileUpdate");
        self.create_radius_decal(id, rules.decal.clone(), rules.decal_radius, target, false);
        self.set_drawable_hidden(id, true);
    }

    pub(in crate::skirmish) fn update_neutron_missile(&mut self, id: usize) -> i32 {
        if self.entities[id].radius_decal.is_some() {
            self.update_radius_decal(id);
        }
        if let Some(launch) = self.entities[id].neutron_missile.launch.take() {
            self.launch_neutron_missile(id, &launch);
        } else if self.entities[id].neutron_missile.flight.is_some() {
            self.fly_neutron_missile(id);
        }
        // This stock projectile has no PhysicsBehavior; its HeightDieUpdate
        // owns contact with terrain during descent and the death snap.
        1
    }

    fn launch_neutron_missile(&mut self, id: usize, launch: &Launch) {
        if !self.entities[launch.launcher].exists() {
            self.destroy_maintenance_object(id);
            return;
        }
        let pose = self.launch_transform(launch.launcher, launch.binding, launch.barrel);
        let mut basis =
            std::array::from_fn(|row| std::array::from_fn(|column| pose.axes[column][row]));
        transform::rotate(&mut basis, [FRAC_PI_2, 0.0, 0.0]);
        let velocity = self.entities[launch.launcher].physics_velocity();
        let now = self.update_frame();
        let rules = self.catalog[self.entities[id].type_index]
            .behavior
            .neutron_missile
            .as_ref()
            .expect("bound NeutronMissileUpdate");
        let distance_left = rules.initial_distance;
        let reached_approach = rules.approach_height == 0.0;
        let launch_fx = rules.launch_fx.clone();
        let ignition_fx = rules.ignition_fx.clone();
        let exhaust = rules.exhaust.clone();
        self.ensure_projectile_motion(id);
        self.set_drawable_hidden(id, false);
        self.set_object_transform(id, pose.position, basis);
        self.entities[id].experience_sink = Some(launch.launcher);
        self.neutron_fx(id, launch_fx);
        self.change_neutron_pose(id, add(pose.position, velocity), basis);
        self.neutron_fx(id, ignition_fx);
        if !exhaust.is_empty() {
            self.presentation
                .weapon_fx
                .push(super::WeaponFxEvent::AttachedParticle(
                    self.presentation.attached_particles.len(),
                ));
            self.presentation.attached_particles.push(
                super::helicopter_death::ParticleAttachment {
                    object: id,
                    name: exhaust,
                    point: [0.0; 3],
                },
            );
        }
        if let Some(index) = self.projectile_component(id) {
            let snapshot = self.projectiles[index].callback_snapshot(&self.entities);
            self.record_projectile_event(
                &snapshot,
                self.object_position(id),
                ProjectileFxStage::Created,
            );
        }
        self.entities[id].neutron_missile.flight = Some(Flight {
            velocity,
            distance_left,
            launch_frame: now,
            launch_height: pose.position[2],
            reached_approach,
        });
    }

    fn fly_neutron_missile(&mut self, id: usize) {
        let now = self.update_frame();
        let radius = self.body_geometry_bounds(id).sphere_radius;
        let (mut position, old_basis) = self.object_transform(id);
        let unit = &self.catalog[self.entities[id].type_index];
        let rules = unit
            .behavior
            .neutron_missile
            .as_ref()
            .expect("bound NeutronMissileUpdate");
        let actor = &mut self.entities[id];
        let state = &mut actor.neutron_missile;
        let flight = state.flight.as_mut().expect("launched NeutronMissile");
        let mut intermediate = state.target;
        intermediate[2] += rules.approach_height;
        if !flight.reached_approach
            && square_length(subtract(intermediate, position)) <= radius * radius
        {
            flight.reached_approach = true;
            position = intermediate;
            flight.velocity = [0.0, 0.0, -square_length(flight.velocity).sqrt() * 0.5];
        }
        let basis = if flight.distance_left > 0.0 {
            old_basis
        } else {
            let destination = if flight.reached_approach {
                state.target
            } else {
                intermediate
            };
            turn_towards(old_basis, subtract(destination, position), rules.turn_rate)
        };
        let direction = normalize(basis.map(|row| row[0]));
        let mut speed = rules.speed;
        if rules.approach_height != 0.0 && flight.reached_approach {
            speed *= 0.5;
        }
        for (velocity, direction) in flight.velocity.iter_mut().zip(direction) {
            let driven = speed * direction;
            let damping = rules.damping * *velocity;
            let acceleration = driven - damping;
            *velocity += acceleration;
        }
        let elapsed = now.wrapping_sub(flight.launch_frame);
        if rules.rise_frames > 0 && elapsed <= rules.rise_frames {
            actor.drawable.instance_matrix = None;
            if elapsed < rules.rise_frames {
                let time = elapsed.to_f32().expect("native flight frame")
                    / rules.rise_frames.to_f32().expect("native rise duration");
                let factor = rules.rise_acceleration.max(0.01);
                let scaled = factor * time;
                let rise = ((scaled * scaled) / factor) * rules.rise_height;
                let z = flight.launch_height + rise;
                flight.velocity = [0.0, 0.0, z - position[2]];
                if rules.jitter > 0.0 {
                    let amplitude = (1.0 - time) * rules.jitter;
                    let jitter = transform::vector(
                        basis,
                        [
                            0.0,
                            startup::random_real(&mut self.random, -1.0, 1.0) * amplitude,
                            startup::random_real(&mut self.random, -1.0, 1.0) * amplitude,
                        ],
                    );
                    actor.drawable.instance_matrix = Some([
                        [1.0, 0.0, 0.0, jitter[0]],
                        [0.0, 1.0, 0.0, jitter[1]],
                        [0.0, 0.0, 1.0, jitter[2]],
                    ]);
                }
            }
        }
        let next = add(position, flight.velocity);
        if flight.distance_left > 0.0 {
            flight.distance_left -= square_length(subtract(next, position)).sqrt();
        }
        self.change_neutron_pose(id, next, basis);
    }

    fn change_neutron_pose(&mut self, id: usize, position: [f32; 3], basis: [[f32; 3]; 3]) {
        self.set_object_transform(id, position, basis);
        if let Some(index) = self.projectile_component(id) {
            self.projectiles[index].position = [position[0], position[1]];
        }
    }

    fn neutron_fx(&mut self, id: usize, fx: String) {
        if !fx.is_empty() {
            self.object_fx(id, fx);
        }
    }

    pub(in crate::skirmish) fn on_neutron_missile_die(&mut self, id: usize) {
        let state = &mut self.entities[id].neutron_missile;
        state.flight = None;
        self.entities[id].radius_decal = None;
        // The original death model states have no missile mesh. The object and
        // its attached exhaust remain until the authored slow-death cleanup.
        self.set_drawable_hidden(id, true);
        self.hide_bound_projectile(id);
    }

    pub(in crate::skirmish) fn update_neutron_death(&mut self, id: usize, ordinal: usize) -> i32 {
        let index = self.slow_death_index(id, "NeutronMissileSlowDeathBehavior", ordinal);
        self.update_slow_death_base(id, index);
        let state = self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state");
        if !state.native[index].active() || !self.entities[id].exists() {
            return 1;
        }
        let Derived::Neutron(rules) = &self.native_slow_rule(id, index).derived else {
            unreachable!("bound NeutronMissileSlowDeathBehavior");
        };
        let now = self.update_frame();
        let activation = self.entities[id].neutron_missile.activation_frame;
        if activation.is_none() {
            let effect = rules.effect.clone();
            self.entities[id].neutron_missile.activation_frame = Some(now);
            if !effect.is_empty() {
                let mut position = self.object_position(id);
                position[2] = self.ground_height([position[0], position[1]]);
                self.positional_fx_with_radius(id, effect, position, 0.0);
            }
        }
        let elapsed = now.wrapping_sub(activation.unwrap_or(now));
        self.update_neutron_blasts(id, index, elapsed.to_f32().expect("native blast frame"));
        1
    }

    fn update_neutron_blasts(&mut self, id: usize, index: usize, elapsed: f32) {
        let Derived::Neutron(rules) = &self.native_slow_rule(id, index).derived else {
            unreachable!("bound NeutronMissileSlowDeathBehavior");
        };
        let count = rules.blasts.len();
        let scorch_size = rules.scorch_size;
        for index_in_wave in 0..count {
            let Derived::Neutron(rules) = &self.native_slow_rule(id, index).derived else {
                unreachable!("bound NeutronMissileSlowDeathBehavior");
            };
            let blast = &rules.blasts[index_in_wave];
            let bit = 1 << index_in_wave;
            let state = &self.entities[id].neutron_missile;
            let scorch = state.completed_scorches & bit == 0 && elapsed > blast.scorch_delay;
            let damage = state.completed_blasts & bit == 0 && elapsed > blast.delay;
            let outer = blast.outer_radius;
            if damage {
                let blast = Blast {
                    delay: blast.delay,
                    scorch_delay: blast.scorch_delay,
                    inner_radius: blast.inner_radius,
                    outer_radius: outer,
                    max_damage: blast.max_damage,
                    min_damage: blast.min_damage,
                    topple_speed: blast.topple_speed,
                };
                self.neutron_blast_wave(id, &blast, scorch_size);
                self.entities[id].neutron_missile.completed_blasts |= bit;
            }
            if scorch {
                self.neutron_scorch_wave(id, outer);
                self.entities[id].neutron_missile.completed_scorches |= bit;
            }
        }
    }

    fn neutron_blast_wave(&mut self, id: usize, blast: &Blast, scorch_size: f32) {
        let position = self.object_position(id);
        // CRC=0 and both PRESERVE_RETAIL_NUKE_MISSILE_OUTER_RADIUS guards=0.
        for target in self.sphere_range_targets(position, blast.outer_radius) {
            let center = subtract(self.object_position(target), position);
            let bounds = self.body_geometry_bounds(target);
            let mut edge = center;
            edge[2] += bounds.center_height;
            let length = square_length(edge).sqrt();
            let ratio = if bounds.sphere_radius > 0.0 {
                if length <= bounds.sphere_radius {
                    0.0
                } else {
                    (length - bounds.sphere_radius) / length
                }
            } else {
                1.0
            };
            let force = std::array::from_fn(|axis| {
                let edge = edge[axis] * ratio;
                (center[axis] + edge) * 0.5
            });
            self.topple_object(target, force, blast.topple_speed, 3);
            let distance = square_length(force).sqrt();
            let damage = if distance <= blast.inner_radius {
                blast.max_damage
            } else {
                let factor = 1.0
                    - (distance - blast.inner_radius)
                        / (blast.outer_radius - blast.inner_radius + 0.01);
                (blast.max_damage * factor).max(blast.min_damage)
            };
            if damage != 0.0 {
                self.attempt_health_damage(
                    target,
                    Some(id),
                    damage,
                    DamageType::Explosion,
                    DeathType::Exploded,
                );
                if !self.entities[id].neutron_missile.scorch_placed {
                    self.presentation
                        .weapon_fx
                        .push(super::WeaponFxEvent::NeutronScorch(
                            self.presentation.neutron_scorches.len(),
                        ));
                    self.presentation.neutron_scorches.push(Scorch {
                        position,
                        radius: scorch_size,
                    });
                    self.entities[id].neutron_missile.scorch_placed = true;
                }
            }
        }
    }

    fn neutron_scorch_wave(&mut self, id: usize, radius: f32) {
        if radius == 0.0 {
            return;
        }
        let radius_squared = radius * radius;
        let center = self.entities[id].position;
        for target in self.partition.fastest_range_objects(center, radius) {
            let position = self.entities[target].position;
            let dx = position[0] - center[0];
            let dy = position[1] - center[1];
            let xx = dx * dx;
            let yy = dy * dy;
            if xx + yy < radius_squared {
                self.entities[target]
                    .drawable
                    .model_conditions
                    .set_flag(ModelConditions::BURNED, true);
                if self.attack_kind(target, "SHRUBBERY") {
                    self.entities[target].drawable.status &= !2;
                }
            }
        }
    }
}

pub struct Scorch {
    pub position: [f32; 3],
    pub radius: f32,
}

fn turn_towards(basis: [[f32; 3]; 3], direction: [f32; 3], limit: f32) -> [[f32; 3]; 3] {
    let from = basis.map(|row| row[0]);
    let target = normalize(direction);
    let angle = dot(from, target).clamp(-1.0, 1.0).acos();
    let [x, y, z] = if angle < limit {
        target
    } else {
        let axis = normalize(cross(from, target));
        let (sin, cos) = limit.sin_cos();
        let perpendicular = cross(axis, from);
        let projection = dot(axis, from) * (1.0 - cos);
        std::array::from_fn(|i| {
            let current = from[i] * cos;
            let rotated = perpendicular[i] * sin;
            let projected = axis[i] * projection;
            (current + rotated) + projected
        })
    };
    let horizontal = square_length([x, y, 0.0]).sqrt();
    let (sin, cos) = if horizontal == 0.0 {
        (0.0, 1.0)
    } else {
        (y / horizontal, x / horizontal)
    };
    [
        [cos * horizontal, -sin, -cos * z],
        [sin * horizontal, cos, -sin * z],
        [z, 0.0, horizontal],
    ]
}

fn add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| left[i] + right[i])
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| left[i] - right[i])
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [(1, 2), (2, 0), (0, 1)].map(|(a, b)| {
        let positive = left[a] * right[b];
        let negative = left[b] * right[a];
        positive - negative
    })
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    let products: [f32; 3] = std::array::from_fn(|i| left[i] * right[i]);
    (products[0] + products[1]) + products[2]
}

fn square_length(value: [f32; 3]) -> f32 {
    dot(value, value)
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = square_length(value).sqrt();
    if length == 0.0 {
        value
    } else {
        value.map(|component| component / length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Kind, tests::game};

    #[test]
    fn positional_blast_places_one_scorch_without_a_second_object_in_range() {
        let mut game = game();
        let kind = game.type_for(0, Kind::Tank).unwrap();
        let source = game.spawn(kind, 0, [550.0, 370.0], true);
        game.update_ground_contacts();
        let position = game.object_position(source);
        assert!(game.emp_range_targets(source, 10.0).is_empty());
        assert_eq!(game.sphere_range_targets(position, 10.0), [source]);
        let blast = Blast {
            delay: 0.0,
            scorch_delay: 0.0,
            inner_radius: 5.0,
            outer_radius: 10.0,
            max_damage: 1.0,
            min_damage: 1.0,
            topple_speed: 0.0,
        };
        game.neutron_blast_wave(source, &blast, 70.0);
        game.neutron_blast_wave(source, &blast, 70.0);
        assert_eq!(game.presentation.neutron_scorches.len(), 1);
        let scorch = &game.presentation.neutron_scorches[0];
        assert_eq!(
            scorch.position.map(f32::to_bits),
            position.map(f32::to_bits)
        );
        assert_eq!(scorch.radius.to_bits(), 70.0_f32.to_bits());
    }
}
