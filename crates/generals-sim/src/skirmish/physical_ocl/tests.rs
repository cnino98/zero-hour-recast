use super::*;
use crate::skirmish::{
    Faction, Kind,
    tests::{fixture_type, game},
};

mod active_body;
mod collisions;

#[test]
fn motive_friction_reprojects_lateral_force_at_diagonal_yaw() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    let id = game.new_object(kind, 0, true);
    game.set_object_transform(id, [300.0, 300.0, 0.0], transform::yaw(0.73));
    let motion = &mut game.entities[id].physical.as_mut().unwrap().motion;
    motion.mass = 40.0;
    motion.velocity = [17.1, -4.7, 0.0];
    let mass = motion.mass;
    physics::integrate(motion, &rules().motion, 0.0, false, false, true, mass);
    // SH PhysicsUpdate.cpp applyFrictionalForces -> applyForce, IEEE float
    // scalar fixture compiled without floating-point contraction.
    assert_eq!(motion.velocity[0].to_bits(), 0x4179_be55);
    assert_eq!(motion.velocity[1].to_bits(), 0xc042_2b38);
}

#[test]
fn downward_rest_contact_keeps_integrated_pose_when_bounce_force_is_zero() {
    for roll in [0.3, std::f32::consts::PI - 0.3] {
        let (mut game, _) = fixture();
        let kind = game.catalog.len() - 1;
        let mut basis = transform::yaw(0.4);
        transform::rotate(&mut basis, [roll, 0.2, 0.0]);
        let id = game.new_object(kind, 0, true);
        game.set_object_transform(id, [300.0, 300.0, 0.0], basis);
        let motion = &mut game.entities[id].physical.as_mut().unwrap().motion;
        motion.velocity[2] = -1.0;
        let force = motion.bounce(&rules().motion, 0.0, 0.0, basis, motion.mass);
        assert_eq!(force.to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            motion.basis.map(|row| row.map(f32::to_bits)),
            basis.map(|row| row.map(f32::to_bits))
        );
    }
}

#[test]
fn special_ability_initializes_pitch_from_the_created_bomb_variation() {
    let (mut game, host) = crate::skirmish::helix::tests::setup();
    let concrete = game
        .catalog
        .iter_mut()
        .find(|unit| unit.name == "NapalmBomb")
        .unwrap();
    concrete.name = "PhysicalBomb".into();
    let mut physics = rules();
    physics.motion.center_of_mass_offset = 0.13;
    concrete.physical = Some(physics);
    crate::skirmish::tests::register_update(concrete, "PhysicsBehavior");
    let mut wrapper = fixture_type(Faction::China, Kind::Effect);
    wrapper.name = "NapalmBomb".into();
    wrapper.variations.push("PhysicalBomb".into());
    game.catalog.push(wrapper);
    game.apply_unpause_special_power_upgrade(host, 0);
    game.set_special_power_ready_frame(host, "NapalmPower", game.update_frame());
    game.helix_bomb(host, game.entities[host].position).unwrap();
    for _ in 0..20 {
        game.step();
        if game.entities[host].helix_bomb_object().is_some() {
            break;
        }
    }
    let bomb = game.entities[host].helix_bomb_object().unwrap();
    assert_eq!(
        game.catalog[game.entities[bomb].type_index].name,
        "PhysicalBomb"
    );
    let motion = &game.entities[bomb].physical.as_ref().unwrap().motion;
    assert_eq!(motion.rates[1].to_bits(), 0.13_f32.to_bits());
}

#[test]
fn newborn_hull_and_bomb_wait_one_frame_then_apply_airborne_drag() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    game.catalog[kind]
        .physical
        .as_mut()
        .unwrap()
        .motion
        .aerodynamic_friction = 0.125;
    let hull = game.new_object(kind, 0, true);
    game.set_object_transform(hull, [300.0, 300.0, 100.0], transform::yaw(0.0));
    let bomb = game.new_object(kind, 0, true);
    game.set_object_transform(bomb, [400.0, 300.0, 100.0], transform::yaw(0.0));
    game.entities[bomb].set_physics_pitch_rate(0.25);
    game.entities[bomb].set_physics_airborne_friction(false);
    for id in [hull, bomb] {
        game.entities[id].physical.as_mut().unwrap().motion.velocity = [2.0, 4.0, -2.0];
        let position = game.object_position(id);
        game.integrate_physical_object(id);
        assert_eq!(
            game.object_position(id).map(f32::to_bits),
            position.map(f32::to_bits)
        );
    }
    assert_eq!(
        game.entities[bomb].physical.as_ref().unwrap().motion.rates[1].to_bits(),
        0.25_f32.to_bits()
    );
    game.tick += 1;
    for id in [hull, bomb] {
        game.integrate_physical_object(id);
        let state = game.entities[id].physical.as_ref().unwrap();
        // Source aerodynamic drag acts on all three pre-step velocity axes;
        // gravity contributes separately to the downward acceleration.
        let expected = [1.75, 3.5, -2.0_f32 + (0.25 - 0.1)];
        assert_eq!(
            state.motion.velocity.map(f32::to_bits),
            expected.map(f32::to_bits)
        );
        assert!(state.motion.position[2] < 100.0);
    }
    let special = game.entities[bomb].physical.as_ref().unwrap();
    assert_eq!(special.motion.rates[1].to_bits(), 0.21875_f32.to_bits());
}

