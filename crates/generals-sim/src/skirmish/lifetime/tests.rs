use crate::skirmish::{
    Kind,
    tests::{game, register_update},
};

#[test]
fn ordered_lifetimes_and_ocl_override_wake_the_existing_module() {
    let mut game = game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[kind].behavior.lifetimes = vec![[30; 2], [60; 2]];
    register_update(&mut game.catalog[kind], "LifetimeUpdate");
    register_update(&mut game.catalog[kind], "LifetimeUpdate");
    // Compare the same tank birth without LifetimeUpdate. Fixed endpoints consume no draw.
    let mut control = crate::skirmish::tests::game();
    control.spawn(kind, 0, [240.0, 240.0], true);
    let id = game.spawn(kind, 0, [240.0, 240.0], true);
    assert_eq!(game.random, control.random);
    assert_eq!(game.entities[id].behaviors.lifetime_die_frames, [30, 60]);
    game.set_lifetime_range(id, 0, [5; 2]);
    assert_eq!(game.entities[id].behaviors.lifetime_die_frames, [5, 60]);
    game.tick = 4;
    game.run_object_updates();
    assert!(game.entities[id].alive());
    game.tick = 5;
    game.run_object_updates();
    assert!(!game.entities[id].alive());
    assert!(game.entities[id].is_effectively_dead());
}
