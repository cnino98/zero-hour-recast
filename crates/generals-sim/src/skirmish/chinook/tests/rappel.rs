use super::{aircraft, run_until};
use crate::skirmish::{
    Order, Skirmish,
    ai_goals::{COMMAND_FROM_AI, MainState},
    chinook::Phase,
    model_bones,
    physical_ocl::{CollisionRules, LiveRules, MotionRules},
};
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> (Skirmish, usize, usize) {
    let (mut game, host) = aircraft();
    let index = game.entities[host].type_index;
    let drop = &mut game.catalog[index].chinook.as_mut().unwrap().drop;
    drop.ropes = 1;
    drop.delay = [1, 1];
    drop.wait_for_ropes = false;
    let mut transforms = BTreeMap::new();
    transforms.insert(
        "ropestart01".into(),
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    );
    transforms.insert(
        "ropeend01".into(),
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, -4.0],
        ],
    );
    game.catalog[index].pristine_draws.push(model_bones::Draw {
        states: vec![model_bones::State {
            conditions: vec![BTreeSet::new()],
            transforms,
        }],
        ignored: BTreeSet::new(),
    });
    let unit = &mut game.catalog[5];
    unit.kind_of.push("CAN_RAPPEL".into());
    unit.locomotor.stick_to_ground = true;
    unit.live_physics = Some(LiveRules {
        motion: MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -64.0 / 900.0,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
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
        shock_rotation: [0.0; 3],
        min_fall_speed: 99.0,
        fall_damage_factor: 1.0,
    });
    crate::skirmish::tests::register_update(unit, "PhysicsBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x11;
    let rider = game.spawn(5, 0, [350.0, 335.0], true);
    game.add_to_contain(rider, host);
    assert_eq!(game.entities[rider].contained_by(), Some(host));
    (game, host, rider)
}

#[test]
fn combat_drop_removes_passenger_and_runs_rappel_until_actual_ground_contact() {
    let (mut game, host, rider) = fixture();
    game.set_chinook_state(
        host,
        Phase::CombatDrop,
        None,
        Some([350.0, 335.0, 0.0]),
        COMMAND_FROM_AI,
    );
    game.update_chinook_state(host);
    assert!(game.entities[rider].contained_by().is_none());
    assert!(!game.entities[rider].disabled_held());
    assert!(game.entities[rider].physics_allows_fall());
    assert!(matches!(
        game.entities[rider].ai.command.main_state,
        MainState::Rappel
    ));
    assert_eq!(game.object_position(rider)[2].to_bits(), 96.0_f32.to_bits());
    assert_eq!(
        game.entities[rider].ai.command.last_command_source,
        COMMAND_FROM_AI
    );
    assert!(
        game.model_flags(
            &game.entities[rider],
            game.catalog[5].body_damage_thresholds
        )
        .contains("RAPPELLING")
    );
    run_until(&mut game, |game| {
        !matches!(
            game.entities[rider].ai.command.main_state,
            MainState::Rappel
        )
    });
    assert!(game.entities[rider].alive());
    assert_eq!(game.height_above_ground(rider).to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        game.entities[rider].ai.command.desired_speed.to_bits(),
        f32::MAX.to_bits()
    );
    assert!(
        !game
            .model_flags(
                &game.entities[rider],
                game.catalog[5].body_damage_thresholds
            )
            .contains("RAPPELLING")
    );
    run_until(&mut game, |game| !game.entities[host].disabled_held());
    assert!(
        game.rope_drawables
            .drawables
            .iter()
            .all(|rope| rope.released.is_some())
    );
}

#[test]
fn dead_host_interrupts_active_rappeller_and_releases_held_and_rope_state() {
    let (mut game, host, rider) = fixture();
    game.set_chinook_state(
        host,
        Phase::CombatDrop,
        None,
        Some([350.0, 335.0, 0.0]),
        COMMAND_FROM_AI,
    );
    game.update_chinook_state(host);
    assert!(matches!(
        game.entities[rider].ai.command.main_state,
        MainState::Rappel
    ));
    game.kill_object(host, crate::skirmish::DeathType::Normal);
    game.update_ai_module(host);
    assert!(!game.entities[host].disabled_held());
    assert!(game.entities[rider].alive());
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(!matches!(
        game.entities[rider].ai.command.main_state,
        MainState::Rappel
    ));
    assert_eq!(
        game.entities[rider].ai.command.desired_speed.to_bits(),
        f32::MAX.to_bits()
    );
    assert!(game.rope_drawables.drawables[0].released.is_some());
}

