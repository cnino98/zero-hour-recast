use super::{ArmorUpgrade, MovementUpgrade, PowerUpgrade, WeaponUpgrade};
use crate::skirmish::locomotor::GroundGoal;
use crate::skirmish::{GroundAppearance, GroundRules, Kind, Research, tests::game};

fn research(name: &str, player: bool) -> Research {
    Research {
        name: name.into(),
        cost: 100,
        frames: 2,
        player,
        sciences: Vec::new(),
        ignores_underpowered: false,
    }
}

fn register_upgrade(unit: &mut crate::skirmish::UnitType, family: &str, trigger: &str) {
    crate::skirmish::tests::register_behavior(unit, family, 0x80);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec![trigger.into()];
}

#[test]
fn power_research_is_per_object_and_transfers_without_stacking() {
    let mut game = game();
    // Retire the old instance before changing its template's behavior census.
    game.destroy_maintenance_object(0);
    game.process_update_destruction();
    game.catalog[0].energy = 5;
    game.catalog[0].maintenance.research = vec![research("Rods", false)];
    game.catalog[0].maintenance.power_upgrade = Some(PowerUpgrade {
        trigger: "Rods".into(),
        bonus: 5,
    });
    game.catalog[0].maintenance.power_plants = vec![18];
    crate::skirmish::tests::register_update(&mut game.catalog[0], "PowerPlantUpdate");
    crate::skirmish::tests::register_behavior(&mut game.catalog[0], "PowerPlantUpgrade", 0x80);
    game.catalog[0]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers = vec!["Rods".into()];
    // The original starting Object predates this test-authored template. Recreate
    // both test plants through their actual constructor registrations.
    let first = game.spawn(0, 0, [100.0, 100.0], true);
    let second = game.spawn(0, 0, [450.0, 100.0], true);
    game.update_power();
    let before = game.players[0].power;
    let spent = game.players[0].statistics.money_spent;
    game.queue_upgrade(first, 0).unwrap();
    game.cancel_upgrade(0, first, 0).unwrap();
    assert_eq!(game.players[0].statistics.money_spent, spent);
    game.queue_upgrade(first, 0).unwrap();
    game.update_production(first);
    assert_eq!(game.players[0].power, before);
    game.update_production(first);
    assert_eq!(game.players[0].power, before + 5);
    assert_eq!(game.players[0].statistics.money_spent, spent + 100);
    assert!(
        game.entities[first]
            .upgrade_conditions(game.tick)
            .contains(&"POWER_PLANT_UPGRADING")
    );
    assert!(
        game.entities[second]
            .upgrade_conditions(game.tick)
            .is_empty()
    );
    game.activate_upgrade_modules(first);
    game.update_power();
    assert_eq!(game.players[0].power, before + 5);
    game.tick += 18;
    game.run_object_updates();
    assert!(
        game.entities[first]
            .upgrade_conditions(game.tick)
            .contains(&"POWER_PLANT_UPGRADED")
    );
    let enemy_before = game.players[1].power;
    game.transfer_maintenance_owner(first, 1);
    assert_eq!(game.players[1].power, enemy_before + 10);
    assert_eq!(game.players[0].power, before - 5);
}

#[test]
fn duplicate_power_upgrades_transfer_each_authored_bonus_once() {
    let mut game = game();
    game.catalog[0].energy = 5;
    game.catalog[0].maintenance.power_upgrade = Some(PowerUpgrade {
        trigger: "Rods".into(),
        bonus: 5,
    });
    for _ in 0..2 {
        crate::skirmish::tests::register_behavior(&mut game.catalog[0], "PowerPlantUpgrade", 0x80);
        game.catalog[0]
            .behavior_modules
            .last_mut()
            .unwrap()
            .upgrade
            .triggers = vec!["Rods".into()];
    }
    let plant = game.spawn(0, 0, [450.0, 100.0], true);
    game.update_power();
    game.grant_native_object_upgrade(plant, "Rods");
    assert!(game.native_upgrade_executed(plant, "PowerPlantUpgrade", 0));
    assert!(game.native_upgrade_executed(plant, "PowerPlantUpgrade", 1));
    let old_power = game.players[0].power;
    let new_power = game.players[1].power;
    game.transfer_maintenance_owner(plant, 1);
    assert_eq!(game.players[0].power, old_power - 15);
    assert_eq!(game.players[1].power, new_power + 15);
    assert!(game.native_upgrade_executed(plant, "PowerPlantUpgrade", 0));
    assert!(game.native_upgrade_executed(plant, "PowerPlantUpgrade", 1));
    game.destroy_maintenance_object(plant);
    assert!(!game.native_upgrade_executed(plant, "PowerPlantUpgrade", 0));
    assert!(!game.native_upgrade_executed(plant, "PowerPlantUpgrade", 1));
    game.process_update_destruction();
    game.update_power();
    assert_eq!(game.players[1].power, new_power);
}

