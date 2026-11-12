//! Native `DumbProjectileBehavior` and direct-weapon damage scheduling.
mod bezier;
mod binding;
mod events;
#[cfg(test)]
pub(in crate::skirmish) mod fixtures;
pub(in crate::skirmish) use events::ProjectileEvent;
mod jamming;
pub(super) use bezier::distance;
pub(super) mod missile;
mod physics;
mod shell_contact;
mod shell_physics;
mod terrain;
pub use missile::{MissileRetirement, MissileRules, ProjectilePhysics, ThrustRules};

use super::{Skirmish, firing::WeaponBinding};
pub(super) use terrain::Terrain;
pub use terrain::TerrainHeights;
use terrain::bounded_index;

struct ProjectileLaunch<'a> {
    transform: &'a LaunchTransform,
    destination: [f32; 3],
    speed: f32,
    frame: u32,
}

pub struct LaunchTransform {
    pub position: [f32; 3],
    pub axes: [[f32; 3]; 3],
}

impl LaunchTransform {
    pub const IDENTITY: Self = Self {
        position: [0.0; 3],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
}

pub struct WeaponMotion {
    pub turret_mounts: Option<[super::turret::Mount; 3]>,
    pub launches: [Vec<LaunchTransform>; 3],
    pub damage_thresholds: [f32; 2],
    pub shots_per_barrel: u32,
    pub projectile_type: Option<usize>,
    pub collide_mask: u16,
    pub shell_speed_scale: Option<ShellSpeedScale>,
}

impl WeaponMotion {
    pub(super) fn launch_bones(&self, health: f32, max_health: f32) -> &[LaunchTransform] {
        let ratio = health / max_health;
        let index = if ratio <= self.damage_thresholds[1] {
            2
        } else {
            usize::from(ratio <= self.damage_thresholds[0])
        };
        &self.launches[index]
    }
}

pub enum FlightRules {
    Shell(ShellRules),
    Missile(MissileRules),
}

pub struct ShellRules {
    pub detonate_calls_kill: bool,
    pub tumble: bool,
    pub orient_to_flight_path: bool,
    pub physics: Option<super::physical_ocl::MotionRules>,
    pub contact: Option<ShellContact>,
    pub heights: [f32; 2],
    pub indents: [f32; 2],
    pub adjust_per_frame: f32,
    pub lifespan: u32,
}

pub struct ShellSpeedScale {
    pub minimum_speed: f32,
    pub minimum_range: f32,
    pub maximum_range: f32,
}

impl ShellSpeedScale {
    fn speed(&self, start: [f32; 3], end: [f32; 3], maximum_speed: f32) -> f32 {
        // DumbProjectileBehavior uses the launched projectile's XY position and
        // the unmodified template range. The ratio is deliberately not clamped.
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let squared = [dx * dx, dy * dy];
        let range = (squared[0] + squared[1]).sqrt();
        let ratio = (range - self.minimum_range) / (self.maximum_range - self.minimum_range);
        let scaled = ratio * (maximum_speed - self.minimum_speed);
        scaled + self.minimum_speed
    }
}

pub struct ShellContact {
    pub geometry: crate::collision_geometry::Geometry,
    pub clear: Option<ClearBuilding>,
}

pub struct ClearBuilding {
    pub count: u32,
    pub allowed: Vec<String>,
    pub forbidden: Vec<String>,
}

pub(super) struct Shell {
    start: [f32; 3],
    end: [f32; 3],
    path: Vec<[f32; 3]>,
    step: usize,
    expires: u32,
    detonated: bool,
    launched: bool,
}

pub(super) enum Control {
    Shell(Shell),
    Missile(missile::Missile),
}

// One native Object transform and Physics state, shared by authored controls.
pub(super) struct Flight {
    pub(super) motion: Motion,
    pub(super) controls: Vec<Control>,
    pub(super) ignore_collisions_with: Option<usize>,
    hidden: bool,
    jammed: bool,
    status: u64,
    created_frame: u32,
}

pub(super) struct Motion {
    pub(super) position: [f32; 3],
    pub(super) axes: [[f32; 3]; 3],
    pub(super) velocity: [f32; 3],
    pub(super) acceleration: [f32; 3],
    pub(super) rates: [f32; 3],
    motive_expires: u32,
    force_wake: bool,
    airborne_last: Option<bool>,
    pub(in crate::skirmish) airborne_friction: bool,
    bouncing: bool,
}

pub(super) struct DirectHit {
    pub(super) deadline: u32,
    pub(super) radius: f32,
    pub(super) position: [f32; 3],
}

impl Motion {
    const fn new(position: [f32; 3]) -> Self {
        Self {
            position,
            axes: LaunchTransform::IDENTITY.axes,
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            rates: [0.0; 3],
            motive_expires: 0,
            force_wake: false,
            airborne_last: None,
            airborne_friction: false,
            bouncing: false,
        }
    }

