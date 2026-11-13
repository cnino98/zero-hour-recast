//! `PhysicsBehavior::onCollide`, including SH's direct collided-object weapon fixes.
use super::Skirmish;
use crate::{
    collision_geometry::{Geometry, Shape},
    skirmish::{DamageType, DeathType, UnitType, partition::Identity},
};
use num_traits::ToPrimitive;

pub struct CollisionRules {
    pub stiffness: f32,
    pub rubble_height: f32,
    pub allow_force: bool,
    /// Building then non-building; no temporary area weapon is fired in this profile.
    pub weapons: [Option<CrashWeapon>; 2],
}

pub struct CrashWeapon {
    pub damage: f32,
    pub damage_type: DamageType,
    pub death_type: DeathType,
    pub effects: [String; 4],
}

impl UnitType {
    #[must_use]
    pub fn collision_rules(&self) -> Option<&CollisionRules> {
        self.physical
            .as_ref()
            .map(|rules| &rules.collision)
            .or_else(|| self.live_physics.as_ref().map(|rules| &rules.collision))
    }

    pub(in crate::skirmish) fn immobile(&self) -> bool {
        self.kind_of.iter().any(|kind| kind == "IMMOBILE")
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn physical_contact(&mut self, id: usize, other: usize) {
        let actor = &self.entities[id];
        let target = &self.entities[other];
        let victim = &self.catalog[target.type_index];
        if !(actor.physical_present()
            || actor.live_physics_active()
            || (actor.jet_corpse() || actor.helicopter_corpse()))
            || actor.contained_by() == Some(other)
            || target.contained_by() == Some(id)
        {
            return;
        }
        if actor.parachute_rider() && target.parachute_rider() {
            return;
        }
        if actor.ai.ignored_producer == Some(other) {
            self.unmanned_contact(id, other);
            return;
        }
        if target.ai.ignored_producer == Some(id)
            || actor.physics_ignores(other)
            || target.physics_ignores(id)
            || (!victim.immobile() && victim.crush.physics_module.is_none())
        {
            return;
        }
        if self.check_physics_overlap(id, other) {
            return;
        }
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let victim = &self.catalog[self.entities[other].type_index];
        if !matches!(unit.ai.update, crate::skirmish::UnitAiUpdate::Absent)
            && !((actor.is_effectively_dead() || actor.parachute_rider()) && victim.immobile())
            && !self.process_ground_contact(id, other)
        {
            return;
        }
        self.resolve_physical_contact(id, Identity::Actor(other));
    }

    pub(in crate::skirmish) fn physical_map_contact(&mut self, id: usize, index: usize) {
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        // Noncatalog scenery has no AI. Its lack of AI rejects the ordinary
        // mover collision in processCollision, but not dead/parachuting movers.
        if !(actor.physical_present() || actor.live_physics_active())
            || actor.projectile_flight.is_some()
            || !unit
                .behavior_modules
                .iter()
                .any(|module| module.interfaces & 0x10 != 0 && module.family == "PhysicsBehavior")
            || (!matches!(unit.ai.update, crate::skirmish::UnitAiUpdate::Absent)
                && !actor.is_effectively_dead()
                && !actor.parachute_rider())
        {
            return;
        }
        self.resolve_physical_contact(id, Identity::Map(index));
    }

    fn resolve_physical_contact(&mut self, id: usize, other: Identity) {
        let unit = &self.catalog[self.entities[id].type_index];
        let position = self.object_position(id);
        let us = center(self.body_geometry(id), position);
        let (geometry, position) = match other {
            Identity::Actor(other) => (self.body_geometry(other), self.object_position(other)),
            Identity::Map(index) => {
                let object = &self.layout.world_objects[index];
                (&object.geometry, object.position)
            }
        };
        let them = center(geometry, position);
        let mut delta = std::array::from_fn::<_, 3, _>(|axis| them[axis] - us[axis]);
        let above = self.height_above_ground(id) > 0.0;
        if !above {
            delta[2] = 0.0;
        }
        let our_radius = radius(self.body_geometry(id), above);
        let radius = our_radius + radius(geometry, above);
        let squares = delta.map(|value| value * value);
        let distance_squared = squares[0] + squares[1] + squares[2];
        if distance_squared > radius * radius {
            return;
        }
        let rules = unit.collision_rules().expect("physical collision rules");
        let allow_force = rules.allow_force;
        let actor = &mut self.entities[id];
        if let Some(state) = &mut actor.physical {
            state.last_collidee = Some(other);
        } else if let Some(state) = &mut actor.live_physics {
            state.last_collidee = Some(other);
        }
        if !allow_force {
            return;
        }
        self.apply_physical_collision_force(id, other, delta, radius, our_radius, distance_squared);
    }

    fn apply_physical_collision_force(
        &mut self,
        id: usize,
        other: Identity,
        delta: [f32; 3],
        radius: f32,
        our_radius: f32,
        distance_squared: f32,
    ) {
        let rules = self.catalog[self.entities[id].type_index]
            .collision_rules()
            .expect("collision rules");
        let stiffness = rules.stiffness.clamp(0.01, 0.99);
        let rubble_height = rules.rubble_height;
        let immobile = match other {
            Identity::Actor(other) => self.catalog[self.entities[other].type_index].immobile(),
            Identity::Map(_) => true,
        };
        let position = self.object_position(id);
        let distance = distance_squared.sqrt();
        let overlap = radius - distance;
        let distance = distance.max(1.0);
        let destroyed = self.update_objects[self.entities[id].update_owner].destroy_requested;
        if immobile && !destroyed && self.entities[id].parachute_rider() {
            let mut carrier = id;
            while let Some(parent) = self.entities[carrier].contained_by() {
                carrier = parent;
            }
            let mut position = self.object_position(carrier);
            let bounce = our_radius * 0.1;
            position[0] -= bounce * delta[0] / distance;
            position[1] -= bounce * delta[1] / distance;
            self.set_object_position(carrier, position);
            let mut velocity = self.entities[carrier].physics_velocity();
            velocity[0] = 0.0;
            velocity[1] = 0.0;
            self.entities[carrier].set_aircraft_physics(position, velocity);
            if let Some(state) = &mut self.entities[carrier].live_physics {
                state.motion.velocity = velocity;
            }
            if let Some(state) = &mut self.entities[carrier].physical {
                state.motion.velocity = velocity;
            }
            return;
        }
        let factor = if immobile && !destroyed {
            let velocity = self.physical_velocity(id);
            let squares = velocity.map(|value| value * value);
            let speed = (squares[0] + squares[1] + squares[2])
                .sqrt()
                .max(1.0 / (30.0 * 5.0));
            let factor =
                -speed * self.object_physics_mass(id).expect("collision Physics") * stiffness;
            if delta[2] < 0.0 && position[2] >= rubble_height && self.crash_into_immobile(id, other)
            {
                return;
            }
            self.clear_physical_velocity(id);
            factor
        } else {
            -overlap.min(5.0)
        };
        self.physical_force(id, delta.map(|value| factor * value / distance));
    }

    fn crash_into_immobile(&mut self, id: usize, other: Identity) -> bool {
        let unit = &self.catalog[self.entities[id].type_index];
        let kinds = match other {
            Identity::Actor(other) => &self.catalog[self.entities[other].type_index].kind_of,
            Identity::Map(index) => &self.layout.world_objects[index].kind_of,
        };
        let building = kinds.iter().any(|kind| kind == "STRUCTURE");
        let vehicle = unit.kind_of.iter().any(|kind| kind == "VEHICLE");
        if vehicle
            && let Some(weapon) =
                &unit.collision_rules().expect("collision rules").weapons[usize::from(!building)]
        {
            let amount = weapon.damage;
            let damage_type = weapon.damage_type;
            let death = weapon.death_type;
            let effect = weapon.effects[usize::from(self.entities[id].veterancy)].clone();
            // CRC=0: SH 17/05/2026 and 19/04/2026 fixes target this object only.
            if let Identity::Actor(other) = other {
                self.apply_weapon_hit(other, id, amount, damage_type, death);
            }
            self.object_fx(id, effect);
        }
        if building {
            self.destroy_maintenance_object(id);
        }
        building
    }

    fn physical_velocity(&self, id: usize) -> [f32; 3] {
        self.entities[id].physical.as_ref().map_or_else(
            || self.entities[id].physics_velocity(),
            |state| state.motion.velocity,
        )
    }

    fn clear_physical_velocity(&mut self, id: usize) {
        if let Some(state) = &mut self.entities[id].physical {
            state.motion.velocity = [0.0; 3];
        } else if let Some(mut state) = self.entities[id].live_physics.take() {
            let actor = &mut self.entities[id];
            state.synchronize_death(actor, self.tick);
            state.motion.velocity = [0.0; 3];
            actor.set_death_physics(&state.motion, state.allows_fall());
            actor.set_aircraft_physics(state.motion.position, [0.0; 3]);
            actor.live_physics = Some(state);
        } else {
            self.clear_flight_velocity(id);
        }
    }

    pub(in crate::skirmish) fn physical_force(&mut self, id: usize, mut force: [f32; 3]) {
        let mass = self.object_physics_mass(id).expect("collision Physics");
        if self.entities[id].projectile_flight.is_some() {
            self.apply_projectile_force(id, force, mass);
            return;
        }
        let actor = &self.entities[id];
        let motive = actor
            .aircraft_physics_input(&self.catalog[actor.type_index])
            .map_or(actor.ai.motion.motive_until, |input| input.motive_until)
            > self.tick;
        if motive {
            let direction = [actor.facing.cos(), actor.facing.sin()];
            let lateral_x = force[0] * -direction[1];
            let lateral_y = force[1] * direction[0];
            let lateral = lateral_x + lateral_y;
            force[0] = lateral * -direction[1];
            force[1] = lateral * direction[0];
        }
        if let Some(state) = &mut self.entities[id].physical {
            state.motion.force(force, mass);
        } else if let Some(mut state) = self.entities[id].live_physics.take() {
            let actor = &mut self.entities[id];
            state.synchronize_death(actor, self.tick);
            state.motion.force(force, mass);
            actor.set_death_physics(&state.motion, state.allows_fall());
            actor.live_physics = Some(state);
        } else {
            self.apply_flight_force(id, force);
        }
        if !force.iter().any(|value| value.is_nan()) {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }
}

fn center(geometry: &Geometry, mut position: [f32; 3]) -> [f32; 3] {
    if !matches!(geometry.shape, Shape::Sphere) {
        let offset = geometry.height * 0.5;
        position[2] += offset;
    }
    position
}

fn radius(geometry: &Geometry, above: bool) -> f32 {
    match geometry.shape {
        Shape::Cylinder if above => geometry.major.max(geometry.height * 0.5),
        Shape::Sphere | Shape::Cylinder => geometry.major,
        Shape::Box => {
            let x = geometry.major * geometry.major;
            let y = geometry.minor * geometry.minor;
            let height = if above { geometry.height * 0.5 } else { 0.0 };
            let z = height * height;
            (x + y + z).sqrt()
        }
    }
}

impl crate::skirmish::Entity {
    pub(in crate::skirmish) fn physics_has_overlap(&self) -> bool {
        self.physical.as_ref().map_or_else(
            || {
                self.live_physics.as_ref().is_some_and(|state| {
                    state.current_overlap.is_some() || state.previous_overlap.is_some()
                })
            },
            |state| state.current_overlap.is_some() || state.previous_overlap.is_some(),
        )
    }

    fn add_physics_overlap(&mut self, other: usize) -> bool {
        if let Some(state) = &mut self.physical {
            state.current_overlap = Some(other);
            state.previous_overlap != Some(other)
        } else {
            let state = self
                .live_physics
                .as_mut()
                .expect("ordinary Physics overlap");
            state.current_overlap = Some(other);
            state.previous_overlap != Some(other)
        }
    }
}

impl Skirmish {
    fn check_physics_overlap(&mut self, id: usize, other: usize) -> bool {
        if self.entities[id]
            .physics_velocity()
            .iter()
            .all(|value| value.abs() < 0.01)
        {
            return false;
        }
        let crushing = self.can_crush_only(id, other);
        let crushed = self.can_crush_only(other, id);
        if crushing && crushed {
            return false;
        }
        if crushed {
            return true;
        }
        if !crushing {
            return false;
        }
        if self.entities[id].add_physics_overlap(other) {
            self.apply_weapon_hit(other, id, 0.0, DamageType::Crush, DeathType::Crushed);
        }
        let target = &self.entities[other];
        let front = target.damage_state.front_crushed;
        let back = target.damage_state.back_crushed;
        if !(front && back) {
            let actor = &self.entities[id];
            let direction = [actor.facing.cos(), actor.facing.sin()];
            let offset = self.body_geometry(other).major / 2.0;
            let target_direction = [target.facing.cos(), target.facing.sin()];
            let points = crush_points(actor.position, target.position, target_direction, offset);
            let point = if front {
                1
            } else if back {
                0
            } else {
                overlap_point(points, direction)
            };
            let delta = points[point];
            let projected_x = direction[0] * delta[0];
            let projected_y = direction[1] * delta[1];
            let dot = projected_x + projected_y;
            let squares = delta.map(|value| value * value);
            let distance = squares[0] + squares[1];
            // The unsuffixed C++ 2.25 literal promotes both multiplications to double.
            let limit = (2.25_f64 * f64::from(offset) * f64::from(offset))
                .to_f32()
                .expect("native crush distance");
            if dot < 0.0 && distance < limit {
                self.apply_weapon_hit(other, id, 999_999.0, DamageType::Crush, DeathType::Crushed);
            }
        }
        true
    }

    /// `CrushDie::crushLocationCheck`; masks are front=1, back=2, total=3.
    pub(in crate::skirmish) fn crush_death_location(
        &self,
        crusher: usize,
        victim: usize,
    ) -> Option<u8> {
        let actor = &self.entities[crusher];
        let target = &self.entities[victim];
        let front = target.damage_state.front_crushed;
        let back = target.damage_state.back_crushed;
        let points = crush_points(
            actor.position,
            target.position,
            [target.facing.cos(), target.facing.sin()],
            self.body_geometry(victim).major * 0.5,
        );
        let square = |point: [f32; 2]| {
            let squares = point.map(|value| value * value);
            squares[0] + squares[1]
        };
        let mut best = 99_999.0;
        let mut result = None;
        if !front && !back {
            result = Some(3);
            best = square(points[2]);
        }
        if !front && square(points[0]) < best {
            result = Some(if back { 3 } else { 1 });
            best = square(points[0]);
        }
        if !back && square(points[1]) < best {
            result = Some(if front { 3 } else { 2 });
        }
        result
    }
}

fn crush_points(
    origin: [f32; 2],
    victim: [f32; 2],
    direction: [f32; 2],
    offset: f32,
) -> [[f32; 2]; 3] {
    let offset = direction.map(|value| value * offset);
    [
        [
            (victim[0] + offset[0]) - origin[0],
            (victim[1] + offset[1]) - origin[1],
        ],
        [
            (victim[0] - offset[0]) - origin[0],
            (victim[1] - offset[1]) - origin[1],
        ],
        [victim[0] - origin[0], victim[1] - origin[1]],
    ]
}

fn overlap_point(points: [[f32; 2]; 3], direction: [f32; 2]) -> usize {
    let length = |point: [f32; 2]| {
        let squares = point.map(|value| value * value);
        (squares[0] + squares[1]).sqrt()
    };
    let perps = points.map(|point| {
        let projected_x = point[0] * direction[0];
        let projected_y = point[1] * direction[1];
        let ray = projected_x + projected_y;
        let along = direction.map(|component| ray * component);
        length([along[0] - point[0], along[1] - point[1]])
    });
    let equal = |a: usize, b: usize| (perps[a] - perps[b]).abs() <= 0.15;
    let shorter = |a: usize, b: usize| {
        if length(points[a]) < length(points[b]) {
            a
        } else {
            b
        }
    };
    if perps[0] <= perps[2] && perps[0] <= perps[1] {
        if equal(0, 2) {
            shorter(0, 2)
        } else if equal(0, 1) {
            shorter(0, 1)
        } else {
            0
        }
    } else if perps[1] <= perps[2] && perps[1] <= perps[0] {
        if equal(1, 2) {
            shorter(1, 2)
        } else if equal(1, 0) {
            shorter(1, 0)
        } else {
            1
        }
    } else if equal(2, 0) {
        shorter(2, 0)
    } else if equal(2, 1) {
        shorter(2, 1)
    } else {
        2
    }
}