#[test]
fn player_research_activates_owned_existing_and_future_objects_and_adds_weapon_bonuses() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[0].maintenance.research = vec![research("AP", true)];
    game.catalog[tank].maintenance.weapon_upgrade = Some(WeaponUpgrade {
        trigger: "AP".into(),
    });
    game.catalog[tank].weapon.bonuses.template[5][0] = 1.25;
    game.catalog[tank].weapon.bonuses.global[10][0] = 1.2;
    register_upgrade(&mut game.catalog[tank], "WeaponBonusUpgrade", "AP");
    let owned = game.spawn(tank, 0, [450.0, 100.0], true);
    let enemy = game.spawn(tank, 1, [500.0, 100.0], true);
    game.set_veterancy(owned, 2);
    game.queue_upgrade(0, 0).unwrap();
    game.update_production(0);
    game.update_production(0);
    let future = game.spawn(tank, 0, [600.0, 100.0], true);
    assert!(
        (game.entities[owned]
            .weapon_damage_bonus(game.entities[owned].active_weapon(&game.catalog[tank]), 0)
            - 1.45)
            .abs()
            < 0.000_001
    );
    assert_eq!(
        game.entities[future]
            .weapon_damage_bonus(game.entities[future].active_weapon(&game.catalog[tank]), 0)
            .to_bits(),
        1.25_f32.to_bits()
    );
    assert_eq!(
        game.entities[enemy]
            .weapon_damage_bonus(game.entities[enemy].active_weapon(&game.catalog[tank]), 0)
            .to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn player_upgrade_flag_uses_each_weapon_bonus_and_preserves_launch_conditions() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].maintenance.weapon_upgrade = Some(WeaponUpgrade {
        trigger: "AP".into(),
    });
    game.catalog[tank].weapon.bonuses.global[5][0] = 1.1;
    game.catalog[tank].weapon.bonuses.template[5][0] = 1.25;
    game.catalog[tank].secondary_weapon = Some(crate::skirmish::WeaponRules {
        damage: 1.0,
        damage_type: crate::skirmish::DamageType::KillPilot,
        ..crate::skirmish::WeaponRules::unarmed()
    });
    register_upgrade(&mut game.catalog[tank], "WeaponBonusUpgrade", "AP");
    let actor = game.spawn(tank, 0, [450.0, 100.0], true);
    let before = game.entities[actor].damage_snapshot(0);
    game.players[0].upgrades.push("AP".into());
    game.activate_upgrade_modules(actor);
    let launch = game.entities[actor].damage_snapshot(0);
    assert_eq!(
        launch
            .factor(
                &game.catalog[tank].weapon,
                0,
                crate::skirmish::containment::bonuses::DAMAGE
            )
            .to_bits(),
        1.35_f32.to_bits()
    );
    game.entities[actor].weapon_set.current = 1;
    assert_eq!(
        game.entities[actor]
            .weapon_damage_bonus(game.entities[actor].active_weapon(&game.catalog[tank]), 0)
            .to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        before
            .factor(
                &game.catalog[tank].weapon,
                0,
                crate::skirmish::containment::bonuses::DAMAGE
            )
            .to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        launch
            .factor(
                &game.catalog[tank].weapon,
                0,
                crate::skirmish::containment::bonuses::DAMAGE
            )
            .to_bits(),
        1.35_f32.to_bits()
    );
}

#[test]
fn fortified_armor_is_used_for_actual_direct_and_area_hits() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[0].maintenance.armor_upgrade = Some(ArmorUpgrade {
        trigger: "Fortified".into(),
        coefficients: [
            0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0,
        ],
    });
    register_upgrade(&mut game.catalog[0], "ArmorUpgrade", "Fortified");
    game.reset_slots(game.slots.clone()).unwrap();
    let target = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && actor.type_index == 0)
        .unwrap();
    let attacker = game.spawn(tank, 1, [300.0, 100.0], true);
    let initial = game.entities[target].health;
    game.catalog[tank].weapon.damage = 20.0;
    game.players[0].upgrades.push("Fortified".into());
    game.activate_upgrade_modules(target);
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        Some(target),
        game.object_position(target),
        1.0,
    );
    assert_eq!(
        game.entities[target].health.to_bits(),
        (initial - 10.0).to_bits()
    );
    game.catalog[tank].weapon.area = Some(crate::skirmish::AreaDamage {
        at_source: false,
        primary_radius: 10.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 2,
    });
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        Some(target),
        game.object_position(target),
        1.0,
    );
    assert_eq!(
        game.entities[target].health.to_bits(),
        (initial - 20.0).to_bits()
    );
}

