use super::*;
use crate::skirmish::tests::game;

fn object(helper: Helper) -> (Skirmish, usize) {
    let mut game = game();
    game.catalog[6].update_modules = vec![ModuleRegistration::Helper(helper)];
    let id = game.spawn(6, 0, [300.0, 300.0], true);
    (game, id)
}

#[test]
fn painted_zero_duration_is_set_then_cleared_by_due_callback_and_reapply_renews() {
    let (mut game, id) = object(Helper::StatusDamage);
    game.do_painted_status_damage(id, 0.0);
    assert!(game.entities[id].painted());
    assert_eq!(game.entities[id].helpers.status_damage.until, 0);
    game.tick = 1;
    game.run_object_updates();
    assert!(!game.entities[id].painted());
    game.do_painted_status_damage(id, 4.9);
    assert_eq!(game.entities[id].helpers.status_damage.until, 5);
    game.tick = 4;
    game.run_object_updates();
    assert!(game.entities[id].painted());
    game.do_painted_status_damage(id, 3.1);
    assert_eq!(game.entities[id].helpers.status_damage.until, 7);
    game.tick = 5;
    game.run_object_updates();
    assert!(game.entities[id].painted());
    game.tick = 7;
    game.run_object_updates();
    assert!(!game.entities[id].painted());
}

#[test]
fn status_deadline_preserves_native_signed_duration_and_unsigned_frame_wrap() {
    let (mut game, id) = object(Helper::StatusDamage);
    game.tick = u64::from(u32::MAX) - 1;
    game.do_painted_status_damage(id, 4.75);
    assert_eq!(game.entities[id].helpers.status_damage.until, 2);
    assert!(game.entities[id].painted());
    game.tick = 0;
    game.do_painted_status_damage(id, -0.1);
    assert_eq!(game.entities[id].helpers.status_damage.until, u32::MAX);
}

#[test]
fn subdual_unsigned_zero_countdown_wraps_and_positive_reapply_resets_real_healing() {
    let (mut game, id) = object(Helper::SubdualDamage);
    game.catalog[6].maintenance.subdual = Some(crate::skirmish::microwave::SubdualRules {
        cap: 200.0,
        heal_rate: 0,
        heal_amount: 50.0,
    });
    game.apply_subdual_damage(id, 100.0);
    assert!(game.entities[id].gaining_subdual_damage());
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(
        game.entities[id].maintenance.microwave.heal_countdown,
        u32::MAX
    );
    assert_eq!(
        game.entities[id].subdual_damage().to_bits(),
        100.0_f32.to_bits()
    );
    game.catalog[6]
        .maintenance
        .subdual
        .as_mut()
        .unwrap()
        .heal_rate = 2;
    game.apply_subdual_damage(id, 1.0);
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(
        game.entities[id].subdual_damage().to_bits(),
        101.0_f32.to_bits()
    );
    game.tick = 3;
    game.run_object_updates();
    assert_eq!(
        game.entities[id].subdual_damage().to_bits(),
        51.0_f32.to_bits()
    );
    game.apply_subdual_damage(id, -1.0);
    assert!(!game.entities[id].gaining_subdual_damage());
    assert_eq!(game.entities[id].maintenance.microwave.heal_countdown, 2);
    game.tick = 4;
    game.run_object_updates();
    game.tick = 5;
    game.run_object_updates();
    assert_eq!(
        game.entities[id].subdual_damage().to_bits(),
        0.0_f32.to_bits()
    );
}

#[test]
fn capture_expiry_precedes_death_and_emits_real_white_flash_ding_event() {
    let (mut game, id) = object(Helper::Defection);
    game.entities[id].helpers.defection.undetected = true;
    game.start_defection_timer(id, 1, true);
    game.entities[id].set_effectively_dead(true);
    game.tick = 1;
    game.run_object_updates();
    assert!(!game.entities[id].helpers.defection.undetected);
    assert!(
        game.presentation
            .capture_events
            .iter()
            .any(|event| event.object == id
                && event.target == id
                && matches!(event.effect, crate::skirmish::CaptureEffect::Detected))
    );
}

