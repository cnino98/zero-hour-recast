use crate::skirmish::{
    Order, Skirmish,
    containment::{Payload, tests::setup},
};

fn crawler() -> (Skirmish, usize, usize) {
    crawler_with_capture(false)
}

fn crawler_with_capture(capture: bool) -> (Skirmish, usize, usize) {
    let (mut game, carrier) = setup(10.0, 0.1);
    let index = game.entities[carrier].type_index;
    game.catalog[5].name = "PayloadInfantry".into();
    if capture {
        crate::skirmish::special_ability::capture_fixture::bind(
            &mut game.catalog[5],
            crate::skirmish::special_ability::PowerType::SpecialInfantryCaptureBuilding,
            [2, 4, 2, 2],
            true,
        );
        game.catalog[5].behavior.special_abilities[0]
            .approach
            .start_range = 200.0;
    }
    let infantry = game.catalog[5].name.clone();
    let rules = game.catalog[index].contain.as_mut().unwrap();
    rules.capacity = 8;
    rules.assault_heal_ratio = Some(0.5);
    rules.weapons.fire = false;
    rules.payload = Some(Payload {
        template: infantry,
        count: 8,
    });
    game.catalog[index].weapon.damage_type = crate::skirmish::DamageType::Deploy;
    game.catalog[index].weapon.damage = 0.00001;
    game.catalog[index].weapon.range = 175.0;
    game.catalog[index].weapon.delay = 1.0;
    game.catalog[index].weapon.aim_delta = std::f32::consts::PI;
    for module in &mut game.catalog[index].behavior_modules {
        if module.family == "AIUpdateInterface" {
            module.family = "AssaultTransportAIUpdate".into();
        }
    }
    for module in &mut game.catalog[index].update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = module
            && family == "AIUpdateInterface"
        {
            *family = "AssaultTransportAIUpdate".into();
        }
    }
    let carrier = game.spawn(index, 0, [355.0, 255.0], true);
    assert!(game.entities[carrier].assault.is_some());
    let target = game.spawn(6, 1, [415.0, 255.0], true);
    game.tick = 7;
    let built = game.players[0].statistics.units_built;
    game.create_transport_payload(carrier);
    assert_eq!(game.players[0].statistics.units_built, built);
    (game, carrier, target)
}

fn board_member(game: &mut Skirmish, rider: usize, carrier: usize) {
    if !matches!(game.entities[rider].ai.order, Order::Enter(_)) {
        game.native_enter_from(rider, carrier, crate::skirmish::ai_goals::COMMAND_FROM_AI);
    }
    game.set_object_position(rider, game.object_position(carrier));
    game.process_path_service();
    game.update_enter(rider, carrier);
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
}

#[test]
fn payload_created_once_silently_with_append_order_and_no_production_score() {
    let (mut game, carrier, _) = crawler();
    let ids = game.passengers(carrier).to_vec();
    assert_eq!(ids.len(), 8);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        ids.iter()
            .all(|&id| game.entities[id].masked() && game.entities[id].ai.command.from_ai)
    );
    assert!(game.presentation.container_sounds.is_empty());
    let count = game.entities.len();
    game.create_transport_payload(carrier);
    assert_eq!(game.entities.len(), count);
    let pending = game.spawn(game.entities[carrier].type_index, 0, [455.0, 355.0], true);
    game.destroy_maintenance_object(pending);
    game.create_transport_payload(pending);
    assert_eq!(game.entities.len(), count + 1);
}

#[test]
fn deploy_shot_starts_assault_and_retains_native_zero_speed_damage_deadline() {
    let (mut game, carrier, target) = crawler();
    game.attack(carrier, target).unwrap();
    let health = game.entities[target].health;
    assert!(!game.update_assault_transport(carrier));
    for _ in 0..30 {
        assert!(game.update_attack(carrier, target));
        if game.entities[carrier]
            .assault
            .as_ref()
            .unwrap()
            .target
            .is_some()
        {
            break;
        }
        game.tick += 1;
    }
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    assert_eq!(game.projectiles.len(), 1);
    let delayed = &game.projectiles[0];
    assert!(delayed.object.is_none());
    assert_eq!(
        delayed.direct.as_ref().unwrap().deadline,
        game.update_frame().wrapping_add(0x8000_0000)
    );
    assert_eq!(
        game.entities[carrier].assault.as_ref().unwrap().target,
        Some(target)
    );
    game.update_assault_transport(carrier);
    assert!(
        game.passengers(carrier)
            .iter()
            .all(|&id| matches!(game.entities[id].ai.order, Order::ExitContainer))
    );
}

