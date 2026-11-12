//! Synthetic deployment rules on the real ground/AI scheduler, without forced phases.
use super::{Phase, Skirmish, rules, turret};
use crate::skirmish::{Kind, Order};

pub(super) fn setup(armed: bool) -> (Skirmish, usize) {
    let (mut game, id) = crate::skirmish::ground_movement::tests::moving_tank();
    let index = game.entities[id].type_index;
    let position = game.entities[id].position;
    game.destroy_maintenance_object(id);
    game.process_update_destruction();
    let unit = &mut game.catalog[index];
    unit.deployment = Some(rules());
    unit.ai.acquisition = 0;
    unit.weapon.pre_attack_frames = 3;
    let mut mounted = turret::Rules::source_defaults();
    mounted.controlled_slots = 1;
    mounted.turn_rate = std::f32::consts::PI;
    mounted.idle_intervals = [10_000; 2];
    mounted.idle_angles = [0.0; 2];
    unit.turret = Some(mounted);
    for module in &mut unit.behavior_modules {
        if module.family == "AIUpdateInterface" {
            module.family = "DeployStyleAIUpdate".into();
        }
    }
    for module in &mut unit.update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = module
            && family == "AIUpdateInterface"
        {
            *family = "DeployStyleAIUpdate".into();
        }
    }
    let id = game.spawn(index, 0, position, true);
    let actor = &mut game.entities[id];
    if !armed {
        actor.weapon_set.indices = Some([None; 3]);
    }
    (game, id)
}

fn phase(game: &Skirmish, id: usize) -> &Phase {
    &game.entities[id].deployment.as_ref().unwrap().phase
}

pub(super) fn guard_deploy(game: &mut Skirmish, id: usize) {
    game.guard(id, game.entities[id].position).unwrap();
    for _ in 0..120 {
        game.step();
        if matches!(phase(game, id), Phase::Attack) {
            return;
        }
    }
    panic!("The scheduled guard did not finish deploying");
}

fn attack_setup() -> (Skirmish, usize, [f32; 3], u64) {
    let (mut game, old) = setup(true);
    let kind = game.entities[old].type_index;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    // Temporary AI_BUSY gates the primary state; turrets have their own clock.
    game.catalog[kind].turret = None;
    let id = game.spawn(kind, 0, [300.0, 150.0], true);
    let target = [350.0, 150.0, 0.0];
    game.group_force_attack_position(&[id], target).unwrap();
    game.step();
    assert!(matches!(phase(&game, id), Phase::Deploy));
    let deadline = u64::from(game.entities[id].deployment.as_ref().unwrap().deadline);
    (game, id, target, deadline)
}

#[test]
fn object_attack_deploys_from_machine_goal_before_current_victim_is_recorded() {
    let (mut game, old) = setup(true);
    let kind = game.entities[old].type_index;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    game.catalog[kind].kind_of.push("CAN_ATTACK".into());
    game.catalog[kind]
        .turret
        .as_mut()
        .unwrap()
        .initially_disabled = true;
    let id = game.spawn(kind, 0, [300.0, 150.0], true);
    let builder = game.type_for(1, Kind::Builder).unwrap();
    let target = game.spawn(builder, 1, [350.0, 150.0], true);
    game.attack(id, target).unwrap();
    assert!(game.entities[id].ai.current_victim.is_none());
    assert!(game.entities[id].ai.attack.is_some());
    game.step();
    assert!(matches!(phase(&game, id), Phase::Deploy));
    assert!(game.entities[id].ai.current_victim.is_none());
    assert_eq!(
        u64::from(game.entities[id].deployment.as_ref().unwrap().deadline),
        game.tick + 30
    );
    assert!(game.entities[id].weapon_set.weapons[0].pre_attack.is_none());
    let deadline = u64::from(game.entities[id].deployment.as_ref().unwrap().deadline);
    while game.tick < deadline {
        game.step();
        assert!(game.entities[id].ai.current_victim.is_none());
    }
    game.step();
    assert_eq!(game.entities[id].ai.current_victim, Some(target));
}

