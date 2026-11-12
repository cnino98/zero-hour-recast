use super::*;
use crate::skirmish::{
    Faction, Kind, Order, UnitType,
    module_updates::FOREVER,
    tests::{self, fixture_type, register_update},
};

fn blast(game: &mut Skirmish, position: [f32; 2], air: bool, allies: bool) -> usize {
    let mut unit = fixture_type(Faction::Usa, Kind::TechBuilding);
    unit.name = "NativeNeutronBlast".into();
    unit.kind_of = vec!["INERT".into()];
    unit.behavior.neutron_blast.push(Rules {
        radius: 50.0,
        affect_airborne: air,
        affect_allies: allies,
    });
    register_update(&mut unit, "NeutronBlastBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 3;
    unit.behavior.lifetimes.push([2; 2]);
    register_update(&mut unit, "LifetimeUpdate");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    game.spawn(kind, 0, position, true)
}

fn spawn(game: &mut Skirmish, unit: UnitType, owner: usize, position: [f32; 2]) -> usize {
    let kind = game.catalog.len();
    game.catalog.push(unit);
    game.spawn(kind, owner, position, true)
}

#[test]
fn scheduled_death_uses_strict_center_2d_alive_map_air_alliance_filters() {
    let mut game = tests::game();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    let source = blast(&mut game, [10.0, 300.0], false, false);
    let infantry = |faction| {
        let mut unit = fixture_type(faction, Kind::Infantry);
        unit.ai.acquisition = 0;
        unit
    };
    let killed = spawn(&mut game, infantry(Faction::Gla), 1, [20.0, 300.0]);
    // Geometry/z are irrelevant to FROM_CENTER_2D, including an elevated non-air target.
    game.set_object_position(killed, [20.0, 300.0, 200.0]);
    let edge = spawn(&mut game, infantry(Faction::Gla), 1, [60.0, 300.0]);
    let ally = spawn(&mut game, infantry(Faction::Usa), 0, [20.0, 310.0]);
    // x=0 is outside the strict playable extent but the body's real coverage
    // overlaps partition cell0, so SameMapStatus, not missing cells, rejects it.
    let mut offmap_unit = infantry(Faction::Gla);
    // This partition-filter target has no native AI module: an ordinary mobile
    // infantry's AI may issue border correction/avoidance before frame2.
    offmap_unit.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    offmap_unit
        .behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    offmap_unit.update_modules.retain(|module| {
        !matches!(module,
        crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. }
        if family == "AIUpdateInterface")
    });
    let offmap = spawn(&mut game, offmap_unit, 1, [0.0, 300.0]);
    assert!(game.entities[offmap].ai.registration.is_none());
    assert!(game.entities[offmap].is_off_map());
    let mut airborne_unit = infantry(Faction::Gla);
    airborne_unit.locomotor.airborne_targeting_height = 10.0;
    let airborne = spawn(&mut game, airborne_unit, 1, [30.0, 310.0]);
    game.set_object_position(airborne, [30.0, 310.0, 100.0]);
    game.update_ground_locomotor(airborne);
    assert!(game.entities[airborne].is_airborne_target());
    let mut aircraft = infantry(Faction::Gla);
    aircraft.kind_of.push("AIRCRAFT".into());
    let aircraft = spawn(&mut game, aircraft, 1, [30.0, 320.0]);
    let dead = spawn(&mut game, infantry(Faction::Gla), 1, [35.0, 300.0]);
    game.kill_object(dead, DeathType::Normal);
    game.update_ground_contacts();
    let candidates = game.partition.fastest_range_objects([10.0, 300.0], 50.0);
    for id in [offmap, airborne, edge] {
        assert!(candidates.contains(&id));
    }
    let registration = game.catalog[game.entities[source].type_index]
        .update_modules
        .len()
        - 2;
    assert_eq!(game.update_remaining_sleep(source, registration), FOREVER);
    game.tick = 1;
    game.run_object_updates();
    assert!(game.entities[killed].alive());
    assert_eq!(
        game.entities[offmap].position.map(f32::to_bits),
        [0.0_f32, 300.0].map(f32::to_bits)
    );
    assert!(game.entities[offmap].is_off_map());
    assert!(game.entities[airborne].is_airborne_target());
    game.tick = 2;
    game.run_object_updates();
    assert!(!game.entities[source].alive());
    assert!(!game.entities[killed].alive());
    assert!(game.entities[killed].damage_state.last_attacker.is_none());
    for id in [edge, ally, offmap, airborne, aircraft] {
        assert!(game.entities[id].alive(), "native filtered target {id}");
    }
    assert_eq!(
        game.entities[dead].damage_state.death_type as u32,
        DeathType::Normal as u32
    );
}

