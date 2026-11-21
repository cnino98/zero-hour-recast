use super::*;
use crate::skirmish::{
    Faction, Kind, Order, Research,
    tests::{fixture_type, game},
};

fn rules(repair: bool) -> Rules {
    Rules {
        guard_range: 35.0,
        guard_wander: 35.0,
        attack_range: 75.0,
        attack_wander: 10.0,
        scout_range: 75.0,
        scout_wander: 10.0,
        spotting_range: 0.0,
        repair: repair.then_some(Repair {
            range: 8.0,
            altitude: [18.0, 24.0],
            per_second: 10.0,
            below_percent: 60,
            ready_frames: [9, 23],
            weld_frames: [8, 15],
            welding_system: "BlueSparks".into(),
            welding_bone: "Muzzle02".into(),
        }),
        panic: None,
        panic_speed: 90.0,
        stay_on_master_layer: false,
    }
}

fn spawn_drone(repair: bool) -> (Skirmish, usize, usize) {
    spawn_drone_with_gravity(repair, 0.0)
}

fn spawn_drone_with_gravity(repair: bool, gravity: f32) -> (Skirmish, usize, usize) {
    let mut game = game();
    let parent_type = game.catalog.len();
    let mut master = fixture_type(Faction::Usa, Kind::Tank);
    master.name = "DroneMaster".into();
    master.production = Some(Vec::new());
    crate::skirmish::tests::register_update(&mut master, "ProductionUpdate");
    game.catalog.push(master);
    let parent = game.spawn(parent_type, 0, [300.0, 200.0], true);
    let index = game.catalog.len();
    let mut unit = fixture_type(Faction::Usa, Kind::Vehicle);
    unit.name = "Drone".into();
    unit.kind_of.push("DRONE".into());
    unit.behavior
        .upgrade_die
        .push(crate::skirmish::death_callbacks::UpgradeRules {
            conditions: crate::skirmish::tests::death_conditions(),
            upgrade: Some("Battle".into()),
        });
    crate::skirmish::tests::register_behavior(&mut unit, "UpgradeDie", 2);
    unit.behavior.slaves.push(rules(repair));
    crate::skirmish::tests::register_update(&mut unit, "SlavedUpdate");
    unit.locomotor.ground = None;
    unit.locomotor.legal_surfaces = 8;
    unit.locomotor.flight = Some(super::super::flight::tests::rules(
        super::super::flight::Appearance::Hover,
    ));
    unit.movement_surface = super::super::MovementSurface::Air;
    unit.preferred_height = 100.0;
    unit.live_physics = Some(crate::skirmish::physical_ocl::LiveRules {
        motion: crate::skirmish::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 1.0,
            gravity,
            stiffness: 0.5,
            forward_friction: 0.0,
            lateral_friction: 0.0,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: true,
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
    crate::skirmish::tests::register_update(&mut unit, "PhysicsBehavior");
    unit.ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    game.catalog.push(unit);
    let child = game.spawn(index, 0, game.entities[parent].position, true);
    game.enslave_spawn(child, parent);
    game.entities[child].producer = Some(parent);

    (game, parent, child)
}

fn master_attack(game: &mut Skirmish, parent: usize, target: usize) {
    game.observe_ground_partition(parent);
    game.observe_ground_partition(target);
    // A player's attack allows pursuit; an AI idle-acquisition order on a human
    // unit deliberately refuses to chase beyond range in native AIApproachTarget.
    game.attack(parent, target).unwrap();
    assert!(game.update_attack(parent, target));
    assert_eq!(game.entities[parent].ai.current_victim, Some(target));
}

#[test]
fn repairing_waits_for_unpack_ready_and_extend_then_keeps_healing_between_welds() {
    let (mut game, parent, child) = spawn_drone(true);
    game.entities[parent].health = 40.0;
    let before = game.entities[parent].health;
    game.update_drone(child, 0);
    assert_eq!(game.entities[child].drone_condition(), Some("UNPACKING"));
    for _ in 0..14 {
        game.tick += 1;
        game.update_drone(child, 0);
    }
    assert_eq!(game.entities[parent].health.to_bits(), before.to_bits());
    game.tick += 1;
    game.update_drone(child, 0);
    let wait = game.entities[child].behaviors.slaves.first().unwrap().wait;
    assert!((9..=23).contains(&wait));
    for _ in 0..wait {
        game.tick += 1;
        game.update_drone(child, 0);
    }
    assert_eq!(game.entities[child].drone_condition(), Some("FIRING_B"));
    assert!(
        game.presentation
            .slave_welds
            .iter()
            .rev()
            .find(|event| event.object == child)
            .map(|event| &event.weld)
            .is_none()
    );
    for _ in 0..5 {
        game.tick += 1;
        game.update_drone(child, 0);
    }
    let weld = game
        .presentation
        .slave_welds
        .iter()
        .rev()
        .find(|event| event.object == child)
        .map(|event| &event.weld)
        .unwrap();
    assert_eq!(weld.frame, game.tick);
    assert!((240.0..=450.0).contains(&weld.lifetime));
    let origin = weld.position;
    let ratio = weld.health_ratio;
    game.entities[child].health = 20.0;
    game.set_flight_position(child, [origin[0] + 1.0, origin[1], origin[2]]);
    let weld = game
        .presentation
        .slave_welds
        .iter()
        .rev()
        .find(|event| event.object == child)
        .map(|event| &event.weld)
        .unwrap();
    assert_eq!(weld.position.map(f32::to_bits), origin.map(f32::to_bits));
    assert_eq!(weld.health_ratio.to_bits(), ratio.to_bits());
    let amount = 10.0 / 30.0;
    assert_eq!(
        game.entities[parent].health.to_bits(),
        (before + amount).to_bits()
    );
    let first = game.entities[parent].health;
    game.tick += 1;
    game.update_drone(child, 0);
    assert_eq!(
        game.entities[parent].health.to_bits(),
        (first + amount).to_bits()
    );
}

#[test]
fn critical_health_uses_truncated_percent_and_overrides_master_attack() {
    let (mut game, parent, child) = spawn_drone(true);
    game.entities[parent].health = 60.99;
    master_attack(&mut game, parent, 2);
    game.update_drone(child, 0);
    assert_eq!(game.entities[child].drone_condition(), Some("UNPACKING"));
    game.entities[parent].health = 61.0;
    game.entities[child]
        .behaviors
        .slaves
        .first_mut()
        .unwrap()
        .wait = 0;
    game.update_drone(child, 0);
    assert_eq!(game.entities[child].drone_condition(), Some("PACKING"));
    assert!(
        !game.entities[child]
            .behaviors
            .slaves
            .first()
            .unwrap()
            .repairing
    );
}

#[test]
fn guard_offset_uses_native_integer_angle_and_normal_updates_wait_seven_frames() {
    let (mut game, _, child) = spawn_drone(false);
    let mut expected = game.random;
    let angle = progression::integer_real(startup::random_value(&mut expected) % 7);
    let actual = game.drone_random_offset(35.0);
    assert_eq!(
        actual.map(f32::to_bits),
        [35.0 * angle.cos(), 35.0 * angle.sin()].map(f32::to_bits)
    );
    assert_eq!(game.random, expected);
    game.entities[child]
        .behaviors
        .slaves
        .first_mut()
        .unwrap()
        .wait = 7;
    let random = game.random;
    for _ in 0..6 {
        game.update_drone(child, 0);
    }
    assert_eq!(game.random, random);
    assert_eq!(
        game.entities[child].behaviors.slaves.first().unwrap().wait,
        1
    );
}

#[test]
fn captured_master_transfers_enemy_drone_but_retains_allied_drone_owner() {
    let (mut game, parent, child) = spawn_drone(false);
    game.transfer_maintenance_owner(parent, 1);
    game.update_drone(child, 0);
    assert_eq!(game.entities[child].owner, 1);
    assert_eq!(game.entities[child].drone_parent(), Some(parent));
    game.slots[0].team = Some(0);
    game.slots[1].team = Some(0);
    game.transfer_maintenance_owner(parent, 0);
    game.entities[child]
        .behaviors
        .slaves
        .first_mut()
        .unwrap()
        .wait = 0;
    game.update_drone(child, 0);
    assert_eq!(game.entities[child].owner, 1);
}

#[test]
fn destroyed_master_releases_the_drone_to_physics_before_normal_crash_death() {
    let (mut game, parent, child) = spawn_drone_with_gravity(false, -1.0);
    let height = game.object_position(child)[2];
    game.apply_hit(parent, 2, 1000.0);
    game.update_drone(child, 0);
    assert!(game.entities[child].alive());
    assert!(game.entities[child].unmanned());
    assert!(game.entities[child].drone_parent().is_none());
    game.tick += 1;
    game.update_object_physics(child);
    assert!(game.object_position(child)[2] < height);
    for _ in 0..120 {
        if !game.entities[child].alive() {
            break;
        }
        game.tick += 1;
        game.update_object_physics(child);
    }
    assert!(!game.entities[child].alive());
    assert!(game.produced_drone(parent).is_none());
}

#[test]
fn master_target_follow_issues_native_flight_and_moves_both_drone_appearances() {
    for appearance in [
        super::super::flight::Appearance::Hover,
        super::super::flight::Appearance::Wings,
    ] {
        let (mut game, parent, child) = spawn_drone(false);
        let index = game.entities[child].type_index;
        let mut flight = super::super::flight::tests::rules(appearance);
        if matches!(flight.appearance, super::super::flight::Appearance::Wings) {
            flight.min_speed = 1.0;
        }
        game.catalog[index].locomotor.flight = Some(flight);
        game.entities[parent].position = [100.0, 100.0];
        game.set_flight_position(child, [100.0, 100.0, 100.0]);
        game.entities[child].facing = 0.0;
        game.entities[2].position = [400.0, 100.0];
        master_attack(&mut game, parent, 2);
        game.update_drone(child, 0);
        assert!(matches!(game.entities[child].ai.order, Order::Move(_)));
        let Order::Move(goal) = game.entities[child].ai.order else {
            unreachable!()
        };
        assert!(goal[0] >= 165.0 && goal[0] <= 185.0);
        for _ in 0..10 {
            game.tick += 1;
            game.travel_flight(child, goal, 0.1);
            game.update_flight(child);
            game.update_object_physics(child);
        }
        assert!(game.entities[child].position[0] > 100.0);
    }
}

#[test]
fn drone_death_releases_paid_upgrade_without_refund_and_allows_replacement() {
    let (mut game, parent, child) = spawn_drone(false);
    let unit = &mut game.catalog[game.entities[parent].type_index];
    unit.maintenance.research.push(Research {
        name: "Battle".into(),
        cost: 300,
        frames: 150,
        player: false,
        sciences: Vec::new(),
        ignores_underpowered: false,
    });
    unit.behavior.drone_upgrades.push(Upgrade {
        trigger: "Battle".into(),
        conflicts: vec!["Hellfire".into()],
        object: "Drone".into(),
        offset: [0.0, 0.0, 10.0],
    });
    game.finish_research(parent, 0);
    assert!(!game.can_research(parent, 0));
    let cash = game.players[0].cash;
    game.apply_hit(child, 2, 1000.0);
    assert_eq!(game.players[0].cash, cash);
    assert_eq!(game.produced_drone(parent), None);
    assert!(!game.object_has_upgrade(parent, "Battle"));
    game.queue_upgrade(parent, 0).unwrap();
    assert!(!game.drone_research_available(parent, "Battle"));
    game.cancel_upgrade(0, parent, 0).unwrap();
    game.catalog[game.entities[parent].type_index]
        .maintenance
        .research
        .push(Research {
            name: "Hellfire".into(),
            cost: 500,
            frames: 150,
            player: false,
            sciences: Vec::new(),
            ignores_underpowered: false,
        });
    game.finish_research(parent, 1);
    assert!(!game.can_research(parent, 0));
}

#[test]
fn slaved_follow_refresh_keeps_independent_turret_fire_and_primary_move_order() {
    let (mut game, parent, child) = spawn_drone(false);
    let tank = game.type_for(1, Kind::Tank).unwrap();
    let target = game.spawn(tank, 1, [350.0, 200.0], true);
    let kind = game.entities[child].type_index;
    let mut turret = super::super::turret::Rules::source_defaults();
    turret.controlled_slots = 1;
    turret.turn_rate = std::f32::consts::PI;
    game.catalog[kind].turret = Some(turret);
    game.catalog[kind].weapon = fixture_type(Faction::Usa, Kind::Tank).weapon;
    game.catalog[kind].weapon.pre_attack_frames = 9;
    game.catalog[kind].ai.acquisition |= crate::skirmish::UnitAi::IDLE;
    game.entities[child].ai.turret =
        super::super::turret::State::new(game.catalog[kind].turret.as_ref());
    game.set_flight_position(child, [300.0, 200.0, 100.0]);
    game.entities[parent].position = [300.0, 200.0];
    master_attack(&mut game, parent, target);
    game.observe_visibility(parent);
    game.observe_visibility(child);
    game.update_visibility();
    for _ in 0..40 {
        game.tick += 1;
        game.update_weapon_clocks(child);
        game.update_drone(child, 0);
        game.update_idle_turret(child);
        assert!(matches!(game.entities[child].ai.order, Order::Move(_)));
        assert!(game.entities[child].ai.ai_target.is_none());
    }
    assert!(
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == child && shot.target == Some(target)),
        "ready={}, visible={}, reachable={}, angle={}, facing={}, next_mood={}, weapon={}, rounds={}, pre={:?}",
        game.entities[child].complete(),
        game.visible_to(0, target),
        game.weapon_target_reachable(child, target),
        game.entities[child].ai.turret.angle,
        game.entities[child].facing,
        game.entities[child].ai.mood.next_check,
        game.entities[child].weapon_slot(),
        game.entities[child].weapon_set.weapons[0].clip_remaining,
        game.entities[child].weapon_set.weapons[0]
            .pre_attack
            .as_ref()
            .map(|attack| (attack.start, attack.frames))
    );
}

#[test]
fn original_stinger_slave_defaults_bind_parent_without_air_orders_or_drone_upgrade_release() {
    let mut game = game();
    let mut slave = rules(false);
    slave.guard_range = 0.0;
    slave.guard_wander = 0.0;
    slave.attack_range = 0.0;
    slave.attack_wander = 0.0;
    slave.scout_range = 0.0;
    slave.scout_wander = 0.0;
    game.catalog[5].behavior.slaves.push(slave);
    crate::skirmish::tests::register_update(&mut game.catalog[5], "SlavedUpdate");
    let parent = game.spawn(0, 0, [300.0, 200.0], true);
    let child = game.spawn(5, 0, [320.0, 200.0], true);
    let random_before = game.random;
    game.enslave_spawn(child, parent);
    assert_eq!(game.entities[child].drone_parent(), Some(parent));
    assert_ne!(game.random, random_before);
    let original = game.entities[child].position.map(f32::to_bits);
    for _ in 0..21 {
        game.update_drone(child, 0);
    }
    assert_eq!(game.entities[child].position.map(f32::to_bits), original);
    assert!(matches!(game.entities[child].ai.order, Order::Idle));
    assert!(game.entities[child].flight.is_none());
    assert!(game.produced_drone(parent).is_none());
    game.kill_object(child, crate::skirmish::DeathType::Normal);
    assert_eq!(game.entities[child].drone_parent(), Some(parent));
}

#[test]
fn scout_spotting_uses_pre_move_bounds_distance_and_clears_then_sets_each_native_wake() {
    let (mut game, parent, child) = spawn_drone(false);
    let kind = game.entities[parent].type_index;
    game.catalog[kind].weapon.range = 100.0;
    game.catalog[kind].weapon.delay = 1.0;
    game.catalog[kind].weapon.bonuses.global[6] = [1.2, 1.0, 1.2, 1.5, 1.0];
    game.catalog[kind].weapon.bonuses.template[6] = [1.3, 1.0, 1.3, 1.25, 1.0];
    game.catalog[game.entities[child].type_index]
        .behavior
        .slaves
        .first_mut()
        .unwrap()
        .spotting_range = 20.0;
    game.set_object_position(parent, [550.0, 300.0, 0.0]);
    let target = game.spawn(6, 1, [600.0, 300.0], true);
    master_attack(&mut game, parent, target);
    game.set_flight_position(child, [600.0, 300.0, 100.0]);
    game.fire_weapon(parent, target);
    assert!(
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == parent)
    );
    game.tick += 26;
    game.update_weapon_clocks(parent);
    assert_eq!(
        game.entities[parent].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        4.0_f32.to_bits()
    );
    game.update_drone(child, 0);
    let actor = &game.entities[parent];
    let unit = &game.catalog[kind];
    assert!(actor.firing.drone_spotting);
    assert_eq!(actor.weapon_range(unit, 0).to_bits(), 150.0_f32.to_bits());
    assert_eq!(
        actor
            .weapon_rate_for(actor.active_weapon(unit), 0)
            .to_bits(),
        1.75_f32.to_bits()
    );
    assert_eq!(
        actor.weapon_damage_bonus(&unit.weapon, 0).to_bits(),
        1.5_f32.to_bits()
    );
    assert_eq!(
        actor.weapon_set.weapons[0].cooldown.to_bits(),
        17.0_f32.to_bits()
    );
    game.tick += 15;
    game.update_weapon_clocks(parent);
    assert_eq!(
        game.entities[parent].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        2.0_f32.to_bits()
    );
    game.entities[child]
        .behaviors
        .slaves
        .first_mut()
        .unwrap()
        .wait = 0;
    game.update_drone(child, 0);
    // SlavedUpdate explicitly clears then reasserts the bit, so an unchanged
    // close target still triggers both native cooldown callbacks each wake.
    assert_eq!(
        game.entities[parent].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        17.0_f32.to_bits()
    );
    game.ai_idle_from_ai(parent);
    game.tick += 7;
    game.entities[child]
        .behaviors
        .slaves
        .first_mut()
        .unwrap()
        .wait = 0;
    game.update_drone(child, 0);
    assert!(!game.entities[parent].firing.drone_spotting);
    assert_eq!(
        game.entities[parent].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        30.0_f32.to_bits()
    );
}

