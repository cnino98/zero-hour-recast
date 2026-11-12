use super::*;
use crate::skirmish::{
    AreaDamage, ClipReload, ConditionalWeapon, DamageType, DeathType, Faction, GroundAppearance,
    GroundRules, Kind, LaunchTransform, WeaponMotion, WeaponRules, WeaponTemplateSet,
    tests::{fixture_type, game},
};

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    let driver = &mut game.catalog[5];
    driver.behavior.car_bomb_converter = Some(Rules {
        effect: "FX_MakeCarBombSuccess".into(),
        required: vec!["VEHICLE".into()],
        forbidden: Vec::new(),
    });
    crate::skirmish::tests::register_behavior(driver, "ConvertToCarBombCrateCollide", 0x10);
    driver.locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 5.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    driver.sight.clearing_range = 200.0;
    let mut car = fixture_type(Faction::Civilian, Kind::Vehicle);
    car.name = "CarCamaro".into();
    car.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    car.ai.vision_range = 0.0;
    car.sight.clearing_range = 0.0;
    car.veterancy.required = [0, 100, 200, 400];
    car.weapon_set_rules.templates.push(ConditionalWeapon {
        flags: 8,
        slot: 0,
        rules: WeaponRules {
            name: "SuicideCarBomb".into(),
            damage: 700.0,
            range: 5.0,
            speed: 99_999.0,
            damage_type: DamageType::Explosion,
            death_type: DeathType::Suicided,
            clip_size: 1,
            reload: ClipReload::None,
            area: Some(AreaDamage {
                at_source: true,
                primary_radius: 20.0,
                secondary_radius: 50.0,
                secondary_damage: 100.0,
                affects: 63,
            }),
            motion: Some(WeaponMotion {
                turret_mounts: None,
                launches: std::array::from_fn(|_| vec![LaunchTransform::IDENTITY]),
                damage_thresholds: [0.5, 0.1],
                shots_per_barrel: 1,
                projectile_type: None,
                collide_mask: 0,
                shell_speed_scale: None,
            }),
            ..WeaponRules::unarmed()
        },
    });
    car.weapon_set_rules.replacements = vec![
        WeaponTemplateSet {
            flags: 0,
            weapons: [Some(0), None, None],
            share_lock: false,
        },
        WeaponTemplateSet {
            flags: 8,
            weapons: [Some(7), None, None],
            share_lock: false,
        },
    ];
    let car_type = game.catalog.len();
    game.catalog.push(car);
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    let car = game.spawn(car_type, game.neutral_owner(), [360.0, 300.0], true);
    game.civilian_objects.insert(car);
    game.update_visibility();
    (game, actor, car)
}

#[test]
fn entering_a_civilian_car_transfers_control_name_sight_and_rank_without_a_death() {
    let (mut game, driver, car) = fixture();
    game.map_objects.insert("Driver".into(), driver);
    game.entities[driver].veterancy = 2;
    let losses = game.players[0].statistics.units_lost;
    game.make_car_bomb(driver, car).unwrap();
    for _ in 0..300 {
        game.step();
        if game.entities[car].car_bomb() {
            break;
        }
    }
    assert!(game.entities[car].car_bomb());
    assert_eq!(game.entities[car].owner, 0);
    assert!(!game.entities[driver].alive());
    assert!(game.entities[driver].masked());
    assert_eq!(game.players[0].statistics.units_lost, losses);
    assert_eq!(game.map_objects["Driver"], car);
    assert_eq!(game.entities[car].weapon_index(0), 7);
    assert_eq!(game.entities[car].veterancy, 2);
    assert_eq!(game.entities[car].experience, 200);
    assert_eq!(game.presentation.car_bomb_events.len(), 1);
    assert_eq!(game.presentation.car_bomb_events[0].source, driver);
    assert!(game.presentation.shots.is_empty());
    let unit = &game.catalog[game.entities[car].type_index];
    assert_eq!(
        game.entities[car].vision_range(unit).to_bits(),
        150.0_f32.to_bits()
    );
    assert_eq!(
        game.entities[car].shroud_range(unit).to_bits(),
        200.0_f32.to_bits()
    );
}

