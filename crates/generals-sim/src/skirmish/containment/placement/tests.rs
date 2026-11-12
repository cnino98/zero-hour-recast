use crate::skirmish::containment;

#[test]
fn nonenclosing_firepoint_keeps_full_basis_and_turret_mode_discards_bone_rotation() {
    for in_turret in [false, true] {
        let (mut game, old) = containment::tests::setup(0.0, 0.0);
        let kind = game.entities[old].type_index;
        let rules = game.catalog[kind].contain.as_mut().unwrap();
        rules.passengers_in_turret = in_turret;
        rules.art = std::array::from_fn(|_| containment::Art {
            exits: Vec::new(),
            fire_points: vec![containment::FirePoint {
                position: [4.0, 6.0, 8.0],
                basis: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            }],
        });
        game.catalog[kind].turret = Some(crate::skirmish::turret::Rules::source_defaults());
        let host = containment::tests::respawn_contain_family(&mut game, old, "OverlordContain");
        assert!(game.has_native_turret(host, 0));
        let host_basis = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
        game.set_object_transform(host, [355.0, 255.0, 20.0], host_basis);
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.add_to_contain(rider, host);
        let (position, basis) = game.object_transform(rider);
        assert_eq!(
            position.map(f32::to_bits),
            [363.0_f32, 261.0, 16.0].map(f32::to_bits)
        );
        let expected = if in_turret {
            host_basis
        } else {
            [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        };
        assert_eq!(
            basis.map(|row| row.map(f32::to_bits)),
            expected.map(|row| row.map(f32::to_bits))
        );
    }
}

#[test]
fn round_robin_starts_at_newest_rider_and_literal_first_receives_full_basis() {
    let (mut game, old) = containment::tests::setup(0.0, 0.0);
    let kind = game.entities[old].type_index;
    game.catalog[kind].contain.as_mut().unwrap().capacity = 4;
    let host = containment::tests::respawn_contain_family(&mut game, old, "OverlordContain");
    let mut riders = Vec::new();
    for _ in 0..4 {
        let rider = game.spawn(5, 0, [355.0, 255.0], true);
        game.set_object_transform(
            rider,
            [355.0, 255.0, 0.0],
            [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        );
        game.add_to_contain(rider, host);
        riders.push(rider);
    }
    assert_eq!(
        game.object_position(riders[0]).map(f32::to_bits),
        [353.0, 252.0, 5.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(riders[1]).map(f32::to_bits),
        [357.0, 258.0, 5.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(riders[2]).map(f32::to_bits),
        [353.0, 252.0, 5.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(riders[3]).map(f32::to_bits),
        [357.0, 258.0, 5.0].map(f32::to_bits)
    );
    // Native push_back makes the oldest literal rider the nonenclosing rider.
    assert_eq!(
        game.object_transform(riders[0])
            .1
            .map(|row| row.map(f32::to_bits)),
        [[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|row| row.map(f32::to_bits))
    );
    assert_eq!(
        game.object_transform(riders[3])
            .1
            .map(|row| row.map(f32::to_bits)),
        [[1.0_f32, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]].map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn no_firepoint_fallback_moves_to_object_center_without_rotating_passenger() {
    let (mut game, old) = containment::tests::setup(0.0, 0.0);
    let kind = game.entities[old].type_index;
    let rules = game.catalog[kind].contain.as_mut().unwrap();
    rules.passengers_in_turret = true;
    for art in &mut rules.art {
        art.fire_points.clear();
    }
    let host = containment::tests::respawn_contain_family(&mut game, old, "OverlordContain");
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.set_object_orientation(rider, 0.5);
    let original = game.object_transform(rider).1;
    game.add_to_contain(rider, host);
    game.set_object_orientation(host, 1.0);
    game.set_object_position(host, [400.0, 300.0, 12.0]);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [400.0, 300.0, 12.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_transform(rider)
            .1
            .map(|row| row.map(f32::to_bits)),
        original.map(|row| row.map(f32::to_bits))
    );
}

#[test]
fn turret_position_changes_only_at_native_containment_notifications() {
    let (mut game, old) = containment::tests::setup(0.0, 0.0);
    let kind = game.entities[old].type_index;
    game.catalog[kind]
        .contain
        .as_mut()
        .unwrap()
        .passengers_in_turret = true;
    let mut turret = crate::skirmish::turret::Rules::source_defaults();
    turret.mounts[0].yaw = [2.0, 0.0, 0.0];
    game.catalog[kind].turret = Some(turret);
    let host = containment::tests::respawn_contain_family(&mut game, old, "OverlordContain");
    let rider = game.spawn(5, 0, [355.0, 255.0], true);
    game.add_to_contain(rider, host);
    let original = game.object_position(rider);
    game.entities[host].ai.turret.angle = std::f32::consts::FRAC_PI_2;
    game.update_attachments();
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        original.map(f32::to_bits)
    );
    game.containment_transform_changed(host);
    let position = game.object_position(rider);
    assert!((position[0] - 354.0).abs() < 0.000_01);
    assert!((position[1] - 255.0).abs() < 0.000_01);
    assert_eq!(position[2].to_bits(), 5.0_f32.to_bits());
}

#[test]
fn turret_firepoint_admission_rejects_undefined_query_but_keeps_no_art_fallback() {
    let (mut game, host) = containment::tests::setup(0.0, 0.0);
    let kind = game.entities[host].type_index;
    game.catalog[kind]
        .contain
        .as_mut()
        .unwrap()
        .passengers_in_turret = true;
    assert!(
        containment::validate(&game.catalog)
            .unwrap_err()
            .contains("actual AI and MAIN turret")
    );
    game.catalog[kind].turret = Some(crate::skirmish::turret::Rules::source_defaults());
    containment::validate(&game.catalog).unwrap();
    game.catalog[kind].ai.update = crate::skirmish::UnitAiUpdate::Absent;
    assert!(containment::validate(&game.catalog).is_err());
    game.catalog[kind].turret = None;
    for art in &mut game.catalog[kind].contain.as_mut().unwrap().art {
        art.fire_points.clear();
    }
    containment::validate(&game.catalog).unwrap();
}
