use super::*;
use crate::collision_geometry::Shape;
use crate::skirmish::LaunchTransform;

pub(super) fn rules() -> MissileRules {
    let frame = 1.0 / 30.0;
    let acceleration_scale = frame * frame;
    MissileRules {
        jam_scatter: 75.0,
        retirement: crate::skirmish::MissileRetirement {
            calls_kill: false,
            delay: 3,
        },
        initial_velocity: 150.0 * frame,
        ignition_delay: 0,
        fuel_lifetime: 90,
        initial_distance: 3.0,
        dive_distance: 0.0,
        lock_distance: 75.0,
        follow_target: true,
        use_weapon_speed: false,
        detonate_on_no_fuel: false,
        thrust: ThrustRules {
            max_speed: 225.0 * frame,
            min_speed: 120.0 * frame,
            acceleration: 675.0 * acceleration_scale,
            braking: 0.0,
            turn_rate: 100.0 * (frame * (std::f32::consts::PI / 180.0)),
            max_thrust_angle: 45.0 * (std::f32::consts::PI / 180.0),
            preferred_height: 0.0,
            height_damping: 1.0,
            close_enough: 1.0,
            airborne_motive: true,
        },
        physics: ProjectilePhysics {
            mass: 1.0,
            gravity: -64.0 * acceleration_scale,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            extra_friction: 0.0,
            apply_friction_when_airborne: false,
        },
        geometry: Geometry {
            is_small: true,
            shape: Shape::Sphere,
            major: 2.0,
            minor: 2.0,
            height: 2.0,
        },
        clear: None,
    }
}

fn launch(
    rules: &MissileRules,
    target: Option<&Target>,
) -> (Missile, crate::skirmish::projectiles::Motion) {
    let launch = LaunchTransform {
        position: [0.0, 0.0, 10.0],
        axes: LaunchTransform::IDENTITY.axes,
    };
    let mut motion = crate::skirmish::projectiles::Motion::new([0.0; 3]);
    let mut missile = Missile::new([0.0; 3], rules, 100);
    missile.fire(
        &mut motion,
        &crate::skirmish::projectiles::ProjectileLaunch {
            transform: &launch,
            destination: [200.0, 0.0, 10.0],
            speed: 20.0,
            frame: 100,
        },
        target,
        rules,
    );
    (missile, motion)
}

fn target(position: [f32; 3]) -> Target {
    Target {
        position,
        max_height: 10.0,
        center_height: 5.0,
        sphere_radius: 5.0,
        velocity: [0.0; 3],
        direction: [1.0, 0.0, 0.0],
    }
}

#[test]
fn ordinary_missile_creation_consumes_each_native_constructor_before_later_rng() {
    use crate::skirmish::{
        Kind, LaunchTransform, LocomotorVariant, WeaponMotion,
        point_defense::tests::missile_template,
        startup,
        tests::{fixture_type, game},
    };
    for native_missile in [false, true] {
        let mut game = game();
        let mut missile = missile_template(rules(), 1.0);
        let mut second = fixture_type(crate::skirmish::Faction::Gla, Kind::Effect).locomotor;
        second.wander_length = Some(4.0);
        second.legal_surfaces = 4;
        missile.locomotor_sets.normal.push(LocomotorVariant {
            speed: 225.0,
            locomotor: second,
        });
        let projectile_type = game.catalog.len();
        game.catalog.push(missile);
        game.catalog[6].weapon.motion = Some(WeaponMotion {
            turret_mounts: None,
            launches: std::array::from_fn(|_| vec![LaunchTransform::IDENTITY]),
            damage_thresholds: [0.5, 0.1],
            shots_per_barrel: 1,
            projectile_type: native_missile.then_some(projectile_type),
            collide_mask: 4,
            shell_speed_scale: None,
        });
        game.catalog[6].weapon.speed = 600.0;
        let victim = game.spawn(6, 0, [405.0, 305.0], true);
        let source = game.spawn(6, 1, [300.0, 305.0], true);
        let mut expected = game.random;
        if native_missile {
            for _ in 0..2 {
                startup::random_real(
                    &mut expected,
                    -std::f32::consts::PI / 6.0,
                    std::f32::consts::PI / 6.0,
                );
                startup::random_real(&mut expected, 0.8, 1.2);
                startup::random_between(&mut expected, 1);
            }
            // Inherited AIUpdateInterface::onObjectCreated constructs three
            // nested Idle machines, then enters the primary default Idle.
            for _ in 0..4 {
                startup::random_between(&mut expected, 60);
            }
        }
        game.fire_weapon(source, victim);
        assert_eq!(game.projectiles.len(), 1);
        assert_eq!(game.random, expected);
    }
}

