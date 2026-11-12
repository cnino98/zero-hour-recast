use super::*;

fn map(players: usize) -> Visibility {
    Visibility::new(
        VisibilityRules {
            cell_size: 40.0,
            unlook_persist_frames: 150,
            use_shroud: true,
        },
        [400.0, 400.0],
        players,
    )
}

#[test]
fn unseen_becomes_clear_then_remembered_only_after_strict_undo_deadline() {
    let mut map = map(2);
    assert!(matches!(map.cell(0, [200.0, 200.0]), Status::Shrouded));
    let look = map.look([5, 5], 40.0, vec![0]);
    assert!(matches!(map.cell(0, [200.0, 200.0]), Status::Clear));
    assert!(matches!(map.cell(1, [200.0, 200.0]), Status::Shrouded));
    map.unlook(look, 10);
    map.expire(160);
    assert!(matches!(map.cell(0, [200.0, 200.0]), Status::Clear));
    map.expire(161);
    assert!(matches!(map.cell(0, [200.0, 200.0]), Status::Fogged));
}

#[test]
fn overlapping_allied_lookers_do_not_remove_each_others_sight() {
    let mut map = map(3);
    let first = map.look([5, 5], 80.0, vec![0, 1]);
    let second = map.look([6, 5], 80.0, vec![0, 1]);
    map.unlook(first, 0);
    map.expire(151);
    assert!(matches!(map.cell(0, [200.0, 200.0]), Status::Clear));
    assert!(matches!(map.cell(1, [200.0, 200.0]), Status::Clear));
    assert!(matches!(map.cell(2, [200.0, 200.0]), Status::Shrouded));
    map.unlook(second, 151);
    map.expire(302);
    assert!(matches!(map.cell(1, [200.0, 200.0]), Status::Fogged));
}

#[test]
fn bresenham_radius_one_preserves_native_three_cell_vertical_degeneracy() {
    let cells = circle::cells([2, 2], 1, [5, 5]);
    assert_eq!(cells, [17, 7, 12]);
    assert_eq!(circle::cells([0, 0], 1, [5, 5]), [5, 0]);
}

#[test]
fn negative_world_coordinates_floor_outside_and_reset_removes_memory() {
    let mut map = map(2);
    let _look = map.look([0, 0], 40.0, vec![0]);
    assert!(matches!(map.cell(0, [-0.01, 0.0]), Status::Shrouded));
    map.reset(5);
    assert_eq!(map.cells.len(), 5);
    assert!(matches!(map.cell(0, [0.0, 0.0]), Status::Shrouded));
}

#[test]
fn explored_map_hides_unseen_enemies_then_remembers_only_the_seen_building() {
    let mut game = super::super::tests::game();
    game.entities.clear();
    game.partition.clear();
    for unit in &mut game.catalog {
        unit.sight.clearing_range = 0.0;
        unit.sight.immobile = unit.kind.is_building();
    }
    game.catalog[5].sight.clearing_range = 80.0;
    game.configure_visibility(VisibilityRules {
        cell_size: 40.0,
        unlook_persist_frames: 150,
        use_shroud: false,
    })
    .unwrap();
    let observer = game.spawn(5, 0, [50.0, 50.0], true);
    let building = game.spawn(0, 1, [300.0, 300.0], true);
    let tank = game.spawn(6, 1, [300.0, 300.0], true);
    game.update_visibility();
    assert!(matches!(
        game.visibility_cell(0, [300.0, 300.0]),
        Status::Fogged
    ));
    assert!(matches!(
        game.visibility_status(0, building),
        Status::Shrouded
    ));
    game.entities[observer].position = [300.0, 300.0];
    game.observe_visibility(observer);
    game.update_visibility();
    assert!(game.visible_to(0, tank));
    game.entities[observer].position = [50.0, 50.0];
    game.observe_visibility(observer);
    game.tick = 151;
    game.update_visibility();
    assert!(matches!(game.visibility_status(0, tank), Status::Shrouded));
    assert!(matches!(
        game.visibility_status(0, building),
        Status::Fogged
    ));
    game.entities[building].health = 0.0;
    game.observe_visibility(building);
    game.update_visibility();
    assert!(matches!(
        game.visibility_status(0, building),
        Status::Fogged
    ));
    game.entities[observer].position = [300.0, 300.0];
    game.observe_visibility(observer);
    game.update_visibility();
    assert!(matches!(game.visibility_status(0, building), Status::Clear));
}

