use super::{
    DebrisDraw, INHERIT_VELOCITY, LIKE_EXISTING, List, Nugget, ON_GROUND, RANDOM_FORCE,
    SEND_FLYING, SEND_UP, Skirmish, startup, transform,
};
use num_traits::ToPrimitive;
use std::f32::consts::{PI, TAU};

struct Origin {
    object: usize,
    position: [f32; 3],
    basis: [[f32; 3]; 3],
    facing: f32,
    velocity: [f32; 3],
    color: Option<u32>,
}

impl Skirmish {
    /// Executes the represented original physical OCL nuggets in declaration order.
    /// # Errors
    /// Every named object must have its original physical module in the catalog.
    /// # Panics
    /// `source` must identify an object in this match.
    pub fn create_physical_ocl(
        &mut self,
        source: usize,
        list: &List,
    ) -> Result<Vec<usize>, &'static str> {
        self.create_physical_ocl_at(source, list, self.object_position(source))
    }

    pub(in crate::skirmish) fn create_physical_ocl_at(
        &mut self,
        source: usize,
        list: &List,
        point: [f32; 3],
    ) -> Result<Vec<usize>, &'static str> {
        if let Some(force) = &list.initial_force {
            self.apply_ocl_source_force(source, force);
        }
        let mut created = Vec::new();
        for nugget in &list.nuggets {
            if nugget.names.is_empty() {
                continue;
            }
            for _ in 0..nugget.count {
                let max = u32::try_from(nugget.names.len() - 1).expect("native OCL choice count");
                let choice = usize::try_from(self.random_between(max)).expect("native OCL choice");
                let name = if nugget.debris_mass.is_some() {
                    "GenericDebris"
                } else {
                    &nugget.names[choice]
                };
                let kind = self
                    .catalog
                    .iter()
                    .position(|unit| unit.name == name)
                    .ok_or("Physical OCL target is missing from the catalog")?;
                if self.catalog[kind].physical.is_none() {
                    return Err("Physical OCL target lacks its native physical modules");
                }
                let origin = point;
                let basis = self.entities[source].object_basis();
                let inherited = self.entities[source].physical.as_ref().map_or_else(
                    || self.entities[source].physics_velocity(),
                    |state| state.motion.velocity,
                );
                let facing = self.entities[source].facing;
                let owner = self.entities[source].owner;
                let color = if nugget.debris_mass.is_some() && nugget.draw.keep_color {
                    Some(
                        self.players[owner]
                            .color
                            .ok_or("Debris owner lacks configured PlayerColor")?,
                    )
                } else {
                    None
                };
                let id = self.new_object(kind, owner, true);
                self.entities[id].producer = Some(source);
                let origin = Origin {
                    object: source,
                    position: origin,
                    basis,
                    facing,
                    velocity: inherited,
                    color,
                };
                self.configure_physical_nugget(id, nugget, &nugget.names[choice], &origin);
                created.push(id);
            }
        }
        Ok(created)
    }

    fn apply_ocl_source_force(&mut self, source: usize, force: &super::RandomForce) {
        if self.object_physics_mass(source).is_none() {
            // Native asserts and returns without any RNG when getPhysics is null.
            return;
        }
        let impulse = calc_random_force(&mut self.random, force.pitch, force.magnitude);
        self.physical_force(source, impulse);
        let rates = std::array::from_fn(|_| {
            startup::random_real(&mut self.random, -force.spin_rate, force.spin_rate)
        });
        self.entities[source].set_physics_angular_rates(rates);
    }

    fn configure_debris_draw(
        &mut self,
        nugget: &Nugget,
        name: &str,
        color: Option<u32>,
    ) -> Option<DebrisDraw> {
        nugget.debris_mass?;
        let animations = if nugget.draw.animations.is_empty() {
            None
        } else {
            let maximum = u32::try_from(nugget.draw.animations.len() - 1)
                .expect("native debris animation count");
            let choice = usize::try_from(self.random_between(maximum))
                .expect("native debris animation choice");
            Some(nugget.draw.animations[choice].clone())
        };
        Some(DebrisDraw {
            model: name.to_owned(),
            animations,
            final_fx: nugget.draw.final_fx.clone(),
            color,
        })
    }

    fn configure_physical_nugget(
        &mut self,
        id: usize,
        nugget: &Nugget,
        name: &str,
        origin: &Origin,
    ) {
        let contained_mass = self.contained_physics_mass(id);
        let mut state = self.entities[id]
            .physical
            .take()
            .expect("physical creation");
        if let Some(range) = nugget.lifetime
            && !self.entities[id].behaviors.lifetime_die_frames.is_empty()
        {
            self.set_lifetime_range(id, 0, range);
        }
        state.particle.clone_from(&nugget.particle);
        if nugget.ignore_primary_obstacle {
            state.ignore_collisions_with = Some(origin.object);
        }
        state.draw = self.configure_debris_draw(nugget, name, origin.color);
        let offset = transform::vector(origin.basis, nugget.offset);
        let mut position = std::array::from_fn(|axis| origin.position[axis] + offset[axis]);
        let health = startup::random_real(&mut self.random, nugget.health[0], nugget.health[1]);
        let actor = &mut self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if unit.physical.as_ref().expect("physical rules").active_body {
            // OCL passes Real*100 into ActiveBody::setInitialHealth(Int), then
            // internalChangeHealth adds the difference without death callbacks.
            let percent = (health * 100.0).to_i32().expect("source health percent");
            let factor = percent.to_f32().expect("source health percent") / 100.0;
            let requested = factor * unit.initial_health;
            let delta = requested - actor.health;
            actor.health = (actor.health + delta).clamp(0.0, unit.max_health);
            if unit.body_kind.is_active() {
                actor.refresh_body_health();
            }
        }
        if nugget.disposition & INHERIT_VELOCITY != 0 {
            state
                .motion
                .force(origin.velocity, state.motion.mass + contained_mass);
        }
        if nugget.disposition & LIKE_EXISTING != 0 {
            state.motion.basis = origin.basis;
            state.motion.position = position;
        }
        if nugget.disposition & ON_GROUND != 0 {
            state.motion.basis = transform::yaw(startup::random_real(&mut self.random, 0.0, TAU));
            position[2] = self.ground_height([position[0], position[1]]);
            state.motion.position = position;
        }
        if nugget.disposition & (SEND_FLYING | SEND_UP | RANDOM_FORCE) != 0 {
            state.motion.position = position;
            if let Some(mass) = nugget.debris_mass {
                state.motion.mass = mass;
            }
            state.motion.extra_friction = nugget.extra_friction;
            state.bounce_sound.clone_from(&nugget.bounce_sound);
            state.motion.bouncing = true;
            let spin = nugget.spin_rate.unwrap_or((PI / 32.0) * nugget.intensity);
            let rates = nugget.angular_rates.map(|rate| {
                let rate = rate.unwrap_or(spin);
                startup::random_real(&mut self.random, -rate, rate)
            });
            let force = random_force(nugget, &mut self.random);
            state
                .motion
                .force(force, state.motion.mass + contained_mass);
            let facing = if nugget.orient_in_force_direction {
                force[1].atan2(force[0])
            } else {
                origin.facing
            };
            state.motion.basis = transform::yaw(facing);
            state.motion.rates = [rates[1], rates[2], rates[0]];
        }
        let actor = &mut self.entities[id];
        actor.position = [state.motion.position[0], state.motion.position[1]];
        actor.facing = state.motion.basis[1][0].atan2(state.motion.basis[0][0]);
        actor.transform =
            Some(crate::skirmish::physical_ocl::transform::Matrix::from_basis(state.motion.basis));
        actor.physical = Some(state);
        self.observe_ground_partition(id);
    }
}