#[test]
fn researched_armor_replaces_every_special_damage_coefficient_for_real_hits() {
    use crate::skirmish::DamageType;
    for damage in [
        DamageType::Laser,
        DamageType::Poison,
        DamageType::HazardCleanup,
        DamageType::Crush,
        DamageType::Sniper,
        DamageType::Flame,
        DamageType::JetMissiles,
        DamageType::Radiation,
        DamageType::Microwave,
    ] {
        let mut game = game();
        let tank = game.type_for(0, Kind::Tank).unwrap();
        game.catalog[tank].weapon.damage_type = damage;
        game.catalog[tank].weapon.damage = 80.0;
        game.catalog[0].armor[damage as usize] = 0.5;
        let mut coefficients = [1.0; crate::skirmish::DAMAGE_TYPE_COUNT];
        coefficients[damage as usize] = 0.125;
        game.catalog[0].maintenance.armor_upgrade = Some(ArmorUpgrade {
            trigger: "Fortified".into(),
            coefficients,
        });
        register_upgrade(&mut game.catalog[0], "ArmorUpgrade", "Fortified");
        game.reset_slots(game.slots.clone()).unwrap();
        let target = game
            .entities
            .iter()
            .position(|actor| actor.owner == 0 && actor.type_index == 0)
            .unwrap();
        let attacker = game.spawn(tank, 1, [300.0, 100.0], true);
        let initial = game.entities[target].health;
        game.deal_bound_weapon_damage(
            attacker,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[attacker].type_index,
                weapon_index: 0,
                weapon_slot: game.entities[attacker].weapon_slot(),
            },
            Some(target),
            game.object_position(target),
            1.0,
        );
        assert_eq!(
            game.entities[target].health.to_bits(),
            (initial - 40.0).to_bits()
        );
        game.players[0].upgrades.push("Fortified".into());
        game.activate_upgrade_modules(target);
        game.deal_bound_weapon_damage(
            attacker,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[attacker].type_index,
                weapon_index: 0,
                weapon_slot: game.entities[attacker].weapon_slot(),
            },
            Some(target),
            game.object_position(target),
            1.0,
        );
        assert_eq!(
            game.entities[target].health.to_bits(),
            (initial - 50.0).to_bits()
        );
    }
}

#[test]
fn jet_missile_hits_use_separate_normal_and_researched_armor() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].weapon.damage = 100.0;
    game.catalog[tank].weapon.damage_type = crate::skirmish::DamageType::JetMissiles;
    game.catalog[0].armor[crate::skirmish::DamageType::JetMissiles as usize] = 0.6;
    game.catalog[0].maintenance.armor_upgrade = Some(ArmorUpgrade {
        trigger: "Fortified".into(),
        coefficients: [
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.25, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0, 1.0, 1.0, 1.0,
        ],
    });
    register_upgrade(&mut game.catalog[0], "ArmorUpgrade", "Fortified");
    game.reset_slots(game.slots.clone()).unwrap();
    let target = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && actor.type_index == 0)
        .unwrap();
    let attacker = game.spawn(tank, 1, [300.0, 100.0], true);
    let initial = game.entities[target].health;
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        Some(target),
        game.object_position(target),
        1.0,
    );
    assert_eq!(
        game.entities[target].health.to_bits(),
        (initial - 60.0).to_bits()
    );
    game.players[0].upgrades.push("Fortified".into());
    game.activate_upgrade_modules(target);
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        Some(target),
        game.object_position(target),
        1.0,
    );
    assert_eq!(
        game.entities[target].health.to_bits(),
        (initial - 85.0).to_bits()
    );
}

#[test]
fn actual_hit_applies_armor_before_battle_scalar_with_native_rounding() {
    use crate::skirmish::DamageType;

    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].weapon.damage_type = DamageType::ArmorPiercing;
    game.catalog[tank].weapon.damage = 13.7;
    game.catalog[0].armor[DamageType::ArmorPiercing as usize] = 0.33;
    game.entities[0].battle_bonuses.armor = 0.8;
    game.entities[0].health = 4.0;
    let attacker = game.spawn(tank, 1, [300.0, 100.0], true);
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        Some(0),
        game.object_position(0),
        1.0,
    );
    // Native separate float products yield 0x406779a7; multiplying the two
    // coefficients first yields 0x406779a8 and a different surviving health.
    let expected = 4.0 - f32::from_bits(0x4067_79a7);
    assert_eq!(game.entities[0].health.to_bits(), expected.to_bits());
}

