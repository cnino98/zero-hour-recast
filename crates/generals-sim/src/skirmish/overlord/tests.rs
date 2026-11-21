use super::{Propaganda, Upgrade};
use crate::skirmish::{Faction, Kind, Research, Skirmish, tests::fixture_type};

fn setup() -> (Skirmish, usize) {
    let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 0.5);
    let child_type = game.entities[carrier].type_index;
    game.entities[carrier].health = 0.0;
    game.finish_death(carrier, None);
    game.destroy_maintenance_object(carrier);
    game.process_update_destruction();
    game.catalog[child_type].name = "PortableBunker".into();
    game.catalog[child_type].attachment_bone = Some("FIREPOINT01".into());
    game.catalog[child_type].kind_of = vec!["PORTABLE_STRUCTURE".into()];
    game.catalog[child_type].speed = 0.0;
    game.catalog[child_type].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    let mut parent = fixture_type(Faction::Gla, Kind::Tank);
    parent.turret = Some(crate::skirmish::turret::Rules::source_defaults());
    // Installed ChinaTankOverlord authors TransportAIUpdate, whose virtual
    // attack command forwards player/script orders to the portable passenger.
    for module in &mut parent.behavior_modules {
        if module.family == "AIUpdateInterface" {
            module.family = "TransportAIUpdate".into();
        }
    }
    for module in &mut parent.update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } = module
            && family == "AIUpdateInterface"
        {
            *family = "TransportAIUpdate".into();
        }
    }
    parent.production_queue_limit = 1;
    parent.production = Some(Vec::new());
    crate::skirmish::tests::register_update(&mut parent, "ProductionUpdate");
    parent.attachment_point = [12.0, 0.0, 15.0];
    let mut contain = crate::skirmish::containment::tests::rules(0.0, 1.0);
    contain.capacity = 1;
    contain.passengers_in_turret = true;
    contain.allowed = vec!["PORTABLE_STRUCTURE".into()];
    contain.art = std::array::from_fn(|_| crate::skirmish::containment::Art {
        exits: Vec::new(),
        fire_points: crate::skirmish::containment::tests::fire_points(&[[12.0, 0.0, 15.0]]),
    });
    parent.contain = Some(contain);
    crate::skirmish::tests::register_update(&mut parent, "OverlordContain");
    parent
        .behavior
        .object_creation
        .push(Some(crate::skirmish::object_creation::List {
            name: "FixtureBunkerCreation".into(),
            objects: vec![crate::skirmish::object_creation::Object {
                container: String::new(),
                invulnerable_time: 0,
                force_intensity: 0.0,
                force_magnitude: [0.0; 2],
                force_pitch: [0.0; 2],
                spin_rate: -1.0,
                formation_distances: [0.0; 3],
                fade_frames: 0,
                fade_sound: String::new(),
                names: vec!["PortableBunker".into()],
                count: 1,
                offset: [0.0; 3],
                disposition: crate::skirmish::physical_ocl::ON_GROUND,
                health: [1.0; 2],
                lifetime: [0; 2],
                flags: crate::skirmish::object_creation::CONTAIN_INSIDE_SOURCE
                    | crate::skirmish::object_creation::PRESERVE_LAYER,
            }],
        }));
    crate::skirmish::tests::register_behavior(&mut parent, "ObjectCreationUpgrade", 0x80);
    parent.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Bunker".into()];
    parent.behavior.attachments = vec![Upgrade {
        trigger: "Bunker".into(),
        conflicts: vec!["Cannon".into()],
        object: "PortableBunker".into(),
        command_set: "BunkerCommands".into(),
    }];
    parent.maintenance.research = vec![
        Research {
            name: "Bunker".into(),
            cost: 400,
            frames: 450,
            player: false,
            sciences: Vec::new(),
            ignores_underpowered: false,
        },
        Research {
            name: "Cannon".into(),
            cost: 1200,
            frames: 600,
            player: false,
            sciences: Vec::new(),
            ignores_underpowered: false,
        },
    ];
    game.catalog.push(parent);
    let host = game.spawn(game.catalog.len() - 1, 0, [355.0, 255.0], true);
    // Object update registration begins at native frame1.
    game.tick = 1;
    (game, host)
}

fn upgrade(game: &mut Skirmish, host: usize) -> usize {
    game.queue_upgrade(host, 0).unwrap();
    for _ in 0..450 {
        game.step();
    }
    game.entities[host]
        .attachment_child()
        .expect("completed attachment")
}