    pub(in crate::skirmish) const fn reset_dynamic_physics(&mut self) {
        self.velocity = [0.0; 3];
        self.acceleration = [0.0; 3];
        self.rates = [0.0; 3];
        self.force_wake = false;
    }

    fn take_force_wake(&mut self) -> bool {
        std::mem::take(&mut self.force_wake)
    }

    fn inherit_velocity(&mut self, velocity: [f32; 3]) {
        for (current, inherited) in self.velocity.iter_mut().zip(velocity) {
            *current += inherited;
        }
    }
}

// Core ObjectStatusTypes.h: native Object status indices 5 and 39.
const NO_COLLISIONS: u64 = 1 << 5;
const MISSILE_KILLING_SELF: u64 = 1 << 39;

impl Flight {
    pub(in crate::skirmish) const fn no_collisions(&self) -> bool {
        self.status & NO_COLLISIONS != 0
    }
    const fn disable_collisions(&mut self) {
        self.status |= NO_COLLISIONS;
    }
    pub(in crate::skirmish) const fn killing_self(&self) -> bool {
        self.status & MISSILE_KILLING_SELF != 0
    }
    const fn mark_killing_self(&mut self) {
        self.status |= MISSILE_KILLING_SELF;
    }

    const fn new(position: [f32; 3], frame: u32) -> Self {
        Self {
            motion: Motion::new(position),
            controls: Vec::new(),
            ignore_collisions_with: None,
            hidden: false,
            jammed: false,
            status: 0,
            created_frame: frame,
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn projectile_index(&self, id: u64) -> Option<usize> {
        self.projectile_indices.get(&id).copied()
    }

    pub(in crate::skirmish) fn rebuild_projectile_indices(&mut self) {
        self.projectile_indices.clear();
        for (index, projectile) in self.projectiles.iter().enumerate() {
            self.projectile_indices.insert(projectile.id, index);
        }
    }

    pub(super) fn ability_line_of_sight(&self, id: usize, target: usize) -> bool {
        let position = |id| {
            let mut position = self.object_position(id);
            position[2] += self.body_geometry_bounds(id).max_height;
            position
        };
        self.projectile_terrain
            .as_ref()
            .is_none_or(|terrain| terrain.line_of_sight(position(id), position(target)))
    }

    pub(super) fn direct_damage_frames(
        &self,
        source: usize,
        binding: WeaponBinding,
        position: [f32; 3],
    ) -> f32 {
        // WeaponTemplate uses object positions, not launch bones/geometry centers.
        let speed = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired weapon")
            .speed
            / 30.0;
        distance(self.object_position(source), position) / speed
    }

    /// Source position including container placement and height above terrain.
    /// # Panics
    /// Requires an object index from this match.
    #[must_use]
    pub fn object_position(&self, id: usize) -> [f32; 3] {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if let Some(flight) = &actor.projectile_flight {
            return flight.motion.position;
        }
        [
            actor.position[0],
            actor.position[1],
            self.regular_bridge_height(id).unwrap_or_else(|| {
                if actor.contained_by().is_some() {
                    actor.containment.height
                } else {
                    actor
                        .corpse_height()
                        .or_else(|| actor.physical_height())
                        .or_else(|| actor.flight_height())
                        .or_else(|| actor.fire_height())
                        .unwrap_or_else(|| {
                            self.movement_layer_height(actor.layer, actor.position)
                                + actor.placed_height_offset.unwrap_or(unit.preferred_height)
                        })
                }
            }),
        ]
    }
    /// `Object::calculateHeightAboveTerrain` uses the current pathfinding layer.
    #[must_use]
    pub fn height_above_ground(&self, id: usize) -> f32 {
        let actor = &self.entities[id];
        self.object_position(id)[2] - self.movement_layer_height(actor.layer, actor.position)
    }

    /// Original raw corner samples, native height units, ten units between samples.
    /// # Errors
    /// Rejects malformed terrain dimensions, nonfinite values and invalid partition size.
    pub fn set_projectile_terrain(
        &mut self,
        width: u16,
        height: u16,
        origin: [f32; 2],
        heights: Vec<f32>,
        partition_cell_size: f32,
    ) -> Result<(), &'static str> {
        self.projectile_terrain = Some(Terrain::new(
            width,
            height,
            origin,
            heights,
            partition_cell_size,
            [
                f32::from(self.grid.width()) * 10.0,
                f32::from(self.grid.height()) * 10.0,
            ],
        )?);
        Ok(())
    }

    pub(super) fn launch_transform(
        &self,
        id: usize,
        binding: WeaponBinding,
        barrel: usize,
    ) -> LaunchTransform {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        if actor.contained_by().is_some() {
            let (sine, cosine) = actor.facing.sin_cos();
            return LaunchTransform {
                position: self.object_position(id),
                axes: [[cosine, sine, 0.0], [-sine, cosine, 0.0], [0.0, 0.0, 1.0]],
            };
        }
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("fired weapon");
        let local = weapon.motion.as_ref().and_then(|weapon| {
            weapon
                .launch_bones(actor.health, actor.max_health)
                .get(barrel)
        });
        let mut attachment = LaunchTransform {
            position: local.map_or([0.0; 3], |transform| transform.position),
            axes: local.map_or(LaunchTransform::IDENTITY.axes, |transform| transform.axes),
        };
        let turret = if unit
            .turret
            .as_ref()
            .is_some_and(|rules| rules.controlled_slots & (1 << binding.weapon_slot) != 0)
        {
            unit.turret.as_ref().map(|rules| (rules, &actor.ai.turret))
        } else {
            unit.secondary_turret
                .as_ref()
                .filter(|rules| rules.controlled_slots & (1 << binding.weapon_slot) != 0)
                .zip(actor.ai.secondary_turret.as_ref())
        };
        if let Some((rules, turret)) = turret {
            let ratio = actor.health / actor.max_health;
            let state = weapon.motion.as_ref().map_or(0, |weapon| {
                if ratio <= weapon.damage_thresholds[1] {
                    2
                } else {
                    usize::from(ratio <= weapon.damage_thresholds[0])
                }
            });
            let mounts = weapon
                .motion
                .as_ref()
                .and_then(|weapon| weapon.turret_mounts.as_ref())
                .unwrap_or(&rules.mounts);
            super::turret::launch(&mut attachment, &mounts[state], turret);
        }
        let local_position = attachment.position;
        let axes = attachment.axes;
        let cosine = actor.facing.cos();
        let sine = actor.facing.sin();
        let rotate = |point: [f32; 3]| {
            let forward_x = cosine * point[0];
            let side_x = sine * point[1];
            let forward_y = sine * point[0];
            let side_y = cosine * point[1];
            [forward_x - side_x, forward_y + side_y, point[2]]
        };
        let offset = rotate(local_position);
        LaunchTransform {
            position: [
                actor.position[0] + offset[0],
                actor.position[1] + offset[1],
                self.object_position(id)[2] + offset[2],
            ],
            axes: axes.map(rotate),
        }
    }

    pub(super) fn target_center(&self, id: usize) -> [f32; 3] {
        let actor = &self.entities[id];
        let center_height = self.body_geometry_bounds(id).center_height;
        [
            actor.position[0],
            actor.position[1],
            self.object_position(id)[2] + center_height,
        ]
    }

    /// Original ground-layer height in native map coordinates.
    #[must_use]
    pub fn ground_height(&self, position: [f32; 2]) -> f32 {
        self.projectile_terrain
            .as_ref()
            .map_or(0.0, |terrain| terrain.sample(position))
    }
}

impl super::Projectile {
    #[must_use]
    pub fn created_frame(&self, entities: &[super::Entity]) -> Option<u64> {
        let flight = entities[self.object?].projectile_flight.as_ref()?;
        Some(u64::from(flight.created_frame))
    }