#[test]
fn junk_repair_activation_damage_wake_and_dead_unit_rules_match_auto_heal() {
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank]
        .maintenance
        .auto_heal
        .push(crate::skirmish::tests::self_heal(2.0, 30, 0, &["Junk"]));
    crate::skirmish::tests::register_update(&mut game.catalog[tank], "AutoHealBehavior");
    game.catalog[tank]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers = vec!["Junk".into()];
    let unit = game.spawn(tank, 0, [450.0, 100.0], true);
    game.entities[unit].health = 50.0;
    game.players[0].upgrades.push("Junk".into());
    game.activate_upgrade_modules(unit);
    game.run_object_updates();
    assert_eq!(game.entities[unit].health.to_bits(), 50.0_f32.to_bits());
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[unit].health.to_bits(), 52.0_f32.to_bits());
    game.tick += 10;
    game.apply_hit(unit, 2, 1.0);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[unit].health.to_bits(), 51.0_f32.to_bits());
    game.tick += 19;
    game.run_object_updates();
    assert_eq!(game.entities[unit].health.to_bits(), 53.0_f32.to_bits());
    game.entities[unit].health = game.entities[unit].max_health;
    game.tick += 30;
    game.run_object_updates();
    // Native friend_setNextCallFrame clamps the absolute 30-bit wake frame;
    // getWakeFrame then reports that deadline minus the current frame.
    assert_eq!(
        game.update_remaining_sleep(
            unit,
            game.update_objects[game.entities[unit].update_owner]
                .handles
                .len()
                - 1
        ) + game.update_frame(),
        crate::skirmish::module_updates::FOREVER
    );
    game.tick += 1;
    game.apply_hit(unit, 2, 5.0);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(
        game.entities[unit].health.to_bits(),
        (game.entities[unit].max_health - 3.0).to_bits()
    );
    game.entities[unit].health = 0.0;
    game.entities[unit].set_effectively_dead(true);
    game.tick += 30;
    game.run_object_updates();
    assert_eq!(game.entities[unit].health.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn worker_shoes_select_the_entire_locomotor_and_supply_boost_uses_current_owner() {
    let mut game = game();
    let worker = game.type_for(0, Kind::Builder).unwrap();
    let mut shoes =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Builder)
            .locomotor;
    shoes.speed_damaged = 20.0;
    shoes.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 150.0 / 900.0,
        acceleration_damaged: 75.0 / 900.0,
        braking: 150.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    game.catalog[worker].maintenance.movement_upgrade = Some(MovementUpgrade {
        trigger: "Shoes".into(),
        speed: 30.0,
        locomotor: shoes,
    });
    game.catalog[worker].maintenance.collector_boost = Some(crate::skirmish::SupplyBoost {
        trigger: "Shoes".into(),
        amount: 8,
    });
    register_upgrade(&mut game.catalog[worker], "LocomotorSetUpgrade", "Shoes");
    let id = game.spawn(worker, 0, [450.0, 100.0], true);
    game.entities[id].facing = 0.0;
    game.players[0].upgrades.push("Shoes".into());
    game.activate_upgrade_modules(id);
    let unit = &game.catalog[worker];
    let actor = &mut game.entities[id];
    assert_eq!(
        crate::skirmish::locomotor::speed(actor, unit).to_bits(),
        30.0_f32.to_bits()
    );
    let mass = actor.active_locomotor(unit).ground.as_ref().unwrap().mass;
    crate::skirmish::locomotor::move_ground(
        actor,
        unit,
        &GroundGoal {
            height: 0.0,
            position: [900.0, 100.0],
            distance: 450.0,
            desired_speed: 1.0,
            no_slow_down: false,
            ultra_accurate: false,
        },
        mass,
        1,
        &crate::skirmish::physical_ocl::transform::TerrainContext {
            terrain: game.projectile_terrain.as_ref(),
            layers: &game.movement_terrain,
            height: 0.0,
        },
        |_| true,
    );
    crate::skirmish::locomotor::integrate_ground(actor, unit, 1);
    assert!((actor.ai.motion.velocity[0] - 150.0 / 900.0).abs() < 0.000_001);
    assert_eq!(game.upgraded_collector_boost(id), 8);
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(game.upgraded_collector_boost(id), 0);
    // A fired LocomotorSetUpgrade remains executed after capture.
    assert_eq!(
        crate::skirmish::locomotor::speed(&game.entities[id], &game.catalog[worker]).to_bits(),
        30.0_f32.to_bits()
    );
}

