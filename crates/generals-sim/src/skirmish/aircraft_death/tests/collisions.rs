use super::*;
use crate::skirmish::{DamageType, partition::Identity};

fn scene() -> (Skirmish, usize, usize, usize) {
    let (mut game, jet) = fixture();
    let kind = game.entities[jet].type_index;
    let rules = &mut game.catalog[kind].live_physics.as_mut().unwrap().collision;
    rules.weapons = std::array::from_fn(|index| {
        Some(physical_ocl::CrashWeapon {
            damage: 30.0,
            damage_type: if index == 0 {
                DamageType::Explosion
            } else {
                DamageType::Crush
            },
            death_type: if index == 0 {
                DeathType::Exploded
            } else {
                DeathType::Crushed
            },
            effects: std::array::from_fn(|level| format!("Crash{index}Vet{level}")),
        })
    });
    let kind = game.type_for(1, Kind::Barracks).unwrap();
    game.catalog[kind].geometry.height = 100.0;
    let building = game.spawn(kind, 1, [300.0, 300.0], true);
    let bystander = game.spawn(kind, 1, [340.0, 300.0], true);
    game.apply_typed_hit(jet, jet, 999_999.0, DeathType::Normal);
    game.set_flight_position(jet, [300.0, 300.0, 65.0]);
    game.observe_ground_partition(jet);
    (game, jet, building, bystander)
}

#[test]
fn falling_jet_partition_contact_damages_only_struck_structure_then_destroys_source() {
    let (mut game, jet, building, bystander) = scene();
    let before = game.entities[building].health;
    let neighbor = game.entities[bystander].health;
    game.update_ground_contacts();
    game.process_update_destruction();
    assert!(!game.entities[jet].exists());
    assert_eq!(
        game.entities[building].health.to_bits(),
        (before - 30.0).to_bits()
    );
    assert_eq!(
        game.entities[bystander].health.to_bits(),
        neighbor.to_bits()
    );
    assert_eq!(
        game.entities[building].damage_state.last_attacker,
        Some(jet)
    );
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "Crash0Vet0"
    );
    assert!(game.presentation.shots.is_empty() && game.projectiles.is_empty());
    assert_eq!(
        game.entities.len(),
        bystander + 1,
        "direct source fix must not create FireOCL objects"
    );
    let count = game.presentation.object_fx_events.len();
    game.update_ground_contacts();
    assert_eq!(game.presentation.object_fx_events.len(), count);
}

#[test]
fn falling_jet_ignored_producer_and_no_collide_suppress_physics_before_damage() {
    let (mut game, jet, building, _) = scene();
    let before = game.entities[building].health;
    game.entities[jet].ai.ignored_producer = Some(building);
    game.update_ground_contacts();
    assert!(game.entities[jet].jet_corpse());
    assert_eq!(game.entities[building].health.to_bits(), before.to_bits());
    game.entities[jet].ai.ignored_producer = None;
    let kind = game.entities[building].type_index;
    game.catalog[kind].kind_of.push("NO_COLLIDE".into());
    game.update_ground_contacts();
    assert!(game.entities[jet].jet_corpse());
    assert_eq!(game.entities[building].health.to_bits(), before.to_bits());
}

#[test]
fn nonbuilding_crash_keeps_jet_and_tree_contact_triggers_hit_ground_on_next_normal_callback() {
    let (mut game, jet, tree, bystander) = scene();
    let kind = game.entities[tree].type_index;
    game.catalog[kind].kind_of = vec!["IMMOBILE".into(), "SHRUBBERY".into()];
    let before = game.entities[tree].health;
    let neighbor = game.entities[bystander].health;
    game.update_ground_contacts();
    assert!(game.entities[jet].jet_corpse());
    assert_eq!(
        game.entities[tree].health.to_bits(),
        (before - 30.0).to_bits()
    );
    assert_eq!(
        game.entities[bystander].health.to_bits(),
        neighbor.to_bits()
    );
    assert_eq!(
        game.entities[jet].physics_last_collidee(),
        Some(Identity::Actor(tree))
    );
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "Crash1Vet0"
    );
    game.tick += 1;
    game.update_jet_slow_death(jet, 0);
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "JetPhase3"
    );
    assert!(game.object_position(jet)[2] > game.ground_height(game.entities[jet].position));
}

#[test]
fn force_disabled_still_records_contact_without_damage_and_mobile_ai_contact_is_ignored() {
    let (mut game, jet, target, _) = scene();
    let kind = game.entities[jet].type_index;
    game.catalog[kind]
        .live_physics
        .as_mut()
        .unwrap()
        .collision
        .allow_force = false;
    let before = game.entities[target].health;
    game.update_ground_contacts();
    assert_eq!(
        game.entities[jet].physics_last_collidee(),
        Some(Identity::Actor(target))
    );
    assert!(game.entities[jet].jet_corpse());
    assert_eq!(game.entities[target].health.to_bits(), before.to_bits());
    let kind = game.entities[target].type_index;
    game.catalog[kind].kind_of = vec!["VEHICLE".into()];
    game.catalog[kind].crush.physics_module = Some(0);
    game.entities[jet]
        .live_physics
        .as_mut()
        .unwrap()
        .last_collidee = None;
    game.update_ground_contacts();
    assert_eq!(game.entities[jet].physics_last_collidee(), None);
}