#[test]
fn scout_creation_rotates_the_authored_backpack_offset_before_enslaving() {
    for facing in [0.0, std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
        let (mut game, parent, old_child) = spawn_drone(false);
        game.apply_native_damage(
            old_child,
            None,
            10_000.0,
            crate::skirmish::DeathType::Normal,
        );
        let parent_type = game.entities[parent].type_index;
        let origin = game.object_position(parent);
        game.destroy_maintenance_object(parent);
        game.process_update_destruction();
        let unit = &mut game.catalog[parent_type];
        unit.maintenance.research.push(Research {
            name: "Scout".into(),
            cost: 100,
            frames: 150,
            player: false,
            sciences: Vec::new(),
            ignores_underpowered: false,
        });
        unit.behavior.drone_upgrades.push(Upgrade {
            trigger: "Scout".into(),
            conflicts: Vec::new(),
            object: "Drone".into(),
            offset: [-8.0, 0.0, 10.0],
        });
        unit.behavior
            .object_creation
            .push(Some(crate::skirmish::object_creation::List {
                name: "ScoutCreation".into(),
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
                    names: vec!["Drone".into()],
                    count: 1,
                    offset: [-8.0, 0.0, 10.0],
                    disposition: crate::skirmish::physical_ocl::LIKE_EXISTING,
                    health: [1.0; 2],
                    lifetime: [0; 2],
                    flags: crate::skirmish::object_creation::REQUIRES_LIVE_PLAYER
                        | crate::skirmish::object_creation::PRESERVE_LAYER
                        | crate::skirmish::object_creation::IGNORE_PRIMARY_OBSTACLE,
                }],
            }));
        crate::skirmish::tests::register_behavior(unit, "ObjectCreationUpgrade", 0x80);
        unit.behavior_modules.last_mut().unwrap().upgrade.triggers = vec!["Scout".into()];
        let parent = game.spawn(parent_type, 0, [origin[0], origin[1]], true);
        game.set_object_orientation(parent, facing);
        let facing = game.entities[parent].facing;
        let origin = game.object_position(parent);
        game.finish_research(parent, 0);
        let child = game.produced_drone(parent).unwrap();
        let position = game.object_position(child);
        let horizontal = [8.0 * facing.cos(), 8.0 * facing.sin()];
        let expected = [
            origin[0] - horizontal[0],
            origin[1] - horizontal[1],
            origin[2] + 10.0,
        ];
        assert_eq!(position.map(f32::to_bits), expected.map(f32::to_bits));
        assert_eq!(game.entities[child].drone_parent(), Some(parent));
        // OCL copies the raw Matrix; Thing::setTransform derives heading with
        // atan2, independently of the source's cached normalized +PI heading.
        let basis = game.entities[parent].object_basis();
        assert_eq!(
            game.entities[child]
                .object_basis()
                .map(|row| row.map(f32::to_bits)),
            basis.map(|row| row.map(f32::to_bits))
        );
        assert_eq!(
            game.entities[child].facing.to_bits(),
            basis[1][0].atan2(basis[0][0]).to_bits()
        );
        // OCL IgnorePrimaryObstacle is Physics collision state, independently
        // of the AI pathfinder's producer exception.
        assert!(game.entities[child].physics_ignores(parent));
    }
}