#[test]
fn advanced_training_applies_once_to_existing_and_future_units_and_not_general_experience() {
    use crate::skirmish::{ExperienceUpgrade, FactionProgression, ProgressionRules, Rank};
    let mut game = game();
    game.configure_progression(ProgressionRules {
        multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
        ranks: vec![
            Rank {
                skill_points: 0,
                science_points: 1,
                sciences: Vec::new(),
            },
            Rank {
                skill_points: 100,
                science_points: 1,
                sciences: Vec::new(),
            },
        ],
        sciences: Vec::new(),
        factions: vec![FactionProgression {
            side: "GLA".into(),
            preferred_color: 0xff00_ff00,
            playable_side: true,
            faction: game.players[0].faction,
            intrinsic_points: 0,
            intrinsic_sciences: Vec::new(),
        }],
        powers: Vec::new(),
    })
    .unwrap();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].veterancy.experience_value = [11; 4];
    game.catalog[tank].progression.skill_point_value = [7; 4];
    game.catalog[tank].veterancy.trainable = true;
    game.catalog[tank].veterancy.required = [0, 100, 200, 300];
    game.catalog[tank]
        .maintenance
        .experience_upgrades
        .push(ExperienceUpgrade {
            trigger: "AdvancedTraining".into(),
            scalar: 1.0,
        });
    register_upgrade(
        &mut game.catalog[tank],
        "ExperienceScalarUpgrade",
        "AdvancedTraining",
    );
    let unit = game.spawn(tank, 0, [450.0, 100.0], true);
    game.add_unit_experience(unit, 10);
    assert_eq!(game.entities[unit].experience, 10);
    let general_experience = game.players[0].skill_points;
    game.players[0].upgrades.push("AdvancedTraining".into());
    game.activate_upgrade_modules(unit);
    game.activate_upgrade_modules(unit);
    game.add_unit_experience(unit, 11);
    assert_eq!(game.entities[unit].experience, 32);
    let future = game.spawn(tank, 0, [500.0, 100.0], true);
    game.add_unit_experience(future, 11);
    assert_eq!(game.entities[future].experience, 22);
    assert_eq!(game.players[0].skill_points, general_experience);
    let victim = game.spawn(tank, 1, [550.0, 100.0], true);
    game.apply_hit(victim, unit, 10_000.0);
    assert_eq!(game.entities[unit].experience, 54);
    assert_eq!(game.players[0].skill_points, general_experience + 7);
    game.entities[unit].owner = 1;
    game.activate_upgrade_modules(unit);
    game.add_unit_experience(unit, 10);
    assert_eq!(game.entities[unit].experience, 74);
}

#[test]
fn composite_armor_keeps_both_source_health_modules_and_does_not_reapply_on_capture() {
    use crate::skirmish::HealthUpgrade;
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    for _ in 0..2 {
        game.catalog[tank]
            .maintenance
            .health_upgrades
            .push(HealthUpgrade {
                change_type: crate::skirmish::MaxHealthChange::AddCurrentHealthToo,
                trigger: "Composite".into(),
                amount: 100.0,
            });
        register_upgrade(&mut game.catalog[tank], "MaxHealthUpgrade", "Composite");
    }
    let unit = game.spawn(tank, 0, [450.0, 100.0], true);
    let maximum = game.entities[unit].max_health;
    game.entities[unit].health = maximum - 35.0;
    game.players[0].upgrades.push("Composite".into());
    game.activate_upgrade_modules(unit);
    assert_eq!(
        game.entities[unit].max_health.to_bits(),
        (maximum + 200.0).to_bits()
    );
    assert_eq!(
        game.entities[unit].health.to_bits(),
        (maximum + 165.0).to_bits()
    );
    game.activate_upgrade_modules(unit);
    game.entities[unit].owner = 1;
    game.activate_upgrade_modules(unit);
    assert_eq!(
        game.entities[unit].max_health.to_bits(),
        (maximum + 200.0).to_bits()
    );
    let future = game.spawn(tank, 0, [500.0, 100.0], true);
    assert_eq!(
        game.entities[future].max_health.to_bits(),
        (maximum + 200.0).to_bits()
    );
    assert_eq!(
        game.entities[future].health.to_bits(),
        (maximum + 200.0).to_bits()
    );
}

