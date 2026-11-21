//! Object-owned projectile controls; module construction precedes weapon launch.
use super::{Control, Flight, FlightRules, LaunchTransform, Shell};
use crate::skirmish::{Skirmish, WeaponBinding, module_updates::ModuleRegistration};

impl Skirmish {
    pub(in crate::skirmish) fn hide_bound_projectile(&mut self, id: usize) {
        if let Some(flight) = &mut self.entities[id].projectile_flight {
            flight.hidden = true;
        }
    }

    pub(in crate::skirmish) fn ensure_projectile_motion(&mut self, id: usize) {
        if self.entities[id].projectile_flight.is_none() {
            self.entities[id].projectile_flight = Some(Flight::new([0.0; 3], self.update_frame()));
        }
    }

    pub(in crate::skirmish) fn construct_projectile_module(
        &mut self,
        id: usize,
        registration: usize,
    ) {
        self.ensure_projectile_motion(id);
        let ordinal = self.projectile_control_index(id, registration);
        let kind = self.entities[id].type_index;
        if matches!(
            self.catalog[kind].projectile_rules[ordinal],
            FlightRules::Missile(_)
        ) {
            // MissileAIUpdate constructs AIUpdateInterface before its own fields.
            self.construct_ai_module(id, registration);
        }
        let now = self.update_frame();
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("projectile object");
        let control = match &self.catalog[kind].projectile_rules[ordinal] {
            FlightRules::Missile(rules) => Control::Missile(super::missile::Missile::new(
                flight.motion.position,
                rules,
                now,
            )),
            FlightRules::Shell(_) => Control::Shell(Shell::new()),
        };
        assert_eq!(
            ordinal,
            flight.controls.len(),
            "source module construction order"
        );
        flight.controls.push(control);
    }

    pub(in crate::skirmish) fn projectile_control_index(
        &self,
        id: usize,
        registration: usize,
    ) -> usize {
        self.catalog[self.entities[id].type_index].update_modules[..registration]
            .iter()
            .filter(|module| {
                matches!(module,
                ModuleRegistration::Behavior { family, .. }
                if matches!(family.as_str(), "MissileAIUpdate" | "DumbProjectileBehavior"))
            })
            .count()
    }

    pub(in crate::skirmish) fn projectile_component(&self, id: usize) -> Option<usize> {
        self.projectile_index(self.entities[id].native_projectile?)
    }

    pub(in crate::skirmish::projectiles) fn synchronize_projectile_transform(&mut self, id: usize) {
        let motion = &self.entities[id]
            .projectile_flight
            .as_ref()
            .expect("projectile motion")
            .motion;
        let position = motion.position;
        let basis =
            std::array::from_fn(|row| std::array::from_fn(|column| motion.axes[column][row]));
        self.set_object_transform(id, position, basis);
        if let Some(index) = self.projectile_component(id) {
            self.projectiles[index].position = [position[0], position[1]];
        }
    }

    pub(in crate::skirmish::projectiles) fn wake_projectile_force(&mut self, id: usize) {
        let force = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("projectile motion")
            .motion
            .take_force_wake();
        if force {
            self.wake_family_update(id, "PhysicsBehavior", 0, 1);
        }
    }