#[test]
fn ordinary_missile_birth_initializes_inherited_idle_before_weapon_launch() {
    use crate::skirmish::{
        point_defense::tests::missile_template, projectiles::Control, tests::game,
    };
    let mut game = game();
    let kind = game.catalog.len();
    game.catalog.push(missile_template(rules(), 1.0));
    let object = game.spawn(kind, 0, [305.0, 205.0], true);
    let actor = &game.entities[object];
    assert!(actor.ai.primary.initialized);
    assert!(actor.ai.idle.needs_init && actor.ai_idle());
    assert_eq!(actor.ai.primary.sleep_until, 0);
    assert!(actor.ai.current_victim.is_none());
    let Control::Missile(missile) = &actor.projectile_flight.as_ref().unwrap().controls[0] else {
        panic!("actual MissileAI control");
    };
    assert!(!missile.launched && matches!(missile.state, State::Prelaunch));
    assert_eq!(
        game.update_remaining_sleep(object, actor.ai.registration.unwrap()),
        1
    );
}

#[test]
fn integration_tracks_retained_dead_objects_and_retires_for_destroyed_targets() {
    use crate::skirmish::{
        point_defense::tests::missile_targeted, projectiles::Control, tests::game,
    };
    for (health, masked, exists) in [
        (100.0, true, true),
        (0.0, false, true),
        (0.0, true, true),
        (0.0, true, false),
    ] {
        let mut game = game();
        let target = game.spawn(6, 0, [700.0, 305.0], true);
        let serial = missile_targeted(
            &mut game,
            [430.0, 305.0, 10.0],
            1,
            1.0,
            Some(target),
            [700.0, 305.0, 0.0],
        );
        let object = game.projectiles[game.projectile_index(serial).unwrap()]
            .object
            .unwrap();
        let registration = game.catalog[game.entities[object].type_index].update_modules.iter()
            .position(|entry| matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior {family, ..} if family == "MissileAIUpdate")).unwrap();
        game.tick = 1;
        game.update_missile_module(object, registration);
        if health == 0.0 {
            game.kill_object(target, crate::skirmish::DeathType::Normal);
        }
        game.entities[target].maintenance.masked = masked;
        if !exists {
            game.destroy_maintenance_object(target);
            game.process_update_destruction();
        }
        assert_eq!(game.entities[target].exists(), exists);
        game.tick = 2;
        game.update_missile_module(object, registration);
        let Control::Missile(missile) = &game.entities[object]
            .projectile_flight
            .as_ref()
            .unwrap()
            .controls[0]
        else {
            panic!("missile")
        };
        assert_eq!(matches!(missile.state, State::Retiring), !exists);
        if exists {
            assert!(matches!(missile.state, State::AttackNoTurn | State::Attack));
        }
        // A missing victim starts delayed retirement without hiding the Object.
        assert!(
            game.projectiles[game.projectile_index(serial).unwrap()]
                .render_transform(&game.entities)
                .is_some()
        );
    }
}

