use super::{Goal, Phase};
use crate::skirmish::{Kind, Order, Skirmish, tests::game, turret::Rules, weapons::Lock};

fn shooter() -> (Skirmish, usize, usize) {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].weapon.pre_attack_frames = 3;
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    let target = game.spawn(kind, 1, [300.0, 250.0], true);
    game.entities[actor].facing = 0.0;
    (game, actor, target)
}

#[test]
fn position_attack_enters_fire_then_keeps_pre_attack_until_its_native_deadline() {
    let (mut game, actor, _) = shooter();
    let position = [300.0, 250.0, 0.0];
    let kind = game.entities[actor].type_index;
    game.catalog[kind].location_weapons = 1;
    game.tick = 10;
    game.fire_weapon_at(actor, 0, [position[0], position[1]])
        .unwrap();
    assert!(game.entities[actor].attacking());
    assert!(game.entities[actor].weapon_set.status.aiming);
    assert!(!game.entities[actor].weapon_set.status.firing);
    game.tick = 11;
    assert!(game.update_native_attack(actor, Goal::Position(position)));
    assert!(!game.entities[actor].weapon_set.status.aiming);
    assert!(game.entities[actor].weapon_set.status.firing);
    let preattack = game.entities[actor].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .unwrap();
    assert_eq!((preattack.start, preattack.frames), (11, 3));
    for tick in 12..14 {
        game.tick = tick;
        assert!(game.update_native_attack(actor, Goal::Position(position)));
        assert!(game.presentation.shots.is_empty());
    }
    game.tick = 14;
    assert!(game.update_native_attack(actor, Goal::Position(position)));
    assert_eq!(game.presentation.shots.len(), 1);
    assert!(game.presentation.shots[0].target.is_none());
    game.exit_native_attack(actor);
    assert!(!game.entities[actor].attacking());
    assert!(!game.entities[actor].weapon_set.status.aiming);
    assert!(!game.entities[actor].weapon_set.status.firing);
    assert!(game.entities[actor].ai.current_victim.is_none());
}

#[test]
fn immediate_immobile_range_failure_never_enters_attacking_status() {
    let (mut game, actor, target) = shooter();
    let kind = game.entities[actor].type_index;
    game.catalog[kind].kind_of.push("IMMOBILE".into());
    game.entities[target].position = [900.0, 900.0];
    assert!(!game.update_native_attack(actor, Goal::Object(target)));
    assert!(!game.entities[actor].attacking());
    assert!(game.entities[actor].ai.attack.is_none());
    assert!(game.entities[actor].ai.path_request.pending.is_none());
}

#[test]
fn portable_structure_waits_for_its_carrier_then_enters_aim_without_a_path() {
    let (mut game, actor, target) = shooter();
    let kind = game.entities[actor].type_index;
    game.catalog[kind]
        .kind_of
        .extend(["PORTABLE_STRUCTURE".into(), "CAN_ATTACK".into()]);
    game.entities[target].position = [900.0, 900.0];
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    assert!(matches!(
        game.entities[actor].ai.attack.as_ref().unwrap().phase,
        Phase::Carrier
    ));
    assert!(game.entities[actor].ai.path_request.pending.is_none());
    game.entities[target].position = [300.0, 250.0];
    game.tick += 1;
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    assert!(matches!(
        game.entities[actor].ai.attack.as_ref().unwrap().phase,
        Phase::Aim
    ));
    assert!(game.entities[actor].weapon_set.status.aiming);
}

