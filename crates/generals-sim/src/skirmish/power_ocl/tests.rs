use super::*;
use crate::skirmish::{
    Faction, Kind, deliver_payload, module_updates::ModuleRegistration, physical_ocl,
    special_power, tests,
};

fn owner_location_fixture(update_starts_attack: bool) -> (Skirmish, usize, usize, usize) {
    let mut game = tests::game();
    let mut carrier = tests::fixture_type(Faction::China, Kind::Vehicle);
    carrier.name = "DeliveryCarrier".into();
    carrier.update_modules.retain(|module| {
        !matches!(module, ModuleRegistration::Behavior {family, ..} if family == "AIUpdateInterface")
    });
    carrier
        .behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    carrier.deliver_payload = Some(deliver_payload::TransportRules {
        door_delay: 0,
        exit_bone: String::new(),
        exit_pitch_rate: 0.0,
        dive_sound: String::new(),
        strafe_effects: Vec::new(),
    });
    tests::register_update(&mut carrier, "DeliverPayloadAIUpdate");
    tests::register_behavior(&mut carrier, "OCLSpecialPower", 0x0100);
    let tag = carrier.behavior_modules.last().unwrap().tag.clone();
    carrier.special_powers.push(special_power::Rules {
        tag,
        name: "FixturePower".into(),
        power_type: "SPECIAL_ARTILLERY_BARRAGE".into(),
        required_science: String::new(),
        timer: special_power::Timer {
            reload_frames: 60,
            shared: false,
            public: false,
        },
        view_range: 0.0,
        view_frames: 0,
        view_template: None,
        initiate_sound: String::new(),
        target_sound: String::new(),
        module_initiate_sound: String::new(),
        academy_classification: "ACT_SUPERPOWER".into(),
        update_starts_attack,
        startup: special_power::Startup {
            paused: false,
            ready: true,
        },
        script_only: false,
    });
    let mut marker = tests::fixture_type(Faction::China, Kind::Infantry);
    marker.name = "UniqueOclMarker".into();
    let marker_name = marker.name.clone();
    let marker_kind = game.catalog.len();
    game.catalog.push(marker);
    let carrier_kind = game.catalog.len();
    carrier.behavior.ocl_powers.push(Rules {
        power: "FixturePower".into(),
        location: Location::Owner,
        adjust_position: false,
        reference: String::new(),
        default: List {
            name: "FixtureList".into(),
            nuggets: vec![
                Nugget::Objects(object_creation::List {
                    name: "FixtureList".into(),
                    objects: vec![object_creation::Object {
                        names: vec![marker_name],
                        count: 1,
                        offset: [0.0; 3],
                        disposition: physical_ocl::LIKE_EXISTING,
                        health: [1.0; 2],
                        lifetime: [0; 2],
                        flags: 0,
                        container: String::new(),
                        invulnerable_time: 0,
                        force_intensity: 0.0,
                        force_magnitude: [0.0; 2],
                        force_pitch: [0.0; 2],
                        spin_rate: -1.0,
                        formation_distances: [0.0; 3],
                        fade_frames: 0,
                        fade_sound: String::new(),
                    }],
                }),
                Nugget::Delivery("FixtureDelivery".into()),
            ],
        },
        upgrades: Vec::new(),
    });
    game.catalog.push(carrier);
    game.configure_delivery_lists(vec![delivery_rules()])
        .unwrap();
    let provider = game.spawn(carrier_kind, 0, [200.0, 250.0], true);
    (game, provider, carrier_kind, marker_kind)
}

fn delivery_rules() -> deliver_payload::Rules {
    deliver_payload::Rules {
        name: "FixtureDelivery".into(),
        transport: "DeliveryCarrier".into(),
        payload: Vec::new(),
        container: String::new(),
        transport_start: deliver_payload::TransportStart {
            preferred_height: false,
            max_speed: false,
            delay_delivery_max: 0,
        },
        release: deliver_payload::Release {
            drop_offset: [0.0; 3],
            drop_delay: 0,
            parachute_directly: false,
            drop_variance: [0.0; 3],
            inherit_velocity: false,
            exit_pitch_rate: 0.0,
            fire_weapon: false,
        },
        attempts: 1,
        delivery_distance: 0.0,
        pre_open_distance: 0.0,
        formation_size: 1,
        formation_spacing: 25.0,
        convergence: 0.0,
        error_radius: 0.0,
        self_destruct: false,
        visible: None,
        dive: None,
        decal: None,
    }
}

#[test]
fn owner_location_with_angle_uses_real_overload_and_creates_delivery_transport() {
    let (mut game, provider, carrier_kind, marker_kind) = owner_location_fixture(false);
    let point = [350.0, 300.0, 0.0];
    let angle = 1.25_f32;
    let first = game.entities.len();
    game.execute_power_ocl(provider, 0, point, Some(angle));
    let created = &game.entities[first..];
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].type_index, marker_kind);
    assert_eq!(created[0].facing.to_bits(), angle.to_bits());
    assert_eq!(created[1].type_index, carrier_kind);
    assert_eq!(created[1].producer, Some(provider));
    assert_eq!(
        created[1].position.map(f32::to_bits),
        [350.0_f32, 300.0].map(f32::to_bits)
    );
}

#[test]
fn targetless_ocl_uses_bool_overload_and_keeps_existing_delivery_transport() {
    let (mut game, provider, _, marker_kind) = owner_location_fixture(false);
    let first = game.entities.len();
    game.execute_power_ocl_without_target(provider, 0);
    let created = &game.entities[first..];
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].type_index, marker_kind);
    assert_eq!(
        created[0].position.map(f32::to_bits),
        game.entities[provider].position.map(f32::to_bits)
    );
}

#[test]
fn update_started_power_keeps_native_ocl_creation_but_waits_to_trigger_and_recharge() {
    for updates_start in [false, true] {
        let (mut game, provider, _, marker_kind) = owner_location_fixture(updates_start);
        assert!(game.special_power_ready(provider, "FixturePower"));
        let first = game.entities.len();
        assert!(game.native_special_power(
            provider,
            "FixturePower",
            None,
            Some([350.0, 300.0, 0.0]),
            false
        ));
        assert_eq!(game.entities[first].type_index, marker_kind);
        assert_eq!(game.players[0].academy.special_powers_used, 1);
        assert_eq!(
            game.special_power_triggers.len(),
            usize::from(!updates_start)
        );
        assert_eq!(
            game.special_power_remaining(provider, "FixturePower"),
            if updates_start { 0 } else { 60 }
        );
    }
}

#[test]
fn attack_nugget_does_not_lock_an_absent_weapon_slot() {
    let (mut game, provider, kind, _) = owner_location_fixture(false);
    game.catalog[kind].behavior.ocl_powers[0]
        .default
        .nuggets
        .push(Nugget::Attack {
            shots: 9,
            slot: 2,
            decal: crate::skirmish::radius_decal::Template {
                texture: String::new(),
                opacity: [0.0; 2],
                throb_frames: 0,
                color: [0; 4],
                only_owner: true,
                additive: false,
            },
            radius: 0.0,
        });
    assert!(!game.weapon_slot_present(provider, 2));
    game.execute_power_ocl(provider, 0, [350.0, 300.0, 0.0], None);
    assert!(matches!(
        game.entities[provider].weapon_set.lock,
        crate::skirmish::weapons::Lock::None
    ));
    assert_eq!(game.entities[provider].weapon_set.current, 0);
}