#[test]
fn vehicle_pilot_death_preserves_health_resets_experience_and_deselects_all_players() {
    let mut game = tests::game();
    let mut vehicle = fixture_type(Faction::Gla, Kind::Tank);
    vehicle.veterancy.trainable = true;
    vehicle.veterancy.required = [0, 100, 200, 400];
    let tank = spawn(&mut game, vehicle, 1, [410.0, 300.0]);
    game.set_veterancy(tank, 3);
    game.entities[tank].health = 50.0;
    game.ai_busy_from_ai(tank);
    game.create_selection(0, &[], &[tank], true);
    game.create_selection(1, &[], &[tank], true);
    let source = blast(&mut game, [400.0, 300.0], true, true);
    game.update_ground_contacts();
    game.kill_object(source, DeathType::Normal);
    assert!(game.entities[tank].alive());
    assert!(game.entities[tank].unmanned());
    assert_eq!(game.entities[tank].health.to_bits(), 50.0_f32.to_bits());
    assert_eq!(game.entities[tank].experience, 0);
    assert_eq!(game.entities[tank].veterancy, 0);
    assert_eq!(game.entities[tank].owner, game.neutral_owner());
    assert!(matches!(game.entities[tank].ai.order, Order::Idle));
    assert!(game.selected_objects(0).is_empty());
    assert!(game.selected_objects(1).is_empty());
    assert!(game.entities[tank].terrain_decal_none());
    assert_eq!(game.presentation.pilot_events.len(), 1);
    let mut bike = fixture_type(Faction::Gla, Kind::Tank);
    bike.kind_of.push("CLIFF_JUMPER".into());
    let bike = spawn(&mut game, bike, 1, [420.0, 300.0]);
    let mut drone = fixture_type(Faction::Gla, Kind::Tank);
    drone.kind_of.push("DRONE".into());
    let drone = spawn(&mut game, drone, 1, [420.0, 310.0]);
    let source = blast(&mut game, [400.0, 300.0], true, true);
    game.update_ground_contacts();
    game.kill_object(source, DeathType::Normal);
    assert!(!game.entities[bike].alive());
    assert!(!game.entities[bike].unmanned());
    assert!(game.entities[drone].alive());
    assert!(!game.entities[drone].unmanned());
    assert_eq!(game.entities[drone].owner, 1);
}

#[test]
fn ordinary_kill_all_calls_direct_removal_without_world_reinsertion() {
    let (mut game, host) = crate::skirmish::containment::tests::setup(0.0, 0.0);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, host);
    assert!(game.entities[rider].containment.nested_removed);
    assert!(game.entities[rider].masked());
    game.kill_all_contained(host);
    assert!(game.passengers(host).is_empty());
    assert!(game.entities[rider].contained_by().is_none());
    assert!(!game.entities[rider].masked());
    assert!(game.entities[rider].containment.nested_removed);
    assert!(!game.partition.registered(rider));
    assert!(!game.entities[rider].alive());
    assert!(game.entities[rider].damage_state.last_attacker.is_none());
}

#[test]
fn virtual_overlord_kill_all_preserves_portable_and_only_visits_redirected_cargo() {
    for with_container in [false, true] {
        let (mut game, old) = crate::skirmish::containment::tests::setup(0.0, 0.0);
        let host = crate::skirmish::containment::tests::respawn_contain_family(
            &mut game,
            old,
            "OverlordContain",
        );
        let mut portable = fixture_type(Faction::Gla, Kind::Infantry);
        portable.kind_of = vec!["PORTABLE_STRUCTURE".into()];
        if with_container {
            portable.contain = Some(crate::skirmish::containment::tests::rules(0.0, 0.0));
            register_update(&mut portable, "TransportContain");
        }
        let turret = spawn(&mut game, portable, 0, [355.0, 255.0]);
        game.add_to_contain(turret, host);
        let rider = if with_container {
            let rider = game.spawn(5, 0, [355.0, 255.0], true);
            game.add_to_contain(rider, host); // actual virtual redirection
            assert_eq!(game.entities[rider].contained_by(), Some(turret));
            Some(rider)
        } else {
            None
        };
        game.kill_all_contained(host);
        assert!(game.entities[turret].alive());
        assert_eq!(game.entities[turret].contained_by(), Some(host));
        assert_eq!(game.entities[host].containment.passengers, [turret]);
        if let Some(rider) = rider {
            assert!(!game.entities[rider].alive());
            assert!(game.entities[rider].contained_by().is_none());
            assert!(game.passengers(turret).is_empty());
        }
    }
}

#[test]
fn native_relationship_guards_use_actual_undetected_defection_at_both_ends() {
    for source_defects in [false, true] {
        let mut game = tests::game();
        let source = blast(&mut game, [400.0, 300.0], true, false);
        let target = game.spawn(5, usize::from(source_defects), [410.0, 300.0], true);
        let team = game.team_membership.default_team(1);
        game.defect_to_team(if source_defects { source } else { target }, team, 30);
        assert!(
            game.entities[if source_defects { source } else { target }]
                .helpers
                .defection
                .undetected
        );
        game.update_ground_contacts();
        game.kill_object(source, DeathType::Normal);
        // Undetected source regards everyone as neutral. An undetected target
        // is treated as allied, despite its actual enemy team after defection.
        assert_eq!(game.entities[target].alive(), !source_defects);
    }
}
