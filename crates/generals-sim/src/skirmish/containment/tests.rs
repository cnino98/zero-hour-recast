use super::{Art, PassengerWeapons, Rules};
use crate::skirmish::{
    Faction, Kind, Order,
    tests::{fixture_type, game},
};

pub(in crate::skirmish) fn setup(regen: f32, damage: f32) -> (crate::skirmish::Skirmish, usize) {
    let mut game = game();
    game.catalog[5].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        appearance: crate::skirmish::GroundAppearance::Legs,
        is_dozer: false,
        downhill_only: false,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.15,
        lateral_friction: 0.15,
        extra_friction: 0.0,
    });
    let mut unit = fixture_type(Faction::Gla, Kind::Vehicle);
    unit.kind_of = vec!["VEHICLE".into()];
    unit.geometry.major = 12.0;
    unit.geometry.minor = 12.0;
    unit.weapon.damage = 0.0;
    unit.contain = Some(rules(regen, damage));
    crate::skirmish::tests::register_update(&mut unit, "TransportContain");
    let index = game.catalog.len();
    game.catalog.push(unit);
    let carrier = game.spawn(index, 0, [355.0, 255.0], true);
    (game, carrier)
}

#[test]
fn original_enter_command_approaches_boards_and_removes_world_claims() {
    let (mut game, carrier) = setup(0.0, 1.0);
    game.ai_busy_from_ai(carrier);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(rider, carrier).unwrap();
    for _ in 0..1200 {
        game.step();
        if game.entities[rider].contained_by().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    assert!(game.entities[rider].masked());
    assert!(!game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
    assert!(game.move_to(rider, [50.0, 50.0]).is_err());
    assert_eq!(game.passengers(carrier), &[rider]);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [357.0_f32, 258.0, 5.0].map(f32::to_bits)
    );
    game.move_to(carrier, [405.0, 255.0]).unwrap();
    for _ in 0..180 {
        game.step();
    }
    let expected = game.container_point(carrier, [2.0, 3.0, 5.0]);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        expected.map(f32::to_bits)
    );
}

#[test]
fn enter_interrupts_movement_without_retiring_the_new_host_exclusion() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.move_to(rider, [455.0, 335.0]).unwrap();
    for _ in 0..10 {
        game.step();
    }
    assert!(game.entities[rider].moving());
    game.enter(rider, carrier).unwrap();
    game.step();
    assert!(matches!(game.entities[rider].ai.order, Order::Enter(target) if target == carrier));
    assert_eq!(game.entities[rider].ai.ignored_producer, Some(carrier));
    for _ in 0..1200 {
        game.step();
        if game.entities[rider].contained_by().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
}

#[test]
fn entering_uses_native_closest_route_when_host_center_is_obstructed() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    let goal = game.entities[carrier].position;
    let cell = game.cell_at(goal).unwrap();
    game.navigation = game.grid.with_obstacles([cell]);
    game.enter(rider, carrier).unwrap();
    assert!(
        game.movement_path(rider, game.entities[rider].position, goal)
            .is_none()
    );
    game.step();
    assert!(matches!(game.entities[rider].ai.order, Order::Enter(target) if target == carrier));
    // GameLogic processes the queue after this frame's object updates, so
    // the ordinary failure and successful closest fallback are already delivered.
    assert!(game.entities[rider].ai.path_request.pending.is_none());
    assert_eq!(game.entities[rider].ai.path_request.last_goal, Some(goal));
    let endpoint = game.entities[rider].ai.path_request.last_endpoint.unwrap();
    assert_ne!(endpoint.map(f32::to_bits), goal.map(f32::to_bits));
    assert!(!game.entities[rider].ai.path_request.failed);
    assert!(game.entities[rider].ai.ground_path.is_some());
    assert!(!game.entities[rider].ai.path.optimized.is_empty());
    game.step();
    assert!(game.occupancy.goal_cell(rider).is_none());
    for _ in 0..1200 {
        game.step();
        if game.entities[rider].contained_by().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
}

#[test]
fn closest_route_does_not_force_boarding_through_a_blocked_enclosure() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.navigation = game
        .grid
        .with_obstacles((33..=37).flat_map(|x| (23..=27).map(move |y| crate::grid::Cell { x, y })));
    game.enter(rider, carrier).unwrap();
    for _ in 0..1200 {
        game.step();
        if matches!(game.entities[rider].ai.order, Order::Idle) {
            break;
        }
    }
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert!(game.entities[rider].contained_by().is_none());
    assert!(game.passengers(carrier).is_empty());
    assert!(game.can_enter(rider, carrier));
}

