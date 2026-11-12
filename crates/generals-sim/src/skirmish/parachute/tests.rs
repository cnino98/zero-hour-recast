use super::*;
use crate::collision_geometry::Geometry;
use crate::skirmish::{
    Faction, Kind, Order,
    flight::{self, Appearance},
    tests::fixture_type,
};

pub(in crate::skirmish) fn fixture_catalog() -> (Skirmish, usize) {
    let mut game = crate::skirmish::tests::game();
    let mut unit = fixture_type(Faction::Usa, Kind::Vehicle);
    unit.contain = Some(crate::skirmish::containment::tests::rules(0.0, 0.0));
    unit.geometry.major = 12.0;
    unit.geometry.minor = 12.0;
    unit.weapon.damage = 0.0;
    let index = game.catalog.len();
    game.catalog.push(unit);
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    let mut normal = flight::tests::rules(Appearance::Hover);
    normal.preferred_height = -1.0;
    normal.speed_limit_z = 15.0 / 30.0;
    normal.lift = 100.0 / 900.0;
    normal.gravity = -64.0 / 900.0;
    normal.draw.stiffness = [0.02; 2];
    normal.draw.damping = [0.01; 2];
    let mut fall = flight::tests::rules(Appearance::Hover);
    fall.preferred_height = -1.0;
    fall.acceleration = 0.0;
    fall.acceleration_damaged = 0.0;
    fall.braking = 0.0;
    fall.lift = 0.0;
    fall.lift_damaged = 0.0;
    fall.gravity = normal.gravity;
    let mut freefall = fixture_type(Faction::Usa, Kind::Vehicle).locomotor;
    freefall.ground = None;
    freefall.flight = Some(fall);
    freefall.legal_surfaces = 8;
    game.catalog[index].locomotor.ground = None;
    game.catalog[index].locomotor.flight = Some(normal);
    game.catalog[index].locomotor.legal_surfaces = 8;
    game.catalog[index].speed = 60.0;
    game.catalog[index].kind_of = vec!["PARACHUTE".into(), "UNATTACKABLE".into()];
    game.catalog[index].parachute = Some(Rules {
        freefall,
        angular_rate: [std::f32::consts::PI / 90.0; 2],
        low_damping: 0.2,
        open_distance: 25.0,
        water_slop: 10.0,
        fall_damage: 0.5,
        open_sound: "ParachuteOpen".into(),
    });
    crate::skirmish::aircraft_death::tests::install_physics(&mut game.catalog[index]);
    crate::skirmish::tests::register_update(&mut game.catalog[index], "ParachuteContain");
    (game, index)
}

pub(in crate::skirmish) fn fixture(height: f32) -> (Skirmish, usize, usize) {
    let (mut game, index) = fixture_catalog();
    let chute = game.spawn(index, 0, [355.0, 255.0], true);
    game.set_object_position(chute, [355.0, 255.0, height]);
    let point = game.entities[chute].position;
    let rider = game.spawn(5, 0, point, true);
    game.board_parachute(rider, chute);
    game.position_parachute_rider(rider, chute);
    (game, chute, rider)
}

#[test]
fn dry_ground_parachute_freefalls_opens_after_authored_distance_and_releases_living_rider() {
    let (mut game, chute, rider) = fixture(120.0);
    game.update_parachute(chute);
    assert!(!game.entities[chute].parachute_opened());
    assert_eq!(game.entities[rider].contained_by(), Some(chute));
    assert!(game.entities[rider].parachute_rider());
    assert!(!game.entities[rider].masked());
    let mut opened = false;
    for _ in 0..600 {
        game.step();
        if game.entities[chute].parachute_opened() && !opened {
            assert!(120.0 - game.object_position(chute)[2] >= 25.0);
            opened = true;
        }
        if game.entities[rider].contained_by().is_none() {
            break;
        }
    }
    assert!(opened);
    assert!(game.entities[rider].alive());
    assert!(!game.entities[rider].parachute_rider());
    assert_eq!(game.entities[rider].contained_by(), None);
    assert_eq!(game.height_above_ground(rider).to_bits(), 0.0_f32.to_bits());
    assert!(!game.entities[chute].alive());
}

#[test]
fn low_ejection_uses_native_start_height_floor_and_opens_immediately() {
    let (mut game, chute, rider) = fixture(20.0);
    game.update_parachute(chute);
    assert!(game.entities[chute].parachute_opened());
    assert_eq!(
        game.entities[chute]
            .parachute
            .as_ref()
            .unwrap()
            .start_height,
        Some(50.0)
    );
    assert_eq!(game.presentation.object_sounds.len(), 1);
    assert_eq!(game.presentation.object_sounds[0].object, rider);
    assert_eq!(game.presentation.object_sounds[0].name, "ParachuteOpen");
    game.update_parachute(chute);
    assert_eq!(game.presentation.object_sounds.len(), 1);
}

