//! Physical `GenericObjectCreationNugget` objects and their original body modules.
//! SH 2bfaea60: ObjectCreationList.cpp, PhysicsUpdate.cpp and SlowDeathBehavior.cpp.
use super::{Entity, Skirmish, partition::Identity, startup};
use num_traits::ToPrimitive;

mod collision;
mod creation;
pub(in crate::skirmish) use creation::calc_random_force as random_force;
mod live;
pub(super) mod physics;
pub use live::LiveRules;
pub(super) use live::LiveState;
pub mod transform;
pub use collision::{CollisionRules, CrashWeapon};

#[cfg(test)]
mod tests;

pub struct Rules {
    pub motion: MotionRules,
    pub kill_when_resting: bool,
    pub active_body: bool,
    pub collision: CollisionRules,
    pub death: Death,
}

pub struct MotionRules {
    pub center_of_mass_offset: f32,
    pub mass: f32,
    pub gravity: f32,
    pub stiffness: f32,
    pub forward_friction: f32,
    pub lateral_friction: f32,
    pub aerodynamic_friction: f32,
    pub angular_factor: f32,
    pub allow_bouncing: bool,
}

impl super::UnitType {
    pub(in crate::skirmish) fn physics_motion_rules(&self) -> Option<&MotionRules> {
        self.physical
            .as_ref()
            .map(|rules| &rules.motion)
            .or_else(|| self.live_physics.as_ref().map(|rules| &rules.motion))
    }
}

pub enum Death {
    Slow(SlowDeath),
    Instant,
}

pub struct SlowDeath {
    pub probability: u32,
    pub sink_delay: [u32; 2],
    pub destruction_delay: [u32; 2],
    pub sink_rate: f32,
}

/// Lists are copied out of immutable template data before callbacks create objects.
#[derive(Clone)]
pub struct List {
    pub name: String,
    pub initial_force: Option<RandomForce>,
    pub nuggets: Vec<Nugget>,
}

#[derive(Clone)]
pub struct Nugget {
    pub names: Vec<String>,
    pub debris_mass: Option<f32>,
    pub count: u32,
    pub ignore_primary_obstacle: bool,
    pub orient_in_force_direction: bool,
    pub draw: DebrisDrawRules,
    pub offset: [f32; 3],
    pub disposition: u16,
    pub intensity: f32,
    pub spin_rate: Option<f32>,
    /// Native overrides are yaw, roll, pitch, in this RNG order.
    pub angular_rates: [Option<f32>; 3],
    pub extra_friction: f32,
    pub bounce_sound: String,
    pub particle: String,
    pub force_magnitude: [f32; 2],
    pub force_pitch: [f32; 2],
    pub health: [f32; 2],
    pub lifetime: Option<[u32; 2]>,
}

/// Native `ApplyRandomForce` nugget, before object creation in admitted lists.
#[derive(Clone)]
pub struct RandomForce {
    pub magnitude: [f32; 2],
    pub pitch: [f32; 2],
    pub spin_rate: f32,
}

/// Authored `W3DDebrisDraw` setup, selected by the game-logic OCL callback.
#[derive(Clone)]
pub struct DebrisDrawRules {
    pub animations: Vec<[String; 3]>,
    pub final_fx: String,
    pub keep_color: bool,
}

/// Model/animation/color names captured by the native debris draw interface.
pub struct DebrisDraw {
    pub model: String,
    pub animations: Option<[String; 3]>,
    pub final_fx: String,
    pub color: Option<u32>,
}

pub const LIKE_EXISTING: u16 = 1;
pub const ON_GROUND: u16 = 2;
pub const INHERIT_VELOCITY: u16 = 128;
pub const SEND_FLYING: u16 = 4;
pub const SEND_UP: u16 = 8;
pub const SEND_OUT: u16 = 16;
pub const RANDOM_FORCE: u16 = 32;

