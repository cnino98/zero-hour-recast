use super::*;
use crate::skirmish::{DeathType, Faction, Kind, tests};

fn ordinary(name: &str) -> object_creation::List {
    object_creation::List {
        name: "NativeOnce".into(),
        objects: vec![object_creation::Object {
            names: vec![name.into()],
            count: 1,
            offset: [0.0; 3],
            disposition: physical_ocl::LIKE_EXISTING | physical_ocl::SEND_FLYING,
            health: [1.0; 2],
            lifetime: [0; 2],
            flags: object_creation::PRESERVE_LAYER,
            container: String::new(),
            invulnerable_time: 0,
            force_intensity: 0.5,
            force_magnitude: [0.0; 2],
            force_pitch: [0.0; 2],
            spin_rate: -1.0,
            formation_distances: [0.0; 3],
            fade_frames: 0,
            fade_sound: String::new(),
        }],
    }
}

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = tests::game();
    let mut child = tests::fixture_type(Faction::Civilian, Kind::Effect);
    child.name = "NoPhysicsChild".into();
    child.ai.update = crate::skirmish::UnitAiUpdate::Absent;
    child
        .behavior_modules
        .retain(|module| module.family != "AIUpdateInterface");
    child.update_modules.retain(|module| !matches!(module, crate::skirmish::module_updates::ModuleRegistration::Behavior{family,..} if family == "AIUpdateInterface"));
    let child_type = game.catalog.len();
    game.catalog.push(child);
    let mut parent = tests::fixture_type(Faction::Usa, Kind::Tank);
    parent.behavior.destroy_die.push(tests::death_conditions());
    tests::register_behavior(&mut parent, "DestroyDie", 2);
    parent.behavior.create_object_die.push(Rules {
        conditions: tests::death_conditions(),
        actions: vec![Action::Objects(ordinary("NoPhysicsChild"))],
        transfer_previous_health: false,
    });
    tests::register_behavior(&mut parent, "CreateObjectDie", 2);
    let kind = game.catalog.len();
    game.catalog.push(parent);
    let id = game.spawn(kind, 0, [355.0, 255.0], true);
    (game, id, child_type)
}

#[test]
fn generic_no_physics_preserves_upper_layer_and_skips_force_rng_after_destroy_die() {
    let (mut game, id, child_type) = fixture();
    game.entities[id].layer = 2;
    game.set_object_position(id, [355.0, 255.0, 40.0]);
    let first = game.entities.len();
    let random = game.random;
    game.kill_object(id, DeathType::Normal);
    assert_eq!(game.entities.len(), first + 1);
    assert_eq!(game.entities[first].type_index, child_type);
    assert_eq!(game.entities[first].layer, 2);
    assert_eq!(
        game.object_position(first).map(f32::to_bits),
        [355.0_f32, 255.0, 40.0].map(f32::to_bits)
    );
    assert!(game.object_physics_mass(first).is_none());
    assert_eq!(game.random, random);
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
    assert!(game.entities[first].exists());
    // No old trailing lifecycle adapter duplicates creation after the actual DIE slot.
    game.step();
    assert_eq!(
        game.entities
            .iter()
            .filter(|actor| actor.type_index == child_type)
            .count(),
        1
    );
}

#[test]
fn two_die_instances_use_actual_required_exempt_and_veterancy_filters() {
    let (mut game, old, child_type) = fixture();
    let kind = game.entities[old].type_index;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    let first = &mut game.catalog[kind].behavior.create_object_die[0];
    first.conditions.veterancy_mask = 2;
    first.conditions.required_status = 1 << 10;
    let mut conditions = tests::death_conditions();
    conditions.exempt_status = 1 << 10;
    let mut second = ordinary("NoPhysicsChild");
    second.objects[0].offset = [10.0, 0.0, 0.0];
    game.catalog[kind].behavior.create_object_die.push(Rules {
        conditions,
        actions: vec![Action::Objects(second)],
        transfer_previous_health: false,
    });
    tests::register_behavior(&mut game.catalog[kind], "CreateObjectDie", 2);
    for (hijacked, level, expected) in [
        (false, 1, Some(365.0_f32)),
        (true, 1, Some(355.0_f32)),
        (true, 0, None),
    ] {
        let id = game.spawn(kind, 0, [355.0, 255.0], true);
        game.set_hijacker_veterancy(id, level, false);
        game.entities[id].crew.hijacked = hijacked;
        let before = game.entities.len();
        game.kill_object(id, DeathType::Normal);
        assert_eq!(
            game.entities.len(),
            before + usize::from(expected.is_some())
        );
        if let Some(expected) = expected {
            assert_eq!(game.entities[before].type_index, child_type);
            assert_eq!(game.entities[before].producer, Some(id));
            assert_eq!(
                game.object_position(before)[0].to_bits(),
                expected.to_bits()
            );
        }
    }
}

#[test]
fn timed_tunnel_replacement_transfers_missing_health_subdual_and_attackers_once() {
    let mut game = tests::game();
    let mut tunnel = tests::fixture_type(Faction::Gla, Kind::Barracks);
    tunnel.name = "ExposedTunnel".into();
    tunnel.kind_of = vec!["STRUCTURE".into()];
    tunnel.maintenance.subdual = Some(crate::skirmish::microwave::SubdualRules {
        cap: 1200.0,
        heal_rate: 15,
        heal_amount: 100.0,
    });
    let tunnel_type = game.catalog.len();
    game.catalog.push(tunnel);

    let mut seed = tests::fixture_type(Faction::Gla, Kind::Barracks);
    seed.kind_of = vec!["STRUCTURE".into()];
    seed.maintenance.subdual = Some(crate::skirmish::microwave::SubdualRules {
        cap: 1200.0,
        heal_rate: 15,
        heal_amount: 100.0,
    });
    seed.behavior.destroy_die.push(tests::death_conditions());
    tests::register_behavior(&mut seed, "DestroyDie", 2);
    let mut contents = ordinary("ExposedTunnel");
    contents.objects[0].disposition = physical_ocl::LIKE_EXISTING;
    seed.behavior.create_object_die.push(Rules {
        conditions: tests::death_conditions(),
        actions: vec![Action::Objects(contents)],
        transfer_previous_health: true,
    });
    tests::register_behavior(&mut seed, "CreateObjectDie", 2);
    let seed_type = game.catalog.len();
    game.catalog.push(seed);

    let source = game.spawn(seed_type, 0, [380.0, 270.0], true);
    let attacker = game.spawn(6, 1, [380.0, 330.0], true);
    game.attack(attacker, source).unwrap();
    game.attempt_health_damage(
        source,
        Some(attacker),
        15.0,
        DamageType::Unresistable,
        DeathType::Normal,
    );
    game.apply_subdual_damage(source, 20.0);
    let expected_health = game.entities[source].health;
    let replacement = game.entities.len();
    game.kill_object(source, DeathType::Normal);

    assert_eq!(game.entities.len(), replacement + 1);
    assert_eq!(game.entities[replacement].type_index, tunnel_type);
    assert_eq!(
        game.entities[replacement].health.to_bits(),
        expected_health.to_bits()
    );
    assert_eq!(
        game.entities[replacement].subdual_damage().to_bits(),
        20.0_f32.to_bits()
    );
    assert!(
        matches!(game.entities[attacker].ai.order, crate::skirmish::Order::Attack(id) if id == replacement)
    );
}