#[test]
fn passive_freefall_accepts_zero_motive_coefficients_but_not_zero_mass() {
    let (mut game, chute, _) = fixture(120.0);
    let index = game.entities[chute].type_index;
    let rules = game.catalog[index]
        .parachute
        .as_mut()
        .unwrap()
        .freefall
        .flight
        .as_mut()
        .unwrap();
    assert!(flight::validate(rules).is_ok());
    rules.mass = 0.0;
    assert!(flight::validate(rules).is_err());
}

#[test]
fn dying_unopened_chute_keeps_native_opening_without_a_live_move_request() {
    let (mut game, chute, _) = fixture(20.0);
    game.kill_object(chute, crate::skirmish::DeathType::Normal);
    assert!(game.entities[chute].is_effectively_dead());
    game.update_parachute(chute);
    assert!(game.entities[chute].parachute_opened());
    assert!(matches!(game.entities[chute].ai.order, Order::Idle));
}

#[test]
fn invalid_terrain_landing_detaches_before_one_native_death_without_reentry() {
    let (mut game, chute, rider) = fixture(0.0);
    let cell = game.cell_at(game.entities[chute].position).unwrap();
    game.movement_terrain = crate::terrain::TerrainMap::classify(
        game.movement_terrain.width(),
        game.movement_terrain.height(),
        [cell],
        [],
    )
    .unwrap();
    let losses = game.players[0].statistics.units_lost;
    let kills = game.players[1].statistics.units_destroyed;
    game.remove_passenger(rider, chute);
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(!game.entities[rider].parachute_rider());
    assert!(game.entities[chute].containment.passengers.is_empty());
    assert!(!game.entities[rider].alive());
    assert_eq!(game.players[0].statistics.units_lost, losses + 1);
    assert_eq!(game.players[1].statistics.units_destroyed, kills);
}

#[test]
fn direct_delivery_opening_resets_freefall_ultra_flag_before_normal_close_override() {
    let (mut game, chute, _) = fixture(120.0);
    game.entities[chute]
        .parachute
        .as_mut()
        .unwrap()
        .landing_override = Some([450.0, 350.0, 0.0]);
    game.update_parachute(chute);
    assert!(!game.entities[chute].parachute_opened());
    assert_eq!(
        game.entities[chute].ai.locomotor_set,
        crate::skirmish::locomotor::set::FREEFALL
    );
    game.set_flight_position(chute, [355.0, 255.0, 90.0]);
    game.update_parachute(chute);
    assert!(game.entities[chute].parachute_opened());
    assert_eq!(
        game.entities[chute].ai.locomotor_set,
        crate::skirmish::locomotor::set::NORMAL
    );
    assert!(!game.entities[chute].flight_ultra_accurate());
    let Order::Move(goal) = game.entities[chute].ai.order else {
        panic!("direct source move goal")
    };
    assert_eq!(goal.map(f32::to_bits), [450.0_f32, 350.0].map(f32::to_bits));
}

#[test]
fn held_canopy_runs_open_base_without_opening_and_ground_contact_is_not_forwarded_from_rider() {
    let (mut game, chute, rider) = fixture(20.0);
    game.set_disabled_held(chute);
    assert_eq!(game.update_parachute(chute), 1);
    assert!(!game.entities[chute].parachute_opened());
    assert!(game.entities[chute].drawable_effectively_hidden());
    assert!(
        game.entities[chute]
            .parachute
            .as_ref()
            .unwrap()
            .start_height
            .is_none()
    );
    game.clear_disabled_held(chute);
    game.update_parachute(chute);
    assert!(game.entities[chute].parachute_opened());
    assert!(!game.entities[chute].drawable_effectively_hidden());
    game.parachute_ground_contact(rider);
    assert_eq!(game.entities[rider].contained_by(), Some(chute));
    game.parachute_ground_contact(chute);
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(!game.entities[chute].alive());
    assert!(game.body_collision_status(chute));
    assert!(game.entities[rider].drawable.instance_matrix.is_none());
}

