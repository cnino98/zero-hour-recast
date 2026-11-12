use super::*;
use crate::skirmish::{
    Faction, GroundAppearance, GroundRules, Kind, Order,
    tests::{fixture_type, game},
    weapons::Lock,
};

fn setup() -> (crate::skirmish::Skirmish, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    let mut laser = fixture_type(Faction::Civilian, Kind::Effect);
    laser.name = "TestTargetLaser".into();
    laser.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    laser.body_kind = crate::skirmish::BodyKind::Inactive;
    laser.behavior_modules.clear();
    laser.update_modules.clear();
    laser.laser_update = Some(crate::skirmish::laser_update::Rules {
        particles: [String::new(), String::new()],
        punch_through: 0.0,
    });
    game.catalog.push(laser);
    let unit = &mut game.catalog[5];
    unit.sight = crate::skirmish::visibility::Sight::new(600.0);
    add_laser_ability(unit);
    let mut secondary = fixture_type(Faction::Gla, Kind::Infantry).weapon;
    secondary.auto_choose = 0;
    secondary.range = 300.0;
    unit.secondary_weapon = Some(secondary);
    unit.locomotor.ground = Some(GroundRules {
        wander: None,
        appearance: GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let actor = game.spawn(5, 0, [300.0, 100.0], true);
    let target = game.spawn(6, 1, [400.0, 100.0], true);
    game.entities[target].health = 10_000.0;
    // Native LOS range queries use partition cell membership. Finish placement
    // before issuing the command whose first callback is being observed.
    game.update_ground_contacts();
    game.update_visibility();
    (game, actor, target)
}

#[test]
fn laser_main_goal_snapshots_face_and_each_attack_callback_but_clears_for_preparation() {
    let (mut game, actor, target) = setup();
    game.set_ai_ground_goal(actor, [200.0, 300.0]);
    game.laser_lock(actor, target).unwrap();
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    update_callback(&mut game, actor);
    let face = game.object_position(target);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        face.map(f32::to_bits)
    );
    game.entities[target].position[0] += 10.0;
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        face.map(f32::to_bits)
    );
    update_callback(&mut game, actor);
    assert_eq!(game.entities[actor].ai.command.ability_goal, None);
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    for _ in 0..30 {
        update_callback(&mut game, actor);
    }
    assert_eq!(
        game.entities[actor].ai_goal_position().map(f32::to_bits),
        game.object_position(target).map(f32::to_bits)
    );
}

#[test]
fn persistent_preparation_uses_thirty_then_fifteen_frames_and_restores_primary_on_command() {
    let (mut game, actor, target) = setup();
    game.laser_lock(actor, target).unwrap();
    update_callback(&mut game, actor);
    assert!(game.entities[actor].laser_target().is_none());
    update_callback(&mut game, actor);
    assert_eq!(game.entities[actor].laser_target(), Some(target));
    for _ in 0..29 {
        update_callback(&mut game, actor);
        assert_eq!(game.entities[actor].weapon_slot(), 0);
        assert!(game.presentation.shots.is_empty());
    }
    update_callback(&mut game, actor);
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    assert!(matches!(game.entities[actor].ai.order, Order::Attack(id) if id == target));
    for _ in 0..14 {
        update_callback(&mut game, actor);
    }
    assert!(
        game.entities[actor].behaviors.special_abilities[0]
            .preparing
            .frames
            == 1
    );
    update_callback(&mut game, actor);
    assert!(
        game.entities[actor].behaviors.special_abilities[0]
            .preparing
            .frames
            == 15
    );
    game.move_to(actor, [350.0, 150.0]).unwrap();
    assert!(matches!(game.entities[actor].weapon_set.lock, Lock::None));
    // Native onExit observes the player source in the next ability callback;
    // killSpecialObjects then requests a temporary PRIMARY lock.
    update_callback(&mut game, actor);
    assert!(game.entities[actor].laser_target().is_none());
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Temporary
    ));
}

#[test]
fn source_target_filters_abort_distance_and_permanent_lock_are_preserved() {
    let (mut game, actor, target) = setup();
    assert!(!game.can_laser_lock(actor, 2)); // enemy Command Center
    let ally = game.spawn(6, 0, [350.0, 100.0], true);
    assert!(!game.can_laser_lock(actor, ally));
    game.switch_weapon(actor, 0).unwrap();
    game.laser_lock(actor, target).unwrap();
    for _ in 0..32 {
        update_callback(&mut game, actor);
    }
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    game.entities[target].position = [600.0, 100.0];
    update_callback(&mut game, actor);
    assert!(game.entities[actor].laser_target().is_none());
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Permanent
    ));
    game.entities[target].health = 0.0;
    assert!(!game.can_laser_lock(actor, target));
}

