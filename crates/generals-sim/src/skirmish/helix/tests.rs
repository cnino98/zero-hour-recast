use crate::skirmish::{Faction, Kind, Research, Skirmish, overlord::Upgrade, tests::fixture_type};

pub(in crate::skirmish) fn setup() -> (Skirmish, usize) {
    let (mut game, host) = crate::skirmish::containment::tests::setup(0.0, 1.0);
    let index = game.entities[host].type_index;
    // Retire the old Object against its unchanged module declarations before
    // adding constructor-owned ability instances to the replacement template.
    game.destroy_maintenance_object(host);
    game.process_update_destruction();
    let unit = &mut game.catalog[index];
    let contain = unit.contain.as_mut().unwrap();
    contain.capacity = 5;
    contain.weapons.fire = false;
    contain.allowed.push("VEHICLE".into());
    add_native_napalm(unit);
    unit.special_powers
        .push(crate::skirmish::special_power::Rules {
            tag: "NapalmAbility".into(),
            name: "NapalmPower".into(),
            power_type: "SPECIAL_HELIX_NAPALM_BOMB".into(),
            required_science: String::new(),
            timer: crate::skirmish::special_power::Timer {
                reload_frames: 300,
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
            startup: crate::skirmish::special_power::Startup {
                paused: true,
                ready: false,
            },
            script_only: false,
        });
    crate::skirmish::tests::register_behavior(unit, "SpecialAbility", 0x0100);
    unit.behavior_modules.last_mut().unwrap().tag = "NapalmAbility".into();
    unit.unpause_special_powers.push(Some("NapalmPower".into()));
    crate::skirmish::tests::register_behavior(unit, "PassengersFireUpgrade", 0x80);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Bunker".into()];
    unit.behavior
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
                names: vec!["HelixBunker".into()],
                count: 1,
                offset: [0.0; 3],
                disposition: crate::skirmish::physical_ocl::ON_GROUND,
                health: [1.0; 2],
                lifetime: [0; 2],
                flags: crate::skirmish::object_creation::CONTAIN_INSIDE_SOURCE
                    | crate::skirmish::object_creation::PRESERVE_LAYER,
            }],
        }));
    crate::skirmish::tests::register_behavior(unit, "ObjectCreationUpgrade", 0x80);
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Bunker".into()];
    unit.behavior.attachments = vec![Upgrade {
        trigger: "Bunker".into(),
        conflicts: vec!["Gattling".into()],
        object: "HelixBunker".into(),
        command_set: "BunkerCommands".into(),
    }];
    unit.production = Some(Vec::new());
    crate::skirmish::tests::register_update(unit, "ProductionUpdate");
    unit.maintenance.research.push(Research {
        name: "Bunker".into(),
        cost: 400,
        frames: 1,
        player: false,
    });
    let mut child = fixture_type(Faction::Gla, Kind::Vehicle);
    child.name = "HelixBunker".into();
    child.kind_of = vec!["PORTABLE_STRUCTURE".into()];
    child.attachment_bone = Some("CHASSISBONE".into());
    child.transport_slots = 1;
    child.speed = 0.0;
    game.catalog.push(child);
    let mut bomb = fixture_type(Faction::China, Kind::Effect);
    bomb.name = "NapalmBomb".into();
    bomb.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    bomb.update_modules.clear();
    bomb.behavior_modules.clear();
    game.catalog.push(bomb);
    let host = crate::skirmish::containment::tests::respawn_contain_family(
        &mut game,
        host,
        "HelixContain",
    );
    game.tick = 1;
    (game, host)
}

#[test]
fn helix_attachment_does_not_consume_a_passenger_slot_or_redirect_bunker_inventory() {
    let (mut game, host) = setup();
    game.queue_upgrade(host, 0).unwrap();
    game.step();
    let child = game.entities[host].attachment_child().unwrap();
    assert_eq!(game.entities[child].contained_by(), Some(host));
    assert!(game.passengers(host).is_empty());
    assert_eq!(game.redirected_container(host), host);
    for _ in 0..5 {
        let rider = game.spawn(5, 0, game.entities[host].position, true);
        game.board(rider, host);
        assert_eq!(game.entities[rider].contained_by(), Some(host));
    }
    let sixth = game.spawn(5, 0, game.entities[host].position, true);
    assert!(!game.can_enter(sixth, host));
    assert_eq!(game.passengers(host).len(), 5);
    assert!(!game.passengers(host).contains(&child));
    assert!(!game.entities[child].garrison_weapon_bonus());
}

#[test]
fn helix_passengers_gain_bonus_before_bunker_but_only_infantry_can_fire_after_it() {
    let (mut game, host) = setup();
    let infantry = game.spawn(5, 0, game.entities[host].position, true);
    game.board(infantry, host);
    assert!(game.entities[infantry].garrison_weapon_bonus());
    assert!(!game.entities[infantry].garrisoned());
    assert!(!game.passenger_can_fire(infantry));
    game.queue_upgrade(host, 0).unwrap();
    game.step();
    assert!(game.passenger_can_fire(infantry));
    let mut vehicle = fixture_type(Faction::Gla, Kind::Vehicle);
    vehicle.kind_of = vec!["VEHICLE".into()];
    vehicle.transport_slots = 1;
    game.catalog.push(vehicle);
    let vehicle = game.spawn(
        game.catalog.len() - 1,
        0,
        game.entities[host].position,
        true,
    );
    game.board(vehicle, host);
    assert_eq!(game.entities[vehicle].contained_by(), Some(host));
    assert!(!game.passenger_can_fire(vehicle));
    game.remove_passenger(infantry, host);
    assert!(!game.entities[infantry].garrison_weapon_bonus());
}