#[test]
fn locked_weapon_allocation_replacement_exits_even_when_the_template_index_matches() {
    let (mut game, actor, target) = shooter();
    let kind = game.entities[actor].type_index;
    game.catalog[kind].weapon_set_rules.replacements = vec![
        crate::skirmish::WeaponTemplateSet {
            flags: 0,
            weapons: [Some(0), None, None],
            share_lock: true,
        },
        crate::skirmish::WeaponTemplateSet {
            flags: 1,
            weapons: [Some(0), None, None],
            share_lock: true,
        },
    ];
    game.replace_weapon_set(actor);
    game.entities[actor].weapon_set.lock = Lock::Temporary;
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    let index = game.entities[actor].weapon_index(0);
    game.replace_weapon_set(actor);
    game.tick += 1;
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    game.entities[actor].weapon_set.upgraded = true;
    game.replace_weapon_set(actor);
    game.tick += 1;
    assert!(!game.update_native_attack(actor, Goal::Object(target)));
    assert_eq!(game.entities[actor].weapon_index(0), index);
    assert!(!game.entities[actor].attacking());
    assert!(
        game.entities[actor].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
}

#[test]
fn target_loss_cancels_fire_and_clears_leech_and_turret_goals() {
    let (mut game, actor, target) = shooter();
    let kind = game.entities[actor].type_index;
    game.catalog[kind].weapon.leech_range = true;
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    game.tick += 1;
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    assert!(game.entities[actor].weapon_set.weapons[0].leech_range_active);
    game.kill_object(target, crate::skirmish::DeathType::Normal);
    game.tick += 1;
    assert!(!game.update_native_attack(actor, Goal::Object(target)));
    assert!(!game.entities[actor].weapon_set.weapons[0].leech_range_active);
    assert!(
        game.entities[actor].weapon_set.weapons[0]
            .pre_attack
            .is_none()
    );
    assert!(!game.entities[actor].attacking());
}

#[test]
fn alternate_rotating_turret_owns_fire_while_the_body_remains_in_aim() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut turret = Rules::source_defaults();
    turret.controlled_slots = 1;
    turret.turn_rate = 0.1;
    game.catalog[kind].secondary_turret = Some(turret);
    game.catalog[kind].weapon.pre_attack_frames = 0;
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    let target = game.spawn(kind, 1, [250.0, 300.0], true);
    game.entities[actor].facing = 0.0;
    game.set_order(actor, Order::Attack(target));
    for tick in 1..19 {
        game.tick = tick;
        assert!(game.update_native_attack(actor, Goal::Object(target)));
        game.update_idle_turret(actor);
        assert!(matches!(
            game.entities[actor].ai.attack.as_ref().unwrap().phase,
            Phase::Aim
        ));
        assert_eq!(game.entities[actor].facing.to_bits(), 0.0_f32.to_bits());
    }
    assert_eq!(game.presentation.shots.len(), 1);
    assert!(
        game.entities[actor]
            .ai
            .secondary_turret
            .as_ref()
            .unwrap()
            .angle
            > 1.5
    );
    assert_eq!(
        game.entities[actor].ai.turret.angle.to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn linked_position_fire_visits_each_allocated_slot_without_changing_current_weapon() {
    let (mut game, actor, _) = shooter();
    let kind = game.entities[actor].type_index;
    let mut secondary =
        crate::skirmish::tests::fixture_type(game.catalog[kind].faction, Kind::Tank).weapon;
    secondary.pre_attack_frames = 0;
    game.catalog[kind].secondary_weapon = Some(secondary);
    game.catalog[kind].turrets_linked = true;
    game.catalog[kind].weapon.pre_attack_frames = 0;
    let goal = Goal::Position([300.0, 250.0, 0.0]);
    for tick in 10..13 {
        game.tick = tick;
        assert!(game.update_native_attack(actor, goal));
    }
    assert_eq!(game.presentation.shots.len(), 2);
    assert_eq!(
        game.presentation
            .shots
            .iter()
            .map(|shot| shot.weapon_slot)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(game.entities[actor].weapon_slot(), 0);
}

#[test]
fn ai_command_replaces_player_source_before_selecting_the_native_weapon() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let mut secondary =
        crate::skirmish::tests::fixture_type(game.players[0].faction, Kind::Tank).weapon;
    secondary.auto_choose = 4;
    secondary.damage = 10.0;
    game.catalog[kind].weapon.auto_choose = 1;
    game.catalog[kind].weapon.damage = 100.0;
    game.catalog[kind].secondary_weapon = Some(secondary);
    let actor = game.spawn(kind, 0, [250.0, 250.0], true);
    let target = game.spawn(kind, 1, [300.0, 250.0], true);
    game.attack(actor, target).unwrap();
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    game.ai_attack_goal(actor, Goal::Object(target), 3);
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    assert_eq!(
        game.entities[actor].weapon_set.weapons[1].max_shots,
        Some(3)
    );
    assert!(game.entities[actor].attacking());
}

#[test]
fn transferred_primary_attack_keeps_prefire_deadline_and_command_origin() {
    let (mut game, actor, target) = shooter();
    let next = game.spawn(game.entities[target].type_index, 1, [305.0, 250.0], true);
    game.tick = 10;
    game.attack(actor, target).unwrap();
    game.tick = 11;
    assert!(game.update_native_attack(actor, Goal::Object(target)));
    let before = game.entities[actor].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .unwrap();
    let deadline = (before.start, before.frames);
    let source = game.entities[actor].ai.command.last_command_source;
    game.transfer_native_attack(actor, target, next);
    assert_eq!(game.entities[actor].ai.current_victim, Some(next));
    assert_eq!(game.entities[actor].ai.command.goal_object, Some(next));
    assert_eq!(game.entities[actor].ai.command.last_command_source, source);
    assert!(matches!(
        game.entities[actor].ai.attack.as_ref().unwrap().phase,
        Phase::Fire
    ));
    let after = game.entities[actor].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .unwrap();
    assert_eq!((after.start, after.frames), deadline);
    game.tick = 12;
    assert!(game.update_native_attack(actor, Goal::Object(next)));
    assert!(game.presentation.shots.is_empty());
    game.tick = 14;
    assert!(game.update_native_attack(actor, Goal::Object(next)));
    assert_eq!(game.presentation.shots.len(), 1);
    assert_eq!(game.presentation.shots[0].target, Some(next));
}

#[test]
fn transfer_updates_current_victim_but_respects_locked_primary_goal() {
    let (mut game, actor, target) = shooter();
    let next = game.spawn(game.entities[target].type_index, 1, [305.0, 250.0], true);
    game.attack(actor, target).unwrap();
    game.entities[actor].ai.current_victim = Some(target);
    game.entities[actor].ai.command.locked = true;
    game.transfer_native_attack(actor, target, next);
    assert_eq!(game.entities[actor].ai.current_victim, Some(next));
    assert_eq!(game.entities[actor].ai.command.goal_object, Some(target));
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(victim) if victim == target));
}