#[test]
fn native_preparation_follows_one_face_callback_even_when_the_turn_is_incomplete() {
    let (mut game, actor, target) = setup();
    // Finish the genuine first Idle callback, including native ground-cell
    // placement, before measuring the independent ability's stationary prep.
    game.step();
    game.set_object_orientation(actor, std::f32::consts::PI);
    game.laser_lock(actor, target).unwrap();
    let origin = game.entities[actor].position;
    game.step();
    assert!(
        game.entities[actor].behaviors.special_abilities[0]
            .facing
            .initiated
    );
    assert!(
        !game.entities[actor].behaviors.special_abilities[0]
            .facing
            .complete
    );
    game.set_object_position(target, [400.0, 170.0, 0.0]);
    game.step();
    // cpp405 starts prep when isFacing() returns true. It does not wait
    // for Face to return to Idle; startPreparation itself issues aiIdle.
    assert_eq!(
        game.entities[actor].behaviors.special_abilities[0]
            .preparing
            .frames,
        30
    );
    assert!(game.entities[actor].ai_idle());
    let angle = game.entities[actor].facing;
    assert!(
        crate::skirmish::combat::relative_angle(origin, angle, game.entities[target].position)
            .abs()
            > 0.035
    );
    for _ in 0..27 {
        game.step();
    }
    assert_eq!(
        game.entities[actor].position.map(f32::to_bits),
        origin.map(f32::to_bits)
    );
    assert_eq!(game.entities[actor].facing.to_bits(), angle.to_bits());
    assert!(game.presentation.shots.is_empty());
}

#[test]
fn approach_uses_ability_range_even_when_primary_range_is_longer() {
    let (mut game, actor, target) = setup();
    game.catalog[5].weapon.range = 500.0;
    game.entities[target].position = [580.0, 100.0];
    game.laser_lock(actor, target).unwrap();
    for _ in 0..400 {
        game.step();
        if game.entities[actor].laser_target().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[actor].laser_target(), Some(target));
    assert!(game.range_distance(actor, target) <= 200.0);
    assert!(game.entities[actor].position[0] > 300.0);
}

#[test]
fn interrupted_move_completes_before_ability_ignores_the_occupied_target() {
    let (mut game, actor, _) = setup();
    let target = game.spawn(6, 1, [580.0, 170.0], true);
    game.move_to(actor, [350.0, 170.0]).unwrap();
    game.step();
    assert!(game.entities[actor].ai.motion.is_moving());
    game.laser_lock(actor, target).unwrap();
    for _ in 0..400 {
        game.step();
        if game.entities[actor].laser_target().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[actor].laser_target(), Some(target));
    assert!(game.range_distance(actor, target) <= 200.0);
}

#[test]
fn null_initial_route_preserves_ability_until_reopened_and_stop_still_cancels() {
    let (mut game, actor, _) = setup();
    let target = game.spawn(6, 1, [680.0, 100.0], true);
    let ally = game.spawn(6, 0, game.entities[actor].position, true);
    game.reserve_goal(ally, game.entities[actor].position);
    let start = game.cell_at(game.entities[actor].position).unwrap();
    assert!(game.unavailable_goals(actor).contains(&start));
    let blocked = (29..=31)
        .flat_map(|x| (9..=11).map(move |y| crate::grid::Cell { x, y }))
        .filter(|&cell| cell != start);
    game.navigation = crate::grid::Grid::new(80, 60, blocked).unwrap();
    game.update_visibility();
    assert!(
        game.object_approach_path_world(actor, game.object_position(target))
            .route
            .is_none()
    );
    game.laser_lock(actor, target).unwrap();
    for _ in 0..10 {
        game.step();
        assert!(game.entities[actor].ability_approaching());
        assert!(game.entities[actor].laser_target().is_none());
        assert!(game.presentation.shots.is_empty());
    }
    game.navigation = crate::grid::Grid::new(80, 60, []).unwrap();
    for _ in 0..600 {
        game.step();
        if game.entities[actor].laser_target().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[actor].laser_target(), Some(target));

    game.stop(actor).unwrap();
    update_callback(&mut game, actor);
    assert!(!game.entities[actor].behaviors.special_abilities[0].active);
    assert!(game.entities[actor].laser_target().is_none());
}

