use crate::{
    infiltrator_verification,
    skirmish_verification::{advance_until, require},
    strategy_verification::research,
};
use generals_sim::skirmish::Order;

fn game() -> generals_sim::skirmish::Skirmish {
    let root = &generals_assets::game_root().unwrap();
    infiltrator_verification::load_game(root).unwrap()
}

#[ignore = "requires original game installation"]
#[test]
fn native_scout_preparation_leaves_paid_button_and_real_detection_for_the_capture() {
    let mut game = game();
    let (parent, target) = infiltrator_verification::prepare_scout(&mut game).unwrap();
    assert!(game.produced_drone(parent).is_none());
    assert!(game.entities[target].stealthed());
    assert!(!game.logically_visible(0, target));
    assert!(game.players[0].cash >= 100);
    research(&mut game, parent, "Upgrade_AmericaScoutDrone").unwrap();
    let child = game.produced_drone(parent).unwrap();
    let scout = &game.entities[child];
    let unit = &game.catalog[scout.type_index];
    assert!(unit.kind_of.iter().any(|flag| flag == "NO_SELECT"));
    assert!(!scout.selectable(unit));
    assert!(!game.entities[target].detected());
    game.move_to(parent, game.entities[target].position)
        .unwrap();
    advance_until(
        &mut game,
        18_000,
        "native Scout staging reaches concealed target normally",
        |game| {
            game.entities[target].stealthed()
                && game.entities[target].detected()
                && game.logically_visible(0, target)
        },
    )
    .unwrap();
    game.stop(parent).unwrap();
    assert!(game.entities[target].alive());
    assert!(game.entities[child].drone_parent() == Some(parent));
}

#[ignore = "requires original game installation"]
#[test]
fn native_pathfinder_preparation_preserves_combat_earned_science_and_a_live_paid_victim() {
    let mut game = game();
    let (actor, target) =
        crate::progression_verification::pathfinder::prepare_capture(&mut game).unwrap();
    assert!(game.players[0].rank >= 3 && game.players[0].has_science("SCIENCE_Pathfinder"));
    assert!(game.entities[actor].stealthed() && game.entities[target].alive());
    let origin = game.entities[actor].position;
    let victim = game.entities[target].position;
    let distance = (origin[0] - victim[0]).hypot(origin[1] - victim[1]);
    assert!(distance > 330.0);
    let point = [
        ((origin[0] - victim[0]) / distance).mul_add(330.0, victim[0]),
        ((origin[1] - victim[1]) / distance).mul_add(330.0, victim[1]),
    ];
    game.move_to(actor, point).unwrap();
    advance_until(
        &mut game,
        3600,
        "native Pathfinder movement staging",
        |game| {
            matches!(game.entities[actor].ai.order, Order::Idle) && !game.entities[actor].moving()
        },
    )
    .unwrap();
    assert!(game.entities[target].alive() && game.entities[actor].stealthed());
    require(
        game.visible_to(0, target),
        "native Pathfinder victim must be legitimately revealed",
    )
    .unwrap();
    game.attack(actor, target).unwrap();
    advance_until(
        &mut game,
        3600,
        "native Pathfinder staging permits original sniper kill",
        |game| !game.entities[target].alive(),
    )
    .unwrap();
    assert!(game.entities[actor].alive());
}
