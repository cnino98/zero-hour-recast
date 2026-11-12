use super::*;
use crate::skirmish::{Kind, ground_movement, startup, tests};

#[test]
fn hunt_initialization_draws_scan_then_nested_idle_and_releases_on_real_stop() {
    let (mut game, actor) = ground_movement::tests::moving_tank();
    let mut expected = game.random;
    let scan = startup::random_between(&mut expected, 30);
    let idle = startup::random_between(&mut expected, 60);
    game.ai_hunt_from_ai(actor);
    assert_eq!(game.random, expected);
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert!(!game.entities[actor].ai_idle());
    assert!(game.entities[actor].last_command_from_ai());
    assert_eq!(
        game.entities[actor].ai.hunt.as_ref().unwrap().next_scan,
        game.update_frame().wrapping_add(scan)
    );
    assert_eq!(game.entities[actor].ai.idle.sleep_offset, idle);
    assert_eq!(game.entities[actor].ai.command.goal_object, None);
    game.stop(actor).unwrap();
    assert!(game.entities[actor].ai.hunt.is_none());
    assert!(game.entities[actor].ai_idle());
}

#[test]
fn primary_hunt_finds_an_unseen_enemy_and_runs_real_attack_without_changing_source() {
    let (mut game, actor, target) = computer_hunter();
    assert!(!game.visible_to(1, target));
    game.ai_hunt_from_ai(actor);
    game.tick = 31;
    game.update_ai_module(actor);
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert_eq!(game.entities[actor].ai.current_victim, Some(target));
    assert!(game.entities[actor].ai.attack.is_some());
    assert!(game.entities[actor].last_command_from_ai());
}

#[test]
fn no_victim_returns_to_primary_idle_unless_player_all_hunt_is_enabled() {
    let (mut game, actor) = ground_movement::tests::moving_tank();
    for id in 0..game.entities.len() {
        if game.entities[id].owner != 0 {
            game.destroy_maintenance_object(id);
        }
    }
    game.process_update_destruction();
    game.update_ground_contacts();
    game.ai_hunt_from_ai(actor);
    game.tick = 31;
    game.update_ai_module(actor);
    assert!(game.entities[actor].ai_idle());
    assert!(game.entities[actor].ai.hunt.is_none());
    game.set_player_units_should_hunt(0, true, 1);
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert_eq!(game.entities[actor].ai.command.last_command_source, 1);
    game.tick = 62;
    game.update_ai_module(actor);
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
}

#[test]
fn native_player_hunt_excludes_real_dozer_and_keeps_team_order_rng() {
    let mut game = tests::game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let first = game.spawn(kind, 0, [300.0, 150.0], true);
    let second = game.spawn(kind, 0, [350.0, 150.0], true);
    let mut expected = game.random;
    let mut scans = Vec::new();
    for id in game.team_membership.player_objects(0) {
        if ["DOZER", "HARVESTER", "IGNORES_SELECT_ALL"]
            .iter()
            .any(|kind| game.attack_kind(id, kind))
            || game.entities[id].ai.registration.is_none()
            || game.attack_kind(id, "IMMOBILE")
            || game.object_disabled_mask(id) != 0
            || game.attack_kind(id, "PROJECTILE")
        {
            continue;
        }
        scans.push((id, startup::random_between(&mut expected, 30)));
        startup::random_between(&mut expected, 60);
    }
    game.set_player_units_should_hunt(0, true, 1);
    assert_eq!(game.random, expected);
    assert!(scans.iter().any(|&(id, _)| id == first));
    assert!(scans.iter().any(|&(id, _)| id == second));
    for (id, scan) in scans {
        assert_eq!(
            game.entities[id].ai.hunt.as_ref().unwrap().next_scan,
            game.update_frame().wrapping_add(scan)
        );
    }
    assert!(game.entities.iter().filter(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder).all(|actor| actor.ai.hunt.is_none()));
}

