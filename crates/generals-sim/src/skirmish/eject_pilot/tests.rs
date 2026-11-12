use super::*;
use crate::skirmish::{
    DeathType, Faction, Kind, StartingVeterancy, WeaponFxEvent,
    object_creation::{self, Object},
    physical_ocl,
    tests::{self, fixture_type, register_behavior},
};

fn list(airborne: bool) -> List {
    List {
        name: if airborne { "EjectAir" } else { "EjectGround" }.into(),
        objects: vec![Object {
            names: vec!["FixturePilot".into()],
            count: 1,
            offset: [0.0; 3],
            disposition: physical_ocl::RANDOM_FORCE,
            health: [1.0; 2],
            lifetime: [0; 2],
            flags: object_creation::REQUIRES_LIVE_PLAYER
                | object_creation::IGNORE_PRIMARY_OBSTACLE
                | object_creation::INHERITS_VETERANCY,
            container: if airborne { "FixtureChute" } else { "" }.into(),
            invulnerable_time: 60,
            force_intensity: 0.0,
            force_magnitude: if airborne { [10.0, 12.0] } else { [2.0, 3.0] },
            force_pitch: [50.0_f32.to_radians(), 60.0_f32.to_radians()],
            spin_rate: 0.0,
        }],
    }
}

fn pilot_physics(pilot: &mut UnitType) {
    // Original AmericaInfantryPilot has ground locomotion and Physics Mass=5.
    let gravity = -64.0 / 900.0;
    pilot.live_physics = Some(physical_ocl::LiveRules {
        motion: physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 5.0,
            gravity,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: physical_ocl::CollisionRules {
            stiffness: 0.5,
            rubble_height: 5.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.05, 0.025, 0.025],
        min_fall_speed: (2.0_f32 * gravity * 40.0).abs().sqrt(),
        fall_damage_factor: 1.0,
    });
    tests::register_update(pilot, "PhysicsBehavior");
}

fn fixture() -> (Skirmish, usize) {
    let (mut game, chute_type) = crate::skirmish::parachute::tests::fixture_catalog();
    game.catalog[chute_type].name = "FixtureChute".into();
    let pilot = &mut game.catalog[5];
    pilot.name = "FixturePilot".into();
    pilot.veterancy.trainable = true;
    pilot.veterancy.required = [0, 1, 2, 3];
    pilot.veterancy.starting.push(StartingVeterancy {
        level: 1,
        science: None,
    });
    register_behavior(pilot, "VeterancyGainCreate", 8);
    pilot_physics(pilot);
    let mut parent = fixture_type(Faction::Usa, Kind::Tank);
    let mut conditions = tests::death_conditions();
    conditions.veterancy_mask = 14;
    conditions.exempt_status = 1 << 10;
    parent.behavior.eject_pilots.push(Rules {
        conditions,
        ground: Some(list(false)),
        airborne: Some(list(true)),
        sounds: ["PilotVoiceEject".into(), "PilotSoundEject".into()],
    });
    register_behavior(&mut parent, "EjectPilotDie", 2);
    parent.behavior.destroy_die.push(tests::death_conditions());
    register_behavior(&mut parent, "DestroyDie", 2);
    let kind = game.catalog.len();
    game.catalog.push(parent);
    let source = game.spawn(kind, 0, [355.0, 255.0], true);
    (game, source)
}

