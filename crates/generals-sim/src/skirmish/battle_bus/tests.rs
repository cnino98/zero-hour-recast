use super::*;
use crate::skirmish::{
    containment::tests::setup,
    death_callbacks::Conditions,
    death_lifecycle::{Rules as LifecycleRules, SlowDeath},
    tests::register_update,
};

fn rules() -> Rules {
    Rules {
        second_health: 650.0,
        first_armor: [[0.5; DAMAGE_TYPE_COUNT]; 2],
        second_armor: [[0.2; DAMAGE_TYPE_COUNT]; 3],
    }
}

pub(in crate::skirmish) fn bind_death(unit: &mut UnitType) {
    register_update(unit, "BattleBusSlowDeathBehavior");
    let module = unit.behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    unit.death_lifecycle = Some(LifecycleRules {
        on_die: Vec::new(),
        weapons: Vec::new(),
        physics: None,
        slow: vec![SlowDeath {
            derived: Derived::BattleBus(Box::new(DeathRules {
                throw_force: 100.0,
                passenger_damage: 0.5,
                empty_delay: 30,
                start_fx: "BusStart".into(),
                ground_fx: "BusGround".into(),
                start_ocl: None,
                ground_ocl: None,
            })),
            tag: module.tag.clone(),
            conditions: Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
            probability: 5,
            overkill_bonus: 0.0,
            sink_delay: [0, 0],
            destruction_delay: [30, 0],
            sink_rate: 0.0,
            fling: None,
            fx: [vec!["RealStart".into()], Vec::new(), Vec::new()],
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        }],
    });
}

fn bus() -> (Skirmish, usize) {
    let (mut game, old) = setup(0.0, 1.0);
    let index = game.entities[old].type_index;
    let unit = &mut game.catalog[index];
    unit.battle_bus = Some(rules());
    unit.initial_health = 400.0;
    unit.max_health = 400.0;
    bind_death(unit);
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    let id = game.spawn(index, 0, [355.0, 255.0], true);
    (game, id)
}

#[test]
fn damage_estimate_defers_first_death_until_armored_damage_is_lethal() {
    let (mut game, id) = bus();
    let index = game.entities[id].type_index;
    game.catalog[index].armor[DamageType::ArmorPiercing as usize] = 0.5;
    game.apply_weapon_hit(id, 0, 600.0, DamageType::ArmorPiercing, DeathType::Normal);
    assert_eq!(game.entities[id].health.to_bits(), 100.0_f32.to_bits());
    assert!(!game.entities[id].battle_bus_second_life());
    game.apply_weapon_hit(id, 0, 200.0, DamageType::ArmorPiercing, DeathType::Normal);
    assert_eq!(game.entities[id].health.to_bits(), 650.0_f32.to_bits());
    assert_eq!(
        game.entities[id].damage_state.initial_health.to_bits(),
        650.0_f32.to_bits()
    );
    assert!(!game.entities[id].is_ai_dead());
    assert!(game.battle_bus_death_state(id, 0).first_death);
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "BusStart"
    );
}

#[test]
fn fake_landing_uses_strict_ten_frame_delay_and_empty_penalty_is_strict() {
    let (mut game, id) = bus();
    game.apply_weapon_hit(id, 0, 500.0, DamageType::ArmorPiercing, DeathType::Normal);
    game.tick = 10;
    game.run_object_updates();
    assert!(!game.entities[id].battle_bus_grounded());
    game.tick = 11;
    game.run_object_updates();
    assert!(game.entities[id].battle_bus_grounded());
    assert!(game.entities[id].disabled_held());
    game.tick = 12;
    game.run_object_updates();
    assert_eq!(game.battle_bus_death_state(id, 0).penalty, 42);
    game.tick = 42;
    game.run_object_updates();
    assert!(game.entities[id].alive());
    game.tick = 57;
    game.run_object_updates();
    assert!(!game.entities[id].alive());
    assert!(matches!(
        game.entities[id].damage_state.death_type,
        DeathType::Extra4
    ));
    assert!(game.battle_bus_death_state(id, 0).real_death);
    assert!(!game.battle_bus_death_state(id, 0).first_death);
}

