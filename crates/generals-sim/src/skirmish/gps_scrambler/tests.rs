use super::{Event, Rules};
use crate::skirmish::{Kind, Skirmish, tests};

fn gps_fixture() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].kind_of = vec!["VEHICLE".into()];
    let mut stealth = crate::skirmish::stealth::test_rules();
    stealth.innate = false;
    game.catalog[kind].behavior.stealth = Some(stealth);
    tests::register_update(&mut game.catalog[kind], "StealthUpdate");
    let marker_kind = game.catalog.len();
    let mut marker_type = tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Effect);
    marker_type.behavior.gps_scramblers.push(Rules {
        start_radius: 20.0,
        final_radius: 100.0,
        radius_per_frame: 10.0,
        kind_of: Some(vec!["VEHICLE".into(), "INFANTRY".into()]),
        radius_particle: Some("OriginalEmitter".into()),
    });
    tests::register_update(&mut marker_type, "GrantStealthBehavior");
    game.catalog.push(marker_type);
    (game, kind, marker_kind)
}

#[test]
fn gps_grows_in_ten_unit_steps_grants_allied_units_and_retires_after_final_scan() {
    let (mut game, kind, marker_kind) = gps_fixture();
    let near = game.spawn(kind, 0, [320.0, 300.0], true);
    let edge = game.spawn(kind, 0, [330.0, 300.0], true);
    let far = game.spawn(kind, 0, [399.0, 300.0], true);
    let beyond = game.spawn(kind, 0, [400.0, 300.0], true);
    let hostile = game.spawn(kind, 1, [300.0, 320.0], true);
    let marker = game.spawn(marker_kind, 0, [300.0, 300.0], true);
    // A regular logic tick updates the partition after these create callbacks.
    game.update_ground_contacts();
    game.update_gps_scrambler(marker, 0);
    assert!(game.entities[near].stealthed());
    assert!(!game.entities[edge].stealthed());
    assert!(!game.entities[far].stealthed());
    game.update_gps_scrambler(marker, 0);
    assert!(game.entities[edge].stealthed());
    for _ in 0..6 {
        game.update_gps_scrambler(marker, 0);
    }
    assert!(game.entities[far].stealthed());
    assert!(!game.entities[beyond].stealthed());
    assert!(!game.entities[hostile].stealthed());
    assert!(game.update_objects[game.entities[marker].update_owner].destroy_requested);
    assert_eq!(
        game.presentation
            .stealth_events
            .iter()
            .filter(
                |event| matches!(event, Event::GrantEmitter { object, .. } if *object == marker)
            )
            .count(),
        1
    );
}

#[test]
fn interrupted_gps_scan_destroys_its_emitter_and_preserves_existing_grants() {
    let (mut game, kind, marker_kind) = gps_fixture();
    let near = game.spawn(kind, 0, [320.0, 300.0], true);
    let far = game.spawn(kind, 0, [380.0, 300.0], true);
    let marker = game.spawn(marker_kind, 0, [300.0, 300.0], true);
    game.update_ground_contacts();
    game.update_gps_scrambler(marker, 0);
    assert!(game.entities[near].stealthed());
    assert!(!game.entities[far].stealthed());
    assert!(
        game.presentation
            .stealth_events
            .iter()
            .any(|event| matches!(event, Event::GrantEmitter { object, .. } if *object == marker))
    );

    game.destroy_maintenance_object(marker);
    assert_eq!(
        game.presentation
            .stealth_events
            .iter()
            .filter(|event| matches!(event, Event::DestroyGrantEmitter { object, .. } if *object == marker))
            .count(),
        1
    );
    for _ in 0..10 {
        game.step();
    }
    assert!(!game.entities[marker].exists());
    assert!(game.entities[near].stealthed());
    assert!(!game.entities[far].stealthed());
}
