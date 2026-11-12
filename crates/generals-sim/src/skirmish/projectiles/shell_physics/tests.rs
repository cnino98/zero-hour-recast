use crate::skirmish::projectiles::{Motion, Shell, ShellRules};
use crate::skirmish::{LaunchTransform, physical_ocl::MotionRules, startup};

fn terrain() -> super::super::Terrain {
    super::super::Terrain::new(32, 32, [0.0; 2], vec![0.0; 1024], 40.0, [300.0; 2]).unwrap()
}

fn rules() -> ShellRules {
    ShellRules {
        detonate_calls_kill: false,
        contact: None,
        heights: [10.0; 2],
        indents: [0.5, 0.9],
        adjust_per_frame: 0.0,
        lifespan: 300,
        tumble: true,
        orient_to_flight_path: true,
        physics: Some(MotionRules {
            center_of_mass_offset: 0.0,
            mass: 0.1,
            gravity: -0.1,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        }),
    }
}

fn launch(rules: &ShellRules, random: &mut [u32; 6]) -> (Shell, Motion) {
    launch_from(
        rules,
        &LaunchTransform {
            position: [20.0, 20.0, 10.0],
            axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        },
        [200.0, 20.0, 10.0],
        10.0,
        random,
    )
    .unwrap()
}

fn launch_from(
    rules: &ShellRules,
    launch: &LaunchTransform,
    end: [f32; 3],
    speed: f32,
    random: &mut [u32; 6],
) -> Option<(Shell, Motion)> {
    let mut shell = Shell::new();
    let mut motion = Motion::new(launch.position);
    if let Some(physics) = &rules.physics {
        motion.bouncing = physics.allow_bouncing;
    }
    shell.fire(
        &mut motion,
        &crate::skirmish::projectiles::ProjectileLaunch {
            transform: launch,
            destination: end,
            speed,
            frame: 10,
        },
        None,
        rules,
        &terrain(),
        random,
    )?;
    Some((shell, motion))
}

#[test]
fn tumble_draws_pitch_yaw_roll_before_path_validation_and_normal_shell_draws_nothing() {
    let mut random = startup::seed_random(37);
    let mut reference = random;
    let limit = 1.0 / std::f32::consts::PI;
    let pitch = startup::random_real(&mut reference, -limit, limit);
    let yaw = startup::random_real(&mut reference, -limit, limit);
    let roll = startup::random_real(&mut reference, -limit, limit);
    let (_, motion) = launch(&rules(), &mut random);
    assert_eq!(random, reference);
    assert_eq!(
        motion.rates.map(f32::to_bits),
        [roll, pitch, yaw].map(f32::to_bits)
    );
    let failed = launch_from(
        &rules(),
        &LaunchTransform {
            position: [10000.0; 3],
            ..LaunchTransform::IDENTITY
        },
        [11000.0; 3],
        10.0,
        &mut random,
    );
    assert!(failed.is_none());
    for _ in 0..3 {
        startup::random_real(&mut reference, -limit, limit);
    }
    assert_eq!(random, reference);
    let mut normal = rules();
    normal.tumble = false;
    launch(&normal, &mut random);
    assert_eq!(random, reference);
}