#[test]
fn height_die_is_strict_and_delivers_each_matching_instant_effect_once() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    let rules = game.catalog[kind].physical.as_mut().unwrap();
    rules.death = Death::Instant;
    game.catalog[kind].behavior.instant_death = [
        (1 << super::super::DeathType::Normal as u32, "FX_BombNormal"),
        (1 << super::super::DeathType::Crushed as u32, "FX_Crushed"),
        (u32::MAX, "FX_BombAnyDeath"),
    ]
    .into_iter()
    .map(
        |(death_mask, effect)| crate::skirmish::instant_death::Rules {
            death_mask,
            veterancy_mask: 15,
            required_status: 0,
            exempt_status: 0,
            effects: vec![Some(effect.into())],
            objects: Vec::new(),
            weapons: Vec::new(),
        },
    )
    .collect();
    game.catalog[kind]
        .behavior
        .height_die
        .push(crate::skirmish::height_die::Rules {
            target_height: 1.0,
            includes_structures: false,
            only_moving_down: false,
            particle_height: -1.0,
            snap_to_ground: false,
            initial_delay: 0,
        });
    crate::skirmish::tests::register_update(&mut game.catalog[kind], "HeightDieUpdate");
    for _ in 0..game.catalog[kind].behavior.instant_death.len() {
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[kind],
            "InstantDeathBehavior",
            2,
        );
    }
    let bomb = game.new_object(kind, 0, true);
    game.set_object_transform(bomb, [300.0, 300.0, 1.0], transform::yaw(0.0));
    game.entities[bomb].set_physics_pitch_rate(0.0);
    game.entities[bomb].set_physics_airborne_friction(false);
    game.run_object_updates();
    assert!(game.entities[bomb].physical_present());
    assert!(game.presentation.object_fx_events.is_empty());
    game.tick += 1;
    game.entities[bomb]
        .physical
        .as_mut()
        .unwrap()
        .motion
        .position[2] = 0.99;
    game.run_object_updates();
    assert!(game.update_objects[game.entities[bomb].update_owner].destroy_requested);
    assert!(game.entities[bomb].exists());
    game.process_update_destruction();
    assert!(!game.entities[bomb].physical_present());
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .map(|event| event.effect.as_str())
            .collect::<Vec<_>>(),
        ["FX_BombNormal", "FX_BombAnyDeath"],
    );
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.presentation.object_fx_events.len(), 2);
}

fn rules() -> Rules {
    Rules {
        motion: super::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 1.0,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: true,
        },
        kill_when_resting: true,
        active_body: false,
        collision: collision_rules(),
        death: Death::Slow(SlowDeath {
            probability: 10,
            sink_delay: [30, 0],
            destruction_delay: [60, 0],
            sink_rate: 1.0 / 30.0,
        }),
    }
}

fn collision_rules() -> CollisionRules {
    CollisionRules {
        stiffness: 0.5,
        rubble_height: 5.0,
        allow_force: true,
        weapons: [None, None],
    }
}

fn fixture() -> (Skirmish, usize) {
    let mut game = game();
    let mut debris = fixture_type(Faction::Gla, Kind::Effect);
    debris.name = "GenericDebris".into();
    // Installed GenericDebris has Physics and SlowDeath, with no AI interface.
    // An ordinary fixture AI would consume idle RNG and reject these contacts.
    debris.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    debris
        .behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    debris.update_modules.retain(|module| {
        !matches!(module,
        crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. }
            if family == "AIUpdateInterface")
    });

    debris.body_kind = crate::skirmish::BodyKind::Inactive;
    debris.initial_health = 0.0;
    debris.max_health = 0.0;
    debris.physical = Some(rules());
    game.catalog.push(debris);
    let source = game.spawn(6, 0, [300.0, 300.0], true);
    game.entities[source].ai.motion.velocity = [2.0, 3.0];
    (game, source)
}

fn list() -> List {
    List {
        name: "OCL_RaptorDeathInitial".into(),
        initial_force: None,
        nuggets: vec![Nugget {
            names: vec!["AVRaptor_D4".into()],
            debris_mass: Some(3.0),
            count: 1,
            ignore_primary_obstacle: false,
            orient_in_force_direction: false,
            draw: super::DebrisDrawRules {
                animations: Vec::new(),
                final_fx: String::new(),
                keep_color: false,
            },
            offset: [0.0, 0.0, 10.0],
            disposition: SEND_FLYING | INHERIT_VELOCITY | RANDOM_FORCE,
            intensity: 0.5,
            spin_rate: Some(20.0_f32.to_radians() / 30.0),
            angular_rates: [None; 3],
            extra_friction: 0.0,
            bounce_sound: String::new(),
            particle: String::new(),
            force_magnitude: [100.0, 200.0],
            force_pitch: [0.0, 1.0],
            health: [1.0, 1.0],
            lifetime: None,
        }],
    }
}

