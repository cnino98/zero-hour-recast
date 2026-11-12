use super::*;
use crate::skirmish::{Faction, Kind, Order, drones, tests};

fn rules() -> MobRules {
    MobRules {
        catch_up: 40,
        no_catch_up: 15,
        squirrelliness: 0.05,
        crisis_calls: 30,
        wander: tests::fixture_type(Faction::Gla, Kind::Infantry).locomotor,
        wander_speed: 10.0,
        panic: tests::fixture_type(Faction::Gla, Kind::Infantry).locomotor,
        panic_speed: 30.0,
    }
}

fn family() -> (Skirmish, usize, usize) {
    family_with_freedom(false)
}

fn family_with_freedom(free_will: bool) -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    game.catalog[5].name = "NativeMobMember".into();
    game.catalog[5].behavior.mobs.push(rules());
    tests::register_update(&mut game.catalog[5], "MobMemberSlavedUpdate");
    let mut parent = tests::fixture_type(Faction::Gla, Kind::Vehicle);
    parent.contain = Some(crate::skirmish::containment::tests::rules(0.0, 0.0));
    tests::register_update(&mut parent, "TransportContain");
    parent.behavior.spawns.push(crate::skirmish::spawn::Rules {
        templates: vec!["NativeMobMember".into()],
        count: 1,
        replace_frames: 30,
        flags: crate::skirmish::spawn::Rules::ONE_SHOT
            | if free_will {
                crate::skirmish::spawn::Rules::FREE_WILL
            } else {
                0
            },
        initial_burst: 0,
        conditions: tests::death_conditions(),
    });
    tests::register_update(&mut parent, "SpawnBehavior");
    parent.behavior_modules.last_mut().unwrap().interfaces |= 6;
    let kind = game.catalog.len();
    game.catalog.push(parent);
    let parent = game.spawn(kind, 0, [355.0, 255.0], true);
    game.tick = 2;
    game.update_spawn(parent, 0);
    let child = game.spawned_children(parent)[0];
    (game, parent, child)
}

#[test]
fn group_stop_enters_parent_idle_before_its_slave_idle() {
    let (mut game, parent, child) = family();
    let mut expected = game.random;
    let parent_offset = startup::random_between(&mut expected, 60);
    let child_offset = startup::random_between(&mut expected, 60);
    assert_ne!(
        parent_offset, child_offset,
        "distinct real draws make entry order observable"
    );
    game.stop(parent).unwrap();
    assert_eq!(game.entities[parent].ai.idle.sleep_offset, parent_offset);
    assert_eq!(game.entities[child].ai.idle.sleep_offset, child_offset);
    assert_eq!(game.random, expected);
    assert!(game.entities[parent].ai_idle() && game.entities[child].ai_idle());
    assert!(!game.entities[parent].last_command_from_ai());
    assert!(!game.entities[child].last_command_from_ai());
}

