use super::*;
use crate::skirmish::{DeathType, Kind, Skirmish, flight, tests::game};

mod collisions;

fn fixture() -> (Skirmish, usize) {
    let mut game = game();
    game.tick = 10;
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[kind];
    unit.locomotor.ground = None;
    unit.locomotor.flight = Some(flight::tests::rules(flight::Appearance::Wings));
    unit.locomotor.works_when_dead = true;
    unit.locomotor.legal_surfaces = 8;
    unit.preferred_height = 100.0;
    unit.movement_surface = crate::skirmish::MovementSurface::Air;
    install_slow(
        unit,
        "JetSlowDeathBehavior",
        crate::skirmish::death_lifecycle::Derived::Jet(Box::new(Jet {
            phases: std::array::from_fn(|index| Phase {
                effect: format!("JetPhase{index}"),
                objects: None,
            }),
            secondary_delay: 15,
            final_delay: 6,
            roll_rate: 0.2,
            roll_delta: 1.0,
            pitch_rate: 0.0,
            fall_speed: 1.1,
            loop_sound: "JetLoop".into(),
        })),
    );
    install_physics(unit);
    game.gravity = game.catalog[kind]
        .locomotor
        .flight
        .as_ref()
        .unwrap()
        .gravity;
    let id = game.spawn(kind, 0, [300.0, 300.0], true);
    game.set_object_position(id, [300.0, 300.0, 100.0]);
    (game, id)
}

#[test]
fn ground_jet_death_skips_slow_rng_and_destroys_after_its_ground_callback() {
    let (mut game, id) = fixture();
    game.set_flight_position(id, [300.0, 300.0, 0.0]);
    let random = game.random;
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
    assert!(!game.entities[id].jet_corpse());
    assert_eq!(game.random, random);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    assert_eq!(game.presentation.object_fx_events[0].effect, "JetPhase0");
    assert!(game.presentation.object_sounds.is_empty());
}

#[test]
fn airborne_jet_retains_its_body_and_source_roll_before_real_ground_final_retirement() {
    let (mut game, id) = fixture();
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    assert!(game.entities[id].jet_corpse() && game.entities[id].corpse_present());
    assert_eq!(game.entities[id].death_condition(), Some("DYING"));
    assert_eq!(game.presentation.object_fx_events[0].effect, "JetPhase1");
    assert_eq!(game.presentation.object_sounds[0].name, "JetLoop");
    let module_index = game.catalog[game.entities[id].type_index]
        .behavior_modules
        .iter()
        .position(|module| module.family == "JetSlowDeathBehavior")
        .unwrap();
    assert!(
        module_index > 0,
        "earlier non-death modules own distinct loop identities"
    );
    assert!(matches!(game.presentation.object_sounds[0].action,
        crate::skirmish::object_sounds::Action::PlayLoop(index) if index == module_index));
    assert!(!game.entities[id].alive());
    let start = game.object_position(id);
    game.update_jet_slow_death(id, 0);
    for _ in 0..5 {
        game.tick += 1;
        game.update_flight(id);
        game.integrate_live_physics(id);
        game.update_jet_slow_death(id, 0);
    }
    assert!(game.object_position(id)[2] < start[2]);
    let basis = game.entities[id].physical_basis().unwrap();
    assert_ne!(basis[1][2].to_bits(), 0.0_f32.to_bits());
    // First terrain contact and secondary timeout share a callback: impact comes first.
    game.tick = 25;
    game.set_flight_position(id, [300.0, 300.0, 0.0]);
    game.update_jet_slow_death(id, 0);
    let names: Vec<_> = game
        .presentation
        .object_fx_events
        .iter()
        .map(|event| event.effect.as_str())
        .collect();
    assert_eq!(names, ["JetPhase1", "JetPhase3", "JetPhase2"]);
    assert!(matches!(
        game.presentation.object_sounds[1].action,
        crate::skirmish::object_sounds::Action::StopLoop(index) if index == module_index
    ));
    assert!(game.entities[id].corpse_present());
    game.tick = 30;
    game.update_jet_slow_death(id, 0);
    assert!(!game.entities[id].masked());
    game.tick = 31;
    game.update_jet_slow_death(id, 0);
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "JetPhase4"
    );
    assert!(
        game.update_objects[game.entities[id].update_owner]
            .handles
            .is_empty()
    );
}

#[test]
fn airborne_slowdeath_keeps_native_timing_draws_and_dead_locomotor_applicability() {
    let (mut game, id) = fixture();
    let mut expected = game.random;
    // Native Real multiplication truncates 0.65f * 100000 to 64999.
    for upper in [9, 29_999] {
        crate::skirmish::startup::random_between(&mut expected, upper);
    }
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    assert_eq!(game.random, expected);
    game.tick += 1;
    let initial = game.object_position(id)[2];
    game.update_flight(id);
    game.integrate_live_physics(id);
    let accelerated = initial - game.object_position(id)[2];
    let (mut ordinary, other) = fixture();
    let kind = ordinary.entities[other].type_index;
    ordinary.catalog[kind].locomotor.works_when_dead = false;
    ordinary.apply_typed_hit(other, other, 999_999.0, DeathType::Normal);
    ordinary.tick += 1;
    ordinary.update_flight(other);
    ordinary.integrate_live_physics(other);
    let gravity_only = initial - ordinary.object_position(other)[2];
    assert!(accelerated > gravity_only && gravity_only > 0.0);
}