#[test]
fn launch_queues_force_and_native_physics_applies_gravity_once() {
    let rules = rules();
    let (missile, mut motion) = launch(&rules, None);
    assert_eq!(motion.velocity.map(f32::to_bits), [0.0_f32.to_bits(); 3]);
    assert_eq!(
        motion.acceleration[0].to_bits(),
        rules.initial_velocity.to_bits()
    );
    assert_eq!(
        motion.position.map(f32::to_bits),
        [0.0_f32.to_bits(), 0.0_f32.to_bits(), 10.0_f32.to_bits()]
    );
    motion.integrate(
        &rules.physics,
        101,
        missile.physics_braking(),
        |_| 0.0,
        |_| 0.0,
    );
    assert_eq!(
        motion.position[0].to_bits(),
        rules.initial_velocity.to_bits()
    );
    assert_eq!(
        motion.velocity[2].to_bits(),
        rules.physics.gravity.to_bits()
    );
    assert_eq!(
        motion.acceleration.map(f32::to_bits),
        [0.0_f32.to_bits(); 3]
    );
}

#[test]
fn ignition_arms_immediately_but_noturn_transition_keeps_its_previous_cap() {
    let rules = rules();
    let (mut missile, mut motion) = launch(&rules, None);
    assert!(!missile.can_collide());
    assert!(matches!(
        missile.update(&mut motion, None, &rules, 101, 0.0),
        Event::None
    ));
    assert!(missile.can_collide());
    assert!(matches!(missile.state, State::AttackNoTurn));
    assert_eq!(missile.fuel_expires, 191);
    motion.position[0] = 20.0;
    missile.update(&mut motion, None, &rules, 102, 0.0);
    assert!(matches!(missile.state, State::Attack));
    assert_eq!(missile.turn_cap.to_bits(), 0.0_f32.to_bits());
    missile.update(&mut motion, None, &rules, 103, 0.0);
    assert_eq!(missile.turn_cap.to_bits(), 99_999.0_f32.to_bits());
}

#[test]
fn attack_fuel_exhaustion_coasts_but_kill_fuel_exhaustion_retires() {
    let mut rules = rules();
    rules.fuel_lifetime = 1;
    let (mut missile, mut motion) = launch(&rules, None);
    missile.update(&mut motion, None, &rules, 101, 0.0);
    missile.update(&mut motion, None, &rules, 102, 0.0);
    assert!(!matches!(missile.state, State::Retiring));
    assert_eq!(missile.acceleration_cap.to_bits(), 0.0_f32.to_bits());
    missile.state = State::Kill;
    missile.update(&mut motion, None, &rules, 103, 0.0);
    assert!(matches!(missile.state, State::Retiring));
    assert!(matches!(
        missile.update(&mut motion, None, &rules, 105, 0.0),
        Event::None
    ));
    assert!(matches!(
        missile.update(&mut motion, None, &rules, 106, 0.0),
        Event::Remove
    ));
}

#[test]
fn collision_detonation_disables_further_contacts_until_native_kill_self_deadline() {
    use crate::skirmish::{point_defense::tests::missile_targeted, tests::game};
    let mut game = game();
    game.tick = 100;
    let victim = game.spawn(6, 0, [450.0, 305.0], true);
    let serial = missile_targeted(
        &mut game,
        [430.0, 305.0, 10.0],
        1,
        1.0,
        Some(victim),
        [450.0, 305.0, 0.0],
    );
    let object = game.projectiles[game.projectile_index(serial).unwrap()]
        .object
        .unwrap();
    let registration = game.catalog[game.entities[object].type_index].update_modules.iter()
        .position(|entry| matches!(entry, crate::skirmish::module_updates::ModuleRegistration::Behavior {family,..} if family == "MissileAIUpdate")).unwrap();
    game.tick = 101;
    game.run_object_updates();
    game.tick = 102;
    assert!(game.projectile_contact(object, Some(victim)));
    let flight = game.entities[object].projectile_flight.as_ref().unwrap();
    assert!(flight.hidden);
    assert!(flight.no_collisions());
    assert!(flight.killing_self());
    assert!(
        game.projectiles[game.projectile_index(serial).unwrap()]
            .render_transform(&game.entities)
            .is_none()
    );
    let effects = game.presentation.weapon_fx.len();
    game.tick = 103;
    assert!(game.projectile_contact(object, Some(victim)));
    assert_eq!(game.presentation.weapon_fx.len(), effects);
    game.tick = 104;
    game.update_missile_module(object, registration);
    game.process_update_destruction();
    assert!(game.entities[object].exists());
    game.tick = 105;
    game.update_missile_module(object, registration);
    game.process_update_destruction();
    assert!(!game.entities[object].exists());
}

