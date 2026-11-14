use super::aircraft;
use crate::skirmish::{Faction, Kind, Order, Skirmish, tests::fixture_type};

fn armed_helix() -> (Skirmish, usize, usize, usize, usize) {
    let (mut game, host) = aircraft();
    let index = game.entities[host].type_index;
    let unit = &mut game.catalog[index];
    unit.kind_of.push("CAN_ATTACK".into());
    unit.weapon.damage = 1.0;
    unit.contain.as_mut().unwrap().payload_templates = vec!["PortableCannon".into()];
    // The authored HelixContain family selects the separate portable path.
    let mut portable = fixture_type(Faction::China, Kind::Vehicle);
    portable.name = "PortableCannon".into();
    // The original portable cannon has a damage weapon. A Vehicle fixture
    // defaults to unarmed, which correctly exits attack on its first update.
    portable.weapon = fixture_type(Faction::China, Kind::Tank).weapon;
    portable.kind_of = vec!["PORTABLE_STRUCTURE".into(), "CAN_ATTACK".into()];
    portable.attachment_bone = Some("CHASSISBONE".into());
    game.catalog.push(portable);
    let position = game.object_position(host);
    let host = crate::skirmish::containment::tests::respawn_contain_family(
        &mut game,
        host,
        "HelixContain",
    );
    game.set_object_position(host, position);
    let child = game.entities[host].attachment_child().unwrap();
    let rider = game.spawn(5, 0, game.entities[host].position, true);
    game.board(rider, host);
    let target = game.spawn(0, 1, [1200.0, 1000.0], true);
    (game, host, rider, child, target)
}