#[test]
fn wounded_return_below_threshold_and_only_full_health_exits() {
    let (mut game, carrier, target) = crawler();
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    let rider = game.passengers(carrier)[0];
    game.update_container_exit(rider);
    game.set_order(rider, Order::Idle);
    game.entities[rider].health = game.entities[rider].max_health * 0.5;
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
    game.entities[rider].health -= 0.01;
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::Enter(id) if id == carrier));
    board_member(&mut game, rider, carrier);
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    game.entities[rider].health = game.entities[rider].max_health;
    game.update_assault_transport(carrier);
    assert!(matches!(
        game.entities[rider].ai.order,
        Order::ExitContainer
    ));
}

#[test]
fn new_boarder_waits_for_new_attack_and_explicit_member_command_detaches() {
    let (mut game, carrier, target) = crawler();
    game.update_assault_transport(carrier);
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    let rider = game.passengers(carrier)[0];
    game.update_container_exit(rider);
    game.set_order(rider, Order::Idle);
    game.move_to(rider, [455.0, 155.0]).unwrap();
    game.update_assault_transport(carrier);
    assert!(
        !game.entities[carrier]
            .assault
            .as_ref()
            .unwrap()
            .members
            .iter()
            .any(|member| member.id == rider)
    );
    assert!(!game.entities[rider].ai.command.allowed_to_chase);
    board_member(&mut game, rider, carrier);
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    assert!(matches!(
        game.entities[rider].ai.order,
        Order::ExitContainer
    ));
}

#[test]
fn stop_recalls_exposed_members_and_death_hands_attack_to_survivors() {
    let (mut game, carrier, target) = crawler();
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    let rider = game.passengers(carrier)[0];
    game.update_container_exit(rider);
    game.set_order(rider, Order::Idle);
    game.stop(carrier).unwrap();
    assert!(matches!(game.entities[rider].ai.order, Order::Enter(id) if id == carrier));
    board_member(&mut game, rider, carrier);
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    // The prior exit reserved the door for ExitDelay frames. Run this callback
    // at its real availability deadline before testing an exposed survivor.
    game.tick = game.tick.max(game.entities[carrier].containment.exit_ready);
    game.update_container_exit(rider);
    assert!(game.entities[rider].contained_by().is_none());
    game.kill_object(carrier, crate::skirmish::DeathType::Normal);
    assert_eq!(
        game.update_assault_module(carrier),
        i32::try_from(crate::skirmish::module_updates::FOREVER).unwrap()
    );
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
    assert!(!game.entities[rider].ai.command.allowed_to_chase);
    assert!(!game.entities[rider].ai.command.from_ai);
}

#[test]
fn active_angle_goal_is_moving_until_idle_under_native_predicate() {
    let (mut game, carrier, target) = crawler();
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    let rider = game.passengers(carrier)[0];
    game.update_container_exit(rider);
    game.set_order(rider, Order::FollowPath);
    game.entities[rider].ai.motion.request_angle(1.0);
    assert!(!game.entities[rider].ai.motion.is_moving());
    assert!(game.entities[rider].ai.motion.has_goal());
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::FollowPath));
    game.entities[rider].ai.order = Order::Idle;
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
}

#[test]
fn attack_move_retains_original_command_source_across_its_inner_attack() {
    let (mut game, carrier, target) = crawler();
    let rider = game.passengers(carrier)[0];
    game.exit_container(rider).unwrap();
    game.update_container_exit(rider);
    let goal = [455.0, 155.0];
    game.attack_move(rider, goal).unwrap();
    assert!(!game.entities[rider].ai.command.from_ai);
    game.entities[rider].ai.mood.next_check = 0;
    game.update_visibility();
    game.update_attack_move(rider, goal);
    assert_eq!(game.entities[rider].ai.ai_target, Some(target));
    assert!(game.entities[rider].ai.command.from_ai);
    game.kill_object(target, crate::skirmish::DeathType::Normal);
    game.update_attack_move(rider, goal);
    assert!(game.entities[rider].ai.ai_target.is_none());
    assert!(!game.entities[rider].ai.command.from_ai);
}