#[test]
fn advanced_training_truncates_the_scaled_real_gain_before_accumulating_experience() {
    use crate::skirmish::ExperienceUpgrade;
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[tank].veterancy.trainable = true;
    game.catalog[tank].veterancy.required = [0, 100, 200, 300];
    game.catalog[tank]
        .maintenance
        .experience_upgrades
        .push(ExperienceUpgrade {
            trigger: "Fractional".into(),
            scalar: 0.5,
        });
    game.players[0].upgrades.push("Fractional".into());
    register_upgrade(
        &mut game.catalog[tank],
        "ExperienceScalarUpgrade",
        "Fractional",
    );
    let unit = game.spawn(tank, 0, [450.0, 100.0], true);
    game.add_unit_experience(unit, 3);
    game.add_unit_experience(unit, 3);
    assert_eq!(game.entities[unit].experience, 8);
}

#[test]
fn chemical_suits_apply_original_armor_and_decal_to_existing_future_and_captured_infantry() {
    use crate::skirmish::DamageType;
    let mut game = game();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[0].maintenance.research = vec![research("Upgrade_AmericaChemicalSuits", true)];
    let mut coefficients = game.catalog[infantry].armor;
    coefficients[DamageType::Poison as usize] = 0.1;
    game.catalog[infantry].maintenance.armor_upgrade = Some(ArmorUpgrade {
        trigger: "Upgrade_AmericaChemicalSuits".into(),
        coefficients,
    });
    register_upgrade(
        &mut game.catalog[infantry],
        "ArmorUpgrade",
        "Upgrade_AmericaChemicalSuits",
    );
    let existing = game.spawn(infantry, 0, [450.0, 100.0], true);
    let enemy = game.spawn(infantry, 1, [600.0, 100.0], true);
    game.queue_upgrade(0, 0).unwrap();
    game.update_production(0);
    assert!(!game.entities[existing].chemical_suit());
    game.update_production(0);
    let future = game.spawn(infantry, 0, [500.0, 100.0], true);
    for id in [existing, future] {
        assert!(game.entities[id].chemical_suit());
        assert_eq!(
            game.entities[id]
                .adjust_damage(&game.catalog[infantry], DamageType::Poison, 50.0)
                .to_bits(),
            5.0_f32.to_bits()
        );
    }
    assert!(!game.entities[enemy].chemical_suit());
    game.transfer_maintenance_owner(existing, 1);
    game.activate_upgrade_modules(existing);
    assert!(game.entities[existing].chemical_suit());
    assert_eq!(
        game.entities[existing]
            .adjust_damage(&game.catalog[infantry], DamageType::Poison, 50.0)
            .to_bits(),
        5.0_f32.to_bits()
    );
}

fn health_registration(
    trigger: &str,
    removes: &[&str],
) -> crate::skirmish::module_updates::BehaviorRegistration {
    crate::skirmish::module_updates::BehaviorRegistration {
        family: "MaxHealthUpgrade".into(),
        tag: "HealthUpgrade".into(),
        interfaces: 0x80,
        upgrade: crate::skirmish::module_updates::UpgradeData {
            triggers: vec![trigger.into()],
            removals: removes.iter().map(|name| (*name).into()).collect(),
            ..crate::skirmish::module_updates::UpgradeData::none()
        },
        source: crate::skirmish::module_updates::ModuleSource {
            path: "test.ini".into(),
            origin: "upgrade reset regression".into(),
            line: 1,
        },
    }
}

#[test]
fn object_upgrade_removal_preserves_health_effect_and_allows_actual_reexecution() {
    let mut game = game();
    game.catalog[5]
        .maintenance
        .health_upgrades
        .push(super::HealthUpgrade {
            change_type: crate::skirmish::MaxHealthChange::AddCurrentHealthToo,
            trigger: "Health".into(),
            amount: 25.0,
        });
    game.catalog[5]
        .behavior_modules
        .push(health_registration("Health", &["Health"]));
    crate::skirmish::module_updates::validate_registrations(&game.catalog[5]).unwrap();
    let id = game.spawn(5, 0, [300.0, 300.0], true);
    game.entities[id].maintenance.upgrades.push("Health".into());
    let initial = game.entities[id].max_health;
    game.activate_upgrade_modules(id);
    assert_eq!(
        game.entities[id].max_health.to_bits(),
        (initial + 25.0).to_bits()
    );
    game.remove_native_object_upgrade(id, "Health");
    assert!(!game.object_has_upgrade(id, "Health"));
    assert_eq!(
        game.entities[id].max_health.to_bits(),
        (initial + 25.0).to_bits()
    );
    game.activate_upgrade_modules(id);
    assert_eq!(
        game.entities[id].max_health.to_bits(),
        (initial + 25.0).to_bits()
    );
    game.entities[id].maintenance.upgrades.push("Health".into());
    game.activate_upgrade_modules(id);
    assert_eq!(
        game.entities[id].max_health.to_bits(),
        (initial + 50.0).to_bits()
    );
}