#[test]
fn capacity_is_rechecked_on_contact_and_foreign_or_destroyed_objects_are_rejected() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let riders = (0..3)
        .map(|_| game.spawn(5, 0, [355.0, 255.0], true))
        .collect::<Vec<_>>();
    let enemy = game.spawn(5, 1, [355.0, 255.0], true);
    assert!(!game.can_enter(enemy, carrier));
    for &rider in &riders {
        game.enter(rider, carrier).unwrap();
    }
    assert!(game.container_contact(riders[0], carrier));
    assert!(game.container_contact(riders[1], carrier));
    assert!(!game.container_contact(riders[2], carrier));
    assert_eq!(game.passengers(carrier), &[riders[0], riders[1]]);
    game.entities[carrier].health = 0.0;
    assert!(!game.can_enter(riders[2], carrier));
}

#[test]
fn native_exit_delay_preserves_held_world_state_until_each_authored_exit() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let kind = game.entities[carrier].type_index;
    // Exit happens in AI, before NORMAL decrements this authored door timer.
    game.catalog[kind].contain.as_mut().unwrap().door_frames = 2;
    let a = game.spawn(5, 0, [355.0, 255.0], true);
    let b = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(a, carrier);
    game.board(b, carrier);
    game.evacuate(carrier).unwrap();
    game.step();
    let exits = [a, b].map(|id| game.entities[id].contained_by().is_none());
    assert_eq!(exits.iter().filter(|&&value| value).count(), 1);
    let remaining = if exits[0] { b } else { a };
    for _ in 0..7 {
        game.step();
    }
    assert_eq!(game.entities[remaining].contained_by(), Some(carrier));
    game.step();
    assert!(game.passengers(carrier).is_empty());
    assert!(!game.entities[remaining].container_door_open(game.tick));
    assert!(game.entities[carrier].container_door_open(game.tick));
    assert!(matches!(
        game.entities[remaining].ai.order,
        Order::FollowPath
    ));
    assert_eq!(game.entities[remaining].ai.waypoints.points.len(), 2);
    assert!(game.entities[remaining].position[1] < game.entities[carrier].position[1]);
}

#[test]
fn medic_regeneration_and_death_damage_use_rider_maximum_health() {
    let (mut game, carrier) = setup(25.0, 0.1);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.entities[rider].health = 10.0;
    game.board(rider, carrier);
    assert!(!game.passenger_can_fire(rider));
    let max_health = game.entities[rider].max_health;
    let regen = max_health * 25.0 / 100.0 * crate::skirmish::STEP_SECONDS;
    let expected = 10.0 + regen;
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[rider].health.to_bits(), expected.to_bits());
    game.entities[rider].health = max_health;
    let health = game.entities[carrier].health;
    game.apply_hit(carrier, 0, health);
    assert!(game.entities[rider].contained_by().is_none());
    let damage = max_health * 0.1;
    assert_eq!(
        game.entities[rider].health.to_bits(),
        (max_health - damage).to_bits()
    );
    assert!(game.passengers(carrier).is_empty());
    assert!(game.entities[rider].alive());
}

#[test]
fn lethal_carrier_damage_handles_recursive_passenger_death_without_stale_membership() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let riders = (0..2)
        .map(|_| game.spawn(5, 0, [355.0, 255.0], true))
        .collect::<Vec<_>>();
    for &id in &riders {
        game.board(id, carrier);
    }
    let health = game.entities[carrier].health;
    game.apply_hit(carrier, 0, health);
    for id in riders {
        assert!(!game.entities[id].alive());
        assert!(game.entities[id].contained_by().is_none());
    }
    assert!(game.passengers(carrier).is_empty());
}

#[test]
fn partition_area_damage_cannot_hit_enclosed_passengers() {
    let (mut game, carrier) = setup(0.0, 1.0);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(rider, carrier);
    let attacker = game.spawn(6, 1, [455.0, 255.0], true);
    game.catalog[6].weapon.area = Some(crate::skirmish::AreaDamage {
        at_source: false,
        primary_radius: 100.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 15,
    });
    game.catalog[6].weapon.damage = 1.0;
    let health = game.entities[rider].health;
    let position = game.object_position(carrier);
    game.update_ground_contacts();
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        None,
        position,
        1.0,
    );
    assert_eq!(game.entities[rider].health.to_bits(), health.to_bits());
    assert!(game.entities[carrier].health < game.entities[carrier].max_health);
}