#[test]
fn debris_preserves_native_rng_force_before_mass_and_flying_precedence() {
    let (mut game, source) = fixture();
    let mut random = game.random;
    startup::random_between(&mut random, 0);
    startup::random_real(&mut random, 1.0, 1.0);
    let spin = 20.0_f32.to_radians() / 30.0;
    let yaw = startup::random_real(&mut random, -spin, spin);
    let roll = startup::random_real(&mut random, -spin, spin);
    let pitch = startup::random_real(&mut random, -spin, spin);
    let x = startup::random_real(&mut random, -2.0, 2.0);
    let y = startup::random_real(&mut random, -2.0, 2.0);
    let z = startup::random_real(&mut random, 1.5 * 0.33, 1.5);
    let id = game.create_physical_ocl(source, &list()).unwrap()[0];
    let actor = &game.entities[id];
    assert_eq!(actor.health.to_bits(), 0.0_f32.to_bits());
    assert!(actor.physical_present());
    assert_eq!(actor.producer, Some(source));
    assert_eq!(actor.physical_model(), Some("AVRaptor_D4"));
    let state = actor.physical.as_ref().unwrap();
    let fx = x * (1.0 / 3.0);
    let fy = y * (1.0 / 3.0);
    let fz = z * (1.0 / 3.0);
    assert_eq!(
        state.motion.acceleration.map(f32::to_bits),
        [2.0 + fx, 3.0 + fy, fz].map(f32::to_bits)
    );
    assert_eq!(
        state.motion.rates.map(f32::to_bits),
        [roll, pitch, yaw].map(f32::to_bits)
    );
    assert_eq!(game.random, random);
}

#[test]
fn random_force_uses_native_positive_full_circle_after_angular_draws() {
    let (mut game, source) = fixture();
    let mut list = list();
    let rule = &mut list.nuggets[0];
    rule.disposition = RANDOM_FORCE;
    rule.debris_mass = Some(1.0);
    rule.bounce_sound = "VehicleDebris".into();
    let mut random = game.random;
    startup::random_between(&mut random, 0);
    startup::random_real(&mut random, 1.0, 1.0);
    let spin = rule.spin_rate.unwrap();
    for _ in 0..3 {
        startup::random_real(&mut random, -spin, spin);
    }
    let yaw = startup::random_real(&mut random, 0.0, std::f32::consts::TAU);
    let pitch = startup::random_real(&mut random, 0.0, 1.0);
    let magnitude = startup::random_real(&mut random, 100.0, 200.0);
    let x = yaw.cos() * magnitude;
    let y = yaw.sin() * magnitude;
    let force = [
        pitch.cos() * x,
        pitch.cos() * y,
        -(-pitch).sin() * magnitude,
    ];
    let id = game.create_physical_ocl(source, &list).unwrap()[0];
    let state = game.entities[id].physical.as_ref().unwrap();
    assert_eq!(
        state.motion.acceleration.map(f32::to_bits),
        force.map(f32::to_bits)
    );
    assert_eq!(state.bounce_sound, "VehicleDebris");
    assert_eq!(game.random, random);
}

