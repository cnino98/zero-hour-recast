//! SH HelicopterSlowDeathUpdate.cpp: per-instance derived `SlowDeath` state.
use super::{
    Entity, Skirmish, UnitAiUpdate,
    aircraft_death::Phase,
    death_lifecycle::{Derived, native::Extension},
    object_sounds::Action,
    physical_ocl::transform,
    startup,
};
use num_traits::ToPrimitive;

#[cfg(test)]
mod tests;

pub struct Rules {
    pub orbit_turn: f32,
    pub orbit_speed: f32,
    pub damping: f32,
    pub spin: [f32; 2],
    pub spin_delay: f32,
    pub spin_step: f32,
    pub fall: f32,
    pub blade_delay: [f32; 2],
    pub phases: [Phase; 3],
    pub final_delay: f32,
    pub rubble: String,
    pub loop_sound: String,
    pub braking: f32,
    pub particle: String,
    pub particle_bone: String,
    pub particle_point: [f32; 3],
    pub blade_bone: String,
    pub blade_point: Option<[f32; 3]>,
    pub eject_pilot: Option<super::object_creation::List>,
    pub eject_sounds: [String; 2],
}

pub struct ParticleAttachment {
    pub object: usize,
    pub name: String,
    pub point: [f32; 3],
}

pub(in crate::skirmish) struct NativeState {
    forward_angle: f32,
    forward_speed: f32,
    spin: f32,
    towards_max: bool,
    last_spin: u32,
    blade_frames: u32,
    ground_frame: u32,
}

impl NativeState {
    pub(in crate::skirmish) const fn new() -> Self {
        Self {
            forward_angle: 0.0,
            forward_speed: 0.0,
            spin: 0.0,
            towards_max: false,
            last_spin: 0,
            blade_frames: 0,
            ground_frame: 0,
        }
    }
}

impl Entity {
    #[must_use]
    pub fn helicopter_corpse(&self) -> bool {
        !self.masked()
            && self.death_lifecycle.as_ref().is_some_and(|state| {
                state.native.iter().any(|state| {
                    state.active() && matches!(state.extension, Extension::Helicopter(_))
                })
            })
    }

    #[must_use]
    pub const fn helicopter_hit_ground(&self) -> bool {
        self.drawable.model_conditions.has_flag(1 << 6)
    }

    fn native_helicopter(&self, index: usize) -> &NativeState {
        let Extension::Helicopter(state) = &self
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[index]
            .extension
        else {
            unreachable!("HelicopterSlowDeath state")
        };
        state
    }

    fn native_helicopter_mut(&mut self, index: usize) -> &mut NativeState {
        let Extension::Helicopter(state) = &mut self
            .death_lifecycle
            .as_mut()
            .expect("SlowDeath state")
            .native[index]
            .extension
        else {
            unreachable!("HelicopterSlowDeath state")
        };
        state
    }
}

impl Skirmish {
    fn native_helicopter_rule(&self, id: usize, index: usize) -> &Rules {
        let Derived::Helicopter(rule) = &self.native_slow_rule(id, index).derived else {
            unreachable!("HelicopterSlowDeath data")
        };
        rule
    }

    pub(in crate::skirmish) fn helicopter_slow_death_on_die(&mut self, id: usize, ordinal: usize) {
        let index = self.slow_death_index(id, "HelicopterSlowDeathBehavior", ordinal);
        self.native_slow_death_on_die(id, index);
    }

    pub(in crate::skirmish) fn begin_native_helicopter_death(&mut self, id: usize, index: usize) {
        self.activate_slow_death(id, index);
        if self.entities[id].drawable.present {
            self.object_sound(id, String::new(), Action::StopAmbient);
        }
        let rule = self.native_helicopter_rule(id, index);
        let sound = rule.loop_sound.clone();
        let delay = rule.blade_delay;
        let speed = rule.orbit_speed;
        let spin = rule.spin[0];
        let lift = -self.gravity * (1.0 - rule.fall);
        let braking = rule.braking;
        self.object_sound(
            id,
            sound,
            Action::PlayLoop(self.slow_death_module_index(id, index)),
        );
        let blades = startup::random_real(&mut self.random, delay[0], delay[1])
            .to_u32()
            .expect("native blade counter");
        let angle = self.entities[id].facing;
        let state = self.entities[id].native_helicopter_mut(index);
        state.blade_frames = blades;
        state.forward_angle = angle;
        state.forward_speed = speed;
        state.spin = spin;
        state.towards_max = true;
        if matches!(
            self.catalog[self.entities[id].type_index].ai.update,
            UnitAiUpdate::Absent
        ) {
            return;
        }
        self.set_flight_lift_limit(id, lift);
        self.set_helicopter_braking(id, braking);
        let rule = self.native_helicopter_rule(id, index);
        if !rule.particle.is_empty() {
            let point = if !rule.particle_bone.is_empty() && !self.entities[id].drawable.present {
                [0.0; 3]
            } else {
                rule.particle_point
            };
            let event = ParticleAttachment {
                object: id,
                name: rule.particle.clone(),
                point,
            };
            self.presentation
                .weapon_fx
                .push(super::WeaponFxEvent::AttachedParticle(
                    self.presentation.attached_particles.len(),
                ));
            self.presentation.attached_particles.push(event);
        }
    }

