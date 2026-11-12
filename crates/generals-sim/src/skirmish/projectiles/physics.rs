//! `PhysicsUpdate` PHYSICS callback for a common Object's projectile motion.
use super::{Control, FlightRules, Motion};
use crate::skirmish::{
    Skirmish,
    module_updates::FOREVER,
    physical_ocl::{self, MotionRules},
};

impl Motion {
    pub(in crate::skirmish) const fn allow_ocl_bouncing(&mut self) {
        // No admitted projectile producer sets nonzero extra friction.
        self.bouncing = true;
    }

    fn hold(&mut self, ground: f32) -> bool {
        let airborne = self.position[2] > ground;
        let was_airborne = self.airborne_last.unwrap_or(airborne);
        // PhysicsBehavior clears forces even while HELD; existing velocity and
        // rotation survive, and a carrier may have moved the object onto ground.
        self.acceleration = [0.0; 3];
        self.airborne_last = Some(airborne);
        was_airborne && !airborne
    }

    pub(in crate::skirmish::projectiles) fn integrate_shell(
        &mut self,
        rules: &MotionRules,
        frame: u32,
        height: impl Fn([f32; 2]) -> f32,
        terrain_height: impl Fn([f32; 2]) -> f32,
    ) -> bool {
        let mut physical = physical_ocl::Motion {
            position: self.position,
            basis: transpose(self.axes),
            velocity: self.velocity,
            acceleration: self.acceleration,
            rates: self.rates,
            mass: rules.mass,
            extra_friction: 0.0,
            bouncing: self.bouncing,
        };
        let ground = terrain_height([physical.position[0], physical.position[1]]);
        let was_airborne = self.airborne_last.unwrap_or_else(|| {
            self.position[2] > terrain_height([self.position[0], self.position[1]])
        });
        let old_height = physical.position[2];
        let height_above_ground = old_height - ground;
        let old_basis = physical.basis;
        physical_ocl::physics::integrate(
            &mut physical,
            rules,
            height_above_ground,
            self.airborne_friction,
            false,
            self.motive_expires > frame,
            rules.mass,
        );
        let ground = height([physical.position[0], physical.position[1]]);
        let bounce = physical.bounce(rules, old_height, ground, old_basis, rules.mass);
        if physical.position[2] <= ground {
            physical.velocity[2] += ground - physical.position[2];
            physical.velocity[2] = physical.velocity[2].min(0.0);
            physical.position[2] = ground;
        }
        physical.acceleration = [0.0; 3];
        if bounce > 0.0 && physical.bouncing {
            physical.force([0.0, 0.0, bounce], rules.mass);
        }
        let airborne =
            physical.position[2] > terrain_height([physical.position[0], physical.position[1]]);
        self.position = physical.position;
        self.axes = transpose(physical.basis);
        self.velocity = physical.velocity;
        self.acceleration = physical.acceleration;
        self.rates = physical.rates;
        self.bouncing = physical.bouncing;
        self.airborne_last = Some(airborne);
        was_airborne && !airborne
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn apply_projectile_force(
        &mut self,
        id: usize,
        force: [f32; 3],
        mass: f32,
    ) {
        let now = self.update_frame();
        self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("projectile Physics")
            .motion
            .apply_force(force, mass, now);
        if !force.iter().any(|value| value.is_nan()) {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }

    pub(in crate::skirmish) fn apply_projectile_motive_force(
        &mut self,
        id: usize,
        force: [f32; 3],
        mass: f32,
        frame: u32,
    ) {
        let motion = &mut self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("projectile Physics")
            .motion;
        motion.motive_expires = 0;
        let valid = !force.iter().any(|value| value.is_nan());
        if valid {
            let inverse = 1.0 / mass;
            for (acceleration, force) in motion.acceleration.iter_mut().zip(force) {
                let change = force * inverse;
                *acceleration += change;
            }
        }
        motion.motive_expires = frame.wrapping_add(10);
        if valid {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }

    pub(in crate::skirmish) fn construct_projectile_physics(&mut self, id: usize) {
        self.ensure_projectile_motion(id);
        let rules = self.catalog[self.entities[id].type_index]
            .live_physics
            .as_ref()
            .expect("admitted PhysicsBehavior instance data");
        self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("projectile motion")
            .motion
            .bouncing = rules.motion.allow_bouncing;
    }

    pub(in crate::skirmish) fn update_projectile_physics(&mut self, id: usize) -> i32 {
        let now = self.update_frame();
        let held = self.object_disabled_mask(id) & (1 << 3) != 0;
        let layer = self.entities[id].layer;
        let kind = self.entities[id].type_index;
        let terrain = &self.projectile_terrain;
        let movement = &self.movement_terrain;
        let terrain_height = |point| {
            terrain
                .as_ref()
                .map_or(0.0, |terrain| terrain.sample(point))
        };
        let height = |point| {
            movement
                .bridge_height(layer, point)
                .unwrap_or_else(|| terrain_height(point))
        };
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("admitted projectile Physics");
        let landed = if held {
            flight.motion.hold(height([
                flight.motion.position[0],
                flight.motion.position[1],
            ]))
        } else {
            match (&flight.controls[0], &self.catalog[kind].projectile_rules[0]) {
                (Control::Shell(_), FlightRules::Shell(_)) => flight.motion.integrate_shell(
                    &self.catalog[kind]
                        .live_physics
                        .as_ref()
                        .expect("PhysicsBehavior data")
                        .motion,
                    now,
                    height,
                    height,
                ),
                (Control::Missile(missile), FlightRules::Missile(rules)) => {
                    flight.motion.integrate(
                        &rules.physics,
                        now,
                        missile.physics_braking(),
                        height,
                        height,
                    )
                }
                _ => unreachable!("first projectile interface matches authored data"),
            }
        };
        self.synchronize_projectile_transform(id);
        if landed {
            self.object_collide(id, None);
        }
        self.projectile_physics_sleep(id)
    }

    pub(in crate::skirmish) fn projectile_physics_sleep(&self, id: usize) -> i32 {
        let now = self.update_frame();
        let layer = self.entities[id].layer;
        let motion = &self.entities[id]
            .projectile_flight
            .as_ref()
            .expect("deferred Object destruction")
            .motion;
        // A first ProjectileUpdateInterface handles every collision before Physics
        // can write an overlap ID. Both native overlap fields therefore remain INVALID.
        let asleep = motion.airborne_last.is_some()
            && motion.velocity.iter().all(|&value| value == 0.0)
            && motion.acceleration.iter().all(|&value| value == 0.0)
            && motion.rates.iter().all(|&value| value == 0.0)
            && motion.motive_expires <= now
            && layer == 1
            && motion.position[2] <= self.ground_height(self.entities[id].position);
        if asleep {
            i32::from_ne_bytes(FOREVER.to_ne_bytes())
        } else {
            1
        }
    }
}

fn transpose(axes: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row]))
}