#[test]
fn like_existing_debris_and_object_fx_keep_source_bank_and_roll_snapshot() {
    let (mut game, source) = fixture();
    let first = game.create_physical_ocl(source, &list()).unwrap()[0];
    let mut basis = transform::yaw(0.7);
    transform::rotate(&mut basis, [0.4, -0.2, 0.0]);
    game.entities[first].physical.as_mut().unwrap().motion.basis = basis;
    let mut list = list();
    list.nuggets[0].disposition = LIKE_EXISTING;
    let offset = transform::vector(basis, list.nuggets[0].offset);
    let origin = game.object_position(first);
    let expected: [f32; 3] = std::array::from_fn(|axis| origin[axis] + offset[axis]);
    let second = game.create_physical_ocl(first, &list).unwrap()[0];
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    assert_eq!(
        game.entities[second]
            .physical_basis()
            .unwrap()
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    game.object_fx(first, "AircraftImpact".into());
    game.entities[first].physical.as_mut().unwrap().motion.basis = transform::yaw(0.0);
    assert_eq!(
        game.presentation.object_fx_events[0]
            .basis
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn landing_sound_tracks_previous_airborne_frame_even_when_held_and_fires_once() {
    let (mut game, source) = fixture();
    let mut list = list();
    list.nuggets[0].bounce_sound = "VehicleDebris".into();
    let id = game.create_physical_ocl(source, &list).unwrap()[0];
    let state = game.entities[id].physical.as_mut().unwrap();
    state.motion.position[2] = 10.0;
    game.set_disabled_held(id);
    game.tick += 1; // Physics constructor wakes on the following logic frame.
    game.integrate_physical_object(id);
    assert!(game.presentation.object_sounds.is_empty());
    game.entities[id].physical.as_mut().unwrap().motion.position[2] = 0.0;
    game.tick += 1;
    game.integrate_physical_object(id);
    assert_eq!(game.presentation.object_sounds.len(), 1);
    assert_eq!(game.presentation.object_sounds[0].object, id);
    assert_eq!(game.presentation.object_sounds[0].name, "VehicleDebris");
    assert!(matches!(
        game.presentation.object_sounds[0].action,
        crate::skirmish::object_sounds::Action::Play
    ));
    game.tick += 1;
    game.integrate_physical_object(id);
    assert_eq!(game.presentation.object_sounds.len(), 1);
    let grounded = game.create_physical_ocl(source, &list).unwrap()[0];
    let state = game.entities[grounded].physical.as_mut().unwrap();
    state.motion.position[2] = 0.0;
    game.set_disabled_held(grounded);
    game.tick += 1;
    game.integrate_physical_object(grounded);
    assert_eq!(game.presentation.object_sounds.len(), 1);
}

#[test]
fn lifetime_kills_bodyless_hulk_then_sinks_and_retires_without_repeated_death_rng() {
    let (mut game, source) = fixture();
    let unit = game.catalog.last_mut().unwrap();
    unit.behavior.lifetimes = vec![[2; 2]];
    crate::skirmish::tests::register_update(unit, "LifetimeUpdate");
    let rules = unit.physical.as_mut().unwrap();
    let Death::Slow(death) = &mut rules.death else {
        unreachable!()
    };
    death.sink_delay = [1, 0];
    death.destruction_delay = [3, 0];
    let id = game.create_physical_ocl(source, &list()).unwrap()[0];
    let born = game.tick;
    game.tick = born + 1;
    game.run_object_updates();
    game.update_physical_slow_death(id);
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Living
    ));
    game.tick += 1;
    game.run_object_updates();
    game.update_physical_slow_death(id);
    let random = game.random;
    let height = game.object_position(id)[2];
    game.kill_physical_object(id);
    assert_eq!(game.random, random);
    game.tick += 1;
    game.run_object_updates();
    game.update_physical_slow_death(id);
    assert!(game.object_position(id)[2] < height);
    game.tick = born + 5;
    game.run_object_updates();
    game.update_physical_slow_death(id);
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert!(game.entities[id].exists());
    game.process_update_destruction();
    assert!(game.entities[id].masked());
    assert!(!game.entities[id].physical_present());
}

#[test]
fn physical_birth_waits_then_runs_once_and_ground_bounce_uses_old_pose_before_sink_holds() {
    let (mut game, source) = fixture();
    let id = game.create_physical_ocl(source, &list()).unwrap()[0];
    let state = game.entities[id].physical.as_mut().unwrap();
    state.motion.position[2] = 0.5;
    state.motion.velocity = [0.0, 0.0, -2.0];
    state.motion.acceleration = [0.0; 3];
    state.motion.rates = [0.1, 0.2, 0.3];
    let basis = transform::yaw(0.0);
    game.integrate_physical_object(id);
    assert_eq!(game.object_position(id)[2].to_bits(), 0.5_f32.to_bits());
    game.tick += 1;
    game.integrate_physical_object(id);
    let state = game.entities[id].physical.as_ref().unwrap();
    assert_eq!(state.motion.position[2].to_bits(), 0.0_f32.to_bits());
    assert!(state.motion.acceleration[2] > 0.0);
    assert_eq!(
        state.motion.basis.map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    let velocity = state.motion.velocity;
    game.integrate_physical_object(id);
    assert_eq!(
        game.entities[id]
            .physical
            .as_ref()
            .unwrap()
            .motion
            .velocity
            .map(f32::to_bits),
        velocity.map(f32::to_bits)
    );
}

#[test]
fn post_rotations_preserve_local_axes_and_do_not_rebuild_world_euler_angles() {
    let mut basis = transform::yaw(std::f32::consts::FRAC_PI_2);
    transform::rotate(&mut basis, [0.7, 0.3, 0.1]);
    let before = basis;
    transform::rotate(&mut basis, [0.7, 0.3, 0.1]);
    let mut rebuilt = transform::yaw(std::f32::consts::FRAC_PI_2);
    transform::rotate(&mut rebuilt, [1.4, 0.6, 0.2]);
    assert_ne!(
        basis.map(|row| row.map(f32::to_bits)),
        rebuilt.map(|row| row.map(f32::to_bits))
    );
    assert_ne!(
        basis.map(|row| row.map(f32::to_bits)),
        before.map(|row| row.map(f32::to_bits))
    );
    let unit = transform::vector(basis, [1.0, 0.0, 0.0]);
    let length = unit.iter().map(|value| value * value).sum::<f32>();
    assert!((length - 1.0).abs() < 0.000_001);
}

#[test]
fn inactive_debris_resting_death_dispatches_one_mux_and_timing_sequence_then_retires() {
    let (mut game, source) = fixture();
    let id = game.create_physical_ocl(source, &list()).unwrap()[0];
    assert_eq!(game.entities[id].max_health.to_bits(), 0.0_f32.to_bits());
    assert!(game.entities[id].death_lifecycle.is_none());
    let state = game.entities[id].physical.as_mut().unwrap();
    state.motion.position[2] = 0.0;
    state.motion.velocity = [0.0; 3];
    state.motion.acceleration = [0.0; 3];
    game.tick += 1;
    let mut expected = game.random;
    // One SlowDeath selector and three timing requests. CRC=0 skips equal
    // sink/destruction bounds, so only selector and midpoint advance the RNG.
    startup::random_between(&mut expected, 9);
    startup::random_between(&mut expected, 0);
    startup::random_between(&mut expected, 0);
    startup::random_between(&mut expected, 18); // floor(.65*60)-floor(.35*60)
    game.integrate_physical_object(id);
    assert_eq!(game.random, expected);
    let death_frame = game.tick;
    assert!(
        matches!(game.entities[id].physical.as_ref().unwrap().phase, Phase::Dying { sink, destruction } if sink == death_frame + 30 && destruction == death_frame + 60)
    );
    game.kill_physical_object(id);
    assert_eq!(game.random, expected);
    assert!(game.presentation.object_fx_events.is_empty());
    // Exercise this callback directly; unrelated live objects have their own
    // scheduled AI INITIAL entry and legitimate RNG consumption.
    game.tick = death_frame + 59;
    game.update_physical_slow_death(id);
    assert!(game.entities[id].physical_present());
    game.tick += 1;
    game.update_physical_slow_death(id);
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert!(game.entities[id].exists());
    game.process_update_destruction();
    assert!(!game.entities[id].physical_present());
    assert!(!game.entities[id].exists());
    game.update_physical_slow_death(id);
    assert_eq!(game.random, expected);
}

#[test]
fn ocl_lifetime_override_retimes_only_the_first_authored_module() {
    let (mut game, source) = fixture();
    let unit = game.catalog.last_mut().unwrap();
    unit.behavior.lifetimes = vec![[30; 2], [60; 2]];
    crate::skirmish::tests::register_update(unit, "LifetimeUpdate");
    crate::skirmish::tests::register_update(unit, "LifetimeUpdate");
    let mut creation = list();
    creation.nuggets[0].lifetime = Some([7; 2]);
    let id = game.create_physical_ocl(source, &creation).unwrap()[0];
    let born = game.update_frame();
    assert_eq!(
        game.entities[id].behaviors.lifetime_die_frames,
        [born + 7, born + 60]
    );
    game.tick += 6;
    game.run_object_updates();
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Living
    ));
    game.tick += 1;
    game.run_object_updates();
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Dying { .. }
    ));
}

