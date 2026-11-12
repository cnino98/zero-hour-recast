use super::{Event, Target};
#[cfg(test)]
use crate::collision_geometry::{Body, intersects};
use crate::skirmish::projectiles::Control;
use crate::skirmish::{FlightRules, Skirmish};

impl Skirmish {
    pub(in crate::skirmish) fn update_missile_module(
        &mut self,
        id: usize,
        registration: usize,
    ) -> i32 {
        let ordinal = self.projectile_control_index(id, registration);
        let now = self.update_frame();
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("MissileAI object");
        if flight.motion.position[2] < 0.0 {
            self.destroy_maintenance_object(id);
            return i32::from_ne_bytes(crate::skirmish::module_updates::FOREVER.to_ne_bytes());
        }
        let Control::Missile(missile) = &mut flight.controls[ordinal] else {
            unreachable!("MissileAI state");
        };
        let decoy = missile.countermeasure_due(now);
        if decoy && let Some(index) = self.projectile_component(id) {
            let target = self.projectiles[index]
                .target
                .filter(|&target| self.entities[target].exists());
            if let Some(flare) = target.and_then(|target| self.countermeasure_target(target)) {
                let position = self.object_position(flare);
                let flight = self.entities[id]
                    .projectile_flight
                    .as_mut()
                    .expect("MissileAI object");
                let Control::Missile(missile) = &mut flight.controls[ordinal] else {
                    unreachable!();
                };
                missile.countermeasure_divert(position);
                self.projectiles[index].target = Some(flare);
                self.entities[id].ai.current_victim = Some(flare);
            }
        }
        // MissileAIUpdate calls its common AI base before its own event handling.
        let FlightRules::Missile(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
        else {
            unreachable!("MissileAI data");
        };
        let allow = rules.physics.apply_friction_when_airborne;
        self.entities[id].set_physics_airborne_friction(allow);
        let event = self.advance_missile(id, ordinal);
        self.complete_missile_event(id, ordinal, &event);
        if !matches!(event, Event::None) {
            self.finish_missile_movement(id, ordinal);
        }
        self.synchronize_projectile_transform(id);
        self.wake_projectile_force(id);
        1
    }

    fn complete_missile_event(&mut self, id: usize, ordinal: usize, event: &Event) {
        let flight = self.entities[id]
            .projectile_flight
            .as_ref()
            .expect("MissileAI object");
        let Control::Missile(missile) = &flight.controls[ordinal] else {
            unreachable!();
        };
        let deals_damage = missile.deals_damage();
        let component = missile
            .launched
            .then(|| self.projectile_component(id))
            .flatten();
        match *event {
            Event::None => {}
            Event::Detonate(position) => {
                if let Some(index) = component {
                    let projectile = &self.projectiles[index];
                    let snapshot = projectile.callback_snapshot(&self.entities);
                    let mut impact = crate::skirmish::fire::Impact::new(
                        projectile,
                        &self.entities,
                        position,
                        self.projectile_damage_bonus(projectile),
                        self.projectile_bonus_factor(
                            projectile,
                            crate::skirmish::containment::bonuses::RADIUS,
                        ),
                    );
                    if !deals_damage {
                        impact = impact.without_damage();
                    }
                    let calls_on_die = self.catalog[projectile.weapon_type]
                        .weapon_template(projectile.weapon_index)
                        .expect("firing weapon")
                        .projectile
                        .calls_on_die;
                    self.create_weapon_hazard(
                        snapshot.source,
                        snapshot.weapon_index,
                        snapshot.owner,
                        position,
                        self.tick,
                    );
                    self.record_projectile_event(
                        &snapshot,
                        position,
                        crate::skirmish::ProjectileFxStage::Detonation,
                    );
                    self.apply_projectile_impact(&impact);
                    if calls_on_die {
                        self.damage_projectile_on_detonation(id);
                    }
                } else if deals_damage {
                    self.damage_projectile_on_detonation(id);
                }
                let now = self.update_frame();
                let flight = self.entities[id]
                    .projectile_flight
                    .as_mut()
                    .expect("deferred destruction");
                flight.hidden = true;
                flight.mark_killing_self();
                let Control::Missile(missile) = &mut flight.controls[ordinal] else {
                    unreachable!();
                };
                missile.finish_detonation(now);
            }
            Event::Remove => {
                if component.is_some() {
                    let FlightRules::Missile(rules) =
                        &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
                    else {
                        unreachable!();
                    };
                    if rules.retirement.calls_kill {
                        self.kill_object(id, crate::skirmish::DeathType::Normal);
                    } else {
                        self.destroy_maintenance_object(id);
                    }
                }
                let flight = self.entities[id]
                    .projectile_flight
                    .as_mut()
                    .expect("deferred destruction");
                let Control::Missile(missile) = &mut flight.controls[ordinal] else {
                    unreachable!();
                };
                missile.finish_removal();
            }
        }
    }

    fn missile_control_target(&self, id: usize, ordinal: usize) -> Option<Target> {
        let flight = self.entities[id].projectile_flight.as_ref()?;
        let Control::Missile(missile) = &flight.controls[ordinal] else {
            return None;
        };
        if !missile.launched {
            return None;
        }
        let projectile = &self.projectiles[self.projectile_component(id)?];
        projectile
            .target
            .filter(|&target| self.entities[target].exists())
            .map(|target| self.missile_target(target))
    }

    fn finish_missile_movement(&mut self, id: usize, ordinal: usize) {
        let target = self.missile_control_target(id, ordinal);
        let FlightRules::Missile(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
        else {
            unreachable!();
        };
        let now = self.update_frame();
        let terrain = &self.projectile_terrain;
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("MissileAI object");
        let Control::Missile(missile) = &mut flight.controls[ordinal] else {
            unreachable!();
        };
        let surface = terrain.as_ref().map_or(0.0, |terrain| {
            terrain.sample([flight.motion.position[0], flight.motion.position[1]])
        });
        missile.update_move(&mut flight.motion, target.as_ref(), rules, now, surface);
    }

    fn advance_missile(&mut self, id: usize, ordinal: usize) -> Event {
        let target = self.missile_control_target(id, ordinal);
        let component = self.projectile_component(id);
        let now = self.update_frame();
        let FlightRules::Missile(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
        else {
            unreachable!();
        };
        let terrain = &self.projectile_terrain;
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("MissileAI object");
        let Control::Missile(missile) = &mut flight.controls[ordinal] else {
            unreachable!();
        };
        let before = matches!(missile.state, super::State::Launch);
        let position = flight.motion.position;
        let axes = flight.motion.axes;
        let surface = terrain
            .as_ref()
            .map_or(0.0, |terrain| terrain.sample([position[0], position[1]]));
        let event = missile.update(&mut flight.motion, target.as_ref(), rules, now, surface);
        if before
            && matches!(missile.state, super::State::AttackNoTurn)
            && let Some(index) = component
        {
            let projectile = &self.projectiles[index];
            self.presentation
                .weapon_fx
                .push(crate::skirmish::WeaponFxEvent::Projectile(
                    crate::skirmish::ProjectileFxEvent {
                        weapon_type: projectile.weapon_type,
                        weapon_index: projectile.weapon_index,
                        object: None,
                        object_geometry_radius: None,
                        weapon_slot: projectile.weapon_slot,
                        projectile: projectile.id,
                        source: projectile.source,
                        position,
                        axes,
                        veterancy: projectile.veterancy,
                        stage: crate::skirmish::ProjectileFxStage::Ignition,
                    },
                ));
        }
        event
    }

    pub(in crate::skirmish::projectiles) fn missile_ground_contact(
        &mut self,
        id: usize,
        ordinal: usize,
    ) {
        let now = self.update_frame();
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("MissileAI object");
        let Control::Missile(missile) = &mut flight.controls[ordinal] else {
            unreachable!();
        };
        let event = missile.ground_contact(&flight.motion, now);
        self.complete_missile_event(id, ordinal, &event);
        if matches!(event, Event::Detonate(_)) {
            self.entities[id]
                .projectile_flight
                .as_mut()
                .expect("deferred destruction")
                .disable_collisions();
        }
    }

    pub(in crate::skirmish::projectiles) fn resolve_missile_contact(
        &mut self,
        id: usize,
        ordinal: usize,
        target: usize,
    ) {
        let Some(index) = self.projectile_component(id) else {
            return;
        };
        let projectile = &self.projectiles[index];
        let flight = self.entities[id]
            .projectile_flight
            .as_ref()
            .expect("MissileAI object");
        let Control::Missile(missile) = &flight.controls[ordinal] else {
            unreachable!();
        };
        if flight.no_collisions() || !missile.can_collide() || !missile.launched {
            return;
        }
        let FlightRules::Missile(rules) =
            &self.catalog[self.entities[id].type_index].projectile_rules[ordinal]
        else {
            unreachable!();
        };
        let mask = self.catalog[projectile.weapon_type]
            .weapon_template(projectile.weapon_index)
            .and_then(|weapon| weapon.motion.as_ref())
            .expect("firing weapon motion")
            .collide_mask;
        if !self.projectile_collides_with(
            target,
            projectile.source,
            projectile.target.filter(|_| missile.tracking),
            mask,
        ) {
            return;
        }
        let snapshot = projectile.callback_snapshot(&self.entities);
        let clear = rules
            .clear
            .as_ref()
            .map(|clear| (clear.count, clear.allowed.clone(), clear.forbidden.clone()));
        if let Some((count, allowed, forbidden)) = clear
            && self.clearable_garrison(target)
            && self.clear_garrison(snapshot.source, target, count, &allowed, &forbidden) > 0
        {
            self.record_garrison_kill_fx(&snapshot, target);
            self.destroy_maintenance_object(id);
            return;
        }
        let now = self.update_frame();
        let flight = self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("MissileAI object");
        let Control::Missile(missile) = &mut flight.controls[ordinal] else {
            unreachable!();
        };
        let event = missile.collide(&flight.motion, now);
        self.complete_missile_event(id, ordinal, &event);
        self.entities[id]
            .projectile_flight
            .as_mut()
            .expect("deferred destruction")
            .disable_collisions();
    }

    pub(in crate::skirmish::projectiles) fn missile_target(&self, id: usize) -> Target {
        let entity = &self.entities[id];
        let bounds = self.body_geometry_bounds(id);
        Target {
            position: self.object_position(id),
            max_height: bounds.max_height,
            center_height: bounds.center_height,
            sphere_radius: bounds.sphere_radius,
            velocity: entity.physics_velocity(),
            direction: [entity.facing.cos(), entity.facing.sin(), 0.0],
        }
    }

    #[cfg(test)]
    pub(in crate::skirmish) fn missile_contact(
        &self,
        missile: &Body<'_>,
        id: usize,
        launcher: usize,
        target: Option<usize>,
        mask: u16,
    ) -> bool {
        if !self.projectile_collides_with(id, launcher, target, mask) {
            return false;
        }
        intersects(
            missile,
            &Body {
                geometry: self.body_geometry(id),
                position: self.object_position(id),
                angle: self.entities[id].facing,
            },
        )
    }

    pub(in crate::skirmish) fn projectile_collides_with(
        &self,
        id: usize,
        launcher: usize,
        target: Option<usize>,
        mask: u16,
    ) -> bool {
        let entity = &self.entities[id];
        let unit = &self.catalog[entity.type_index];
        if self.body_no_collisions(id) {
            return false;
        }
        // WeaponTemplate::shouldProjectileCollideWith: intended target precedes
        // launcher/relationship filtering. Controlled buildings have their own bit.
        if target != Some(id) {
            if id == launcher || self.entities[launcher].contained_by() == Some(id) {
                return false;
            }
            if entity.aircraft_targeting_offset(unit, self.tick).is_some() {
                return false;
            }
            let owner = self.entities[launcher].owner;
            let mut required = if self.allied(entity.owner, owner) {
                0x0001
            } else if self.enemies(entity.owner, owner) {
                0x0002
            } else {
                0
            };
            if unit.kind_of.iter().any(|kind| kind == "STRUCTURE") {
                required |= if entity.owner == owner {
                    0x0100
                } else {
                    0x0004
                };
            }
            for (kind, bit) in [
                ("SHRUBBERY", 0x0008),
                ("PROJECTILE", 0x0010),
                ("SMALL_MISSILE", 0x0040),
                ("BALLISTIC_MISSILE", 0x0080),
            ] {
                if unit.kind_of.iter().any(|flag| flag == kind) {
                    required |= bit;
                }
            }
            if unit.fence_width > 0.0 {
                required |= 0x0020;
            }
            if mask & required == 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::Missile;
    use super::Body;
    use crate::skirmish::{LaunchTransform, projectiles::Motion};

    #[test]
    fn ascending_target_lead_uses_full_physics_speed_along_native_object_facing() {
        use crate::skirmish::flight::{Appearance, tests::rules};
        use crate::skirmish::{Kind, MovementSurface};

        let mut game = crate::skirmish::tests::game();
        let index = game.type_for(0, Kind::Tank).unwrap();
        let unit = &mut game.catalog[index];
        let mut flight = rules(Appearance::Hover);
        flight.mass = 1.0;
        flight.gravity = 0.0;
        unit.locomotor.ground = None;
        unit.locomotor.flight = Some(flight);
        unit.locomotor.legal_surfaces = 8;
        unit.movement_surface = MovementSurface::Air;
        unit.preferred_height = 100.0;
        unit.geometry_bounds.max_height = 20.0;
        let id = game.spawn(index, 0, [200.0, 100.0], true);
        game.entities[id].facing = 0.0;
        game.apply_flight_force(id, [3.0, 0.0, 4.0]);
        game.integrate_flight(id);
        let target = game.missile_target(id);
        assert_eq!(
            target.velocity.map(f32::to_bits),
            [3.0_f32, 0.0, 4.0].map(f32::to_bits)
        );

        let launch = LaunchTransform {
            position: [
                target.position[0] - 200.0,
                target.position[1],
                target.position[2] + 10.0,
            ],
            axes: LaunchTransform::IDENTITY.axes,
        };
        let rules = super::super::tests::rules();
        let mut missile = Missile::new([0.0; 3], &rules, 100);
        let mut motion = Motion::new([0.0; 3]);
        missile.fire(
            &mut motion,
            &crate::skirmish::projectiles::ProjectileLaunch {
                transform: &launch,
                destination: target.position,
                speed: 20.0,
                frame: 100,
            },
            Some(&target),
            &rules,
        );
        // AIMoveToState::update: distance 200, target speed 5, missile's minimum
        // lead speed 5. Lead is 100 along facing, not along its ascent velocity.
        let expected = [
            target.position[0] + 100.0,
            target.position[1],
            target.position[2] + 10.0,
        ];
        assert_eq!(
            missile
                .move_destination(&motion, Some(&target))
                .map(f32::to_bits),
            expected.map(f32::to_bits)
        );
        game.entities[id].flight = None;
        assert_eq!(
            game.missile_target(id).velocity.map(f32::to_bits),
            [3.0_f32, 0.0, 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn passenger_missile_ignores_own_carrier_unless_it_is_the_designated_victim() {
        let mut game = crate::skirmish::tests::game();
        let carrier = game.spawn(6, 0, [300.0, 100.0], true);
        let launcher = game.spawn(5, 0, [300.0, 100.0], true);
        game.entities[launcher].containment.carrier = Some(carrier);
        let body = Body {
            geometry: &game.catalog[5].geometry,
            position: game.object_position(carrier),
            angle: 0.0,
        };
        assert!(!game.missile_contact(&body, carrier, launcher, None, u16::MAX));
        assert!(game.missile_contact(&body, carrier, launcher, Some(carrier), u16::MAX));
        game.entities[launcher].containment.carrier = None;
        assert!(game.missile_contact(&body, carrier, launcher, None, u16::MAX));
    }
    #[test]
    fn projectile_masks_distinguish_alliance_structure_ownership_and_original_kind_flags() {
        let mut game = crate::skirmish::tests::game();
        game.slots[0].team = Some(1);
        game.slots[1].team = Some(1);
        let launcher = game.spawn(5, 0, [300.0, 100.0], true);
        let target = game.spawn(3, 1, [300.0, 100.0], true);
        let body_geometry = crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Gla,
            crate::skirmish::Kind::Tank,
        )
        .geometry;
        let body = Body {
            geometry: &body_geometry,
            position: game.object_position(target),
            angle: 0.0,
        };
        assert!(game.missile_contact(&body, target, launcher, None, 1));
        assert!(!game.missile_contact(&body, target, launcher, None, 2));
        assert!(game.missile_contact(&body, target, launcher, None, 4));
        assert!(!game.missile_contact(&body, target, launcher, None, 0x0100));
        let unit = &mut game.catalog[3];
        unit.kind_of.clear();
        unit.fence_width = 2.0;
        assert!(game.missile_contact(&body, target, launcher, None, 0x0020));
        assert!(!game.missile_contact(&body, target, launcher, None, 4));
        game.catalog[3].fence_width = 0.0;
        for (kind, bit) in [
            ("SHRUBBERY", 8),
            ("PROJECTILE", 16),
            ("SMALL_MISSILE", 64),
            ("BALLISTIC_MISSILE", 128),
        ] {
            game.catalog[3].kind_of = vec![kind.into()];
            assert!(game.missile_contact(&body, target, launcher, None, bit));
        }
    }
}