#[test]
fn actual_ai_move_suspends_hunt_scan_and_preserves_its_parent_machine() {
    let (mut game, actor) = ground_movement::tests::moving_tank();
    game.ai_hunt_from_ai(actor);
    let scan = game.entities[actor].ai.hunt.as_ref().unwrap().next_scan;
    let source = game.entities[actor].ai.command.last_command_source;
    game.construction_move(actor, [650.0, 150.0, 0.0]);
    assert!(game.entities[actor].ai.blocking.temporary_move_to());
    game.tick = 31;
    game.update_ai_module(actor);
    assert!(game.entities[actor].ai.blocking.temporary_move_to());
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert_eq!(
        game.entities[actor].ai.hunt.as_ref().unwrap().next_scan,
        scan
    );
    assert_eq!(game.entities[actor].ai.command.last_command_source, source);
}

fn computer_hunter() -> (Skirmish, usize, usize) {
    computer_hunter_with_aircraft(false)
}

fn computer_hunter_with_aircraft(aircraft: bool) -> (Skirmish, usize, usize) {
    let (mut game, template_actor) = ground_movement::tests::moving_tank();
    let kind = game.entities[template_actor].type_index;
    for id in 0..game.entities.len() {
        if game.entities[id].owner == 0 {
            game.destroy_maintenance_object(id);
        }
    }
    game.process_update_destruction();
    game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Easy);
    game.catalog[kind].weapon.anti_mask |= 0x04;
    if aircraft {
        // Native WeaponSet classifies airborne VEHICLE as anti-mask 0x01.
        game.catalog[kind].weapon.anti_mask |= 0x01;
        let target = &mut game.catalog[5];
        target.kind_of.retain(|kind| kind != "INFANTRY");
        target.kind_of.extend(["VEHICLE".into(), "AIRCRAFT".into()]);
        target.locomotor.ground = None;
        target.locomotor.flight = Some(crate::skirmish::flight::tests::rules(
            crate::skirmish::flight::Appearance::Wings,
        ));
        target.locomotor.legal_surfaces = 8;
        target.locomotor.airborne_targeting_height = 10.0;
        target.movement_surface = crate::skirmish::MovementSurface::Air;
    }
    let actor = game.spawn(kind, 1, [300.0, 150.0], true);
    let target = game.spawn(5, 0, [600.0, 100.0], true);
    game.update_ground_contacts();
    (game, actor, target)
}

#[test]
fn human_ai_hunt_keeps_native_approach_chase_rejection() {
    let (mut game, actor) = ground_movement::tests::moving_tank();
    let target = game.spawn(5, 1, [600.0, 150.0], true);
    game.update_ground_contacts();
    assert!(!game.visible_to(0, target));
    game.ai_hunt_from_ai(actor);
    game.tick = 31;
    game.update_ai_module(actor);
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert_eq!(
        game.entities[actor].ai.hunt.as_ref().unwrap().goal,
        Some(target)
    );
    assert!(game.entities[actor].ai.attack.is_none());
    assert_eq!(game.entities[actor].ai.current_victim, None);
    assert!(game.entities[actor].last_command_from_ai());
}

#[test]
fn computer_hunt_approaches_aircraft_that_ordinary_attack_will_not_chase() {
    let (mut game, actor, target) = computer_hunter_with_aircraft(true);
    // AIRBORNE_TARGET is produced by native doLocomotor, not setPosition.
    let position = game.object_position(target);
    game.set_object_position(target, [position[0], position[1], 100.0]);
    game.update_ground_locomotor(target);
    assert!(game.entities[target].is_airborne_target());
    assert!(game.weapon_can_target(actor, target));
    game.update_ground_contacts();
    game.ai_attack_goal(actor, Goal::Object(target), i32::MAX.unsigned_abs());
    assert!(game.entities[actor].ai.attack.is_none());
    game.ai_hunt_from_ai(actor);
    game.tick = 31;
    game.update_ai_module(actor);
    assert!(game.entities[actor].ai.attack.is_some());
    assert_eq!(game.entities[actor].ai.current_victim, Some(target));
}

