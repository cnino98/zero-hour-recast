use super::*;
use crate::{
    skirmish_verification::{construct, friendly_entity, produce},
    terrain::Terrain,
};
use generals_assets::map::MapData;
use generals_sim::skirmish::{Faction, Order};

fn original_golden() -> (Match, Mouse, usize) {
    let root = &generals_assets::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    let map = MapData::parse(
        &assets
            .read("maps/golden oasis/golden oasis.map")
            .unwrap()
            .bytes,
    )
    .unwrap();
    let mut data = crate::skirmish_data::load(&assets, &map.objects).unwrap();
    let settings = crate::world_data::camera_settings(&assets, root).unwrap();
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )
    .unwrap();
    let (layout, _) = crate::world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )
    .unwrap();
    let movement = terrain
        .classify_movement(&map.triggers, &map.objects)
        .unwrap();
    let mut game = data.into_simulation(movement, layout).unwrap();
    game.reset(Faction::Usa, Faction::China).unwrap();
    let builder = friendly_entity(&game, Kind::Builder, 0).unwrap();
    construct(&mut game, builder, Kind::PowerPlant, 0).unwrap();
    construct(&mut game, builder, Kind::SupplyCenter, 0).unwrap();
    let factory = construct(&mut game, builder, Kind::Factory, 0).unwrap();
    let tank = produce(&mut game, factory, Kind::Tank, 0).unwrap();
    let mouse = load(&assets, &root.join("UserData/Options.ini"), &game.catalog).unwrap();
    // The new cursor is decoded from the installed Mouse.ini/ANI data too.
    crate::attack_cursor::load(&assets).unwrap();
    (
        Match {
            game_speed: 30,
            observer_player: None,
            observer_players: Vec::new(),
            simulation: game,
            selected: vec![tank],
            logic_selection: vec![tank],
            placing: None,
            started: true,
            paused: false,
            promotions_open: false,
            generation: 0,
            opponent_difficulty: None,
            opponent_faction: Faction::China,
        },
        mouse,
        tank,
    )
}

#[ignore = "requires original game installation"]
#[test]
fn original_targetable_tower_uses_player_attack_cursor_order_and_voice() {
    let (mut battle, mut mouse, tank) = original_golden();
    let target = battle
        .simulation
        .entities
        .iter()
        .position(|unit| {
            battle.simulation.catalog[unit.type_index].name == "StanWatchTowerTall"
                && unit.script_targetable()
        })
        .unwrap();
    assert!(
        !battle
            .simulation
            .enemies(0, battle.simulation.entities[target].owner)
    );
    mouse.hovered = Some(target);
    assert_eq!(crate::attack_cursor::context(&mut battle, &mouse), Some(0));
    assert!(context_order(
        &mut battle,
        None,
        Some(PickTarget::Unit(target)),
        &mut mouse
    ));
    assert!(matches!(battle.simulation.entities[tank].ai.order, Order::Attack(id) if id == target));
    assert!(matches!(
        mouse.voices.last().unwrap().voice(),
        Voice::Attack
    ));
    first_selected_force_gate(&mut battle, &mut mouse, tank, target);
    force_neutral_and_allied_objects(&mut battle, &mut mouse, tank);
}