#[test]
fn refill_cancels_penalty_and_real_death_cancels_first_landing() {
    let (mut game, id) = bus();
    game.apply_weapon_hit(id, 0, 500.0, DamageType::ArmorPiercing, DeathType::Normal);
    game.tick = 11;
    game.run_object_updates();
    game.tick = 12;
    game.run_object_updates();
    let rider = game.spawn(5, 0, game.entities[id].position, true);
    game.board(rider, id);
    assert_eq!(game.entities[rider].contained_by(), Some(id));
    game.tick = 27;
    game.run_object_updates();
    assert_eq!(game.battle_bus_death_state(id, 0).penalty, 0);
    game.kill_object(id, DeathType::Crushed);
    assert!(game.battle_bus_death_state(id, 0).real_death);
    assert!(!game.battle_bus_death_state(id, 0).first_death);
    assert!(game.entities[id].death_lifecycle.as_ref().unwrap().native[0].active());
}

#[test]
fn constructor_does_not_run_empty_timer_and_forced_first_update_does() {
    use crate::skirmish::module_updates::{FOREVER, ModuleRegistration};
    let (mut game, id) = bus();
    let registration=game.catalog[game.entities[id].type_index].update_modules.iter().position(|module| matches!(module,ModuleRegistration::Behavior{family,..} if family=="BattleBusSlowDeathBehavior")).unwrap();
    assert_eq!(
        game.update_remaining_sleep(id, registration) + game.update_frame(),
        FOREVER
    );
    game.tick = 1000;
    game.run_object_updates();
    assert_eq!(game.battle_bus_death_state(id, 0).penalty, 0);
    game.wake_family_update(id, "BattleBusSlowDeathBehavior", 0, 1);
    game.tick = 1001;
    game.run_object_updates();
    assert_eq!(game.battle_bus_death_state(id, 0).penalty, 1031);
}

#[test]
fn raw_environment_damage_runs_undead_wrapper_even_when_active_body_rejects_damage() {
    let (mut game, id) = bus();
    let health = game.entities[id].health;
    game.change_body_health(id, -health);
    game.attempt_health_damage(id, None, 999_999.0, DamageType::Water, DeathType::Flooded);
    assert!(game.entities[id].battle_bus_second_life());
    assert_eq!(game.entities[id].health.to_bits(), 650.0_f32.to_bits());
    assert!(!game.entities[id].is_ai_dead());
}

#[test]
fn passenger_damage_detaches_list_and_removes_each_dead_rider_once() {
    let (mut game, id) = bus();
    let index = game.entities[id].type_index;
    game.catalog[index].contain.as_mut().unwrap().death.burned = true;
    let first = game.spawn(5, 0, game.entities[id].position, true);
    let second = game.spawn(5, 0, game.entities[id].position, true);
    game.board(first, id);
    game.board(second, id);
    game.entities[first].health = 10.0;
    let maximum = game.entities[second].max_health;
    game.process_damage_to_contained(id, 0.5);
    assert_eq!(game.passengers(id), &[second]);
    assert_eq!(game.entities[first].contained_by(), None);
    assert!(matches!(
        game.entities[first].damage_state.death_type,
        DeathType::Burned
    ));
    assert_eq!(
        game.entities[second].health.to_bits(),
        (maximum * 0.5).to_bits()
    );
    game.process_damage_to_contained(id, 1.0);
    assert!(game.passengers(id).is_empty());
    assert_eq!(game.entities[second].contained_by(), None);
}

