use crate::skirmish::{
    Faction, Kind, TypeLimit,
    tests::{fixture_type, game},
};

#[test]
fn native_buildable_flags_preserve_player_guards_and_ignore_prerequisite_order() {
    use crate::skirmish::Buildable;
    let mut game = game();
    let unit = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[unit].build_rules.status = Buildable::No;
    assert!(!game.prerequisites_met(0, unit));
    game.catalog[unit].build_rules.status = Buildable::OnlyByAi;
    game.slots[0].difficulty = None;
    assert!(!game.prerequisites_met(0, unit));
    game.slots[0].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    assert!(game.prerequisites_met(0, unit));
    game.catalog[unit].build_rules.sciences = vec!["UnavailableScience".into()];
    game.catalog[unit].build_rules.limit = TypeLimit::Count(1);
    game.spawn(unit, 0, [350.0, 250.0], true);
    assert!(!game.prerequisites_met(0, unit));
    game.catalog[unit].build_rules.status = Buildable::IgnorePrerequisites;
    assert!(game.prerequisites_met(0, unit));
    game.set_map_build_allowed(0, true, false);
    assert!(!game.prerequisites_met(0, unit));
}

#[test]
fn skirmish_options_survive_reset_and_superweapon_caps_are_template_specific() {
    let mut game = game();
    let first = game.type_for(0, Kind::Barracks).unwrap();
    let other = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[first].name = "ParticleCannon".into();
    game.catalog[first].build_rules.limit = TypeLimit::Superweapon;
    game.catalog[other].name = "ScudStorm".into();
    game.catalog[other].build_rules.limit = TypeLimit::Superweapon;
    game.set_match_options(20_000, 1);
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.players[0].cash, 20_000);
    assert_eq!(game.players[1].cash, 20_000);
    assert_eq!(game.players[0].statistics.money_earned, 0);
    let structure = game.spawn(first, 0, [350.0, 250.0], false);
    assert!(!game.prerequisites_met(0, first));
    assert!(game.prerequisites_met(0, other));
    assert!(game.prerequisites_met(1, first));
    game.set_match_options(5_000, 0);
    assert!(game.prerequisites_met(0, first));
    game.catalog[first].build_rules.limit = TypeLimit::Count(1);
    assert!(!game.prerequisites_met(0, first));
    game.destroy_maintenance_object(structure);
    game.process_update_destruction();
    assert!(game.prerequisites_met(0, first));
    game.reset(Faction::Gla, Faction::Gla).unwrap();
    assert_eq!(game.players[0].cash, 5_000);
}

#[test]
fn linked_caps_count_other_templates_and_unfinished_buildings_but_not_dead_objects() {
    let mut game = game();
    let first = game.type_for(0, Kind::Barracks).unwrap();
    let other = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[first].name = "LinkedA".into();
    game.catalog[first].build_rules.limit = TypeLimit::Count(1);
    game.catalog[first].build_rules.linked_limit = Some("Shared".into());
    game.catalog[other].name = "LinkedB".into();
    game.catalog[other].build_rules.linked_limit = Some("Shared".into());
    let structure = game.spawn(other, 0, [350.0, 250.0], false);
    assert!(!game.prerequisites_met(0, first));
    game.destroy_maintenance_object(structure);
    game.process_update_destruction();
    assert!(game.prerequisites_met(0, first));
}

#[test]
fn native_build_cap_counts_owned_foundations_until_cancelled_or_destroyed() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Gla, Kind::TechBuilding);
    unit.name = "LimitedStructure".into();
    unit.kind_of = vec!["STRUCTURE".into()];
    unit.build_rules.limit = TypeLimit::Count(1);
    let index = game.catalog.len();
    game.catalog.push(unit);
    let builder = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    game.catalog[game.entities[builder].type_index]
        .buildable_objects
        .push("LimitedStructure".into());
    game.spawn(index, 1, [600.0, 400.0], true);
    assert!(game.prerequisites_met(0, index));
    let point = game.placement_near(index, [350.0, 250.0]).unwrap();
    let building = game.build(builder, index, point).unwrap();
    assert!(game.entities[building].under_construction());
    assert!(!game.prerequisites_met(0, index));
    let cash = game.players[0].cash;
    assert!(game.build(builder, index, [450.0, 250.0]).is_err());
    assert_eq!(game.players[0].cash, cash);
    game.cancel_construction(0, building).unwrap();
    game.process_update_destruction();
    assert!(game.prerequisites_met(0, index));
    let building = game.spawn(index, 0, point, true);
    assert!(!game.prerequisites_met(0, index));
    let damage = game.entities[building].health;
    game.apply_hit(building, builder, damage);
    assert!(game.prerequisites_met(0, index));
}

