use crate::skirmish::{
    Skirmish,
    map_scripts::{
        Action, BuildEntry, BuildList, Condition, Script, SideDefinition, TeamDefinition,
    },
    startup,
};

pub(super) fn game(actions: Vec<Action>) -> Skirmish {
    let mut game = crate::skirmish::tests::game();
    game.ai_rules.unit_damaged_threshold = 0.5;
    game.catalog[1].locomotor.ground = Some(crate::skirmish::ground_movement::tests::ground_rules(
        crate::skirmish::locomotor::GroundAppearance::Legs,
    ));
    let neutral_templates: Vec<_> = game
        .layout
        .neutral_objects
        .iter()
        .map(|object| {
            game.catalog
                .iter()
                .position(|unit| unit.name == object.name)
                .expect("Fixture neutral template")
        })
        .collect();
    for (index, unit) in game.catalog.iter_mut().enumerate() {
        unit.name = format!("NativeTemplate{index}");
    }
    for (object, template) in game
        .layout
        .neutral_objects
        .iter_mut()
        .zip(neutral_templates)
    {
        object.name.clone_from(&game.catalog[template].name);
    }
    game.layout.map_template_order = game
        .catalog
        .iter()
        .rev()
        .map(|unit| super::super::TemplateDefinition {
            name: unit.name.clone(),
            dozer: unit.kind_of.iter().any(|flag| flag == "DOZER"),
            harvester: unit.kind_of.iter().any(|flag| flag == "HARVESTER"),
        })
        .collect();
    game.layout.map_sides = vec![SideDefinition {
        gatherers: [0; 3],
        name: "NPC".into(),
        faction: "FactionCivilian".into(),
        human: false,
        template_money: 9000,
        start_money: 500,
        allies: Vec::new(),
        enemies: Vec::new(),
    }];
    game.layout.map_teams = vec![TeamDefinition {
        name: "teamNPC".into(),
        owner: "NPC".into(),
        singleton: true,
        attack_common_target: false,
        initial_idle_frames: 0,
        owner_faction: "FactionCivilian".into(),
        owner_human: false,
        owner_allies: Vec::new(),
        owner_enemies: Vec::new(),
    }];
    game.layout.map_build_lists = vec![BuildList {
        owner: "NPC".into(),
        entries: vec![BuildEntry {
            name: "NPC_Barracks".into(),
            template: game.catalog[3].name.clone(),
            position: [500.0, 200.0],
            angle: 0.75,
            initially_built: true,
            rebuilds: 2,
            script: "Startup".into(),
            health: 17,
            properties: BuildEntry::UNSELLABLE | BuildEntry::REPAIRABLE,
        }],
    }];
    game.install_map_scripts(vec![Script {
        owner: String::new(),
        name: "Startup".into(),
        active: true,
        one_shot: false,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: true,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions,
        false_actions: Vec::new(),
    }]);
    game.reset_slots(game.slots.clone()).unwrap();
    game
}

#[test]
fn npc_initial_structure_preserves_rng_health_properties_and_attached_script_order() {
    let baseline = game(Vec::new());
    let game = game(vec![Action::RandomTimer("Attached".into(), 5, 9)]);
    let mut expected = baseline.random;
    startup::random_value(&mut expected);
    assert_eq!(game.random, expected);
    // The attached script runs after its object's three nested idle states and
    // primary idle enter, before the playable command centers and builders.
    let starts = game
        .layout
        .starts
        .iter()
        .map(|start| start.position)
        .collect::<Vec<_>>();
    let mut before_script = crate::skirmish::setup::resolve_player_slots(
        &game.slots,
        game.layout.random_seed,
        8,
        &starts,
    )
    .unwrap()
    .random;
    for _ in 0..4 {
        startup::random_between(&mut before_script, 60);
    }
    let value = 5 + i32::try_from(startup::random_between(&mut before_script, 4)).unwrap();
    assert_eq!(game.map_scripts.counters["Attached"].value, value);
    let id = game.map_objects["NPC_Barracks"];
    let owner = game.map_side_state.owners["NPC"];
    assert_eq!(game.entities[id].owner, owner);
    assert_eq!(
        game.entities[id].position.map(f32::to_bits),
        [500.0_f32, 200.0].map(f32::to_bits)
    );
    assert_eq!(game.entities[id].facing.to_bits(), 0.75_f32.to_bits());
    assert_eq!(
        game.entities[id].health.to_bits(),
        game.entities[id].max_health.to_bits()
    );
    assert!(game.entities[id].complete());
    assert!(!game.can_sell(owner, id));
    assert!(game.entities[id].selectable(&game.catalog[game.entities[id].type_index]));
    assert_eq!(game.players[owner].cash, 9500);
}

#[test]
fn native_new_map_retains_existing_empty_production_factories_without_instant_building() {
    let mut game = game(Vec::new());
    let existing = game.map_objects["NPC_Barracks"];
    let type_index = game.entities[existing].type_index;
    game.catalog[type_index].production = Some(Vec::new());
    game.entities[existing].maintenance.masked = true;
    game.layout.map_build_lists[0].entries.clear();
    game.map_scripts.npc_builds.clear();
    let before = game.entities.len();
    game.initialize_map_build_lists().unwrap();
    assert_eq!(game.entities.len(), before);
    let state = &game.map_scripts.npc_builds[0];
    assert_eq!(state.records.len(), 1);
    assert_eq!(state.records[0].object, Some(existing));
    assert_eq!(state.records[0].remaining, 0);
    assert_eq!(state.records[0].timestamp, 0);
    assert!(state.entries[0].script.is_empty());
    assert!(game.layout.map_build_lists[0].entries.is_empty());
    game.catalog[type_index].production = None;
    game.map_scripts.npc_builds.clear();
    game.initialize_map_build_lists().unwrap();
    assert!(game.map_scripts.npc_builds[0].records.is_empty());
}

#[test]
fn npc_build_permissions_preserve_player_defaults_and_current_script_side() {
    let mut game = game(Vec::new());
    let owner = game.map_side_state.owners["NPC"];
    assert!(game.map_build_allowed(owner, false));
    assert!(!game.map_build_allowed(owner, true));
    assert!(game.map_build_allowed(0, true));
    game.install_map_scripts(vec![Script {
        owner: "NPC".into(),
        name: "EnableNPCUnits".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::PlayerBuild("<This Player>".into(), true, true)],
        false_actions: Vec::new(),
    }]);
    game.update_map_scripts();
    assert!(game.map_build_allowed(owner, true));
    game.set_map_build_allowed(owner, true, false);
    game.install_map_scripts(vec![Script {
        owner: "NPC".into(),
        name: "AttachedUnits".into(),
        active: true,
        one_shot: false,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: true,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::PlayerBuild("<This Player>".into(), true, true)],
        false_actions: Vec::new(),
    }]);
    // runObjectScript changes callingObject, never currentPlayer.
    game.run_attached_map_script("AttachedUnits", game.map_objects["NPC_Barracks"]);
    assert!(!game.map_build_allowed(owner, true));
}