    #[must_use]
    pub fn effect_transform(
        &self,
        entities: &[super::Entity],
    ) -> Option<([f32; 3], [[f32; 3]; 3])> {
        entities[self.object?].projectile_transform()
    }

    #[must_use]
    pub fn render_transform(
        &self,
        entities: &[super::Entity],
    ) -> Option<([f32; 3], [[f32; 3]; 3])> {
        let actor = &entities[self.object?];
        let flight = actor.projectile_flight.as_ref()?;
        (!flight.hidden && actor.exists()).then_some((flight.motion.position, flight.motion.axes))
    }
}

impl super::Entity {
    pub(in crate::skirmish) fn object_direction_3d(&self) -> [f32; 3] {
        if let Some(flight) = &self.projectile_flight {
            return flight.motion.axes[0];
        }
        self.physical_basis().map_or_else(
            || [self.facing.cos(), self.facing.sin(), 0.0],
            |basis| [basis[0][0], basis[1][0], basis[2][0]],
        )
    }

    #[must_use]
    pub const fn projectile_id(&self) -> Option<u64> {
        self.native_projectile
    }

    pub(in crate::skirmish) fn projectile_transform(&self) -> Option<([f32; 3], [[f32; 3]; 3])> {
        let motion = &self.projectile_flight.as_ref()?.motion;
        Some((motion.position, motion.axes))
    }