#[test]
fn authored_constructor_draws_and_created_clamp_are_per_instance() {
    let mut game = tests::game();
    for ratio in [-0.5, 2.0] {
        let mut data = rules();
        data.squirrelliness = ratio;
        game.catalog[5].behavior.mobs.push(data);
        tests::register_update(&mut game.catalog[5], "MobMemberSlavedUpdate");
    }
    let mut expected = game.random;
    for _ in 0..3 {
        startup::random_value(&mut expected);
    }
    let mut waits = Vec::new();
    for _ in 0..2 {
        waits.push(i32::try_from(startup::random_between(&mut expected, 20)).unwrap());
        for _ in 0..3 {
            startup::random_real(&mut expected, 0.2, 0.4);
        }
    }
    // Attack-move, team follow and individual follow each construct a child Idle.
    for _ in 0..3 {
        startup::random_between(&mut expected, 60);
    }
    let idle_offset = startup::random_between(&mut expected, 60); // Outer Idle.
    let child = game.spawn(5, 0, [300.0, 300.0], true);
    assert_eq!(game.random, expected);
    assert_eq!(game.entities[child].ai.idle.sleep_offset, idle_offset);
    assert_eq!(
        game.entities[child]
            .behaviors
            .mobs
            .iter()
            .map(|state| state.wait)
            .collect::<Vec<_>>(),
        waits
    );
    assert_eq!(
        game.entities[child].behaviors.mobs[0]
            .squirrelliness
            .to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[child].behaviors.mobs[1]
            .squirrelliness
            .to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn first_slaved_virtual_interface_wins_over_later_mob_and_orphan_kills_natively() {
    let mut game = tests::game();
    game.catalog[5].behavior.slaves.push(drones::Rules {
        guard_range: 0.0,
        guard_wander: 0.0,
        attack_range: 0.0,
        attack_wander: 0.0,
        scout_range: 0.0,
        scout_wander: 0.0,
        spotting_range: 0.0,
        repair: None,
        panic: None,
        panic_speed: 0.0,
        stay_on_master_layer: false,
    });
    tests::register_update(&mut game.catalog[5], "SlavedUpdate");
    game.catalog[5].behavior.mobs.push(rules());
    tests::register_update(&mut game.catalog[5], "MobMemberSlavedUpdate");
    let child = game.spawn(5, 0, [300.0, 300.0], true);
    game.enslave_spawn(child, 0);
    assert_eq!(game.slaved_parent(child), Some(0));
    assert_eq!(game.entities[child].behaviors.mobs[0].parent, None);
    game.update_mob_member(child, 0);
    assert!(!game.entities[child].alive());
    assert_eq!(game.entities[child].last_attacker(), None);
}

#[test]
fn native_parent_death_releases_mob_and_canonical_held_before_detaching_producer() {
    let (mut game, parent, child) = family();
    game.set_disabled_held(child);
    game.apply_native_damage(
        parent,
        None,
        game.entities[parent].health,
        crate::skirmish::DeathType::Normal,
    );
    assert_eq!(game.slaved_parent(child), None);
    assert_eq!(game.entities[child].producer, None);
    assert!(!game.entities[child].disabled_held());
    assert!(game.entities[child].alive());
}

#[test]
fn self_task_admission_reads_retained_command_source_and_actual_spawn_count() {
    let (mut game, parent, _) = family();
    assert!(game.may_spawn_self_task(parent, 0.5));
    assert!(!game.may_spawn_self_task(parent, 0.0));
    game.move_to(parent, [450.0, 255.0]).unwrap();
    assert!(!game.may_spawn_self_task(parent, 0.5));
    game.ai_idle_from_ai(parent);
    assert!(game.may_spawn_self_task(parent, 0.5));
}

#[test]
fn vertical_separation_uses_center_3d_and_ai_move_retains_parent_xyz() {
    let (mut game, parent, child) = family();
    let position = game.entities[child].position;
    game.set_object_position(parent, [position[0], position[1], 150.0]);
    game.ai_idle_from_ai(parent);
    for _ in 0..16 {
        game.update_mob_member(child, 0);
    }
    assert_eq!(game.entities[child].ai.locomotor_set, set::PANIC);
    assert_eq!(
        game.entities[child].ai_goal_position()[2].to_bits(),
        150.0_f32.to_bits()
    );
    assert!(game.entities[child].last_command_from_ai());
}

#[test]
fn player_group_attack_forces_slaves_but_move_does_not_invent_child_orders() {
    let (mut game, parent, child) = family();
    let target = game.spawn(6, 1, [390.0, 255.0], true);
    game.group_attack_objects(&[parent], target, false).unwrap();
    assert!(matches!(game.entities[child].ai.order, Order::Attack(id) if id == target));
    assert!(game.entities[child].ai.attack_context.forced);
    assert!(!game.entities[child].last_command_from_ai());
    game.stop(parent).unwrap();
    assert!(game.entities[child].ai_idle());
    game.move_to(parent, [450.0, 255.0]).unwrap();
    assert!(
        game.entities[child].ai_idle(),
        "regrouping belongs to the real sparse Mob callback"
    );
}

#[test]
fn scripts_and_mob_ai_share_actual_role_and_constructor_order() {
    let mut game = tests::game();
    game.catalog[5].behavior.mobs.push(rules());
    tests::register_update(&mut game.catalog[5], "MobMemberSlavedUpdate");
    let mut expected = game.random;
    let _ = crate::skirmish::locomotor::WanderState::new(&mut expected, 1.0);
    let wait = i32::try_from(startup::random_between(&mut expected, 20)).unwrap();
    for _ in 0..3 {
        startup::random_real(&mut expected, 0.2, 0.4);
    }
    // AIStateMachine constructs all three child Idles before entering outer Idle.
    for _ in 0..3 {
        startup::random_between(&mut expected, 60);
    }
    let idle_offset = startup::random_between(&mut expected, 60);
    let id = game.spawn(5, 0, [300.0, 400.0], true);
    assert_eq!(game.random, expected);
    assert_eq!(game.entities[id].ai.idle.sleep_offset, idle_offset);
    assert_eq!(game.entities[id].behaviors.mobs[0].wait, wait);
    game.choose_locomotor_set(id, set::WANDER);
    let mut expected = game.random;
    let _ = crate::skirmish::locomotor::WanderState::new(&mut expected, 1.0);
    game.choose_mob_locomotor(id, set::NORMAL);
    assert_eq!(game.entities[id].ai.locomotor_set, set::NORMAL);
    assert_eq!(game.random, expected);
    game.choose_mob_locomotor(id, set::PANIC);
    let mut expected = game.random;
    let _ = crate::skirmish::locomotor::WanderState::new(&mut expected, 1.0);
    game.choose_locomotor_set(id, set::WANDER);
    assert_eq!(game.entities[id].ai.locomotor_set, set::WANDER);
    assert_eq!(
        game.entities[id].upgraded_speed(&game.catalog[5]).to_bits(),
        10.0_f32.to_bits()
    );
    assert_eq!(game.random, expected);
    game.choose_mob_locomotor(id, set::WANDER);
    assert_eq!(game.random, expected);
}

#[test]
fn ground_group_dispatches_real_slave_position_commands_only_without_free_will() {
    for free_will in [false, true] {
        let (mut game, parent, child) = family_with_freedom(free_will);
        let revision = game.entities[child].ai.command_revision;
        let goal = [410.0, 270.0, 18.0];
        game.group_force_attack_position(&[parent], goal).unwrap();
        assert!(matches!(
            game.entities[parent].ai.order,
            Order::FireAt { .. }
        ));
        if free_will {
            assert_eq!(game.entities[child].ai.command_revision, revision);
        } else {
            assert!(matches!(
                game.entities[child].ai.order,
                Order::FireAt { .. }
            ));
            assert_eq!(
                game.entities[child].ai_goal_position().map(f32::to_bits),
                goal.map(f32::to_bits)
            );
            assert!(!game.entities[child].last_command_from_ai());
            let slot = game.entities[child].weapon_slot();
            assert_eq!(
                game.entities[child].weapon_set.weapons[slot].max_shots,
                Some(i32::MAX.unsigned_abs())
            );
        }
    }
}