pub fn lists(unit: &super::UnitType) -> impl Iterator<Item = &List> {
    let jets = unit
        .jet_death_rules()
        .flat_map(|jet| &jet.phases)
        .filter_map(|phase| phase.objects.as_ref());
    let deaths =
        unit.death_lifecycle
            .iter()
            .flat_map(|rules| &rules.slow)
            .flat_map(|death| death.objects.iter().flatten())
            .filter_map(|spawn| match spawn {
                super::death_lifecycle::Spawn::Physical(list) => Some(list),
                super::death_lifecycle::Spawn::Object { .. }
                | super::death_lifecycle::Spawn::None => None,
            });
    let helicopters = unit
        .helicopter_death_rules()
        .flat_map(|rules| &rules.phases)
        .filter_map(|phase| phase.objects.as_ref());
    let instant = unit
        .behavior
        .instant_death
        .iter()
        .flat_map(|rules| &rules.objects)
        .filter_map(Option::as_ref);
    let on_die = unit
        .behavior
        .create_object_die
        .iter()
        .flat_map(|rules| &rules.actions)
        .filter_map(|action| match action {
            super::create_object_die::Action::Physical(list) => Some(list),
            super::create_object_die::Action::Objects(_) => None,
        });
    let bus = unit
        .death_lifecycle
        .iter()
        .flat_map(|rules| &rules.slow)
        .filter_map(|rule| match &rule.derived {
            super::death_lifecycle::Derived::BattleBus(bus) => Some(bus.as_ref()),
            _ => None,
        })
        .flat_map(|rules| [&rules.start_ocl, &rules.ground_ocl])
        .filter_map(Option::as_ref);
    jets.chain(
        unit.behavior
            .fire_spread
            .iter()
            .filter_map(|rules| rules.embers.as_ref()),
    )
    .chain(deaths)
    .chain(helicopters)
    .chain(instant)
    .chain(on_die)
    .chain(bus)
    .chain(
        unit.structure_death
            .iter()
            .flat_map(super::structure_death::Rules::physical_lists),
    )
}

pub(super) struct State {
    pub(in crate::skirmish) motion: Motion,
    bounce_sound: String,
    particle: String,
    was_airborne: Option<bool>,
    phase: Phase,
    draw: Option<DebrisDraw>,
    physics_frame: Option<u64>,
    normal_frame: Option<u64>,
    last_collidee: Option<Identity>,
    ignore_collisions_with: Option<usize>,
    current_overlap: Option<usize>,
    previous_overlap: Option<usize>,
}

pub(super) struct Motion {
    pub(super) position: [f32; 3],
    pub(super) basis: [[f32; 3]; 3],
    pub(super) velocity: [f32; 3],
    pub(super) acceleration: [f32; 3],
    pub(super) rates: [f32; 3],
    pub(super) mass: f32,
    pub(super) extra_friction: f32,
    pub(super) bouncing: bool,
}

enum Phase {
    Living,
    Dying { sink: u64, destruction: u64 },
}

impl Entity {
    /// The native row-major object basis, including pitch, roll and matrix drift.
    #[must_use]
    pub fn object_basis(&self) -> [[f32; 3]; 3] {
        self.projectile_transform()
            .map(|(_, axes)| {
                std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row]))
            })
            .or_else(|| self.physical_basis())
            .unwrap_or_else(|| transform::yaw(self.facing))
    }

    pub(in crate::skirmish) fn set_object_basis(&mut self, basis: [[f32; 3]; 3]) {
        self.facing = basis[1][0].atan2(basis[0][0]);
        self.transform = Some(transform::Matrix::from_basis(basis));
        if let Some(state) = &mut self.physical {
            state.motion.basis = basis;
        }
        if let Some(state) = &mut self.live_physics {
            state.motion.basis = basis;
        }
        self.set_projectile_basis(basis);
        self.set_structure_basis(basis);
    }
    pub(in crate::skirmish) fn set_physical_position(&mut self, position: [f32; 3]) {
        if let Some(state) = &mut self.physical {
            state.motion.position = position;
        }
        self.set_live_physics_position(position);
    }

    #[must_use]
    pub const fn physical_present(&self) -> bool {
        !self.masked() && self.physical.is_some()
    }

    #[must_use]
    pub fn physical_height(&self) -> Option<f32> {
        self.physical
            .as_ref()
            .map(|state| state.motion.position[2])
            .or_else(|| self.live_height())
    }

    #[must_use]
    pub fn physical_basis(&self) -> Option<[[f32; 3]; 3]> {
        self.physical
            .as_ref()
            .map(|state| state.motion.basis)
            .or_else(|| self.live_basis())
            .or_else(|| self.structure_basis())
            .or_else(|| self.transform.as_ref().map(|matrix| matrix.basis))
    }

    #[must_use]
    pub fn physical_particle(&self) -> Option<&str> {
        self.physical
            .as_ref()
            .map(|state| state.particle.as_str())
            .filter(|name| !name.is_empty())
    }

    #[must_use]
    pub fn physical_model(&self) -> Option<&str> {
        Some(&self.physical.as_ref()?.draw.as_ref()?.model)
    }

    #[must_use]
    pub fn debris_draw(&self) -> Option<&DebrisDraw> {
        self.physical.as_ref()?.draw.as_ref()
    }
}