    pub(in crate::skirmish) fn projectile_velocity(&self) -> Option<[f32; 3]> {
        Some(self.projectile_flight.as_ref()?.motion.velocity)
    }

    pub(in crate::skirmish) fn set_projectile_basis(&mut self, basis: [[f32; 3]; 3]) {
        if let Some(flight) = &mut self.projectile_flight {
            flight.motion.axes =
                std::array::from_fn(|column| std::array::from_fn(|row| basis[row][column]));
        }
    }

    pub(in crate::skirmish) const fn set_projectile_position(&mut self, position: [f32; 3]) {
        if let Some(flight) = &mut self.projectile_flight {
            flight.motion.position = position;
        }
    }
}

impl Shell {
    const fn new() -> Self {
        Self {
            start: [0.0; 3],
            end: [0.0; 3],
            path: Vec::new(),
            step: 0,
            expires: 0,
            detonated: false,
            launched: false,
        }
    }

    fn fire(
        &mut self,
        motion: &mut Motion,
        launch: &ProjectileLaunch<'_>,
        speed_scale: Option<&ShellSpeedScale>,
        rules: &ShellRules,
        terrain: &Terrain,
        random: &mut [u32; 6],
    ) -> Option<()> {
        let ProjectileLaunch {
            transform: launch,
            destination: end,
            speed,
            frame,
        } = *launch;
        // DumbProjectile draws pitch, yaw, roll before constructing its path.
        let mut rates = [0.0; 3];
        if rules.tumble && rules.physics.is_some() {
            let limit = 1.0 / std::f32::consts::PI;
            for axis in [1, 2, 0] {
                rates[axis] = super::startup::random_real(random, -limit, limit);
            }
        }
        let start = launch.position;
        let speed = speed_scale.map_or(speed, |scale| scale.speed(start, end, speed));
        let points = control_points(start, end, rules, terrain)?;
        let count = bounded_index((bezier::length(points) / speed).ceil()).ok()?;
        let path = bezier::sample(points, count);
        self.launched = true;
        self.start = start;
        self.end = end;
        self.path = path;
        self.step = 0;
        self.expires = frame.wrapping_add(rules.lifespan);
        motion.position = start;
        motion.axes = launch.axes;
        motion.rates = rates;
        Some(())
    }

