use super::Rules;
use crate::skirmish::{DeathType, WeaponRules, tests::game};

fn rules(effect: &str) -> Rules {
    Rules {
        death_mask: u32::MAX,
        veterancy_mask: 15,
        exempt_status: 0,
        required_status: 0,
        effects: vec![Some(effect.into())],
        objects: Vec::new(),
        weapons: Vec::new(),
    }
}

#[test]
fn repeated_modules_keep_effects_after_first_marks_ai_dead_without_extra_rng() {
    let mut game = game();
    let kind = 5;
    game.catalog[kind].behavior.instant_death = vec![rules("First"), rules("Second")];
    for _ in 0..game.catalog[kind].behavior.instant_death.len() {
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[kind],
            "InstantDeathBehavior",
            0x0002,
        );
    }
    let id = game.spawn(kind, 0, [300.0, 300.0], true);
    let seed = game.random;
    game.kill_object(id, DeathType::Normal);
    assert!(game.entities[id].is_ai_dead());
    assert!(game.entities[id].exists());
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert_eq!(game.random, seed);
    assert_eq!(
        game.presentation
            .object_fx_events
            .iter()
            .map(|event| event.effect.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second"]
    );
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
}

#[test]
fn construction_omits_weapon_and_its_choice_but_keeps_fx_and_ocl_choices() {
    let mut game = game();
    let kind = 5;
    let mut death = rules("Scaffold");
    death.effects.push(None);
    death.objects = vec![None, None];
    death.weapons = vec![Some(WeaponRules::unarmed()), Some(WeaponRules::unarmed())];
    death.required_status = 1 << 3;
    game.catalog[kind].behavior.instant_death = vec![death];
    for _ in 0..game.catalog[kind].behavior.instant_death.len() {
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[kind],
            "InstantDeathBehavior",
            0x0002,
        );
    }
    let id = game.spawn(kind, 0, [300.0, 300.0], false);
    let mut expected = game.random;
    crate::skirmish::startup::random_between(&mut expected, 1);
    crate::skirmish::startup::random_between(&mut expected, 1);
    game.kill_object(id, DeathType::Normal);
    assert_eq!(game.random, expected);
    assert!(game.presentation.shots.is_empty());
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
}

#[test]
fn death_veterancy_and_status_guards_precede_any_choices() {
    let mut game = game();
    let kind = 5;
    let mut death = rules("Allowed");
    death.death_mask = 1 << DeathType::Lasered as u32;
    death.veterancy_mask = 1 << 2;
    death.exempt_status = 1 << 3;
    game.catalog[kind].behavior.instant_death = vec![death];
    for _ in 0..game.catalog[kind].behavior.instant_death.len() {
        crate::skirmish::tests::register_behavior(
            &mut game.catalog[kind],
            "InstantDeathBehavior",
            0x0002,
        );
    }
    let id = game.spawn(kind, 0, [300.0, 300.0], false);
    let seed = game.random;
    game.instant_die(id, 0);
    assert!(game.presentation.object_fx_events.is_empty());
    game.entities[id].damage_state.death_type = DeathType::Lasered;
    game.instant_die(id, 0);
    assert!(game.presentation.object_fx_events.is_empty());
    game.entities[id].veterancy = 2;
    game.instant_die(id, 0);
    assert!(game.presentation.object_fx_events.is_empty());
    game.entities[id].build_progress = 1.0;
    game.instant_die(id, 0);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    assert_eq!(game.random, seed);
}