#[test]
fn birth_keeps_launch_axes_then_physics_postrotates_roll_pitch_yaw_before_normal_position() {
    let rules = rules();
    let (mut shell, mut motion) = launch(&rules, &mut startup::seed_random(37));
    let axes = motion.axes;
    assert!(!shell.update(&mut motion, None, &rules, &terrain(), 10));
    assert_eq!(
        motion.axes.map(|axis| axis.map(f32::to_bits)),
        axes.map(|axis| axis.map(f32::to_bits))
    );
    motion.rates = [std::f32::consts::FRAC_PI_4; 3];
    motion.velocity = [1.0, 2.0, 3.0];
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 11, |_| 0.0, |_| 0.0));
    // Original yaw 90 followed by local roll 90, pitch 90, yaw 90.
    let expected = [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    for (actual, expected) in motion.axes.iter().flatten().zip(expected.iter().flatten()) {
        assert!((actual - expected).abs() < 0.000_001);
    }
    assert_eq!(
        motion.position.map(f32::to_bits),
        [21.0_f32, 22.0, 12.9].map(f32::to_bits)
    );
    let axes = motion.axes;
    shell.step = 1; // NORMAL's caller advances the curve after the birth callback.
    assert!(!shell.update(&mut motion, None, &rules, &terrain(), 11));
    assert_eq!(
        motion.position.map(f32::to_bits),
        shell.path[1].map(f32::to_bits)
    );
    assert_eq!(
        motion.axes.map(|axis| axis.map(f32::to_bits)),
        axes.map(|axis| axis.map(f32::to_bits))
    );
}

#[test]
fn lifespan_one_survives_birth_and_terminal_tick_retains_physics_position() {
    let mut rules = rules();
    rules.lifespan = 1;
    let (mut shell, mut motion) = launch(&rules, &mut startup::seed_random(37));
    assert!(!shell.update(&mut motion, None, &rules, &terrain(), 10));
    let before = motion.position;
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 11, |_| 0.0, |_| 0.0));
    assert!(shell.update(&mut motion, None, &rules, &terrain(), 11));
    assert_eq!(motion.position[2].to_bits(), (before[2] - 0.1).to_bits());
}

#[test]
fn tumble_single_sample_keeps_first_point_and_ground_contact_preempts_high_normal_sample() {
    let rules = rules();
    let (mut shell, mut motion) = launch_from(
        &rules,
        &LaunchTransform {
            position: [20.0, 20.0, 0.05],
            ..LaunchTransform::IDENTITY
        },
        [21.0, 20.0, 10.0],
        1000.0,
        &mut startup::seed_random(37),
    )
    .unwrap();
    assert_eq!(shell.path.len(), 1);
    assert!(!shell.update(&mut motion, None, &rules, &terrain(), 10));
    assert_eq!(
        motion.position.map(f32::to_bits),
        shell.path[0].map(f32::to_bits)
    );
    assert_eq!(
        motion.position.map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    // A multi-point path starts at the real launch height; its following high
    // sample must not overwrite a preceding PHYSICS ground collision.
    let (mut shell, mut motion) = launch_from(
        &rules,
        &LaunchTransform {
            position: [20.0, 20.0, 0.05],
            ..LaunchTransform::IDENTITY
        },
        [200.0, 20.0, 10.0],
        10.0,
        &mut startup::seed_random(37),
    )
    .unwrap();
    assert!(shell.path.len() > 1 && shell.path[1][2] > 0.05);
    assert!(!shell.update(&mut motion, None, &rules, &terrain(), 10));
    // Physics constructor starts WAS_AIRBORNE_LAST_FRAME clear. Observe one
    // real airborne update before the following fall can dispatch ground contact.
    motion.velocity[2] = 0.1;
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 11, |_| 0.0, |_| 0.0));
    let rates = motion.rates;
    assert!(motion.integrate_shell(rules.physics.as_ref().unwrap(), 12, |_| 0.0, |_| 0.0));
    assert_eq!(motion.position[2].to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        motion.rates.map(f32::to_bits),
        rates.map(|rate| (rate * 0.85).to_bits())
    );
}

#[test]
fn zero_lifespan_uses_native_absolute_deadline_and_noorient_retains_launch_basis() {
    let mut rules = rules();
    rules.lifespan = 0;
    let (mut shell, mut motion) = launch(&rules, &mut startup::seed_random(37));
    assert!(shell.update(&mut motion, None, &rules, &terrain(), 10));
    rules.lifespan = 300;
    rules.tumble = false;
    rules.orient_to_flight_path = false;
    let (mut shell, mut motion) = launch(&rules, &mut startup::seed_random(37));
    let axes = motion.axes;
    assert!(!shell.update(&mut motion, None, &rules, &terrain(), 10));
    assert_eq!(
        motion.axes.map(|axis| axis.map(f32::to_bits)),
        axes.map(|axis| axis.map(f32::to_bits))
    );
}