    pub(super) fn update(
        &mut self,
        motion: &mut Motion,
        target: Option<[f32; 3]>,
        rules: &ShellRules,
        terrain: &Terrain,
        frame: u32,
    ) -> bool {
        if (self.expires != 0 && frame >= self.expires) || self.step >= self.path.len() {
            return true;
        }
        if let Some(target) = target.filter(|_| rules.adjust_per_frame > 0.0) {
            let delta: [f32; 3] = std::array::from_fn(|axis| target[axis] - self.end[axis]);
            let squared = delta.map(|value| value * value);
            let distance_squared = squared[0] + squared[1] + squared[2];
            if distance_squared > 0.1 {
                let distance = distance_squared.sqrt();
                let travel = distance.min(rules.adjust_per_frame);
                let inverse = 1.0 / distance;
                for (end, delta) in self.end.iter_mut().zip(delta) {
                    let direction = delta * inverse;
                    let adjustment = travel * direction;
                    *end += adjustment;
                }
                let Some(points) = control_points(self.start, self.end, rules, terrain) else {
                    return true;
                };
                let Ok(count) = u16::try_from(self.path.len()) else {
                    return true;
                };
                self.path = bezier::sample(points, count);
            }
        }
        motion.position = self.path[self.step];
        if rules.orient_to_flight_path && !rules.tumble {
            let (previous, current) = if self.step > 0 {
                (self.path[self.step - 1], motion.position)
            } else if self.path.len() >= 2 {
                (self.path[0], self.path[1])
            } else {
                // Fixed upstream branch; retail attempted path[1] out of bounds.
                motion.position = self.end;
                (self.start, self.end)
            };
            motion.axes = flight_axes(std::array::from_fn(|axis| current[axis] - previous[axis]));
        }
        false
    }
}

fn flight_axes(direction: [f32; 3]) -> [[f32; 3]; 3] {
    // Matrix3D::buildTransformMatrix: yaw then pitch, with fixed yaw when vertical.
    let squared = direction.map(|value| value * value);
    let length = (squared[0] + squared[1] + squared[2]).sqrt();
    let inverse = if length > 0.0 { 1.0 / length } else { 0.0 };
    direction_axes(direction.map(|value| value * inverse))
}

fn direction_axes(native: [f32; 3]) -> [[f32; 3]; 3] {
    let horizontal_x = native[0] * native[0];
    let horizontal_y = native[1] * native[1];
    let horizontal = (horizontal_x + horizontal_y).sqrt();
    let (sine, cosine) = if horizontal > 0.0 {
        (native[1] / horizontal, native[0] / horizontal)
    } else {
        (0.0, 1.0)
    };
    [
        [cosine * horizontal, sine * horizontal, native[2]],
        [-sine, cosine, 0.0],
        [-cosine * native[2], -sine * native[2], horizontal],
    ]
}

fn control_points(
    start: [f32; 3],
    end: [f32; 3],
    rules: &ShellRules,
    terrain: &Terrain,
) -> Option<[[f32; 3]; 4]> {
    let high = terrain.highest(start, end)?.max(start[2]).max(end[2]);
    let distance = bezier::distance(start, end);
    let direction = if distance == 0.0 {
        [0.0; 3]
    } else {
        let inverse = 1.0 / distance;
        std::array::from_fn(|axis| (end[axis] - start[axis]) * inverse)
    };
    let middle = std::array::from_fn::<_, 2, _>(|index| {
        let amount = distance * rules.indents[index];
        let x = direction[0] * amount;
        let y = direction[1] * amount;
        [x + start[0], y + start[1], high + rules.heights[index]]
    });
    Some([start, middle[0], middle[1], end])
}

impl Skirmish {
    pub(super) fn projectile_damage_bonus(&self, projectile: &super::Projectile) -> f32 {
        self.projectile_bonus_factor(projectile, super::containment::bonuses::DAMAGE)
    }