#[test]
fn capture_facing_preserves_goal_but_unpack_busy_allows_native_assault_retarget() {
    let (mut game, carrier, _) = crawler_with_capture(true);
    let target = game.spawn(0, 1, [415.0, 255.0], true);
    game.players[0].upgrades.push("Capture".into());
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    let rider = game.passengers(carrier)[0];
    game.update_container_exit(rider);
    game.activate_upgrade_modules(rider);
    game.update_visibility();
    // Finish the prior frame's real partition publication before the ability's
    // LOS range query; newly registered objects are not yet in cell lists.
    game.update_ground_contacts();
    game.tick += 2;
    game.capture(rider, target).unwrap();
    assert_eq!(game.entities[rider].ai.command.goal_object, None);
    game.tick += 1;
    // Isolate the callback contract: establish Face through the actual ability
    // callback, then test a subsequent carrier callback. This is not a claim
    // about same-frame scheduler order (carrier INITIAL precedes ability NORMAL).
    game.update_special_ability(rider, 0);
    assert!(
        matches!(
            game.entities[rider].ai.command.main_state,
            crate::skirmish::ai_goals::MainState::FaceObject { .. }
        ),
        "contained={:?} capture={:?} goal={:?}; {}",
        game.entities[rider].contained_by(),
        game.entities[rider].capture_condition(),
        game.entities[rider].ai.command.goal_object,
        crate::skirmish::diagnostics::movement_details(&game, rider)
    );
    assert_eq!(game.entities[rider].ai.command.goal_object, Some(target));
    game.update_assault_transport(carrier);
    assert!(matches!(
        game.entities[rider].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::FaceObject { .. }
    ));
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[rider].capture_condition(), Some("UNPACKING"));
    // SpecialAbilityUpdate::startUnpacking issues aiBusy, whose main machine
    // has no goal. AssaultTransportAIUpdate therefore orders this stationary
    // member back to its designated target on its next update.
    assert_eq!(game.entities[rider].ai.command.goal_object, None);
    assert_eq!(
        game.entities[rider].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    game.update_assault_transport(carrier);
    assert!(matches!(game.entities[rider].ai.order, Order::Attack(id) if id == target));
}

#[test]
fn evacuation_resets_finished_assault_and_does_not_recall_exiting_members() {
    let (mut game, carrier, target) = crawler();
    game.attack(carrier, target).unwrap();
    game.begin_assault_target(carrier, Some(target));
    game.update_assault_transport(carrier);
    game.kill_object(target, crate::skirmish::DeathType::Normal);
    game.update_assault_transport(carrier);
    let riders = game.passengers(carrier).to_vec();
    game.evacuate(carrier).unwrap();
    let state = game.entities[carrier].assault.as_ref().unwrap();
    assert!(state.members.is_empty());
    assert!(state.target.is_none());
    assert!(!state.attack_object);
    for _ in 0..500 {
        game.step();
    }
    assert!(game.passengers(carrier).is_empty());
    assert!(riders.iter().all(|&id| game.entities[id].alive()));
    assert!(
        riders
            .iter()
            .all(|&id| !matches!(game.entities[id].ai.order, Order::Enter(_)))
    );
}

#[test]
fn override_resets_before_rejected_base_commands_and_ai_retains_full_move_goal() {
    use crate::skirmish::{
        ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_SCRIPT},
        combat::attack::Goal,
    };
    let (mut game, carrier, target) = crawler();
    let point = [500.0, 300.0, 49.0];
    game.native_attack_move_from(carrier, point, 9, COMMAND_FROM_SCRIPT);
    assert_eq!(
        game.entities[carrier]
            .assault
            .as_ref()
            .unwrap()
            .attack_move
            .unwrap()
            .map(f32::to_bits),
        point.map(f32::to_bits)
    );
    game.ai_idle_from_ai(carrier);
    assert_eq!(
        game.entities[carrier]
            .assault
            .as_ref()
            .unwrap()
            .attack_move
            .unwrap()
            .map(f32::to_bits),
        point.map(f32::to_bits)
    );
    game.entities[carrier].set_effectively_dead(true);
    game.native_attack_goal_from(carrier, Goal::Object(target), 5, COMMAND_FROM_SCRIPT, true);
    let state = game.entities[carrier].assault.as_ref().unwrap();
    assert!(state.attack_move.is_none());
    assert!(!state.attack_object); // FORCE_ATTACK_OBJECT takes the reset default.
    assert!(game.entities[carrier].ai_idle());
    game.entities[carrier].set_effectively_dead(false);
    game.native_attack_move_from(carrier, point, 9, COMMAND_FROM_AI);
    assert!(
        game.entities[carrier]
            .assault
            .as_ref()
            .unwrap()
            .attack_move
            .is_none()
    );
}