fn adjusting_shell_template() -> crate::skirmish::UnitType {
    use crate::skirmish::{projectiles::FlightRules, tests::register_update};
    let mut shell_rules = rules();
    shell_rules.adjust_per_frame = 100.0 / 30.0;
    let mut template = super::super::fixtures::shell_template(300, false);
    // Actual native source order: Physics first, DumbProjectile control second.
    template.update_modules.retain(|entry| {
        !matches!(
            entry,
            crate::skirmish::module_updates::ModuleRegistration::Behavior { .. }
        )
    });
    template.behavior_modules.clear();
    register_update(&mut template, "PhysicsBehavior");
    register_update(&mut template, "DumbProjectileBehavior");
    let physics = shell_rules.physics.as_ref().unwrap();
    template
        .live_physics
        .as_mut()
        .expect("authored fixture PhysicsBehavior")
        .motion = MotionRules {
        center_of_mass_offset: 0.0,
        mass: physics.mass,
        gravity: physics.gravity,
        stiffness: physics.stiffness,
        forward_friction: physics.forward_friction,
        lateral_friction: physics.lateral_friction,
        aerodynamic_friction: physics.aerodynamic_friction,
        angular_factor: physics.angular_factor,
        allow_bouncing: physics.allow_bouncing,
    };
    template.projectile_rules = vec![FlightRules::Shell(shell_rules)];
    template
}

fn assert_birth_callback_preserves_motion(game: &mut crate::skirmish::Skirmish, object: usize) {
    let motion = &game.entities[object]
        .projectile_flight
        .as_ref()
        .unwrap()
        .motion;
    let birth_position = motion.position;
    let birth_axes = motion.axes;
    let birth_rates = motion.rates;
    game.run_object_updates();
    let motion = &game.entities[object]
        .projectile_flight
        .as_ref()
        .unwrap()
        .motion;
    assert_eq!(
        motion.position.map(f32::to_bits),
        birth_position.map(f32::to_bits)
    );
    assert_eq!(
        motion.axes.map(|axis| axis.map(f32::to_bits)),
        birth_axes.map(|axis| axis.map(f32::to_bits))
    );
    assert_eq!(
        motion.rates.map(f32::to_bits),
        birth_rates.map(f32::to_bits)
    );
}