#[test]
fn enclosed_rifle_keeps_native_angle_aiming_and_fires_without_chasing() {
    let (mut game, carrier) = setup(0.0, 1.0);
    game.ai_busy_from_ai(carrier);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let enemy = game.spawn(6, 1, [285.0, 255.0], true);
    game.catalog[6].weapon.damage = 0.0;
    game.catalog[5].weapon.range = 120.0;
    game.catalog[5].weapon.motion = Some(crate::skirmish::WeaponMotion {
        turret_mounts: None,
        launches: std::array::from_fn(|_| {
            vec![crate::skirmish::LaunchTransform {
                position: [99.0, 50.0, 10.0],
                axes: crate::skirmish::LaunchTransform::IDENTITY.axes,
            }]
        }),
        damage_thresholds: [0.5, 0.1],
        shots_per_barrel: 1,
        projectile_type: None,
        collide_mask: 0,
        shell_speed_scale: None,
    });
    game.entities[rider].facing = 0.0;
    game.board(rider, carrier);
    let position = game.object_position(rider);
    let health = game.entities[enemy].health;
    game.entities[rider].ai.order = Order::Attack(enemy);
    game.step();
    assert!(game.presentation.shots.is_empty());
    assert_eq!(game.entities[enemy].health.to_bits(), health.to_bits());
    for _ in 0..240 {
        game.step();
        if !game.presentation.shots.is_empty() {
            break;
        }
    }
    let shot = game
        .presentation
        .shots
        .iter()
        .find(|shot| shot.source == rider)
        .expect("enclosed rifle fires after rotating");
    assert_eq!(
        shot.source_position.map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(
        game.launch_transform(
            rider,
            crate::skirmish::WeaponBinding {
                type_index: game.entities[rider].type_index,
                weapon_index: 0,
                weapon_slot: 0,
            },
            0
        )
        .position
        .map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        position.map(f32::to_bits)
    );
    assert!(game.entities[rider].facing.abs() > 2.0);
    game.entities[enemy].position = [100.0, 255.0];
    game.step();
    assert!(matches!(game.entities[rider].ai.order, Order::Idle));
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        position.map(f32::to_bits)
    );
}

#[test]
fn hacker_packs_before_entering_and_cannot_earn_inside_transport() {
    let (mut game, carrier) = setup(0.0, 1.0);
    game.catalog[5].behavior.hacking = Some(crate::skirmish::income::HackerRules {
        contained_delay: 2,
        unpack: 2,
        pack: 2,
        variation: 0.0,
        delay: 2,
        amounts: [5, 6, 8, 10],
        experience: 1,
    });
    game.catalog[5].weapon.damage = 0.0;
    crate::skirmish::income::tests::bind_hacker(&mut game.catalog[5]);
    let rider = game.spawn(5, 0, [355.0, 335.0], true);
    game.hack_internet(rider).unwrap();
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(game.entities[rider].hacking_condition(), Some("FIRING_A"));
    let origin = game.entities[rider].position;
    let cash = game.players[0].cash;
    game.enter(rider, carrier).unwrap();
    assert_eq!(game.entities[rider].hacking_condition(), Some("PACKING"));
    for _ in 0..3 {
        game.step();
        assert_eq!(
            game.entities[rider].position.map(f32::to_bits),
            origin.map(f32::to_bits)
        );
        assert!(game.entities[rider].contained_by().is_none());
        assert_eq!(game.players[0].cash, cash);
    }
    game.step();
    assert!(matches!(game.entities[rider].ai.order, Order::Enter(target) if target == carrier));
    for _ in 0..1200 {
        game.step();
        if game.entities[rider].contained_by().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    assert!(game.hack_internet(rider).is_err());
    for _ in 0..10 {
        game.step();
    }
    assert_eq!(game.players[0].cash, cash);
}

#[test]
fn fixed_vertical_overlap_keeps_arrived_rider_waiting_until_touching() {
    let (mut game, carrier) = setup(0.0, 1.0);
    game.catalog[5].preferred_height = 100.0;
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.enter(rider, carrier).unwrap();
    game.step();
    game.step();
    assert!(game.entities[rider].contained_by().is_none());
    assert!(matches!(game.entities[rider].ai.order, Order::Enter(target) if target == carrier));
    game.catalog[5].preferred_height = 0.0;
    let [x, y, _] = game.object_position(rider);
    game.set_object_position(rider, [x, y, 0.0]);
    game.step();
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
}

pub(in crate::skirmish) fn respawn_contain_family(
    game: &mut crate::skirmish::Skirmish,
    old: usize,
    family: &str,
) -> usize {
    let unit = game.entities[old].type_index;
    let position = game.entities[old].position;
    let owner = game.entities[old].owner;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    for module in &mut game.catalog[unit].behavior_modules {
        if module.family.ends_with("Contain") {
            module.family = family.into();
        }
    }
    for module in &mut game.catalog[unit].update_modules {
        if let crate::skirmish::module_updates::ModuleRegistration::Behavior {
            family: current, ..
        } = module
            && current.ends_with("Contain")
        {
            *current = family.into();
        }
    }
    game.spawn(unit, owner, position, true)
}

#[test]
fn open_door_countdown_runs_in_actual_callbacks_and_keeps_closing_state() {
    let (mut game, old) = setup(0.0, 0.0);
    let carrier = respawn_contain_family(&mut game, old, "OpenContain");
    let index = game.entities[carrier].type_index;
    game.catalog[index].contain.as_mut().unwrap().door_frames = 2;
    game.open_container_exit_door(carrier);
    assert!(game.entities[carrier].container_door_open(0));
    assert_eq!(
        game.entities[carrier].container_conditions(),
        vec!["DOOR_1_OPENING"]
    );
    game.tick = 1;
    game.run_object_updates();
    assert!(game.entities[carrier].container_door_open(1));
    game.tick = 2;
    game.run_object_updates();
    assert!(!game.entities[carrier].container_door_open(2));
    assert_eq!(
        game.entities[carrier].container_conditions(),
        vec!["DOOR_1_CLOSING"]
    );
    game.tick = 3;
    game.run_object_updates();
    assert_eq!(
        game.entities[carrier].container_conditions(),
        vec!["DOOR_1_CLOSING"]
    );
}

#[test]
fn open_containment_does_not_invent_transport_held_status() {
    let (mut game, old) = setup(0.0, 0.0);
    let carrier = respawn_contain_family(&mut game, old, "OpenContain");
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, carrier);
    assert_eq!(game.entities[rider].contained_by(), Some(carrier));
    assert_eq!(game.object_disabled_mask(rider) & (1 << 3), 0);
    assert!(!game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]));
}

