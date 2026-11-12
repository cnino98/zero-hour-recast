use super::*;
use crate::skirmish::{Kind, ProductionItem, hacker_disable::Effect, income::HackerRules, tests};

pub(super) fn setup() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let index = game.type_for(0, Kind::Infantry).unwrap();
    game.catalog[index].weapon.damage = 0.0;
    add_disable_ability(&mut game.catalog[index]);
    game.catalog[index].behavior.hacking = Some(HackerRules {
        unpack: 1,
        pack: 4,
        variation: 0.0,
        delay: 10,
        contained_delay: 10,
        amounts: [5; 4],
        experience: 1,
    });
    crate::skirmish::income::tests::bind_hacker(&mut game.catalog[index]);
    game.catalog[index]
        .behavior
        .destroy_die
        .push(tests::death_conditions());
    tests::register_behavior(&mut game.catalog[index], "DestroyDie", 2);
    let mut stream = tests::fixture_type(crate::skirmish::Faction::Civilian, Kind::Effect);
    stream.name = "TestDisableStream".into();
    stream.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    stream.body_kind = crate::skirmish::BodyKind::Inactive;
    stream.behavior_modules.clear();
    stream.update_modules.clear();
    stream.laser_update = Some(crate::skirmish::laser_update::Rules {
        particles: [String::new(), String::new()],
        punch_through: 0.0,
    });
    game.catalog.push(stream);
    let hacker = game.spawn(index, 0, [200.0, 200.0], true);
    let index = game.type_for(1, Kind::Barracks).unwrap();
    game.catalog[index].kind_of.push("CAPTURABLE".into());
    let target = game.spawn(index, 1, [280.0, 200.0], true);
    game.update_ground_contacts();
    game.update_visibility();
    assert!(!game.disable_building_ready(hacker));
    advance(&mut game, 2);
    (game, hacker, target)
}

fn advance(game: &mut Skirmish, frames: u32) {
    for _ in 0..frames {
        game.step();
    }
}

