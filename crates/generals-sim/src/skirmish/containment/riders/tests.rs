use crate::skirmish::{
    AreaDamage, ConditionalWeapon, Skirmish, WeaponRules, WeaponTemplateSet,
    containment::{Payload, tests::setup},
    weapons,
};

fn armed_transport() -> (Skirmish, usize) {
    let (mut game, carrier) = setup(0.0, 0.1);
    let unit = &mut game.catalog[game.entities[carrier].type_index];
    let rules = unit.contain.as_mut().unwrap();
    rules.weapons.upgrade_transport = true;
    rules.capacity = 4;
    rules.allowed.push("VEHICLE".into());
    let mut dummy = WeaponRules::unarmed();
    dummy.name = "PassengerDummyWeapon".into();
    dummy.damage = 1.0;
    dummy.range = 150.0;
    dummy.clip_size = 2;
    unit.weapon_set_rules.templates.push(ConditionalWeapon {
        rules: dummy,
        flags: 1,
        slot: 0,
    });
    unit.weapon_set_rules.replacements = vec![
        WeaponTemplateSet {
            flags: 0,
            weapons: [None; 3],
            share_lock: false,
        },
        WeaponTemplateSet {
            flags: 1,
            weapons: [Some(7), None, None],
            share_lock: false,
        },
    ];
    game.entities[carrier].weapon_set = weapons::State::new(unit);
    (game, carrier)
}

#[test]
fn boarding_and_last_armed_exit_change_sets_without_refilling_for_other_riders() {
    let (mut game, carrier) = armed_transport();
    let first = game.spawn(5, 0, [0.0; 2], true);
    let second = game.spawn(5, 0, [0.0; 2], true);
    assert!(!game.weapon_available(carrier, 0));
    game.board(first, carrier);
    assert_eq!(game.entities[carrier].weapon_index(0), 7);
    game.entities[carrier].weapon_set.weapons[0].clip_remaining = 1;
    game.entities[carrier].weapon_set.weapons[0].cooldown = 0.5;
    game.board(second, carrier);
    game.remove_passenger(first, carrier);
    assert_eq!(game.entities[carrier].weapon_index(0), 7);
    assert_eq!(
        game.entities[carrier].weapon_set.weapons[0].clip_remaining,
        1
    );
    assert_eq!(
        game.entities[carrier].weapon_set.weapons[0]
            .cooldown
            .to_bits(),
        0.5_f32.to_bits()
    );
    game.remove_passenger(second, carrier);
    assert!(!game.weapon_available(carrier, 0));
    assert!(!game.entities[carrier].weapon_set.upgraded);
}

#[test]
fn only_infantry_with_noncontact_allocated_damage_slots_enable_the_carrier() {
    let (mut game, carrier) = armed_transport();
    game.catalog[5].weapon.range = 10.0;
    let contact = game.spawn(5, 0, [0.0; 2], true);
    let vehicle = game.spawn(6, 0, [0.0; 2], true);
    game.board(contact, carrier);
    game.board(vehicle, carrier);
    assert!(!game.weapon_available(carrier, 0));
    let mut splash = WeaponRules::unarmed();
    splash.name = "StationFixtureSplash".into();
    splash.range = 75.0;
    splash.clip_size = 1;
    splash.reload_seconds = 6.0;
    splash.reload = crate::skirmish::ClipReload::None;
    splash.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 0.0,
        secondary_radius: 10.0,
        secondary_damage: 3.0,
        affects: 2,
    });
    let mut infantry = crate::skirmish::tests::fixture_type(
        game.players[0].faction,
        crate::skirmish::Kind::Infantry,
    );
    infantry.weapon.damage = 0.0;
    infantry.tertiary_weapon = Some(splash);
    let kind = game.catalog.len();
    game.catalog.push(infantry);
    let artillery = game.spawn(kind, 0, [0.0; 2], true);
    game.switch_weapon(artillery, 2).unwrap();
    let position = game.object_position(artillery);
    let target = crate::skirmish::damage::WeaponTarget {
        object: None,
        position,
        projectile_position: position,
        scatter_radius: 0.0,
    };
    assert!(
        game.fire_current_weapon_payload(artillery, target)
            .is_some()
    );
    game.board(artillery, carrier);
    assert!(game.weapon_available(carrier, 0));
    // Ammo/readiness and AutoChooseSources are not part of isDamageWeapon.
    assert_eq!(
        game.entities[artillery].weapon_set.weapons[2].clip_remaining,
        0
    );
    game.remove_passenger(artillery, carrier);
    assert!(!game.weapon_available(carrier, 0));
    game.remove_passenger(contact, carrier);
    assert!(!game.weapon_available(carrier, 0));
}

#[test]
fn native_initial_payload_arms_once_and_rider_death_clears_the_last_weapon() {
    let (mut game, carrier) = armed_transport();
    game.catalog[5].name = "TankHunterPayload".into();
    let kind = game.entities[carrier].type_index;
    game.catalog[kind].contain.as_mut().unwrap().payload = Some(Payload {
        template: "TankHunterPayload".into(),
        count: 2,
    });
    game.create_transport_payload(carrier);
    assert!(game.weapon_available(carrier, 0));
    let riders = game.passengers(carrier).to_vec();
    assert_eq!(riders.len(), 2);
    game.create_transport_payload(carrier);
    assert_eq!(game.passengers(carrier), riders);
    for id in riders {
        let health = game.entities[id].health;
        game.apply_hit(id, carrier, health);
    }
    assert!(game.passengers(carrier).is_empty());
    assert!(!game.weapon_available(carrier, 0));
}

#[test]
fn zero_damage_disarm_counts_as_a_weapon_only_when_it_is_not_contact_range() {
    let (mut game, carrier) = armed_transport();
    let weapon = &mut game.catalog[5].weapon;
    weapon.damage = 0.0;
    weapon.damage_type = crate::skirmish::DamageType::Disarm;
    weapon.range = 10.0;
    let contact = game.spawn(5, 0, [0.0; 2], true);
    game.board(contact, carrier);
    assert!(!game.weapon_available(carrier, 0));
    game.remove_passenger(contact, carrier);
    game.catalog[5].weapon.range = 100.0;
    let ranged = game.spawn(5, 0, [0.0; 2], true);
    game.board(ranged, carrier);
    assert!(game.weapon_available(carrier, 0));
    assert_eq!(game.catalog[5].weapon.damage.to_bits(), 0.0_f32.to_bits());
    game.remove_passenger(ranged, carrier);
    assert!(!game.weapon_available(carrier, 0));
}