#[test]
fn rider_offset_uses_both_raw_object_bases_and_canopy_transform_does_not_reposition_it() {
    let (mut game, chute, rider) = fixture(120.0);
    let kind = game.entities[chute].type_index;
    bind_bone(&mut game.catalog[kind], "para_attch", [10.0, 0.0, 3.0]);
    let rider_type = game.entities[rider].type_index;
    bind_bone(&mut game.catalog[rider_type], "para_man", [0.0, 2.0, 1.0]);
    let state = game.entities[chute].parachute.as_mut().unwrap();
    state.update_para_bones = true;
    state.update_rider_bones = true;
    state.angles = [0.2, -0.1];
    game.set_object_transform(
        rider,
        [355.0, 255.0, 100.0],
        transform::yaw(std::f32::consts::FRAC_PI_2),
    );
    game.set_object_transform(
        chute,
        [355.0, 255.0, 120.0],
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
    );
    let before = game.object_position(rider);
    game.set_object_position(chute, [365.0, 255.0, 120.0]);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        before.map(f32::to_bits)
    );
    game.position_parachute_rider(rider, chute);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [377.0_f32, 252.0, 119.0].map(f32::to_bits)
    );
    let matrix = game.entities[rider].drawable.instance_matrix.unwrap();
    let pivot = game.entities[chute]
        .parachute
        .as_ref()
        .unwrap()
        .rider_sway_offset;
    let transformed: [f32; 3] = std::array::from_fn(|axis| {
        let row = matrix[axis];
        let products = std::array::from_fn::<_, 3, _>(|column| row[column] * pivot[column]);
        ((products[0] + products[1]) + products[2]) + row[3]
    });
    for axis in 0..3 {
        assert!((transformed[axis] - pivot[axis]).abs() < 0.00001);
    }
}

#[test]
fn canopy_always_rejects_second_rider_even_without_capacity_check() {
    let (mut game, chute, _) = fixture(120.0);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    let unit = &game.catalog[game.entities[chute].type_index];
    assert!(
        unit.behavior_modules
            .iter()
            .position(|module| module.family == "PhysicsBehavior")
            < unit
                .behavior_modules
                .iter()
                .position(|module| module.family == "ParachuteContain")
    );
    assert_eq!(unit.contain_family(), Some("ParachuteContain"));
    assert!(!game.native_container_admits(rider, chute, false));
    assert!(!game.native_container_admits(rider, chute, true));
}

fn bind_bone(unit: &mut crate::skirmish::UnitType, name: &str, position: [f32; 3]) {
    unit.pristine_draws
        .push(crate::skirmish::model_bones::Draw {
            ignored: std::collections::BTreeSet::new(),
            states: vec![crate::skirmish::model_bones::State {
                conditions: vec![std::collections::BTreeSet::new()],
                transforms: std::collections::BTreeMap::from([(
                    name.to_owned(),
                    [
                        [1.0, 0.0, 0.0, position[0]],
                        [0.0, 1.0, 0.0, position[1]],
                        [0.0, 0.0, 1.0, position[2]],
                    ],
                )]),
            }],
        });
}

#[test]
fn opening_refreshes_pristine_attachment_bones_for_actual_model_conditions() {
    let (mut game, kind) = fixture_catalog();
    bind_bone(&mut game.catalog[kind], "para_attch", [0.0, 0.0, 10.0]);
    bind_bone(&mut game.catalog[5], "para_man", [0.0, 0.0, 1.0]);
    game.catalog[5].pristine_draws[0]
        .states
        .push(crate::skirmish::model_bones::State {
            conditions: vec![std::collections::BTreeSet::from(["PARACHUTING".into()])],
            transforms: std::collections::BTreeMap::from([(
                "para_man".into(),
                [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 3.0],
                ],
            )]),
        });
    let chute = game.spawn(kind, 0, [355.0, 255.0], true);
    game.set_object_position(chute, [355.0, 255.0, 20.0]);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board_parachute(rider, chute);
    game.position_parachute_rider(rider, chute);
    assert_eq!(game.object_position(rider)[2].to_bits(), 29.0_f32.to_bits());
    game.update_parachute(chute);
    assert!(game.entities[chute].parachute_opened());
    assert_eq!(game.object_position(rider)[2].to_bits(), 27.0_f32.to_bits());
    assert_eq!(
        game.entities[chute]
            .parachute
            .as_ref()
            .unwrap()
            .rider_attach_bone[2]
            .to_bits(),
        3.0_f32.to_bits()
    );
}