    pub(super) fn projectile_bonus_factor(
        &self,
        projectile: &super::Projectile,
        field: usize,
    ) -> f32 {
        let weapon = self.catalog[projectile.weapon_type]
            .weapon_template(projectile.weapon_index)
            .expect("projectile weapon");
        let object = projectile.object.expect("native projectile Object");
        let flags = self.entities[object].weapon_bonus_flags() | self.passenger_bonus_flags(object);
        projectile.damage_state.factor(weapon, flags, field)
    }
}

#[cfg(test)]
mod tests {
    use super::{LaunchTransform, Motion, Shell, ShellRules, ShellSpeedScale, Terrain};

    #[test]
    fn firebase_shell_speed_matches_pinned_cpp_float_bits() {
        let scale = ShellSpeedScale {
            minimum_speed: 75.0 * (1.0 / 30.0),
            minimum_range: 47.5,
            maximum_range: 275.0,
        };
        // Extracted pinned C++: output/firebase-speed-reference.log.
        for (range, bits) in [
            (0.0, 0x3f6f_1ef2),
            (47.5, 0x4020_0001),
            (161.25, 0x40c8_0002),
            (275.0, 0x4120_0001),
            (388.75, 0x415c_0002),
        ] {
            assert_eq!(
                scale
                    .speed([0.0; 3], [range, 0.0, 100.0], 300.0 * (1.0 / 30.0))
                    .to_bits(),
                bits
            );
        }
    }

    #[test]
    fn scaled_shell_uses_projectile_xy_unmodified_range_and_unclamped_ratio() {
        let scale = ShellSpeedScale {
            minimum_speed: 2.5,
            minimum_range: 47.5,
            maximum_range: 275.0,
        };
        // Fire Base's source minimum range is 50 minus a quarter path cell.
        for (range, expected) in [(47.5, 2.5_f32), (161.25, 6.25), (275.0, 10.0)] {
            assert_eq!(
                scale
                    .speed([30.0, 20.0, 500.0], [30.0 + range, 20.0, 0.0], 10.0)
                    .to_bits(),
                expected.to_bits()
            );
        }
        let scale = ShellSpeedScale {
            minimum_speed: 5.0,
            minimum_range: 150.0,
            maximum_range: 350.0,
        };
        for (range, expected) in [(50.0, 0.0_f32), (450.0, 20.0)] {
            assert_eq!(
                scale.speed([0.0; 3], [range, 0.0, 500.0], 15.0).to_bits(),
                expected.to_bits()
            );
        }
    }

