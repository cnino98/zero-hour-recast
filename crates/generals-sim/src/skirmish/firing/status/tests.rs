use crate::skirmish::{firing::PreAttack, tests::game};

#[test]
fn final_helper_retains_drawable_preattack_until_its_disabled_gate_allows_the_callback() {
    let mut game = game();
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    game.entities[id].weapon_set.status.attacking = true;
    game.entities[id].weapon_set.weapons[0].pre_attack = Some(PreAttack {
        start: 10,
        frames: 8,
    });
    game.tick = 10;
    game.run_object_updates();
    assert_eq!(game.entities[id].weapon_condition(), Some("PREATTACK_A"));
    assert_eq!(
        game.entities[id].pre_attack_animation(),
        Some((10, 8, false))
    );
    game.tick = 12;
    game.run_object_updates();
    assert_eq!(
        game.entities[id].pre_attack_animation(),
        Some((10, 8, false))
    );
    game.set_hacked_until(id, crate::skirmish::ai_dispatch::native_frame(100));
    game.tick = 20;
    game.run_object_updates();
    assert_eq!(game.entities[id].weapon_condition(), Some("PREATTACK_A"));
    game.clear_hacked_disable(id);
    game.tick = 21;
    game.run_object_updates();
    assert_eq!(game.entities[id].weapon_condition(), None);
    assert_eq!(game.entities[id].pre_attack_animation(), None);
}

#[test]
fn direct_fire_waits_for_final_helper_and_last_shot_wins_over_orders() {
    let mut game = game();
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(5, 1, [330.0, 300.0], true);
    game.tick = 10;
    game.fire_weapon(id, target);
    assert_eq!(game.entities[id].weapon_set.weapons[0].last_shot, Some(10));
    assert_eq!(game.entities[id].weapon_condition(), None);
    assert!(!game.entities[id].weapon_set.status.condition_initialized[0]);
    game.entities[id].weapon_set.status.attacking = false;
    game.update_weapon_status(id);
    assert_eq!(game.entities[id].weapon_condition(), Some("FIRING_A"));
    assert_eq!(
        game.entities[id].weapon_set.status.last_condition[0],
        Some("FIRING_A")
    );
    game.tick = 11;
    game.update_weapon_status(id);
    assert_eq!(game.entities[id].weapon_condition(), None);
}

#[test]
fn ready_aimed_weapon_keeps_between_pose_but_empty_clip_does_not() {
    let mut game = game();
    game.catalog[6].weapon.clip_size = 3;
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    game.entities[id].weapon_set.status.attacking = true;
    game.entities[id].weapon_set.status.aiming = true;
    game.tick = 20;
    game.update_weapon_status(id);
    assert_eq!(
        game.entities[id].weapon_condition(),
        Some("BETWEEN_FIRING_SHOTS_A")
    );
    game.entities[id].weapon_set.weapons[0].clip_remaining = 0;
    game.update_weapon_status(id);
    assert_eq!(game.entities[id].weapon_condition(), None);
    assert_eq!(game.entities[id].weapon_set.status.clip_status[0], (0, 3));
}