    pub(in crate::skirmish) fn update_helicopter_slow_death(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let index = self.slow_death_index(id, "HelicopterSlowDeathBehavior", ordinal);
        self.update_slow_death_base(id, index);
        if !self.entities[id]
            .death_lifecycle
            .as_ref()
            .expect("SlowDeath state")
            .native[index]
            .active()
        {
            return 1;
        }
        if self.entities[id].native_helicopter(index).ground_frame == 0 {
            self.spiral_helicopter(id, index);
        }
        let now = self.update_frame();
        if self.entities[id].native_helicopter(index).ground_frame == 0 {
            let point = self.object_position(id);
            let terrain = self.ground_height([point[0], point[1]]);
            let layer = self.movement_terrain.get_highest_layer_for_destination(
                [point[0], point[1], 99999.0],
                terrain,
                false,
            );
            self.entities[id].layer = layer;
            let ground = self.movement_terrain.destination_layer_height(
                layer,
                [point[0], point[1]],
                terrain,
            );
            if point[2] <= ground + 1.0 || self.physics_collided_with_tree(id) {
                self.entities[id].native_helicopter_mut(index).ground_frame = now;
                self.native_helicopter_phase(id, index, 1);
                self.set_disabled_held(id);
                if self.entities[id].drawable.present {
                    self.entities[id]
                        .drawable
                        .model_conditions
                        .set_flag(1 << 6, true);
                }
                let sound = self.native_helicopter_rule(id, index).loop_sound.clone();
                self.object_sound(
                    id,
                    sound,
                    Action::StopLoop(self.slow_death_module_index(id, index)),
                );
            }
        }
        let ground_frame = self.entities[id].native_helicopter(index).ground_frame;
        if ground_frame != 0
            && now
                .wrapping_sub(ground_frame)
                .to_f32()
                .expect("native frame")
                > self.native_helicopter_rule(id, index).final_delay
        {
            self.native_helicopter_phase(id, index, 2);
            let name = &self.native_helicopter_rule(id, index).rubble;
            if let Some(kind) = self.catalog.iter().position(|unit| &unit.name == name) {
                let team = self.team_membership.team_of(id);
                let rubble = self.new_object_on_team(kind, self.entities[id].owner, team);
                let (position, basis) = self.object_transform(id);
                self.set_object_transform(rubble, position, basis);
            }
            self.destroy_maintenance_object(id);
        }
        1
    }

    fn spiral_helicopter(&mut self, id: usize, index: usize) {
        let spin = self.entities[id].native_helicopter(index).spin;
        let (point, mut basis) = self.object_transform(id);
        pre_rotate_z(&mut basis, spin);
        self.set_object_transform(id, point, basis);
        let rule = self.native_helicopter_rule(id, index);
        let spin_delay = rule.spin_delay;
        let spin_step = rule.spin_step / 30.0;
        let limits = rule.spin;
        let turn = rule.orbit_turn;
        let damping = rule.damping;
        let now = self.update_frame();
        let state = self.entities[id].native_helicopter_mut(index);
        if spin_delay != 0.0
            && now
                .wrapping_sub(state.last_spin)
                .to_f32()
                .expect("native spin frame")
                > spin_delay
        {
            if state.towards_max {
                state.spin += spin_step;
                if state.spin > limits[1] {
                    state.spin = limits[1];
                    state.towards_max = false;
                }
            } else {
                state.spin -= spin_step;
                if state.spin < limits[0] {
                    state.spin = limits[0];
                    state.towards_max = true;
                }
            }
            state.last_spin = now;
        }
        let force = [
            f64::from(state.forward_angle).cos().to_f32().expect("cos") * state.forward_speed,
            f64::from(state.forward_angle).sin().to_f32().expect("sin") * state.forward_speed,
            0.0,
        ];
        self.apply_physics_motive_force(id, force);
        let state = self.entities[id].native_helicopter_mut(index);
        state.forward_angle += turn;
        state.forward_speed *= damping;
        if state.blade_frames > 0 {
            state.blade_frames -= 1;
            if state.blade_frames == 0 {
                self.native_helicopter_blades(id, index);
            }
        }
    }

    fn native_helicopter_blades(&mut self, id: usize, index: usize) {
        let position = self.object_position(id);
        let rule = self.native_helicopter_rule(id, index);
        let Phase { effect, objects } = rule.phases[0].clone();
        let point = if self.entities[id].drawable.present {
            // A missing pristine bone leaves the caller's cached world point in bladePos.
            let local = rule.blade_point.unwrap_or(position);
            let (origin, basis) = self.object_transform(id);
            let offset = transform::vector(basis, local);
            std::array::from_fn(|axis| origin[axis] + offset[axis])
        } else {
            position
        };
        self.object_fx_at(id, effect, point, transform::yaw(0.0));
        if let Some(list) = objects {
            self.create_physical_ocl_at(id, &list, point)
                .expect("bound blade OCL");
        }
        if self.entities[id].veterancy > 0
            && let Some(list) = self.native_helicopter_rule(id, index).eject_pilot.clone()
        {
            let sounds = self.native_helicopter_rule(id, index).eject_sounds.clone();
            self.eject_pilot_objects(id, None, &list, sounds);
        }
    }

    fn native_helicopter_phase(&mut self, id: usize, index: usize, phase: usize) {
        let Phase { effect, objects } =
            self.native_helicopter_rule(id, index).phases[phase].clone();
        self.object_fx(id, effect);
        if let Some(list) = objects {
            self.create_physical_ocl(id, &list)
                .expect("bound helicopter death OCL");
        }
    }
}

// Matrix3D::In_Place_Pre_Rotate_Z rotates rows without translating the origin.
fn pre_rotate_z(basis: &mut [[f32; 3]; 3], angle: f32) {
    let cosine = angle.cos();
    let sine = angle.sin();
    let [first_row, second_row, _] = basis;
    for (first, second) in first_row.iter_mut().zip(second_row) {
        let horizontal = [cosine * *first, sine * *second];
        let vertical = [sine * *first, cosine * *second];
        *first = horizontal[0] - horizontal[1];
        *second = vertical[0] + vertical[1];
    }
}
