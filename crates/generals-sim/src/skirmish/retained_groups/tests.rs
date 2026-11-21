use crate::skirmish::{Kind, tests::game};

#[test]
fn dispatcher_cleanup_releases_only_its_group_and_preserves_callback_transfers() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let a = game.spawn(tank, 0, [305.0, 405.0], true);
    let b = game.spawn(tank, 0, [355.0, 405.0], true);
    game.create_selection(0, &[], &[a, b], true);
    let command = game.begin_command_group(0);
    assert_eq!(command.members, [a, b]);
    game.group_move_to_world(&command.members, [505.0, 405.0, 0.0], false)
        .unwrap();
    assert!(game.entities[a].ai.command.ai_group.is_some());
    assert_eq!(
        game.entities[a].ai.command.ai_group,
        game.entities[b].ai.command.ai_group
    );
    game.finish_command_group(command);
    assert!(game.entities[a].ai.command.ai_group.is_none());
    assert!(game.entities[b].ai.command.ai_group.is_none());

    let command = game.begin_command_group(0);
    game.materialize_native_group(&[a]);
    let transferred = game.entities[a].ai.command.ai_group;
    game.finish_command_group(command);
    assert_eq!(game.entities[a].ai.command.ai_group, transferred);
    assert!(game.entities[b].ai.command.ai_group.is_none());
}

#[test]
fn selection_messages_remove_the_dispatch_group_after_callbacks() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let a = game.spawn(tank, 0, [305.0, 405.0], true);
    let b = game.spawn(tank, 0, [355.0, 405.0], true);
    let selection = game.create_selection(0, &[], &[a, b], true);
    assert_eq!(selection, vec![a, b]);
    assert!(game.entities[a].ai.command.ai_group.is_none());
    assert!(game.entities[b].ai.command.ai_group.is_none());
    assert_eq!(game.materialize_selection_group(0, &selection), selection);
    assert!(!game.single_ai_group(a));
    let old = game.entities[b].ai.command.ai_group;
    let selection = game.create_selection(0, &selection, &[a], true);
    assert_eq!(selection, vec![a]);
    assert!(game.entities[a].ai.command.ai_group.is_none());
    assert_ne!(game.entities[b].ai.command.ai_group, old);
    assert!(game.entities[b].ai.command.ai_group.is_none());
    game.destroy_selection(0, &selection);
    assert!(game.entities[a].ai.command.ai_group.is_none());
    let group = game.entities[a].ai.command.ai_group;
    game.materialize_selection_group(0, &[]);
    assert_eq!(game.entities[a].ai.command.ai_group, group);
}

#[test]
fn source_squad_duplicates_reorder_commands_and_foreign_members_leave_old_group() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let a = game.spawn(tank, 0, [305.0, 405.0], true);
    let b = game.spawn(tank, 0, [355.0, 405.0], true);
    let foreign = game.spawn(tank, 1, [405.0, 405.0], true);
    game.materialize_selection_group(1, &[foreign]);
    let selected = game.create_selection(0, &[], &[a, b, a], true);
    assert_eq!(selected, vec![a, b, a]);
    assert_eq!(
        game.materialize_selection_group(0, &[a, foreign, b, a]),
        vec![b, a]
    );
    assert!(game.entities[foreign].ai.command.ai_group.is_none());
    assert!(!game.single_ai_group(a));
    let remaining = game.remove_selection(0, &selected, &[a]);
    assert_eq!(remaining, vec![b]);
    assert!(game.entities[a].ai.command.ai_group.is_none());
    assert!(game.entities[b].ai.command.ai_group.is_none());
}

#[test]
fn actual_group_count_controls_exact_xyz_and_death_is_not_object_deletion() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let a = game.spawn(tank, 0, [305.0, 405.0], true);
    let b = game.spawn(tank, 0, [355.0, 405.0], true);
    let goal = [503.25, 405.75, 137.0];
    let ungrouped = game.adjusted_destination_world(a, goal).unwrap();
    assert_ne!(ungrouped.map(f32::to_bits), goal.map(f32::to_bits));
    game.materialize_selection_group(0, &[a]);
    assert_eq!(
        game.adjusted_destination_world(a, goal)
            .unwrap()
            .map(f32::to_bits),
        goal.map(f32::to_bits)
    );
    game.materialize_selection_group(0, &[a, b]);
    game.entities[b].health = 0.0;
    assert!(!game.single_ai_group(a));
    game.entities[b].maintenance.deleted = true;
    assert!(game.single_ai_group(a));
}

#[test]
fn structure_is_selectable_only_as_first_replacement_and_empty_message_retains_squad() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let building = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[building].kind_of.push("STRUCTURE".into());
    let a = game.spawn(tank, 0, [305.0, 405.0], true);
    let structure = game.spawn(building, 0, [505.0, 405.0], true);
    assert_eq!(game.create_selection(0, &[a], &[structure], false), vec![a]);
    assert_eq!(game.create_selection(0, &[a], &[], true), vec![a]);
    assert_eq!(
        game.create_selection(0, &[a], &[structure], true),
        vec![structure]
    );
    assert!(game.entities[structure].ai.command.ai_group.is_none());
}