#[test]
fn hijacker_ejection_creates_on_actual_team_then_sets_full_world_position() {
    let (mut game, kind) = fixture_catalog();
    game.catalog[kind].name = "NativeChute".into();
    game.catalog[5]
        .behavior
        .hijackers
        .push(crate::skirmish::hijacker::UpdateRules {
            attach_bone: String::new(),
            parachute: "NativeChute".into(),
        });
    let prototype = game
        .team_membership
        .add_prototype(0, "ScriptedDrivers".into());
    let team = game.team_membership.create_team(prototype);
    let rider = game.spawn(5, 0, [200.0, 200.0], true);
    game.team_membership.set_team(rider, Some(team));
    let before = game.entities.len();
    game.eject_hijacker(rider, 0, [350.0, 250.0, 120.0]);
    assert_eq!(game.entities.len(), before + 1);
    let chute = game.entities[rider].contained_by().unwrap();
    assert_eq!(game.team_membership.team_of(chute), Some(team));
    assert_eq!(
        game.object_position(chute).map(f32::to_bits),
        [350.0_f32, 250.0, 120.0].map(f32::to_bits)
    );
    assert!(game.entities[rider].disabled_held());
    assert!(game.entities[rider].parachute_rider());
}

#[test]
fn held_rider_physics_forwards_its_landing_to_the_actual_parachute() {
    let (mut game, kind) = fixture_catalog();
    game.catalog[5].live_physics = Some(super::super::physical_ocl::LiveRules {
        motion: super::super::physical_ocl::MotionRules {
            center_of_mass_offset: 0.0,
            mass: 50.0,
            gravity: -64.0 / 900.0,
            stiffness: 0.5,
            forward_friction: 0.15,
            lateral_friction: 0.15,
            aerodynamic_friction: 0.0,
            angular_factor: 2.0,
            allow_bouncing: false,
        },
        resistance: 0.0,
        kill_when_resting: false,
        collision: super::super::physical_ocl::CollisionRules {
            stiffness: 0.5,
            rubble_height: 5.0,
            allow_force: true,
            weapons: [None, None],
        },
        shock_rotation: [0.0; 3],
        min_fall_speed: 999.0,
        fall_damage_factor: 1.0,
    });
    crate::skirmish::tests::register_update(&mut game.catalog[5], "PhysicsBehavior");
    let chute = game.spawn(kind, 0, [355.0, 255.0], true);
    game.set_object_position(chute, [355.0, 255.0, 20.0]);
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.board_parachute(rider, chute);
    game.update_parachute(chute);
    assert!(game.entities[rider].disabled_held());
    assert!(game.height_above_ground(rider) > 0.0);
    game.update_object_physics(rider);
    // Containment updates the rider's real Object position before HELD Physics
    // sees the airborne-to-ground edge; the leaf Parachute callback does not forward it.
    game.set_object_position(rider, [355.0, 255.0, 0.0]);
    game.tick += 1;
    game.update_object_physics(rider);
    assert_eq!(game.entities[rider].contained_by(), None);
    assert!(!game.entities[rider].parachute_rider());
    assert!(!game.entities[rider].disabled_held());
    assert!(game.entities[rider].alive());
    assert!(!game.entities[chute].alive());
}

#[test]
fn landing_probe_uses_current_partition_geometry_and_asymmetric_height_gate() {
    let (mut game, _) = fixture_catalog();
    let point = [355.0, 255.0];
    game.update_ground_contacts();
    assert!(game.default_position_clear([point[0], point[1], game.ground_height(point)], 0.0));
    let obstacle = game.spawn(5, 0, point, true);
    game.update_ground_contacts();
    assert!(!game.default_position_clear([point[0], point[1], game.ground_height(point)], 0.0));
    game.set_object_position(obstacle, [point[0], point[1], 6.0]);
    game.update_ground_contacts();
    assert!(game.default_position_clear([point[0], point[1], game.ground_height(point)], 0.0));
    game.layout
        .world_objects
        .push(crate::skirmish::MapWorldObject {
            template: "LandingObstacle".into(),
            kind_of: Vec::new(),
            position: [point[0], point[1], 0.0],
            angle: 0.0,
            geometry: Geometry {
                shape: Shape::Sphere,
                is_small: true,
                major: 5.0,
                minor: 5.0,
                height: 5.0,
            },
            original_team: String::new(),
        });
    let index = game.layout.world_objects.len() - 1;
    game.observe_map_partition(index);
    game.update_ground_contacts();
    assert!(!game.default_position_clear([point[0], point[1], game.ground_height(point)], 0.0));
    game.layout.world_objects[index].position[2] = -6.0;
    game.observe_map_partition(index);
    game.update_ground_contacts();
    // Sphere intersection alone would overlap: native positive-height gating
    // excludes a sphere whose upper edge is below the probe origin.
    assert!(game.default_position_clear([point[0], point[1], game.ground_height(point)], 0.0));
}
