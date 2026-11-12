//! Locomotor constructor and `TWO_LEGS` side-to-side wandering.
use crate::skirmish::startup;
use std::f32::consts::PI;

pub(in crate::skirmish) struct State {
    offset: f32,
    increment: f32,
    increasing: bool,
}

impl State {
    pub(in crate::skirmish) fn new(random: &mut [u32; 6], length: f32) -> Self {
        let offset = startup::random_real(random, -PI / 6.0, PI / 6.0);
        let increment = (PI / 40.0) * (startup::random_real(random, 0.8, 1.2) / length);
        let increasing = startup::random_between(random, 1) != 0;
        Self {
            offset,
            increment,
            increasing,
        }
    }

    pub(super) fn angle(&mut self, desired: f32, speed: f32, width: f32) -> f32 {
        let limit = PI / 8.0 * width;
        let change = self.increment * speed;
        if self.increasing {
            self.offset += change;
            if self.offset > limit {
                self.increasing = false;
            }
        } else {
            self.offset -= change;
            if self.offset < -limit {
                self.increasing = true;
            }
        }
        super::normalize_angle(desired + self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_matches_extracted_native_bits_and_rng_stream() {
        let mut seed = None;
        let mut random = [0; 6];
        for line in include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/reference/locomotor-rng-reference.txt"
        ))
        .lines()
        {
            let columns: Vec<_> = line.split_whitespace().collect();
            let row_seed = columns[0].parse::<u32>().unwrap();
            if seed != Some(row_seed) {
                random = startup::seed_random(row_seed);
                seed = Some(row_seed);
            }
            let state = State::new(&mut random, columns[1].parse().unwrap());
            assert_eq!(state.offset.to_bits(), columns[2].parse::<u32>().unwrap());
            assert_eq!(
                state.increment.to_bits(),
                columns[3].parse::<u32>().unwrap()
            );
            assert_eq!(
                u32::from(state.increasing),
                columns[4].parse::<u32>().unwrap()
            );
            let expected: Vec<u32> = columns[5..]
                .iter()
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(random.as_slice(), expected);
        }
    }

    fn assert_state(actual: &State, expected: &State) {
        assert_eq!(actual.offset.to_bits(), expected.offset.to_bits());
        assert_eq!(actual.increment.to_bits(), expected.increment.to_bits());
        assert_eq!(actual.increasing, expected.increasing);
    }

    #[test]
    fn native_ground_and_flight_templates_draw_even_without_wander_width() {
        use crate::skirmish::{flight, tests::game};
        for flying in [false, true] {
            let mut game = game();
            let template = &mut game.catalog[5].locomotor;
            template.wander_length = Some(4.0);
            if !flying {
                template.ground = Some(super::super::GroundRules {
                    wander: None,
                    appearance: super::super::GroundAppearance::Legs,
                    is_dozer: false,
                    downhill_only: false,
                    acceleration: 100.0 / 900.0,
                    acceleration_damaged: 50.0 / 900.0,
                    braking: 100.0 / 900.0,
                    min_speed: 0.0,
                    close_enough: 1.0,
                    mass: 50.0,
                    forward_friction: 0.15 / 30.0,
                    lateral_friction: 0.15 / 30.0,
                    extra_friction: 0.0,
                });
            }
            if flying {
                template.flight = Some(flight::tests::rules(flight::Appearance::Wings));
                template.legal_surfaces = 8;
            }
            let mut expected = game.random;
            let state = State::new(&mut expected, 4.0);
            // AIStates.cpp691–693 constructs three child Idle machines;
            // AIUpdate.cpp630 then enters primary Idle.
            for _ in 0..4 {
                crate::skirmish::startup::random_between(&mut expected, 60);
            }
            let id = game.spawn(5, 0, [300.0, 400.0], true);
            assert_eq!(game.random, expected);
            assert_state(game.entities[id].ai.motion.wander.as_ref().unwrap(), &state);
        }
    }

