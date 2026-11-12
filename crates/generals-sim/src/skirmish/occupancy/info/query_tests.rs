use crate::skirmish::{Kind, tests::game};

#[test]
fn searched_building_infos_survive_removal_until_the_clear_cell_is_searched_again() {
    let mut game = game();
    for id in 0..game.entities.len() {
        game.destroy_maintenance_object(id);
    }
    game.process_update_destruction();
    assert!(game.entities.iter().all(|actor| !actor.exists()));
    let building = game.type_for(0, Kind::CommandCenter).unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let producer = game.spawn(building, 0, [285.0, 305.0], true);
    let actor = game.spawn(tank, 0, [285.0, 305.0], true);
    game.rebuild_navigation();
    let result = game.movement_path_counted(actor, [285.0, 305.0], [455.0, 305.0]);
    let route = result.route.expect("native tunneling leaves the producer");
    let retained = *route
        .raw
        .iter()
        .find(|&&node| {
            game.navigation.layer_obstacle(node)
                && !game.occupancy.positions.contains_key(&node)
                && !game.occupancy.goals.contains_key(&node)
        })
        .expect("searched obstacle cell without mobile claims");
    let info = game.occupancy.infos[&retained];
    assert!(info.cost >= 1000);
    assert!(info.total >= info.cost);
    game.entities[producer].health = 0.0;
    game.rebuild_navigation();
    assert!(!game.navigation.layer_obstacle(retained));
    assert_eq!(game.occupancy.infos[&retained].cost, info.cost);
    assert_eq!(game.occupancy.infos[&retained].total, info.total);
    let goal = game.path_position(actor, retained.cell);
    let result = game.movement_path_counted(actor, [285.0, 305.0], goal);
    assert!(result.route.is_some());
    assert!(!game.occupancy.infos.contains_key(&retained));
}