fn force_neutral_and_allied_objects(battle: &mut Match, mouse: &mut Mouse, tank: usize) {
    let neutral = battle
        .simulation
        .entities
        .iter()
        .enumerate()
        .find(|(id, unit)| {
            unit.owner == battle.simulation.neutral_owner()
                && !unit.script_targetable()
                && unit.selectable(&battle.simulation.catalog[unit.type_index])
                && battle.simulation.force_attack_target_reachable(tank, *id)
        })
        .map(|(id, _)| id)
        .unwrap();
    battle.simulation.stop(tank).unwrap();
    mouse.voices.clear();
    mouse.hovered = Some(neutral);
    assert_ne!(crate::attack_cursor::context(battle, mouse), Some(0));
    assert!(!context_order(
        battle,
        None,
        Some(PickTarget::Unit(neutral)),
        mouse
    ));
    assert!(mouse.voices.is_empty());
    let allied = friendly_entity(&battle.simulation, Kind::CommandCenter, 0).unwrap();
    mouse
        .placement_modifiers
        .event(KeyCode::ControlLeft, true, false);
    for target in [neutral, allied] {
        mouse.hovered = Some(target);
        mouse.waypoint_mode = true;
        mouse.pending_command = CommandMode::AttackMove;
        assert_eq!(crate::attack_cursor::context(battle, mouse), Some(13));
        mouse.set_alternate_mouse(false);
        left_click(battle, None, Some(PickTarget::Unit(target)), true, mouse);
        assert!(
            matches!(battle.simulation.entities[tank].ai.order, Order::Attack(id) if id == target)
        );
        assert_eq!(battle.selected, [tank]);
        assert!(matches!(
            mouse.voices.last().unwrap().voice(),
            Voice::Attack
        ));
    }
    // Alternate left remains selection; its context button alone forces fire.
    mouse.set_alternate_mouse(true);
    mouse.pending_command = CommandMode::Normal;
    battle.simulation.stop(tank).unwrap();
    left_click(battle, None, Some(PickTarget::Unit(tank)), false, mouse);
    assert!(!matches!(
        battle.simulation.entities[tank].ai.order,
        Order::Attack(_)
    ));
    right_click(battle, mouse, Some(PickTarget::Unit(allied)), None);
    assert!(matches!(battle.simulation.entities[tank].ai.order, Order::Attack(id) if id == allied));
    force_ground_and_modifier_reset(battle, mouse, tank);
}

fn force_ground_and_modifier_reset(battle: &mut Match, mouse: &mut Mouse, tank: usize) {
    battle.simulation.stop(tank).unwrap();
    mouse.hovered = None;
    let count = mouse.voices.len();
    mouse.world_position = Some(Vec3::new(
        500.0,
        500.0,
        battle.simulation.ground_height([500.0, 500.0]),
    ));
    assert_eq!(crate::attack_cursor::context(battle, mouse), Some(12));
    assert!(context_order(
        battle,
        Some(Vec2::new(500.0, 500.0)),
        None,
        mouse
    ));
    assert!(!matches!(
        battle.simulation.entities[tank].ai.order,
        Order::Move(_)
    ));
    assert!(matches!(
        battle.simulation.entities[tank].ai.order,
        Order::FireAt { .. }
    ));
    assert_eq!(mouse.voices.len(), count + 1);
    mouse
        .placement_modifiers
        .event(KeyCode::ControlLeft, false, false);
    assert!(!mouse.force_attack_mode());
    mouse
        .placement_modifiers
        .event(KeyCode::ControlRight, true, false);
    mouse.reset_generation(1);
    assert!(!mouse.force_attack_mode());
}

fn first_selected_force_gate(battle: &mut Match, mouse: &mut Mouse, tank: usize, target: usize) {
    mouse.set_alternate_mouse(false);
    let builder = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
    battle.simulation.stop(tank).unwrap();
    mouse
        .placement_modifiers
        .event(KeyCode::ControlLeft, true, false);
    battle.select_objects(&[builder, tank], true);
    let voices = mouse.voices.len();
    // The dozer has a mine-clearing weapon. CommandXlat emits an impossible
    // attack hint for its invalid shot at a tower and sends no attack command.
    assert_eq!(crate::attack_cursor::context(battle, mouse), Some(3));
    left_click(battle, None, Some(PickTarget::Unit(target)), false, mouse);
    assert!(!matches!(
        battle.simulation.entities[tank].ai.order,
        Order::Attack(_)
    ));
    assert_eq!(mouse.voices.len(), voices);
    battle.select_objects(&[tank, builder], true);
    assert_eq!(crate::attack_cursor::context(battle, mouse), Some(13));
    left_click(battle, None, Some(PickTarget::Unit(target)), false, mouse);
    assert!(matches!(battle.simulation.entities[tank].ai.order, Order::Attack(id) if id == target));
    assert!(!matches!(
        battle.simulation.entities[builder].ai.order,
        Order::Attack(_)
    ));
    assert_eq!(mouse.voices.len(), voices + 1);
    battle.select_objects(&[tank], true);
    mouse
        .placement_modifiers
        .event(KeyCode::ControlLeft, false, false);
}