#[test]
fn nonstructure_limit_includes_queues_and_cancellation_releases_capacity() {
    let mut game = game();
    let index = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[index].name = "LimitedInfantry".into();
    game.catalog[index].build_rules.limit = TypeLimit::Count(1);
    let barracks = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[barracks].buildable_objects = vec!["LimitedInfantry".into()];
    let producer = game.spawn(barracks, 0, [350.0, 250.0], true);
    game.train(producer, index).unwrap();
    let cash = game.players[0].cash;
    assert!(game.train(producer, index).is_err());
    assert_eq!(game.players[0].cash, cash);
    let entry = game.entities[producer].queue[0].id;
    game.cancel_production(0, producer, entry).unwrap();
    assert!(game.prerequisites_met(0, index));
    game.train(producer, index).unwrap();
    let damage = game.entities[producer].health;
    game.apply_hit(producer, 0, damage);
    assert!(game.prerequisites_met(0, index));
    game.spawn(index, 1, [650.0, 400.0], true);
    assert!(game.prerequisites_met(0, index));
    game.spawn(index, 0, [350.0, 250.0], true);
    assert!(!game.prerequisites_met(0, index));
}

#[test]
fn variation_limits_match_existing_objects_but_count_only_exact_queued_templates() {
    let mut game = game();
    let wrapper = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[wrapper].name = "RandomInfantry".into();
    game.catalog[wrapper].variations = vec!["InfantryVariant".into()];
    game.catalog[wrapper].build_rules.limit = TypeLimit::Count(1);
    let mut unit = fixture_type(Faction::Gla, Kind::Infantry);
    unit.name = "InfantryVariant".into();
    unit.build_rules.limit = TypeLimit::Count(1);
    let variant = game.catalog.len();
    game.catalog.push(unit);
    let barracks = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[barracks].buildable_objects = vec!["RandomInfantry".into()];
    // A zero-production fixture incurs the source low-energy build-time penalty.
    game.catalog[barracks].energy = 1;
    let producer = game.spawn(barracks, 0, [350.0, 250.0], true);
    game.update_power();
    assert_eq!(
        game.production_frames(0, wrapper).to_bits(),
        150.0_f32.to_bits()
    );
    game.train(producer, wrapper).unwrap();
    assert!(!game.prerequisites_met(0, wrapper));
    assert!(game.prerequisites_met(0, variant));
    for _ in 0..150 {
        game.update_production(producer);
    }
    assert!(game.entities[producer].queue.is_empty());
    assert!(!game.prerequisites_met(0, wrapper));
    assert!(!game.prerequisites_met(0, variant));
    let id = game.entities.len() - 1;
    assert_eq!(game.entities[id].type_index, variant);
    game.destroy_maintenance_object(id);
    game.process_update_destruction();
    assert!(game.prerequisites_met(0, wrapper));
    assert!(game.prerequisites_met(0, variant));
}

#[test]
fn prerequisite_reskin_counts_only_completed_owned_buildings() {
    let mut game = game();
    let required = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[required].name = "Barracks".into();
    let mut unit = fixture_type(Faction::Gla, Kind::Barracks);
    unit.name = "BarracksReskin".into();
    unit.reskin = Some("Barracks".into());
    let reskin = game.catalog.len();
    game.catalog.push(unit);
    let product = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[product].build_rules.objects = vec![vec!["Barracks".into()]];
    game.spawn(reskin, 1, [650.0, 400.0], true);
    assert!(!game.prerequisites_met(0, product));
    let foundation = game.spawn(reskin, 0, [350.0, 250.0], false);
    assert!(!game.prerequisites_met(0, product));
    game.entities[foundation].build_progress = 1.0;
    game.entities[foundation]
        .drawable
        .model_conditions
        .construction = super::super::ConstructionState::Complete;
    assert!(game.prerequisites_met(0, product));
    game.destroy_maintenance_object(foundation);
    game.process_update_destruction();
    assert!(!game.prerequisites_met(0, product));
}
