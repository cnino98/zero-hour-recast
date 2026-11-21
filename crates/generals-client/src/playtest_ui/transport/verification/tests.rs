//! Paid mixed infantry preserve the source list and its actual inventory actions.
use super::*;

fn assert_inventory(battle: &Match, data: &UiData, expected: &[usize]) {
    for (index, &id) in expected.iter().enumerate() {
        assert_eq!(passenger(battle, index), Some(id));
        let command = selected_command(battle, data, index + 4).unwrap();
        assert!(matches!(command.action, CommandAction::ExitContainer(slot) if slot == index));
        let unit = &data.units[battle.simulation.entities[id].type_index];
        assert_eq!(
            super::super::image(battle, data, command),
            unit.button_image
        );
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_transport_inventory_keeps_boarding_order_and_reentry_appends_the_same_actor() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    let mut app = App::new();
    app.insert_resource(data).insert_resource(mouse);
    // Exercise the native capture's actual paid preparation and inventory gate.
    prepare(&mut app, &mut battle).unwrap();
    let check = app.world().resource::<Check>();
    let carrier = check.carrier;
    let mut boarded = check.riders.clone();
    assert_eq!(boarded.len(), 5);
    let first = boarded[0];
    assert_eq!(
        battle.simulation.catalog[battle.simulation.entities[first].type_index].name,
        "AmericaInfantryMissileDefender"
    );
    assert!(boarded[1..].iter().all(|&id| {
        battle.simulation.catalog[battle.simulation.entities[id].type_index].name
            == "AmericaInfantryRanger"
    }));
    let data = app.world_mut().remove_resource::<UiData>().unwrap();
    let mut mouse = app.world_mut().remove_resource::<Mouse>().unwrap();
    assert_inventory(&battle, &data, &boarded);
    let exit = selected_command(&battle, &data, 4).unwrap().action;
    dispatch_command(&mut battle, &mut mouse, exit);
    input::commands::step(&mut battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        100,
        "first inventory passenger exit",
        |game| game.entities[first].contained_by().is_none(),
    )
    .unwrap();
    assert_eq!(battle.simulation.passengers(carrier), &boarded[1..]);
    battle.select_objects(&[first], true);
    assert!(input::context_order(
        &mut battle,
        None,
        Some(PickTarget::Unit(carrier)),
        &mut mouse
    ));
    input::commands::step(&mut battle, &mut mouse);
    advance_until(
        &mut battle.simulation,
        3000,
        "same inventory passenger reentry",
        |game| game.entities[first].contained_by() == Some(carrier),
    )
    .unwrap();
    boarded.rotate_left(1);
    assert_eq!(battle.simulation.passengers(carrier), boarded);
    battle.select_objects(&[carrier], true);
    assert_inventory(&battle, &data, &boarded);
    assert!(boarded.iter().all(|&id| {
        battle.simulation.entities[id].alive() && battle.simulation.entities[id].masked()
    }));
}
