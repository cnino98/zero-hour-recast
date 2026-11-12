use super::*;

pub(in crate::skirmish) fn rules() -> Rules {
    Rules {
        rows: 2,
        columns: 2,
        has_runways: true,
        approach_height: 50.0,
        deck_height: 0.0,
        park_in_hangars: false,
        heal_per_second: 10.0,
    }
}

pub(in crate::skirmish) fn bind_bones(unit: &mut crate::skirmish::UnitType) {
    let mut transforms = std::collections::BTreeMap::new();
    let matrix = |x, y| [[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, 0.0]];
    for row in 1_u8..=2 {
        for column in 1_u8..=2 {
            let x = f32::from(row - 1) * 30.0;
            let y = f32::from(column - 1) * 100.0;
            transforms.insert(format!("runway{column}park{row}han"), matrix(x, y));
            transforms.insert(format!("runway{column}parking{row}"), matrix(x + 20.0, y));
            transforms.insert(format!("runway{column}prep{row}"), matrix(90.0, y));
        }
    }
    for column in 1_u8..=2 {
        let y = f32::from(column - 1) * 100.0;
        transforms.insert(format!("runwaystart{column}"), matrix(100.0, y));
        transforms.insert(format!("runwayend{column}"), matrix(300.0, y));
    }
    transforms.insert("helipark01".into(), matrix(-40.0, 0.0));
    unit.pristine_draws = vec![crate::skirmish::model_bones::Draw {
        ignored: std::collections::BTreeSet::new(),
        states: vec![crate::skirmish::model_bones::State {
            conditions: vec![std::collections::BTreeSet::new()],
            transforms,
        }],
    }];
}

#[test]
fn queued_jets_reserve_doors_in_row_order_and_helipads_do_not_consume_slots() {
    let mut doors = super::doors::State::new();
    let mut state = built_state([0.0; 3], 0.0);
    for door in 0..4 {
        assert_eq!(state.reserve_door(false).unwrap(), Some(door));
    }
    assert!(state.reserve_door(false).is_err());
    assert!(!state.available(false, |_| true));
    assert!(state.available(true, |_| true));
    assert_eq!(state.reserve_door(true).unwrap(), None);
    state.unreserve_door(Some(1));
    assert_eq!(state.reserve_door(false).unwrap(), Some(1));
    state.exit(Some(1), 51).unwrap();
    assert_eq!(state.reserve_space(51, 1, Some(&mut doors)), Some(1));
    let (_, set) = doors.update_doors(
        &super::DoorRules {
            count: 4,
            opening: 60,
            waiting: 90,
            closing: 60,
            complete: 30,
        },
        1,
    );
    assert!(super::doors::condition_names(set).contains(&"DOOR_2_OPENING"));
    assert!(state.exit(Some(1), 52).is_err());
    assert!(state.reserve_space(52, 1, Some(&mut doors)).is_none());
}

#[test]
fn availability_peeks_dead_objects_without_mutating_claims_or_runways() {
    let mut doors = super::doors::State::new();
    let mut state = built_state([0.0; 3], 0.0);
    for id in 10..14 {
        assert!(state.reserve_space(id, 1, Some(&mut doors)).is_some());
    }
    assert!(state.reserve_runway(10, false));
    assert!(!state.available(false, |_| true));
    assert!(state.available(false, |id| id != 10));
    assert_eq!(state.space_index(10), Some(0));
    assert_eq!(state.runway_owner(0), Some(10));
    state.purge(20, Some(&mut doors), |id| id != 10);
    assert!(state.space_index(10).is_none());
    assert_eq!(state.runway_owner(0), None);

    assert_eq!(state.reserve_space(14, 1, Some(&mut doors)), Some(0));
}

