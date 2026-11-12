use super::*;
use crate::skirmish::DamageType;

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
    assert_eq!(game.entities[jet].physics_last_collidee(), Some(tree));
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
    assert_eq!(game.entities[jet].physics_last_collidee(), Some(target));
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
    assert_eq!(game.entities[jet].physics_last_collidee(), Some(tree));
    game.apply_typed_hit(jet, jet, 999_999.0, DeathType::Normal);
    assert_eq!(game.entities[jet].physics_last_collidee(), Some(tree));
    game.update_jet_slow_death(jet, 0);
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "JetPhase3"
    );
    assert!(game.object_position(jet)[2] > game.ground_height(game.entities[jet].position));
}