impl Skirmish {
    /// Native `Thing::getTransformMatrix`, with the object's current world position.
    /// # Panics
    /// Requires an object index from this match.
    #[must_use]
    pub fn object_transform(&self, id: usize) -> ([f32; 3], [[f32; 3]; 3]) {
        let actor = &self.entities[id];
        let position = actor
            .transform
            .as_ref()
            .and_then(|matrix| matrix.translation)
            .unwrap_or_else(|| self.object_position(id));
        (position, actor.object_basis())
    }

    /// `Thing::setTransformMatrix` retains every basis component before notifying
    /// drawable, containment, partition and visibility through the position seam.
    pub(in crate::skirmish) fn set_object_transform(
        &mut self,
        id: usize,
        position: [f32; 3],
        basis: [[f32; 3]; 3],
    ) {
        self.entities[id].set_object_basis(basis);
        self.write_object_position(id, position);
        self.note_drawable_transform(id, true);
        self.update_position_claim(id);
        self.observe_ground_partition(id);
    }

    /// `Thing::setOrientation`, including its distinct cached position on slopes.
    pub(in crate::skirmish) fn set_object_orientation(&mut self, id: usize, angle: f32) {
        let terrain = transform::TerrainContext {
            terrain: self.projectile_terrain.as_ref(),
            layers: &self.movement_terrain,
            height: self.object_transform(id).0[2],
        };
        let actor = &mut self.entities[id];
        terrain.orient(actor, &self.catalog[actor.type_index], angle);
        self.note_drawable_transform(id, true);
        self.update_position_claim(id);
        self.observe_ground_partition(id);
    }

    pub(in crate::skirmish) fn terrain_aligned_transform(
        &self,
        position: [f32; 3],
        angle: f32,
    ) -> ([f32; 3], [[f32; 3]; 3]) {
        transform::TerrainContext {
            terrain: self.projectile_terrain.as_ref(),
            layers: &self.movement_terrain,
            height: position[2],
        }
        .aligned(position, angle)
    }

    pub(super) fn initialize_physical_object(&mut self, id: usize) {
        let actor = &self.entities[id];
        let Some(rules) = &self.catalog[actor.type_index].physical else {
            return;
        };
        let position = self.object_position(id);
        let actor = &mut self.entities[id];
        actor.physical = Some(Box::new(State {
            motion: Motion {
                position,
                basis: actor.object_basis(),
                velocity: [0.0; 3],
                acceleration: [0.0; 3],
                rates: [0.0; 3],
                mass: rules.motion.mass,
                extra_friction: 0.0,
                bouncing: rules.motion.allow_bouncing,
            },
            bounce_sound: String::new(),
            particle: String::new(),
            was_airborne: None,
            phase: Phase::Living,
            draw: None,
            // PhysicsBehavior's SLEEPY_PHYSICS constructor requests NONE=1.
            // registerObject preserves that wake frame in the live update heap.
            physics_frame: Some(self.tick),
            normal_frame: None,
            last_collidee: None,
            ignore_collisions_with: None,
            current_overlap: None,
            previous_overlap: None,
        }));
    }

