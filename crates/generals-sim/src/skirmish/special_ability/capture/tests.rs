use super::*;
use crate::skirmish::{
    special_ability::{FeedbackEffect, capture_fixture},
    tests::game,
};

fn setup() -> (Skirmish, usize, usize) {
    let mut game = game();
    game.catalog[5].ai.acquisition &= !crate::skirmish::UnitAi::IDLE;
    capture_fixture::bind(
        &mut game.catalog[5],
        PowerType::SpecialBlacklotusCaptureBuilding,
        [2, 4, 2, 0],
        false,
    );
    capture_fixture::bind(
        &mut game.catalog[5],
        PowerType::SpecialBlacklotusDisableVehicleHack,
        [2, 4, 2, 0],
        false,
    );
    capture_fixture::bind(
        &mut game.catalog[5],
        PowerType::SpecialBlacklotusStealCashHack,
        [2, 4, 2, 60],
        false,
    );
    capture_fixture::stream(&mut game);
    game.catalog[6].kind_of.push("VEHICLE".into());
    game.catalog[6].weapon.damage = 0.0;
    let actor = game.spawn(5, 0, [300.0, 300.0], true);
    let target = game.spawn(6, 1, [350.0, 300.0], true);
    game.catalog[6].kind_of.push("VEHICLE".into());
    game.update_visibility();
    (game, actor, target)
}

fn advance(game: &mut Skirmish, count: u64) {
    for _ in 0..count {
        game.step();
    }
}

#[test]
fn finite_vehicle_hack_uses_native_preparation_then_expires_without_health_damage() {
    let (mut game, actor, target) = setup();
    let health = game.entities[target].health;
    game.hack(actor, target, 1).unwrap();
    // FacePosition and unpack run in the native AI/update schedulers before
    // the preparation counter starts; four frames from command issue is not
    // the original preparation clock.
    for _ in 0..120 {
        if game.entities[actor].lotus_stream_target().is_some() {
            break;
        }
        assert!(!game.entities[target].hacked());
        assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
        game.step();
    }
    assert_eq!(game.entities[actor].lotus_stream_target(), Some(target));
    assert_eq!(game.entities[actor].capture_condition(), Some("FIRING_A"));
    for _ in 0..3 {
        game.step();
        assert!(!game.entities[target].hacked());
        assert_eq!(game.entities[actor].lotus_stream_target(), Some(target));
    }
    game.step();
    assert!(game.entities[target].hacked());
    assert_eq!(game.entities[target].health.to_bits(), health.to_bits());
    assert!(game.entities[actor].lotus_stream_target().is_none());
    assert_eq!(game.entities[actor].capture_condition(), Some("PACKING"));
    let flags = &game.entities[actor].drawable.model_conditions;
    assert!(flags.has_flag(super::super::FIRING_A));
    assert!(flags.has_flag(super::super::PACKING));
    let end = game.entities[target].hacked_until;
    game.tick = end - 1;
    game.expire_hacked();
    assert!(game.entities[target].hacked());
    game.tick += 1;
    game.expire_hacked();
    assert!(!game.entities[target].hacked());
}

#[test]
fn cash_hack_has_its_own_recharge_caps_at_available_funds_and_external_stop_cancels() {
    let (mut game, actor, target) = setup();
    game.catalog[6]
        .kind_of
        .extend(["CASH_GENERATOR".into(), "CAPTURABLE".into()]);
    assert!(!game.finite_ability_ready(actor, 2));
    assert!(game.finite_ability_ready(actor, 1));
    advance(&mut game, 60);
    let own = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    game.players[1].cash = 700;
    game.hack(actor, target, 2).unwrap();
    advance(&mut game, 5);
    game.stop(actor).unwrap();
    advance(&mut game, 70);
    assert_eq!(game.players[1].cash, 700);
    game.hack(actor, target, 2).unwrap();
    advance(&mut game, 8);
    assert_eq!(game.players[1].cash, 0);
    assert_eq!(game.players[0].cash, own + 700);
    assert_eq!(game.players[0].statistics.money_earned, earned + 700);
    assert!(game.finite_ability_ready(actor, 1));
    assert!(!game.finite_ability_ready(actor, 2));
    assert!(
        game.presentation
            .ability_feedback
            .iter()
            .any(|event| matches!(event.effect, FeedbackEffect::CashStolen(700)))
    );
}

#[test]
fn native_vehicle_hack_rejects_aircraft_and_allied_targets() {
    let (mut game, actor, target) = setup();
    assert!(game.can_hack(actor, target, 1));
    game.catalog[6].kind_of.push("AIRCRAFT".into());
    assert!(!game.can_hack(actor, target, 1));
    game.catalog[6].kind_of.pop();
    game.entities[target].owner = 0;
    assert!(!game.can_hack(actor, target, 1));
}

#[test]
fn finite_lotus_preparation_owns_a_real_stream_and_stop_retires_it_on_the_next_callback() {
    let (mut game, actor, target) = setup();
    game.hack(actor, target, 1).unwrap();
    for _ in 0..120 {
        game.step();
        if !game.entities[actor].behaviors.special_abilities[1]
            .objects
            .is_empty()
        {
            break;
        }
    }
    let objects = &game.entities[actor].behaviors.special_abilities[1].objects;
    assert_eq!(objects.len(), 1);
    let object = objects[0];
    assert_eq!(
        game.catalog[game.entities[object].type_index].name,
        "BinaryDataStream"
    );
    assert_eq!(game.entities[object].owner, game.entities[actor].owner);
    assert_eq!(game.entities[object].experience_sink, Some(actor));
    let event = game
        .presentation
        .laser_initializations
        .iter()
        .find(|event| event.object == object)
        .expect("native initLaser callback");
    assert_eq!(event.init.parent, Some(actor));
    assert_eq!(event.init.target, Some(target));
    assert_eq!(
        event.init.end.unwrap().map(f32::to_bits),
        game.target_center(target).map(f32::to_bits)
    );
    assert!(game.entities[actor].object_status & (1 << 24) != 0);
    game.stop(actor).unwrap();
    assert!(game.entities[object].exists());
    game.step();
    assert!(!game.entities[actor].behaviors.special_abilities[1].active);
    assert!(
        game.entities[actor].behaviors.special_abilities[1]
            .objects
            .is_empty()
    );
    assert!(!game.entities[object].exists());
    assert!(!game.entities[target].hacked());
}

#[test]
fn switching_finite_power_retires_the_previous_native_stream_and_preserves_its_cooldown() {
    let (mut game, actor, target) = setup();
    game.catalog[6]
        .kind_of
        .extend(["CASH_GENERATOR".into(), "CAPTURABLE".into()]);
    advance(&mut game, 60);
    game.hack(actor, target, 1).unwrap();
    for _ in 0..120 {
        game.step();
        if !game.entities[actor].behaviors.special_abilities[1]
            .objects
            .is_empty()
        {
            break;
        }
    }
    let object = game.entities[actor].behaviors.special_abilities[1].objects[0];
    let ready = game.special_power_ready_frame(actor, "LotusVehicle");
    game.hack(actor, target, 2).unwrap();
    assert!(!game.entities[actor].behaviors.special_abilities[1].active);
    assert!(
        game.entities[actor].behaviors.special_abilities[1]
            .objects
            .is_empty()
    );
    assert!(game.update_objects[game.entities[object].update_owner].destroy_requested);
    assert!(game.entities[actor].behaviors.special_abilities[2].active);
    assert_eq!(game.special_power_ready_frame(actor, "LotusVehicle"), ready);
    game.step();
    assert!(!game.entities[object].exists());
}