#[test]
fn admission_rejects_unbound_upgrade_independently_of_removal_masks() {
    let mut unit =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::China, Kind::Vehicle);
    let mut module = health_registration("Armor", &["Other"]);
    module.family = "ArmorUpgrade".into();
    unit.behavior_modules.push(module);
    let error = crate::skirmish::maintenance::validate_upgrade_resets(&unit).unwrap_err();
    assert!(error.contains("ArmorUpgrade HealthUpgrade at test.ini:1"));
    unit.behavior_modules.last_mut().unwrap().upgrade.removals = vec!["Armor".into()];
    let error = crate::skirmish::maintenance::validate_upgrade_resets(&unit).unwrap_err();
    assert!(error.contains("ArmorUpgrade HealthUpgrade at test.ini:1"));
}

fn health_research_template(player: bool) -> crate::skirmish::UnitType {
    let mut unit =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::China, Kind::Vehicle);
    unit.maintenance.research.push(research("Health", player));
    unit.maintenance.health_upgrades.push(super::HealthUpgrade {
        trigger: "Health".into(),
        amount: 25.0,
        change_type: super::MaxHealthChange::AddCurrentHealthToo,
    });
    let mut module = health_registration("Health", &[]);
    module.upgrade.fx = "FX_ResearchOrder".into();
    unit.behavior_modules.push(module);
    unit
}

#[test]
fn player_research_visits_team_membership_order_including_dead_existing_objects() {
    let mut game = game();
    let kind = game.catalog.len();
    game.catalog.push(health_research_template(true));
    let first = game.spawn(kind, 0, [300.0, 300.0], true);
    let dead = game.spawn(kind, 0, [350.0, 300.0], true);
    let third = game.spawn(kind, 0, [400.0, 300.0], true);
    let prototype = game.team_membership.add_prototype(0, "ResearchTeam".into());
    let older_team = game.team_membership.create_team(prototype);
    let newer_team = game.team_membership.create_team(prototype);
    game.team_membership.set_team(first, Some(older_team));
    game.team_membership.set_team(third, Some(older_team));
    game.team_membership.set_team(dead, Some(newer_team));
    game.change_body_health(dead, -100.0);
    assert!(!game.entities[dead].alive());
    assert!(game.entities[dead].exists());
    let begin = game.presentation.object_fx_events.len();
    game.finish_research(first, 0);
    let callbacks: Vec<_> = game.presentation.object_fx_events[begin..]
        .iter()
        .filter(|event| event.effect == "FX_ResearchOrder")
        .map(|event| event.object)
        .collect();
    assert_eq!(callbacks, [dead, third, first]);
    assert_eq!(game.entities[dead].health.to_bits(), 25.0_f32.to_bits());
    for id in [first, dead, third] {
        assert_eq!(game.entities[id].max_health.to_bits(), 125.0_f32.to_bits());
    }
}

#[test]
fn object_research_does_not_reexecute_another_objects_reset_player_upgrade() {
    let mut game = game();
    let kind = game.catalog.len();
    game.catalog.push(health_research_template(false));
    let producer = game.spawn(kind, 0, [300.0, 300.0], true);
    let other = game.spawn(kind, 0, [350.0, 300.0], true);
    game.players[0].upgrades.push("Health".into());
    game.activate_upgrade_modules(other);
    game.remove_native_object_upgrade(other, "Health");
    assert!(!game.native_upgrade_executed(other, "MaxHealthUpgrade", 0));
    assert!(game.has_upgrade(0, "Health"));
    let before = game.entities[other].max_health;
    let begin = game.presentation.object_fx_events.len();
    game.finish_research(producer, 0);
    let callbacks: Vec<_> = game.presentation.object_fx_events[begin..]
        .iter()
        .filter(|event| event.effect == "FX_ResearchOrder")
        .map(|event| event.object)
        .collect();
    assert_eq!(callbacks, [producer]);
    assert_eq!(game.entities[other].max_health.to_bits(), before.to_bits());
    assert!(!game.native_upgrade_executed(other, "MaxHealthUpgrade", 0));
}

