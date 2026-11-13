use super::*;
use crate::skirmish::{
    Kind, Order,
    ai_goals::{COMMAND_FROM_AI, COMMAND_FROM_SCRIPT},
    tests,
    weapons::Lock,
};

fn setup(command: Command, power: Option<Power>) -> (Skirmish, usize, usize) {
    let (mut game, old) = crate::skirmish::ground_movement::tests::moving_tank();
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    game.opponent_rules = Some(std::sync::Arc::new(
        crate::skirmish::opponent::tests::rules(Vec::new()),
    ));
    game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[kind];
    unit.command_set = "HuntCommands".into();
    unit.secondary_weapon =
        Some(tests::fixture_type(crate::skirmish::Faction::Gla, Kind::Tank).weapon);
    for _ in 0..2 {
        unit.behavior.command_button_hunts.push(Rules {
            scan_frames: 7,
            scan_range: 250.0,
        });
        tests::register_update(unit, "CommandButtonHuntUpdate");
    }
    if let Some(power) = &power {
        add_power(unit, power);
    }
    let mut slots = [None; 18];
    slots[17] = Some(0);
    game.configure_command_buttons(Definitions {
        buttons: vec![Button {
            name: "HuntButton".into(),
            command,
            weapon_slot: 1,
            power,
            upgrade: None,
            needs_object: true,
        }],
        sets: BTreeMap::from([("HuntCommands".into(), slots)]),
    });
    let actor = game.spawn(kind, 1, [300.0, 200.0], true);
    let registration = game.catalog[kind].update_modules.iter().position(|module| matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior {family, ..} if family == "CommandButtonHuntUpdate")).unwrap();
    game.update_ground_contacts();
    (game, actor, registration)
}

#[test]
fn script_only_slot_wakes_first_real_instance_and_player_stop_retires_it() {
    let (mut game, actor, registration) = setup(Command::SwitchWeapon, None);
    assert!(game.update_remaining_sleep(actor, registration) > 1000);
    game.team_command_button_hunt(&[actor], "HuntButton");
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Hunt
    ));
    assert!(game.entities[actor].last_command_from_ai());
    assert_eq!(game.entities[actor].weapon_slot(), 1);
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Temporary
    ));
    assert_eq!(game.update_remaining_sleep(actor, registration), 1);
    assert!(
        game.entities[actor].behaviors.command_button_hunts[1]
            .button
            .is_none()
    );
    game.stop(actor).unwrap();
    game.tick += 1;
    game.run_object_updates();
    assert!(
        game.entities[actor].behaviors.command_button_hunts[0]
            .button
            .is_none()
    );
    assert!(
        game.entities[actor].behaviors.command_button_hunts[0]
            .name
            .is_empty()
    );
    assert!(game.update_remaining_sleep(actor, registration) > 1000);
}

#[test]
fn weapon_hunt_does_not_override_a_permanent_lock_or_invent_an_empty_slot() {
    let (mut game, actor, _) = setup(Command::FireWeapon, None);
    game.switch_weapon(actor, 0).unwrap();
    game.team_command_button_hunt(&[actor], "HuntButton");
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    assert!(matches!(
        game.entities[actor].weapon_set.lock,
        Lock::Permanent
    ));
    game.stop(actor).unwrap();
    game.entities[actor].weapon_set.lock = Lock::None;
    game.command_buttons.buttons[0].weapon_slot = 2;
    game.team_command_button_hunt(&[actor], "HuntButton");
    assert_eq!(game.entities[actor].weapon_slot(), 0);
    assert!(matches!(game.entities[actor].weapon_set.lock, Lock::None));
}

#[test]
fn special_hunt_starts_real_ability_and_active_gate_preserves_scan_schedule() {
    let power = Power {
        name: "HuntPower".into(),
        kind: crate::skirmish::special_ability::PowerType::SpecialHackerDisableBuilding,
    };
    let (mut game, actor, registration) = setup(Command::SpecialPower, Some(power));
    let kind = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[kind].kind_of.push("CAPTURABLE".into());
    let target = game.spawn(kind, 0, [380.0, 200.0], true);
    game.update_ground_contacts();
    assert_eq!(game.scan_command_button_target(actor, 0, 0), Some(target));
    game.team_command_button_hunt(&[actor], "HuntButton");
    assert!(game.native_ability_active(actor, 0));
    assert_eq!(game.update_remaining_sleep(actor, registration), 1);
    assert_eq!(game.update_command_button_hunt(actor, 0), 7);
    game.stop(actor).unwrap();
    assert_eq!(
        game.update_command_button_hunt(actor, 0),
        i32::from_ne_bytes(FOREVER.to_ne_bytes())
    );
}