#[test]
fn actual_death_chooses_ground_at_significant_height_boundary_and_air_above_it() {
    for airborne in [false, true] {
        let (mut game, source) = fixture();
        game.set_hijacker_veterancy(source, 2, false);
        let boundary = -9.0 * game.gravity;
        let height = if airborne { boundary + 1.0 } else { boundary };
        game.set_object_position(source, [355.0, 255.0, height]);
        let first = game.entities.len();
        game.kill_object(source, DeathType::Normal);
        let pilot = first + usize::from(airborne);
        assert_eq!(
            game.catalog[game.entities[pilot].type_index].name,
            "FixturePilot"
        );
        assert_eq!(game.entities[pilot].producer, Some(source));
        assert_eq!(game.entities[pilot].veterancy, 2);
        assert_eq!(game.entities[pilot].owner, 0);
        assert!(game.entities[pilot].helpers.defection.undetected);
        assert!(game.entities[pilot].physics_ignores(source));
        if airborne {
            assert_eq!(
                game.catalog[game.entities[first].type_index].name,
                "FixtureChute"
            );
            assert_eq!(game.entities[pilot].contained_by(), Some(first));
            assert!(game.entities[pilot].parachute_rider());
        } else {
            assert!(game.entities[pilot].contained_by().is_none());
        }
        assert_eq!(
            game.presentation
                .object_sounds
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["PilotVoiceEject", "PilotSoundEject"]
        );
        assert!(matches!(
            game.presentation.object_sounds[0].action,
            Action::PlayAtPosition
        ));
        assert!(matches!(
            game.presentation.object_sounds[1].action,
            Action::PlayAtPositionUnowned
        ));
        game.process_update_destruction();
        assert!(!game.entities[source].exists());
        let sounds = game
            .presentation
            .weapon_fx
            .iter()
            .position(|event| matches!(event, WeaponFxEvent::ObjectSound(_)))
            .unwrap();
        let destroyed = game
            .presentation
            .weapon_fx
            .iter()
            .position(
                |event| matches!(event, WeaponFxEvent::DrawableDestroyed(id) if *id == source),
            )
            .unwrap();
        assert!(sounds < destroyed);
        assert!(game.entities[pilot].exists());
        if !airborne {
            // OCL applies force; the ordinary scheduled Physics callback integrates it.
            game.step();
            assert!(game.entities[pilot].physics_velocity()[2] > 0.0);
            game.tick = 59;
            game.run_object_updates();
            assert!(game.entities[pilot].helpers.defection.undetected);
            game.tick = 60;
            game.run_object_updates();
            assert!(!game.entities[pilot].helpers.defection.undetected);
        }
    }
}

#[test]
fn regular_and_actual_hijacked_status_reject_ejection_but_module_presence_remains() {
    for hijacked in [false, true] {
        let (mut game, source) = fixture();
        if hijacked {
            game.set_hijacker_veterancy(source, 2, false);
            game.entities[source].crew.hijacked = true;
            assert_eq!(game.object_status_bits(source) & (1 << 10), 1 << 10);
        }
        assert!(game.catalog[game.entities[source].type_index].has_eject_pilot());
        let count = game.entities.len();
        let random = game.random;
        game.kill_object(source, DeathType::Normal);
        assert_eq!(game.entities.len(), count);
        assert!(game.presentation.object_sounds.is_empty());
        assert_eq!(game.random, random);
    }
}

#[test]
fn defeated_player_skips_real_ocl_creation_but_retains_native_ejection_sounds() {
    let (mut game, source) = fixture();
    game.set_hijacker_veterancy(source, 2, false);
    game.defeated[0] = true;
    let count = game.entities.len();
    let random = game.random;
    game.kill_object(source, DeathType::Normal);
    assert_eq!(game.entities.len(), count);
    assert_eq!(game.random, random);
    assert_eq!(game.presentation.object_sounds.len(), 2);
}

#[test]
fn ordered_eject_instances_continue_past_destroy_die_before_actual_destructor() {
    let (mut game, source) = fixture();
    let kind = game.entities[source].type_index;
    // Author the extra instance before constructing the source Object.
    game.destroy_maintenance_object(source);
    game.process_update_destruction();
    game.catalog[kind].behavior.eject_pilots.push(Rules {
        conditions: tests::death_conditions(),
        ground: Some(list(false)),
        airborne: None,
        sounds: ["SecondVoice".into(), "SecondSound".into()],
    });
    register_behavior(&mut game.catalog[kind], "EjectPilotDie", 2);
    let source = game.spawn(kind, 0, [355.0, 255.0], true);
    game.set_hijacker_veterancy(source, 2, false);
    let first = game.entities.len();
    game.presentation.weapon_fx.clear();
    game.kill_object(source, DeathType::Normal);
    assert_eq!(game.entities.len(), first + 2);
    assert!(
        game.entities[first].producer == Some(source)
            && game.entities[first + 1].producer == Some(source)
    );
    assert_eq!(
        game.presentation
            .object_sounds
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        [
            "PilotVoiceEject",
            "PilotSoundEject",
            "SecondVoice",
            "SecondSound"
        ]
    );
    assert!(game.update_objects[game.entities[source].update_owner].destroy_requested);
    assert!(
        !game
            .presentation
            .weapon_fx
            .iter()
            .any(|event| matches!(event, WeaponFxEvent::DrawableDestroyed(id) if *id == source))
    );
    game.process_update_destruction();
    assert!(game.entities[first].exists() && game.entities[first + 1].exists());
}
