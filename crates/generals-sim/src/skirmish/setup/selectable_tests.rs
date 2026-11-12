//! Native Object bit, independent status, map overrides and Drawable selection.
use super::*;
use crate::skirmish::{
    BodyKind, Kind, containment,
    tests::{fixture_type, game},
};

fn placed(game: &mut Skirmish, name: &str, selectable: Option<bool>) -> usize {
    let index = game.layout.neutral_objects.len();
    game.layout.neutral_objects.push(NeutralObject {
        initial_health: None,
        attitude: None,
        recruitable: None,
        selectable,
        original_team: "teamplayer0".into(),
        script_name: None,
        name: name.into(),
        position: [355.0, 305.0, 0.0],
        angle: 0.0,
        script_targetable: false,
        indestructible: false,
    });
    game.spawn_neutral_object(index, false).unwrap()
}

#[test]
fn map_selectability_overrides_raw_template_without_mutating_it() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::Infantry);
    unit.name = "MappedSelection".into();
    unit.selectable = false;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let default = placed(&mut game, "MappedSelection", None);
    let enabled = placed(&mut game, "MappedSelection", Some(true));
    let disabled = placed(&mut game, "MappedSelection", Some(false));
    assert!(!game.catalog[kind].selectable);
    assert!(!game.entities[default].selectable(&game.catalog[kind]));
    assert!(game.entities[enabled].selectable(&game.catalog[kind]));
    assert!(!game.entities[disabled].selectable(&game.catalog[kind]));
    let selection = game.create_selection(0, &[], &[enabled], true);
    assert_eq!(selection, [enabled]);
    assert_eq!(
        game.create_selection(0, &selection, &[disabled], false),
        selection
    );
    assert_eq!(
        game.materialize_selection_group(0, &[default, enabled, disabled]),
        [enabled]
    );
}

#[test]
fn false_instance_remains_false_after_real_container_status_is_cleared() {
    let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
    let mut unit = fixture_type(Faction::Civilian, Kind::Infantry);
    unit.name = "UnselectableRider".into();
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let rider = placed(&mut game, "UnselectableRider", Some(false));
    assert!(!game.entities[rider].selectable(&game.catalog[kind]));
    assert!(!game.entities[rider].unselectable());
    game.add_to_contain(rider, carrier);
    assert!(game.entities[rider].unselectable());
    game.set_map_body_controls(rider, true, true);
    assert!(game.entities[rider].unselectable());
    game.remove_passenger(rider, carrier);
    assert!(!game.entities[rider].unselectable());
    assert!(game.entities[rider].contained_by().is_none());
    assert!(!game.entities[rider].selectable(&game.catalog[kind]));
    game.set_object_selectable(rider, true);
    assert!(game.entities[rider].selectable(&game.catalog[kind]));
}

#[test]
fn native_always_selectable_precedes_no_select_status_and_effective_death() {
    let (mut game, carrier) = containment::tests::setup(0.0, 0.0);
    let mut unit = fixture_type(Faction::Civilian, Kind::Infantry);
    unit.name = "AlwaysSelectableRider".into();
    unit.selectable = false;
    unit.kind_of
        .extend(["ALWAYS_SELECTABLE".into(), "NO_SELECT".into()]);
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let actor = placed(&mut game, "AlwaysSelectableRider", Some(false));
    assert!(game.entities[actor].selectable(&game.catalog[kind]));
    game.add_to_contain(actor, carrier);
    assert!(game.entities[actor].unselectable());
    assert!(game.entities[actor].selectable(&game.catalog[kind]));
    game.remove_passenger(actor, carrier);
    game.change_body_health(actor, -f32::MAX);
    assert!(game.entities[actor].is_effectively_dead());
    assert!(game.entities[actor].selectable(&game.catalog[kind]));
}

#[test]
fn no_select_and_inactive_effective_death_remain_native_query_guards() {
    let mut game = game();
    let mut unit = fixture_type(Faction::Civilian, Kind::TechBuilding);
    unit.name = "InactiveSelection".into();
    unit.body_kind = BodyKind::Inactive;
    unit.initial_health = 0.0;
    unit.max_health = 0.0;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let inactive = placed(&mut game, "InactiveSelection", Some(true));
    assert_eq!(game.entities[inactive].health.to_bits(), 0.0_f32.to_bits());
    // InactiveBody constructor explicitly sets effective death, independent of AI.
    assert!(game.entities[inactive].is_effectively_dead());
    assert!(game.entities[inactive].selectable);
    assert!(!game.entities[inactive].selectable(&game.catalog[kind]));
    let mut unit = fixture_type(Faction::Civilian, Kind::Infantry);
    unit.name = "NoSelect".into();
    unit.kind_of.push("NO_SELECT".into());
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let guarded = placed(&mut game, "NoSelect", Some(true));
    assert!(game.entities[guarded].selectable);
    assert!(!game.entities[guarded].selectable(&game.catalog[kind]));
}

#[test]
fn drawable_deselection_preserves_logic_squad_groups_and_other_command() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let a = game.spawn(kind, 0, [305.0, 405.0], true);
    let b = game.spawn(kind, 0, [355.0, 405.0], true);
    game.move_to(b, [605.0, 405.0]).unwrap();
    let selection = game.create_selection(0, &[], &[a, b], true);
    game.materialize_selection_group(0, &selection);
    game.sync_drawable_selection(&selection);
    let groups = [
        game.entities[a].ai.command.ai_group,
        game.entities[b].ai.command.ai_group,
    ];
    let command = game.entities[b].ai_goal_position().map(f32::to_bits);
    let revision = game.entities[b].ai.command_revision;
    let notices = game.deselected_objects.len();
    game.set_object_selectable(a, false);
    assert!(!game.entities[a].selectable(&game.catalog[kind]));
    assert_eq!(game.entities[a].drawable.selected, 0);
    assert_eq!(game.entities[b].drawable.selected, 1);
    assert_eq!(&game.deselected_objects[notices..], &[a]);
    assert_eq!(game.selected_objects(0), selection);
    assert_eq!(
        [
            game.entities[a].ai.command.ai_group,
            game.entities[b].ai.command.ai_group
        ],
        groups
    );
    assert_eq!(
        game.entities[b].ai_goal_position().map(f32::to_bits),
        command
    );
    assert_eq!(game.entities[b].ai.command_revision, revision);
    game.set_object_selectable(a, true);
    assert!(game.entities[a].selectable(&game.catalog[kind]));
    assert_eq!(game.entities[a].drawable.selected, 0);
    assert_eq!(game.deselected_objects.len(), notices + 1);
}
