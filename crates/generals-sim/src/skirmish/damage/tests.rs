use super::{AreaDamage, Source};
use crate::skirmish::{
    DamageType, DeathType, Faction, Kind, instant_death,
    tests::{fixture_type, game},
};

#[test]
fn projectile_splash_excludes_its_producer_unless_designated_as_primary() {
    let mut game = game();
    let weapon = &mut game.catalog[6].weapon;
    weapon.damage = 10.0;
    weapon.damage_type = DamageType::Unresistable;
    weapon.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 100.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 1 | 2 | 4,
    });
    let launcher = game.spawn(6, 0, [300.0, 200.0], true);
    let projectile = game.spawn(5, 0, [325.0, 200.0], true);
    game.entities[projectile].producer = Some(launcher);
    let victim = game.spawn(5, 1, [350.0, 200.0], true);
    game.update_ground_contacts();
    let initial = game.entities[launcher].health;
    let origin = Source::Projectile {
        launcher,
        object: projectile,
    };
    game.deal_bound_weapon_damage_with_radius(
        &origin,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[launcher].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[launcher].weapon_slot(),
        },
        None,
        [325.0, 200.0, 0.0],
        1.0,
        1.0,
    );
    assert_eq!(game.entities[launcher].health.to_bits(), initial.to_bits());
    assert_eq!(
        game.entities[projectile].health.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(game.entities[victim].health.to_bits(), 90.0_f32.to_bits());
    game.deal_bound_weapon_damage_with_radius(
        &origin,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[launcher].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[launcher].weapon_slot(),
        },
        Some(launcher),
        [0.0; 3],
        1.0,
        1.0,
    );
    assert_eq!(
        game.entities[launcher].health.to_bits(),
        (initial - 10.0).to_bits()
    );
}

#[test]
fn splash_visits_inactive_body_and_defers_its_actual_destroy_callback() {
    let mut game = game();
    game.catalog[6].weapon.damage_type = DamageType::Unresistable;
    game.catalog[6].weapon.area = Some(AreaDamage {
        at_source: false,
        primary_radius: 100.0,
        secondary_radius: 0.0,
        secondary_damage: 0.0,
        affects: 2,
    });
    let mut field = fixture_type(Faction::Gla, Kind::Infantry);
    field.name = "InactiveDamageFixture".into();
    field.body_kind = crate::skirmish::BodyKind::Inactive;
    field.behavior.instant_death = vec![instant_death::Rules {
        death_mask: 1 << DeathType::Normal as u32,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
        effects: vec![Some("FieldDestroyed".into())],
        objects: Vec::new(),
        weapons: Vec::new(),
    }];
    crate::skirmish::tests::register_behavior(&mut field, "InstantDeathBehavior", 2);
    let kind = game.catalog.len();
    game.catalog.push(field);
    let source = game.spawn(6, 0, [300.0, 200.0], true);
    let target = game.spawn(kind, 1, [350.0, 200.0], true);
    game.update_ground_contacts();
    assert!(game.entities[target].is_effectively_dead());
    game.deal_bound_weapon_damage(
        source,
        crate::skirmish::WeaponBinding {
            type_index: game.entities[source].type_index,
            weapon_index: 0,
            weapon_slot: game.entities[source].weapon_slot(),
        },
        None,
        [350.0, 200.0, 0.0],
        1.0,
    );
    assert_eq!(
        game.presentation.object_fx_events.last().unwrap().effect,
        "FieldDestroyed"
    );
    assert!(game.entities[target].exists());
    game.process_update_destruction();
    assert!(!game.entities[target].exists());
}