#[test]
fn authored_slave_instances_construct_and_sleep_independently_with_real_held_admission() {
    let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
    let parent = 1;
    for _ in 0..2 {
        let mut slave = rules(false);
        slave.guard_range = 0.0;
        slave.guard_wander = 0.0;
        slave.attack_range = 0.0;
        slave.scout_range = 0.0;
        game.catalog[5].behavior.slaves.push(slave);
        crate::skirmish::tests::register_update(&mut game.catalog[5], "SlavedUpdate");
    }
    let child = game.spawn(5, 0, [300.0, 200.0], true);
    assert_eq!(game.entities[child].behaviors.slaves.len(), 2);
    assert!(
        game.entities[child]
            .behaviors
            .slaves
            .iter()
            .all(|state| state.wait == 0)
    );
    game.enslave_spawn(child, parent);
    assert_eq!(
        game.entities[child].behaviors.slaves[0].parent,
        Some(parent)
    );
    assert_eq!(game.entities[child].behaviors.slaves[1].parent, None);
    game.tick = 1;
    game.run_object_updates();
    assert!(
        game.entities[child]
            .behaviors
            .slaves
            .iter()
            .all(|state| state.wait == 7)
    );
    game.tick = 2;
    game.run_object_updates();
    assert!(
        game.entities[child]
            .behaviors
            .slaves
            .iter()
            .all(|state| state.wait == 6)
    );
    game.add_to_contain(child, carrier);
    assert!(game.entities[child].disabled_held());
    game.tick = 3;
    game.run_object_updates();
    assert!(
        game.entities[child]
            .behaviors
            .slaves
            .iter()
            .all(|state| state.wait == 6)
    );
    game.remove_passenger(child, carrier);
    game.tick = 4;
    game.run_object_updates();
    assert!(
        game.entities[child]
            .behaviors
            .slaves
            .iter()
            .all(|state| state.wait == 5)
    );
}
