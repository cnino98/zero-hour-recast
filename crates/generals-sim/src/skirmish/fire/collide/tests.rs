use super::*;
use crate::skirmish::{DamageType, Faction, Kind, tests};

fn rules(name: &str, damage: f32) -> Rules {
    let mut weapon = WeaponRules::unarmed();
    weapon.name = name.into();
    weapon.damage = damage;
    weapon.range = 100.0;
    weapon.delay = 10.0;
    weapon.clip_size = 1;
    weapon.damage_type = DamageType::Unresistable;
    Rules {
        weapon,
        required: 1 << 11,
        forbidden: 1 << 12,
        fire_once: true,
    }
}

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let mut tree = tests::fixture_type(Faction::Civilian, Kind::Effect);
    tree.name = "BurningContactTree".into();
    tree.behavior.collision_weapons =
        vec![rules("FirstContact", 7.0), rules("SecondContact", 11.0)];
    tests::register_behavior(&mut tree, "FireWeaponCollide", 0x10);
    tests::register_behavior(&mut tree, "FireWeaponCollide", 0x10);
    tree.behavior
        .flammable
        .push(crate::skirmish::fire::flammable::Rules {
            burned_delay: 0,
            duration: 100,
            damage_delay: 0,
            damage: 0.0,
            limit: 1.0,
            expiration: 60,
            sound: String::new(),
        });
    tests::register_update(&mut tree, "FlammableUpdate");
    let kind = game.catalog.len();
    game.catalog.push(tree);
    let tree = game.spawn(kind, game.neutral_owner(), [400.0, 400.0], true);
    let mut victim = tests::fixture_type(Faction::Usa, Kind::Effect);
    victim.name = "ContactVictim".into();
    let kind = game.catalog.len();
    game.catalog.push(victim);
    let victim = game.spawn(kind, 0, [400.0, 400.0], true);
    (game, tree, victim)
}

#[test]
fn ordered_collision_weapons_reload_each_contact_and_fire_once_keeps_native_unassigned_latch() {
    let (mut game, tree, victim) = fixture();
    assert!(
        game.entities[tree]
            .behaviors
            .collision_weapons
            .iter()
            .all(|state| state.weapon.clip_remaining == 0
                && matches!(state.weapon.status, WeaponStatus::OutOfAmmo))
    );
    game.object_contact(tree, victim, false);
    assert_eq!(game.entities[victim].health.to_bits(), 100.0_f32.to_bits());
    game.ignite_flammable(tree, 0);
    game.object_contact(tree, victim, false);
    assert_eq!(game.entities[victim].health.to_bits(), 82.0_f32.to_bits());
    let names: Vec<_> = game
        .presentation
        .shots
        .iter()
        .map(|shot| {
            game.catalog[shot.weapon_type]
                .weapon_template(shot.weapon_index)
                .unwrap()
                .name
                .as_str()
        })
        .collect();
    assert_eq!(names, ["FirstContact", "SecondContact"]);
    game.object_contact(tree, victim, false);
    assert_eq!(game.entities[victim].health.to_bits(), 64.0_f32.to_bits());
    assert!(
        game.entities[tree]
            .behaviors
            .collision_weapons
            .iter()
            .all(|state| !state.ever_fired)
    );
    assert!(
        game.entities[tree]
            .behaviors
            .collision_weapons
            .iter()
            .all(|state| state.weapon.max_shots == Some(0x7fff_fffd))
    );
    let shots = game.presentation.shots.len();
    game.object_collide(tree, None);
    assert_eq!(game.presentation.shots.len(), shots);
}

#[test]
fn actual_burned_transition_stops_contacts_and_missing_instance_data_is_rejected() {
    let (mut game, tree, victim) = fixture();
    let kind = game.entities[tree].type_index;
    game.catalog[kind].behavior.flammable[0].burned_delay = 1;
    game.ignite_flammable(tree, 0);
    game.tick = 1;
    game.run_object_updates();
    assert!(game.entities[tree].burned());
    game.object_contact(tree, victim, false);
    assert_eq!(game.entities[victim].health.to_bits(), 100.0_f32.to_bits());
    game.catalog[kind].behavior.collision_weapons.pop();
    assert!(crate::skirmish::crushing::validate(&game.catalog[kind]).is_err());
}