pub(in crate::skirmish) fn install_slow(
    unit: &mut crate::skirmish::UnitType,
    family: &str,
    derived: crate::skirmish::death_lifecycle::Derived,
) {
    use crate::skirmish::{death_callbacks::Conditions, death_lifecycle, tests::register_update};
    register_update(unit, family);
    let module = unit.behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    unit.death_lifecycle = Some(death_lifecycle::Rules {
        on_die: Vec::new(),
        physics: None,
        weapons: Vec::new(),
        slow: vec![death_lifecycle::SlowDeath {
            derived,
            tag: module.tag.clone(),
            conditions: Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
            probability: 10,
            overkill_bonus: 0.0,
            sink_delay: [0, 0],
            destruction_delay: [100_000, 0],
            sink_rate: 0.0,
            fling: None,
            fx: std::array::from_fn(|_| Vec::new()),
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        }],
    });
}

pub(in crate::skirmish) fn install_physics(unit: &mut crate::skirmish::UnitType) {
    let flight = unit.locomotor.flight.as_ref().unwrap();
    unit.live_physics = Some(physical_ocl::LiveRules {
        motion: physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: flight.mass,
            gravity: flight.gravity,
            stiffness: 0.5,
            forward_friction: flight.forward_friction,
            lateral_friction: flight.lateral_friction,
            aerodynamic_friction: flight.aerodynamic_friction,
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
        shock_rotation: [0.0; 3],
        min_fall_speed: 999.0,
        fall_damage_factor: 1.0,
    });
    crate::skirmish::tests::register_update(unit, "PhysicsBehavior");
}

#[test]
fn ordinary_die_interface_selects_derived_instance_and_only_wakes_its_authored_update() {
    use crate::skirmish::{
        death_lifecycle::Derived,
        module_updates::{FOREVER, ModuleRegistration},
    };
    let (mut game, original) = fixture();
    let kind = game.entities[original].type_index;
    let unit = &mut game.catalog[kind];
    let mut jet = unit.death_lifecycle.as_mut().unwrap().slow.remove(0);
    unit.behavior_modules
        .retain(|module| module.family != "JetSlowDeathBehavior");
    unit.update_modules.retain(|module|!matches!(module,ModuleRegistration::Behavior{family,..} if family=="JetSlowDeathBehavior"));
    install_slow(unit, "SlowDeathBehavior", Derived::None);
    unit.death_lifecycle.as_mut().unwrap().slow[0].probability = 1;
    crate::skirmish::tests::register_update(unit, "JetSlowDeathBehavior");
    let module = unit.behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    jet.tag = module.tag.clone();
    jet.probability = 1000;
    unit.death_lifecycle.as_mut().unwrap().slow.push(jet);
    let actor = game.new_object(kind, 0, true);
    game.set_object_position(actor, [400.0, 400.0, 100.0]);
    let registrations:Vec<_>=game.catalog[kind].update_modules.iter().enumerate().filter_map(|(index,module)|matches!(module,ModuleRegistration::Behavior{family,..} if matches!(family.as_str(),"SlowDeathBehavior"|"JetSlowDeathBehavior")).then_some(index)).collect();
    for &registration in &registrations {
        assert_eq!(
            game.update_remaining_sleep(actor, registration) + game.update_frame(),
            FOREVER
        );
    }
    let mut predicted = game.random;
    assert!(crate::skirmish::startup::random_between(&mut predicted, 1000) > 0);
    game.apply_typed_hit(actor, actor, 999_999.0, DeathType::Normal);
    let states = &game.entities[actor]
        .death_lifecycle
        .as_ref()
        .unwrap()
        .native;
    assert!(!states[0].active() && states[1].active());
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "JetPhase1"
    );
    assert_eq!(
        game.update_remaining_sleep(actor, registrations[0]) + game.update_frame(),
        FOREVER
    );
    assert_eq!(game.update_remaining_sleep(actor, registrations[1]), 1);
    let module_index = game.catalog[kind]
        .behavior_modules
        .iter()
        .position(|module| module.family == "JetSlowDeathBehavior")
        .unwrap();
    assert_ne!(
        module_index, 1,
        "SlowDeath slice index is not a global module identity"
    );
    assert!(matches!(
        game.presentation.object_sounds.last().unwrap().action,
        crate::skirmish::object_sounds::Action::PlayLoop(index) if index == module_index
    ));
}

#[test]
fn deck_ground_death_precedes_die_filter_and_clears_deck_status() {
    let (mut game, id) = fixture();
    let kind = game.entities[id].type_index;
    game.catalog[kind].death_lifecycle.as_mut().unwrap().slow[0]
        .conditions
        .death_mask = 0;
    game.entities[id].object_status |= 1 << 29;
    let random = game.random;
    game.apply_typed_hit(id, id, 999_999.0, DeathType::Normal);
    assert_eq!(game.entities[id].object_status & (1 << 29), 0);
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "JetPhase0"
    );
    assert_eq!(game.random, random);
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
}