#[test]
fn special_targeting_uses_computer_fog_exception_and_live_relationship_filters() {
    use crate::skirmish::special_ability::PowerType;
    let (mut game, actor, _) = setup(
        Command::SpecialPower,
        Some(Power {
            name: "HuntPower".into(),
            kind: PowerType::SpecialHackerDisableBuilding,
        }),
    );
    let kind = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[kind].kind_of.push("CAPTURABLE".into());
    let target = game.spawn(kind, 0, [380.0, 200.0], true);
    game.update_ground_contacts();
    assert!(!game.visible_to(1, target));
    assert_eq!(game.scan_command_button_target(actor, 0, 0), Some(target));
    game.slots[1].difficulty = None;
    assert_eq!(game.scan_command_button_target(actor, 0, 0), None);
    assert!(game.can_special_power_at_object_from(
        actor,
        "HuntPower",
        PowerType::SpecialHackerDisableBuilding,
        target,
        COMMAND_FROM_SCRIPT
    ));
    game.slots[1].difficulty = Some(crate::skirmish::OpponentDifficulty::Hard);
    game.catalog[kind].kind_of.push("REBUILD_HOLE".into());
    assert_eq!(game.scan_command_button_target(actor, 0, 0), None);
}

#[test]
fn entering_hunt_selects_nearest_valid_crate_and_keeps_ai_command_source() {
    let (mut game, old, _) = setup(Command::Hijack, None);
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    let kind = game.entities[old].type_index;
    game.catalog[kind].hijacker = Some(crate::skirmish::hijacker::Rules {
        required: vec!["VEHICLE".into()],
        forbidden: Vec::new(),
        pickup_science: None,
    });
    tests::register_behavior(
        &mut game.catalog[kind],
        "ConvertToHijackedVehicleCrateCollide",
        0x10,
    );
    let actor = game.spawn(kind, 1, [300.0, 200.0], true);
    let near = game.spawn(kind, 0, [340.0, 200.0], true);
    let far = game.spawn(kind, 0, [380.0, 200.0], true);
    game.entities[near].crew.hijacked = true;
    game.update_ground_contacts();
    game.team_command_button_hunt(&[actor], "HuntButton");
    assert!(matches!(game.entities[actor].ai.order, Order::Enter(id) if id == far));
    assert_eq!(
        game.entities[actor].ai.command.last_command_source,
        COMMAND_FROM_AI
    );
    assert_eq!(game.entities[far].owner, 0);
}

#[test]
fn missing_button_replaces_retained_name_but_never_issues_an_order() {
    let (mut game, actor, _) = setup(Command::SwitchWeapon, None);
    game.team_command_button_hunt(&[actor], "HuntButton");
    game.set_hunt_command_button(actor, 0, "Missing");
    assert_eq!(
        game.entities[actor].behaviors.command_button_hunts[0].name,
        "Missing"
    );
    assert!(
        game.entities[actor].behaviors.command_button_hunts[0]
            .button
            .is_none()
    );
    assert!(matches!(
        game.entities[actor].ai.command.main_state,
        crate::skirmish::ai_goals::MainState::Hunt
    ));
}

#[test]
fn native_map_team_action_dispatches_only_its_actual_members() {
    use crate::skirmish::map_scripts::{Action, Condition, Script};
    let (mut game, actor, _) = setup(Command::SwitchWeapon, None);
    let kind = game.entities[actor].type_index;
    let excluded = game.spawn(kind, 1, [340.0, 200.0], true);
    let prototype = game.team_membership.add_prototype(1, "ButtonTeam".into());
    let team = game.team_membership.create_team(prototype);
    game.team_membership.set_team(actor, Some(team));
    game.install_map_scripts(vec![Script {
        owner: String::new(),
        name: "Button hunt".into(),
        active: true,
        one_shot: true,
        difficulty: [true; 3],
        delay_frames: 0,
        subroutine: false,
        group: None,
        conditions: vec![vec![Condition::True]],
        actions: vec![Action::TeamHuntCommand(
            "ButtonTeam".into(),
            "HuntButton".into(),
        )],
        false_actions: Vec::new(),
    }]);
    game.update_map_scripts();
    assert!(
        game.entities[actor].behaviors.command_button_hunts[0]
            .button
            .is_some()
    );
    assert!(
        game.entities[excluded].behaviors.command_button_hunts[0]
            .button
            .is_none()
    );
}

#[test]
fn center_range_is_strict_but_priority_uses_bounding_circle_distance() {
    use crate::skirmish::special_ability::PowerType;
    let (mut game, actor, _) = setup(
        Command::SpecialPower,
        Some(Power {
            name: "HuntPower".into(),
            kind: PowerType::SpecialHackerDisableBuilding,
        }),
    );
    let small = game.type_for(0, Kind::Barracks).unwrap();
    let large = game.type_for(0, Kind::Factory).unwrap();
    game.catalog[small].kind_of.push("CAPTURABLE".into());
    game.catalog[large].kind_of.push("CAPTURABLE".into());
    game.catalog[large].geometry.shape = crate::collision_geometry::Shape::Cylinder;
    game.catalog[large].geometry.major = 180.0;
    game.catalog[large].geometry.minor = 180.0;
    let near = game.spawn(small, 0, [380.0, 200.0], true);
    let far = game.spawn(large, 0, [420.0, 200.0], true);
    game.update_ground_contacts();
    assert_eq!(game.scan_command_button_target(actor, 0, 0), Some(far));
    game.set_object_position(far, [550.0, 200.0, 0.0]);
    game.update_ground_contacts();
    assert_eq!(game.scan_command_button_target(actor, 0, 0), Some(near));
}