#[test]
fn contact_requires_the_explicit_goal_and_rejects_neutral_aircraft_and_repeat_pickup() {
    let (mut game, driver, car) = fixture();
    assert!(!game.car_bomb_contact(driver, car));
    game.civilian_objects.remove(&car);
    assert!(!game.can_make_car_bomb(driver, car));
    game.civilian_objects.insert(car);
    let car_type = game.entities[car].type_index;
    game.catalog[car_type].kind_of.push("AIRCRAFT".into());
    assert!(!game.can_make_car_bomb(driver, car));
    game.catalog[car_type].kind_of.pop();
    assert!(!game.can_make_car_bomb(driver, usize::MAX));
    game.make_car_bomb(driver, car).unwrap();
    assert!(game.car_bomb_contact(driver, car));
    let second = game.spawn(5, 0, [350.0, 300.0], true);
    assert!(!game.can_make_car_bomb(second, car));
    assert!(!game.car_bomb_contact(driver, car));
    assert_eq!(game.presentation.car_bomb_events.len(), 1);
    assert!(game.entities[second].alive());
}

#[test]
fn converted_car_detonates_at_its_position_without_aiming_and_damages_both_rings() {
    let (mut game, driver, car) = fixture();
    game.make_car_bomb(driver, car).unwrap();
    assert!(game.car_bomb_contact(driver, car));
    let target = game.spawn(6, 1, [372.0, 300.0], true);
    let inner = game.spawn(1, 0, [380.0, 300.0], true);
    let outer = game.spawn(6, 1, [409.0, 300.0], true);
    for id in [target, inner, outer] {
        game.entities[id].health = 2_000.0;
    }
    game.set_object_orientation(car, std::f32::consts::PI);
    game.update_ground_contacts();
    game.attack(car, target).unwrap();
    assert!(game.entities[car].attacking());
    assert!(!game.update_attack(car, target));
    assert!(!game.entities[car].alive());
    assert_eq!(
        game.entities[target].health.to_bits(),
        1_300.0_f32.to_bits()
    );
    assert_eq!(game.entities[inner].health.to_bits(), 1_300.0_f32.to_bits());
    assert_eq!(game.entities[outer].health.to_bits(), 1_900.0_f32.to_bits());
    assert!(matches!(
        game.entities[car].damage_state.death_type,
        DeathType::Suicided
    ));
    assert_eq!(game.presentation.shots.len(), 1);
}

#[test]
fn conversion_precedes_crushing_and_destroyed_driver_cannot_convert_twice() {
    let (mut game, driver, car) = fixture();
    game.entities[driver].position = [361.0, 300.0];
    game.catalog[5].crush.squish_module = Some(2);
    crate::skirmish::tests::register_behavior(&mut game.catalog[5], "SquishCollide", 0x10);
    let car_type = game.entities[car].type_index;
    game.catalog[car_type].crush.crusher_level = 1;
    game.entities[car].ai.motion.velocity = [1.0, 0.0];
    game.make_car_bomb(driver, car).unwrap();
    game.object_contact(driver, car, true);
    assert!(game.entities[car].car_bomb());
    assert!(matches!(
        game.entities[driver].damage_state.death_type,
        DeathType::Normal
    ));
    game.object_contact(driver, car, true);
    assert_eq!(game.presentation.car_bomb_events.len(), 1);
}

#[test]
fn authored_car_scrap_without_conditional_carbomb_set_rejects_driver() {
    let (mut game, driver, car) = fixture();
    let car_type = game.entities[car].type_index;
    let unit = &mut game.catalog[car_type];
    unit.name = "CarScrap".into();
    unit.weapon_set_rules.replacements.clear();
    assert!(!game.can_make_car_bomb(driver, car));
    assert!(game.make_car_bomb(driver, car).is_err());
    assert!(game.entities[driver].alive());
    assert!(!game.entities[car].car_bomb());
    assert!(game.presentation.object_fx_events.is_empty());
}