#[cfg(test)]
mod tests {
    use super::super::Motion;

    #[test]
    fn shell_drag_uses_altitude_above_nonzero_terrain_in_both_friction_branches() {
        let rules = crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 2.0,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.5,
            lateral_friction: 0.5,
            aerodynamic_friction: 0.125,
            angular_factor: 1.0,
            allow_bouncing: false,
        };
        for (ground, altitude, speed) in [(0.25, 10.0, 1.75_f32), (25.0, 0.0, 1.0)] {
            let mut motion = Motion::new([10.0, 20.0, ground + altitude]);
            motion.velocity = [2.0, 0.0, 0.0];
            assert!(!motion.integrate_shell(&rules, 1, |_| ground, |_| ground));
            assert_eq!(motion.velocity[0].to_bits(), speed.to_bits());
            assert_eq!(motion.position[0].to_bits(), (10.0 + speed).to_bits());
            let expected_height = (ground + altitude + rules.gravity).max(ground);
            assert_eq!(motion.position[2].to_bits(), expected_height.to_bits());
        }
    }

    #[test]
    fn held_physics_discards_force_but_preserves_motion_and_external_landing() {
        let mut motion = Motion::new([10.0, 20.0, 30.0]);
        motion.velocity = [1.0, 2.0, -3.0];
        motion.acceleration = [4.0, 5.0, 6.0];
        motion.rates = [0.1, 0.2, 0.3];
        assert!(!motion.hold(0.0));
        assert_eq!(
            motion.position.map(f32::to_bits),
            [10.0_f32, 20.0, 30.0].map(f32::to_bits)
        );
        assert_eq!(
            motion.velocity.map(f32::to_bits),
            [1.0_f32, 2.0, -3.0].map(f32::to_bits)
        );
        assert_eq!(
            motion.acceleration.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            motion.rates.map(f32::to_bits),
            [0.1_f32, 0.2, 0.3].map(f32::to_bits)
        );
        motion.position[2] = 0.0;
        assert!(motion.hold(0.0));
        assert!(!motion.hold(0.0));
        assert!(!Motion::new([0.0; 3]).hold(0.0));
    }

    #[test]
    fn registered_held_physics_runs_and_preserves_the_carried_transform() {
        use crate::skirmish::{
            containment::tests::setup, module_updates::FOREVER, point_defense::tests::missile,
        };
        let (mut game, carrier) = setup(0.0, 1.0);
        let carrier_kind = game.entities[carrier].type_index;
        game.catalog[carrier_kind]
            .contain
            .as_mut()
            .unwrap()
            .allowed
            .push("PROJECTILE".into());
        // Keep the carrier stationary while measuring the passenger's PHYSICS update.
        game.ai_busy_from_ai(carrier);
        let serial = missile(&mut game, [357.0, 258.0, 25.0], 0, 1.0);
        let object = game.projectiles[game.projectile_index(serial).unwrap()]
            .object
            .unwrap();
        game.board(object, carrier);
        assert_eq!(game.entities[object].contained_by(), Some(carrier));
        game.wake_family_update(object, "MissileAIUpdate", 0, FOREVER);
        let motion = &mut game.entities[object]
            .projectile_flight
            .as_mut()
            .unwrap()
            .motion;
        motion.velocity = [1.0, 2.0, -3.0];
        motion.acceleration = [4.0, 5.0, 6.0];
        motion.rates = [0.1, 0.2, 0.3];
        let position = motion.position.map(f32::to_bits);
        let axes = motion.axes.map(|axis| axis.map(f32::to_bits));
        game.tick = 1;
        game.run_object_updates();
        let motion = &game.entities[object]
            .projectile_flight
            .as_ref()
            .unwrap()
            .motion;
        assert_eq!(motion.position.map(f32::to_bits), position);
        assert_eq!(motion.axes.map(|axis| axis.map(f32::to_bits)), axes);
        assert_eq!(
            motion.velocity.map(f32::to_bits),
            [1.0_f32, 2.0, -3.0].map(f32::to_bits)
        );
        assert_eq!(
            motion.acceleration.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
        assert_eq!(
            motion.rates.map(f32::to_bits),
            [0.1_f32, 0.2, 0.3].map(f32::to_bits)
        );
    }
}