#[test]
fn vanished_tracking_target_retires_without_explosion() {
    let rules = rules();
    let target = target([200.0, 0.0, 0.0]);
    let (mut missile, mut motion) = launch(&rules, Some(&target));
    missile.update(&mut motion, Some(&target), &rules, 101, 0.0);
    assert!(matches!(
        missile.update(&mut motion, None, &rules, 102, 0.0),
        Event::None
    ));
    assert!(matches!(missile.state, State::Retiring));
}

#[test]
fn nontracking_lock_is_half_distance_and_restores_original_target_height() {
    let rules = rules();
    let (mut missile, mut motion) = launch(&rules, None);
    missile.update(&mut motion, None, &rules, 101, 0.0);
    motion.position = [162.5, 0.0, 10.0];
    missile.update(&mut motion, None, &rules, 102, 0.0);
    assert!(!matches!(missile.state, State::Kill));
    motion.position[0] = 163.0;
    missile.update(&mut motion, None, &rules, 103, 0.0);
    assert!(matches!(missile.state, State::Kill));
    assert_eq!(
        missile.goal.position.map(f32::to_bits),
        missile.original_target.map(f32::to_bits)
    );
}

#[test]
fn kill_direct_step_clamps_at_goal_and_physics_does_not_move_it_twice() {
    let rules = rules();
    let (mut missile, mut motion) = launch(&rules, None);
    missile.locomotor_flags |= IS_BRAKING;
    motion.velocity = [100.0, 0.0, 0.0];
    motion.acceleration = [0.0; 3];
    missile.move_towards(&mut motion, [2.0, 0.0, 10.0], 2.0, &rules, 101, 0.0);
    assert_eq!(
        motion.position.map(f32::to_bits),
        [2.0_f32.to_bits(), 0.0_f32.to_bits(), 10.0_f32.to_bits()]
    );
    motion.integrate(
        &rules.physics,
        101,
        missile.physics_braking(),
        |_| 0.0,
        |_| 0.0,
    );
    assert_eq!(motion.position[0].to_bits(), 2.0_f32.to_bits());
}

#[test]
fn ground_contact_ignores_high_distant_ground_but_not_near_goal() {
    let rules = rules();
    let (mut missile, mut motion) = launch(&rules, None);
    missile.update(&mut motion, None, &rules, 101, 0.0);
    motion.position = [0.0, 0.0, 40.0];
    assert!(matches!(missile.ground_contact(&motion, 102), Event::None));
    motion.position = [190.0, 0.0, 21.0];
    assert!(matches!(
        missile.ground_contact(&motion, 103),
        Event::Detonate(_)
    ));
}

#[test]
fn target_lead_uses_height_correction_and_half_travel_time() {
    let rules = rules();
    let mut target = target([200.0, 0.0, 10.0]);
    target.velocity = [0.0, 2.0, 0.0];
    target.direction = [0.0, 1.0, 0.0];
    let (missile, motion) = launch(&rules, Some(&target));
    let goal = missile.move_destination(&motion, Some(&target));
    assert_eq!(goal[2].to_bits(), 20.0_f32.to_bits());
    let expected = 0.5 * (40_000.0_f32 + 100.0).sqrt() * 2.0 / 5.0;
    assert_eq!(goal[1].to_bits(), expected.to_bits());
}

#[test]
fn fixed_profile_diving_uses_moving_target_not_stale_launch_goal() {
    let mut rules = rules();
    rules.lock_distance = 0.0;
    rules.dive_distance = 25.0;
    rules.thrust.preferred_height = 50.0;
    let mut target = target([200.0, 0.0, 10.0]);
    let (mut missile, mut motion) = launch(&rules, Some(&target));
    missile.update(&mut motion, Some(&target), &rules, 101, 0.0);
    target.position[0] = 10.0;
    missile.update(&mut motion, Some(&target), &rules, 102, 0.0);
    assert_ne!(missile.locomotor_flags & PRECISE_Z_POS, 0);
}

