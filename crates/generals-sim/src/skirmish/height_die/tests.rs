use super::{Rules, State};
use crate::skirmish::{Kind, tests::game};

#[test]
fn delayed_downward_crossing_snaps_before_death_and_destroys_attached_particles_once() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].behavior.height_die = vec![Rules {
        target_height: 5.0,
        includes_structures: false,
        only_moving_down: true,
        particle_height: 2.0,
        snap_to_ground: true,
        initial_delay: 3,
    }];
    let id = game.spawn(kind, 0, [240.0, 240.0], true);
    game.entities[id].behaviors.height_die = vec![State::new()];
    game.set_object_position(id, [240.0, 240.0, 4.0]);
    assert_eq!(game.update_height_die(id, 0), 1);
    game.tick = 3;
    game.update_height_die(id, 0);
    // Initial lastPosition.z=-1: below threshold alone cannot count as descent.
    assert!(game.entities[id].alive());
    game.set_object_position(id, [240.0, 240.0, 3.0]);
    game.tick = 4;
    game.update_height_die(id, 0);
    assert!(!game.entities[id].alive());
    assert_eq!(
        game.object_position(id)[2].partial_cmp(&0.0),
        Some(std::cmp::Ordering::Equal)
    );
    assert_eq!(
        game.presentation
            .weapon_fx
            .iter()
            .filter_map(|event| match event {
                crate::skirmish::WeaponFxEvent::DestroyAttachedParticles(object) => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [id]
    );
    game.tick = 5;
    game.update_height_die(id, 0);
    assert_eq!(
        game.presentation
            .weapon_fx
            .iter()
            .filter_map(|event| match event {
                crate::skirmish::WeaponFxEvent::DestroyAttachedParticles(object) => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [id]
    );
}
