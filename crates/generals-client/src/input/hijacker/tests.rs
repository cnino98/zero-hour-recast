use crate::{
    attack_cursor,
    control_bar::{self, CommandAction},
    input::{self, CommandMode},
    playtest_ui,
    source_audio::Voice,
};
use generals_sim::skirmish::Order;

#[ignore = "requires original game installation"]
#[test]
fn paid_original_hijacker_button_and_context_use_enter_hostile_and_aggressive_cursor() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = crate::hijacker_verification::prepare(&mut battle.simulation).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    battle.started = true;
    battle.select_objects(&[setup.actor], true);
    let command = data
        .commands(&battle.simulation, setup.actor)
        .iter()
        .find(|command| matches!(command.action, CommandAction::Hijack))
        .unwrap();
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    playtest_ui::dispatch_command(&mut battle, &mut mouse, command.action);
    assert!(matches!(mouse.pending_command, CommandMode::Hijack));
    mouse.hovered = None;
    assert_eq!(attack_cursor::context(&mut battle, &mouse), Some(3));
    mouse.hovered = Some(setup.host);
    assert_eq!(attack_cursor::context(&mut battle, &mouse), Some(10));
    let point = battle.simulation.entities[setup.host].position;
    input::targeted_order(&mut battle, &mut mouse, point.into(), Some(setup.host));
    assert!(
        matches!(battle.simulation.entities[setup.actor].ai.order, Order::Enter(target) if target == setup.host)
    );
    assert!(matches!(mouse.voices[0].voice(), Voice::EnterHostile));
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
    battle.simulation.stop(setup.actor).unwrap();
    mouse.voices.clear();
    assert!(input::context_order(
        &mut battle,
        None,
        Some(input::PickTarget::Unit(setup.host)),
        &mut mouse
    ));
    assert!(
        matches!(battle.simulation.entities[setup.actor].ai.order, Order::Enter(target) if target == setup.host)
    );
    assert!(matches!(mouse.voices[0].voice(), Voice::EnterHostile));
}
