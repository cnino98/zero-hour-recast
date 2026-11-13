use super::*;
use crate::skirmish::{
    AreaDamage, DamageType, DeathType, Faction, Kind, Order, Research, WeaponRules,
    death_lifecycle::{DieAction, Rules, SlowDeath},
    tests::{fixture_type, game},
};

fn fixture() -> (Skirmish, usize, usize, usize) {
    let mut game = game();
    let real = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[real].name = "GLABarracks".into();
    let unit = fake_type();
    let fake = game.catalog.len();
    game.catalog.push(unit);
    let worker = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let worker_type = game.entities[worker].type_index;
    game.catalog[worker_type]
        .buildable_objects
        .push("FakeGLABarracks".into());
    crate::skirmish::module_updates::validate_registrations(&game.catalog[fake]).unwrap();
    // Construction requires currently clear shroud, not just a geometric preview.
    // Start the worker near the test site through the actual transform/visibility seam.
    game.set_object_position(worker, [300.0, 400.0, 0.0]);
    (game, fake, real, worker)
}

fn fake_type() -> super::UnitType {
    let mut unit = fixture_type(Faction::Gla, Kind::TechBuilding);
    unit.name = "FakeGLABarracks".into();
    unit.kind_of.push("FS_FAKE".into());
    unit.kind_of.push("CAN_ATTACK".into());
    unit.cost = 125;
    unit.build_seconds = 5.0;
    unit.max_health = 125.0;
    unit.initial_health = 125.0;
    unit.weapon = WeaponRules {
        name: "SuicideWeapon".into(),
        range: 1.0,
        anti_mask: 2,
        clip_size: 1,
        damage: 999_999.0,
        damage_type: DamageType::Explosion,
        death_type: DeathType::Suicided,
        auto_choose: 0,
        area: Some(AreaDamage {
            at_source: true,
            primary_radius: 1.0,
            secondary_radius: 0.0,
            secondary_damage: 0.0,
            affects: 40,
        }),
        ..WeaponRules::unarmed()
    };
    unit.maintenance.replacement = Some(Replacement {
        upgrade: "Upgrade_BecomeRealGLABarracks".into(),
        object: "GLABarracks".into(),
    });
    unit.behavior.replacement_names.push("GLABarracks".into());
    crate::skirmish::tests::register_behavior(&mut unit, "ReplaceObjectUpgrade", 0x80);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers =
        vec!["Upgrade_BecomeRealGLABarracks".into()];
    unit.production = Some(Vec::new());
    crate::skirmish::tests::register_update(&mut unit, "ProductionUpdate");
    unit.maintenance.research.push(Research {
        name: "Upgrade_BecomeRealGLABarracks".into(),
        cost: 375,
        frames: 300,
        player: false,
    });
    unit.behavior
        .death_fx
        .push(crate::skirmish::tests::death_fx("FX_StructureSmallDeath"));
    let mut destruction = crate::skirmish::tests::death_conditions();
    destruction.required_status = 1 << 3;
    unit.behavior.destroy_die.push(destruction);
    crate::skirmish::tests::register_behavior(&mut unit, "FXListDie", 0x82);
    crate::skirmish::tests::register_behavior(&mut unit, "DestroyDie", 2);
    unit.death_lifecycle = Some(Rules {
        on_die: vec![DieAction::Slow],
        physics: None,
        slow: vec![SlowDeath {
            derived: crate::skirmish::death_lifecycle::Derived::None,
            tag: "Slow".into(),
            overkill_bonus: 0.0,
            conditions: crate::skirmish::death_callbacks::Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 1 << 3,
                required_status: 0,
            },
            probability: 10,
            sink_delay: [0, 0],
            destruction_delay: [0, 0],
            sink_rate: 0.0,
            fling: None,
            fx: std::array::from_fn(|_| Vec::new()),
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: [Vec::new(), Vec::new(), vec![0]],
        }],
        weapons: vec![WeaponRules {
            name: "FakeStructureDetonationWeapon".into(),
            damage: 200.0,
            damage_type: DamageType::Explosion,
            death_type: DeathType::Exploded,
            area: Some(AreaDamage {
                at_source: true,
                primary_radius: 50.0,
                secondary_radius: 200.0,
                secondary_damage: 50.0,
                affects: 15,
            }),
            ..WeaponRules::unarmed()
        }],
    });
    crate::skirmish::tests::register_update(&mut unit, "SlowDeathBehavior");
    let module = unit.behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    unit.death_lifecycle.as_mut().unwrap().slow[0]
        .tag
        .clone_from(&module.tag);
    unit
}

