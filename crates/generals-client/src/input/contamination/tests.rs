use super::*;
use crate::{
    cleanup_verification, control_bar,
    playtest_ui::{dispatch_command, selected_command},
    skirmish_verification::produce_type,
    source_audio::cleanup_verification::tests::assert_candidates,
    tractor_verification,
};
use generals_sim::skirmish::Order;

#[ignore = "requires original game installation"]
#[test]
fn original_contamination_button_targets_both_tractors_and_keeps_mixed_commands_hidden() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    drop(crate::attack_cursor::load(&assets).unwrap());
    let setup = tractor_verification::prepare(&mut battle.simulation).unwrap();
    let unit = battle.simulation.entities[setup.actor].type_index;
    let second = produce_type(&mut battle.simulation, setup.factory, unit, 0).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let ui = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = crate::input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    battle.select_objects(&[setup.actor, setup.builder], true);
    assert!(selected_command(&battle, &ui, 1).is_none());
    battle.select_objects(&[setup.actor, second], true);
    let command = selected_command(&battle, &ui, 1).unwrap();
    assert!(matches!(
        command.action,
        control_bar::CommandAction::FireWeapon(1)
    ));
    dispatch_command(&mut battle, &mut mouse, command.action);
    assert!(matches!(mouse.pending_command, CommandMode::FireWeapon(1)));
    assert_eq!(crate::attack_cursor::context(&mut battle, &mouse), Some(5));
    assert!(matches!(mouse.voices[0].voice(), Voice::ContaminateMode));
    let origin = battle.simulation.entities[setup.actor].position;
    let point = [origin[0] + 4.0, origin[1]];
    super::super::targeted_order(&mut battle, &mut mouse, point.into(), None);
    crate::input::commands::step(&mut battle, &mut mouse);
    for id in [setup.actor, second] {
        let actor = &battle.simulation.entities[id];
        assert!(matches!(actor.ai.order, Order::FireAt { position, weapon }
            if position.map(f32::to_bits) == point.map(f32::to_bits)
                && weapon == actor.weapon_index(1)));
        assert_eq!(actor.weapon_slot(), 1);
    }
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
    assert_eq!(mouse.voices.len(), 2);
    assert!(matches!(mouse.voices[1].voice(), Voice::PoisonLocation));
    assert_candidates(&mouse.voices[1], &[setup.actor, second]);
    assert!(mouse.move_hints.is_empty());
}