#[test]
fn offset_physical_ocl_does_not_reveal_the_unplaced_source_location() {
    use crate::skirmish::visibility::{Status, VisibilityRules};
    let (mut game, source) = fixture();
    for unit in &mut game.catalog {
        unit.sight.clearing_range = 0.0;
    }
    let kind = game.catalog.len() - 1;
    let unit = &mut game.catalog[kind];
    unit.name = "VisiblePhysicalObject".into();
    unit.body_kind = crate::skirmish::BodyKind::Active;
    unit.initial_health = 100.0;
    unit.max_health = 100.0;
    unit.physical.as_mut().unwrap().active_body = true;
    unit.sight.clearing_range = 80.0;
    game.configure_visibility(VisibilityRules {
        cell_size: 40.0,
        unlook_persist_frames: 150,
        use_shroud: true,
    })
    .unwrap();
    let mut list = list();
    let nugget = &mut list.nuggets[0];
    nugget.names = vec!["VisiblePhysicalObject".into()];
    nugget.debris_mass = None;
    nugget.disposition = LIKE_EXISTING;
    nugget.offset = [300.0, 0.0, 0.0];
    game.create_physical_ocl(source, &list).unwrap();
    assert!(matches!(
        game.visibility_cell(0, [300.0, 300.0]),
        Status::Shrouded
    ));
    assert!(matches!(
        game.visibility_cell(0, [600.0, 300.0]),
        Status::Clear
    ));
}

