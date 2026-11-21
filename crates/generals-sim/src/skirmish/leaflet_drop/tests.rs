use super::*;
use crate::skirmish::{DeathType, Faction, Kind, tests};

#[test]
fn falling_leaflets_emit_once_and_impact_disables_only_nearby_enemy_troops() {
    let mut game = tests::game();
    let mut container = tests::fixture_type(Faction::Usa, Kind::Effect);
    container.behavior.leaflet_drops.push(Rules {
        delay: 75,
        disabled: 600,
        radius: 110.0,
        particle: "LeafletParticles1".into(),
    });
    tests::register_update(&mut container, "LeafletDropBehavior");
    let container_type = game.catalog.len();
    game.catalog.push(container);
    let mut tank = tests::fixture_type(Faction::China, Kind::Tank);
    tank.kind_of.push("VEHICLE".into());
    let tank_type = game.catalog.len();
    game.catalog.push(tank);
    let mut infantry = tests::fixture_type(Faction::China, Kind::Infantry);
    infantry.kind_of.push("INFANTRY".into());
    let infantry_type = game.catalog.len();
    game.catalog.push(infantry);
    let structure_type = game.type_for(1, Kind::CommandCenter).unwrap();
    let container = game.spawn(container_type, 0, [300.0, 300.0], true);
    let tank = game.spawn(tank_type, 1, [350.0, 300.0], true);
    let infantry = game.spawn(infantry_type, 1, [250.0, 300.0], true);
    let ally = game.spawn(tank_type, 0, [300.0, 350.0], true);
    let outside = game.spawn(tank_type, 1, [500.0, 300.0], true);
    let building = game.spawn(structure_type, 1, [300.0, 250.0], true);
    game.update_ground_contacts();

    game.tick = 1;
    let sleep = i32::from_ne_bytes(FOREVER.to_ne_bytes());
    assert_eq!(game.update_leaflet_drop(container, 0), sleep);
    game.tick = 2;
    assert_eq!(game.update_leaflet_drop(container, 0), sleep);
    let [effect] = &game.presentation.attached_particles[..] else {
        panic!("LeafletDropBehavior must attach exactly one falling particle system");
    };
    assert_eq!(effect.object, container);
    assert_eq!(effect.name, "LeafletParticles1");
    assert!(!game.object_disabled(tank));
    assert!(!game.object_disabled(infantry));

    game.tick = 50;
    game.kill_object(container, DeathType::Normal);
    for victim in [tank, infantry] {
        assert_eq!(game.entities[victim].emp_until, 650);
        assert!(game.object_disabled(victim));
    }
    for unaffected in [ally, outside, building] {
        assert_eq!(game.entities[unaffected].emp_until, 0);
    }
    game.set_hacked_until(tank, 800);
    game.tick = 649;
    game.expire_emp();
    assert!(game.object_disabled(infantry));
    game.tick = 650;
    game.expire_emp();
    assert!(!game.object_disabled(infantry));
    assert_ne!(game.object_disabled_mask(tank) & 2, 0);
    assert_eq!(game.object_disabled_mask(tank) & 4, 0);
}