#[test]
fn held_hulk_attack_fires_passenger_without_starting_ground_chase() {
    let (mut game, id) = bus();
    let index = game.entities[id].type_index;
    // BattleBusLocomotor: FOUR_WHEELS, acceleration/braking 1000 per second².
    game.catalog[index].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Wheels {
            min_turn_speed: 0.0,
            can_move_backwards: false,
        },
        is_dozer: false,
        downhill_only: false,
        acceleration: 1000.0 / 900.0,
        acceleration_damaged: 1000.0 / 900.0,
        braking: 1000.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    game.catalog[index].speed = 70.0;
    game.catalog[index].locomotor.speed_damaged = 50.0;
    game.catalog[5].weapon.range = 100.0;
    let position = game.entities[id].position;
    let rider = game.spawn(5, 0, position, true);
    game.board(rider, id);
    assert_eq!(game.entities[rider].contained_by(), Some(id));
    let target = game.spawn(0, 1, [position[0] + 70.0, position[1]], true);
    let health = game.entities[target].health;
    game.apply_weapon_hit(
        id,
        target,
        500.0,
        DamageType::ArmorPiercing,
        DeathType::Normal,
    );
    for _ in 0..12 {
        game.step();
    }
    assert!(game.entities[id].battle_bus_grounded());
    let position = game.entities[id].position;
    assert!(game.container_can_attack(id, target));
    game.attack(id, target).unwrap();
    let mut fired = false;
    for _ in 0..240 {
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == rider);
        assert_eq!(
            game.entities[id].position.map(f32::to_bits),
            position.map(f32::to_bits)
        );
        assert!(game.entities[id].ai.path_request.pending.is_none());
        assert!(game.entities[id].ai.ground_path.is_none());
        assert!(!game.entities[id].ai.motion.has_goal());
        if fired && game.entities[target].health < health {
            break;
        }
    }
    assert!(fired, "held hulk passenger must emit a shot");
    assert!(game.entities[target].health < health);
    assert!(game.entities[id].alive());
    assert!(game.entities[id].battle_bus_grounded());
}

#[test]
fn authored_physics_receives_throw_once_and_preserves_vertical_velocity() {
    use crate::skirmish::physical_ocl::{CollisionRules, LiveRules, MotionRules};
    let (mut game, old) = bus();
    let index = game.entities[old].type_index;
    let unit = &mut game.catalog[index];
    unit.live_physics = Some(LiveRules {
        motion: MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.0,
            lateral_friction: 0.0,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: CollisionRules {
            stiffness: 0.5,
            rubble_height: 0.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.05, 0.025, 0.025],
        min_fall_speed: 99.0,
        fall_damage_factor: 1.0,
    });
    register_update(unit, "PhysicsBehavior");
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    let id = game.spawn(index, 0, [355.0, 255.0], true);
    let motion = &mut game.entities[id].live_physics.as_mut().unwrap().motion;
    motion.velocity = [3.0, 4.0, 1.25];
    motion.acceleration = [7.0, 8.0, 9.0];
    game.apply_weapon_hit(id, 0, 500.0, DamageType::ArmorPiercing, DeathType::Normal);
    let motion = &game.entities[id].live_physics.as_ref().unwrap().motion;
    assert_eq!(
        motion.velocity.map(f32::to_bits),
        [0.0_f32, 0.0, 1.25].map(f32::to_bits)
    );
    assert_eq!(
        motion.acceleration.map(f32::to_bits),
        [0.0_f32, 0.0, 2.0].map(f32::to_bits)
    );
    game.step();
    assert_eq!(
        game.height_above_ground(id).to_bits(),
        (1.25_f32 + (2.0 - 0.1)).to_bits()
    );
    for _ in 0..100 {
        game.step();
        if game.entities[id].battle_bus_grounded() {
            break;
        }
    }
    assert!(game.entities[id].battle_bus_grounded());
    assert!(game.entities[id].disabled_held());
}

#[test]
fn undead_selector_can_choose_an_ordinary_authored_slow_death() {
    let (mut game, old) = bus();
    let index = game.entities[old].type_index;
    let unit = &mut game.catalog[index];
    // The BattleBus instance is not applicable to this incoming death type.
    // UndeadBody must still visit the other SlowDeath interface.
    unit.death_lifecycle.as_mut().unwrap().slow[0]
        .conditions
        .death_mask = 0;
    register_update(unit, "SlowDeathBehavior");
    let module = unit.behavior_modules.last_mut().unwrap();
    module.interfaces |= 2;
    unit.death_lifecycle.as_mut().unwrap().slow.push(SlowDeath {
        derived: Derived::None,
        tag: module.tag.clone(),
        conditions: Conditions {
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
        },
        probability: 10,
        overkill_bonus: 0.0,
        sink_delay: [0, 0],
        destruction_delay: [30, 0],
        sink_rate: 0.0,
        fling: None,
        fx: [vec!["OrdinarySelected".into()], Vec::new(), Vec::new()],
        objects: std::array::from_fn(|_| Vec::new()),
        weapons: std::array::from_fn(|_| Vec::new()),
    });
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    let id = game.spawn(index, 0, [355.0, 255.0], true);
    game.apply_weapon_hit(id, 0, 500.0, DamageType::ArmorPiercing, DeathType::Normal);
    assert!(game.entities[id].battle_bus_second_life());
    assert!(!game.entities[id].is_ai_dead());
    assert!(!game.battle_bus_death_state(id, 0).first_death);
    assert!(game.entities[id].death_lifecycle.as_ref().unwrap().native[1].active());
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "OrdinarySelected"
    );
}

