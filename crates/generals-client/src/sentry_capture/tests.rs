use crate::{
    cleanup_verification,
    control_bar::{self, CommandAction},
    input,
    playtest_ui::{dispatch_command, selected_command},
    sentry_verification,
    skirmish_verification::advance_until,
    source_audio::Voice,
};
use generals_sim::skirmish::Order;

#[ignore = "requires original game installation"]
#[test]
fn paid_sentry_original_buttons_allocate_gun_and_issue_guard_then_pack_for_context_move() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    let setup = sentry_verification::prepare(&mut battle.simulation).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    battle.select_objects(&[setup.factory], true);
    let button = selected_command(&battle, &data, 9).unwrap();
    let CommandAction::Research(index) = button.action else {
        panic!("source WarFactory Sentry research button missing");
    };
    let rules = &battle.simulation.catalog[battle.simulation.entities[setup.factory].type_index]
        .maintenance
        .research[index];
    assert_eq!(rules.name, "Upgrade_AmericaSentryDroneGun");
    assert_eq!((rules.cost, rules.frames), (1000, 900));
    let cash = battle.simulation.players[0].cash;
    let frame = battle.simulation.tick;
    assert!(!battle.simulation.weapon_available(setup.drone, 0));
    dispatch_command(&mut battle, &mut mouse, button.action);
    assert_eq!(battle.simulation.players[0].cash + 1000, cash);
    advance_until(
        &mut battle.simulation,
        901,
        "source Sentry Gun button",
        |game| game.has_upgrade(0, "Upgrade_AmericaSentryDroneGun"),
    )
    .unwrap();
    assert_eq!(battle.simulation.tick - frame, 900);
    assert!(battle.simulation.weapon_available(setup.drone, 0));
    battle.select_objects(&[setup.drone], true);
    let button = selected_command(&battle, &data, 13).unwrap();
    assert!(matches!(button.action, CommandAction::Guard));
    dispatch_command(&mut battle, &mut mouse, button.action);
    let point = battle.simulation.entities[setup.drone].position;
    input::targeted_order(&mut battle, &mut mouse, point.into(), None);
    assert!(matches!(
        battle.simulation.entities[setup.drone].ai.order,
        Order::Guard(_)
    ));
    assert!(
        mouse
            .voices
            .last()
            .is_some_and(|request| matches!(request.voice(), Voice::Guard))
    );
    advance_until(
        &mut battle.simulation,
        600,
        "native Guard deployment",
        |game| game.entities[setup.drone].deployment_condition() == Some("DEPLOYED"),
    )
    .unwrap();
    let destination = [point[0] + 120.0, point[1]];
    assert!(input::context_order(
        &mut battle,
        Some(destination.into()),
        None,
        &mut mouse
    ));
    assert!(
        mouse
            .voices
            .last()
            .is_some_and(|request| matches!(request.voice(), Voice::Move))
    );
    advance_until(
        &mut battle.simulation,
        600,
        "context Move packing",
        |game| game.entities[setup.drone].deployment_condition() == Some("PACKING"),
    )
    .unwrap();
    advance_until(
        &mut battle.simulation,
        1500,
        "Sentry completing the context Move",
        |game| {
            let drone = &game.entities[setup.drone];
            matches!(drone.ai.order, Order::Idle)
                && !drone.moving()
                && (drone.position[0] - destination[0]).hypot(drone.position[1] - destination[1])
                    < 15.0
        },
    )
    .unwrap();
    verify_context_attack(&mut battle, &mut mouse, &setup);
}

fn verify_context_attack(
    battle: &mut crate::playtest::Match,
    mouse: &mut input::Mouse,
    setup: &sentry_verification::Setup,
) {
    let game = &mut battle.simulation;
    let approach = setup.approach_drone(game).unwrap();
    advance_until(
        game,
        18_000,
        "enemy crossing into the Sentry clearing",
        |game| {
            let target = game.entities[setup.target].position;
            (target[0] - approach[0]).hypot(target[1] - approach[1]) < 15.0
        },
    )
    .unwrap();
    game.stop(setup.target).unwrap();
    let position = game.entities[setup.target].position;
    let health = game.entities[setup.target].health;
    assert!(input::context_order(
        battle,
        Some(position.into()),
        Some(input::PickTarget::Unit(setup.target)),
        mouse
    ));
    advance_until(
        &mut battle.simulation,
        1500,
        "Sentry context attack firing",
        |game| {
            game.entities[setup.target].health < health
                && game
                    .presentation
                    .shots
                    .iter()
                    .any(|shot| shot.source == setup.drone && shot.target == Some(setup.target))
        },
    )
    .unwrap();
    assert_eq!(
        battle.simulation.entities[setup.drone].deployment_condition(),
        Some("DEPLOYED")
    );
}