    pub(in crate::skirmish::projectiles) fn damage_projectile_on_detonation(&mut self, id: usize) {
        let amount = self.entities[id].max_health;
        self.apply_body_damage(
            id,
            None,
            amount,
            crate::skirmish::DamageType::Unresistable,
            crate::skirmish::DeathType::Detonated,
            false,
        );
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn launch_native_projectile(
        &mut self,
        id: usize,
        launcher: usize,
        binding: crate::skirmish::firing::WeaponBinding,
        barrel: usize,
        destination: [f32; 3],
        target: Option<usize>,
    ) {
        if self.catalog[self.entities[id].type_index]
            .behavior
            .neutron_missile
            .is_some()
        {
            self.prepare_neutron_missile(id, launcher, binding, barrel, destination, target);
            return;
        }
        // WeaponTemplate uses only the first ProjectileUpdateInterface.
        if self.catalog[self.entities[id].type_index]
            .projectile_rules
            .is_empty()
        {
            self.set_object_position(id, destination);
            return;
        }
        let launch = self.launch_transform(launcher, binding, barrel);
        let transfer_physics = self.registered_physics(id)
            && (self.entities[launcher].physical.is_some()
                || self.entities[launcher].live_physics_active()
                || self.registered_physics(launcher));
        let velocity = self.entities[launcher].physics_velocity();
        if transfer_physics {
            let flight = self.entities[id]
                .projectile_flight
                .as_mut()
                .expect("native projectile control");
            flight.motion.inherit_velocity(velocity);
            flight.ignore_collisions_with = Some(launcher);
        }
        self.fire_projectile_from_pose(id, binding, &launch, destination, target);
    }

    pub(in crate::skirmish) fn launch_delivered_projectile(
        &mut self,
        carrier: usize,
        payload: usize,
        weapon: &str,
        position: [f32; 3],
        destination: [f32; 3],
    ) -> bool {
        if self.catalog[self.entities[payload].type_index]
            .projectile_rules
            .is_empty()
        {
            return false;
        }
        let type_index = self.entities[carrier].type_index;
        let unit = &self.catalog[type_index];
        let weapon_index = (0..unit.weapon_template_count())
            .find(|&index| {
                unit.weapon_template(index)
                    .is_some_and(|rules| rules.name == weapon)
            })
            .expect("validated visible payload weapon");
        let binding = WeaponBinding {
            type_index,
            weapon_index,
            weapon_slot: 0,
        };
        self.register_delivered_projectile(carrier, payload, binding, destination);
        let (_, basis) = self.object_transform(payload);
        let launch = LaunchTransform {
            position,
            axes: std::array::from_fn(|column| basis.map(|row| row[column])),
        };
        // DeliverPayloadAIUpdate invokes projectileFireAtObjectOrPosition on the
        // live payload. Its earlier motive force and pose survive this callback.
        self.fire_projectile_from_pose(payload, binding, &launch, destination, None);
        if let Some(index) = self.projectile_component(payload) {
            let snapshot = self.projectiles[index].callback_snapshot(&self.entities);
            self.record_projectile_event(
                &snapshot,
                self.object_position(payload),
                super::super::ProjectileFxStage::Created,
            );
        }
        true
    }

    fn fire_projectile_from_pose(
        &mut self,
        id: usize,
        binding: WeaponBinding,
        launch: &LaunchTransform,
        destination: [f32; 3],
        target: Option<usize>,
    ) {
        let victim = target.map(|target| self.missile_target(target));
        // DumbProjectileBehavior::projectileFireAtObjectOrPosition selects the
        // victim center here; MissileAIUpdate uses the passed victimPos instead.
        let shell_destination = target.map_or(destination, |target| self.target_center(target));
        let now = self.update_frame();
        let kind = self.entities[id].type_index;
        let weapon = self.catalog[binding.type_index]
            .weapon_template(binding.weapon_index)
            .expect("firing weapon");
        let speed = weapon.speed * (1.0 / 30.0);
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("native projectile control");
        flight.hidden = false;
        flight.motion.position = launch.position;
        flight.motion.axes = launch.axes;
        match (
            &mut flight.controls[0],
            &self.catalog[kind].projectile_rules[0],
        ) {
            (Control::Missile(missile), FlightRules::Missile(rules)) => {
                missile.fire(
                    &mut flight.motion,
                    &crate::skirmish::projectiles::ProjectileLaunch {
                        transform: launch,
                        destination,
                        speed,
                        frame: now,
                    },
                    victim.as_ref(),
                    rules,
                );
            }
            (Control::Shell(shell), FlightRules::Shell(rules)) => {
                let scale = weapon
                    .motion
                    .as_ref()
                    .and_then(|motion| motion.shell_speed_scale.as_ref());
                if shell
                    .fire(
                        &mut flight.motion,
                        &crate::skirmish::projectiles::ProjectileLaunch {
                            transform: launch,
                            destination: shell_destination,
                            speed,
                            frame: now,
                        },
                        scale,
                        rules,
                        self.projectile_terrain
                            .as_ref()
                            .expect("native shell terrain"),
                        &mut self.random,
                    )
                    .is_none()
                {
                    self.destroy_maintenance_object(id);
                }
            }
            _ => unreachable!("ordered projectile rule/control identity"),
        }
        if matches!(
            self.catalog[kind].projectile_rules[0],
            FlightRules::Missile(_)
        ) {
            self.entities[id].ai.current_victim = target;
        }
        self.synchronize_projectile_transform(id);
        self.wake_projectile_force(id);
    }
}