#[test]
fn direct_attack_dispatches_out_of_range_infantry_and_separate_portable() {
    let (mut game, host, rider, child, target) = armed_helix();
    assert_eq!(game.passengers(host), &[rider]);
    assert!(!game.weapon_target_reachable(rider, target));
    assert!(game.weapon_can_target(child, target));
    assert!(!game.weapon_target_reachable(child, target));
    // Passive host acquisition must not manufacture a player command for riders.
    game.set_order(host, Order::Attack(target));
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(matches!(game.entities[child].ai.order, Order::Idle));
    game.entities[rider].ai.command.from_ai = true;
    game.entities[child].ai.command.from_ai = true;
    game.attack(host, target).unwrap();
    for id in [host, child] {
        assert!(matches!(game.entities[id].ai.order, Order::Attack(id) if id == target));
        assert!(!game.entities[id].ai.command.from_ai);
    }
    // The player command is forwarded, then Approach fails Object::isMobile
    // because TransportContain holds the out-of-range infantry.
    assert!(!game.entities[rider].ai.command.from_ai);
    assert_eq!(
        game.entities[rider].ai.command.last_command_source,
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER
    );
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(game.entities[rider].ai.attack.is_none());
    assert!(!game.entities[rider].attacking());
    assert_eq!(game.passengers(host), &[rider]);
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn direct_attack_skips_disabled_portable_without_suppressing_infantry() {
    for disabled in [1, 2, 4, 10] {
        let (mut game, host, rider, child, target) = armed_helix();
        // This case isolates portable disable dispatch; the held infantry must
        // already be in weapon range so its real Attack state can remain active.
        let rider_type = game.entities[rider].type_index;
        game.catalog[rider_type].weapon.range = game.range_distance(rider, target) + 10.0;
        match disabled {
            1 => game.set_hacked_until(
                child,
                crate::skirmish::ai_dispatch::native_frame(game.tick + 30),
            ),
            2 => game.entities[child].disabled_status |= 1 << 2,
            4 => game.entities[child].battle_bonuses.paralyzed_until = game.tick + 30,
            10 => {
                let health = game.entities[child].max_health;
                let child_type = game.entities[child].type_index;
                game.catalog[child_type].maintenance.subdual =
                    Some(crate::skirmish::microwave::SubdualRules {
                        cap: health,
                        heal_rate: 30,
                        heal_amount: health,
                    });
                game.apply_subdual_damage(child, health);
                assert!(game.entities[child].subdued());
            }
            _ => unreachable!(),
        }
        game.attack(host, target).unwrap();
        assert!(matches!(game.entities[child].ai.order, Order::Idle));
        assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
        game.entities[child].disabled_status &= !(1 << 2);
        game.entities[child].battle_bonuses.paralyzed_until = 0;
        game.clear_hacked_disable(child);
        game.apply_subdual_damage(child, -game.entities[child].max_health);
        game.attack(host, target).unwrap();
        assert!(matches!(game.entities[child].ai.order, Order::Attack(id) if id == target));
    }
}

#[test]
fn direct_attack_forwards_to_an_unmanned_portable() {
    let (mut game, host, _, child, target) = armed_helix();
    game.entities[child].crew.unmanned = true;
    game.attack(host, target).unwrap();
    assert_eq!(game.entities[child].ai.command.goal_object, Some(target));
}

#[test]
fn direct_attack_preserves_closed_bunker_and_noncombatant_host_guards() {
    let (mut game, host, rider, child, target) = armed_helix();
    let index = game.entities[host].type_index;
    game.catalog[index].contain.as_mut().unwrap().weapons.fire = false;
    game.attack(host, target).unwrap();
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(matches!(game.entities[child].ai.order, Order::Attack(id) if id == target));
    game.set_order(child, Order::Idle);
    game.catalog[index]
        .kind_of
        .retain(|kind| kind != "CAN_ATTACK");
    game.native_attack_goal_from(
        host,
        crate::skirmish::combat::attack::Goal::Object(target),
        9999,
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
        false,
    );
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(matches!(game.entities[child].ai.order, Order::Idle));
}

#[test]
fn direct_attack_waits_for_landing_and_takeoff_before_forwarding() {
    for phase in 0..3 {
        let (mut game, host, rider, child, target) = armed_helix();
        if phase == 1 {
            game.set_chinook_state(
                host,
                super::super::Phase::Land,
                None,
                None,
                crate::skirmish::ai_goals::COMMAND_FROM_AI,
            );
        } else {
            // Begin at the source ground contact boundary. Existing Chinook
            // transition tests exercise reaching this boundary by flight.
            game.set_object_position(host, [350.0, 335.0, 0.0]);
            let state = game.entities[host].chinook.as_mut().unwrap();
            state.flight = super::super::Flight::Landed;
            game.entities[host].flight = Some(Box::new(crate::skirmish::flight::Motion::new(0.0)));
            if phase == 2 {
                game.set_chinook_state(
                    host,
                    super::super::Phase::Takeoff,
                    None,
                    None,
                    crate::skirmish::ai_goals::COMMAND_FROM_AI,
                );
            }
        }
        game.entities[rider].ai.command.from_ai = true;
        game.entities[child].ai.command.from_ai = true;
        game.attack(host, target).unwrap();
        assert!(matches!(game.entities[host].ai.order, Order::Idle));
        for _ in 0..1800 {
            assert!(matches!(game.entities[rider].ai.order, Order::Idle));
            assert!(matches!(game.entities[child].ai.order, Order::Idle));
            assert!(game.entities[rider].ai.command.from_ai);
            game.step();
            if matches!(
                game.entities[host].chinook.as_ref().unwrap().flight,
                super::super::Flight::Flying
            ) && game.entities[host]
                .chinook
                .as_ref()
                .unwrap()
                .pending
                .is_none()
            {
                break;
            }
        }
        assert!(matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            super::super::Flight::Flying
        ));
        assert!(matches!(game.entities[child].ai.order, Order::Attack(id) if id == target));
        assert!(!game.entities[rider].ai.command.from_ai);
        assert!(!game.entities[child].ai.command.from_ai);
        assert!(
            game.entities[host]
                .chinook
                .as_ref()
                .unwrap()
                .pending
                .as_ref()
                .is_none_or(|command| !matches!(
                    command.kind,
                    crate::skirmish::ai_dispatch::stored_command::Kind::Attack { .. }
                ))
        );
    }
}