#[test]
fn hacker_face_goal_is_a_snapshot_while_busy_and_idle_clear_both_main_goals() {
    let (mut game, hacker, target) = setup();
    game.set_ai_ground_goal(hacker, [300.0, 400.0]);
    game.disable_building(hacker, target).unwrap();
    assert_eq!(
        game.entities[hacker].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    update_callback(&mut game, hacker);
    let face = game.object_position(target);
    assert_eq!(
        game.entities[hacker].ai_goal_position().map(f32::to_bits),
        face.map(f32::to_bits)
    );
    game.entities[target].position[0] += 10.0;
    assert_eq!(
        game.entities[hacker].ai_goal_position().map(f32::to_bits),
        face.map(f32::to_bits)
    );
    update_callback(&mut game, hacker);
    assert!(game.entities[hacker].ai_busy());
    assert_eq!(game.entities[hacker].ai.command.goal_object, None);
    assert_eq!(
        game.entities[hacker].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    game.ai_face_from_ai(
        hacker,
        crate::skirmish::combat::attack::Goal::Object(target),
    );
    assert_eq!(game.entities[hacker].ai.command.goal_object, Some(target));
    assert_eq!(
        game.entities[hacker].ai_goal_position().map(f32::to_bits),
        game.object_position(target).map(f32::to_bits)
    );
    game.ai_busy_from_ai(hacker);
    assert_eq!(game.entities[hacker].ai.command.goal_object, None);
    assert_eq!(
        game.entities[hacker].ai_goal_position().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
}

#[test]
fn original_unpack_preparation_retrigger_and_end_frame_expiry() {
    let (mut game, hacker, target) = setup();
    let started = game.tick;
    game.disable_building(hacker, target).unwrap();
    advance(&mut game, 2);
    assert!(
        game.entities[hacker]
            .drawable
            .model_conditions
            .has_flag(UNPACKING)
    );
    assert_eq!(
        game.entities[hacker].animation_duration(),
        Some((started + 2, 2, true))
    );
    advance(&mut game, 2);
    assert_eq!(game.entities[hacker].hacker_disable_target(), Some(target));
    assert!(!game.disable_building_ready(hacker));
    assert!(!game.entities[target].hacked());
    advance(&mut game, 2);
    assert!(game.disable_building_ready(hacker));
    assert!(!game.entities[target].hacked());
    game.step();
    assert_eq!(game.entities[target].hacked_until, started + 13);
    advance(&mut game, 2);
    assert_eq!(game.entities[target].hacked_until, started + 15);
    game.stop(hacker).unwrap();
    update_callback(&mut game, hacker);
    assert!(game.entities[hacker].hacker_disable_target().is_none());
    while game.tick < started + 14 {
        game.step();
    }
    assert!(game.entities[target].hacked());
    game.step();
    assert!(!game.entities[target].hacked());
    assert!(game.presentation.disable_events.iter().any(|event| {
        event.frame == started + 15
            && matches!(event.effect, Effect::Reenabled { target: id } if id == target)
    }));
}

#[test]
fn explicit_and_context_target_admission_retain_native_differences() {
    let (mut game, hacker, target) = setup();
    assert!(game.can_disable_building(hacker, target));
    assert!(game.can_context_disable_building(hacker, target));
    game.entities[target].build_progress = 0.5;
    assert!(game.can_disable_building(hacker, target));
    assert!(!game.can_context_disable_building(hacker, target));
    game.entities[target].build_progress = 1.0;
    let index = game.entities[target].type_index;
    game.catalog[index]
        .kind_of
        .retain(|kind| kind != "CAPTURABLE");
    game.catalog[index].kind_of.push("FS_TECHNOLOGY".into());
    assert!(!game.can_disable_building(hacker, target));
    assert!(game.can_context_disable_building(hacker, target));
    game.catalog[index].kind_of.push("IMMUNE_TO_CAPTURE".into());
    assert!(!game.can_context_disable_building(hacker, target));
    game.catalog[index].kind_of.push("CAPTURABLE".into());
    assert!(game.can_disable_building(hacker, target));
    game.catalog[index].kind_of.push("REBUILD_HOLE".into());
    assert!(!game.can_disable_building(hacker, target));
    game.catalog[index]
        .kind_of
        .retain(|kind| kind != "REBUILD_HOLE");
    let team = game.team_membership.default_team(0);
    game.defect_to_team(target, team, 1);
    assert!(!game.can_disable_building(hacker, target));
    game.entities[target].owner = game.neutral_owner();
    assert!(!game.can_context_disable_building(hacker, target));
    let team = game.team_membership.default_team(1);
    game.defect_to_team(target, team, 1);
    game.entities[target].health = 0.0;
    assert!(!game.can_disable_building(hacker, target));
}

#[test]
fn disabled_production_retains_queue_and_resumes_after_deadline_frame() {
    let (mut game, hacker, target) = setup();
    let infantry = game.type_for(1, Kind::Infantry).unwrap();
    game.train(target, infantry).unwrap();
    game.disable_building(hacker, target).unwrap();
    advance(&mut game, 7);
    let elapsed = game.entities[target].queue[0].elapsed_frames;
    game.stop(hacker).unwrap();
    advance(&mut game, 6);
    assert!(!game.entities[target].hacked());
    assert_eq!(
        game.entities[target].queue[0].elapsed_frames.to_bits(),
        elapsed.to_bits()
    );
    game.step();
    assert_eq!(
        game.entities[target].queue[0].elapsed_frames.to_bits(),
        (elapsed + 1.0).to_bits()
    );
    assert!(
        matches!(game.entities[target].queue[0].item, ProductionItem::Unit(index) if index == infantry)
    );
}

#[test]
fn disabled_power_output_and_proof_radar_stop_but_consumption_remains() {
    let (mut game, hacker, target) = setup();
    let index = game.entities[target].type_index;
    game.catalog[index].energy = 10;
    tests::radar_upgrade(&mut game.catalog[index], "Upgrade_HackerRadar", true, true);
    let position = game.entities[target].position;
    game.destroy_maintenance_object(target);
    game.process_update_destruction();
    let target = game.spawn(index, 1, position, true);
    game.update_ground_contacts();
    game.update_visibility();
    game.update_power();
    let production = game.players[1].energy().0;
    assert!(game.has_radar(1));
    game.disable_building(hacker, target).unwrap();
    advance(&mut game, 7);
    assert_eq!(
        game.players[1].energy().0.to_bits(),
        (production - 10.0).to_bits()
    );
    assert!(!game.has_radar(1));
    // Native Object::onDisabledEdge removes even a DisableProof provider.
    assert!(!game.has_radar(1));
    game.catalog[index].energy = -10;
    game.update_power();
    assert_eq!(game.players[1].energy().1.to_bits(), 10.0_f32.to_bits());
}

#[test]
fn native_pending_internal_commands_finish_income_pack_without_canceling_ability() {
    let (mut game, hacker, target) = setup();
    game.hack_internet(hacker).unwrap();
    advance(&mut game, 2);
    assert_eq!(game.entities[hacker].hacking_condition(), Some("FIRING_A"));
    let earned = game.players[0].statistics.money_earned;
    game.disable_building(hacker, target).unwrap();
    advance(&mut game, 10);
    assert_eq!(game.entities[hacker].hacker_disable_target(), Some(target));
    assert!(game.entities[target].hacked());
    assert_eq!(game.players[0].statistics.money_earned, earned);
    game.hack_internet(hacker).unwrap();
    assert_eq!(game.entities[hacker].hacker_disable_target(), Some(target));
    game.step();
    assert!(game.entities[hacker].hacker_disable_target().is_none());
    advance(&mut game, 12);
    assert!(game.players[0].statistics.money_earned > earned);
}

#[test]
fn death_capture_and_new_orders_cancel_without_erasing_existing_target_deadline() {
    let (mut game, hacker, target) = setup();
    game.disable_building(hacker, target).unwrap();
    advance(&mut game, 7);
    let team = game.team_membership.default_team(0);
    game.defect_to_team(target, team, 1);
    game.step();
    assert!(game.entities[hacker].hacker_disable_target().is_none());
    assert!(game.entities[target].hacked());
    let team = game.team_membership.default_team(1);
    game.defect_to_team(target, team, 1);
    game.disable_building(hacker, target).unwrap();
    game.move_to(hacker, [180.0, 150.0]).unwrap();
    update_callback(&mut game, hacker);
    assert!(!game.entities[hacker].behaviors.special_abilities[0].active);
    game.disable_building(hacker, target).unwrap();
    game.kill_object(hacker, crate::skirmish::DeathType::Normal);
    game.step();
    assert!(!game.entities[hacker].behaviors.special_abilities[0].active);
}

#[test]
fn blocked_approach_retains_power_and_retries_after_route_reopens() {
    let (mut game, hacker, target) = setup();
    game.entities[target].position = [680.0, 200.0];
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    let unit = &mut game.catalog[game.entities[hacker].type_index];
    unit.sight = crate::skirmish::visibility::Sight::new(600.0);
    unit.locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Legs,
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
    let ally = game.spawn(
        game.type_for(0, Kind::Tank).unwrap(),
        0,
        game.entities[hacker].position,
        true,
    );
    game.reserve_goal(ally, game.entities[hacker].position);
    let start = game.cell_at(game.entities[hacker].position).unwrap();
    assert!(game.unavailable_goals(hacker).contains(&start));
    let blocked = (19..=21)
        .flat_map(|x| (19..=21).map(move |y| crate::grid::Cell { x, y }))
        .filter(|&cell| cell != start);
    game.navigation = crate::grid::Grid::new(80, 60, blocked).unwrap();
    game.observe_ground_partition(hacker);
    game.observe_ground_partition(target);
    game.update_visibility();
    assert!(
        game.object_approach_path_world(hacker, game.object_position(target))
            .route
            .is_none()
    );
    game.disable_building(hacker, target).unwrap();
    for _ in 0..10 {
        game.step();
        assert!(game.entities[hacker].ability_approaching());
        assert_eq!(
            game.entities[hacker].behaviors.special_abilities[0].target,
            Some(target)
        );
        assert!(!game.entities[target].hacked());
    }
    game.navigation = crate::grid::Grid::new(80, 60, []).unwrap();
    for _ in 0..900 {
        game.step();
        if game.entities[target].hacked() {
            break;
        }
    }
    assert!(game.entities[target].hacked());
    game.stop(hacker).unwrap();
    update_callback(&mut game, hacker);
    assert!(!game.entities[hacker].behaviors.special_abilities[0].active);
}

fn update_callback(game: &mut Skirmish, id: usize) {
    game.tick += 1;
    game.update_ai_module(id);
    game.update_special_ability(id, 0);
}

fn add_disable_ability(unit: &mut crate::skirmish::UnitType) {
    use crate::skirmish::{special_power, tests};
    unit.special_powers.push(special_power::Rules {
        tag: "DisablePower".into(),
        name: "TestDisablePower".into(),
        power_type: "SPECIAL_HACKER_DISABLE_BUILDING".into(),
        required_science: String::new(),
        timer: special_power::Timer {
            reload_frames: 2,
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
            ready: false,
        },
        script_only: false,
    });
    tests::register_behavior(unit, "SpecialAbility", 0x0100);
    unit.behavior_modules.last_mut().unwrap().tag = "DisablePower".into();
    unit.behavior.special_abilities.push(Rules {
        power: Some(Power {
            name: "TestDisablePower".into(),
            kind: PowerType::SpecialHackerDisableBuilding,
        }),
        approach: Approach {
            start_range: 150.0,
            abort_range: 10_000_000.0,
            requires_los: true,
            face_target: true,
            flee_range: 0.0,
        },
        timing: Timings {
            preparation: 3,
            persistent: 2,
            pack: 0,
            unpack: 2,
            unstealth: 0,
            effect: 6,
            variation: 0.0,
        },
        objects: Objects {
            name: "TestDisableStream".into(),
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
            disable_particle: Some("DisabledEffectBinaryShower0".into()),
            sounds: std::array::from_fn(|_| String::new()),
            completion_voice: String::new(),
            capture_tick: String::new(),
        },
    });
    tests::register_update(unit, "SpecialAbilityUpdate");
}