#[test]
fn undead_estimate_does_not_apply_the_active_body_damage_scalar() {
    let (mut game, id) = bus();
    game.entities[id].battle_bonuses.armor = 0.5;
    game.apply_weapon_hit(id, 0, 400.0, DamageType::ArmorPiercing, DeathType::Normal);
    assert!(game.entities[id].battle_bus_second_life());
    assert_eq!(game.entities[id].health.to_bits(), 650.0_f32.to_bits());
    assert_eq!(
        game.entities[id].damage_state.previous_health.to_bits(),
        200.5_f32.to_bits()
    );
}

#[test]
fn scalar_can_kill_before_the_armor_only_estimate_reaches_second_life() {
    let (mut game, id) = bus();
    game.entities[id].battle_bonuses.armor = 2.0;
    game.apply_weapon_hit(id, 0, 300.0, DamageType::ArmorPiercing, DeathType::Normal);
    assert!(!game.entities[id].battle_bus_second_life());
    assert!(!game.entities[id].alive());
    assert!(game.battle_bus_death_state(id, 0).real_death);
}

#[test]
fn lethal_excluded_death_enters_second_life_without_slow_death_or_random_draw() {
    for death in [DeathType::Crushed, DeathType::Splatted] {
        let (mut game, id) = bus();
        let index = game.entities[id].type_index;
        // GLAVehicleBattleBus ModuleTag_08: ALL -CRUSHED -SPLATTED -EXTRA_4.
        game.catalog[index].death_lifecycle.as_mut().unwrap().slow[0]
            .conditions
            .death_mask = !((1 << DeathType::Crushed as u32)
            | (1 << DeathType::Splatted as u32)
            | (1 << DeathType::Extra4 as u32));
        let unit = &mut game.catalog[index];
        register_update(unit, "SlowDeathBehavior");
        let module = unit.behavior_modules.last_mut().unwrap();
        module.interfaces |= 2;
        // ModuleTag_30 is the ordinary empty-hulk death, applicable only to EXTRA_4.
        unit.death_lifecycle.as_mut().unwrap().slow.push(SlowDeath {
            derived: Derived::None,
            tag: module.tag.clone(),
            conditions: Conditions {
                death_mask: 1 << DeathType::Extra4 as u32,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
            probability: 10,
            overkill_bonus: 0.0,
            sink_delay: [1, 0],
            destruction_delay: [150, 0],
            sink_rate: 2.5,
            fling: None,
            fx: std::array::from_fn(|_| Vec::new()),
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        });
        game.destroy_maintenance_object(id);
        game.process_update_destruction();
        let id = game.spawn(index, 0, [355.0, 255.0], true);
        // Isolate UndeadBody's selector from damage callbacks, which can draw RNG.
        let selector_id = game.spawn(index, 0, [365.0, 255.0], true);
        let random = game.random;
        game.start_battle_bus_second_life(selector_id, death);
        assert_eq!(game.random, random, "CRC0 inverted bounds consume no RNG");
        assert!(
            game.entities[selector_id]
                .death_lifecycle
                .as_ref()
                .unwrap()
                .native
                .iter()
                .all(|state| !state.active())
        );
        game.apply_weapon_hit(id, 0, 999_999.0, DamageType::Crush, death);
        assert!(game.entities[id].battle_bus_second_life());
        assert_eq!(game.entities[id].health.to_bits(), 650.0_f32.to_bits());
        assert!(game.entities[id].alive());
        assert!(!game.entities[id].is_ai_dead());
        assert!(!game.battle_bus_death_state(id, 0).first_death);
        assert!(
            game.entities[id]
                .death_lifecycle
                .as_ref()
                .unwrap()
                .native
                .iter()
                .all(|state| !state.active())
        );
        assert!(game.presentation.object_fx_events.is_empty());
    }
}
