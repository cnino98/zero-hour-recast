use super::*;
use crate::skirmish::containment::{GarrisonRules, tests::setup};

fn garrison() -> (Skirmish, usize) {
    let (mut game, carrier) = setup(0.0, 1.0);
    let index = game.entities[carrier].type_index;
    game.catalog[index].contain.as_mut().unwrap().garrison = Some(GarrisonRules {
        allow_enemies: false,
        allow_neutral: false,
        immune_to_clear: false,
        heal_frames: None,
        stations: None,
    });
    (game, carrier)
}

fn passenger(game: &mut Skirmish, carrier: usize, health: f32) -> usize {
    let id = game.spawn(
        5,
        game.entities[carrier].owner,
        game.entities[carrier].position,
        true,
    );
    game.entities[id].health = health;
    game.entities[id].max_health = health;
    game.entities[id].containment.carrier = Some(carrier);
    game.entities[carrier].containment.passengers.push(id);
    id
}

#[test]
fn garrison_bust_forces_exit_before_exact_hundred_damage_and_keeps_survivors() {
    let (mut game, carrier) = garrison();
    let weak = passenger(&mut game, carrier, 90.0);
    let strong = passenger(&mut game, carrier, 180.0);
    assert!(game.bustable_container(carrier));
    game.bust_occupants(
        carrier,
        0,
        Some((DamageType::ArmorPiercing, DeathType::Exploded)),
    );
    assert!(game.passengers(carrier).is_empty());
    assert_eq!(game.entities[weak].contained_by(), None);
    assert!(!game.entities[weak].alive());
    assert_eq!(game.entities[strong].contained_by(), None);
    assert_eq!(game.entities[strong].health.to_bits(), 80.0_f32.to_bits());
}

#[test]
fn empty_garrison_and_open_transport_preserve_distinct_bustability() {
    let (mut game, carrier) = garrison();
    game.bust_occupants(
        carrier,
        0,
        Some((DamageType::ArmorPiercing, DeathType::Exploded)),
    );
    assert!(game.passengers(carrier).is_empty());
    game.catalog[game.entities[carrier].type_index]
        .contain
        .as_mut()
        .unwrap()
        .garrison = None;
    assert!(!game.bustable_container(carrier));
}

#[test]
fn no_occupant_weapon_kills_all_and_station_garrison_is_bustable() {
    let (mut game, carrier) = garrison();
    let rider = passenger(&mut game, carrier, 900.0);
    game.catalog[game.entities[carrier].type_index]
        .contain
        .as_mut()
        .unwrap()
        .garrison
        .as_mut()
        .unwrap()
        .stations = Some(vec![[0.0; 3]]);
    assert!(game.bustable_container(carrier));
    game.bust_occupants(carrier, 0, None);
    assert!(!game.entities[rider].alive());
    assert!(game.passengers(carrier).is_empty());
}

pub(in crate::skirmish) fn template(upgrade: Option<String>) -> crate::skirmish::UnitType {
    use crate::skirmish::point_defense::tests::{missile_rules, missile_template};
    let mut unit = missile_template(missile_rules(), 1.0);
    unit.behavior.bunker_busters.push(Rules {
        upgrade,
        detonation_fx: "FX_BunkerBusterExplosion".into(),
        crash_fx: "Crash".into(),
        crash_frames: 4,
        occupant_damage: Some((DamageType::ArmorPiercing, DeathType::Exploded)),
        shockwave: None,
    });
    crate::skirmish::tests::register_update(&mut unit, "BunkerBusterBehavior");
    unit.behavior_modules.last_mut().unwrap().interfaces = 3;
    unit
}

