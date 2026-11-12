//! Common Object fixtures: callers author DIE modules before constructor dispatch.
use crate::skirmish::{
    Faction, FlightRules, Kind, LaunchTransform, ShellRules, Skirmish, UnitType, WeaponBinding,
    WeaponMotion,
    damage::WeaponTarget,
    tests::{fixture_type, register_update},
};

pub(in crate::skirmish) fn shell_template(lifespan: u32, kill: bool) -> UnitType {
    let mut unit = fixture_type(Faction::Gla, Kind::Effect);
    unit.name = "FixtureShell".into();
    unit.kind_of = vec!["PROJECTILE".into()];
    unit.selectable = false;
    unit.max_health = 100.0;
    unit.initial_health = 100.0;
    unit.live_physics = Some(crate::skirmish::physical_ocl::LiveRules {
        motion: crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 1.0,
            gravity: 0.0,
            stiffness: 0.5,
            forward_friction: 0.0,
            lateral_friction: 0.0,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: crate::skirmish::physical_ocl::CollisionRules {
            stiffness: 0.5,
            rubble_height: 0.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.05, 0.025, 0.025],
        min_fall_speed: 0.0,
        fall_damage_factor: 1.0,
    });
    unit.projectile_rules.push(FlightRules::Shell(ShellRules {
        detonate_calls_kill: kill,
        tumble: false,
        orient_to_flight_path: true,
        physics: None,
        contact: None,
        heights: [10.0; 2],
        indents: [0.5, 0.9],
        adjust_per_frame: 100.0 / 30.0,
        lifespan,
    }));
    register_update(&mut unit, "DumbProjectileBehavior");
    register_update(&mut unit, "PhysicsBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 0x11;
    unit
}

pub(in crate::skirmish) fn launch_shell(
    game: &mut Skirmish,
    source: usize,
    projectile_type: usize,
    target: usize,
) -> (u64, usize) {
    let type_index = game.entities[source].type_index;
    let weapon = &mut game.catalog[type_index].weapon;
    weapon.damage = 1.0;
    weapon.speed = 300.0;
    weapon.motion = Some(WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![LaunchTransform {
                position: [0.0, 0.0, 10.0],
                axes: LaunchTransform::IDENTITY.axes,
            }]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: Some(projectile_type),
        collide_mask: 0,
        shell_speed_scale: None,
    });
    let position = game.target_center(target);
    let target = WeaponTarget {
        object: Some(target),
        position,
        projectile_position: position,
        scatter_radius: 0.0,
    };
    game.create_weapon_projectile(
        source,
        &target,
        WeaponBinding {
            type_index,
            weapon_index: 0,
            weapon_slot: 0,
        },
        1.0,
        0,
    );
    let serial = game.projectile_serial;
    let index = game
        .projectile_index(serial)
        .expect("native shell component");
    let object = game.projectiles[index].object.expect("native shell Object");
    (serial, object)
}