#[test]
fn scripted_capture_chooses_a_neutral_provider_and_preserves_other_team_members() {
    use crate::skirmish::special_ability::PowerType;
    let (mut game, actor, _) = setup(
        Command::SpecialPower,
        Some(Power {
            name: "HuntPower".into(),
            kind: PowerType::SpecialInfantryCaptureBuilding,
        }),
    );
    let kind = game.type_for(0, Kind::Barracks).unwrap();
    game.catalog[kind].name = "Provider".into();
    game.catalog[kind]
        .kind_of
        .extend(["STRUCTURE".into(), "CAPTURABLE".into()]);
    let neutral = game.neutral_owner();
    let target = game.spawn(kind, neutral, [380.0, 200.0], true);
    let friendly = game.spawn(kind, 1, [330.0, 200.0], true);
    let other = game.spawn(game.entities[actor].type_index, 1, [300.0, 250.0], true);
    game.update_ground_contacts();
    assert!(
        game.team_power_on_nearest_type(&[actor], "HuntButton", &["Provider".into()])
            .is_some()
    );
    assert!(game.native_ability_active(actor, 0));
    assert!(!game.capture_in_use(other));
    for _ in 0..300 {
        if game.entities[target].owner == 1 {
            break;
        }
        game.step();
    }
    assert_eq!(game.entities[target].owner, 1);
    assert_eq!(game.entities[friendly].owner, 1);
}

#[test]
fn scripted_capture_without_a_valid_list_target_leaves_current_orders_intact() {
    use crate::skirmish::special_ability::PowerType;
    let (mut game, actor, _) = setup(
        Command::SpecialPower,
        Some(Power {
            name: "HuntPower".into(),
            kind: PowerType::SpecialInfantryCaptureBuilding,
        }),
    );
    game.move_to(actor, [600.0, 200.0]).unwrap();
    assert!(
        game.team_power_on_nearest_type(&[actor], "HuntButton", &["AbsentProvider".into()])
            .is_some()
    );
    assert!(matches!(game.entities[actor].ai.order, Order::Move(_)));
    assert!(!game.capture_in_use(actor));
}

pub(in crate::skirmish) fn add_power(unit: &mut crate::skirmish::UnitType, power: &Power) {
    use crate::skirmish::{special_ability as ability, special_power};
    unit.special_powers.push(special_power::Rules {
        tag: "HuntPower".into(),
        name: power.name.clone(),
        power_type: match power.kind {
            ability::PowerType::SpecialInfantryCaptureBuilding => {
                "SPECIAL_INFANTRY_CAPTURE_BUILDING"
            }
            ability::PowerType::SpecialHackerDisableBuilding => "SPECIAL_HACKER_DISABLE_BUILDING",
            ability::PowerType::SpecialDisguiseAsVehicle => "SPECIAL_DISGUISE_AS_VEHICLE",
            _ => unreachable!("fixture power"),
        }
        .into(),
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
    unit.behavior_modules.last_mut().unwrap().tag = "HuntPower".into();
    unit.behavior.special_abilities.push(ability::Rules {
        power: Some(Power {
            name: power.name.clone(),
            kind: power.kind,
        }),
        approach: ability::Approach {
            start_range: 500.0,
            abort_range: 1000.0,
            requires_los: false,
            face_target: false,
            flee_range: 0.0,
        },
        timing: ability::Timings {
            preparation: 30,
            persistent: 0,
            pack: 0,
            unpack: 0,
            unstealth: 0,
            effect: 60,
            variation: 0.0,
        },
        objects: ability::Objects {
            name: String::new(),
            bone: String::new(),
            maximum: 1,
            persistent: false,
            unique_targets: false,
            persist_on_death: false,
        },
        packing: ability::Packing {
            skip_no_target: false,
            flip_after_pack: false,
            flip_after_unpack: false,
        },
        preparation: ability::Preparation {
            always_validate: false,
            capture_fx: false,
            persistence_recharges: false,
        },
        effects: ability::Effects {
            value: 1,
            experience: 0,
            skill_points: -1,
            lose_stealth: false,
        },
        feedback: ability::Feedback {
            disable_particle: None,
            sounds: std::array::from_fn(|_| String::new()),
            completion_voice: String::new(),
            capture_tick: String::new(),
        },
    });
    tests::register_update(unit, "SpecialAbilityUpdate");
}
