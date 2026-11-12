//! Real ability init callbacks followed by the original stream's deferred deletion.
use super::*;
use bevy::ecs::system::SystemState;
use generals_sim::skirmish::{WeaponFxEvent, special_ability::PowerType};

#[ignore = "requires original game installation"]
#[test]
fn laser_retirement_replays_actual_reinitialization_before_deferred_drawable_destruction() {
    let (assets, mut battle, mut animations) =
        crate::cleanup_verification::tests::installed_with_animations();
    let object = prepare_timed_stream(&mut battle.simulation);
    let mut particles = crate::particles::load(&assets, &battle.simulation.catalog).unwrap();
    let mut world = World::new();
    let mut query = SystemState::<Query<(&UnitVisual, &Players, &Transform)>>::new(&mut world);
    let models = query.get(&world).unwrap();
    let mut native = Lasers::new();
    let mut handles = None;
    let mut calls = 0;
    let mut retired = false;
    for _ in 0..16 {
        let mut initialized_this_tick = false;
        for event in &battle.simulation.presentation.weapon_fx {
            match event {
                WeaponFxEvent::LaserInit(index) => {
                    let init = &battle.simulation.presentation.laser_initializations[*index];
                    if init.object != object {
                        continue;
                    }
                    native.initialize(&battle, init, &mut animations, &mut particles, &models);
                    let state = &native.active[&object];
                    assert!(state.emitters.iter().all(Option::is_some));
                    assert_eq!(
                        state.points[1].map(f32::to_bits),
                        init.init.end.unwrap().map(f32::to_bits)
                    );
                    if let Some(handles) = handles {
                        assert_eq!(
                            state.emitters, handles,
                            "reinit must reuse original emitters"
                        );
                    } else {
                        handles = Some(state.emitters);
                    }
                    initialized_this_tick = true;
                    calls += 1;
                }
                WeaponFxEvent::DrawableDestroyed(id) if *id == object => {
                    assert!(
                        initialized_this_tick,
                        "reinit and destructor must share this tick"
                    );
                    assert!(!battle.simulation.entities[object].exists());
                    assert!(!battle.simulation.entities[object].has_drawable());
                    assert!(
                        native.active.contains_key(&object),
                        "earlier init remains live until teardown"
                    );
                    native.retire(*id, &mut particles);
                    assert!(!native.active.contains_key(&object));
                    retired = true;
                }
                _ => {}
            }
        }
        native.update(&battle, &mut particles, &models);
        if retired {
            break;
        }
        battle.simulation.step();
    }
    assert!(
        calls > 1,
        "actual preparation must reinitialize the same stream"
    );
    assert!(
        retired,
        "original DeletionUpdate must reach ordered teardown"
    );
    assert!(!native.active.contains_key(&object));
}

fn prepare_timed_stream(game: &mut generals_sim::skirmish::Skirmish) -> usize {
    let setup = crate::infiltrator_verification::prepare(game).unwrap();
    // Explicit custom-map SpecialObject override: the installed Microwave stream
    // already supplies both endpoint emitters and its original 126ms DeletionUpdate.
    let kind = game.entities[setup.actor].type_index;
    game.catalog[kind]
        .behavior
        .special_abilities
        .iter_mut()
        .find(|rules| {
            rules
                .power
                .as_ref()
                .is_some_and(|power| power.kind == PowerType::SpecialBlacklotusDisableVehicleHack)
        })
        .unwrap()
        .objects
        .name = "MicrowaveDisableStream".into();
    game.hack(setup.actor, setup.vehicle, 1).unwrap();
    crate::skirmish_verification::advance_until(game, 18_000, "real Lotus laser init", |game| {
        game.presentation
            .laser_initializations
            .iter()
            .any(|event| event.init.parent == Some(setup.actor))
    })
    .unwrap();
    let object = game
        .presentation
        .laser_initializations
        .iter()
        .find(|event| event.init.parent == Some(setup.actor))
        .unwrap()
        .object;
    assert_eq!(
        game.catalog[game.entities[object].type_index].name,
        "MicrowaveDisableStream"
    );
    object
}
