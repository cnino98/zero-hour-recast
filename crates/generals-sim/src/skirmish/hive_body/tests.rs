use super::Rules;
use crate::skirmish::{DamageType, Skirmish, containment::tests::setup};

fn fixture() -> (Skirmish, usize, usize, usize) {
    let (mut game, carrier) = setup(0.0, 0.0);
    let index = game.entities[carrier].type_index;
    game.catalog[index].hive_body = Some(Rules {
        propagate: 1 << DamageType::Sniper as usize,
        swallow: 0,
    });
    game.catalog[index].armor[crate::skirmish::DamageType::Sniper as usize] = 0.0;
    let first = game.spawn(5, 0, [355.0, 255.0], true);
    let second = game.spawn(5, 0, [355.0, 255.0], true);
    game.board(first, carrier);
    game.board(second, carrier);
    (game, carrier, first, second)
}

#[test]
fn sniper_uses_closest_rider_and_its_armor_before_protected_structure_armor() {
    let (mut game, carrier, first, second) = fixture();
    let shooter = game.spawn(6, 1, [400.0, 255.0], true);
    game.entities[first].position = [340.0, 255.0];
    game.entities[second].position = [370.0, 255.0];
    game.catalog[5].armor[crate::skirmish::DamageType::Sniper as usize] = 0.5;
    game.catalog[6].weapon.damage_type = DamageType::Sniper;
    game.catalog[6].weapon.damage = 20.0;
    game.catalog[6].weapon.area = None;
    let before = game.entities[second].health;
    let host_health = game.entities[carrier].health;
    game.deal_bound_weapon_damage(
        shooter,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[shooter].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[shooter].weapon_slot(),
        },
        Some(carrier),
        game.object_position(carrier),
        1.0,
    );
    assert_eq!(
        game.entities[second].health.to_bits(),
        (before - 10.0).to_bits()
    );
    assert_eq!(game.entities[first].health.to_bits(), before.to_bits());
    assert_eq!(
        game.entities[carrier].health.to_bits(),
        host_health.to_bits()
    );
}

#[test]
fn hive_nearest_ties_use_entry_order_and_empty_swallow_requires_existing_shooter() {
    let (mut game, carrier, first, second) = fixture();
    let shooter = game.spawn(6, 1, [400.0, 255.0], true);
    game.entities[first].position = [370.0, 250.0];
    game.entities[second].position = [370.0, 260.0];
    assert_eq!(
        game.hive_damage_target(carrier, shooter, DamageType::Sniper),
        Some(first)
    );
    assert_eq!(
        game.hive_damage_target(carrier, shooter, DamageType::Explosion),
        Some(carrier)
    );
    game.remove_passenger(first, carrier);
    game.remove_passenger(second, carrier);
    assert_eq!(
        game.hive_damage_target(carrier, shooter, DamageType::Sniper),
        Some(carrier)
    );
    let index = game.entities[carrier].type_index;
    game.catalog[index].hive_body.as_mut().unwrap().swallow = 1 << DamageType::Sniper as usize;
    assert_eq!(
        game.hive_damage_target(carrier, shooter, DamageType::Sniper),
        None
    );
    assert_eq!(
        game.hive_damage_target(carrier, usize::MAX, DamageType::Sniper),
        Some(carrier)
    );
}