#[test]
fn ordinary_physics_tree_contact_survives_the_jet_death_constructor() {
    let (mut game, jet) = fixture();
    let kind = game.entities[jet].type_index;
    // An ordinary Physics object without an AI collision veto records this contact.
    game.catalog[kind].ai.update = crate::skirmish::UnitAiUpdate::Absent;
    game.catalog[kind]
        .live_physics
        .as_mut()
        .unwrap()
        .collision
        .allow_force = false;
    let tree_kind = game.type_for(1, Kind::Barracks).unwrap();
    game.catalog[tree_kind].kind_of = vec!["IMMOBILE".into(), "SHRUBBERY".into()];
    game.catalog[tree_kind].geometry.height = 100.0;
    let tree = game.spawn(tree_kind, 1, [300.0, 300.0], true);
    game.set_flight_position(jet, [300.0, 300.0, 65.0]);
    game.physical_contact(jet, tree);
    assert_eq!(
        game.entities[jet].physics_last_collidee(),
        Some(Identity::Actor(tree))
    );
    game.apply_typed_hit(jet, jet, 999_999.0, DeathType::Normal);
    assert_eq!(
        game.entities[jet].physics_last_collidee(),
        Some(Identity::Actor(tree))
    );
    game.update_jet_slow_death(jet, 0);
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "JetPhase3"
    );
    assert!(game.object_position(jet)[2] > game.ground_height(game.entities[jet].position));
}

#[test]
fn jet_crash_uses_destination_layer_and_bridge_slop_instead_of_terrain_height() {
    for (height, hit_bridge) in [(80.5, true), (40.0, false)] {
        let (mut game, jet) = fixture();
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
        game.apply_typed_hit(jet, jet, 999_999.0, DeathType::Normal);
        game.set_object_position(jet, [300.0, 300.0, height]);
        game.update_jet_slow_death(jet, 0);
        assert_eq!(game.entities[jet].layer, if hit_bridge { layer } else { 1 });
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .any(|event| event.effect == "JetPhase3"),
            hit_bridge
        );
        assert!(game.entities[jet].jet_corpse());
    }
}

#[test]
fn dying_jet_hits_logical_map_shrubbery_above_terrain_but_respects_no_collide_and_height() {
    use crate::{
        collision_geometry::{Geometry, Shape},
        skirmish::MapWorldObject,
    };

    for (height, no_collide, impact) in [
        (10.0, false, true),
        (30.0, false, false),
        (10.0, true, false),
    ] {
        let (mut game, jet) = fixture();
        let mut kinds = vec!["IMMOBILE".into(), "SHRUBBERY".into()];
        if no_collide {
            kinds.push("NO_COLLIDE".into());
        }
        game.layout.world_objects.push(MapWorldObject {
            template: "MapShrubbery".into(),
            kind_of: kinds,
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
        game.set_object_position(jet, [300.0, 300.0, 10.0]);
        game.update_ground_contacts();
        assert_eq!(game.entities[jet].physics_last_collidee(), None);

        game.set_object_position(jet, [300.0, 300.0, 100.0]);
        game.apply_typed_hit(jet, jet, 999_999.0, DeathType::Normal);
        game.set_object_position(jet, [300.0, 300.0, height]);
        game.update_ground_contacts();
        game.tick += 1;
        game.update_jet_slow_death(jet, 0);
        assert_eq!(
            game.entities[jet].physics_last_collidee(),
            impact.then_some(Identity::Map(0))
        );
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .any(|event| event.effect == "JetPhase3"),
            impact
        );
        assert!(game.object_position(jet)[2] > game.ground_height([300.0, 300.0]));
    }
}

#[test]
fn ai_physics_uses_effective_death_for_contacts_with_both_immobile_identities() {
    use crate::{
        collision_geometry::{Geometry, Shape},
        skirmish::MapWorldObject,
    };

    for effective_dead in [false, true] {
        let (mut game, jet) = fixture();
        game.entities[jet].health = if effective_dead { 50.0 } else { 0.0 };
        game.entities[jet].set_effectively_dead(effective_dead);
        let geometry = || Geometry {
            shape: Shape::Cylinder,
            is_small: true,
            major: 4.0,
            minor: 4.0,
            height: 15.0,
        };
        let kind = game.type_for(1, Kind::Barracks).unwrap();
        game.catalog[kind].kind_of = vec!["IMMOBILE".into(), "SHRUBBERY".into()];
        game.catalog[kind].geometry = geometry();
        let tree = game.spawn(kind, 1, [300.0, 300.0], true);
        game.layout.world_objects.push(MapWorldObject {
            template: "MapShrubbery".into(),
            kind_of: vec!["IMMOBILE".into(), "SHRUBBERY".into()],
            position: [400.0, 300.0, 0.0],
            angle: 0.0,
            geometry: geometry(),
            original_team: "teamCivilian".into(),
        });
        game.observe_map_partition(0);
        game.update_ground_contacts();
        for (position, expected) in [
            ([300.0, 300.0, 10.0], Identity::Actor(tree)),
            ([400.0, 300.0, 10.0], Identity::Map(0)),
        ] {
            game.set_object_position(jet, position);
            game.update_ground_contacts();
            assert_eq!(
                game.entities[jet].physics_last_collidee(),
                effective_dead.then_some(expected)
            );
        }
    }
}