    #[test]
    fn absent_ai_or_native_template_does_not_construct_a_locomotor() {
        use crate::skirmish::{UnitAiUpdate, tests::game};
        for absent_ai in [false, true] {
            let mut game = game();
            if absent_ai {
                let unit = &mut game.catalog[5];
                unit.ai.update = UnitAiUpdate::Absent;
                unit.update_modules.retain(|module| {
                    !matches!(module,
                    crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. }
                    if family == "AIUpdateInterface")
                });
                unit.behavior_modules
                    .retain(|module| module.family != "AIUpdateInterface");
            } else {
                game.catalog[5].locomotor.wander_length = None;
            }
            let mut expected = game.random;
            if !absent_ai {
                // The three child Idle constructors and primary Idle entry
                // do not require a Locomotor template to be present.
                for _ in 0..4 {
                    crate::skirmish::startup::random_between(&mut expected, 60);
                }
            }
            let id = game.spawn(5, 0, [300.0, 400.0], true);
            assert_eq!(game.random, expected);
            assert!(game.entities[id].ai.motion.wander.is_none());
            assert!(game.entities[id].ai.surface_motion.is_empty());
        }
    }

    #[test]
    fn role_recreation_draws_only_selected_templates_in_order_and_preserves_physics() {
        use crate::skirmish::{
            Faction, Kind, LocomotorVariant,
            tests::{fixture_type, game},
        };
        let mut game = game();
        let mut secondary = fixture_type(Faction::Usa, Kind::Infantry).locomotor;
        secondary.wander_length = Some(2.0);
        secondary.legal_surfaces = 4;
        game.catalog[5]
            .locomotor_sets
            .normal
            .push(LocomotorVariant {
                speed: 20.0,
                locomotor: secondary,
            });
        for length in [4.0, 8.0] {
            let mut template = fixture_type(Faction::Usa, Kind::Infantry).locomotor;
            template.wander_length = Some(length);
            game.catalog[5]
                .locomotor_sets
                .wander
                .push(LocomotorVariant {
                    speed: 10.0,
                    locomotor: template,
                });
        }
        let mut expected = game.random;
        let primary = State::new(&mut expected, 1.0);
        let secondary = State::new(&mut expected, 2.0);
        // All selected Locomotor constructors precede the three child Idle
        // constructors and primary Idle entry in AIUpdate::onObjectCreated.
        for _ in 0..4 {
            crate::skirmish::startup::random_between(&mut expected, 60);
        }
        let id = game.spawn(5, 0, [300.0, 400.0], true);
        assert_eq!(game.random, expected);
        assert_eq!(game.entities[id].ai.surface_motion.len(), 2);
        assert_state(
            game.entities[id].ai.motion.wander.as_ref().unwrap(),
            &primary,
        );
        assert_state(
            game.entities[id].ai.surface_motion[1]
                .wander
                .as_ref()
                .unwrap(),
            &secondary,
        );
        game.entities[id].ai.motion.velocity = [3.0, -2.0];
        game.entities[id].ai.motion.braking_factor = 0.25;
        game.tick = 50;
        let first_wander = State::new(&mut expected, 4.0);
        let second_wander = State::new(&mut expected, 8.0);
        game.choose_locomotor_set(id, crate::skirmish::locomotor::set::WANDER);
        assert_eq!(game.random, expected);
        assert_state(
            game.entities[id].ai.motion.wander.as_ref().unwrap(),
            &first_wander,
        );
        assert_state(
            game.entities[id].ai.surface_motion[1]
                .wander
                .as_ref()
                .unwrap(),
            &second_wander,
        );
        assert_eq!(game.entities[id].ai.motion.donut_until, 125);
        assert_eq!(
            game.entities[id].ai.motion.braking_factor.to_bits(),
            1.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[id].ai.motion.velocity.map(f32::to_bits),
            [3.0_f32, -2.0].map(f32::to_bits)
        );
        game.tick = 60;
        game.choose_locomotor_set(id, crate::skirmish::locomotor::set::WANDER);
        assert_eq!(game.random, expected);
        assert_eq!(game.entities[id].ai.motion.donut_until, 125);
        let recreated_primary = State::new(&mut expected, 1.0);
        let recreated_secondary = State::new(&mut expected, 2.0);
        game.choose_locomotor_set(id, crate::skirmish::locomotor::set::NORMAL);
        assert_eq!(game.random, expected);
        assert_state(
            game.entities[id].ai.motion.wander.as_ref().unwrap(),
            &recreated_primary,
        );
        assert_state(
            game.entities[id].ai.surface_motion[1]
                .wander
                .as_ref()
                .unwrap(),
            &recreated_secondary,
        );
        assert_eq!(game.entities[id].ai.motion.donut_until, 135);
    }

    #[test]
    fn changing_surface_keeps_constructor_state_and_does_not_redraw() {
        use crate::skirmish::{
            Faction, Kind, LocomotorVariant,
            tests::{fixture_type, game},
        };
        use crate::{Cell, terrain::TerrainMap};
        let mut game = game();
        let mut cliff = fixture_type(Faction::Gla, Kind::Infantry).locomotor;
        cliff.legal_surfaces = 4;
        cliff.wander_length = Some(2.0);
        game.catalog[5]
            .locomotor_sets
            .normal
            .push(LocomotorVariant {
                speed: 20.0,
                locomotor: cliff,
            });
        let terrain = TerrainMap::classify(80, 60, (0..60).map(|y| Cell { x: 35, y }), []).unwrap();
        game.configure_movement_terrain(terrain).unwrap();
        let id = game.spawn(5, 0, [300.0, 400.0], true);
        game.entities[id].ai.motion.wander.as_mut().unwrap().offset = 0.375;
        game.entities[id].ai.motion.donut_until = 123;
        let expected = game.random;
        game.entities[id].position = [350.0, 400.0];
        game.select_surface_locomotor(id);
        assert_eq!(game.entities[id].ai.surface_locomotor, 1);
        game.entities[id].position = [300.0, 400.0];
        game.select_surface_locomotor(id);
        assert_eq!(game.entities[id].ai.surface_locomotor, 0);
        assert_eq!(game.random, expected);
        assert_eq!(
            game.entities[id]
                .ai
                .motion
                .wander
                .as_ref()
                .unwrap()
                .offset
                .to_bits(),
            0.375_f32.to_bits()
        );
        assert_eq!(game.entities[id].ai.motion.donut_until, 123);
        game.entities[id].position = [350.0, 400.0];
        game.select_surface_locomotor(id);
        let mut expected = game.random;
        let ground = State::new(&mut expected, 1.0);
        let cliff = State::new(&mut expected, 2.0);
        game.initialize_locomotor_set(id);
        assert_eq!(game.random, expected);
        assert_eq!(game.entities[id].ai.surface_locomotor, 1);
        assert_eq!(game.entities[id].ai.surface_motion.len(), 2);
        assert_state(game.entities[id].ai.motion.wander.as_ref().unwrap(), &cliff);
        assert_state(
            game.entities[id].ai.surface_motion[0]
                .wander
                .as_ref()
                .unwrap(),
            &ground,
        );
    }

    #[test]
    fn upgraded_normal_recreates_its_own_template_after_wander() {
        use crate::skirmish::{
            Faction, Kind, LocomotorVariant, MovementUpgrade,
            tests::{fixture_type, game},
        };
        let mut game = game();
        let mut shoes = fixture_type(Faction::Gla, Kind::Builder).locomotor;
        shoes.wander_length = Some(2.0);
        game.catalog[5].maintenance.movement_upgrade = Some(MovementUpgrade {
            trigger: "Shoes".into(),
            speed: 30.0,
            locomotor: shoes,
        });
        let mut wander = fixture_type(Faction::Gla, Kind::Builder).locomotor;
        wander.wander_length = Some(4.0);
        game.catalog[5]
            .locomotor_sets
            .wander
            .push(LocomotorVariant {
                speed: 10.0,
                locomotor: wander,
            });
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[5],
            "LocomotorSetUpgrade",
            0x80,
        );
        game.catalog[5]
            .behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers = vec!["Shoes".into()];
        let id = game.spawn(5, 0, [300.0, 400.0], true);
        game.choose_locomotor_set(id, crate::skirmish::locomotor::set::WANDER);
        let before_upgrade = game.random;
        game.players[0].upgrades.push("Shoes".into());
        game.activate_upgrade_modules(id);
        assert_eq!(game.random, before_upgrade);
        let mut expected = game.random;
        let upgraded = State::new(&mut expected, 2.0);
        game.choose_locomotor_set(id, crate::skirmish::locomotor::set::NORMAL);
        assert_eq!(game.random, expected);
        assert_eq!(game.entities[id].ai.surface_motion.len(), 1);
        assert_state(
            game.entities[id].ai.motion.wander.as_ref().unwrap(),
            &upgraded,
        );
        assert_eq!(
            game.entities[id]
                .active_locomotor(&game.catalog[5])
                .wander_length
                .map(f32::to_bits),
            Some(2.0_f32.to_bits())
        );
    }

    #[test]
    fn new_role_uses_ground_fallback_instead_of_first_unusable_template() {
        use crate::skirmish::{
            Faction, Kind, LocomotorVariant,
            tests::{fixture_type, game},
        };
        use crate::{Cell, terrain::TerrainMap};
        let mut game = game();
        for surfaces in [4, 1] {
            let mut locomotor = fixture_type(Faction::Gla, Kind::Infantry).locomotor;
            locomotor.legal_surfaces = surfaces;
            game.catalog[5]
                .locomotor_sets
                .wander
                .push(LocomotorVariant {
                    speed: 10.0,
                    locomotor,
                });
        }
        game.configure_movement_terrain(
            TerrainMap::classify(80, 60, [], (0..60).map(|y| Cell { x: 35, y })).unwrap(),
        )
        .unwrap();
        let id = game.spawn(5, 0, [350.0, 400.0], true);
        game.choose_locomotor_set(id, crate::skirmish::locomotor::set::WANDER);
        assert_eq!(game.entities[id].ai.surface_locomotor, 1);
        assert_eq!(
            game.entities[id]
                .active_locomotor(&game.catalog[5])
                .legal_surfaces,
            1
        );
    }

    #[test]
    fn sideways_offset_reverses_only_after_strict_limit_and_uses_signed_forward_speed() {
        let mut state = State {
            offset: PI / 8.0,
            increment: 0.25,
            increasing: true,
        };
        let at_limit = state.angle(0.0, 0.0, 1.0);
        assert_eq!(at_limit.to_bits(), (PI / 8.0).to_bits());
        assert!(state.increasing);
        state.angle(0.0, 1.0, 1.0);
        assert!(!state.increasing);
        let previous = state.offset;
        state.angle(0.0, -1.0, 1.0);
        assert_eq!(state.offset.to_bits(), (previous + 0.25).to_bits());
        state.offset = -PI / 8.0;
        state.angle(0.0, 0.0, 1.0);
        assert!(!state.increasing);
        state.angle(0.0, 1.0, 1.0);
        assert!(state.increasing);
    }
}