#[test]
fn helix_logical_attachment_and_passengers_follow_distinct_original_heights() {
    let (mut game, host) = setup();
    game.queue_upgrade(host, 0).unwrap();
    game.step();
    let child = game.entities[host].attachment_child().unwrap();
    let rider = game.spawn(5, 0, game.entities[host].position, true);
    game.board(rider, host);
    game.entities[host].position = [410.0, 370.0];
    game.entities[host].facing = 0.7;
    game.entities[host].ai.turret.angle = 1.0;
    game.update_attachments();
    game.deploy_helix_passengers(host);
    let position = game.object_position(host);
    assert_eq!(
        game.object_position(child).map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(game.entities[child].facing.to_bits(), 0.7_f32.to_bits());
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [position[0], position[1], position[2] + 8.0].map(f32::to_bits)
    );
    game.entities[host].health = 0.0;
    game.finish_death(host, None);
    assert!(!game.entities[child].alive());
    assert!(!game.entities[rider].alive());
}

#[test]
fn native_helix_portable_link_bypasses_ordinary_list_and_sound_callbacks() {
    let (mut game, host) = setup();
    let child_type = game
        .catalog
        .iter()
        .position(|unit| unit.name == "HelixBunker")
        .unwrap();
    let child = game.new_object(child_type, 0, true);
    let original_position = game.object_position(child);
    game.presentation.container_sounds.clear();
    game.add_to_contain(child, host);
    assert_eq!(game.entities[child].contained_by(), Some(host));
    assert!(game.passengers(host).is_empty());
    assert!(game.presentation.container_sounds.is_empty());
    assert_eq!(game.object_disabled_mask(child) & (1 << 3), 0);
    assert!(!game.entities[child].garrison_weapon_bonus());
    assert_eq!(
        game.object_position(child).map(f32::to_bits),
        original_position.map(f32::to_bits)
    );
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(
        game.object_position(child).map(f32::to_bits),
        game.object_position(host).map(f32::to_bits)
    );
}

#[test]
fn napalm_upgrade_unpauses_native_timer_and_disable_freezes_recharge() {
    let (mut game, host) = setup();
    assert!(!game.helix_bomb_ready(host));
    game.tick = 20;
    game.apply_unpause_special_power_upgrade(host, 0);
    assert_eq!(game.helix_bomb_ready_frame(host), Some(320));
    game.tick = 40;
    game.set_hacked_until(host, crate::skirmish::ai_dispatch::native_frame(70));
    game.observe_spy_vision_disabled(host);
    let progress = game.helix_bomb_percent(host).unwrap();
    game.tick = 60;
    assert_eq!(
        game.helix_bomb_percent(host).unwrap().to_bits(),
        progress.to_bits()
    );
    game.tick = 70;
    game.clear_hacked_disable(host);
    game.observe_spy_vision_disabled(host);
    assert_eq!(game.helix_bomb_ready_frame(host), Some(350));
    game.tick = 350;
    assert!(game.helix_bomb_ready(host));
}

fn add_native_napalm(unit: &mut crate::skirmish::UnitType) {
    use crate::skirmish::special_ability::{
        Approach, Effects, Feedback, Objects, Packing, Power, PowerType, Preparation, Rules,
        Timings,
    };
    unit.behavior.special_abilities.push(Rules {
        power: Some(Power {
            name: "NapalmPower".into(),
            kind: PowerType::SpecialHelixNapalmBomb,
        }),
        approach: Approach {
            start_range: 3.0,
            abort_range: 10_000_000.0,
            requires_los: false,
            face_target: false,
            flee_range: 0.0,
        },
        timing: Timings {
            preparation: 0,
            persistent: 3,
            pack: 0,
            unpack: 15,
            unstealth: 0,
            effect: 0,
            variation: 0.0,
        },
        objects: Objects {
            name: "NapalmBomb".into(),
            bone: String::new(),
            maximum: 1,
            persistent: true,
            unique_targets: false,
            persist_on_death: true,
        },
        packing: Packing {
            skip_no_target: false,
            flip_after_pack: false,
            flip_after_unpack: false,
        },
        preparation: Preparation {
            always_validate: false,
            capture_fx: false,
            persistence_recharges: true,
        },
        effects: Effects {
            value: 0,
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
    crate::skirmish::tests::register_update(unit, "SpecialAbilityUpdate");
}

#[test]
fn direct_helix_delete_destroys_portable_without_ordinary_removal_callbacks() {
    let (mut game, host) = setup();
    game.queue_upgrade(host, 0).unwrap();
    game.step();
    let portable = game.entities[host].attachment_child().unwrap();
    let original = game.object_position(portable);
    let sounds = game.presentation.container_sounds.len();
    game.destroy_maintenance_object(host);
    assert!(game.entities[portable].contained_by().is_none());
    assert!(game.entities[host].attachment_child().is_none());
    assert_eq!(
        game.object_position(portable).map(f32::to_bits),
        original.map(f32::to_bits)
    );
    assert_eq!(game.presentation.container_sounds.len(), sounds);
    game.process_update_destruction();
    assert!(!game.entities[host].exists());
    assert!(!game.entities[portable].exists());
}