#[test]
fn health_upgrade_accepts_zero_healing_and_negative_finite_maximum_changes() {
    use super::MaxHealthChange;

    for (amount, change_type, expected) in [
        (0.0, MaxHealthChange::FullyHeal, 100.0_f32),
        (-20.0, MaxHealthChange::PreserveRatio, 40.0),
        (-20.0, MaxHealthChange::AddCurrentHealthToo, 30.0),
        (-60.0, MaxHealthChange::SameCurrentHealth, 40.0),
        (-20.0, MaxHealthChange::FullyHeal, 80.0),
    ] {
        let mut game = game();
        let mut unit = health_research_template(false);
        unit.maintenance.health_upgrades[0].amount = amount;
        unit.maintenance.health_upgrades[0].change_type = change_type;
        super::validate(&unit).unwrap();
        crate::skirmish::module_updates::validate_registrations(&unit).unwrap();
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [300.0, 300.0], true);
        game.change_body_health(id, -50.0);
        game.grant_native_object_upgrade(id, "Health");
        assert_eq!(game.entities[id].health.to_bits(), expected.to_bits());
        assert_eq!(
            game.entities[id].max_health.to_bits(),
            (100.0 + amount).to_bits()
        );
        assert_eq!(
            game.entities[id].damage_state.initial_health.to_bits(),
            (100.0 + amount).to_bits()
        );
    }
}

#[test]
fn tank_locomotor_upgrade_honors_masks_roles_damage_and_capture_persistence() {
    use crate::skirmish::locomotor::{self, set};
    use crate::skirmish::{Faction, LocomotorVariant, tests::fixture_type};
    let mut game = game();
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let mut upgraded = fixture_type(Faction::China, Kind::Tank).locomotor;
    upgraded.speed_damaged = 32.0;
    game.catalog[tank].maintenance.movement_upgrade = Some(MovementUpgrade {
        trigger: "Upgrade_ChinaNuclearTanks".into(),
        speed: 35.0,
        locomotor: upgraded,
    });
    assert_eq!(game.catalog[tank].supply.capacity, 0);
    super::validate(&game.catalog[tank]).unwrap();
    register_upgrade(
        &mut game.catalog[tank],
        "LocomotorSetUpgrade",
        "Upgrade_ChinaNuclearTanks",
    );
    let mask = &mut game.catalog[tank]
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade;
    mask.triggers.push("Upgrade_Ready".into());
    mask.requires_all = true;
    mask.conflicts.push("Upgrade_Blocked".into());
    game.catalog[tank]
        .locomotor_sets
        .wander
        .push(LocomotorVariant {
            speed: 10.0,
            locomotor: fixture_type(Faction::China, Kind::Tank).locomotor,
        });
    let id = game.spawn(tank, 0, [450.0, 100.0], true);
    let ordinary = locomotor::speed(&game.entities[id], &game.catalog[tank]);
    game.players[0]
        .upgrades
        .push("Upgrade_ChinaNuclearTanks".into());
    game.activate_upgrade_modules(id);
    assert_eq!(game.entities[id].ai.locomotor_set, set::NORMAL);
    assert_eq!(
        locomotor::speed(&game.entities[id], &game.catalog[tank]).to_bits(),
        ordinary.to_bits()
    );
    game.players[0]
        .upgrades
        .extend(["Upgrade_Ready".into(), "Upgrade_Blocked".into()]);
    game.activate_upgrade_modules(id);
    assert!(!game.native_upgrade_executed(id, "LocomotorSetUpgrade", 0));
    game.choose_locomotor_set(id, set::WANDER);
    game.players[0]
        .upgrades
        .retain(|name| name != "Upgrade_Blocked");
    game.activate_upgrade_modules(id);
    assert!(game.native_upgrade_executed(id, "LocomotorSetUpgrade", 0));
    assert_eq!(game.entities[id].ai.locomotor_set, set::WANDER);
    assert_eq!(
        locomotor::speed(&game.entities[id], &game.catalog[tank]).to_bits(),
        10.0_f32.to_bits()
    );
    game.choose_locomotor_set(id, set::NORMAL);
    assert_eq!(game.entities[id].ai.locomotor_set, set::UPGRADED);
    assert_eq!(
        locomotor::speed(&game.entities[id], &game.catalog[tank]).to_bits(),
        35.0_f32.to_bits()
    );
    game.entities[id].health = 1.0;
    assert_eq!(
        locomotor::speed(&game.entities[id], &game.catalog[tank]).to_bits(),
        32.0_f32.to_bits()
    );
    assert_eq!(game.upgraded_collector_boost(id), 0);
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(game.entities[id].ai.locomotor_set, set::UPGRADED);
    assert!(game.native_upgrade_executed(id, "LocomotorSetUpgrade", 0));
    assert_eq!(
        locomotor::speed(&game.entities[id], &game.catalog[tank]).to_bits(),
        32.0_f32.to_bits()
    );
}
