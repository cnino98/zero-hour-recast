use super::*;
use crate::skirmish::{ContinuousFire, tests::game};

#[test]
fn shots_change_rate_at_native_thresholds_and_keep_coast_across_targets() {
    let mut game = game();
    let weapon = &mut game.catalog[6].weapon;
    weapon.delay = 0.4;
    weapon.continuous_fire = Some(ContinuousFire {
        mean_shots: 2,
        fast_shots: 6,
        coast_frames: 30.0,
    });
    weapon.bonuses.template[2][3] = 2.0;
    weapon.bonuses.template[3][3] = 3.0;
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    let first = game.spawn(6, 1, [350.0, 300.0], true);
    let second = game.spawn(6, 1, [370.0, 300.0], true);
    game.tick = 10;
    for (shot, flags) in [0, 0, MEAN, MEAN, MEAN, MEAN, FAST].into_iter().enumerate() {
        let target = if shot < 3 { first } else { second };
        game.entities[id].spend_weapon_round(&game.catalog[6], game.tick, 0);
        game.notify_firing_tracker_shot(id, 0, Some(target));
        assert_eq!(game.entities[id].firing.continuous_flags, flags);
        let expected = match flags {
            MEAN => 6.0_f32,
            FAST => 4.0_f32,
            _ => 12.0_f32,
        };
        assert_eq!(
            game.entities[id].weapon_set.weapons[0].cooldown.to_bits(),
            expected.to_bits()
        );
        game.tick += 1;
    }
    assert_eq!(game.entities[id].firing.shots, 7);
    assert_eq!(game.entities[id].firing.victim, Some(second));
    let rapid = game
        .presentation
        .weapon_fx
        .iter()
        .filter(|event| matches!(event, WeaponFxEvent::RapidFire { object, .. } if *object == id))
        .count();
    assert_eq!(rapid, 1);
}

#[test]
fn strict_coast_boundary_retains_slow_for_the_following_source_callback() {
    let mut game = game();
    game.catalog[6].weapon.delay = 2.0 / 30.0;
    game.catalog[6].weapon.continuous_fire = Some(ContinuousFire {
        mean_shots: 1,
        fast_shots: 5,
        coast_frames: 60.0,
    });
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [350.0, 300.0], true);
    game.tick = 10;
    for _ in 0..6 {
        game.entities[id].spend_weapon_round(&game.catalog[6], game.tick, 0);
        game.notify_firing_tracker_shot(id, 0, Some(target));
    }
    assert_eq!(game.entities[id].firing.continuous_flags, FAST);
    game.tick = 72;
    assert_eq!(game.update_firing_tracker(id), 1);
    assert_eq!(game.entities[id].firing.continuous_flags, FAST);
    game.tick = 73;
    assert_eq!(game.update_firing_tracker(id), 30);
    assert_eq!(game.entities[id].firing.continuous_flags, 0);
    assert!(game.entities[id].firing.slow);
    game.tick = 103;
    assert_eq!(game.update_firing_tracker(id), 1);
    assert!(game.entities[id].firing.slow);
    game.tick = 104;
    assert_eq!(game.update_firing_tracker(id), 30);
    assert!(!game.entities[id].firing.slow);
    assert_eq!(game.entities[id].firing.cooldown_at, 0);
}

#[test]
fn cooldown_return_can_delay_a_nearer_reload_and_audio_stop_is_ordered_first() {
    let mut game = game();
    game.catalog[6].weapon.clip_size = 6;
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    game.entities[id].weapon_set.weapons[0].clip_remaining = 2;
    game.entities[id].firing.force_reload_at = 15;
    game.entities[id].firing.cooldown_at = 10;
    game.entities[id].firing.stop_sound_at = 11;
    game.entities[id].firing.continuous_flags = MEAN;
    game.tick = 11;
    assert_eq!(game.update_firing_tracker(id), 30);
    assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 2);
    assert!(
        matches!(game.presentation.weapon_fx.last(), Some(WeaponFxEvent::StopFiringSound(object)) if *object == id)
    );
    game.tick = 41;
    assert_eq!(game.update_firing_tracker(id), 1);
    assert_eq!(game.entities[id].weapon_set.weapons[0].clip_remaining, 6);
    assert_eq!(game.entities[id].firing.force_reload_at, 0);
}

#[test]
fn zero_reload_delay_keeps_an_earlier_deadline_and_sound_timer_clears_on_one_shot() {
    let mut game = game();
    game.catalog[6].weapon.auto_reload_idle_frames = 20;
    game.catalog[6].weapon.fire_sound_loop_frames = 3;
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [350.0, 300.0], true);
    game.tick = 10;
    game.notify_firing_tracker_shot(id, 0, Some(target));
    assert_eq!(game.entities[id].firing.force_reload_at, 30);
    assert_eq!(game.entities[id].firing.stop_sound_at, 13);
    game.catalog[6].weapon.auto_reload_idle_frames = 0;
    game.catalog[6].weapon.fire_sound_loop_frames = 0;
    game.tick = 12;
    game.notify_firing_tracker_shot(id, 0, Some(target));
    assert_eq!(game.entities[id].firing.force_reload_at, 30);
    assert_eq!(game.entities[id].firing.stop_sound_at, 0);
    assert!(
        !game
            .presentation
            .weapon_fx
            .iter()
            .any(|event| matches!(event, WeaponFxEvent::StopFiringSound(_)))
    );
    game.stop_firing_sound(id);
    assert!(
        matches!(game.presentation.weapon_fx.last(), Some(WeaponFxEvent::StopFiringSound(object)) if *object == id)
    );
}

#[test]
fn negative_native_thresholds_promote_on_successive_shots() {
    let mut game = game();
    game.catalog[6].weapon.continuous_fire = Some(ContinuousFire {
        mean_shots: -2,
        fast_shots: -1,
        coast_frames: 30.0,
    });
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [350.0, 300.0], true);
    game.notify_firing_tracker_shot(id, 0, Some(target));
    assert_eq!(game.entities[id].firing.continuous_flags, MEAN);
    game.notify_firing_tracker_shot(id, 0, Some(target));
    assert_eq!(game.entities[id].firing.continuous_flags, FAST);
}
