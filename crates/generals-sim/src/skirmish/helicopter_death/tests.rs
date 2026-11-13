use super::*;
use crate::skirmish::{
    DeathType, Faction, Kind,
    aircraft_death::tests::{install_physics, install_slow},
    flight, physical_ocl,
    tests::{fixture_type, game},
};

fn fixture() -> (Skirmish, usize) {
    let mut game = game();
    game.tick = 40;
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut hull = fixture_type(Faction::China, Kind::Effect);
    hull.name = "HelixRubbleHull".into();
    hull.kind_of = vec!["IMMOBILE".into(), "NO_COLLIDE".into(), "HULK".into()];
    hull.body_kind = crate::skirmish::BodyKind::Inactive;
    hull.behavior.lifetimes = vec![[600; 2]];
    crate::skirmish::tests::register_update(&mut hull, "LifetimeUpdate");
    hull.initial_health = 0.0;
    hull.max_health = 0.0;
    hull.physical = Some(physical_ocl::Rules {
        motion: crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 45.0,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        kill_when_resting: true,
        active_body: false,
        collision: collision(),
        death: physical_ocl::Death::Slow(physical_ocl::SlowDeath {
            probability: 10,
            sink_delay: [45, 0],
            destruction_delay: [480, 0],
            sink_rate: 1.0 / 30.0,
        }),
    });
    game.catalog.push(hull);
    let unit = &mut game.catalog[kind];
    unit.locomotor.ground = None;
    unit.locomotor.flight = Some(flight::tests::rules(flight::Appearance::Hover));
    unit.locomotor.works_when_dead = true;
    unit.locomotor.legal_surfaces = 8;
    unit.preferred_height = 100.0;
    game.gravity = unit.locomotor.flight.as_ref().unwrap().gravity;
    unit.movement_surface = crate::skirmish::MovementSurface::Air;
    let rule = Rules {
        orbit_turn: 80.0_f32.to_radians() / 30.0,
        orbit_speed: 110.0 / 30.0,
        damping: 0.9999,
        spin: [40.0_f32.to_radians() / 30.0, 120.0_f32.to_radians() / 30.0],
        spin_delay: 9.0,
        spin_step: 5.0_f32.to_radians(),
        fall: 0.08,
        blade_delay: [30.0, 30.0],
        final_delay: 0.9,
        phases: std::array::from_fn(|index| Phase {
            effect: format!("HelicopterPhase{index}"),
            objects: None,
        }),
        rubble: "HelixRubbleHull".into(),
        loop_sound: "HelixDamagedLoop".into(),
        braking: 210.0 / 900.0,
        particle: "SootySmokeTrail".into(),
        particle_bone: "Propeller02".into(),
        particle_point: [0.0, 0.0, 10.0],
        blade_bone: "Propeller01".into(),
        blade_point: Some([0.0, 0.0, 12.0]),
        eject_pilot: None,
        eject_sounds: [String::new(), String::new()],
    };
    install_slow(
        unit,
        "HelicopterSlowDeathBehavior",
        crate::skirmish::death_lifecycle::Derived::Helicopter(Box::new(rule)),
    );
    install_physics(unit);
    let id = game.spawn(kind, 0, [300.0, 300.0], true);
    (game, id)
}

fn collision() -> physical_ocl::CollisionRules {
    physical_ocl::CollisionRules {
        stiffness: 0.5,
        rubble_height: 5.0,
        allow_force: true,
        weapons: [None, None],
    }
}

#[test]
fn dying_helicopter_keeps_physics_and_source_thirty_callback_blade_delay() {
    let (mut game, id) = fixture();
    game.set_flight_position(id, [300.0, 300.0, 500.0]);
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    assert!(game.entities[id].helicopter_corpse());
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.2])
            .contains("RUBBLE")
    );
    for _ in 0..29 {
        game.step();
    }
    assert!(game.entities[id].helicopter_corpse());
    assert!(
        !game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.effect == "HelicopterPhase0")
    );
    let before = game.object_position(id);
    game.step();
    let event = game
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.effect == "HelicopterPhase0")
        .unwrap();
    assert!((event.position[2] - game.object_position(id)[2] - 12.0).abs() < 0.001);
    assert!(game.object_position(id)[2] < before[2]);
    assert_ne!(
        game.entities[id].position.map(f32::to_bits),
        [300.0_f32, 300.0].map(f32::to_bits)
    );
}

