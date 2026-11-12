use crate::skirmish::{DeathType, Skirmish, maintenance::Research, tests};

fn research_game(player: bool) -> (Skirmish, usize) {
    let mut game = tests::game();
    game.catalog[0].energy = 1;
    game.catalog[0].maintenance.research.push(Research {
        name: "Upgrade_ProductionStatus".into(),
        cost: 100,
        frames: 2,
        player,
    });
    let second = game.spawn(0, 0, [450.0, 350.0], true);
    game.update_power();
    (game, second)
}

#[test]
fn player_cancel_and_completion_clear_status_after_queue_and_before_upgrade_callbacks() {
    let (mut game, second) = research_game(true);
    let cash = game.players[0].cash;
    game.queue_upgrade(0, 0).unwrap();
    assert!(!game.can_research(second, 0));
    game.cancel_upgrade(0, 0, 0).unwrap();
    assert_eq!(game.players[0].cash, cash);
    assert!(game.players[0].upgrades_in_progress.is_empty());
    assert!(game.can_research(second, 0));
    game.queue_upgrade(second, 0).unwrap();
    game.update_production(second);
    assert!(!game.players[0].upgrades_in_progress.is_empty());
    game.update_production(second);
    assert!(game.entities[second].queue.is_empty());
    assert!(game.players[0].upgrades_in_progress.is_empty());
    assert!(game.has_upgrade(0, "Upgrade_ProductionStatus"));
    assert!(!game.can_research(0, 0));
}

#[test]
fn peaceful_producer_destruction_retains_player_status_and_prevents_requeue() {
    let (mut game, second) = research_game(true);
    game.queue_upgrade(0, 0).unwrap();
    let cash = game.players[0].cash;
    game.destroy_maintenance_object(0);
    game.process_update_destruction();
    assert!(game.entities[0].queue.is_empty());
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(
        game.players[0].upgrades_in_progress,
        ["Upgrade_ProductionStatus"]
    );
    assert!(!game.can_research(second, 0));
    assert!(game.queue_upgrade(second, 0).is_err());
}

#[test]
fn team_transfer_without_defection_keeps_old_player_status_and_new_owner_cannot_refund() {
    let (mut game, second) = research_game(true);
    game.queue_upgrade(0, 0).unwrap();
    let original_cash = game.players[0].cash;
    let new_cash = game.players[1].cash;
    // Object::setTeam does not perform Object::defect's explicit cancellation.
    game.transfer_maintenance_owner(0, 1);
    assert_eq!(game.entities[0].queue.len(), 1);
    assert!(game.cancel_upgrade(1, 0, 0).is_err());
    assert_eq!(game.entities[0].queue.len(), 1);
    assert!(!game.can_research(second, 0));
    game.kill_object(0, DeathType::Normal);
    assert!(game.entities[0].queue.is_empty());
    assert_eq!(game.players[0].cash, original_cash);
    assert_eq!(game.players[1].cash, new_cash);
    assert_eq!(
        game.players[0].upgrades_in_progress,
        ["Upgrade_ProductionStatus"]
    );
    assert!(game.players[1].upgrades_in_progress.is_empty());
}

#[test]
fn transferred_research_completes_for_new_owner_without_clearing_old_owner_status() {
    let (mut game, second) = research_game(true);
    game.queue_upgrade(0, 0).unwrap();
    game.transfer_maintenance_owner(0, 1);
    game.update_production(0);
    game.update_production(0);
    assert!(game.has_upgrade(1, "Upgrade_ProductionStatus"));
    assert!(!game.has_upgrade(0, "Upgrade_ProductionStatus"));
    assert!(game.players[1].upgrades_in_progress.is_empty());
    assert_eq!(
        game.players[0].upgrades_in_progress,
        ["Upgrade_ProductionStatus"]
    );
    assert!(!game.can_research(second, 0));
}

#[test]
fn object_upgrade_queue_records_native_player_mask_but_cancel_does_not_remove_it() {
    let (mut game, second) = research_game(false);
    game.queue_upgrade(0, 0).unwrap();
    assert!(game.can_research(second, 0));
    game.cancel_upgrade(0, 0, 0).unwrap();
    assert_eq!(
        game.players[0].upgrades_in_progress,
        ["Upgrade_ProductionStatus"]
    );
    game.queue_upgrade(second, 0).unwrap();
    game.update_production(second);
    game.update_production(second);
    assert!(game.object_has_upgrade(second, "Upgrade_ProductionStatus"));
    assert!(!game.has_upgrade(0, "Upgrade_ProductionStatus"));
    assert_eq!(
        game.players[0].upgrades_in_progress,
        ["Upgrade_ProductionStatus"]
    );
}

#[test]
fn actual_player_create_grant_completes_pending_status_and_repeated_grants_do_not_duplicate() {
    let (mut game, _) = research_game(true);
    game.queue_upgrade(0, 0).unwrap();
    let cash = game.players[0].cash;
    let kind = game.type_for(0, crate::skirmish::Kind::Tank).unwrap();
    tests::register_behavior(&mut game.catalog[kind], "GrantUpgradeCreate", 8);
    game.catalog[kind]
        .behavior
        .radar_grants
        .push(crate::skirmish::radar_update::Grant {
            name: Some("Upgrade_ProductionStatus".into()),
            academy_radar: false,
            player: true,
            on_created: true,
        });
    game.spawn(kind, 0, [450.0, 450.0], true);
    game.spawn(kind, 0, [500.0, 450.0], true);
    assert!(game.players[0].upgrades_in_progress.is_empty());
    assert_eq!(
        game.players[0]
            .upgrades
            .iter()
            .filter(|name| name.as_str() == "Upgrade_ProductionStatus")
            .count(),
        1
    );
    assert!(game.cancel_upgrade(0, 0, 0).is_err());
    game.kill_object(0, DeathType::Normal);
    assert!(game.entities[0].queue.is_empty());
    assert_eq!(game.players[0].cash, cash);
    assert!(game.has_upgrade(0, "Upgrade_ProductionStatus"));
}