    fn kill_physical_object(&mut self, id: usize) {
        self.kill_object(id, super::DeathType::Normal);
    }

    pub(super) fn begin_physical_death(&mut self, id: usize) {
        if self.registered_slow_death(id) {
            return;
        }
        let actor = &self.entities[id];
        let Some(state) = &actor.physical else { return };
        if !matches!(state.phase, Phase::Living) {
            return;
        }
        let death = &self.catalog[actor.type_index]
            .physical
            .as_ref()
            .expect("physical rules")
            .death;
        let Death::Slow(rule) = death else {
            return;
        };
        // DieMux chooses even a single applicable module, followed by all three
        // native SlowDeath timing draws even when the corresponding FX are empty.
        startup::random_between(&mut self.random, rule.probability - 1);
        let sink =
            rule.sink_delay[0] + startup::random_between(&mut self.random, rule.sink_delay[1]);
        let destruction = rule.destruction_delay[0]
            + startup::random_between(&mut self.random, rule.destruction_delay[1]);
        let duration = destruction.to_f32().expect("source destruction frames");
        let low = (0.35 * duration).to_u32().expect("midpoint low");
        let high = (0.65 * duration).to_u32().expect("midpoint high");
        startup::random_between(&mut self.random, high - low);
        let state = self.entities[id].physical.as_mut().expect("physical state");
        state.phase = Phase::Dying {
            sink: self.tick + u64::from(sink),
            destruction: self.tick + u64::from(destruction),
        };
    }

    pub(super) fn update_physical_slow_death(&mut self, id: usize) {
        if self.registered_slow_death(id) {
            return;
        }
        let actor = &self.entities[id];
        let Some(state) = &actor.physical else { return };
        if actor.masked() || state.normal_frame == Some(self.tick) {
            return;
        }
        self.entities[id]
            .physical
            .as_mut()
            .expect("physical state")
            .normal_frame = Some(self.tick);
        let actor = &self.entities[id];
        let state = actor.physical.as_ref().expect("physical state");
        if let Phase::Dying { sink, destruction } = state.phase {
            let Death::Slow(rule) = &self.catalog[actor.type_index]
                .physical
                .as_ref()
                .expect("physical rules")
                .death
            else {
                unreachable!("slow physical death state requires its module")
            };
            let sink_rate = rule.sink_rate;
            if self.tick >= sink && sink_rate > 0.0 {
                self.set_disabled_held(id);
                self.entities[id]
                    .physical
                    .as_mut()
                    .expect("physical state")
                    .motion
                    .position[2] -= sink_rate;
            }
            if self.tick >= destruction {
                self.destroy_maintenance_object(id);
            }
        }
    }
}

impl Skirmish {
    /// `PhysicsBehavior::getMass` and `OpenContain::getContainedItemsMass`.
    pub(in crate::skirmish) fn object_physics_mass(&self, id: usize) -> Option<f32> {
        let actor = &self.entities[id];
        let base = if let Some(state) = &actor.physical {
            state.motion.mass
        } else if let Some(state) = &actor.live_physics {
            state.motion.mass
        } else if self.registered_physics(id) {
            self.catalog[actor.type_index]
                .live_physics
                .as_ref()?
                .motion
                .mass
        } else {
            return None;
        };
        Some(base + self.contained_physics_mass(id))
    }

    fn contained_physics_mass(&self, id: usize) -> f32 {
        let mut mass = 0.0;
        if self.catalog[self.entities[id].type_index].contain.is_some() {
            let contain = &self.entities[id].containment;
            // The native own list excludes Helix's portable rider and does not
            // redirect through an Overlord transport or a tunnel network.
            for &rider in &contain.passengers {
                if let Some(rider_mass) = self.object_physics_mass(rider) {
                    mass += rider_mass;
                }
            }
        }
        mass
    }
}
