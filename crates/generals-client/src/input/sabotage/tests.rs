use super::*;
use crate::{
    attack_cursor, cleanup_verification,
    control_bar::{self, CommandAction},
    input, playtest_ui,
    skirmish_verification::{construct, friendly_entity, produce_type},
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Order};

#[ignore = "requires original game installation"]
#[test]
fn installed_sabotage_button_and_context_enter_hostile_with_exact_cursor_gates() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    battle.simulation.reset(Faction::Gla, Faction::Usa).unwrap();
    let worker = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
    construct(&mut battle.simulation, worker, Kind::SupplyCenter, 0).unwrap();
    let barracks = construct(&mut battle.simulation, worker, Kind::Barracks, 0).unwrap();
    construct(&mut battle.simulation, worker, Kind::Factory, 0).unwrap();
    let saboteur = template(&battle.simulation, "GLAInfantrySaboteur").unwrap();
    let actor = produce_type(&mut battle.simulation, barracks, saboteur, 0).unwrap();
    let target = friendly_entity(&battle.simulation, Kind::CommandCenter, 1).unwrap();
    let position = battle.simulation.entities[actor].position;
    // Isolate UI admission from long-distance pathing with nearby original entities.
    battle.simulation.entities[target].position = [position[0] + 100.0, position[1]];
    battle.simulation.step();
    battle.simulation.stop(worker).unwrap();
    battle.select_objects(&[worker, actor], true);
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let command = data
        .commands(&battle.simulation, actor)
        .iter()
        .find(|command| command.name == "Command_SabotageBuilding")
        .unwrap();
    assert!(matches!(command.action, CommandAction::Sabotage));
    assert_eq!(command.image, "SUSaboteur");
    assert!(command.multi_select);
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    playtest_ui::dispatch_command(&mut battle, &mut mouse, command.action);
    assert!(matches!(mouse.pending_command, CommandMode::Sabotage));
    mouse.hovered = None;
    assert_eq!(attack_cursor::context(&mut battle, &mouse), Some(3));
    mouse.hovered = Some(barracks);
    assert_eq!(attack_cursor::context(&mut battle, &mouse), Some(3));
    input::targeted_order(&mut battle, &mut mouse, position.into(), Some(barracks));
    assert!(matches!(mouse.pending_command, CommandMode::Sabotage));
    assert!(mouse.voices.is_empty());
    mouse.hovered = Some(target);
    assert_eq!(attack_cursor::context(&mut battle, &mouse), Some(10));
    input::targeted_order(&mut battle, &mut mouse, position.into(), Some(target));
    input::commands::step(&mut battle, &mut mouse);
    assert!(
        matches!(battle.simulation.entities[actor].ai.order, Order::Enter(goal) if goal == target)
    );
    assert!(matches!(
        battle.simulation.entities[worker].ai.order,
        Order::Idle
    ));
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
    assert!(matches!(mouse.voices[0].voice(), Voice::EnterHostile));
    assert!(mouse.move_hints.is_empty());
    battle.simulation.stop(actor).unwrap();
    mouse.voices.clear();
    assert_eq!(context_target(&battle, &mouse, Some(target)), Some(target));
    assert_eq!(attack_cursor::context(&mut battle, &mouse), Some(10));
    assert!(input::context_order(
        &mut battle,
        Some(position.into()),
        Some(input::PickTarget::Unit(target)),
        &mut mouse
    ));
    input::commands::step(&mut battle, &mut mouse);
    assert!(
        matches!(battle.simulation.entities[actor].ai.order, Order::Enter(goal) if goal == target)
    );
    assert!(matches!(mouse.voices[0].voice(), Voice::EnterHostile));
    battle.simulation.stop(actor).unwrap();
    mouse.waypoint_mode = true;
    assert_eq!(context_target(&battle, &mouse, Some(target)), None);
    assert_eq!(attack_cursor::context(&mut battle, &mouse), None);
}
