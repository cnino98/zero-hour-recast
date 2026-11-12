//! Persistent countdown exercised through genuine command, unpack and trigger.

#[test]
fn persistent_bomb_preparation_freezes_through_ready_frame_and_keeps_live_object_cap() {
    let (mut game, host) = crate::skirmish::helix::tests::setup();
    game.apply_unpause_special_power_upgrade(host, 0);
    game.set_special_power_ready_frame(host, "NapalmPower", game.update_frame());
    let position = game.entities[host].position;
    game.helix_bomb(host, position).unwrap();
    for _ in 0..20 {
        game.step();
        if game.entities[host].helix_bomb_object().is_some() {
            break;
        }
    }
    let existing = game.entities[host].helix_bomb_object().unwrap();
    assert_eq!(
        game.catalog[game.entities[existing].type_index].name,
        "NapalmBomb"
    );
    assert_eq!(
        game.entities[host].behaviors.special_abilities[0]
            .preparing
            .frames,
        3
    );
    let before = game.entities.len();
    let ready = game.tick + 10;
    game.set_special_power_ready_frame(host, "NapalmPower", u32::try_from(ready).unwrap());
    for frame in game.tick..=ready {
        game.tick = frame;
        game.update_special_ability(host, 0);
        assert_eq!(
            game.entities[host].behaviors.special_abilities[0]
                .preparing
                .frames,
            3
        );
    }
    for remaining in [2, 1] {
        game.tick += 1;
        game.update_special_ability(host, 0);
        assert_eq!(
            game.entities[host].behaviors.special_abilities[0]
                .preparing
                .frames,
            remaining
        );
    }
    game.tick += 1;
    game.update_special_ability(host, 0);
    assert_eq!(game.entities.len(), before);
    assert_eq!(game.entities[host].helix_bomb_object(), Some(existing));
    assert_eq!(game.helix_bomb_ready_frame(host), Some(game.tick + 300));
    game.stop(host).unwrap();
    game.tick += 1;
    game.update_special_ability(host, 0);
    assert_eq!(game.entities[host].helix_bomb_object(), Some(existing));
    assert_eq!(game.helix_bomb_ready_frame(host), Some(game.tick - 1 + 300));
    assert!(!game.entities[host].behaviors.special_abilities[0].active);
}
