use super::*;
use crate::skirmish::{Faction, Kind, tests};

fn register(game: &mut Skirmish, type_index: usize) {
    tests::register_update(&mut game.catalog[type_index], "FlammableUpdate");
}

fn rules() -> Rules {
    Rules {
        burned_delay: 0,
        duration: 150,
        damage_delay: 15,
        damage: 3.0,
        limit: 20.0,
        expiration: 60,
        sound: String::new(),
    }
}

#[test]
fn burn_threshold_accumulates_actual_damage_and_expires_strictly_after_delay() {
    let rules = rules();
    let mut state = State::new(&rules);
    assert!(!state.expose(&rules, Some(1), 10.0, 100, true));
    assert!(state.expose(&rules, Some(2), 10.0, 160, true));
    assert_eq!(state.source, Some(2));
    let mut expired = State::new(&rules);
    assert!(!expired.expose(&rules, Some(1), 10.0, 100, true));
    assert!(!expired.expose(&rules, Some(2), 10.0, 161, true));
}

#[test]
fn zero_duration_preserves_native_forever_sleep_instead_of_immediate_extinguishing() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
    let mut rules = rules();
    rules.duration = 0;
    unit.behavior.flammable = vec![rules];
    let index = game.catalog.len();
    game.catalog.push(unit);
    register(&mut game, index);
    let id = game.spawn(index, 1, [300.0, 300.0], true);
    game.tick = 100;
    game.flame_exposed_module(id, 0, Some(0), 20.0);
    assert!(game.entities[id].aflame());
    assert_eq!(game.entities[id].flammable.instances[0].sleep(100), FOREVER);
    assert_eq!(game.take_burning_sound_events().len(), 1);
    game.tick = 101;
    game.run_object_updates();
    assert!(game.entities[id].aflame());
}

#[test]
fn damage_at_burn_end_precedes_extinguishing_and_retains_real_source_credit() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
    unit.behavior.flammable = vec![rules()];
    unit.armor[crate::skirmish::DamageType::Flame as usize] = 0.5;
    let index = game.catalog.len();
    game.catalog.push(unit);
    register(&mut game, index);
    let id = game.spawn(index, 1, [300.0, 300.0], true);
    game.tick = 100;
    game.flame_exposed_module(id, 0, Some(0), 20.0);
    let before = game.entities[id].health;
    for frame in 101..=250 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert_eq!(
        game.entities[id].health.to_bits(),
        (before - 15.0).to_bits()
    );
    assert!(!game.entities[id].aflame());
    assert_eq!(game.entities[id].damage_state.last_attacker, Some(0));
    game.flame_exposed_module(id, 0, Some(0), 1.0);
    assert!(game.entities[id].aflame());
}

#[test]
fn burned_status_prevents_reignition_but_does_not_end_the_independent_flame_timer() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
    let mut rules = rules();
    rules.burned_delay = 30;
    unit.behavior.flammable = vec![rules];
    let index = game.catalog.len();
    game.catalog.push(unit);
    register(&mut game, index);
    let id = game.spawn(index, 1, [300.0, 300.0], true);
    game.tick = 100;
    game.flame_exposed_module(id, 0, Some(0), 20.0);
    for frame in 101..=130 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert!(game.entities[id].aflame());
    assert!(game.entities[id].smoldering());
    for frame in 131..=250 {
        game.tick = frame;
        game.run_object_updates();
    }
    assert!(!game.entities[id].aflame());
    game.flame_exposed_module(id, 0, Some(0), 100.0);
    assert!(!game.entities[id].aflame());
}

#[test]
fn recurring_flame_damage_intercepts_bus_first_life_before_armor() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::Gla, Kind::Vehicle);
    let mut flame = rules();
    flame.damage = 20.0;
    unit.behavior.flammable = vec![flame];
    unit.battle_bus = Some(crate::skirmish::battle_bus::Rules {
        second_health: 650.0,
        first_armor: [[1.0; crate::skirmish::DAMAGE_TYPE_COUNT]; 2],
        second_armor: [[1.0; crate::skirmish::DAMAGE_TYPE_COUNT]; 3],
    });
    crate::skirmish::battle_bus::tests::bind_death(&mut unit);
    unit.armor[crate::skirmish::DamageType::Flame as usize] = 0.5;
    unit.initial_health = 5.0;
    unit.max_health = 400.0;
    let index = game.catalog.len();
    game.catalog.push(unit);
    register(&mut game, index);
    let id = game.spawn(index, 1, [300.0, 300.0], true);
    game.tick = 100;
    game.flame_exposed_module(id, 0, Some(0), 20.0);
    game.tick = 115;
    game.run_object_updates();
    assert!(game.entities[id].alive());
    assert!(game.entities[id].battle_bus_second_life());
    assert_eq!(game.entities[id].max_health.to_bits(), 650.0_f32.to_bits());
    assert_eq!(game.entities[id].health.to_bits(), 650.0_f32.to_bits());
    assert_eq!(game.entities[id].damage_state.last_attacker, Some(0));
}

#[test]
fn authored_instances_observe_shared_aflame_gate_and_final_destructor_stops_sound() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
    unit.behavior.flammable = vec![rules(), rules()];
    let index = game.catalog.len();
    game.catalog.push(unit);
    register(&mut game, index);
    register(&mut game, index);
    let id = game.spawn(index, 1, [300.0, 300.0], true);
    game.tick = 100;
    game.flame_exposed_module(id, 0, Some(0), 20.0);
    game.flame_exposed_module(id, 1, Some(0), 20.0);
    assert!(matches!(
        game.entities[id].flammable.instances[0].phase,
        Phase::Aflame { .. }
    ));
    assert!(matches!(
        game.entities[id].flammable.instances[1].phase,
        Phase::Normal
    ));
    assert_eq!(game.take_burning_sound_events().len(), 1);
    game.destroy_flammable_module(id, 0);
    assert!(
        matches!(game.take_burning_sound_events().as_slice(), [SoundEvent::Stop { object, ordinal: 0 }] if *object == id)
    );
}

#[test]
fn fixed_spread_delay_does_not_advance_game_logic_random_state() {
    let mut game = tests::game();
    let mut unit = tests::fixture_type(Faction::China, Kind::Vehicle);
    unit.behavior.flammable = vec![rules()];
    unit.behavior.fire_spread = vec![SpreadRules {
        delay: [15, 15],
        range: 0.0,
        embers: None,
    }];
    let index = game.catalog.len();
    game.catalog.push(unit);
    register(&mut game, index);
    tests::register_update(&mut game.catalog[index], "FireSpreadUpdate");
    let id = game.spawn(index, 1, [300.0, 300.0], true);
    let random = game.random;
    game.tick = 100;
    game.flame_exposed_module(id, 0, None, 20.0);
    assert_eq!(game.random, random);
    game.tick = 115;
    assert_eq!(game.update_fire_spread_module(id, 0), 15);
    assert_eq!(game.random, random);
}