#[test]
fn full_object_transform_survives_copy_translation_and_same_heading_uprighting() {
    let mut game = game();
    let kind = 0;
    let original = game.new_object(kind, 0, true);
    let replacement = game.new_object(kind, 0, true);
    let mut basis = transform::yaw(0.7);
    transform::rotate(&mut basis, [0.3, -0.4, 0.0]);
    // Preserve matrix drift too: a quaternion or reconstructed Euler pose would lose it.
    basis[1][2] *= 1.000_01;
    let position = [300.0, 310.0, 47.0];
    game.set_object_transform(original, position, basis);
    let (copied_position, copied_basis) = game.object_transform(original);
    game.set_object_transform(replacement, copied_position, copied_basis);
    game.set_object_position(replacement, [320.0, 330.0, 53.0]);
    assert_eq!(
        game.object_position(replacement).map(f32::to_bits),
        [320.0_f32, 330.0, 53.0].map(f32::to_bits)
    );
    assert_eq!(
        game.entities[replacement]
            .object_basis()
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    game.object_fx(replacement, "FX_TestTransform".into());
    assert_eq!(
        game.presentation
            .object_fx_events
            .last()
            .unwrap()
            .basis
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    let heading = game.entities[replacement].facing;
    game.set_object_orientation(replacement, heading);
    assert_eq!(
        game.entities[replacement]
            .object_basis()
            .map(|row| row.map(f32::to_bits)),
        transform::yaw(heading).map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn full_object_transform_updates_physics_before_the_next_callback() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    let id = game.new_object(kind, 0, true);
    let mut basis = transform::yaw(-0.2);
    transform::rotate(&mut basis, [0.3, 0.4, 0.0]);
    game.set_object_transform(id, [300.0, 300.0, 100.0], basis);
    assert_eq!(
        game.entities[id]
            .physical
            .as_ref()
            .unwrap()
            .motion
            .basis
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    game.tick += 1;
    game.integrate_physical_object(id);
    assert_eq!(
        game.entities[id]
            .object_basis()
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn terrain_position_and_orientation_preserve_distinct_native_cached_and_matrix_heights() {
    let mut game = game();
    let samples = (0..64_u16)
        .map(|index| f32::from(index % 8) * 10.0)
        .collect();
    game.set_projectile_terrain(8, 8, [0.0; 2], samples, 20.0)
        .unwrap();
    let mut unit = fixture_type(Faction::Gla, Kind::Vehicle);
    unit.kind_of.push("STICK_TO_TERRAIN_SLOPE".into());
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.new_object(kind, 0, true);
    game.set_object_position(id, [25.0, 25.0, 70.0]);
    assert_eq!(game.object_position(id)[2].to_bits(), 25.0_f32.to_bits());
    let basis = game.entities[id].object_basis();
    assert!(basis[2][0] > 0.7);
    assert!(basis[0][2] < -0.7);
    game.set_object_transform(id, [25.0, 25.0, 70.0], transform::yaw(0.0));
    assert_eq!(game.object_transform(id).0[2].to_bits(), 70.0_f32.to_bits());
    game.set_object_orientation(id, 0.0);
    // Thing::setOrientation caches the prior matrix position after alignment.
    assert_eq!(game.object_position(id)[2].to_bits(), 70.0_f32.to_bits());
    assert_eq!(game.object_transform(id).0[2].to_bits(), 25.0_f32.to_bits());
    let replacement = game.new_object(kind, 0, true);
    let (position, basis) = game.object_transform(id);
    game.set_object_transform(replacement, position, basis);
    assert_eq!(
        game.object_position(replacement)[2].to_bits(),
        25.0_f32.to_bits()
    );
    game.set_object_position(id, [35.0, 25.0, 500.0]);
    assert_eq!(game.object_position(id)[2].to_bits(), 35.0_f32.to_bits());
    assert_eq!(game.object_transform(id).0[2].to_bits(), 35.0_f32.to_bits());
}

#[test]
fn terrain_alignment_selects_bridge_plane_and_native_deck_offset() {
    let mut game = game();
    game.movement_terrain
        .add_bridge(
            crate::terrain::BridgeGeometry {
                from: [100.0, 200.0, 20.0],
                to: [300.0, 200.0, 40.0],
                corners: [
                    [100.0, 220.0, 20.0],
                    [100.0, 180.0, 20.0],
                    [300.0, 220.0, 40.0],
                    [300.0, 180.0, 40.0],
                ],
            },
            |_| 0.0,
        )
        .unwrap();
    let mut unit = fixture_type(Faction::Gla, Kind::Vehicle);
    unit.kind_of.push("STICK_TO_TERRAIN_SLOPE".into());
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.new_object(kind, 0, true);
    game.set_object_position(id, [200.0, 200.0, 30.0]);
    assert_eq!(game.object_position(id)[2].to_bits(), 32.5_f32.to_bits());
    assert!(game.entities[id].object_basis()[2][0] > 0.09);
    game.set_object_position(id, [200.0, 200.0, 0.0]);
    assert_eq!(game.object_position(id)[2].to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        game.entities[id].object_basis()[2][2].to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn smoothed_terrain_normal_preserves_native_repeated_x3_neighbor() {
    let mut game = game();
    let mut samples = vec![0.0; 64];
    samples[2 * 8 + 4] = 10.0;
    game.set_projectile_terrain(8, 8, [0.0; 2], samples, 20.0)
        .unwrap();
    let normal = game
        .projectile_terrain
        .as_ref()
        .unwrap()
        .normal([25.0, 27.0]);
    // The center triangle is flat; the outer d6 sample supplies X1 AND X3.
    assert_eq!(
        game.ground_height([25.0, 27.0]).to_bits(),
        0.0_f32.to_bits()
    );
    let inverse = 1.0_f32 / 1_114_112.0_f32.sqrt();
    assert_eq!(
        normal.map(f32::to_bits),
        [-256.0 * inverse, 0.0, 1024.0 * inverse].map(f32::to_bits)
    );
    assert_eq!(
        game.projectile_terrain
            .as_ref()
            .unwrap()
            .normal([5.0, 5.0])
            .map(f32::to_bits),
        [0.0_f32, 0.0, 1.0].map(f32::to_bits)
    );
}

#[test]
fn locomotor_turn_aligns_slope_before_following_force_and_preserves_cached_height() {
    let mut game = game();
    let samples = (0..64_u16)
        .map(|index| f32::from(index % 8) * 10.0)
        .collect();
    game.set_projectile_terrain(8, 8, [0.0; 2], samples, 20.0)
        .unwrap();
    let mut unit = fixture_type(Faction::Gla, Kind::Vehicle);
    unit.kind_of.push("STICK_TO_TERRAIN_SLOPE".into());
    unit.locomotor.turn_rate = std::f32::consts::PI;
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.new_object(kind, 0, true);
    game.set_object_transform(id, [25.0, 25.0, 70.0], transform::yaw(0.0));
    let terrain = transform::TerrainContext {
        terrain: game.projectile_terrain.as_ref(),
        layers: &game.movement_terrain,
        height: 70.0,
    };
    crate::skirmish::locomotor::turn_ground_angle(
        &mut game.entities[id],
        &game.catalog[kind],
        0.0,
        1,
        &terrain,
    );
    assert_eq!(game.object_position(id)[2].to_bits(), 70.0_f32.to_bits());
    assert_eq!(game.object_transform(id).0[2].to_bits(), 25.0_f32.to_bits());
    assert!(game.entities[id].object_basis()[2][0] > 0.7);
    game.catalog[kind].locomotor.turn_pivot_offset = -0.5;
    let terrain = transform::TerrainContext {
        terrain: game.projectile_terrain.as_ref(),
        layers: &game.movement_terrain,
        height: 25.0,
    };
    crate::skirmish::locomotor::turn_ground_angle(
        &mut game.entities[id],
        &game.catalog[kind],
        std::f32::consts::FRAC_PI_2,
        2,
        &terrain,
    );
    assert_eq!(game.object_position(id)[2].to_bits(), 25.0_f32.to_bits());
    assert!(game.entities[id].object_basis()[2][0] > 0.7);
}

#[test]
fn held_physical_callback_preserves_terrain_oriented_matrix_and_cached_pose() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    game.catalog[kind]
        .kind_of
        .push("STICK_TO_TERRAIN_SLOPE".into());
    let samples = (0..64_u16)
        .map(|index| f32::from(index % 8) * 10.0)
        .collect();
    game.set_projectile_terrain(8, 8, [0.0; 2], samples, 20.0)
        .unwrap();
    let id = game.new_object(kind, 0, true);
    game.set_object_transform(id, [25.0, 25.0, 70.0], transform::yaw(0.0));
    game.set_object_orientation(id, 0.0);
    let (position, basis) = game.object_transform(id);
    game.set_disabled_held(id);
    game.tick += 1;
    game.update_object_physics(id);
    assert_eq!(game.object_position(id)[2].to_bits(), 70.0_f32.to_bits());
    assert_eq!(
        game.object_transform(id).0.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(
        game.object_transform(id).1.map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn debris_setup_chooses_game_rng_before_health_and_snapshots_color_and_force_yaw() {
    let (mut game, source) = fixture();
    let mut list = list();
    let nugget = &mut list.nuggets[0];
    nugget.disposition = RANDOM_FORCE;
    nugget.orient_in_force_direction = true;
    nugget.draw.keep_color = true;
    nugget.draw.final_fx = "FX_TechnicalGunnerHitsGround".into();
    nugget.draw.animations = ["A", "B", "C"]
        .map(|set| ["1", "2", "3"].map(|phase| format!("UITech_Man_SKL.UITech_Man_DT{set}{phase}")))
        .into();
    game.players[0].color = Some(0xff20_4080);
    let mut random = game.random;
    startup::random_between(&mut random, 0);
    let selected = usize::try_from(startup::random_between(&mut random, 2)).unwrap();
    startup::random_real(&mut random, 1.0, 1.0);
    let spin = nugget.spin_rate.unwrap();
    for _ in 0..3 {
        startup::random_real(&mut random, -spin, spin);
    }
    let force =
        super::creation::calc_random_force(&mut random, nugget.force_pitch, nugget.force_magnitude);
    let expected = nugget.draw.animations[selected].clone();
    let id = game.create_physical_ocl(source, &list).unwrap()[0];
    assert_eq!(game.random, random);
    let draw = game.entities[id].debris_draw().unwrap();
    assert_eq!(draw.animations.as_ref(), Some(&expected));
    assert_eq!(draw.final_fx, "FX_TechnicalGunnerHitsGround");
    assert_eq!(draw.color, Some(0xff20_4080));
    let expected_yaw = force[1].atan2(force[0]);
    let basis = transform::yaw(expected_yaw);
    assert_eq!(
        game.entities[id]
            .object_basis()
            .map(|row| row.map(f32::to_bits)),
        basis.map(|row| row.map(f32::to_bits))
    );
    game.players[0].color = Some(0xff80_4020);
    assert_eq!(
        game.entities[id].debris_draw().unwrap().color,
        Some(0xff20_4080)
    );
}

#[test]
fn initial_random_force_runs_before_model_choice_and_null_physics_draws_nothing() {
    let (mut game, source) = fixture();
    let mut list = list();
    list.initial_force = Some(super::RandomForce {
        magnitude: [60.0, 100.0],
        pitch: [70.0_f32.to_radians(), 90.0_f32.to_radians()],
        spin_rate: 120.0_f32.to_radians() / 30.0,
    });
    // The ordinary source in this fixture has no PhysicsBehavior: native skips
    // the entire nugget, including all six force and angular RNG requests.
    let before = game.random;
    let force_only = List {
        name: "SourceForce".into(),
        initial_force: list.initial_force.clone(),
        nuggets: Vec::new(),
    };
    game.create_physical_ocl(source, &force_only).unwrap();
    assert_eq!(game.random, before);
    let debris = game.create_physical_ocl(source, &list).unwrap()[0];
    let mut expected = game.random;
    let force = list.initial_force.as_ref().unwrap();
    let impulse = super::creation::calc_random_force(&mut expected, force.pitch, force.magnitude);
    let rates: [f32; 3] = std::array::from_fn(|_| {
        startup::random_real(&mut expected, -force.spin_rate, force.spin_rate)
    });
    let acceleration = game.entities[debris]
        .physical
        .as_ref()
        .unwrap()
        .motion
        .acceleration;
    let mass = game.object_physics_mass(debris).unwrap();
    game.create_physical_ocl(debris, &force_only).unwrap();
    assert_eq!(game.random, expected);
    let state = game.entities[debris].physical.as_ref().unwrap();
    let desired: [f32; 3] = std::array::from_fn(|axis| {
        let delta = impulse[axis] * (1.0 / mass);
        acceleration[axis] + delta
    });
    assert_eq!(
        state.motion.acceleration.map(f32::to_bits),
        desired.map(f32::to_bits)
    );
    assert_eq!(
        state.motion.rates.map(f32::to_bits),
        [rates[1], rates[2], rates[0]].map(f32::to_bits)
    );
}

#[test]
fn ordinary_create_object_keeps_layer_inherits_force_and_uses_native_flying_rng_order() {
    let (mut game, _) = fixture();
    let kind = game.catalog.len() - 1;
    let source = game.new_object(kind, 0, true);
    game.set_object_position(source, [300.0, 300.0, 40.0]);
    game.entities[source].layer = 2;
    game.entities[source]
        .physical
        .as_mut()
        .unwrap()
        .motion
        .velocity = [2.0, 3.0, 4.0];
    let mut expected_random = game.random;
    let spin = (std::f32::consts::PI / 32.0) * 0.5;
    let yaw = startup::random_real(&mut expected_random, -spin, spin);
    let roll = startup::random_real(&mut expected_random, -spin, spin);
    let pitch = startup::random_real(&mut expected_random, -spin, spin);
    let force = [
        startup::random_real(&mut expected_random, -2.0, 2.0),
        startup::random_real(&mut expected_random, -2.0, 2.0),
        startup::random_real(&mut expected_random, 1.5 * 0.33, 1.5),
    ];
    let first = game.entities.len();
    game.create_generic_objects(
        source,
        None,
        &crate::skirmish::object_creation::List {
            name: "NativeFlyingObject".into(),
            objects: vec![crate::skirmish::object_creation::Object {
                names: vec!["GenericDebris".into()],
                count: 1,
                offset: [0.0; 3],
                disposition: SEND_FLYING | SEND_UP | INHERIT_VELOCITY,
                health: [1.0; 2],
                lifetime: [0; 2],
                flags: crate::skirmish::object_creation::PRESERVE_LAYER,
                container: String::new(),
                invulnerable_time: 0,
                force_intensity: 0.5,
                force_magnitude: [0.0; 2],
                force_pitch: [0.0; 2],
                spin_rate: -1.0,
            }],
        },
    );
    assert_eq!(game.entities.len(), first + 1);
    assert_eq!(game.entities[first].layer, 2);
    let motion = &game.entities[first].physical.as_ref().unwrap().motion;
    assert_eq!(
        motion.rates.map(f32::to_bits),
        [roll, pitch, yaw].map(f32::to_bits)
    );
    assert_eq!(
        motion.acceleration.map(f32::to_bits),
        [2.0 + force[0], 3.0 + force[1], 4.0 + force[2]].map(f32::to_bits)
    );
    assert!(motion.bouncing);
    assert_eq!(game.random, expected_random);
}

#[test]
fn ordinary_send_out_uses_ground_height_and_horizontal_force_without_airborne_setup() {
    use crate::skirmish::object_creation::{List as ObjectList, Object, PRESERVE_LAYER};
    for physics in [false, true] {
        let (mut game, _) = fixture();
        let kind = game.catalog.len() - 1;
        if physics {
            game.catalog[kind]
                .physical
                .as_mut()
                .unwrap()
                .motion
                .allow_bouncing = false;
        } else {
            game.catalog[kind].physical = None;
        }
        let source = game.new_object(kind, 0, true);
        game.set_object_position(source, [300.0, 300.0, 90.0]);
        game.entities[source].layer = 2;
        let mut expected = game.random;
        let yaw = startup::random_real(&mut expected, 0.0, std::f32::consts::TAU);
        let force = if physics {
            [
                startup::random_real(&mut expected, -16.0, 16.0),
                startup::random_real(&mut expected, -16.0, 16.0),
                0.0,
            ]
        } else {
            [0.0; 3]
        };
        let first = game.entities.len();
        game.create_generic_objects(
            source,
            None,
            &ObjectList {
                name: "OutwardOriginalDisposition".into(),
                objects: vec![Object {
                    names: vec!["GenericDebris".into()],
                    count: 1,
                    offset: [0.0; 3],
                    disposition: SEND_OUT,
                    health: [1.0; 2],
                    lifetime: [0; 2],
                    flags: PRESERVE_LAYER,
                    container: String::new(),
                    invulnerable_time: 0,
                    force_intensity: 4.0,
                    force_magnitude: [0.0; 2],
                    force_pitch: [0.0; 2],
                    spin_rate: -1.0,
                }],
            },
        );
        assert_eq!(game.entities.len(), first + 1);
        assert_eq!(game.entities[first].layer, 2);
        assert_eq!(
            game.object_position(first)[2].to_bits(),
            game.ground_height([300.0, 300.0]).to_bits()
        );
        let heading = if yaw > std::f32::consts::PI {
            yaw - std::f32::consts::TAU
        } else {
            yaw
        };
        assert_eq!(game.entities[first].facing.to_bits(), heading.to_bits());
        assert_eq!(game.random, expected);
        if physics {
            let motion = &game.entities[first].physical.as_ref().unwrap().motion;
            assert_eq!(
                motion.acceleration.map(f32::to_bits),
                force.map(f32::to_bits)
            );
            assert_eq!(
                motion.rates.map(f32::to_bits),
                [0.0_f32; 3].map(f32::to_bits)
            );
            assert!(!motion.bouncing);
            assert_eq!(motion.extra_friction.to_bits(), 0.0_f32.to_bits());
        }
    }
}

#[test]
fn masked_physical_debris_keeps_registered_physics_but_held_still_prevents_displacement() {
    for masked in [false, true] {
        for held in [false, true] {
            let (mut game, _) = fixture();
            let kind = game.catalog.len() - 1;
            crate::skirmish::tests::register_update(&mut game.catalog[kind], "PhysicsBehavior");
            let id = game.new_object(kind, 0, true);
            let initial = [300.0, 300.0, 100.0];
            game.set_object_transform(id, initial, transform::yaw(0.0));
            game.mask_object(id, masked);
            if held {
                game.entities[id].disabled_status |= 1 << 3;
            }
            game.entities[id].physical.as_mut().unwrap().motion.velocity = [2.0, 0.0, -1.0];
            game.tick = 1;
            game.run_object_updates();
            assert_eq!(game.entities[id].masked(), masked);
            let actual = game.object_position(id);
            if held {
                assert_eq!(actual.map(f32::to_bits), initial.map(f32::to_bits));
            } else {
                assert!(
                    actual[0] > initial[0],
                    "masked={masked}: scheduled body did not integrate forward velocity"
                );
                assert!(
                    actual[2] < initial[2],
                    "masked={masked}: scheduled body did not fall"
                );
            }
        }
    }
}