#[test]
fn fixed_profile_postpones_upper_row_then_transfers_takeoff_to_prep() {
    let rules = rules();
    let mut doors = super::doors::State::new();
    let mut state = built_state([0.0; 3], 0.0);
    for id in 10..14 {
        assert!(state.reserve_space(id, 1, Some(&mut doors)).is_some());
    }
    assert!(!state.reserve_runway(12, false));
    assert_eq!(state.runway_owner(0), None);
    assert!(state.reserve_runway(10, false));
    assert!(!state.reserve_runway(12, false));
    assert_eq!(state.runways[0].next, Some(12));
    state.transfer_runway(10);
    assert_eq!(state.runway_owner(0), Some(12));
    assert!(state.reserve_runway(12, false));
    let info = state.info(12, &rules, 0.0).unwrap();
    assert_eq!(info.start.map(f32::to_bits), info.prep.map(f32::to_bits));
    assert_eq!(info.takeoff_distance.to_bits(), 200.0_f32.to_bits());
    state.release_runway(10);
    assert_eq!(state.runway_owner(0), Some(12));
    state.release_runway(12);
    assert_eq!(state.runway_owner(0), None);
}

#[test]
fn landing_does_not_queue_behind_takeoff_and_resets_the_upper_row_postponement() {
    let mut doors = super::doors::State::new();
    let mut state = built_state([0.0; 3], 0.0);
    for id in 10..14 {
        assert!(state.reserve_space(id, 1, Some(&mut doors)).is_some());
    }
    assert!(!state.reserve_runway(12, false));
    assert!(state.reserve_runway(10, false));
    assert!(!state.reserve_runway(12, true));
    assert_eq!(state.runways[0].next, None);
    state.release_runway(10);
    assert!(state.reserve_runway(12, true));
    state.release_runway(12);
    assert!(!state.reserve_runway(12, false));
    assert!(state.reserve_runway(12, false));
}

#[test]
fn rotated_hangar_uses_original_approach_and_parking_orientation_for_offset() {
    let mut rules = rules();
    rules.park_in_hangars = true;
    rules.deck_height = 7.0;
    let mut doors = super::doors::State::new();
    let mut state = built_state([500.0, 200.0, 30.0], std::f32::consts::PI);
    state.spaces[0].geometry.hangar.direction = [0.0, -1.0, 0.0];
    state.reserve_space(10, 1, Some(&mut doors)).unwrap();
    let info = state.info(10, &rules, 10.0).unwrap();
    assert_eq!(info.parking.position[0].to_bits(), 490.0_f32.to_bits());
    assert!((info.parking.orientation() + std::f32::consts::FRAC_PI_2).abs() < 0.000_001);
    assert_eq!(info.approach[0].to_bits(), 50.0_f32.to_bits());
    assert_eq!(info.approach[2].to_bits(), 87.0_f32.to_bits());
    assert_eq!(info.hangar.position[0].to_bits(), 500.0_f32.to_bits());
}

#[test]
fn healing_uses_shared_six_frame_wake_and_new_member_resets_all_healees() {
    let mut doors = super::doors::State::new();
    let mut state = built_state([0.0; 3], 0.0);
    state.heal(1, true, 100);
    state.heal(1, true, 104);
    assert!(state.healing(105).is_empty());
    assert_eq!(state.healing(106), [1]);
    state.heal(2, true, 108);
    assert!(state.healing(112).is_empty());
    assert_eq!(state.healing(114), [1, 2]);
    state.purge(115, Some(&mut doors), |id| id != 1);
    assert!(state.healing(120).is_empty());
    assert_eq!(state.healing(121), [2]);
    state.heal(2, false, 122);
    assert_eq!(state.next_heal, 0x3fff_ffff);
    assert!(state.healing(1000).is_empty());
}

fn built_state(position: [f32; 3], facing: f32) -> State {
    use crate::skirmish::{Faction, Kind, tests};
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::Gla, Kind::Factory);
    unit.parking = Some(rules());
    bind_bones(&mut unit);
    tests::register_update(&mut unit, "ParkingPlaceBehavior");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.new_object(kind, 0, true);
    game.set_object_transform(
        id,
        position,
        crate::skirmish::physical_ocl::transform::yaw(facing),
    );
    game.build_parking_info(id);
    game.entities[id].parking.take().unwrap()
}