#[test]
fn closest_route_stays_active_outside_ability_range_and_does_not_claim_target() {
    let (mut game, actor, _) = setup();
    let target = game.spawn(6, 1, [680.0, 100.0], true);
    game.navigation =
        crate::grid::Grid::new(80, 60, (0..60).map(|y| crate::grid::Cell { x: 35, y })).unwrap();
    game.update_visibility();
    game.laser_lock(actor, target).unwrap();
    for _ in 0..120 {
        game.step();
        assert!(game.entities[actor].ability_approaching());
        assert!(game.entities[actor].laser_target().is_none());
    }
    assert!(game.entities[actor].position[0] > 300.0);
    assert!(game.range_distance(actor, target) > 200.0);
    assert!(game.occupancy.goal_cell(actor) != game.occupancy.goal_cell(target));
    game.stop(actor).unwrap();
    update_callback(&mut game, actor);
    assert!(!game.entities[actor].behaviors.special_abilities[0].active);
}

#[test]
fn active_laser_lock_blocks_friendly_placement_without_interrupting_the_ability() {
    let (mut game, actor, target) = setup();
    let builder_type = game.type_for(0, Kind::Builder).unwrap();
    let building_type = game.type_for(0, Kind::Barracks).unwrap();
    let site = game.entities[actor].position;
    let builder = game.spawn(builder_type, 0, [site[0] - 80.0, site[1]], true);
    game.update_ground_contacts();
    game.update_visibility();
    let angle = game.catalog[building_type].placement_angle;
    assert!(game.can_build_facing(builder, building_type, site, angle));
    game.laser_lock(actor, target).unwrap();
    for _ in 0..3 {
        update_callback(&mut game, actor);
    }
    assert_eq!(game.entities[actor].laser_target(), Some(target));
    assert!(!game.can_build_facing(builder, building_type, site, angle));
    let cash = game.players[0].cash;
    assert!(game.build(builder, building_type, site).is_err());
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(game.entities[actor].laser_target(), Some(target));
}

// Establish the genuine preceding primary callback when the isolated ability
// consumer is waiting for Face. No FINAL/NORMAL phase inversion is introduced.
fn update_callback(game: &mut crate::skirmish::Skirmish, actor: usize) {
    game.tick += 1;
    if matches!(
        game.entities[actor].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::FaceObject { .. }
            | crate::skirmish::ai_goals::MainState::FacePosition { .. }
    ) {
        game.update_ai_module(actor);
    }
    game.update_special_ability(actor, 0);
}

#[test]
fn actual_laser_command_approaches_a_moving_object_and_waits_for_face_idle_before_preparation() {
    let (mut game, actor, target) = setup();
    game.set_object_position(target, [600.0, 300.0, 0.0]);
    game.set_object_orientation(actor, -std::f32::consts::PI);
    game.laser_lock(actor, target).unwrap();
    let mut moved = false;
    let mut faced = false;
    let mut prepared = false;
    for _ in 0..600 {
        game.step();
        if matches!(game.entities[actor].ai.order, Order::Move(_)) {
            assert_eq!(game.entities[actor].ai.command.goal_object, Some(target));
            assert_eq!(game.entities[actor].ai.ignored_producer, Some(target));
            moved = true;
        }
        if matches!(
            game.entities[actor].ai.command.main_state,
            crate::skirmish::ai_goals::MainState::FaceObject { .. }
        ) {
            assert!(
                game.entities[actor].behaviors.special_abilities[0]
                    .facing
                    .initiated
                    && !game.entities[actor].behaviors.special_abilities[0]
                        .facing
                        .complete
            );
            faced = true;
        }
        if game.entities[actor].behaviors.special_abilities[0]
            .preparing
            .frames
            > 0
        {
            assert!(game.entities[actor].ai_idle());
            prepared = true;
            break;
        }
    }
    assert!(moved && faced && prepared);
}