#[test]
fn masking_an_uncontained_unit_preserves_its_look_until_destroyed() {
    let mut game = super::super::tests::game();
    let point = [705.0, 505.0];
    game.catalog[5].sight.clearing_range = 80.0;
    game.configure_visibility(VisibilityRules {
        cell_size: 40.0,
        unlook_persist_frames: 0,
        use_shroud: true,
    })
    .unwrap();
    let scout = game.spawn(5, 0, point, true);
    game.entities[scout].maintenance.masked = true;
    game.observe_visibility(scout);
    game.tick += 1;
    game.update_visibility();
    assert!(matches!(game.visibility_cell(0, point), Status::Clear));
    game.destroy_maintenance_object(scout);
    game.process_update_destruction();
    game.tick += 1;
    game.update_visibility();
    assert!(matches!(game.visibility_cell(0, point), Status::Fogged));
}

#[test]
fn completion_reveals_to_enemies_even_when_own_clearing_radius_is_unchanged() {
    let mut game = super::super::tests::game();
    game.entities.clear();
    game.partition.clear();
    game.visibility.reset(game.players.len());
    game.catalog[0].sight.clearing_range = 25.0;
    game.catalog[0].sight.reveal_to_all_range = 100.0;
    let building = game.spawn(0, 0, [300.0, 300.0], false);
    assert!(matches!(
        game.visibility_cell(1, [300.0, 300.0]),
        Status::Shrouded
    ));
    game.entities[building].build_progress = 1.0;
    game.observe_visibility(building);
    assert!(matches!(
        game.visibility_cell(1, [300.0, 300.0]),
        Status::Clear
    ));
}

#[test]
fn radar_creation_upgrade_requires_completed_live_provider_and_respects_brownout() {
    let mut game = super::super::tests::game();
    game.entities.clear();
    game.partition.clear();
    game.visibility.reset(game.players.len());
    super::super::tests::radar_upgrade(&mut game.catalog[0], "Upgrade_AmericaRadar", true, false);
    let provider = game.spawn(0, 0, [300.0, 300.0], false);
    assert!(!game.has_radar(0));
    game.entities[provider].build_progress = 1.0;
    game.native_build_complete(provider);
    assert!(game.has_radar(0));
    game.set_player_radar_disabled(0, true);
    assert!(!game.has_radar(0));
    // DisableProof bypasses player brownout; it does not bypass an object's disabled edge.
    super::super::tests::radar_upgrade(&mut game.catalog[1], "Upgrade_ProofRadar", true, true);
    let proof = game.spawn(1, 0, [400.0, 300.0], true);
    assert!(game.has_radar(0));
    game.destroy_maintenance_object(proof);
    assert!(!game.has_radar(0));
    game.set_player_radar_disabled(0, false);
    assert!(game.has_radar(0));
    game.destroy_maintenance_object(provider);
    assert!(!game.has_radar(0));
}

#[test]
fn local_defeat_reveals_permanently_and_forces_radar_without_a_provider() {
    let mut game = super::super::tests::game();
    game.tick = 10;
    let point = [750.0, 550.0];
    assert!(matches!(game.visibility_cell(0, point), Status::Shrouded));
    for actor in &mut game.entities {
        if actor.owner == 0 && game.catalog[actor.type_index].scoring.victory {
            actor.health = 0.0;
        }
    }
    game.update_victory();
    assert!(game.defeated[0]);
    assert!(game.has_radar(0));
    assert!(matches!(game.visibility_cell(0, point), Status::Clear));
    game.tick += 151;
    game.update_visibility();
    assert!(matches!(game.visibility_cell(0, point), Status::Clear));
    assert!(game.visibility.cells[0].iter().all(|count| *count < 0));
    game.reset(super::super::Faction::Gla, super::super::Faction::Gla)
        .unwrap();
    assert!(!game.has_radar(0));
    assert!(matches!(game.visibility_cell(0, point), Status::Shrouded));
}
