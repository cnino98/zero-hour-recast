use super::*;
use crate::skirmish::{Research, drones, spawn};

const UPGRADE: &str = "Upgrade_GLACamoNetting";

fn site() -> (Skirmish, usize) {
    let mut game = tests::game();
    let mut stealth = rules();
    stealth.innate = false;
    stealth.forbidden = ATTACKING | USING_ABILITY | TAKING_DAMAGE;
    stealth.upgrades = vec![UPGRADE.into()];
    game.catalog[0].behavior.stealth = Some(stealth);
    tests::register_update(&mut game.catalog[0], "StealthUpdate");
    tests::register_behavior(&mut game.catalog[0], "StealthUpgrade", 0x80);
    game.catalog[0]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers = vec![UPGRADE.into()];
    game.catalog[0]
        .kind_of
        .push("SPAWNS_ARE_THE_WEAPONS".into());
    game.catalog[0].maintenance.research = vec![Research {
        name: UPGRADE.into(),
        cost: 500,
        frames: 2,
        player: false,
    }];
    let mut soldier = rules();
    soldier.innate = false;
    soldier.forbidden = ATTACKING | USING_ABILITY;
    soldier.upgrades = vec![UPGRADE.into()];
    game.catalog[5].name = "StingerChild".into();
    game.catalog[5].behavior.stealth = Some(soldier);
    tests::register_update(&mut game.catalog[5], "StealthUpdate");
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
    game.catalog[0].behavior.spawns.push(spawn::Rules {
        conditions: crate::skirmish::tests::death_conditions(),
        templates: vec!["StingerChild".into()],
        count: 1,
        replace_frames: 30,
        flags: spawn::Rules::REQUIRE_SPAWNER,
        initial_burst: 0,
    });
    game.catalog[0]
        .behavior
        .spawn_exits
        .push(vec![spawn::Point {
            position: [10.0, 0.0, 0.0],
            basis: crate::skirmish::physical_ocl::transform::yaw(0.0),
        }]);
    crate::skirmish::tests::register_update(&mut game.catalog[0], "SpawnPointProductionExitUpdate");
    crate::skirmish::tests::register_update(&mut game.catalog[0], "SpawnBehavior");
    game.catalog[0]
        .behavior_modules
        .last_mut()
        .unwrap()
        .interfaces |= 6;
    let parent = game.spawn(0, 0, [200.0, 300.0], true);
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(game.spawned_children(parent).len(), 1);
    (game, parent)
}

fn complete(game: &mut Skirmish, parent: usize) {
    game.queue_upgrade(parent, 0).unwrap();
    game.update_production(parent);
    assert!(!game.object_has_upgrade(parent, UPGRADE));
    game.update_production(parent);
    assert!(game.object_has_upgrade(parent, UPGRADE));
    game.tick += 3;
    game.run_object_updates();
}

fn replace(game: &mut Skirmish, parent: usize, child: usize, keep_revealed: bool) -> usize {
    game.apply_hit(child, 2, 1000.0);
    assert!(!game.entities[child].alive());
    for _ in 0..46 {
        if keep_revealed {
            game.apply_hit(parent, 2, 1.0);
        }
        game.tick += 1;
        game.run_object_updates();
        if let Some(&replacement) = game.spawned_children(parent).first() {
            return replacement;
        }
    }
    panic!("native replacement scan did not create a soldier");
}

#[test]
fn object_camo_requires_paid_completion_and_cancellation_never_grants_other_sites() {
    let (mut game, parent) = site();
    let child = game.spawned_children(parent)[0];
    let other = game.spawn(0, 0, [300.0, 300.0], true);
    let cash = game.players[0].cash;
    game.queue_upgrade(parent, 0).unwrap();
    assert_eq!(game.players[0].cash, cash - 500);
    game.update_production(parent);
    game.cancel_upgrade(0, parent, 0).unwrap();
    assert_eq!(game.players[0].cash, cash);
    game.tick += 3;
    game.run_object_updates();
    assert!(!game.entities[parent].stealthed());
    assert!(!game.entities[child].stealthed());
    complete(&mut game, parent);
    assert!(game.entities[parent].stealthed());
    assert!(game.entities[child].stealthed());
    assert!(!game.entities[other].stealthed());
    assert!(!game.has_upgrade(0, UPGRADE));
    assert!(!game.object_has_upgrade(child, UPGRADE));
    assert!(game.can_research(other, 0));
    assert!(!game.can_research(parent, 0));
}

#[test]
fn spawned_weapon_attack_and_parent_damage_reveal_until_the_original_delay() {
    let (mut game, parent) = site();
    complete(&mut game, parent);
    let child = game.spawned_children(parent)[0];
    let enemy = game.spawn(6, 1, [230.0, 300.0], true);
    game.attack(child, enemy).unwrap();
    assert!(game.update_attack(child, enemy));
    assert!(game.entities[child].attacking());
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[parent].stealthed());
    assert!(!game.entities[child].stealthed());
    game.stop(parent).unwrap();
    game.tick += 2;
    game.run_object_updates();
    assert!(game.entities[parent].stealthed());
    assert!(game.entities[child].stealthed());
    game.apply_hit(parent, 2, 1.0);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[parent].stealthed());
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[parent].stealthed());
    game.tick += 2;
    game.run_object_updates();
    assert!(game.entities[parent].stealthed());
}

#[test]
fn replacement_inherits_only_a_currently_stealthed_slaver_not_a_replayed_upgrade() {
    let (mut game, parent) = site();
    complete(&mut game, parent);
    let child = game.spawned_children(parent)[0];
    let replacement = replace(&mut game, parent, child, false);
    assert_ne!(replacement, child);
    assert!(game.entities[replacement].stealthed());
    assert!(!game.object_has_upgrade(replacement, UPGRADE));
    // Native onEnslave grants only when the master is STEALTHED. An upgrade
    // that already executed does not periodically grant subsequent children.
    game.apply_hit(parent, 2, 1.0);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[parent].stealthed());
    let revealed_replacement = replace(&mut game, parent, replacement, true);
    game.activate_upgrade_modules(parent);
    game.run_object_updates();
    assert!(!game.entities[revealed_replacement].stealthed());
    assert!(
        !game.entities[revealed_replacement]
            .stealth
            .as_ref()
            .unwrap()
            .has(CAN_STEALTH)
    );
}