#[test]
fn locked_primary_refuses_real_idle_and_goal_changes_but_child_goal_can_retarget() {
    let (mut game, actor, target) = computer_hunter();
    game.ai_hunt_from_ai(actor);
    game.tick = 31;
    game.update_ai_module(actor);
    assert!(!game.entities[actor].ai.command.locked);
    assert!(game.entities[actor].ai.attack.is_some());
    let primary_goal = game.entities[actor].ai_goal_position().map(f32::to_bits);
    let revision = game.entities[actor].ai.command_revision;
    let expected_rng = game.random;
    // The native child update runs between outer-machine lock/unlock. Exercise
    // the same synchronous AI callback against its actual shared mutation seam.
    game.entities[actor].ai.command.locked = true;
    game.ai_idle_from_ai(actor);
    game.set_ai_goal_object(actor, target);
    game.set_ai_goal_position(actor, [100.0, 100.0, 50.0]);
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert_eq!(game.entities[actor].ai.command_revision, revision);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        primary_goal
    );
    assert_eq!(game.random, expected_rng);
    assert!(game.retarget_hunt_goal(actor, None));
    assert_eq!(game.entities[actor].ai.hunt.as_ref().unwrap().goal, None);
    game.entities[actor].ai.command.locked = false;
    game.ai_idle_from_ai(actor);
    assert!(game.entities[actor].ai_idle());
    assert!(game.entities[actor].ai.hunt.is_none());
    assert!(!game.retarget_hunt_goal(actor, Some(target)));
}

#[test]
fn nested_disarm_callback_retargets_child_without_replacing_locked_hunt_primary() {
    use crate::skirmish::DamageType;
    let (mut game, template_actor) = ground_movement::tests::moving_tank();
    let kind = game.entities[template_actor].type_index;
    for id in 0..game.entities.len() {
        if game.entities[id].owner == 0 {
            game.destroy_maintenance_object(id);
        }
    }
    game.process_update_destruction();
    let weapon = &mut game.catalog[kind].weapon;
    weapon.damage_type = DamageType::Disarm;
    weapon.damage = 1.0;
    weapon.range = 200.0;
    weapon.anti_mask = 0x10;
    weapon.continue_attack_range = 100.0;
    weapon.pre_attack_frames = 0;
    game.catalog[5].kind_of = vec!["DEMOTRAP".into()];
    game.catalog[5].weapon = crate::skirmish::WeaponRules::unarmed();
    game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Easy);
    let actor = game.spawn(kind, 1, [300.0, 150.0], true);
    let first = game.spawn(5, 0, [330.0, 150.0], true);
    let next = game.spawn(5, 0, [350.0, 150.0], true);
    game.update_ground_contacts();
    game.ai_hunt_from_ai(actor);
    let primary_goal = game.entities[actor].ai_goal_position().map(f32::to_bits);
    let revision = game.entities[actor].ai.command_revision;
    for frame in 31..80 {
        game.tick = frame;
        game.update_ai_module(actor);
        assert!(!game.entities[actor].ai.command.locked);
        if game.update_objects[game.entities[first].update_owner].destroy_requested {
            break;
        }
    }
    assert!(game.update_objects[game.entities[first].update_owner].destroy_requested);
    assert!(!game.update_objects[game.entities[next].update_owner].destroy_requested);
    assert!(
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == actor && shot.target == Some(first))
    );
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        MainState::Hunt
    ));
    assert!(matches!(game.entities[actor].ai.order, Order::Idle));
    assert_eq!(game.entities[actor].ai.command_revision, revision);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        primary_goal
    );
    assert_eq!(
        game.entities[actor].ai.hunt.as_ref().unwrap().goal,
        Some(next)
    );
    assert!(
        matches!(game.entities[actor].ai.attack.as_ref().unwrap().goal, Goal::Object(id) if id == next)
    );
    assert!(game.entities[actor].last_command_from_ai());
}