    #[test]
    fn scaled_shell_flight_duration_uses_launch_distance() {
        let terrain = Terrain::new(32, 32, [0.0; 2], vec![0.0; 1024], 40.0, [300.0; 2]).unwrap();
        let scale = ShellSpeedScale {
            minimum_speed: 2.5,
            minimum_range: 47.5,
            maximum_range: 275.0,
        };
        let launch = LaunchTransform {
            position: [20.0, 20.0, 10.0],
            ..LaunchTransform::IDENTITY
        };
        let end = [181.25, 20.0, 5.0];
        let count = |speed, scale| {
            let mut shell = Shell::new();
            let mut motion = Motion::new(launch.position);
            shell
                .fire(
                    &mut motion,
                    &crate::skirmish::projectiles::ProjectileLaunch {
                        transform: &launch,
                        destination: end,
                        speed,
                        frame: 0,
                    },
                    scale,
                    &rules(),
                    &terrain,
                    &mut crate::skirmish::startup::seed_random(0),
                )
                .unwrap();
            shell.path.len()
        };
        assert_eq!(count(10.0, Some(&scale)), count(6.25, None));
        assert!(count(10.0, Some(&scale)) > count(10.0, None));
    }

    fn rules() -> ShellRules {
        ShellRules {
            detonate_calls_kill: false,
            contact: None,
            heights: [10.0; 2],
            indents: [0.5, 0.9],
            adjust_per_frame: 100.0 / 30.0,
            lifespan: 300,
            tumble: false,
            orient_to_flight_path: true,
            physics: None,
        }
    }

    #[test]
    fn one_step_shell_uses_fixed_endpoint_and_detonates_next_update() {
        let terrain = Terrain::new(12, 12, [0.0; 2], vec![0.0; 144], 40.0, [100.0; 2]).unwrap();
        let end = [21.0, 20.0, 5.0];
        let rules = rules();
        let launch_axes = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]];
        let mut shell = Shell::new();
        let mut motion = Motion::new([0.0; 3]);
        shell
            .fire(
                &mut motion,
                &crate::skirmish::projectiles::ProjectileLaunch {
                    transform: &LaunchTransform {
                        position: [20.0, 20.0, 5.0],
                        axes: launch_axes,
                    },
                    destination: end,
                    speed: 1000.0,
                    frame: 0,
                },
                None,
                &rules,
                &terrain,
                &mut crate::skirmish::startup::seed_random(0),
            )
            .unwrap();
        assert_eq!(shell.path.len(), 1);
        assert_eq!(
            motion.axes.map(|axis| axis.map(f32::to_bits)),
            launch_axes.map(|axis| axis.map(f32::to_bits))
        );
        assert!(!shell.update(&mut motion, None, &rules, &terrain, 1));
        for (axis, expected) in motion
            .axes
            .iter()
            .flatten()
            .zip(LaunchTransform::IDENTITY.axes.iter().flatten())
        {
            assert!((axis - expected).abs() < f32::EPSILON);
        }
        assert_eq!(motion.position.map(f32::to_bits), end.map(f32::to_bits));
        shell.step += 1;
        assert!(shell.update(&mut motion, None, &rules, &terrain, 2));
    }

    #[test]
    fn moving_target_adjustment_keeps_original_flight_duration() {
        let terrain = Terrain::new(32, 32, [0.0; 2], vec![0.0; 1024], 40.0, [300.0; 2]).unwrap();
        let rules = rules();
        let mut shell = Shell::new();
        let mut motion = Motion::new([0.0; 3]);
        shell
            .fire(
                &mut motion,
                &crate::skirmish::projectiles::ProjectileLaunch {
                    transform: &LaunchTransform {
                        position: [20.0, 20.0, 10.0],
                        ..LaunchTransform::IDENTITY
                    },
                    destination: [200.0, 20.0, 5.0],
                    speed: 400.0 / 30.0,
                    frame: 0,
                },
                None,
                &rules,
                &terrain,
                &mut crate::skirmish::startup::seed_random(0),
            )
            .unwrap();
        let count = shell.path.len();
        assert!(!shell.update(&mut motion, Some([200.0, 80.0, 5.0]), &rules, &terrain, 1));
        assert_eq!(shell.path.len(), count);
        assert!((shell.end[1] - (20.0 + 100.0 / 30.0)).abs() < 0.0001);
    }
}