#[test]
fn no_fuel_detonation_captures_origin_before_hidden_braking_movement() {
    let mut rules = rules();
    rules.detonate_on_no_fuel = true;
    rules.fuel_lifetime = 1;
    let (mut missile, mut motion) = launch(&rules, None);
    missile.update(&mut motion, None, &rules, 101, 0.0);
    missile.state = State::Kill;
    missile.locomotor_flags |= IS_BRAKING;
    motion.velocity = [5.0, 0.0, 0.0];
    let original = motion.position;
    let Event::Detonate(position) = missile.update(&mut motion, None, &rules, 102, 0.0) else {
        panic!("expired fuel must detonate");
    };
    assert_eq!(position.map(f32::to_bits), original.map(f32::to_bits));
    missile.finish_detonation(102);
    missile.update_move(&mut motion, None, &rules, 102, 0.0);
    assert_ne!(
        motion.position.map(f32::to_bits),
        original.map(f32::to_bits)
    );
    // Only the collision callback disables contacts; detonate() itself does not.
    assert!(missile.can_collide());
}

#[test]
fn native_heap_keeps_each_launch_fire_before_exhaust_and_old_ignition_identity() {
    use crate::skirmish::{ProjectileFxStage, WeaponFxEvent};
    fn visual_events(events: &[WeaponFxEvent]) -> Vec<&WeaponFxEvent> {
        events
            .iter()
            .filter(|event| {
                !matches!(
                    event,
                    WeaponFxEvent::FiringSound { .. } | WeaponFxEvent::StopFiringSound(_)
                )
            })
            .collect::<Vec<_>>()
    }
    let (mut game, [first, rifle, second, target], projectile_type) = native_heap_fixture();
    game.attack(first, target).unwrap();
    game.step();
    // AIAttackAimState succeeds on its first update, then Fire's onEnter
    // prepares the weapon. Its first shot belongs to the following update.
    assert!(game.presentation.shots.is_empty());
    game.attack(rifle, target).unwrap();
    game.attack(second, target).unwrap();
    game.step();
    assert_eq!(game.presentation.shots.len(), 1, "first launcher shot");
    let events = visual_events(&game.presentation.weapon_fx);
    let [
        WeaponFxEvent::Shot(0),
        WeaponFxEvent::DrawableCreated {
            object,
            type_index,
            validate_logic,
        },
        WeaponFxEvent::Projectile(created),
    ] = &events[..]
    else {
        panic!("first launch must retain FireFX, drawable construction, then exhaust creation");
    };
    assert_eq!(*type_index, projectile_type);
    assert!(*validate_logic);
    assert_eq!(
        game.entities[*object].projectile_id(),
        Some(created.projectile)
    );
    assert!(matches!(created.stage, ProjectileFxStage::Created));
    let first_projectile = created.projectile;
    // All three callbacks are INITIAL with the same wake frame. SleepyHeap
    // compares only packed priority, retains the left child on equal priority,
    // and rebalances its root after each callback; it is not age/FIFO ordered.
    // For these authored registrations this visits second, rifle, old missile.
    // Keep the exact resulting event stream, not a global projectile-first sort.
    game.step();
    let events = visual_events(&game.presentation.weapon_fx);
    let [
        WeaponFxEvent::Shot(0),
        WeaponFxEvent::DrawableCreated {
            object,
            type_index,
            validate_logic,
        },
        WeaponFxEvent::Projectile(created),
        WeaponFxEvent::Shot(1),
        WeaponFxEvent::BodyParticles(0),
        WeaponFxEvent::Projectile(ignition),
    ] = &events[..]
    else {
        panic!(
            "native equal-priority heap must retain callback order and each launch's FireFX/exhaust pair"
        );
    };
    assert_eq!(*type_index, projectile_type);
    assert!(*validate_logic);
    assert_eq!(
        game.entities[*object].projectile_id(),
        Some(created.projectile)
    );
    assert!(matches!(ignition.stage, ProjectileFxStage::Ignition { .. }));
    assert_eq!(ignition.projectile, first_projectile);
    assert_eq!(game.presentation.shots[0].source, second);
    assert_eq!(game.presentation.shots[1].source, rifle);
    // The immediate rifle damage changes the victim's ActiveBody state before
    // the next heap callback can run the old missile's ignition.
    assert_eq!(game.presentation.body_particles[0].pose.object, target);
    assert!(matches!(created.stage, ProjectileFxStage::Created));
    assert_eq!(created.source, second);
    assert_ne!(created.projectile, first_projectile);
}