#[test]
fn paid_upgrade_has_one_queue_slot_and_creates_one_unselectable_portable_child() {
    let (mut game, host) = setup();
    let cash = game.players[0].cash;
    game.queue_upgrade(host, 0).unwrap();
    assert_eq!(game.players[0].cash, cash - 400);
    assert!(game.queue_upgrade(host, 1).is_err());
    for _ in 0..449 {
        game.step();
    }
    assert!(game.entities[host].attachment_child().is_none());
    game.step();
    let child = game.entities[host].attachment_child().unwrap();
    assert_eq!(game.entities[child].attachment_parent(), Some(host));
    assert!(!game.entities[child].masked());
    assert!(!game.entities[child].selectable(&game.catalog[game.entities[child].type_index]));
    assert!(game.exit_container(child).is_err());
    let count = game.entities.len();
    game.activate_upgrade_modules(host);
    assert_eq!(game.entities.len(), count);
    assert!(game.queue_upgrade(host, 0).is_err());
}

#[test]
fn cancelled_attachment_refunds_payment_and_does_not_create_a_child() {
    let (mut game, host) = setup();
    let cash = game.players[0].cash;
    game.queue_upgrade(host, 0).unwrap();
    game.step();
    game.cancel_upgrade(0, host, 0).unwrap();
    assert_eq!(game.players[0].cash, cash);
    for _ in 0..500 {
        game.step();
    }
    assert!(game.entities[host].attachment_child().is_none());
}

#[test]
fn original_turret_point_and_damage_category_propagate_to_attachment() {
    let (mut game, host) = setup();
    let child = upgrade(&mut game, host);
    game.set_object_orientation(host, 0.0);
    game.entities[host].ai.turret.angle = std::f32::consts::FRAC_PI_2;
    game.containment_transform_changed(host);
    let remaining = game.entities[host].max_health * 0.09;
    let amount = game.entities[host].health - remaining;
    game.apply_native_damage(host, None, amount, crate::skirmish::DeathType::Normal);
    game.set_object_position(host, [355.0, 255.0, 0.0]);
    let position = game.object_position(child);
    assert!((position[0] - 355.0).abs() < 0.001);
    assert!((position[1] - 267.0).abs() < 0.001);
    assert_eq!(position[2].to_bits(), 15.0_f32.to_bits());
    let damaged_health = game.entities[child].max_health * 0.1;
    assert_eq!(
        game.entities[child].health.to_bits(),
        (damaged_health - 1.0).to_bits()
    );
}

#[test]
fn bunker_redirects_boarding_inventory_firing_and_death_to_its_child() {
    let (mut game, host) = setup();
    let child = upgrade(&mut game, host);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(rider, host).unwrap();
    for _ in 0..1200 {
        game.step();
        if game.entities[rider].contained_by().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[rider].contained_by(), Some(child));
    assert_eq!(game.passengers(host), &[rider]);
    assert!(game.passenger_can_fire(rider));
    let before = game.entities[rider].health;
    game.entities[host].health = 0.0;
    game.finish_death(host, None);
    assert!(!game.entities[child].alive());
    assert!(game.entities[rider].alive());
    let damage = game.entities[rider].max_health * 0.5;
    assert_eq!(
        game.entities[rider].health.to_bits(),
        (before - damage).to_bits()
    );
    assert!(game.entities[rider].contained_by().is_none());
}