#[test]
fn payload_checks_projectile_owner_upgrade_at_death_and_open_transport_keeps_rider() {
    use crate::skirmish::point_defense::tests::missile_from_template;
    for (upgraded, open) in [(false, false), (true, false), (true, true)] {
        let (mut game, carrier) = garrison();
        if open {
            game.catalog[game.entities[carrier].type_index]
                .contain
                .as_mut()
                .unwrap()
                .garrison = None;
        }
        let rider = passenger(&mut game, carrier, 180.0);
        // The impact itself must not destroy this test bunker before its occupants' callback.
        game.entities[carrier].health = 10_000.0;
        game.entities[carrier].max_health = 10_000.0;
        let position = game.object_position(carrier);
        let serial = missile_from_template(
            &mut game,
            position,
            0,
            Some(carrier),
            position,
            template(Some("Upgrade_AmericaBunkerBusters".into())),
        );
        let index = game.projectile_index(serial).unwrap();
        let object = game.projectiles[index].object.unwrap();
        let source = game.projectiles[index].source;
        game.entities[source].owner = 1;
        game.tick = 1;
        game.run_object_updates();
        assert_eq!(
            game.entities[object].behaviors.bunker_busters[0].victim,
            Some(carrier)
        );
        assert!(game.projectile_contact(object, Some(carrier)));
        assert!(
            game.entities[object]
                .projectile_flight
                .as_ref()
                .unwrap()
                .killing_self()
        );
        if upgraded {
            game.players[0]
                .upgrades
                .push("Upgrade_AmericaBunkerBusters".into());
        }
        game.kill_object(object, DeathType::Lasered);
        assert_eq!(
            game.entities[rider].contained_by(),
            if upgraded && !open {
                None
            } else {
                Some(carrier)
            }
        );
        assert_eq!(
            game.entities[rider].health.to_bits(),
            if upgraded && !open {
                80.0_f32
            } else {
                180.0_f32
            }
            .to_bits()
        );
        if upgraded {
            assert_eq!(
                game.presentation.object_fx_events.last().unwrap().object,
                carrier
            );
        }
    }
}

#[test]
fn tunnel_null_weapon_swaps_shared_list_before_recursive_endpoint_deaths() {
    use crate::skirmish::{
        AreaDamage, WeaponRules,
        containment::TunnelRules,
        tests::{death_fx, register_behavior},
    };
    let (mut game, old_transport) = setup(0.0, 0.0);
    let tunnel_type = game.entities[old_transport].type_index;
    game.destroy_maintenance_object(old_transport);
    let unit = &mut game.catalog[tunnel_type];
    unit.name = "GLATunnelNetwork".into();
    unit.contain.as_mut().unwrap().tunnel = Some(TunnelRules {
        full_heal_frames: 150.0,
        conditions: crate::skirmish::death_callbacks::Conditions {
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
        },
    });
    unit.contain.as_mut().unwrap().capacity = 10;
    unit.behavior_modules
        .retain(|module| module.family != "TransportContain");
    unit.update_modules.retain(|module| !matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior { family, .. } if family == "TransportContain"));
    crate::skirmish::tests::register_update(unit, "TunnelContain");
    let first = game.spawn(tunnel_type, 0, [355.0, 255.0], true);
    let second = game.spawn(tunnel_type, 0, [360.0, 255.0], true);
    let infantry = &mut game.catalog[5];
    infantry.behavior.death_fx.push(death_fx("RiderDie"));
    register_behavior(infantry, "FXListDie", 0x0082);
    infantry
        .behavior
        .death_weapons
        .push(crate::skirmish::death_weapons::Rules {
            weapon: Some(WeaponRules {
                name: "RecursiveEndpointDeathWeapon".into(),
                damage: 500.0,
                damage_type: DamageType::Unresistable,
                area: Some(AreaDamage {
                    at_source: true,
                    primary_radius: 30.0,
                    secondary_radius: 0.0,
                    secondary_damage: 0.0,
                    affects: 7,
                }),
                ..WeaponRules::unarmed()
            }),
            death_mask: u32::MAX,
            veterancy_mask: 15,
            exempt_status: 0,
            required_status: 0,
            removals: Vec::new(),
            upgrade_fx: String::new(),
            starts_active: true,
            triggers: Vec::new(),
            conflicts: Vec::new(),
            requires_all: false,
        });
    register_behavior(infantry, "FireWeaponWhenDeadBehavior", 0x0082);
    let weak = game.spawn(5, 0, game.entities[first].position, true);
    let other = game.spawn(5, 0, game.entities[second].position, true);
    game.board(weak, first);
    game.board(other, second);
    assert_eq!(game.entities[weak].contained_by(), Some(first));
    assert_eq!(game.entities[other].contained_by(), Some(second));
    assert_eq!(game.passengers(first), [weak, other]);
    game.update_ground_contacts();
    game.bust_occupants(first, 0, None);
    assert!(!game.entities[first].alive());
    assert!(!game.entities[second].alive());
    assert!(game.passengers(first).is_empty());
    assert!(game.passengers(second).is_empty());
    for rider in [weak, other] {
        assert!(!game.entities[rider].alive());
        assert_eq!(game.entities[rider].contained_by(), None);
        // The first death weapon destroys both endpoints recursively. The
        // second rider must still receive kill/DIE, not cave-in destroyObject.
        assert_eq!(
            game.presentation
                .object_fx_events
                .iter()
                .filter(|event| event.object == rider && event.effect == "RiderDie")
                .count(),
            1
        );
    }
}