#[test]
fn defection_fx_skips_missing_drawable_and_ticks_only_on_odd_to_even_phase() {
    let (mut game, id) = object(Helper::Defection);
    game.entities[id].helpers.defection.undetected = true;
    game.start_defection_timer(id, 10, true);
    game.entities[id].helpers.defection.flash_phase = 1.75;
    game.entities[id].drawable.present = false;
    assert_eq!(game.update_builtin_helper(id, Helper::Defection), 1);
    assert_eq!(
        game.entities[id].helpers.defection.flash_phase.to_bits(),
        1.75_f32.to_bits()
    );
    assert!(game.presentation.capture_events.is_empty());
    game.entities[id].drawable.present = true;
    assert_eq!(game.update_builtin_helper(id, Helper::Defection), 1);
    assert!(
        game.presentation
            .capture_events
            .iter()
            .any(|event| matches!(event.effect, crate::skirmish::CaptureEffect::DefectorTick))
    );
    game.presentation.capture_events.clear();
    game.entities[id].helpers.defection.undetected = false;
    assert_eq!(
        game.update_builtin_helper(id, Helper::Defection),
        0x3fff_ffff
    );
    assert!(game.presentation.capture_events.is_empty());
}

#[test]
fn passive_callbacks_clear_actual_model_status_bonus_and_frenzy_without_alive_gate() {
    let (mut game, id) = object(Helper::TemporaryWeaponBonus);
    game.tick = 88;
    game.entities[id].set_effectively_dead(true);
    game.entities[id].helpers.special.cheering = true;
    game.entities[id].helpers.special.until = 77;
    game.entities[id].object_status |= 1 << 9;
    game.entities[id].helpers.temporary.bonus = Some(23);
    game.entities[id].helpers.temporary.until = 88;
    game.entities[id].helpers.temporary.flags = 1 << 23;
    game.entities[id].helpers.temporary.frenzy_tint = true;
    game.catalog[6].weapon.bonuses.global[23][0] = 2.0;
    assert_eq!(
        game.entities[id]
            .weapon_damage_bonus(&game.catalog[6].weapon, 0)
            .to_bits(),
        2.0_f32.to_bits()
    );
    assert_eq!(
        game.update_builtin_helper(id, Helper::StateMachine),
        0x3fff_ffff
    );
    assert!(
        !game
            .model_flags(&game.entities[id], [0.5, 0.1])
            .contains("SPECIAL_CHEERING")
    );
    assert_eq!(
        game.update_builtin_helper(id, Helper::Repulsor),
        0x3fff_ffff
    );
    assert_eq!(game.entities[id].object_status & (1 << 9), 0);
    assert_eq!(
        game.update_builtin_helper(id, Helper::TemporaryWeaponBonus),
        0x3fff_ffff
    );
    assert_eq!(
        game.entities[id]
            .weapon_damage_bonus(&game.catalog[6].weapon, 0)
            .to_bits(),
        1.0_f32.to_bits()
    );
    assert!(game.entities[id].helpers.temporary.bonus.is_none());
    assert_eq!(game.entities[id].helpers.temporary.until, 0);
    assert!(!game.entities[id].frenzy_tint());
}

#[test]
fn zero_damage_repulses_for_two_seconds_and_repeated_damage_does_not_extend_status() {
    let (mut game, id) = object(Helper::Repulsor);
    game.ai_rules.repulsors_enabled = true;
    game.catalog[game.entities[id].type_index]
        .kind_of
        .push("CAN_BE_REPULSED".into());
    let health = game.entities[id].health;
    game.tick = 10;
    game.apply_damage(
        id,
        None,
        0.0,
        crate::skirmish::DamageType::SmallArms,
        crate::skirmish::DeathType::Normal,
    );
    assert_eq!(game.entities[id].health.to_bits(), health.to_bits());
    assert_ne!(game.entities[id].object_status & (1 << 9), 0);
    game.tick = 30;
    game.apply_damage(
        id,
        None,
        0.0,
        crate::skirmish::DamageType::SmallArms,
        crate::skirmish::DeathType::Normal,
    );
    game.tick = 69;
    game.run_object_updates();
    assert_ne!(game.entities[id].object_status & (1 << 9), 0);
    game.tick = 70;
    game.run_object_updates();
    assert_eq!(game.entities[id].object_status & (1 << 9), 0);
}
