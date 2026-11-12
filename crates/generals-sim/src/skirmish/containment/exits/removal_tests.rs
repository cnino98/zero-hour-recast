use crate::skirmish::{containment, maintenance, parachute, tests};

#[test]
fn parachute_on_removing_retains_parent_and_stored_mask_until_object_callback() {
    let (mut game, chute, rider) = parachute::tests::fixture(120.0);
    assert!(!game.entities[rider].masked());
    assert!(!game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
    assert!(game.entities[rider].parachute_rider());
    // Native iterator erasure precedes the virtual container callback. Exercise
    // that actual callback boundary without prematurely running onRemovedFrom.
    game.entities[chute].containment.passengers.clear();
    game.container_on_removing(rider, chute, true);
    assert_eq!(game.entities[rider].contained_by(), Some(chute));
    assert!(!game.entities[rider].parachute_rider());
    assert!(!game.entities[rider].disabled_held());
    assert!(!game.entities[rider].masked());
    assert!(!game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
    let sounds = game.presentation.container_sounds.len();
    // A death/damage callback can reenter parent removal while the pointer is
    // retained. Native removeFromContain finds no literal item and does nothing.
    game.remove_passenger(rider, chute);
    assert_eq!(game.presentation.container_sounds.len(), sounds);
    assert_eq!(game.entities[rider].contained_by(), Some(chute));
    game.object_on_removed_from(rider);
    assert!(game.entities[rider].contained_by().is_none());
    assert_eq!(game.entities[rider].containment.entered_at, 0);
    assert!(game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
}

#[test]
fn actual_nested_parachute_removal_clears_all_object_flags_after_landing_callbacks() {
    let (mut game, chute, rider) = parachute::tests::fixture(120.0);
    game.tick = 19;
    game.remove_passenger(rider, chute);
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.passengers(chute).is_empty());
    assert!(!game.entities[rider].parachute_rider());
    assert!(!game.entities[rider].masked());
    assert!(!game.entities[rider].disabled_held());
    assert!(game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
    assert_eq!(game.entities[rider].containment.entered_at, 0);
}

#[test]
fn crc0_fatal_contained_damage_runs_callbacks_without_iterator_world_restore() {
    let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
    let mut unit = tests::fixture_type(
        crate::skirmish::Faction::Gla,
        crate::skirmish::Kind::Infantry,
    );
    unit.behavior.destroy_die.push(tests::death_conditions());
    tests::register_behavior(&mut unit, "DestroyDie", 2);
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let rider = game.spawn(kind, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    assert!(game.entities[rider].masked() && game.entities[rider].disabled_held());
    assert!(!game.partition.registered(rider));
    game.tick = 1;
    game.presentation.container_sounds.clear();
    game.process_damage_to_contained(carrier, 1.0);
    assert!(game.entities[rider].is_effectively_dead());
    assert!(game.update_objects[game.entities[rider].update_owner].destroy_requested);
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.passengers(carrier).is_empty());
    assert!(!game.entities[rider].masked());
    assert!(!game.entities[rider].disabled_held());
    assert_eq!(
        game.entities[rider].maintenance.instance_flags & maintenance::INSTANCE_UNSELECTABLE,
        0
    );
    // CRC=0 directly calls onRemoving/onRemovedFrom, so this path never invokes
    // iterator removal's addOrRemoveObjFromWorld or coalesced unload sound.
    assert!(game.entities[rider].containment.nested_removed);
    assert!(!game.partition.registered(rider));
    assert_eq!(game.presentation.container_sounds.len(), 2);
    assert!(matches!(
        game.presentation.container_sounds[0].hook,
        containment::SoundHook::SoundExit
    ));
    assert!(matches!(
        game.presentation.container_sounds[1].hook,
        containment::SoundHook::SoundFalling
    ));
}

#[test]
fn crc0_nonfatal_damage_restores_literal_list_without_removal_callbacks() {
    let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    let health = game.entities[rider].health;
    game.presentation.container_sounds.clear();
    game.process_damage_to_contained(carrier, 0.5);
    assert_eq!(
        game.entities[rider].health.to_bits(),
        (health * 0.5).to_bits()
    );
    assert_eq!(game.passengers(carrier), [rider]);
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    assert!(game.entities[rider].masked() && game.entities[rider].disabled_held());
    assert!(!game.partition.registered(rider));
    assert!(game.presentation.container_sounds.is_empty());
}