#[test]
fn scheduled_attack_retains_temporary_busy_through_the_deployed_deadline() {
    let (mut game, id, target, deadline) = attack_setup();
    let goal = game.entities[id].ai_goal_position().map(f32::to_bits);
    while game.tick < deadline {
        game.step();
        assert!(game.entities[id].weapon_set.weapons[0].pre_attack.is_none());
        assert!(!game.presentation.shots.iter().any(|shot| shot.source == id));
    }
    assert!(matches!(phase(&game, id), Phase::Attack));
    assert!(
        matches!(game.entities[id].ai.order, Order::FireAt { position, .. } if position.map(f32::to_bits) == [target[0], target[1]].map(f32::to_bits))
    );
    assert_eq!(game.entities[id].ai_goal_position().map(f32::to_bits), goal);
    game.step();
    let pre_attack = game.entities[id].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .unwrap();
    assert_eq!(pre_attack.start, deadline + 1);
    for _ in 0..3 {
        game.step();
    }
    assert!(
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == id && shot.target.is_none())
    );
}

#[test]
fn scheduled_move_waits_one_callback_after_pack_condition_finishes() {
    let (mut game, id) = setup(true);
    guard_deploy(&mut game, id);
    let destination = [450.0, 150.0];
    game.move_to(id, destination).unwrap();
    let goal = game.entities[id].ai_goal_position().map(f32::to_bits);
    for _ in 0..120 {
        game.step();
        if matches!(phase(&game, id), Phase::Undeploy) {
            break;
        }
    }
    assert!(matches!(phase(&game, id), Phase::Undeploy));
    let deadline = u64::from(game.entities[id].deployment.as_ref().unwrap().deadline);
    let position = game.entities[id].position.map(f32::to_bits);
    while game.tick < deadline {
        game.step();
        assert!(!game.entities[id].ai.motion.has_goal());
        assert_eq!(game.entities[id].position.map(f32::to_bits), position);
    }
    assert!(matches!(phase(&game, id), Phase::Move));
    assert!(game.entities[id].ai.ground_path.is_some());
    assert_eq!(game.entities[id].ai_goal_position().map(f32::to_bits), goal);
    game.step();
    assert!(game.entities[id].ai.motion.has_goal());
    assert_eq!(
        game.entities[id].ai.motion.command_frame,
        Some(deadline + 1)
    );
}

#[test]
fn new_attack_command_clears_the_previous_temporary_busy_callback() {
    let (mut game, id, target, deadline) = attack_setup();
    while game.tick + 1 < deadline {
        game.step();
    }
    game.group_force_attack_position(&[id], target).unwrap();
    game.step();
    assert_eq!(game.tick, deadline);
    assert!(matches!(phase(&game, id), Phase::Attack));
    let pre_attack = game.entities[id].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .unwrap();
    assert_eq!(pre_attack.start, deadline);
}

#[test]
fn unarmed_guard_packs_without_centering_or_disabling_its_unowned_turret() {
    let (mut game, id) = setup(false);
    assert!(!game.weapon_slot_present(id, 0));
    game.entities[id].ai.turret.angle = 1.0;
    guard_deploy(&mut game, id);
    assert_eq!(
        game.entities[id].ai.turret.angle.to_bits(),
        1.0_f32.to_bits()
    );
    assert!(game.entities[id].ai.turret.enabled);
    game.move_to(id, [450.0, 150.0]).unwrap();
    game.step();
    assert!(matches!(phase(&game, id), Phase::Undeploy));
    assert_eq!(
        game.entities[id].ai.turret.angle.to_bits(),
        1.0_f32.to_bits()
    );
    assert!(game.entities[id].ai.turret.enabled);
    assert!(game.entities[id].ai.ground_path.is_some());
}