#[test]
fn propaganda_scans_at_sixty_then_heals_each_frame_from_its_latched_list() {
    let (mut game, host) = setup();
    let kind = game
        .catalog
        .iter()
        .position(|unit| unit.name == "PortableBunker")
        .unwrap();
    game.catalog[kind].behavior.propaganda = Some(Propaganda {
        radius: 150.0,
        delay: 60,
        heal: [0.01, 0.02],
        upgrade: "Subliminal".into(),
        pulse: ["Pulse".into(), "UpgradedPulse".into()],
        affects_self: false,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "PropagandaTowerBehavior");
    let child = upgrade(&mut game, host);
    game.entities[host].health = 50.0;
    game.entities[child].attachment.scanned = game.tick;
    for _ in 0..59 {
        game.step();
    }
    assert_eq!(game.entities[host].health.to_bits(), 50.0_f32.to_bits());
    game.step();
    let healing = 0.01 / 30.0 * game.entities[host].max_health;
    let expected = 50.0_f32 + healing;
    assert_eq!(game.entities[host].health.to_bits(), expected.to_bits());
    assert_eq!(game.presentation.propaganda_events.len(), 1);
    game.entities[host].position = [50.0, 50.0];
    game.entities[child].position = [350.0, 250.0];
    game.update_ground_contacts();
    game.tick += 1;
    game.run_object_updates();
    assert!(game.entities[host].health > expected); // latched source list until next scan
    game.remove_propaganda(child);
    assert_eq!(game.entities[host].attachment.bonuses, 0);
}

#[test]
fn portable_gun_fires_through_the_host_command_and_sinks_kill_experience() {
    let (mut game, host) = setup();
    let parent_type = game.entities[host].type_index;
    let child_type = game
        .catalog
        .iter()
        .position(|unit| unit.name == "PortableBunker")
        .unwrap();
    game.catalog[parent_type].weapon.damage = 0.0;
    game.catalog[child_type].contain = None;
    game.catalog[child_type]
        .behavior_modules
        .retain(|entry| entry.family != "TransportContain");
    game.catalog[child_type].update_modules.retain(|entry| {
        !matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "TransportContain")
    });
    game.catalog[parent_type].veterancy.trainable = true;
    game.catalog[parent_type].veterancy.required = [0, 100, 200, 400];
    game.catalog[child_type].weapon = fixture_type(Faction::Gla, Kind::Infantry).weapon;
    game.catalog[child_type].weapon.range = 150.0;
    game.catalog[child_type].ai.acquisition |= crate::skirmish::UnitAi::IDLE;
    let child = upgrade(&mut game, host);
    let mut victim = fixture_type(Faction::Usa, Kind::Infantry);
    victim.weapon.damage = 0.0;
    victim.veterancy.experience_value = [25; 4];
    game.catalog.push(victim);
    let target = game.spawn(game.catalog.len() - 1, 1, [425.0, 255.0], true);
    game.entities[target].health = 1.0;
    game.attack(host, target).unwrap();
    let mut fired = false;
    for _ in 0..900 {
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == child);
        if !game.entities[target].alive() {
            break;
        }
    }
    assert!(fired);
    assert!(!game.entities[target].alive());
    assert_eq!(game.entities[host].experience, 25);
    assert_eq!(game.entities[child].experience, 0);
}

fn armed_mount(pre_attack_frames: u32, immobile: bool) -> (Skirmish, usize, usize, usize) {
    let (mut game, host) = setup();
    let parent_type = game.entities[host].type_index;
    let child_type = game
        .catalog
        .iter()
        .position(|unit| unit.name == "PortableBunker")
        .unwrap();
    game.catalog[parent_type].weapon.damage = 0.0;
    game.catalog[child_type].contain = None;
    game.catalog[child_type]
        .behavior_modules
        .retain(|entry| entry.family != "TransportContain");
    game.catalog[child_type].update_modules.retain(|entry| {
        !matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "TransportContain")
    });
    game.catalog[child_type].kind_of.push("CAN_ATTACK".into());
    game.catalog[child_type].weapon = fixture_type(Faction::Gla, Kind::Infantry).weapon;
    game.catalog[child_type].weapon.range = 50.0;
    game.catalog[child_type].weapon.pre_attack_frames = pre_attack_frames;
    if immobile {
        game.catalog[child_type].kind_of.push("IMMOBILE".into());
    }
    let mut rules = crate::skirmish::turret::Rules::source_defaults();
    rules.controlled_slots = 1;
    rules.turn_rate = 0.2;
    game.catalog[child_type].turret = Some(rules);
    let child = upgrade(&mut game, host);
    let mut victim = fixture_type(Faction::Usa, Kind::Infantry);
    victim.weapon.damage = 0.0;
    game.catalog.push(victim);
    let target = game.spawn(game.catalog.len() - 1, 1, [425.0, 255.0], true);
    game.set_object_orientation(host, 0.0);
    let range = (game.entities[child].weapon_range(
        &game.catalog[game.entities[child].type_index],
        game.passenger_bonus_flags(child),
    ) + game.object_radius(child)
        + game.object_radius(target))
        - 1.0;
    game.entities[target].position = [game.entities[child].position[0] + range, 255.0];
    assert!(game.weapon_target_reachable(child, target));
    assert!(!game.catalog[child_type].ai.auto_acquire());
    game.attack(host, target).unwrap();
    (game, host, child, target)
}

