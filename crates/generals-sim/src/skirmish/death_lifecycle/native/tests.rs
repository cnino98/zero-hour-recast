use crate::skirmish::{
    DeathType, Skirmish,
    death_callbacks::Conditions,
    death_lifecycle::{Rules, SlowDeath},
    module_updates::{FOREVER, ModuleRegistration},
    tests::{game, register_update},
};

fn fixture(count: usize) -> (Skirmish, usize) {
    let mut game = game();
    let unit = &mut game.catalog[5];
    let mut slow = Vec::new();
    for index in 0..count {
        register_update(unit, "SlowDeathBehavior");
        let module = unit.behavior_modules.last_mut().unwrap();
        module.interfaces |= 2;
        slow.push(SlowDeath {
            derived: crate::skirmish::death_lifecycle::Derived::None,
            tag: module.tag.clone(),
            conditions: Conditions {
                death_mask: u32::MAX,
                veterancy_mask: 15,
                exempt_status: 0,
                required_status: 0,
            },
            probability: 10,
            overkill_bonus: 0.0,
            sink_delay: [4, 0],
            destruction_delay: [10, 0],
            sink_rate: 0.5,
            fling: None,
            fx: [
                vec![format!("Initial{index}")],
                vec![format!("Midpoint{index}")],
                vec![format!("Final{index}")],
            ],
            objects: std::array::from_fn(|_| Vec::new()),
            weapons: std::array::from_fn(|_| Vec::new()),
        });
    }
    unit.death_lifecycle = Some(Rules {
        on_die: Vec::new(),
        slow,
        physics: None,
        weapons: Vec::new(),
    });
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    (game, actor)
}

fn registrations(game: &Skirmish, id: usize) -> Vec<usize> {
    game.catalog[game.entities[id].type_index].update_modules.iter().enumerate()
        .filter_map(|(index, module)| matches!(module, ModuleRegistration::Behavior { family, .. } if family == "SlowDeathBehavior").then_some(index)).collect()
}

#[test]
fn constructor_sleeps_each_instance_and_only_selected_callback_sinks_and_retires() {
    let (mut game, id) = fixture(2);
    for registration in registrations(&game, id) {
        assert_eq!(
            game.update_remaining_sleep(id, registration) + game.update_frame(),
            FOREVER
        );
    }
    game.kill_object(id, DeathType::Normal);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    let chosen = game.presentation.object_fx_events[0].effect.clone();
    let instances = &game.entities[id].death_lifecycle.as_ref().unwrap().native;
    assert_eq!(instances.iter().filter(|state| state.active()).count(), 1);
    let inactive = instances.iter().position(|state| !state.active()).unwrap();
    let module = registrations(&game, id)[inactive];
    assert_eq!(
        game.update_remaining_sleep(id, module) + game.update_frame(),
        FOREVER
    );
    game.tick = 4;
    game.update_deaths(game.entities.len());
    assert_eq!(
        game.object_position(id)[2].to_bits(),
        0.0_f32.to_bits(),
        "bulk adapter cannot consume registered callback"
    );
    game.run_object_updates();
    assert!(game.entities[id].disabled_held());
    assert_eq!(game.object_position(id)[2].to_bits(), (-0.5_f32).to_bits());
    game.run_object_updates();
    assert_eq!(game.object_position(id)[2].to_bits(), (-0.5_f32).to_bits());
    game.tick = 10;
    game.run_object_updates();
    assert!(game.entities[id].exists(), "native destruction is deferred");
    let effects: Vec<_> = game
        .presentation
        .object_fx_events
        .iter()
        .map(|event| event.effect.as_str())
        .collect();
    assert_eq!(
        effects,
        [
            chosen.as_str(),
            &chosen.replace("Initial", "Midpoint"),
            &chosen.replace("Initial", "Final")
        ]
    );
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
    assert!(
        game.update_objects[game.entities[id].update_owner]
            .handles
            .is_empty()
    );
}

#[test]
fn per_instance_die_mux_precedes_ai_dead_and_selection() {
    let (mut game, id) = fixture(2);
    game.catalog[5].death_lifecycle.as_mut().unwrap().slow[0]
        .conditions
        .required_status = 1 << 3;
    game.kill_object(id, DeathType::Normal);
    assert_eq!(game.presentation.object_fx_events.len(), 1);
    assert_eq!(game.presentation.object_fx_events[0].effect, "Initial1");
    assert!(game.entities[id].is_ai_dead());
    let random = game.random;
    game.slow_death_on_die(id, 1);
    assert_eq!(
        game.random, random,
        "AI marked dead prevents a second weighted selection"
    );
}

#[test]
fn later_lod_acceleration_multiplies_absolute_frames_and_zero_scale_defers_destroy() {
    let (mut game, id) = fixture(1);
    game.tick = 100;
    game.kill_object(id, DeathType::Normal);
    game.slow_death_scale = 0.5;
    game.tick = 104;
    game.run_object_updates();
    let state = &game.entities[id].death_lifecycle.as_ref().unwrap().native[0];
    assert_eq!(state.sink, 52);
    assert_eq!(state.destruction, 55);
    assert_eq!(game.object_position(id)[2].to_bits(), (-1.0_f32).to_bits());
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert!(game.entities[id].exists());

    let (mut game, id) = fixture(1);
    game.slow_death_scale = 0.0;
    game.kill_object(id, DeathType::Normal);
    assert!(game.presentation.object_fx_events.is_empty());
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert!(!game.entities[id].death_lifecycle.as_ref().unwrap().native[0].active());
}

#[test]
fn explicitly_woken_inactive_module_runs_native_zero_deadlines_without_activation() {
    let (mut game, id) = fixture(1);
    assert!(!game.entities[id].death_lifecycle.as_ref().unwrap().native[0].active());
    game.wake_family_update(id, "SlowDeathBehavior", 0, 1);
    game.tick += 1;
    game.run_object_updates();
    assert!(!game.entities[id].death_lifecycle.as_ref().unwrap().native[0].active());
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert_eq!(game.object_position(id)[2].to_bits(), (-0.5_f32).to_bits());
    let effects: Vec<_> = game
        .presentation
        .object_fx_events
        .iter()
        .map(|event| event.effect.as_str())
        .collect();
    assert_eq!(effects, ["Midpoint0", "Final0"]);
}
