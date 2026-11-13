//! CountermeasuresBehavior.cpp and Object.cpp ordered interface iteration, CRC=0.
use super::{Skirmish, module_updates::FOREVER, startup};
use num_traits::ToPrimitive;

pub struct Rules {
    pub triggers: Vec<String>,
    pub conflicts: Vec<String>,
    pub requires_all: bool,
    pub flare: String,
    pub evasion_rate: f32,
    pub volley_size: u32,
    pub arc: f32,
    pub velocity_factor: f32,
    pub volley_delay: u32,
    pub volleys: u32,
    pub reload: u32,
    pub decoy_delay: u32,
    pub reaction_delay: u32,
}

impl super::Projectile {
    /// The current target, which can change in flight after countermeasure diversion.
    #[must_use]
    pub const fn target_object(&self) -> Option<usize> {
        self.target
    }
}

pub(super) struct State {
    active: bool,
    available: u32,
    active_count: u32,
    diverted: u32,
    incoming: u32,
    reaction: u32,
    next_volley: u32,
    reload: u32,
    flares: Vec<usize>,
}

impl State {
    pub(super) const fn new(rules: &Rules) -> Self {
        Self {
            active: false,
            available: rules.volleys.wrapping_mul(rules.volley_size),
            active_count: 0,
            diverted: 0,
            incoming: 0,
            reaction: 0,
            next_volley: 0,
            // Native pool allocation zeroes the entire block before construction:
            // GameMemory.h:628 -> GameMemory.cpp:1705. Ctor leaves this field alone.
            reload: 0,
            flares: Vec::new(),
        }
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn reset_countermeasure_upgrade(&mut self, id: usize, ordinal: usize) {
        self.entities[id].behaviors.countermeasures[ordinal].active = false;
    }

    pub(in crate::skirmish) fn construct_countermeasures(&mut self, id: usize, ordinal: usize) {
        let kind = self.entities[id].type_index;
        assert_eq!(self.entities[id].behaviors.countermeasures.len(), ordinal);
        self.entities[id].behaviors.countermeasures.push(State::new(
            &self.catalog[kind].behavior.countermeasures[ordinal],
        ));
    }

    pub(in crate::skirmish) fn apply_countermeasure_upgrade(&mut self, id: usize, ordinal: usize) {
        self.wake_family_update(id, "CountermeasuresBehavior", ordinal, 1);
        self.entities[id].behaviors.countermeasures[ordinal].active = true;
    }

    pub(in crate::skirmish) fn update_countermeasures(&mut self, id: usize, ordinal: usize) -> i32 {
        let actor = &self.entities[id];
        if actor.is_effectively_dead() || !actor.behaviors.countermeasures[ordinal].active {
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        }
        let invalid = actor.behaviors.countermeasures[ordinal]
            .flares
            .iter()
            .copied()
            .filter(|&flare| !self.entities[flare].exists())
            .collect::<Vec<_>>();
        let state = &mut self.entities[id].behaviors.countermeasures[ordinal];
        state.flares.retain(|flare| !invalid.contains(flare));
        state.active_count = state
            .active_count
            .wrapping_sub(u32::try_from(invalid.len()).expect("native flare count"));
        let actor = &self.entities[id];
        let unit = &self.catalog[actor.type_index];
        let airborne = actor.is_airborne_target();
        let volley_delay = unit.behavior.countermeasures[ordinal].volley_delay;
        let reload_delay = unit.behavior.countermeasures[ordinal].reload;
        let now = self.update_frame();
        if airborne && actor.behaviors.countermeasures[ordinal].available != 0 {
            if actor.behaviors.countermeasures[ordinal].reaction != 0
                && actor.behaviors.countermeasures[ordinal].reaction == now
            {
                self.launch_countermeasure_volley(id, ordinal);
                let state = &mut self.entities[id].behaviors.countermeasures[ordinal];
                state.next_volley = now.wrapping_add(volley_delay);
                state.reaction = 0;
            }
            if self.entities[id].behaviors.countermeasures[ordinal].next_volley == now {
                self.launch_countermeasure_volley(id, ordinal);
                self.entities[id].behaviors.countermeasures[ordinal].next_volley =
                    now.wrapping_add(volley_delay);
            }
        }
        let state = &mut self.entities[id].behaviors.countermeasures[ordinal];
        if state.available == 0 && reload_delay != 0 {
            if state.reload == 0 {
                state.reload = now.wrapping_add(reload_delay);
            } else if state.reload <= now {
                self.reload_countermeasures(id, ordinal);
            }
        }
        1
    }

    pub(in crate::skirmish) fn reload_countermeasures(&mut self, id: usize, ordinal: usize) {
        let actor = &mut self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.countermeasures[ordinal];
        actor.behaviors.countermeasures[ordinal].available =
            rules.volleys.wrapping_mul(rules.volley_size);
        actor.behaviors.countermeasures[ordinal].reload = 0;
    }

    fn launch_countermeasure_volley(&mut self, id: usize, ordinal: usize) {
        let actor = &self.entities[id];
        let rules = &self.catalog[actor.type_index].behavior.countermeasures[ordinal];
        let (size, arc, factor) = (rules.volley_size, rules.arc, rules.velocity_factor);
        let kind = self
            .catalog
            .iter()
            .position(|unit| unit.name == rules.flare);
        let position = self.object_position(id);
        let facing = actor.facing;
        let inherited = actor.physics_velocity();
        let owner = actor.owner;
        let squared = inherited.map(|component| component * component);
        let speed = ((squared[0] + squared[1]) + squared[2]).sqrt();
        let speed = if speed < 1.0 { -10.0 } else { speed };
        for index in 0..size {
            let ratio = if size == 1 {
                0.0
            } else {
                let fraction = index.to_f32().expect("native volley ordinal")
                    / (size.to_f32().expect("native volley size") - 1.0)
                    * 2.0;
                fraction - 1.0
            };
            let angle = ratio * arc;
            let (angle_sin, angle_cos) = angle.sin_cos();
            // Vector2.Normalize of the object's XY forward direction precedes Rotate.
            let direction = self.entities[id].object_direction_3d();
            let forward = [direction[0], direction[1]];
            let squared = forward.map(|component| component * component);
            let length_squared = squared[0] + squared[1];
            let forward = if length_squared == 0.0 {
                forward
            } else {
                let inverse_length = 1.0 / length_squared.sqrt();
                forward.map(|value| value * inverse_length)
            };
            let scale = speed * factor;
            let rotated_x = [forward[0] * angle_cos, forward[1] * angle_sin];
            let rotated_y = [forward[0] * angle_sin, forward[1] * angle_cos];
            let force = [
                (rotated_x[0] - rotated_x[1]) * scale,
                (rotated_y[0] + rotated_y[1]) * scale,
                0.0,
            ];
            if let Some(kind) = kind {
                let flare = self.spawn(kind, owner, [0.0; 2], true);
                self.set_object_position(flare, position);
                self.set_object_orientation(flare, facing);
                self.countermeasure_flare_force(flare, inherited, force);
                let state = &mut self.entities[id].behaviors.countermeasures[ordinal];
                state.active_count = state.active_count.wrapping_add(1);
                state.available = state.available.wrapping_sub(1);
                state.flares.push(flare);
            }
        }
    }

    pub(in crate::skirmish) fn countermeasure_target(&self, victim: usize) -> Option<usize> {
        let actor = &self.entities[victim];
        let state = actor.behaviors.countermeasures.first()?;
        let size = self.catalog[actor.type_index].behavior.countermeasures[0]
            .volley_size
            .max(1);
        let mut counted = 0;
        let mut closest = None;
        let mut distance = 1.0e15_f32;
        for &flare in state.flares.iter().rev() {
            if counted >= size {
                break;
            }
            if !self.entities[flare].exists() {
                continue;
            }
            let point = self.entities[flare].position;
            let x = point[0] - actor.position[0];
            let y = point[1] - actor.position[1];
            let y_squared = y * y;
            let x_squared = x * x;
            let squared = x_squared + y_squared;
            if squared < distance {
                distance = squared;
                closest = Some(flare);
            }
            counted += 1;
        }
        closest
    }

    pub(in crate::skirmish) fn report_projectile_countermeasures(
        &mut self,
        projectile: usize,
        victim: Option<usize>,
    ) {
        let Some(victim) = victim else {
            return;
        };
        let actor = &self.entities[victim];
        if !self.catalog[self.entities[projectile].type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "SMALL_MISSILE")
            || !actor
                .behaviors
                .countermeasures
                .first()
                .is_some_and(|state| state.active)
            || matches!(
                self.catalog[actor.type_index].ai.update,
                super::UnitAiUpdate::Absent
            )
            || actor.ai.locomotor_set == super::locomotor::set::SUPERSONIC
        {
            return;
        }
        self.countermeasure_report(victim, projectile);
    }

    fn countermeasure_report(&mut self, victim: usize, projectile: usize) {
        let now = self.update_frame();
        for ordinal in 0..self.entities[victim].behaviors.countermeasures.len() {
            let actor = &mut self.entities[victim];
            let rules = &self.catalog[actor.type_index].behavior.countermeasures[ordinal];
            let state = &mut actor.behaviors.countermeasures[ordinal];
            state.incoming = state.incoming.wrapping_add(1);
            if state.available.wrapping_add(state.active_count) == 0 {
                continue;
            }
            if startup::random_real(&mut self.random, 0.0, 1.0) >= rules.evasion_rate {
                continue;
            }
            let (decoy_delay, reaction_delay) = (rules.decoy_delay, rules.reaction_delay);
            let Some(control) = self.entities[projectile]
                .projectile_flight
                .as_mut()
                .and_then(|flight| flight.controls.first_mut())
            else {
                continue;
            };
            match control {
                super::projectiles::Control::Missile(missile) => {
                    missile.set_countermeasure_delay(now.wrapping_add(decoy_delay));
                }
                // DumbProjectile inherits the interface's empty diversion notification.
                super::projectiles::Control::Shell(_) => {}
            }
            let state = &mut self.entities[victim].behaviors.countermeasures[ordinal];
            state.diverted = state.diverted.wrapping_add(1);
            if state.active_count == 0 && state.reaction == 0 {
                state.reaction = now.wrapping_add(reaction_delay);
            }
        }
    }
}

pub(super) fn validate(catalog: &[super::UnitType]) -> Result<(), &'static str> {
    for rules in catalog
        .iter()
        .flat_map(|unit| &unit.behavior.countermeasures)
    {
        if let Some(flare) = catalog.iter().find(|unit| unit.name == rules.flare)
            && (flare.live_physics.is_none() || flare.locomotor.flight.is_none())
        {
            return Err(
                "CountermeasuresBehavior flare requires an unbound Physics motion provider",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            triggers: vec!["Countermeasures".into()],
            conflicts: Vec::new(),
            requires_all: false,
            flare: String::new(),
            evasion_rate: 0.3,
            volley_size: 2,
            arc: 0.0,
            velocity_factor: 1.0,
            volley_delay: 3,
            volleys: 3,
            reload: 120,
            decoy_delay: 15,
            reaction_delay: 0,
        }
    }

    #[test]
    fn matching_upgrade_removal_clears_latch_without_restoring_ammunition() {
        let mut game = crate::skirmish::tests::game();
        let unit = &mut game.catalog[6];
        unit.behavior.countermeasures.push(rules());
        crate::skirmish::tests::register_update(unit, "CountermeasuresBehavior");
        unit.behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers
            .push("Countermeasures".into());
        game.players[0].upgrades.push("Countermeasures".into());
        let id = game.spawn(6, 0, [300.0, 300.0], true);
        assert!(game.entities[id].behaviors.countermeasures[0].active);
        game.entities[id].behaviors.countermeasures[0].available = 2;
        game.remove_native_object_upgrade(id, "Unrelated");
        assert!(game.entities[id].behaviors.countermeasures[0].active);
        game.remove_native_object_upgrade(id, "Countermeasures");
        assert!(!game.entities[id].behaviors.countermeasures[0].active);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].available, 2);
        assert!(game.has_upgrade(0, "Countermeasures"));
        game.activate_upgrade_modules(id);
        assert!(game.entities[id].behaviors.countermeasures[0].active);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].available, 2);
    }

    #[test]
    fn exhausted_volley_starts_from_allocator_zero_and_reloads_at_native_deadline() {
        let mut game = crate::skirmish::tests::game();
        game.catalog[6].behavior.countermeasures.push(rules());
        crate::skirmish::tests::register_update(&mut game.catalog[6], "CountermeasuresBehavior");
        let id = game.spawn(6, 0, [300.0, 300.0], true);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].available, 6);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].reload, 0);
        // State at completion of the third two-flare volley; no first airfield reload occurred.
        let state = &mut game.entities[id].behaviors.countermeasures[0];
        state.active = true;
        state.available = 0;
        game.tick = 100;
        assert_eq!(game.update_countermeasures(id, 0), 1);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].reload, 220);
        game.tick = 219;
        game.update_countermeasures(id, 0);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].available, 0);
        game.tick = 220;
        game.update_countermeasures(id, 0);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].available, 6);
        assert_eq!(game.entities[id].behaviors.countermeasures[0].reload, 0);
    }

    fn flare_template(game: &mut Skirmish) -> usize {
        use crate::skirmish::physical_ocl::{LiveRules, MotionRules};
        let mut flare = crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Usa,
            crate::skirmish::Kind::Vehicle,
        );
        flare.name = "Flare".into();
        flare.locomotor.ground = None;
        flare.locomotor.flight = Some(crate::skirmish::flight::tests::rules(
            crate::skirmish::flight::Appearance::Hover,
        ));
        flare.live_physics = Some(LiveRules {
            motion: MotionRules {
                center_of_mass_offset: 0.0,
                mass: 1.0,
                gravity: 0.0,
                stiffness: 0.5,
                forward_friction: 0.0,
                lateral_friction: 0.0,
                aerodynamic_friction: 0.0,
                angular_factor: 2.0,
                allow_bouncing: false,
            },
            resistance: 0.0,
            kill_when_resting: false,
            collision: crate::skirmish::physical_ocl::CollisionRules {
                stiffness: 0.5,
                rubble_height: 0.0,
                allow_force: true,
                weapons: [None, None],
            },
            shock_rotation: [0.0; 3],
            min_fall_speed: 99.0,
            fall_damage_factor: 1.0,
        });
        let kind = game.catalog.len();
        game.catalog.push(flare);
        kind
    }

    #[test]
    fn volley_normalizes_forward_with_reciprocal_before_applying_force() {
        let mut game = crate::skirmish::tests::game();
        flare_template(&mut game);
        let mut volley = rules();
        volley.flare = "Flare".into();
        volley.volley_size = 1;
        game.catalog[6].behavior.countermeasures.push(volley);
        crate::skirmish::tests::register_update(&mut game.catalog[6], "CountermeasuresBehavior");
        let id = game.spawn(6, 0, [300.0, 300.0], true);
        game.set_object_orientation(id, 0.002);
        game.launch_countermeasure_volley(id, 0);
        let flare = game.entities[id].behaviors.countermeasures[0].flares[0];
        let actor = &game.entities[flare];
        let input = actor
            .aircraft_physics_input(&game.catalog[actor.type_index])
            .unwrap();
        // Native Vector2 reciprocal multiplication differs from component division
        // by one bit in the X force for this ordinary, nearly axial heading.
        assert_eq!(
            input.acceleration.map(f32::to_bits),
            [0xc11f_ffec, 0xbca3_d704, 0]
        );
    }

    fn missile_object(game: &Skirmish, serial: u64) -> usize {
        game.projectiles[game.projectile_index(serial).unwrap()]
            .object
            .unwrap()
    }

    fn countermeasure_aircraft() -> (Skirmish, usize) {
        let mut game = crate::skirmish::tests::game();
        flare_template(&mut game);
        let mut volley = rules();
        volley.flare = "Flare".into();
        volley.evasion_rate = 1.0;
        volley.reaction_delay = 3;
        volley.decoy_delay = 6;
        volley.volley_delay = 30;
        game.catalog[6].behavior.countermeasures.push(volley);
        crate::skirmish::tests::register_update(&mut game.catalog[6], "CountermeasuresBehavior");
        let id = game.spawn(6, 0, [500.0, 300.0], true);
        game.entities[id].behaviors.countermeasures[0].active = true;
        game.set_object_position(id, [500.0, 300.0, 100.0]);
        game.entities[id].set_airborne_target(true);
        game.tick = 100;
        (game, id)
    }

    #[test]
    fn weapon_launch_keeps_reaction_volley_and_missile_diversion_deadlines_separate() {
        let (mut game, aircraft) = countermeasure_aircraft();
        let destination = game.object_position(aircraft);
        let serial = crate::skirmish::point_defense::tests::missile_targeted(
            &mut game,
            [100.0, 300.0, 100.0],
            1,
            1.0,
            Some(aircraft),
            destination,
        );
        let missile = missile_object(&game, serial);
        let state = &game.entities[aircraft].behaviors.countermeasures[0];
        assert_eq!(
            (state.incoming, state.diverted, state.reaction),
            (1, 1, 103)
        );
        assert_eq!(state.active_count, 0);
        game.tick = 102;
        game.update_countermeasures(aircraft, 0);
        assert!(
            game.entities[aircraft].behaviors.countermeasures[0]
                .flares
                .is_empty()
        );
        game.tick = 103;
        game.update_countermeasures(aircraft, 0);
        let state = &game.entities[aircraft].behaviors.countermeasures[0];
        assert_eq!(
            (state.available, state.active_count, state.next_volley),
            (4, 2, 133)
        );
        let flare = game.countermeasure_target(aircraft).unwrap();
        let registration = game.entities[missile].ai.registration.unwrap();
        game.tick = 105;
        game.update_missile_module(missile, registration);
        assert_eq!(
            game.projectiles[game.projectile_index(serial).unwrap()].target,
            Some(aircraft)
        );
        game.tick = 106;
        game.update_missile_module(missile, registration);
        assert_eq!(
            game.projectiles[game.projectile_index(serial).unwrap()].target,
            Some(flare)
        );
        for frame in [133, 163] {
            game.tick = frame;
            game.update_countermeasures(aircraft, 0);
        }
        let state = &game.entities[aircraft].behaviors.countermeasures[0];
        assert_eq!(
            (state.available, state.active_count, state.reload),
            (0, 6, 283)
        );
        game.tick = 282;
        game.update_countermeasures(aircraft, 0);
        assert_eq!(
            game.entities[aircraft].behaviors.countermeasures[0].available,
            0
        );
        game.tick = 283;
        game.update_countermeasures(aircraft, 0);
        assert_eq!(
            game.entities[aircraft].behaviors.countermeasures[0].available,
            6
        );
    }

    #[test]
    fn expired_flares_still_suppress_damage_when_the_diversion_deadline_arrives() {
        for active in [false, true] {
            let (mut game, aircraft) = countermeasure_aircraft();
            game.entities[aircraft].behaviors.countermeasures[0].active = active;
            let destination = game.object_position(aircraft);
            let serial = crate::skirmish::point_defense::tests::missile_targeted(
                &mut game,
                [100.0, 300.0, 100.0],
                1,
                1.0,
                Some(aircraft),
                destination,
            );
            let missile = missile_object(&game, serial);
            let weapon_type = game.projectiles[game.projectile_index(serial).unwrap()].weapon_type;
            game.catalog[weapon_type].weapon.area = Some(crate::skirmish::damage::AreaDamage {
                at_source: false,
                primary_radius: 5.0,
                secondary_radius: 0.0,
                secondary_damage: 0.0,
                affects: 2,
            });
            game.tick = 103;
            game.update_countermeasures(aircraft, 0);
            let flares = game.entities[aircraft].behaviors.countermeasures[0]
                .flares
                .clone();
            assert_eq!(flares.len(), if active { 2 } else { 0 });
            for flare in flares {
                game.destroy_maintenance_object(flare);
            }
            game.process_update_destruction();
            assert!(game.countermeasure_target(aircraft).is_none());
            game.tick = 106;
            game.update_missile_module(missile, game.entities[missile].ai.registration.unwrap());
            assert_eq!(
                game.projectiles[game.projectile_index(serial).unwrap()].target,
                Some(aircraft)
            );
            let crate::skirmish::projectiles::Control::Missile(control) = &game.entities[missile]
                .projectile_flight
                .as_ref()
                .unwrap()
                .controls[0]
            else {
                panic!("MissileAI control")
            };
            assert_eq!(control.deals_damage(), !active);
            // Contact must occur at the target: detonating at the distant launch
            // position would preserve its health even without damage suppression.
            game.set_object_position(missile, destination);
            let health = game.entities[aircraft].health;
            assert!(game.projectile_contact(missile, Some(aircraft)));
            assert!(game.presentation.weapon_fx.iter().any(|event| matches!(event,
                crate::skirmish::WeaponFxEvent::Projectile(event)
                    if event.projectile == serial && matches!(event.stage, crate::skirmish::ProjectileFxStage::Detonation)
            )));
            if active {
                assert_eq!(game.entities[aircraft].health.to_bits(), health.to_bits());
            } else {
                assert!(game.entities[aircraft].health < health);
            }
        }
    }

    #[test]
    fn diversion_uses_nearest_aircraft_distance_among_newest_surviving_volley() {
        let (mut game, aircraft) = countermeasure_aircraft();
        game.launch_countermeasure_volley(aircraft, 0);
        game.launch_countermeasure_volley(aircraft, 0);
        let flares = game.entities[aircraft].behaviors.countermeasures[0]
            .flares
            .clone();
        for (&flare, x) in flares.iter().zip([501.0, 504.0, 530.0, 505.0]) {
            game.set_object_position(flare, [x, 300.0, 100.0]);
        }
        assert_eq!(game.countermeasure_target(aircraft), Some(flares[3]));
        game.destroy_maintenance_object(flares[3]);
        game.process_update_destruction();
        // The CRC-off count advances only for surviving objects, so the previous
        // volley's last surviving flare enters the two-flare search window.
        assert_eq!(game.countermeasure_target(aircraft), Some(flares[1]));
        game.update_countermeasures(aircraft, 0);
        assert_eq!(
            game.entities[aircraft].behaviors.countermeasures[0].active_count,
            3
        );
    }
}
