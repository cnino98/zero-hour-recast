use crate::skirmish::{Order, Skirmish};

fn fixture() -> (Skirmish, usize) {
    let (mut game, carrier) = super::super::tests::setup(true);
    let unit = &mut game.catalog[game.entities[carrier].type_index];
    unit.kind_of.push("GARRISONABLE_UNTIL_DESTROYED".into());
    unit.contain
        .as_mut()
        .unwrap()
        .garrison
        .as_mut()
        .unwrap()
        .stations = Some(vec![[20.0, 10.0, 15.0], [-20.0, 10.0, 15.0]]);
    // Finish the real first-frame AI callback before placing the stationary
    // station fixture. Native Idle may snap its inherited mobile locomotor on
    // frames 0/1; station geometry below exercises later NORMAL callbacks.
    game.tick = 1;
    game.run_object_updates();
    game.set_object_position(carrier, [300.0, 200.0, 0.0]);
    game.entities[carrier].facing = 0.0;
    (game, carrier)
}

#[test]
fn station_passengers_keep_visible_world_positions_while_firing_and_removing() {
    let (mut game, carrier) = fixture();
    let first = game.spawn(5, 0, [320.0, 180.0], true);
    let second = game.spawn(5, 0, [280.0, 180.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.passengers(carrier), [first, second]);
    assert_eq!(
        game.object_position(first).map(f32::to_bits),
        [320.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        [280.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert!(!game.entities[first].masked());
    assert!(game.entities[first].garrisoned());
    assert!(game.passenger_can_fire(first));
    let target = game.spawn(6, 1, [350.0, 210.0], true);
    game.set_order(first, Order::Attack(target));
    assert!(game.prepare_garrison_attack(first, target));
    game.match_garrison_points(carrier);
    assert_eq!(
        game.object_position(first).map(f32::to_bits),
        [320.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert_eq!(game.garrison_effect(carrier, 0), None);
    game.remove_passenger(first, carrier);
    assert_eq!(game.entities[first].contained_by(), None);
    assert_eq!(
        game.object_position(first).map(f32::to_bits),
        [320.0_f32, 210.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        [280.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
}

#[test]
fn native_redeployment_compacts_stations_on_entry_and_uses_pristine_bones_after_damage() {
    let (mut game, carrier) = fixture();
    let first = game.spawn(5, 0, [320.0, 180.0], true);
    let second = game.spawn(5, 0, [280.0, 180.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    game.tick += 1;
    game.run_object_updates();
    game.remove_passenger(first, carrier);
    let third = game.spawn(5, 0, [320.0, 180.0], true);
    game.board(third, carrier);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        [320.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(third).map(f32::to_bits),
        [280.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    game.entities[carrier].health = game.entities[carrier].max_health * 0.05;
    game.tick += 1;
    game.run_object_updates();
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        [320.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert_eq!(
        game.object_position(third).map(f32::to_bits),
        [280.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert!(!game.clearable_garrison(carrier));
}

#[test]
fn stationed_infantry_receive_area_damage_and_cannot_board_a_second_host() {
    let (mut game, carrier) = fixture();
    let rider = game.spawn(5, 0, [320.0, 180.0], true);
    game.board(rider, carrier);
    game.tick += 1;
    game.run_object_updates();
    let other = game.spawn(game.entities[carrier].type_index, 0, [400.0, 200.0], true);
    assert!(!game.can_enter(rider, other));
    let attacker = game.spawn(6, 1, [350.0, 210.0], true);
    let weapon = &mut game.catalog[6].weapon;
    weapon.damage = 10.0;
    weapon.area = Some(crate::skirmish::AreaDamage {
        at_source: false,
        primary_radius: 4.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 2,
    });
    let before = game.entities[rider].health;
    game.update_ground_contacts();
    game.deal_bound_weapon_damage(
        attacker,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[attacker].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[attacker].weapon_slot(),
        },
        Some(rider),
        game.object_position(rider),
        1.0,
    );
    assert!(game.entities[rider].health < before);
}

#[test]
fn station_removal_uses_both_coordinates_for_ground_height() {
    let (mut game, carrier) = fixture();
    let heights = (0..64_u16)
        .flat_map(|y| (0..64).map(move |_| f32::from(y) * 2.0))
        .collect();
    game.set_projectile_terrain(64, 64, [0.0, 0.0], heights, 10.0)
        .unwrap();
    let rider = game.spawn(5, 0, [320.0, 180.0], true);
    game.board(rider, carrier);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [320.0_f32, 210.0, 55.0].map(f32::to_bits)
    );
    game.remove_passenger(rider, carrier);
    assert_eq!(
        game.object_position(rider).map(f32::to_bits),
        [320.0_f32, 210.0, 42.0].map(f32::to_bits)
    );
}

#[test]
fn firing_condition_change_redeploys_vacant_first_station_in_same_native_update() {
    use crate::skirmish::{Faction, Kind, tests::fixture_type};
    let (mut game, carrier) = fixture();
    let index = game.entities[carrier].type_index;
    game.catalog[index].weapon = fixture_type(Faction::Gla, Kind::Tank).weapon;
    let target = game.spawn(6, 1, [350.0, 200.0], true);
    game.ai_busy_from_ai(target);
    let first = game.spawn(5, 0, [320.0, 180.0], true);
    let second = game.spawn(5, 0, [280.0, 180.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    // First boarding captures the neutral host and clears its previous order.
    // Issue the actual command afterwards: onEnter establishes Aim immediately.
    game.attack(carrier, target).unwrap();
    game.tick += 1;
    game.run_object_updates();
    // INITIAL has transitioned Aim -> Fire; FINAL recorded BETWEEN_FIRING.
    // Fire::onEnter does not spend a shot. The next INITIAL executes Fire::update.
    assert!(
        !game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == carrier)
    );
    game.remove_passenger(first, carrier);
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        [280.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    game.tick += 1;
    game.run_object_updates();
    assert!(
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == carrier)
    );
    assert!(
        game.model_flags(&game.entities[carrier], [0.5, 0.1])
            .contains("FIRING_A")
    );
    assert_eq!(
        game.object_position(second).map(f32::to_bits),
        [320.0_f32, 210.0, 15.0].map(f32::to_bits)
    );
    assert_eq!(game.passengers(carrier), [second]);
}