fn build_fake(game: &mut Skirmish, fake: usize, worker: usize) -> usize {
    // Native Dozer task timestamps use zero as the absent-command sentinel.
    game.step();
    let point = game.placement_near(fake, [350.0, 450.0]).unwrap();
    let id = game.build(worker, fake, point).unwrap();
    for _ in 0..6000 {
        game.step();
        if game.entities[id].complete() {
            break;
        }
    }
    assert!(game.entities[id].complete());
    id
}

#[test]
fn paid_replacement_destroys_old_id_without_refund_death_or_health_transfer() {
    let (mut game, fake_type, real, worker) = fixture();
    let fake = build_fake(&mut game, fake_type, worker);
    game.entities[fake].health = 25.0;
    let old_position = game.object_position(fake);
    let old_facing = game.entities[fake].facing;
    let cash = game.players[0].cash;
    let spent = game.players[0].statistics.money_spent;
    let built = game.players[0].statistics.buildings_built;
    game.queue_upgrade(fake, 0).unwrap();
    assert_eq!(game.players[0].cash, cash - 375);
    for _ in 0..300 {
        game.step();
    }
    let event = game.presentation.replacement_events.last().unwrap();
    assert_eq!(event.previous, fake);
    let next = event.next;
    assert_eq!(game.entities[next].type_index, real);
    assert_eq!(game.entities[next].owner, 0);
    assert_eq!(
        game.object_position(next).map(f32::to_bits),
        old_position.map(f32::to_bits)
    );
    assert_eq!(game.entities[next].facing.to_bits(), old_facing.to_bits());
    assert_eq!(
        game.entities[next].health.to_bits(),
        game.catalog[real].initial_health.to_bits()
    );
    assert!(game.entities[fake].masked());
    assert!(game.entities[fake].queue.is_empty());
    assert_eq!(game.players[0].cash, cash - 375);
    assert_eq!(
        game.players[0].statistics.money_spent,
        spent + 375 + game.catalog[real].cost
    );
    assert_eq!(game.players[0].statistics.buildings_built, built + 1);
    assert!(game.presentation.object_fx_events.is_empty());
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn captured_conversion_keeps_paid_queue_and_replaces_for_current_owner() {
    let (mut game, fake_type, real, worker) = fixture();
    let fake = build_fake(&mut game, fake_type, worker);
    let payer_cash = game.players[0].cash;
    let receiver_cash = game.players[1].cash;
    game.queue_upgrade(fake, 0).unwrap();
    game.transfer_maintenance_owner(fake, 1);
    assert_eq!(game.entities[fake].queue.len(), 1);
    for _ in 0..300 {
        game.step();
    }
    let next = game.presentation.replacement_events.last().unwrap().next;
    assert_eq!(game.entities[next].owner, 1);
    assert_eq!(game.entities[next].type_index, real);
    assert_eq!(game.players[0].cash, payer_cash - 375);
    assert_eq!(game.players[1].cash, receiver_cash);
}

#[test]
fn cancelled_conversion_and_sale_refund_without_detonation() {
    let (mut game, fake_type, _, worker) = fixture();
    // Original GameData SellPercentage is 50%; the generic fixture defaults to zero.
    game.catalog[fake_type].maintenance.sell_fraction = 0.5;
    let fake = build_fake(&mut game, fake_type, worker);
    let cash = game.players[0].cash;
    game.queue_upgrade(fake, 0).unwrap();
    game.cancel_upgrade(0, fake, 0).unwrap();
    assert_eq!(game.players[0].cash, cash);
    game.queue_upgrade(fake, 0).unwrap();
    game.sell(0, fake).unwrap();
    for _ in 0..200 {
        game.step();
    }
    assert!(game.entities[fake].masked());
    assert_eq!(game.players[0].cash, cash + 62);
    assert!(game.presentation.replacement_events.is_empty());
    assert!(game.presentation.shots.is_empty());
    assert!(game.presentation.object_fx_events.is_empty());
}

#[test]
fn stopped_foundation_is_ineligible_and_dies_without_final_detonation() {
    let (mut game, fake_type, _, worker) = fixture();
    let point = game.placement_near(fake_type, [350.0, 450.0]).unwrap();
    let fake = game.build(worker, fake_type, point).unwrap();
    game.stop(worker).unwrap();
    for _ in 0..10 {
        game.step();
    }
    assert!(game.entities[fake].under_construction());
    assert!(matches!(game.entities[worker].ai.order, Order::Idle));
    assert!(!game.can_detonate_fake(0, fake));
    assert!(!game.can_research(fake, 0));
    game.apply_typed_hit(fake, 0, 999_999.0, DeathType::Normal);
    game.step();
    assert!(game.entities[fake].masked());
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn explicit_detonation_fires_suicide_then_final_blast_once_without_a_hole() {
    let (mut game, fake_type, _, worker) = fixture();
    let fake = build_fake(&mut game, fake_type, worker);
    let point = game.entities[fake].position;
    game.catalog[6].ai.acquisition = 0;
    let target = game.spawn(6, 1, [point[0] + 40.0, point[1]], true);
    // Native registration dirties COIs; range iterators see the new enemy only
    // after PartitionManager's end-of-frame update.
    assert!(
        !game
            .partition
            .fastest_range_objects(point, 200.0)
            .contains(&target)
    );
    game.step();
    assert!(
        game.partition
            .fastest_range_objects(point, 200.0)
            .contains(&target)
    );
    assert!(!game.can_detonate_fake(1, fake));
    game.detonate_fake(0, fake).unwrap();
    assert!(game.entities[fake].alive());
    assert!(game.presentation.shots.is_empty());
    assert!(matches!(game.entities[fake].ai.order, Order::FireAt { .. }));
    game.step();
    assert!(matches!(
        game.entities[fake].damage_state.death_type,
        DeathType::Suicided
    ));
    assert!(!game.entities[fake].alive());
    // INITIAL AI fires first. The normal-phase SlowDeath can run on the
    // same tick for the authored zero-delay FINAL callback.
    assert!(game.entities[fake].masked());
    assert!(!game.entities[target].alive());
    let weapons: Vec<_> = game
        .presentation
        .shots
        .iter()
        .map(|shot| {
            game.catalog[shot.weapon_type]
                .weapon_template(shot.weapon_index)
                .unwrap()
                .name
                .as_str()
        })
        .collect();
    assert_eq!(weapons, ["SuicideWeapon", "FakeStructureDetonationWeapon"]);
    assert!(!game.entities[fake].reconstructing());
    assert!(game.detonate_fake(0, fake).is_err());
    game.step();
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn worker_zero_time_command_upgrade_removes_masks_and_can_switch_repeatedly() {
    let mut game = game();
    let worker = game.type_for(0, Kind::Builder).unwrap();
    let old_worker = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && actor.type_index == worker)
        .unwrap();
    let position = game.entities[old_worker].position;
    game.destroy_maintenance_object(old_worker);
    game.process_update_destruction();
    game.catalog[worker].maintenance.command_sets = vec![
        CommandSet {
            alternate: None,
            upgrade: "Upgrade_GLAWorkerFakeCommandSet".into(),
            command_set: "GLAWorkerFakeBuildingsCommandSet".into(),
            removes: vec!["Upgrade_GLAWorkerRealCommandSet".into()],
        },
        CommandSet {
            alternate: None,
            upgrade: "Upgrade_GLAWorkerRealCommandSet".into(),
            command_set: "GLAWorkerCommandSet".into(),
            removes: vec![
                "Upgrade_GLAWorkerFakeCommandSet".into(),
                "Upgrade_GLAWorkerRealCommandSet".into(),
            ],
        },
    ];
    game.catalog[worker].maintenance.research = game.catalog[worker]
        .maintenance
        .command_sets
        .iter()
        .map(|command| Research {
            name: command.upgrade.clone(),
            cost: 0,
            frames: 0,
            player: false,
        })
        .collect();
    for ordinal in 0..game.catalog[worker].maintenance.command_sets.len() {
        let unit = &mut game.catalog[worker];
        crate::skirmish::tests::register_behavior(unit, "CommandSetUpgrade", 0x80);
        let command = &unit.maintenance.command_sets[ordinal];
        let module = unit.behavior_modules.last_mut().unwrap();
        module.upgrade.triggers = vec![command.upgrade.clone()];
        module.upgrade.removals.clone_from(&command.removes);
    }
    // GLAInfantryWorker ProductionUpdate ModuleTag_16: one command-switch upgrade.
    game.catalog[worker].production_queue_limit = 1;
    game.catalog[worker].production = Some(Vec::new());
    crate::skirmish::tests::register_update(&mut game.catalog[worker], "ProductionUpdate");
    crate::skirmish::module_updates::validate_registrations(&game.catalog[worker]).unwrap();
    let id = game.spawn(worker, 0, position, true);
    let cash = game.players[0].cash;
    for _ in 0..3 {
        game.queue_upgrade(id, 0).unwrap();
        assert!(!game.can_research(id, 1));
        game.step();
        assert_eq!(
            game.entities[id].command_set_override(),
            Some("GLAWorkerFakeBuildingsCommandSet")
        );
        assert!(game.object_has_upgrade(id, "Upgrade_GLAWorkerFakeCommandSet"));
        game.queue_upgrade(id, 1).unwrap();
        game.step();
        assert_eq!(
            game.entities[id].command_set_override(),
            Some("GLAWorkerCommandSet")
        );
        assert!(!game.object_has_upgrade(id, "Upgrade_GLAWorkerFakeCommandSet"));
        assert!(!game.object_has_upgrade(id, "Upgrade_GLAWorkerRealCommandSet"));
    }
    assert_eq!(game.players[0].cash, cash);
}

#[test]
fn rotated_replacement_immediately_registers_real_non_square_footprint() {
    let (mut game, fake_type, real, worker) = fixture();
    game.catalog[real].geometry.shape = crate::collision_geometry::Shape::Box;
    game.catalog[real].geometry.major = 60.0;
    game.catalog[real].geometry.minor = 10.0;
    let fake = build_fake(&mut game, fake_type, worker);
    let position = game.entities[fake].position;
    game.set_object_orientation(fake, std::f32::consts::FRAC_PI_2);
    game.rebuild_navigation();
    let tip = game.cell_at([position[0], position[1] + 45.0]).unwrap();
    let unrotated_tip = game.cell_at([position[0] + 45.0, position[1]]).unwrap();
    assert!(!game.navigation.is_blocked(tip));
    game.queue_upgrade(fake, 0).unwrap();
    for _ in 0..300 {
        game.step();
    }
    let next = game.presentation.replacement_events.last().unwrap().next;
    assert_eq!(
        game.entities[next].facing.to_bits(),
        std::f32::consts::FRAC_PI_2.to_bits()
    );
    assert!(game.navigation.is_blocked(tip));
    assert!(!game.navigation.is_blocked(unrotated_tip));
    assert!(game.partition.outward_objects(position).contains(&next));
}

#[test]
fn replacement_releases_shared_cells_before_creation_and_keeps_them_after_old_destruction() {
    use crate::skirmish::movement::obstacles::OwnerId;

    let (mut game, fake_type, _, worker) = fixture();
    let fake = build_fake(&mut game, fake_type, worker);
    let center = game.cell_at(game.entities[fake].position).unwrap();
    assert!(game.navigation_obstacles.at(center).unwrap().id == OwnerId::Actor(fake));
    game.queue_upgrade(fake, 0).unwrap();
    for _ in 0..300 {
        game.update_production(fake);
    }
    let replacement = game.presentation.replacement_events.last().unwrap().next;
    assert!(game.entities[fake].exists());
    assert!(game.update_objects[game.entities[fake].update_owner].destroy_requested);
    assert!(game.navigation_obstacles.at(center).unwrap().id == OwnerId::Actor(replacement));
    game.rebuild_navigation();
    assert!(game.navigation_obstacles.at(center).unwrap().id == OwnerId::Actor(replacement));
    game.process_update_destruction();
    assert!(!game.entities[fake].exists());
    assert!(game.navigation.is_blocked(center));
    assert!(game.navigation_obstacles.at(center).unwrap().id == OwnerId::Actor(replacement));
}