#[test]
fn native_preparation_status_blocks_mood_acquisition_while_primary_idle_still_runs() {
    let (mut game, actor, target) = setup();
    // Start from aligned Face without enabling acquisition until preparation.
    game.laser_lock(actor, target).unwrap();
    update_callback(&mut game, actor);
    update_callback(&mut game, actor);
    assert!(
        game.entities[actor].behaviors.special_abilities[0]
            .preparing
            .frames
            == 30
    );
    assert_ne!(game.entities[actor].object_status & (1 << 24), 0);
    game.catalog[5].ai.acquisition |= crate::skirmish::UnitAi::IDLE;
    // A real AI Idle command forces a fresh primary entry without ending the
    // independent ability. Its native mood query must refuse auto-acquisition.
    game.ai_idle_from_ai(actor);
    game.tick += 1;
    game.update_ai_module(actor);
    assert!(game.entities[actor].ai_idle());
    assert!(game.entities[actor].ai.attack.is_none());
    assert_eq!(game.entities[actor].ai.current_victim, None);
    assert!(
        game.entities[actor].behaviors.special_abilities[0]
            .preparing
            .frames
            == 30
    );
    game.stop(actor).unwrap();
    update_callback(&mut game, actor);
    assert_eq!(game.entities[actor].object_status & (1 << 24), 0);
    assert!(!game.entities[actor].behaviors.special_abilities[0].active);
}

#[test]
fn collision_idle_separation_preserves_the_pending_ability_and_records_ai_source() {
    let (mut game, actor, target) = setup();
    game.set_object_position(target, [650.0, 100.0, 0.0]);
    let other = game.spawn(6, 0, game.entities[actor].position, true);
    game.laser_lock(actor, target).unwrap();
    assert!(game.entities[actor].ai_idle());
    game.process_ground_contact(actor, other);
    assert!(game.entities[actor].ability_approaching());
    assert_eq!(
        game.entities[actor].behaviors.special_abilities[0].target,
        Some(target)
    );
    assert!(matches!(game.entities[actor].ai.order, Order::Move(_)));
    assert!(game.entities[actor].last_command_from_ai());
    assert_eq!(game.entities[actor].ai.command.goal_object, None);
    assert!(game.entities[actor].ai.path_request.pending.is_some());
}

fn add_laser_ability(unit: &mut crate::skirmish::UnitType) {
    use crate::skirmish::{special_power, tests};
    unit.special_powers.push(special_power::Rules {
        tag: "LaserPower".into(),
        name: "TestLaserPower".into(),
        power_type: "SPECIAL_MISSILE_DEFENDER_LASER_GUIDED_MISSILES".into(),
        required_science: String::new(),
        timer: special_power::Timer {
            reload_frames: 0,
            shared: false,
            public: false,
        },
        view_range: 0.0,
        view_frames: 0,
        view_template: None,
        initiate_sound: String::new(),
        target_sound: String::new(),
        module_initiate_sound: String::new(),
        academy_classification: "ACT_NONE".into(),
        update_starts_attack: true,
        startup: special_power::Startup {
            paused: false,
            ready: true,
        },
        script_only: false,
    });
    tests::register_behavior(unit, "SpecialAbility", 0x0100);
    unit.behavior_modules.last_mut().unwrap().tag = "LaserPower".into();
    unit.behavior.special_abilities.push(Rules {
        power: Some(Power {
            name: "TestLaserPower".into(),
            kind: PowerType::SpecialMissileDefenderLaserGuidedMissiles,
        }),
        approach: Approach {
            start_range: 200.0,
            abort_range: 250.0,
            requires_los: true,
            face_target: true,
            flee_range: 0.0,
        },
        timing: Timings {
            preparation: 30,
            persistent: 15,
            pack: 0,
            unpack: 0,
            unstealth: 0,
            effect: 0,
            variation: 0.0,
        },
        objects: Objects {
            name: "TestTargetLaser".into(),
            bone: "Muzzle01".into(),
            maximum: 1,
            persistent: false,
            unique_targets: false,
            persist_on_death: false,
        },
        packing: Packing {
            skip_no_target: false,
            flip_after_pack: false,
            flip_after_unpack: false,
        },
        preparation: Preparation {
            always_validate: false,
            capture_fx: false,
            persistence_recharges: false,
        },
        effects: Effects {
            value: 1,
            experience: 0,
            skill_points: -1,
            lose_stealth: false,
        },
        feedback: Feedback {
            disable_particle: None,
            sounds: std::array::from_fn(|_| String::new()),
            completion_voice: String::new(),
            capture_tick: String::new(),
        },
    });
    tests::register_update(unit, "SpecialAbilityUpdate");
}