#[test]
fn portable_gun_keeps_forwarded_target_when_carrier_rotation_temporarily_loses_range() {
    let (mut game, host, child, target) = armed_mount(0, false);
    game.set_object_orientation(host, std::f32::consts::PI);
    assert!(!game.weapon_target_reachable(child, target));
    for _ in 0..4 {
        game.step();
        assert!(
            matches!(game.entities[child].ai.order, crate::skirmish::Order::Attack(id) if id == target)
        );
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == child)
        );
    }

    game.set_object_orientation(host, 0.0);
    let mut fired = false;
    for _ in 0..120 {
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == child && shot.target == Some(target));
        if fired {
            break;
        }
    }
    assert!(
        fired,
        "the retained command must resume without idle acquisition"
    );

    let (mut game, host, child, target) = armed_mount(0, true);
    game.set_object_orientation(host, std::f32::consts::PI);
    assert!(!game.weapon_target_reachable(child, target));
    assert!(game.entities[child].ai.attack.is_some());
    assert!(game.entities[child].attacking());
    // IMMOBILE CHASE_TARGET is FailureState: entry fails on the first range check.
    game.step();
    assert!(matches!(
        game.entities[child].ai.order,
        crate::skirmish::Order::Idle
    ));
    game.step();
    assert!(matches!(
        game.entities[child].ai.order,
        crate::skirmish::Order::Idle
    ));
    assert!(
        !game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == child)
    );
}

#[test]
fn portable_gun_restarts_preattack_after_its_carrier_loses_and_recovers_range() {
    let (mut game, host, child, target) = armed_mount(8, false);
    for _ in 0..120 {
        game.step();
        if game.entities[child].weapon_set.weapons[0]
            .pre_attack
            .is_some()
        {
            break;
        }
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == child)
        );
    }
    let first = game.entities[child].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .map(|attack| (attack.start, attack.frames))
        .expect("initial preattack");
    assert_eq!(first.1, 8);
    for _ in 0..3 {
        game.step();
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == child)
        );
    }

    game.set_object_orientation(host, std::f32::consts::PI);
    assert!(!game.weapon_target_reachable(child, target));
    for _ in 0..10 {
        game.step();
        assert!(
            game.entities[child].weapon_set.weapons[0]
                .pre_attack
                .is_none()
        );
        assert!(
            matches!(game.entities[child].ai.order, crate::skirmish::Order::Attack(id) if id == target)
        );
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == child)
        );
    }

    game.set_object_orientation(host, 0.0);
    for _ in 0..120 {
        game.step();
        if game.entities[child].weapon_set.weapons[0]
            .pre_attack
            .is_some()
        {
            break;
        }
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == child)
        );
    }
    let restarted = game.entities[child].weapon_set.weapons[0]
        .pre_attack
        .as_ref()
        .map(|attack| (attack.start, attack.frames))
        .expect("new preattack");
    assert_eq!(restarted.0, game.tick);
    assert_eq!(restarted.1, 8);
    assert!(restarted.0 > first.0 + u64::from(first.1));
    for _ in 0..7 {
        game.step();
        assert!(
            !game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == child)
        );
    }
    game.step();
    assert_eq!(game.tick, restarted.0 + 8);
    assert!(
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == child && shot.target == Some(target))
    );
}