#[test]
fn player_stop_latches_supply_busy_only_when_deferred_idle_is_replayed() {
    use crate::skirmish::ai_goals::COMMAND_FROM_AI;

    let (mut game, host) = aircraft();
    game.set_chinook_state(host, super::super::Phase::Land, None, None, COMMAND_FROM_AI);
    game.stop(host).unwrap();
    assert!(!game.entities[host].ai.supply.force_busy);

    // Complete the flight boundary before replaying the actual retained Stop.
    game.set_ai_order(host, Order::Idle);
    assert!(game.entities[host].landed());
    let stop = game.entities[host]
        .chinook
        .as_mut()
        .unwrap()
        .pending
        .take()
        .expect("landing retained player Stop");
    game.replay_stored_command(host, stop);

    assert!(game.entities[host].ai.supply.force_busy);
    assert!(game.entities[host].ai_idle());
    assert!(!game.entities[host].last_command_from_ai());
}

#[test]
fn stop_or_move_supersedes_deferred_attack_without_forwarding() {
    for stop in [true, false] {
        let (mut game, host, rider, child, target) = armed_helix();
        game.set_chinook_state(
            host,
            super::super::Phase::Land,
            None,
            None,
            crate::skirmish::ai_goals::COMMAND_FROM_AI,
        );
        game.attack(host, target).unwrap();
        assert!(
            matches!(game.entities[host].chinook.as_ref().unwrap().pending.as_ref().map(|command| &command.kind), Some(crate::skirmish::ai_dispatch::stored_command::Kind::Attack { goal: crate::skirmish::combat::attack::Goal::Object(victim), .. }) if *victim == target)
        );
        if stop {
            game.stop(host).unwrap();
        } else {
            game.move_to(host, [550.0, 400.0]).unwrap();
        }
        assert!(
            game.entities[host]
                .chinook
                .as_ref()
                .unwrap()
                .pending
                .as_ref()
                .is_none_or(|command| !matches!(
                    command.kind,
                    crate::skirmish::ai_dispatch::stored_command::Kind::Attack { .. }
                ))
        );
        super::run_until(&mut game, |game| {
            matches!(
                game.entities[host].chinook.as_ref().unwrap().flight,
                super::super::Flight::Flying
            )
        });
        assert!(matches!(game.entities[rider].ai.order, Order::Idle));
        assert!(matches!(game.entities[child].ai.order, Order::Idle));
        assert!(!matches!(game.entities[host].ai.order, Order::Attack(_)));
    }
}

#[test]
fn attack_waits_for_active_evacuation_chain_before_forwarding_to_portable() {
    let (mut game, host, rider, child, target) = armed_helix();
    game.group_evacuate(host).unwrap();
    super::run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            super::super::Flight::Landing(_)
        )
    });
    assert!(matches!(
        game.entities[host].chinook.as_ref().unwrap().flight,
        super::super::Flight::Landing(_)
    ));
    game.attack(host, target).unwrap();
    assert!(matches!(game.entities[child].ai.order, Order::Idle));
    super::run_until(&mut game, |game| {
        matches!(
            game.entities[host].chinook.as_ref().unwrap().flight,
            super::super::Flight::Flying
        ) && game.entities[host]
            .chinook
            .as_ref()
            .unwrap()
            .pending
            .is_none()
    });
    // LAND_AND_EVAC succeeds into EVAC_AND_TAKEOFF before the pending
    // Attack is replayed. Only the portable remains attached for forwarding.
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.passengers(host).is_empty());
    assert!(matches!(game.entities[child].ai.order, Order::Attack(id) if id == target));
    let state = game.entities[host].chinook.as_ref().unwrap();
    assert!(state.pending.is_none() && state.phase.is_none());
}