#[test]
fn target_adjustment_retains_hidden_live_and_dead_present_objects_but_stops_after_destruction() {
    use crate::skirmish::{
        WeaponBinding, WeaponMotion, damage::WeaponTarget, projectiles::Control,
    };
    for state in 0..4 {
        let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
        let victim = game.spawn(5, 0, [180.0, 100.0], true);
        if state == 3 {
            game.board(victim, carrier);
            assert_eq!(game.entities[victim].contained_by(), Some(carrier));
            assert!(game.entities[victim].alive() && game.entities[victim].masked());
        }
        let source = game.spawn(6, 1, [20.0, 20.0], true);
        let template = adjusting_shell_template();
        let kind = game.catalog.len();
        game.catalog.push(template);
        game.projectile_terrain = Some(
            super::super::Terrain::new(64, 64, [0.0; 2], vec![0.0; 4096], 40.0, [600.0; 2])
                .unwrap(),
        );
        let end = game.target_center(victim);
        game.tick = 10;
        game.catalog[6].weapon.speed = 300.0;
        game.catalog[6].weapon.damage = 0.0;
        game.catalog[6].weapon.motion = Some(WeaponMotion {
            turret_mounts: None,
            launches: std::array::from_fn(|_| {
                vec![LaunchTransform {
                    position: [0.0, 0.0, 10.0],
                    ..LaunchTransform::IDENTITY
                }]
            }),
            damage_thresholds: [0.5, 0.1],
            shots_per_barrel: 1,
            projectile_type: Some(kind),
            collide_mask: 0,
            shell_speed_scale: None,
        });
        game.create_weapon_projectile(
            source,
            &WeaponTarget {
                object: Some(victim),
                position: end,
                projectile_position: end,
                scatter_radius: 0.0,
            },
            WeaponBinding {
                type_index: 6,
                weapon_index: 0,
                weapon_slot: 0,
            },
            1.0,
            0,
        );
        let object = game.projectiles.last().unwrap().object.unwrap();
        assert_birth_callback_preserves_motion(&mut game, object);
        if state == 0 {
            game.entities[victim].maintenance.masked = true;
        }
        if state == 1 {
            game.apply_hit(victim, source, game.entities[victim].max_health);
        }
        if state == 2 {
            game.destroy_maintenance_object(victim);
            game.process_update_destruction();
        }
        assert_eq!(game.entities[victim].alive(), state == 0 || state == 3);
        assert_eq!(game.entities[victim].exists(), state != 2);
        if state != 2 {
            assert_eq!(game.entities[victim].masked(), state != 1);
        }
        game.entities[victim].position[1] += 60.0;
        game.tick = 11;
        game.run_object_updates();
        let Control::Shell(shell) = &game.entities[object]
            .projectile_flight
            .as_ref()
            .unwrap()
            .controls[0]
        else {
            panic!("native shell")
        };
        if state == 2 {
            assert_eq!(shell.end.map(f32::to_bits), end.map(f32::to_bits));
        } else {
            assert!(shell.end[1] > end[1]);
        }
        assert_eq!(shell.step, 2);
        game.run_object_updates();
        let Control::Shell(shell) = &game.entities[object]
            .projectile_flight
            .as_ref()
            .unwrap()
            .controls[0]
        else {
            panic!("native shell")
        };
        assert_eq!(shell.step, 2);
    }
}

fn upgraded_contact_shell_template() -> crate::skirmish::UnitType {
    use crate::{
        collision_geometry::{Geometry, Shape},
        skirmish::{ShellContact, projectiles::FlightRules, tests::register_behavior},
    };
    let mut shell_rules = rules();
    shell_rules.tumble = false;
    shell_rules.physics = None;
    shell_rules.contact = Some(ShellContact {
        geometry: Geometry {
            is_small: true,
            shape: Shape::Sphere,
            major: 2.0,
            minor: 2.0,
            height: 2.0,
        },
        clear: None,
    });
    let mut template = super::super::fixtures::shell_template(300, false);
    template.geometry = Geometry {
        is_small: true,
        shape: Shape::Sphere,
        major: 2.0,
        minor: 2.0,
        height: 2.0,
    };
    template.projectile_rules = vec![FlightRules::Shell(shell_rules)];
    template.maintenance.weapon_upgrade = Some(crate::skirmish::maintenance::WeaponUpgrade {
        trigger: "ShellProjectileUpgrade".into(),
    });
    register_behavior(&mut template, "WeaponBonusUpgrade", 0x80);
    template
        .behavior_modules
        .last_mut()
        .unwrap()
        .upgrade
        .triggers
        .push("ShellProjectileUpgrade".into());
    template
}