impl super::Motion {
    pub(in crate::skirmish) fn force(&mut self, force: [f32; 3], mass: f32) {
        if force.iter().any(|value| value.is_nan()) {
            return;
        }
        let inverse = 1.0 / mass;
        for (acceleration, force) in self.acceleration.iter_mut().zip(force) {
            let value = force * inverse;
            *acceleration += value;
        }
    }
}

fn random_force(nugget: &Nugget, random: &mut [u32; 6]) -> [f32; 3] {
    if nugget.disposition & (SEND_FLYING | SEND_UP) != 0 {
        let flying = nugget.disposition & SEND_FLYING != 0;
        let horizontal = if flying { 4.0 } else { 2.0 } * nugget.intensity;
        let vertical = if flying { 3.0 } else { 4.0 } * nugget.intensity;
        return [
            startup::random_real(random, -horizontal, horizontal),
            startup::random_real(random, -horizontal, horizontal),
            startup::random_real(
                random,
                vertical * if flying { 0.33 } else { 0.75 },
                vertical,
            ),
        ];
    }
    calc_random_force(random, nugget.force_pitch, nugget.force_magnitude)
}

pub(in crate::skirmish) fn calc_random_force(
    random: &mut [u32; 6],
    pitch: [f32; 2],
    magnitude: [f32; 2],
) -> [f32; 3] {
    let angle = startup::random_real(random, 0.0, TAU);
    let pitch = startup::random_real(random, pitch[0], pitch[1]);
    let magnitude = startup::random_real(random, magnitude[0], magnitude[1]);
    [
        pitch.cos() * (angle.cos() * magnitude),
        pitch.cos() * (angle.sin() * magnitude),
        -(-pitch).sin() * magnitude,
    ]
}
