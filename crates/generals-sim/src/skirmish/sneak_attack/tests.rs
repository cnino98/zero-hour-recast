use crate::skirmish::{Faction, Kind, PlacementFailure, tests, visibility};

#[test]
fn sneak_attack_checks_reference_clear_shroud_and_hidden_overlap() {
    let mut game = tests::game();
    let caster = game
        .entities
        .iter()
        .position(|actor| {
            actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::CommandCenter
        })
        .unwrap();
    let caster_type = game.entities[caster].type_index;
    game.catalog[caster_type].kind_of.push("IMMOBILE".into());
    game.catalog[caster_type].sight.clearing_range = 200.0;
    game.configure_visibility(visibility::VisibilityRules {
        cell_size: 25.0,
        unlook_persist_frames: 0,
        use_shroud: true,
    })
    .unwrap();
    let mut tunnel = tests::fixture_type(Faction::Gla, Kind::Defense);
    tunnel.name = "SneakPlacement".into();
    tunnel.geometry.major = 25.0;
    tunnel.geometry.minor = 25.0;
    let tunnel_type = game.catalog.len();
    game.catalog.push(tunnel);

    let point = [300.0, 305.0, 0.0];
    for edge in [[0.0, 305.0, 0.0], [800.0, 305.0, 0.0]] {
        assert!(matches!(
            game.sneak_attack_placement_failure(caster, tunnel_type, edge, 0.0, true),
            Some(PlacementFailure::RestrictedTerrain)
        ));
    }
    assert!(game.sneak_attack_location_valid(caster, "SneakPlacement", point));
    assert!(!game.sneak_attack_location_valid(caster, "Missing", point));
    assert!(matches!(
        game.sneak_attack_placement_failure(caster, tunnel_type, [760.0, 560.0, 0.0], 0.0, true),
        Some(PlacementFailure::Shroud)
    ));
    assert!(!game.sneak_attack_location_valid(caster, "SneakPlacement", [-20.0, 305.0, 0.0]));

    let mut nearby = tests::fixture_type(Faction::Gla, Kind::Infantry);
    let mut stealth = crate::skirmish::stealth::test_rules();
    stealth.innate = false;
    nearby.behavior.stealth = Some(stealth);
    tests::register_update(&mut nearby, "StealthUpdate");
    let actor_type = game.catalog.len();
    game.catalog.push(nearby);
    let friendly = game.spawn(actor_type, 0, [point[0], point[1]], true);
    game.update_ground_contacts();
    game.update_visibility();
    assert!(game.sneak_attack_location_valid(caster, "SneakPlacement", point));
    let enemy = game.spawn(actor_type, 1, [point[0], point[1]], true);
    game.update_ground_contacts();
    game.update_visibility();
    assert!(!game.sneak_attack_location_valid(caster, "SneakPlacement", point));
    game.receive_stealth_grant(enemy, true, 0);
    assert!(game.sneak_attack_location_valid(caster, "SneakPlacement", point));
    assert!(game.can_place_sneak_attack(caster, tunnel_type, point, 0.0, false));
    assert!(matches!(
        game.sneak_attack_placement_failure(caster, tunnel_type, point, 0.0, true),
        Some(PlacementFailure::Generic)
    ));
    game.catalog[actor_type].kind_of.push("MINE".into());
    assert!(game.can_place_sneak_attack(caster, tunnel_type, point, 0.0, true));
    game.catalog[actor_type].kind_of.pop();
    game.catalog[actor_type].kind_of.push("IMMOBILE".into());
    game.receive_stealth_grant(friendly, true, 0);
    assert!(!game.sneak_attack_location_valid(caster, "SneakPlacement", point));
}

#[test]
fn sneak_attack_client_placement_uses_the_selected_angle() {
    use crate::collision_geometry::Shape;
    use std::f32::consts::FRAC_PI_2;

    let mut game = tests::game();
    let caster = game
        .entities
        .iter()
        .position(|actor| {
            actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::CommandCenter
        })
        .unwrap();
    let caster_type = game.entities[caster].type_index;
    game.catalog[caster_type].kind_of.push("IMMOBILE".into());
    game.catalog[caster_type].sight.clearing_range = 200.0;
    game.configure_visibility(visibility::VisibilityRules {
        cell_size: 25.0,
        unlook_persist_frames: 0,
        use_shroud: true,
    })
    .unwrap();
    let mut exit = tests::fixture_type(Faction::Gla, Kind::Defense);
    exit.name = "RotatingSneakExit".into();
    exit.geometry.shape = Shape::Box;
    exit.geometry.major = 30.0;
    exit.geometry.minor = 6.0;
    exit.placement_angle = FRAC_PI_2;
    let kind = game.catalog.len();
    game.catalog.push(exit);
    let mut obstacle = tests::fixture_type(Faction::Gla, Kind::Infantry);
    obstacle.geometry.major = 3.0;
    let obstacle_type = game.catalog.len();
    game.catalog.push(obstacle);
    let point = [300.0, 305.0, 0.0];
    game.spawn(obstacle_type, 1, [320.0, 305.0], true);
    game.update_ground_contacts();
    game.update_visibility();

    assert!(game.sneak_attack_location_valid(caster, "RotatingSneakExit", point));
    for committing in [false, true] {
        assert!(game.can_place_sneak_attack(caster, kind, point, FRAC_PI_2, committing));
        assert!(!game.can_place_sneak_attack(caster, kind, point, 0.0, committing));
    }
}

#[test]
fn sneak_terrain_samples_the_transformed_world_footprint() {
    let mut game = tests::game();
    let mut tunnel = tests::fixture_type(Faction::Gla, Kind::Defense);
    tunnel.geometry.major = 25.0;
    tunnel.geometry.minor = 25.0;
    let kind = game.catalog.len();
    game.catalog.push(tunnel);
    game.rules.allowed_build_height_variation = 5.0;
    let point = [300.0, 305.0, 0.0];
    assert!(game.native_build_terrain_clear(kind, point, 0.0));
    let mut heights = vec![0.0; 80 * 60];
    for row in 0..60 {
        heights[row * 80 + 30] = 50.0;
    }
    game.set_projectile_terrain(80, 60, [0.0; 2], heights, 10.0)
        .unwrap();
    assert!(matches!(
        game.native_build_terrain_failure(kind, point, 0.0),
        Some(PlacementFailure::NotFlatEnough)
    ));
    // Changing an unrelated patch near the origin cannot change this site's result.
    assert!(game.native_build_terrain_clear(kind, [500.0, 305.0, 0.0], 0.0));
}