#[test]
fn a_second_speaker_cannot_stack_health_and_enclosing_the_host_removes_bonuses() {
    let (mut game, host) = setup();
    let kind = game
        .catalog
        .iter()
        .position(|unit| unit.name == "PortableBunker")
        .unwrap();
    game.catalog[kind].behavior.propaganda = Some(Propaganda {
        radius: 150.0,
        delay: 60,
        heal: [0.01, 0.02],
        upgrade: "Subliminal".into(),
        pulse: ["Pulse".into(), "UpgradedPulse".into()],
        affects_self: false,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "PropagandaTowerBehavior");
    let child = upgrade(&mut game, host);
    let second = game.spawn(kind, 0, game.entities[child].position, true);
    // Both independent native update instances must be due on this frame.
    game.tick += 1;
    game.entities[host].health = 50.0;
    game.update_ground_contacts();
    game.run_object_updates();
    let amount = 0.01 / 30.0 * game.entities[host].max_health;
    assert_eq!(
        game.entities[host].health.to_bits(),
        (50.0 + amount).to_bits()
    );
    assert_eq!(
        game.entities[host].maintenance.healing_benefactor,
        Some(child)
    );
    game.entities[host].containment.carrier = Some(second);
    game.remove_propaganda(second);
    game.tick += 1;
    game.update_ground_contacts();
    game.run_object_updates();
    assert!(game.entities[child].attachment.influence.is_empty());
}

#[test]
fn hostile_capture_transfers_the_attachment_and_orders_exit_but_contained_host_blocks_it() {
    let (mut game, host) = setup();
    let child = upgrade(&mut game, host);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(rider, host).unwrap();
    for _ in 0..1200 {
        game.step();
        if game.entities[rider].contained_by().is_some() {
            break;
        }
    }
    game.transfer_maintenance_owner(host, 1);
    assert_eq!(game.entities[child].owner, 1);
    assert!(matches!(
        game.entities[rider].ai.order,
        crate::skirmish::Order::ExitContainer
    ));
    game.step();
    assert_eq!(game.entities[rider].owner, 0);
    // TransportContain.cpp581: CRC=0 checks me->isContained() without
    // exempting portable Overlord attachments. AIExitState::update fails a
    // DOOR_NONE_AVAILABLE reservation, returning AI_EXIT to AI_IDLE.
    assert_eq!(game.entities[rider].contained_by(), Some(child));
    assert!(matches!(
        game.entities[rider].ai.order,
        crate::skirmish::Order::Idle
    ));
    assert_eq!(game.entities[host].attachment_child(), Some(child));
}

#[test]
fn missing_attachment_template_is_rejected_before_an_upgrade_can_panic() {
    let (mut game, host) = setup();
    let index = game.entities[host].type_index;
    assert!(super::validate(&game.catalog[index], &game.catalog).is_ok());
    game.catalog[index].behavior.attachments[0].object = "MissingPortableChild".into();
    assert_eq!(
        super::validate(&game.catalog[index], &game.catalog),
        Err("Missing attachment template")
    );
}

#[test]
fn propaganda_rejects_a_zero_scan_delay_and_nonfinite_healing() {
    let (mut game, host) = setup();
    let index = game.entities[host].type_index;
    game.catalog[index].behavior.propaganda = Some(Propaganda {
        radius: 150.0,
        delay: 0,
        heal: [0.01, 0.02],
        upgrade: "Subliminal".into(),
        pulse: ["Pulse".into(), "UpgradedPulse".into()],
        affects_self: false,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[index], "PropagandaTowerBehavior");
    assert_eq!(
        super::validate(&game.catalog[index], &game.catalog),
        Err("Invalid propaganda source rules")
    );
    let rules = game.catalog[index].behavior.propaganda.as_mut().unwrap();
    rules.delay = 60;
    rules.heal[0] = f32::NAN;
    assert_eq!(
        super::validate(&game.catalog[index], &game.catalog),
        Err("Invalid propaganda source rules")
    );
}

#[test]
fn damage_and_healing_notify_the_attachment_before_the_next_logic_tick() {
    let (mut game, host) = setup();
    let kind = game
        .catalog
        .iter()
        .position(|unit| unit.name == "PortableBunker")
        .unwrap();
    game.catalog[kind].behavior.propaganda = Some(Propaganda {
        radius: 150.0,
        delay: 60,
        heal: [1.0, 1.0],
        upgrade: "Subliminal".into(),
        pulse: ["Pulse".into(), "UpgradedPulse".into()],
        affects_self: false,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "PropagandaTowerBehavior");
    let child = upgrade(&mut game, host);
    // Healing must cross REALLYDAMAGED -> DAMAGED on a due native update.
    let damage = game.entities[host].max_health * 0.91;
    game.apply_hit(host, 0, damage);
    let child_maximum = game.entities[child].max_health;
    let damaged = child_maximum * 0.1;
    assert_eq!(
        game.entities[child].health.to_bits(),
        (damaged - 1.0).to_bits()
    );
    game.update_ground_contacts();
    game.tick += 1;
    let tick = game.tick;
    game.run_object_updates();
    let healed = child_maximum * 0.5;
    assert_eq!(
        game.entities[child].health.to_bits(),
        (healed - 1.0).to_bits()
    );
    assert_eq!(game.tick, tick);
}

#[test]
fn installing_on_a_damaged_host_waits_for_its_next_damage_category_change() {
    let (mut game, host) = setup();
    let damage = game.entities[host].max_health * 0.51;
    game.apply_hit(host, 0, damage);
    let child = upgrade(&mut game, host);
    assert_eq!(
        game.entities[child].health.to_bits(),
        game.entities[child].max_health.to_bits()
    );
    game.apply_hit(host, 0, 1.0);
    assert_eq!(
        game.entities[child].health.to_bits(),
        game.entities[child].max_health.to_bits()
    );
    let damage = game.entities[host].max_health * 0.39;
    game.apply_hit(host, 0, damage);
    let expected = game.entities[child].max_health * 0.1;
    assert_eq!(
        game.entities[child].health.to_bits(),
        (expected - 1.0).to_bits()
    );
}

#[test]
fn authored_initial_payload_is_created_once_without_research_or_a_queue() {
    let (mut game, old) = setup();
    let parent_type = game.entities[old].type_index;
    game.catalog[parent_type]
        .contain
        .as_mut()
        .unwrap()
        .payload_templates = vec!["PortableBunker".into()];
    game.catalog[parent_type].behavior.attachments.clear();
    assert!(super::validate(&game.catalog[parent_type], &game.catalog).is_ok());
    let cash = game.players[0].cash;
    let host = game.spawn(parent_type, 0, [455.0, 355.0], true);
    let child = game.entities[host]
        .attachment_child()
        .expect("native initial payload");
    assert_eq!(game.entities[child].attachment_parent(), Some(host));
    assert_eq!(game.entities[child].contained_by(), Some(host));
    assert_eq!(game.entities[child].owner, game.entities[host].owner);
    assert!(game.entities[host].queue.is_empty());
    assert_eq!(game.players[0].cash, cash);
    game.activate_upgrade_modules(host);
    assert_eq!(game.entities[host].attachment_child(), Some(child));
    game.entities[host].health = 0.0;
    game.finish_death(host, None);
    assert!(!game.entities[child].alive());
}

#[test]
fn native_overlord_redirection_starts_on_portable_containing_and_honors_sink_flag() {
    let (mut game, old) = setup();
    let kind = game.entities[old].type_index;
    let rules = game.catalog[kind].contain.as_mut().unwrap();
    rules.payload_templates = vec!["MissingNullableTemplate".into(), "PortableBunker".into()];
    rules.experience_sink_for_rider = false;
    let host = game.new_object(kind, 0, true);
    let child = game.entities[host].attachment_child().unwrap();
    assert_eq!(game.entities[host].containment.passengers, vec![child]);
    assert_eq!(game.redirected_container(host), child);
    assert!(game.passengers(host).is_empty());
    assert_eq!(game.entities[child].experience_sink, None);
    assert_eq!(game.entities[child].contained_by(), Some(host));
    assert!(!game.entities[child].masked());
    assert_ne!(game.object_disabled_mask(child) & (1 << 3), 0);
}

#[test]
fn authored_containment_body_edges_use_cached_state_and_native_low_level_setter() {
    let (mut game, old) = setup();
    let kind = game.entities[old].type_index;
    game.catalog[kind]
        .contain
        .as_mut()
        .unwrap()
        .payload_templates = vec!["PortableBunker".into()];
    let host = game.new_object(kind, 0, true);
    let child = game.entities[host].attachment_child().unwrap();
    let maximum = game.entities[host].max_health;
    let child_maximum = game.entities[child].max_health;
    game.apply_native_damage(
        host,
        None,
        maximum * 0.5,
        crate::skirmish::DeathType::Normal,
    );
    assert_eq!(game.body_damage_state(host), 1);
    let scaled = child_maximum * game.catalog[kind].body_damage_thresholds[0];
    assert_eq!(
        game.entities[child].health.to_bits(),
        (scaled - 1.0).to_bits()
    );
    let child_before = game.entities[child].health;
    game.apply_healing(host, None, maximum);
    assert_eq!(game.body_damage_state(host), 0);
    assert_eq!(
        game.entities[child].health.to_bits(),
        (child_maximum - 1.0).to_bits()
    );
    assert_eq!(
        game.entities[child].damage_state.previous_health.to_bits(),
        child_before.to_bits()
    );
    let events = game.presentation.object_fx_events.len();
    let random = game.random;
    game.set_body_damage_state(child, 2);
    assert_eq!(game.presentation.object_fx_events.len(), events);
    assert_eq!(game.random, random);
    assert!(game.entities[child].alive());
}

#[test]
fn direct_overlord_delete_releases_redirected_riders_before_literal_portable() {
    let (mut game, host) = setup();
    let child = upgrade(&mut game, host);
    let rider = game.spawn(5, 0, game.entities[host].position, true);
    game.board(rider, host);
    assert_eq!(game.entities[rider].contained_by(), Some(child));
    game.destroy_maintenance_object(host);
    assert!(game.entities[child].contained_by().is_none());
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.entities[child].attachment_parent().is_none());
    assert_eq!(game.redirected_container(host), host);
    assert!(game.entities[host].containment.passengers.is_empty());
    assert!(game.entities[child].containment.passengers.is_empty());
    game.process_update_destruction();
    assert!(!game.entities[host].exists());
    assert!(game.entities[child].alive());
    assert!(game.entities[rider].alive());
}

#[test]
fn selected_overlord_exit_uses_actual_redirected_membership_and_releases_only_its_rider() {
    let (mut game, parent) = setup();
    let child = upgrade(&mut game, parent);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.board(rider, parent);
    assert_eq!(game.entities[rider].contained_by(), Some(child));
    assert!(game.entities[rider].disabled_held());
    assert!(game.entities[rider].masked());
    assert_eq!(
        game.exit_container_at(rider, child),
        Err("Passenger cannot exit here")
    );
    let kind = game.entities[parent].type_index;
    let wrong_parent = game.spawn(kind, 0, [455.0, 255.0], true);
    let wrong_child = upgrade(&mut game, wrong_parent);
    assert_eq!(
        game.exit_container_at(rider, wrong_parent),
        Err("Passenger is not in this container network")
    );
    assert_eq!(
        game.entities[wrong_child].contained_by(),
        Some(wrong_parent)
    );
    game.exit_container_at(rider, parent).unwrap();
    assert_eq!(game.container_exit_target(rider), Some(parent));
    for _ in 0..10 {
        game.step();
        if game.entities[rider].contained_by().is_none() {
            break;
        }
    }
    assert!(game.entities[rider].contained_by().is_none());
    assert!(!game.entities[rider].masked());
    assert!(!game.entities[rider].disabled_held());
    assert!(!game.entities[rider].unselectable());
    assert!(game.passengers(parent).is_empty());
    assert_eq!(game.entities[child].containment.extra_slots_in_use, 0);
    assert_eq!(game.entities[parent].containment.passengers, [child]);
    assert_eq!(game.entities[child].contained_by(), Some(parent));
    assert_eq!(game.entities[parent].attachment_child(), Some(child));
    assert!(game.entities[child].disabled_held());
    assert_eq!(game.entities[rider].owner, 0);
}

#[test]
fn overlord_inventory_exit_still_rejects_nested_parent_and_lost_parent() {
    let (mut game, parent) = setup();
    let child = upgrade(&mut game, parent);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.board(rider, parent);
    let outer_kind = game.entities[child].type_index;
    game.catalog[outer_kind]
        .contain
        .as_mut()
        .unwrap()
        .allowed
        .push("VEHICLE".into());
    let outer = game.spawn(outer_kind, 0, [455.0, 255.0], true);
    game.board(parent, outer);
    assert_eq!(game.entities[parent].contained_by(), Some(outer));
    assert_eq!(
        game.exit_container_at(rider, parent),
        Err("Passenger cannot exit here")
    );
    assert_eq!(
        game.evacuate(parent),
        Err("Object is dead, unfinished or missing")
    );
    assert_eq!(game.entities[rider].contained_by(), Some(child));
    assert!(game.entities[rider].disabled_held());
    game.remove_passenger(parent, outer);
    game.kill_object(parent, crate::skirmish::DeathType::Normal);
    assert!(game.exit_container_at(rider, parent).is_err());
}

#[test]
fn overlord_ordinary_and_native_evacuate_preserve_parent_goal_and_child() {
    for native in [false, true] {
        let (mut game, parent) = setup();
        let child = upgrade(&mut game, parent);
        let rider = game.spawn(5, 0, [355.0, 335.0], true);
        game.board(rider, parent);
        assert_eq!(game.entities[rider].contained_by(), Some(child));
        assert_eq!(
            game.evacuate(child),
            Err("Object is dead, unfinished or missing")
        );
        assert_eq!(game.entities[rider].contained_by(), Some(child));
        if native {
            game.native_evacuate_from(parent, crate::skirmish::ai_goals::COMMAND_FROM_PLAYER);
        } else {
            game.evacuate(parent).unwrap();
        }
        assert_eq!(game.container_exit_target(rider), Some(parent));
        for _ in 0..10 {
            game.step();
            if game.entities[rider].contained_by().is_none() {
                break;
            }
        }
        assert!(game.entities[rider].contained_by().is_none());
        assert!(!game.entities[rider].masked());
        assert!(!game.entities[rider].disabled_held());
        assert!(!game.entities[rider].unselectable());
        assert_eq!(game.entities[child].containment.extra_slots_in_use, 0);
        assert_eq!(game.entities[parent].containment.passengers, [child]);
        assert_eq!(game.entities[child].contained_by(), Some(parent));
        assert_eq!(game.entities[parent].attachment_child(), Some(child));
        assert!(game.entities[child].disabled_held());
    }
}

#[test]
fn overlord_exit_uses_parent_exit_count_and_bones_but_removes_child_rider() {
    for (parent_paths, child_paths) in [(1, 0), (0, 1)] {
        let (mut game, original) = setup();
        let parent_kind = game.entities[original].type_index;
        let child_kind = game
            .catalog
            .iter()
            .position(|unit| unit.name == "PortableBunker")
            .unwrap();
        // Author each module's independent count and logical-bone art before
        // either object in the tested attachment graph is created.
        game.destroy_maintenance_object(original);
        game.process_update_destruction();
        let parent_rules = game.catalog[parent_kind].contain.as_mut().unwrap();
        parent_rules.exit_paths = parent_paths;
        parent_rules.door_frames = 10;
        for art in &mut parent_rules.art {
            art.exits = vec![([0.0, -45.0, 0.0], [0.0, -100.0, 0.0])];
        }
        let child_rules = game.catalog[child_kind].contain.as_mut().unwrap();
        child_rules.exit_paths = child_paths;
        child_rules.door_frames = 0;
        for art in &mut child_rules.art {
            art.exits = vec![([60.0, 0.0, 0.0], [100.0, 0.0, 0.0])];
        }
        let parent = game.spawn(parent_kind, 0, [355.0, 255.0], true);
        let child = upgrade(&mut game, parent);
        let rider = game.spawn(5, 0, [355.0, 335.0], true);
        game.board(rider, parent);
        assert_eq!(game.entities[rider].contained_by(), Some(child));
        let removal_position = game.object_position(child);
        // The authored parent bones lie on its Y axis; the distinct child
        // bone lies on its X axis. Project them from actual source transforms.
        let (parent_origin, parent_basis) = game.object_transform(parent);
        let (child_origin, child_basis) = game.object_transform(child);
        let start: [f32; 3] =
            std::array::from_fn(|axis| parent_basis[axis][1].mul_add(-45.0, parent_origin[axis]));
        let end: [f32; 3] =
            std::array::from_fn(|axis| parent_basis[axis][1].mul_add(-100.0, parent_origin[axis]));
        let child_start: [f32; 3] =
            std::array::from_fn(|axis| child_basis[axis][0].mul_add(60.0, child_origin[axis]));
        assert_ne!(start.map(f32::to_bits), child_start.map(f32::to_bits));
        game.exit_container_at(rider, parent).unwrap();
        assert_eq!(game.container_exit_target(rider), Some(parent));
        for _ in 0..10 {
            game.step();
            if game.entities[rider].contained_by().is_none() {
                break;
            }
        }
        assert!(game.entities[rider].contained_by().is_none());
        if parent_paths == 1 {
            assert_eq!(
                game.object_position(rider).map(f32::to_bits),
                start.map(f32::to_bits)
            );
            assert!(matches!(
                game.entities[rider].ai.order,
                crate::skirmish::Order::FollowPath
            ));
            assert_eq!(game.entities[rider].ai.waypoints.points, [end, end]);
            assert_eq!(game.entities[rider].ai.ignored_producer, Some(parent));
        } else {
            // A child's authored path must not activate an unconfigured parent
            // path. Native enclosed-child removal supplies the release pose.
            assert_eq!(
                game.object_position(rider).map(f32::to_bits),
                removal_position.map(f32::to_bits)
            );
            assert!(!matches!(
                game.entities[rider].ai.order,
                crate::skirmish::Order::FollowPath
            ));
            assert!(game.entities[rider].ai.waypoints.points.is_empty());
        }
        assert!(game.entities[parent].container_door_open(game.tick));
        assert!(!game.entities[child].container_door_open(game.tick));
        assert_eq!(game.entities[child].containment.extra_slots_in_use, 0);
        assert!(game.entities[child].containment.passengers.is_empty());
        assert_eq!(game.entities[parent].containment.passengers, [child]);
        assert_eq!(game.entities[parent].attachment_child(), Some(child));
        assert_eq!(game.entities[child].contained_by(), Some(parent));
        assert!(game.entities[child].disabled_held());
        assert!(!game.entities[rider].masked());
        assert!(!game.entities[rider].disabled_held());
    }
}