pub(in crate::skirmish) fn rules(regen: f32, damage: f32) -> Rules {
    Rules {
        cave_index: None,
        payload: None,
        payload_templates: Vec::new(),
        experience_sink_for_rider: true,
        passengers_in_turret: false,
        orient_like_container_on_exit: false,
        assault_heal_ratio: None,
        heal: None,
        tunnel: None,
        garrison: None,
        capacity: 2,
        allowed: vec!["INFANTRY".into()],
        forbidden: vec!["AIRCRAFT".into()],
        weapons: PassengerWeapons {
            pass_bonuses: false,
            fire: regen <= 0.0,
            upgrade_transport: false,
        },
        relationships: crate::skirmish::containment::Relationships {
            allies: true,
            enemies: true,
            neutral: true,
        },
        exit_flags: 3,
        exit_frames: 8,
        door_frames: 1,
        exit_paths: 1,
        regen_percent: regen,
        death: super::RiderDeath {
            damage,
            burned: false,
            destroy_if_not_free: false,
        },
        damage_thresholds: [0.5, 0.1],
        art: std::array::from_fn(|_| Art {
            exits: vec![([0.0, -15.0, 0.0], [0.0, -40.0, 0.0])],
            fire_points: fire_points(&[[2.0, 3.0, 5.0], [-2.0, -3.0, 5.0]]),
        }),
    }
}

pub(in crate::skirmish) fn fire_points(points: &[[f32; 3]]) -> Vec<super::FirePoint> {
    points
        .iter()
        .map(|&position| super::FirePoint {
            position,
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        })
        .collect()
}

#[test]
fn transport_exit_orientation_uses_actual_flag_and_inherited_callback_only() {
    for (family, orient) in [
        ("TransportContain", false),
        ("TransportContain", true),
        ("OpenContain", false),
    ] {
        let (mut game, old) = setup(0.0, 0.0);
        let kind = game.entities[old].type_index;
        game.catalog[kind]
            .contain
            .as_mut()
            .unwrap()
            .orient_like_container_on_exit = orient;
        let host = respawn_contain_family(&mut game, old, family);
        game.set_object_orientation(host, 1.0);
        let rider = game.spawn(5, 0, game.entities[host].position, true);
        game.set_object_orientation(rider, 0.5);
        game.add_to_contain(rider, host);
        game.remove_passenger(rider, host);
        let expected = if orient && family == "TransportContain" {
            1.0_f32
        } else {
            0.5_f32
        };
        assert_eq!(game.entities[rider].facing.to_bits(), expected.to_bits());
        assert!(game.entities[rider].contained_by().is_none());
        assert!(!game.entities[rider].masked());
    }
}
