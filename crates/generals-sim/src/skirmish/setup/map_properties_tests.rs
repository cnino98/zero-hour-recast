//! Actual map property application after Object construction, before script flags.
use super::*;
use crate::skirmish::{
    BodyKind, Kind, Order, UnitAiUpdate,
    tests::{fixture_type, game},
};

fn record(name: &str) -> NeutralObject {
    NeutralObject {
        initial_health: Some(50),
        attitude: Some(-2),
        recruitable: Some(false),
        original_team: "team".into(),
        script_name: Some("AuthoredPropertyObject".into()),
        name: name.into(),
        position: [355.0, 205.0, 0.0],
        angle: 0.0,
        script_targetable: false,
        indestructible: false,
        selectable: None,
    }
}

#[test]
fn mapped_initial_health_uses_original_initial_not_max_and_never_fires_die() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = "MappedStorageTank".into();
    unit.initial_health = 80.0;
    unit.max_health = 200.0;
    game.catalog.push(unit);
    let record_index = game.layout.neutral_objects.len();
    game.layout
        .neutral_objects
        .push(record("MappedStorageTank"));
    let actor = game.spawn_neutral_object(record_index, false).unwrap();
    assert_eq!(game.entities[actor].health.to_bits(), 40.0_f32.to_bits());
    assert_eq!(
        game.entities[actor].damage_state.previous_health.to_bits(),
        80.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[actor].damage_state.initial_health.to_bits(),
        80.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[actor].max_health.to_bits(),
        200.0_f32.to_bits()
    );
    assert!(game.entities[actor].exists());
    assert!(!game.entities[actor].is_effectively_dead());
    assert_eq!(game.body_damage_state(actor), 1);
    game.layout.neutral_objects[record_index].initial_health = Some(0);
    let zero = game.spawn_neutral_object(record_index, false).unwrap();
    assert!(game.entities[zero].exists());
    assert!(game.entities[zero].is_effectively_dead());
    assert!(!game.entities[zero].is_ai_dead());
    assert!(!game.entities[zero].corpse_present());
}

#[test]
fn absent_ai_and_inactive_body_ignore_their_map_interface_properties() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = "MappedInactiveScenery".into();
    unit.body_kind = BodyKind::Inactive;
    unit.initial_health = 0.0;
    unit.max_health = 0.0;
    unit.ai.update = UnitAiUpdate::Absent;
    unit.behavior_modules.clear();
    unit.update_modules.clear();
    game.catalog.push(unit);
    let record_index = game.layout.neutral_objects.len();
    game.layout
        .neutral_objects
        .push(record("MappedInactiveScenery"));
    let actor = game.spawn_neutral_object(record_index, false).unwrap();
    assert_eq!(game.entities[actor].health.to_bits(), 0.0_f32.to_bits());
    assert!(game.entities[actor].ai.registration.is_none());
    assert!(game.entities[actor].ai.recruitable);
    assert_eq!(game.entities[actor].ai.mood.attitude, 0);
}

#[test]
fn authored_sleeping_ai_rejects_ai_moves_but_preserves_human_commands() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::Infantry);
    unit.name = "MappedCivilianVehicle".into();
    game.catalog.push(unit);
    game.slots[1].difficulty = Some(OpponentDifficulty::Easy);
    let mut computer = record("MappedCivilianVehicle");
    computer.original_team = "teamplayer1".into();
    let record_index = game.layout.neutral_objects.len();
    game.layout.neutral_objects.push(computer);
    let actor = game.spawn_neutral_object(record_index, false).unwrap();
    assert_eq!(game.entities[actor].owner, 1);
    assert!(game.computer_player(1));
    assert!(game.entities[actor].ai.registration.is_some());
    assert!(game.entities[actor].ai.primary.initialized);
    assert_eq!(game.entities[actor].ai.mood.attitude, -2);
    assert!(!game.entities[actor].ai.recruitable);
    let goal = game.entities[actor].ai_goal_position();
    game.construction_move(actor, [500.0, 205.0, 0.0]);
    assert!(game.entities[actor].ai_idle());
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        goal.map(f32::to_bits)
    );

    // Object applies map attitude after creating the object on its actual team.
    // A separate human-owned map object tests the controller distinction without
    // a capture resetting native team attitude before onCapture.
    let mut human = record("MappedCivilianVehicle");
    human.original_team = "teamplayer0".into();
    human.script_name = Some("AuthoredHumanPropertyObject".into());
    human.position = [355.0, 240.0, 0.0];
    let record_index = game.layout.neutral_objects.len();
    game.layout.neutral_objects.push(human);
    let actor = game.spawn_neutral_object(record_index, false).unwrap();
    assert_eq!(game.entities[actor].owner, 0);
    assert!(!game.computer_player(0));
    assert_eq!(game.entities[actor].ai.mood.attitude, -2);
    game.move_to(actor, [500.0, 240.0]).unwrap();
    assert!(matches!(game.entities[actor].ai.order, Order::Move(_)));
    let before = game.object_position(actor);
    for _ in 0..20 {
        game.step();
    }
    assert_ne!(
        game.object_position(actor).map(f32::to_bits),
        before.map(f32::to_bits)
    );
}

#[test]
fn initial_percentage_preserves_the_concrete_body_virtual_health_rules() {
    for (name, body, minimum, percentage, expected) in [
        ("ActiveZero", BodyKind::Active, 0.0, 0, 0.0_f32),
        ("HighlanderZero", BodyKind::Highlander, 0.0, 0, 0.0),
        // The parser's ImmortalBody identity is active + its virtual health floor.
        ("ImmortalZero", BodyKind::Active, 1.0, 0, 1.0),
        ("ActiveNegative", BodyKind::Active, 0.0, -50, 0.0),
        ("ActiveUpperCap", BodyKind::Active, 0.0, 500, 200.0),
    ] {
        let mut game = game();
        let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
        unit.name = name.into();
        unit.body_kind = body;
        unit.minimum_health = minimum;
        unit.initial_health = 80.0;
        unit.max_health = 200.0;
        game.catalog.push(unit);
        let mut placed = record(name);
        placed.initial_health = Some(percentage);
        let record_index = game.layout.neutral_objects.len();
        game.layout.neutral_objects.push(placed);
        let actor = game.spawn_neutral_object(record_index, false).unwrap();
        assert_eq!(game.entities[actor].health.to_bits(), expected.to_bits());
        assert_eq!(
            game.entities[actor].damage_state.previous_health.to_bits(),
            80.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[actor].damage_state.initial_health.to_bits(),
            80.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[actor].is_effectively_dead(),
            expected.to_bits() == 0.0_f32.to_bits()
        );
        assert!(game.entities[actor].exists());
        assert!(!game.entities[actor].is_ai_dead());
        assert!(!game.entities[actor].corpse_present());
    }
}