fn native_heap_fixture() -> (crate::skirmish::Skirmish, [usize; 4], usize) {
    use crate::skirmish::{Kind, WeaponMotion};
    let mut game = crate::skirmish::tests::game();
    for unit in &mut game.catalog {
        unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    }
    let tank = game.type_for(0, Kind::Tank).unwrap();
    let infantry = game.type_for(0, Kind::Infantry).unwrap();
    let projectile_type = game.catalog.len();
    game.catalog
        .push(crate::skirmish::point_defense::tests::missile_template(
            rules(),
            1.0,
        ));
    let launches = || {
        std::array::from_fn(|_| {
            vec![LaunchTransform {
                position: [0.0, 0.0, 10.0],
                axes: LaunchTransform::IDENTITY.axes,
            }]
        })
    };
    game.catalog[tank].weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: launches(),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(projectile_type),
        collide_mask: 4,
        shell_speed_scale: None,
    });
    game.catalog[infantry].weapon.speed = 999_999.0;
    game.catalog[infantry].weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: launches(),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    let first = game.spawn(tank, 0, [300.0, 100.0], true);
    let rifle = game.spawn(infantry, 0, [300.0, 100.0], true);
    let second = game.spawn(tank, 0, [300.0, 100.0], true);
    let target = game.spawn(tank, 1, [400.0, 100.0], true);
    (game, [first, rifle, second, target], projectile_type)
}

#[test]
fn ordinary_step_runs_native_projectile_motion_only_through_its_heap_callbacks() {
    use crate::skirmish::{point_defense::tests::missile, tests::game};
    let mut snapshots = Vec::new();
    for whole_step in [false, true] {
        let mut game = game();
        let serial = missile(&mut game, [430.0, 305.0, 100.0], 1, 1.0);
        let object = game.projectiles[game.projectile_index(serial).unwrap()]
            .object
            .unwrap();
        if whole_step {
            game.step();
        } else {
            game.tick = 1;
            game.run_object_updates();
        }
        let flight = game.entities[object].projectile_flight.as_ref().unwrap();
        snapshots.push((
            flight.motion.position.map(f32::to_bits),
            flight.motion.velocity.map(f32::to_bits),
            flight.motion.acceleration.map(f32::to_bits),
        ));
        assert!(game.entities[object].live_physics.is_none());
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

#[test]
fn first_physics_callback_reports_landing_from_initial_airborne_position() {
    let rules = rules();
    let mut motion = Motion::new([10.0, 20.0, 1.0]);
    motion.velocity[2] = -2.0;
    assert!(motion.integrate(&rules.physics, 1, false, |_| 0.0, |_| 0.0));
    assert_eq!(motion.position[2].to_bits(), 0.0_f32.to_bits());
    assert!(!motion.integrate(&rules.physics, 1, false, |_| 0.0, |_| 0.0));
    let mut motion = Motion::new([10.0, 20.0, 0.0]);
    assert!(!motion.integrate(&rules.physics, 1, false, |_| 0.0, |_| 0.0));
}

#[test]
fn raised_bridge_layer_contact_is_still_above_terrain() {
    let rules = rules();
    let mut motion = Motion::new([10.0, 20.0, 26.0]);
    motion.velocity[2] = -2.0;
    assert!(!motion.integrate(&rules.physics, 1, false, |_| 25.0, |_| 0.0));
    assert_eq!(motion.position[2].to_bits(), 25.0_f32.to_bits());
    assert!(!motion.integrate(&rules.physics, 1, false, |_| 25.0, |_| 0.0));
}
