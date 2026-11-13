//! Invisible Mob parents retain commands while their selected members are ignored by the GUI.
use super::*;
use generals_sim::skirmish::Order;

#[ignore = "requires original game installation"]
#[test]
fn installed_raptor_and_mig_dispatch_the_original_air_guard_slots() {
    use crate::{
        skirmish_verification::{
            advance_until, construct, construct_type, friendly_entity, produce_type,
        },
        strategy_verification::template,
    };
    use generals_sim::skirmish::{Faction, Kind};
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    animations.bind_logical_bones(&mut battle.simulation.catalog);
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = crate::input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    for (owner, enemy, field, aircraft, slot) in [
        (
            Faction::Usa,
            Faction::China,
            "AmericaAirfield",
            "AmericaJetRaptor",
            10,
        ),
        (
            Faction::China,
            Faction::Usa,
            "ChinaAirfield",
            "ChinaJetMIG",
            12,
        ),
    ] {
        let game = &mut battle.simulation;
        game.reset(owner, enemy).unwrap();
        let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
        construct(game, builder, Kind::PowerPlant, 0).unwrap();
        construct(game, builder, Kind::SupplyCenter, 0).unwrap();
        let field = construct_type(game, builder, template(game, field).unwrap(), 0).unwrap();
        let actor = produce_type(game, field, template(game, aircraft).unwrap(), 0).unwrap();
        advance_until(game, 3600, "paid jet in original hangar", |game| {
            game.entities[actor].aircraft_phase() == Some("idle")
        })
        .unwrap();
        battle.select_objects(&[actor], true);
        let command = selected_command(&battle, &data, slot).unwrap().action;
        assert!(matches!(command, CommandAction::GuardFlying));
        dispatch_command(&mut battle, &mut mouse, command);
        assert!(matches!(mouse.pending_command, CommandMode::GuardFlying));
        let [x, y] = battle.simulation.entities[actor].position;
        let point = [
            x + 100.0,
            y,
            battle.simulation.ground_height([x + 100.0, y]),
        ];
        crate::input::targeted_movement(&mut battle, &mut mouse, Vec3::from_array(point));
        assert!(matches!(mouse.pending_command, CommandMode::Normal));
        advance_until(
            &mut battle.simulation,
            1800,
            "air guard dispatch through jet takeoff",
            |game| matches!(game.entities[actor].ai.order, Order::Guard(_)),
        )
        .unwrap();
    }
}

#[ignore = "requires original game installation"]
#[test]
fn paid_mob_parent_and_selected_members_keep_original_commands_without_unmasking() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = crate::angry_mob_verification::prepare(&mut battle.simulation).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let mut mouse = crate::input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    let members = battle.simulation.spawned_children(setup.nexus).to_vec();
    assert_eq!(members.len(), 10);
    assert!(battle.simulation.entities[setup.nexus].masked());
    assert!(battle.simulation.entities[setup.nexus].selectable(
        &battle.simulation.catalog[battle.simulation.entities[setup.nexus].type_index]
    ));
    assert!(members.iter().all(|&id| {
        battle.simulation.catalog[battle.simulation.entities[id].type_index]
            .progression
            .ignored_in_gui
    }));
    for selection in [
        vec![setup.nexus],
        std::iter::once(setup.nexus)
            .chain(members.iter().copied())
            .collect(),
        members
            .iter()
            .copied()
            .chain(std::iter::once(setup.nexus))
            .collect(),
    ] {
        battle.select_objects(&selection, true);
        let attack_move = selected_command(&battle, &data, 11).unwrap().action;
        assert!(matches!(attack_move, CommandAction::AttackMove));
        dispatch_command(&mut battle, &mut mouse, attack_move);
        assert!(matches!(mouse.pending_command, CommandMode::AttackMove));
        let guard = selected_command(&battle, &data, 13).unwrap().action;
        assert!(matches!(guard, CommandAction::Guard));
        dispatch_command(&mut battle, &mut mouse, guard);
        assert!(matches!(mouse.pending_command, CommandMode::Guard));
        battle.simulation.attack(setup.nexus, setup.target).unwrap();
        assert!(matches!(
            battle.simulation.entities[setup.nexus].ai.order,
            Order::Attack(_)
        ));
        let stop = selected_command(&battle, &data, 14).unwrap().action;
        assert!(matches!(stop, CommandAction::Stop));
        dispatch_command(&mut battle, &mut mouse, stop);
        assert!(matches!(
            battle.simulation.entities[setup.nexus].ai.order,
            Order::Idle
        ));
        assert!(matches!(mouse.pending_command, CommandMode::Normal));
        assert!(battle.simulation.entities[setup.nexus].masked());
    }
    battle.select_objects(&[setup.palace], true);
    let research_slot = data
        .commands(&battle.simulation, setup.palace)
        .iter()
        .find(|command| {
            !command.multi_select && selected_command(&battle, &data, command.slot).is_some()
        })
        .expect("paid Palace should offer a single-selection command")
        .slot;
    battle.selected.push(members[0]);
    assert!(selected_command(&battle, &data, research_slot).is_none());
    battle.select_objects(&members, true);
    assert!(
        [11, 13, 14]
            .into_iter()
            .all(|slot| selected_command(&battle, &data, slot).is_none())
    );
    battle.select_objects(&[setup.nexus], true);
    battle.simulation.entities[setup.nexus].owner = 1;
    assert!(
        [11, 13, 14]
            .into_iter()
            .all(|slot| selected_command(&battle, &data, slot).is_none())
    );
}

#[ignore = "requires original game installation"]
#[test]
fn actual_contained_passenger_keeps_its_command_buttons_hidden() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = crate::transport_verification::prepare(&mut battle.simulation).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let data = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
    let rider = setup.riders[0];
    assert!(battle.simulation.entities[rider].contained_by().is_some());
    assert!(battle.simulation.entities[rider].masked());
    assert!(!battle.simulation.entities[rider].spawn_nexus());
    battle.select_objects(&[rider], true);
    assert!((1..=14).all(|slot| selected_command(&battle, &data, slot).is_none()));
}