#[test]
fn squad_live_objects_honors_source_always_selectable_and_no_select_precedence() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let id = game.spawn(tank, 0, [305.0, 405.0], true);
    game.catalog[tank].kind_of.push("NO_SELECT".into());
    assert!(game.materialize_selection_group(0, &[id]).is_empty());
    game.catalog[tank].kind_of.push("ALWAYS_SELECTABLE".into());
    game.entities[id].health = 0.0;
    assert_eq!(game.materialize_selection_group(0, &[id]), vec![id]);
    assert!(game.single_ai_group(id));
    game.entities[id].maintenance.deleted = true;
    assert!(game.materialize_selection_group(0, &[id]).is_empty());
}

#[test]
fn death_deselects_every_players_logic_squad_before_drawable_removal() {
    let mut game = crate::skirmish::tests::game();
    let dead = 1;
    let other = game.spawn(5, 0, [300.0, 300.0], true);
    let a = game.create_selection(0, &[], &[dead, other], true);
    let b = game.create_selection(1, &[], &[dead], true);
    assert!(a.contains(&dead));
    assert!(b.contains(&dead));
    assert!(game.selected_objects(0).contains(&other));
    game.deselect_dead_object(dead);
    assert!(!game.selected_objects(0).contains(&dead));
    assert!(!game.selected_objects(1).contains(&dead));
    assert!(game.selected_objects(0).contains(&other));
    assert_eq!(game.deselected_objects, [dead]);
    assert!(game.entities[dead].ai.command.ai_group.is_none());
}

#[test]
fn direct_destruction_deselects_without_rematerializing_surviving_groups() {
    let mut game = game();
    let deleted = game.spawn(5, 0, [300.0, 300.0], true);
    let survivor = game.spawn(5, 0, [360.0, 300.0], true);
    game.create_selection(0, &[], &[deleted, survivor], true);
    game.materialize_selection_group(0, &[deleted, survivor]);
    let group = game.entities[survivor].ai.command.ai_group;
    let revision = game.entities[survivor].ai.command_revision;
    game.players[1].selected_objects.push(deleted);
    game.destroy_maintenance_object(deleted);
    assert!(game.selected_objects(0).contains(&deleted));
    game.process_update_destruction();
    assert!(!game.entities[deleted].exists());
    assert_eq!(game.selected_objects(0), [survivor]);
    assert!(game.selected_objects(1).is_empty());
    assert_eq!(game.entities[survivor].ai.command.ai_group, group);
    assert_eq!(game.entities[survivor].ai.command_revision, revision);
    assert_eq!(game.deselected_objects, [deleted]);
    assert!(game.entities[deleted].ai.command.ai_group.is_none());
}

#[test]
fn finalizer_does_not_repeat_client_deselection_after_actual_death() {
    // SlowDeathBehavior::onDie performs the native all-player deselection.
    use crate::skirmish::{death_callbacks::Conditions, death_lifecycle};
    let mut game = game();
    crate::skirmish::tests::register_update(&mut game.catalog[5], "SlowDeathBehavior");
    let module = game.catalog[5].behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    let slow_tag = module.tag.clone();
    game.catalog[5].death_lifecycle = Some(death_lifecycle::Rules {
        on_die: Vec::new(),
        slow: vec![death_lifecycle::SlowDeath {
            derived: death_lifecycle::Derived::None,
            tag: slow_tag,
            conditions: Conditions {
                death_mask: 1 << crate::skirmish::DeathType::Normal as u32,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
            probability: 1,
            overkill_bonus: 0.0,
            sink_delay: [90, 0],
            destruction_delay: [240, 0],
            sink_rate: 0.0,
            fling: None,
            fx: std::array::from_fn(|_| Vec::new()),
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        }],
        physics: None,
        weapons: Vec::new(),
    });
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    let selected = game.create_selection(0, &[], &[actor], true);
    assert_eq!(selected, [actor]);
    game.materialize_selection_group(0, &selected);
    game.sync_drawable_selection(&selected);
    assert_eq!(game.selected_objects(0), [actor]);
    assert_eq!(game.entities[actor].drawable.selected, 1);
    assert!(game.entities[actor].has_drawable());
    assert!(!game.entities[actor].is_ai_dead());
    assert!(game.deselected_objects.is_empty());
    game.kill_object(actor, crate::skirmish::DeathType::Normal);
    assert!(game.entities[actor].is_ai_dead());
    assert_eq!(game.entities[actor].death_condition(), Some("DYING"));
    assert!(game.selected_objects(0).is_empty());
    assert_eq!(game.deselected_objects, [actor]);
    game.destroy_maintenance_object(actor);
    game.process_update_destruction();
    assert_eq!(game.deselected_objects, [actor]);
    assert!(!game.entities[actor].exists());
    assert!(game.selected_objects(0).is_empty());
}