#[test]
fn target_loss_changes_building_descent_to_ground_without_losing_rappel_rate() {
    let (mut game, host, rider) = fixture();
    let target = game.spawn(0, 1, [500.0, 335.0], true);
    game.remove_passenger(rider, host);
    game.set_object_position(rider, [500.0, 335.0, 100.0]);
    game.entities[rider].ai.command.desired_speed = 0.5;
    game.native_rappel_from(
        rider,
        Some(target),
        game.object_position(target),
        COMMAND_FROM_AI,
    );
    game.kill_object(target, crate::skirmish::DeathType::Normal);
    run_until(&mut game, |game| {
        !matches!(
            game.entities[rider].ai.command.main_state,
            MainState::Rappel
        )
    });
    assert!(game.entities[rider].alive());
    assert_eq!(game.height_above_ground(rider).to_bits(), 0.0_f32.to_bits());
    assert!(game.entities[rider].contained_by().is_none());
}

#[test]
fn hostile_garrison_drop_kills_at_most_two_and_only_surviving_rappeller_enters() {
    for enemies in [1, 2, 3] {
        let (mut game, host, rider) = fixture();
        game.remove_passenger(rider, host);
        let mut unit = crate::skirmish::tests::fixture_type(
            crate::skirmish::Faction::Gla,
            crate::skirmish::Kind::Barracks,
        );
        unit.kind_of = vec!["STRUCTURE".into()];
        unit.geometry.height = 30.0;
        unit.geometry_bounds.max_height = 30.0;
        let mut contain = crate::skirmish::containment::tests::rules(0.0, 0.5);
        contain.garrison = Some(crate::skirmish::containment::GarrisonRules {
            allow_enemies: false,
            allow_neutral: true,
            immune_to_clear: false,
            heal_frames: None,
            stations: None,
        });
        unit.contain = Some(contain);
        crate::skirmish::tests::register_update(&mut unit, "GarrisonContain");
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let building = game.spawn(kind, game.neutral_owner(), [500.0, 335.0], true);
        let geometry_source = game.spawn(kind, game.neutral_owner(), [900.0, 335.0], true);
        game.set_dynamic_geometry(geometry_source, [170.0, 30.0, 20.0]);
        game.copy_object_geometry(building, geometry_source);
        assert_eq!(
            game.catalog[kind].geometry_bounds.max_height.to_bits(),
            30.0_f32.to_bits()
        );
        game.set_chinook_state(
            host,
            Phase::MoveCombat,
            Some(building),
            None,
            COMMAND_FROM_AI,
        );
        assert_eq!(
            game.flight_preferred_height(host).to_bits(),
            200.0_f32.to_bits()
        );
        assert_eq!(
            game.entities[host]
                .chinook
                .as_ref()
                .unwrap()
                .combat_destination_z
                .to_bits(),
            200.0_f32.to_bits()
        );
        game.native_idle_from(host, COMMAND_FROM_AI);
        let mut victims = Vec::new();
        for _ in 0..enemies {
            let victim = game.spawn(5, 1, [500.0, 335.0], true);
            game.add_to_contain(victim, building);
            victims.push(victim);
        }
        assert_eq!(game.passengers(building), victims);
        game.set_object_position(rider, [500.0, 335.0, 170.0]);
        game.entities[rider].ai.command.desired_speed = 0.5;
        game.native_rappel_from(
            rider,
            Some(building),
            game.object_position(building),
            COMMAND_FROM_AI,
        );
        game.update_rappel(rider);
        assert!(
            victims
                .iter()
                .take(2)
                .all(|&id| game.entities[id].is_effectively_dead())
        );
        if enemies == 1 {
            assert!(game.entities[rider].alive());
            assert_eq!(game.entities[rider].contained_by(), Some(building));
            assert_eq!(game.passengers(building), &[rider]);
        } else {
            assert!(game.entities[rider].is_effectively_dead());
            assert!(game.entities[rider].contained_by().is_none());
            assert_eq!(game.passengers(building).len(), enemies - 2);
            if enemies == 3 {
                assert!(game.entities[victims[2]].alive());
            }
        }
    }
}
