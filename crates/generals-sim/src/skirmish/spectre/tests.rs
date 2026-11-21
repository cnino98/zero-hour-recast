use super::*;
use crate::skirmish::{
    Kind, MovementSurface, flight,
    tests::{game, register_behavior, register_update},
};

fn power() -> crate::skirmish::special_power::Rules {
    crate::skirmish::special_power::Rules {
        tag: "SpectrePower".into(),
        name: "Spectre".into(),
        power_type: "SPECIAL_SPECTRE_GUNSHIP".into(),
        required_science: String::new(),
        timer: crate::skirmish::special_power::Timer {
            reload_frames: 60,
            shared: true,
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
            paused: false,
            ready: false,
        },
        script_only: false,
    }
}

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = game();
    let ship = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[ship];
    unit.name = "Ship".into();
    unit.locomotor.ground = None;
    unit.locomotor.flight = Some(flight::tests::rules(flight::Appearance::Wings));
    unit.locomotor.legal_surfaces = 8;
    unit.movement_surface = MovementSurface::Air;
    unit.preferred_height = 100.0;
    unit.special_powers.push(power());
    register_behavior(unit, "SpecialAbility", 0x0100);
    unit.behavior_modules.last_mut().unwrap().tag = "SpectrePower".into();
    unit.spectre = Some(Rules {
        power: "Spectre".into(),
        gunner: String::new(),
        howitzer: None,
        orbit_frames: 30,
        firing_rate: 10,
        follow_lag: 0,
        attack_radius: 200.0,
        reticle_radius: 25.0,
        orbit_radius: 250.0,
        strafe_increment: 20.0,
        insertion_slope: 0.7,
        howitzer_offset: 20.0,
        strafe_particle: String::new(),
        decals: std::array::from_fn(|_| Decal {
            texture: "radius".into(),
            opacity: [0.3, 0.8],
            throb_frames: 30,
            color: [0; 4],
            only_owner: true,
            additive: false,
        }),
        afterburner_sound: String::new(),
        howitzer_sound: String::new(),
        attack_voice: String::new(),
    });
    register_update(unit, "SpectreGunshipUpdate");
    let host = game.type_for(0, Kind::CommandCenter).unwrap();
    let unit = &mut game.catalog[host];
    unit.special_powers.push(power());
    register_behavior(unit, "SpecialAbility", 0x0100);
    unit.behavior_modules.last_mut().unwrap().tag = "SpectrePower".into();
    unit.spectre_deployments.push(DeploymentRules {
        power: "Spectre".into(),
        required_science: String::new(),
        gunship: "Ship".into(),
        attack_radius: 200.0,
        edge: CreationEdge::NearTarget,
    });
    register_update(unit, "SpectreGunshipDeploymentUpdate");
    let host = game.spawn(host, 0, [200.0; 2], true);
    (game, host, ship)
}

#[test]
fn deployment_keeps_allocator_zero_radius_create_pose_and_native_three_trigger_order() {
    let (mut game, host, kind) = fixture();
    game.playable_extent = [[0.0; 2], [1000.0; 2]];
    game.cast_power(
        host,
        "Spectre",
        crate::skirmish::PowerTarget::Position3D([300.0, 300.0, 0.0]),
    )
    .unwrap();
    let ship = game.entities[host].behaviors.spectre_deployments[0]
        .gunship
        .unwrap();
    assert_eq!(game.entities[ship].type_index, kind);
    assert_eq!(game.entities[ship].producer, Some(host));
    assert_eq!(
        game.object_position(ship).map(f32::to_bits),
        [300.0_f32, 0.0, 100.0].map(f32::to_bits)
    );
    let state = game.entities[ship].spectre.as_ref().unwrap();
    // CREATE runs before the deployment supplies its absolute flight altitude.
    assert_eq!(state.satellite[2].to_bits(), 0.0_f32.to_bits());
    assert!(game.entities[ship].spectre_afterburners());
    assert_eq!(
        state.decals.positions[0].map(f32::to_bits),
        [300.0_f32, 0.0, 100.0].map(f32::to_bits)
    );
    assert_eq!(
        game.special_power_triggers
            .iter()
            .map(|event| event.object)
            .collect::<Vec<_>>(),
        [ship, ship, host]
    );
    assert_eq!(game.spectre_selections, [ship]);
}

#[test]
fn script_intent_retains_old_aim_and_orbit_clamps_xy_using_three_dimensional_distance() {
    let (mut game, _, kind) = fixture();
    let ship = game.spawn(kind, 0, [100.0; 2], true);
    game.initiate_spectre(ship, "Spectre", [100.0, 100.0, 5.0], false);
    game.initiate_spectre(ship, "Spectre", [300.0, 100.0, 105.0], true);
    let state = game.entities[ship].spectre.as_ref().unwrap();
    assert_eq!(
        state.gunner_target.map(f32::to_bits),
        [100.0_f32, 100.0, 5.0].map(f32::to_bits)
    );
    game.tick = 1;
    game.update_spectre(ship);
    let state = game.entities[ship].spectre.as_ref().unwrap();
    assert!(matches!(state.status, Status::Orbiting));
    assert_eq!(state.escape_frame, 31);
    assert_eq!(state.override_target[2].to_bits(), 5.0_f32.to_bits());
    assert!(state.override_target[0] > 125.0 && state.override_target[0] < 150.0);
    assert_eq!(game.entities[ship].spectre_doors_open(), Some(true));
    assert!(!game.entities[ship].spectre_afterburners());
    game.tick = 31;
    game.update_spectre(ship);
    assert!(matches!(
        game.entities[ship].spectre.as_ref().unwrap().status,
        Status::Departing
    ));
    assert!(
        game.entities[ship]
            .spectre_decals(game.catalog[kind].spectre.as_ref().unwrap())
            .is_none()
    );
    assert!(game.entities[ship].spectre_afterburners());
}

#[test]
fn native_retarget_preserves_height_and_disabled_callback_ignores_the_command() {
    let (mut game, host, _) = fixture();
    game.cast_power(
        host,
        "Spectre",
        crate::skirmish::PowerTarget::Position3D([300.0, 300.0, 17.0]),
    )
    .unwrap();
    let ship = game.entities[host].behaviors.spectre_deployments[0]
        .gunship
        .unwrap();
    assert_eq!(game.power_cooldown_remaining(host, "Spectre"), 60);
    assert!(!game.can_use_power(host, "Spectre"));
    assert!(game.override_special_power_destination(ship, [320.0, 280.0, 29.0], 0));
    assert_eq!(
        game.entities[ship]
            .spectre
            .as_ref()
            .unwrap()
            .override_target
            .map(f32::to_bits),
        [320.0_f32, 280.0, 29.0].map(f32::to_bits)
    );
    game.entities[ship].disabled_status |= 1 << 2;
    assert!(game.override_special_power_destination(ship, [330.0, 290.0, 35.0], 0));
    assert_eq!(
        game.entities[ship]
            .spectre
            .as_ref()
            .unwrap()
            .override_target
            .map(f32::to_bits),
        [320.0_f32, 280.0, 29.0].map(f32::to_bits)
    );
    game.entities[ship].spectre.as_mut().unwrap().status = Status::Departing;
    assert!(!game.override_special_power_destination(ship, [330.0, 290.0, 35.0], 0));
}
