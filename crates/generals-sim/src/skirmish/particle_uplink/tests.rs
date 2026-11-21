use super::*;
use crate::skirmish::{
    Faction, Kind, PowerTarget, special_power,
    tests::{fixture_type, game, register_behavior, register_update},
};

fn fixture() -> (Skirmish, usize, usize, usize) {
    let mut game = game();
    let mut producer = fixture_type(Faction::Usa, Kind::TechBuilding);
    producer.name = "Particle Uplink".into();
    producer.special_powers.push(special_power::Rules {
        tag: "ParticleFire".into(),
        name: "Particle".into(),
        power_type: "SPECIAL_PARTICLE_UPLINK_CANNON".into(),
        required_science: String::new(),
        timer: special_power::Timer {
            reload_frames: 7200,
            shared: false,
            public: true,
        },
        view_range: 0.0,
        view_frames: 0,
        view_template: None,
        initiate_sound: String::new(),
        target_sound: String::new(),
        module_initiate_sound: String::new(),
        academy_classification: String::new(),
        update_starts_attack: true,
        startup: special_power::Startup {
            paused: false,
            ready: true,
        },
        script_only: false,
    });
    producer.behavior.particle_uplinks.push(Rules {
        power: "Particle".into(),
        charge_frames: 150,
        antenna_frames: 140,
        ready_delay: 60,
        firing_frames: 300,
        growth_frames: 60,
        travel_frames: 75,
        damage_per_second: 400.0,
        damage_pulses: 40,
        damage_type: DamageType::ParticleBeam,
        death_type: DeathType::Lasered,
        damage_radius_scale: 3.4,
        scorch_marks: 20,
        scorch_radius_scale: 2.4,
        reveal_range: 50.0,
        orbital_radius: 13.0,
        launch_fx_interval: 30,
        ground_hit_fx: String::new(),
        beam_launch_fx: String::new(),
        swath_distance: 200.0,
        swath_amplitude: 50.0,
        driving_speed: 20.0,
        fast_driving_speed: 40.0,
        double_click_frames: 15,
        remnant: None,
        sounds: std::array::from_fn(|_| String::new()),
        effects: Effects {
            outer_bone: String::new(),
            outer_bone_count: 0,
            connector_bone: String::new(),
            fire_bone: String::new(),
            outer_flares: std::array::from_fn(|_| String::new()),
            connector_lasers: std::array::from_fn(|_| String::new()),
            connector_flares: std::array::from_fn(|_| String::new()),
            base_flares: std::array::from_fn(|_| String::new()),
            orbital_laser: String::new(),
        },
    });
    register_behavior(&mut producer, "SpecialAbility", 0x0100);
    producer.behavior_modules.last_mut().unwrap().tag = "ParticleFire".into();
    register_update(&mut producer, "ParticleUplinkCannonUpdate");
    let kind = game.catalog.len();
    game.catalog.push(producer);
    let source = game.spawn(kind, 0, [175.0, 125.0], true);
    let kind = game.type_for(1, Kind::Tank).unwrap();
    let enemy = game.spawn(kind, 1, [225.0, 140.0], true);
    let allied = game.spawn(kind, 0, [225.0, 146.0], true);
    for target in [enemy, allied] {
        game.entities[target].health = 4000.0;
        game.entities[target].max_health = 4000.0;
    }
    game.update_ground_contacts();
    (game, source, enemy, allied)
}

#[test]
fn controlled_beam_waits_for_orbit_then_damages_friendly_and_enemy_bodies() {
    let (mut game, source, enemy, allied) = fixture();
    game.tick = 100;
    let target = game.object_position(enemy);
    game.particle_uplink_reveal(0, [target[0], target[1]], 50.0);
    game.cast_power(source, "Particle", PowerTarget::Position3D(target))
        .unwrap();
    game.update_particle_uplink(source, 0);
    assert!(game.particle_uplink_destination_active(source, "Particle"));
    let health = game.entities[enemy].health;
    let friendly_health = game.entities[allied].health;
    for frame in 101..175 {
        game.tick = frame;
        game.update_particle_uplink(source, 0);
    }
    assert_eq!(game.entities[enemy].health.to_bits(), health.to_bits());
    assert!(game.presentation.particle_uplink_scorches.is_empty());
    for frame in 175..230 {
        game.tick = frame;
        game.update_particle_uplink(source, 0);
    }
    assert!(game.entities[enemy].health < health);
    assert!(game.entities[allied].health < friendly_health);
    let steer = [target[0] + 300.0, target[1], target[2]];
    assert!(!game.can_override_special_power_destination(source, steer));
    game.particle_uplink_reveal(0, [steer[0], steer[1]], 20.0);
    assert!(game.can_override_special_power_destination(source, steer));
    assert!(game.override_special_power_destination(source, steer, 0));
    game.tick += 1;
    game.update_particle_uplink(source, 0);
    let position = game
        .particle_uplink_visuals(source)
        .next()
        .unwrap()
        .target
        .unwrap()
        .position;
    assert!((position[0] - (target[0] + 20.0 / 30.0)).abs() < 0.001);
    assert!(game.power_cooldown_remaining(source, "Particle") > 6000);
}

#[test]
fn emp_aborts_source_immediately_but_orbital_beam_preserves_travel_and_decay() {
    let (mut game, source, enemy, _) = fixture();
    game.tick = 100;
    let target = game.object_position(enemy);
    game.particle_uplink_reveal(0, [target[0], target[1]], 50.0);
    game.cast_power(source, "Particle", PowerTarget::Position3D(target))
        .unwrap();
    for frame in 100..200 {
        game.tick = frame;
        game.update_particle_uplink(source, 0);
    }
    game.tick = 200;
    game.set_emp_disabled_until(source, 500);
    game.update_particle_uplink(source, 0);
    let previous = game.entities[source].behaviors.particle_uplinks[0].destination;
    let other = [target[0], target[1] + 10.0, target[2]];
    assert!(game.override_special_power_destination(source, other, 0));
    assert_eq!(
        game.entities[source].behaviors.particle_uplinks[0]
            .destination
            .map(f32::to_bits),
        previous.map(f32::to_bits)
    );
    assert!(game.particle_uplink_visuals(source).next().unwrap().stage == Stage::Postfire);
    for frame in 201..335 {
        game.tick = frame;
        game.update_particle_uplink(source, 0);
    }
    assert!(
        game.particle_uplink_visuals(source)
            .next()
            .unwrap()
            .target
            .is_some()
    );
    game.tick = 335;
    game.update_particle_uplink(source, 0);
    assert!(
        game.particle_uplink_visuals(source)
            .next()
            .unwrap()
            .target
            .is_none()
    );
    assert!(!game.particle_uplink_destination_active(source, "Particle"));
}