#[test]
fn ground_impact_holds_the_body_then_creates_the_original_hull_after_strict_real_delay() {
    let (mut game, id) = fixture();
    game.set_flight_position(id, [300.0, 300.0, 0.5]);
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    game.step();
    assert!(game.entities[id].helicopter_hit_ground());
    assert!(!game.entities[id].masked());
    assert_eq!(game.entities[id].death_condition(), Some("SPECIAL_DAMAGED"));
    let position = game.object_position(id);
    let basis = game.entities[id].physical_basis().unwrap();
    game.step();
    assert!(game.entities[id].masked());
    let hull = game
        .entities
        .iter()
        .enumerate()
        .find(|(_, actor)| game.catalog[actor.type_index].name == "HelixRubbleHull")
        .map(|(id, _)| id)
        .unwrap();
    assert_eq!(
        game.object_position(hull).map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(
        game.entities[hull]
            .physical_basis()
            .unwrap()
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    assert!(
        game.presentation
            .object_fx_events
            .iter()
            .any(|event| event.effect == "HelicopterPhase2")
    );
}

#[test]
fn world_axis_spin_preserves_tilt_and_does_not_post_rotate_local_columns() {
    let mut matrix = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
    pre_rotate_z(&mut matrix, std::f32::consts::FRAC_PI_2);
    assert!((matrix[0][2] - 1.0).abs() < 0.000_001);
    assert!((matrix[1][0] - 1.0).abs() < 0.000_001);
    assert_eq!(
        matrix[2].map(f32::to_bits),
        [0.0_f32, 1.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn clearing_held_preserves_ground_death_phase_without_recreating_the_disable() {
    let (mut game, id) = fixture();
    let kind = game.entities[id].type_index;
    let Derived::Helicopter(rule) =
        &mut game.catalog[kind].death_lifecycle.as_mut().unwrap().slow[0].derived
    else {
        unreachable!()
    };
    rule.final_delay = 100.0;
    game.set_flight_position(id, [300.0, 300.0, 0.0]);
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    game.update_helicopter_slow_death(id, 0);
    assert!(game.entities[id].native_helicopter(0).ground_frame != 0);
    assert!(game.entities[id].disabled_held());
    game.clear_disabled_held(id);
    game.tick += 1;
    game.update_helicopter_slow_death(id, 0);
    assert!(game.entities[id].native_helicopter(0).ground_frame != 0);
    assert_eq!(game.object_disabled_mask(id) & (1 << 3), 0);
}

#[test]
fn spin_delay_is_strict_and_zero_blade_counter_never_fires_the_blade_event() {
    let (mut game, id) = fixture();
    let kind = game.entities[id].type_index;
    let Derived::Helicopter(rule) =
        &mut game.catalog[kind].death_lifecycle.as_mut().unwrap().slow[0].derived
    else {
        unreachable!()
    };
    rule.blade_delay = [0.0; 2];
    rule.spin_delay = 40.0;
    let initial = rule.spin[0];
    let changed = initial + rule.spin_step / 30.0;
    game.set_object_position(id, [300.0, 300.0, 500.0]);
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    game.update_helicopter_slow_death(id, 0);
    assert_eq!(
        game.entities[id].native_helicopter(0).spin.to_bits(),
        initial.to_bits()
    );
    game.tick += 1;
    game.update_helicopter_slow_death(id, 0);
    assert_eq!(
        game.entities[id].native_helicopter(0).spin.to_bits(),
        changed.to_bits()
    );
    assert_eq!(game.entities[id].native_helicopter(0).blade_frames, 0);
    assert!(
        !game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.effect == "HelicopterPhase0")
    );
    assert_eq!(game.presentation.attached_particles.len(), 1);
    assert!(matches!(
        game.presentation.object_sounds[0].action,
        Action::StopAmbient
    ));
}

#[test]
fn missing_pristine_blade_bone_transforms_cached_position_as_source_inout_parameter() {
    let (mut game, id) = fixture();
    let kind = game.entities[id].type_index;
    let Derived::Helicopter(rule) =
        &mut game.catalog[kind].death_lifecycle.as_mut().unwrap().slow[0].derived
    else {
        unreachable!()
    };
    rule.blade_point = None;
    rule.blade_delay = [1.0; 2];
    rule.spin = [0.0; 2];
    rule.spin_delay = 0.0;
    game.set_object_position(id, [300.0, 300.0, 500.0]);
    game.set_object_orientation(id, 0.0);
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    game.update_helicopter_slow_death(id, 0);
    let blade = game
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.effect == "HelicopterPhase0")
        .unwrap();
    assert_eq!(
        blade.position.map(f32::to_bits),
        [600.0_f32, 600.0, 1000.0].map(f32::to_bits)
    );
}

#[test]
fn helicopter_crash_uses_highest_bridge_layer_even_when_already_below_deck() {
    for height in [80.5, 40.0] {
        let (mut game, helicopter) = fixture();
        let layer = game
            .movement_terrain
            .add_bridge(
                crate::terrain::BridgeGeometry {
                    from: [250.0, 300.0, 80.0],
                    to: [350.0, 300.0, 80.0],
                    corners: [
                        [250.0, 325.0, 80.0],
                        [250.0, 275.0, 80.0],
                        [350.0, 325.0, 80.0],
                        [350.0, 275.0, 80.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        game.set_object_position(helicopter, [300.0, 300.0, 100.0]);
        game.apply_typed_hit(helicopter, helicopter, 999_999.0, DeathType::Normal);
        game.set_object_position(helicopter, [300.0, 300.0, height]);
        game.update_helicopter_slow_death(helicopter, 0);
        assert_eq!(game.entities[helicopter].layer, layer);
        assert!(game.entities[helicopter].helicopter_hit_ground());
        assert!(
            game.presentation
                .object_fx_events
                .iter()
                .any(|event| event.effect == "HelicopterPhase1")
        );
        assert!(game.entities[helicopter].disabled_status & (1 << 3) != 0);
    }
}

#[test]
fn dying_helicopter_hits_logical_map_shrubbery_and_retires_at_impact() {
    use crate::{
        collision_geometry::{Geometry, Shape},
        skirmish::MapWorldObject,
    };

    let (mut game, helicopter) = fixture();
    game.layout.world_objects.push(MapWorldObject {
        template: "MapShrubbery".into(),
        kind_of: vec!["IMMOBILE".into(), "SHRUBBERY".into()],
        position: [300.0, 300.0, 0.0],
        angle: 0.0,
        geometry: Geometry {
            shape: Shape::Cylinder,
            is_small: true,
            major: 4.0,
            minor: 4.0,
            height: 15.0,
        },
        original_team: "teamCivilian".into(),
    });
    game.observe_map_partition(0);
    game.update_ground_contacts();
    game.set_object_position(helicopter, [300.0, 300.0, 100.0]);
    game.apply_typed_hit(helicopter, helicopter, 999_999.0, DeathType::Normal);
    game.set_object_position(helicopter, [300.0, 300.0, 10.0]);
    game.update_ground_contacts();
    game.tick += 1;
    game.update_helicopter_slow_death(helicopter, 0);
    assert!(game.entities[helicopter].helicopter_hit_ground());
    assert!(game.entities[helicopter].disabled_held());
    let impact_height = game.object_position(helicopter)[2];
    assert!(impact_height > game.ground_height([300.0, 300.0]));

    game.tick += 1;
    game.update_helicopter_slow_death(helicopter, 0);
    game.process_update_destruction();
    assert!(!game.entities[helicopter].exists());
    let hull = game.entities.len() - 1;
    assert_eq!(
        game.catalog[game.entities[hull].type_index].name,
        "HelixRubbleHull"
    );
    assert_eq!(
        game.object_position(hull)[2].to_bits(),
        impact_height.to_bits()
    );
}