#[test]
fn actual_shell_collision_combines_current_projectile_upgrade_with_saved_launch_flags_once() {
    use crate::skirmish::{AreaDamage, WeaponBinding, WeaponMotion, damage::WeaponTarget};
    let mut game = crate::skirmish::tests::game();
    let source = game.spawn(6, 0, [20.0, 20.0], true);
    let victim = game.spawn(5, 1, [200.0, 100.0], true);
    let point = game.target_center(victim);
    let template = upgraded_contact_shell_template();
    let kind = game.catalog.len();
    game.catalog.push(template);
    let weapon = &mut game.catalog[6].weapon;
    weapon.damage = 20.0;
    weapon.speed = 300.0;
    weapon.bonuses.template[5][super::super::super::containment::bonuses::DAMAGE] = 1.5;
    weapon.projectile.upgrade = Some("ShellProjectileUpgrade".into());
    weapon.area = Some(AreaDamage {
        primary_radius: 3.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 2,
        at_source: false,
    });
    weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![LaunchTransform {
                position: [point[0] - 20.0, point[1] - 20.0, point[2]],
                ..LaunchTransform::IDENTITY
            }]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(kind),
        collide_mask: 4,
        shell_speed_scale: None,
    });
    game.projectile_terrain = Some(terrain());
    let destination = [250.0, 100.0, point[2]];
    game.create_weapon_projectile(
        source,
        &WeaponTarget {
            object: Some(victim),
            position: destination,
            projectile_position: destination,
            scatter_radius: 0.0,
        },
        WeaponBinding {
            type_index: 6,
            weapon_index: 0,
            weapon_slot: 0,
        },
        1.0,
        0,
    );
    let index = game.projectiles.len() - 1;
    let object = game.projectiles[index].object.unwrap();
    game.players[0]
        .upgrades
        .push("ShellProjectileUpgrade".into());
    game.activate_upgrade_modules(object);
    assert!(!game.entities[source].weapon_upgrade_active());
    assert!(game.entities[object].weapon_upgrade_active());
    assert_eq!(
        game.projectile_damage_bonus(&game.projectiles[index])
            .to_bits(),
        1.5_f32.to_bits()
    );
    let health = game.entities[victim].health;
    game.update_ground_contacts();
    game.object_contact(object, victim, false);
    assert!(game.body_collision_status(object));
    assert!(
        game.entities[object]
            .projectile_flight
            .as_ref()
            .unwrap()
            .hidden
    );
    assert_eq!(
        game.entities[victim].health.to_bits(),
        (health - 30.0).to_bits()
    );
}

#[test]
fn first_physics_callback_reports_landing_from_initial_airborne_position() {
    let rules = rules();
    let mut motion = Motion::new([10.0, 20.0, 1.0]);
    motion.velocity[2] = -2.0;
    assert!(motion.integrate_shell(rules.physics.as_ref().unwrap(), 1, |_| 0.0, |_| 0.0));
    assert_eq!(motion.position[2].to_bits(), 0.0_f32.to_bits());
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 1, |_| 0.0, |_| 0.0));
    let mut motion = Motion::new([10.0, 20.0, 0.0]);
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 1, |_| 0.0, |_| 0.0));
}

#[test]
fn raised_bridge_layer_contact_is_still_above_terrain() {
    let rules = rules();
    let mut motion = Motion::new([10.0, 20.0, 26.0]);
    motion.velocity[2] = -2.0;
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 1, |_| 25.0, |_| 0.0));
    assert_eq!(motion.position[2].to_bits(), 25.0_f32.to_bits());
    assert!(!motion.integrate_shell(rules.physics.as_ref().unwrap(), 1, |_| 25.0, |_| 0.0));
}

#[test]
fn native_launch_ignores_its_launcher_only_when_both_objects_have_physics() {
    use crate::skirmish::{projectiles::fixtures, tests::game};
    for launcher_physics in [false, true] {
        let mut game = game();
        game.projectile_terrain = Some(terrain());
        let source = 1;
        let target = 2;
        if launcher_physics {
            let kind = game.entities[source].type_index;
            game.catalog[kind].live_physics = fixtures::shell_template(300, false).live_physics;
            game.construct_object_physics(source);
        }
        let projectile_type = game.catalog.len();
        game.catalog.push(fixtures::shell_template(300, false));
        let (_, projectile) = fixtures::launch_shell(&mut game, source, projectile_type, target);
        assert_eq!(
            game.entities[projectile].physics_ignores(source),
            launcher_physics
        );
        assert!(!game.entities[projectile].physics_ignores(target));
    }
}
