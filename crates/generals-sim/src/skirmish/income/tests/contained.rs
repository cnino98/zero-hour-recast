use super::*;

fn center() -> (Skirmish, usize, usize) {
    let (mut game, carrier) = crate::skirmish::containment::tests::setup(10.0, 0.5);
    let index = game.entities[carrier].type_index;
    let unit = &mut game.catalog[index];
    unit.kind = Kind::TechBuilding;
    unit.kind_of = vec!["STRUCTURE".into(), "FS_INTERNET_CENTER".into()];
    unit.speed = 0.0;
    let contain = unit.contain.as_mut().unwrap();
    contain.allowed = vec!["MONEY_HACKER".into()];
    contain.capacity = 8;
    contain.exit_frames = 15;
    contain.exit_flags = 2;
    let unit = &mut game.catalog[5];
    unit.weapon.damage = 0.0;
    unit.kind_of.push("MONEY_HACKER".into());
    unit.behavior.hacking = Some(HackerRules {
        unpack: 2,
        pack: 2,
        variation: 0.0,
        delay: 4,
        contained_delay: 1,
        amounts: [5, 6, 8, 10],
        experience: 1,
    });
    bind_hacker(unit);
    unit.veterancy.trainable = true;
    unit.veterancy.required = [0, 1, 2, 3];
    let carrier = crate::skirmish::containment::tests::respawn_contain_family(
        &mut game,
        carrier,
        "InternetHackContain",
    );
    let id = game.spawn(5, 0, [355.0, 335.0], true);
    game.enter(id, carrier).unwrap();
    for _ in 0..1200 {
        game.step();
        if game.entities[id].contained_by().is_some() {
            break;
        }
    }
    assert_eq!(game.entities[id].contained_by(), Some(carrier));
    (game, carrier, id)
}

fn start_earning(game: &mut Skirmish, id: usize) {
    for _ in 0..4 {
        if game.entities[id].hacking_condition() == Some("FIRING_A") {
            return;
        }
        game.step();
    }
    panic!("Contained Hacker must finish the ordinary unpack states");
}

#[test]
fn boarding_starts_full_unpack_then_fast_counter_and_native_rank_transaction() {
    let (mut game, carrier, id) = center();
    assert!(game.entities[id].masked());
    assert_eq!(game.entities[id].hacking_condition(), Some("UNPACKING"));
    assert_eq!(game.entities[id].animation_duration().unwrap().1, 2);
    assert!(!game.passenger_can_fire(id));
    assert!(game.hack_internet(id).is_err());
    start_earning(&mut game, id);
    let mut cash = game.players[0].cash;
    for amount in [5, 6, 8, 10] {
        game.step();
        assert_eq!(game.players[0].cash, cash);
        game.step();
        cash += amount;
        assert_eq!(game.players[0].cash, cash);
    }
    assert_eq!(game.players[0].statistics.money_earned, 29);
    assert_eq!(game.entities[id].experience, 4);
    assert_eq!(game.entities[id].veterancy, 3);
    assert_eq!(game.passengers(carrier), &[id]);
}

#[test]
fn contained_pack_draws_rng_even_at_zero_then_dispatches_exit_next_update() {
    let (mut game, carrier, id) = center();
    start_earning(&mut game, id);
    game.catalog[5].behavior.hacking.as_mut().unwrap().variation = 0.5;
    let mut expected = game.random;
    startup::random_real(&mut expected, 0.5, 1.5);
    let cash = game.players[0].cash;
    game.exit_container(id).unwrap();
    assert_eq!(game.random, expected);
    assert_eq!(game.entities[id].hacking_condition(), Some("PACKING"));
    assert_eq!(game.entities[id].animation_duration().unwrap().1, 0);
    game.step();
    assert_eq!(game.entities[id].contained_by(), Some(carrier));
    assert!(matches!(game.entities[id].ai.order, Order::Idle));
    assert!(!game.entities[id].ai_idle());
    game.step();
    assert!(game.entities[id].hacking_condition().is_none());
    assert_eq!(game.entities[id].contained_by(), None);
    assert_eq!(game.players[0].cash, cash);
    assert!(!game.entities[id].masked());
}

#[test]
fn contained_healing_and_half_health_death_damage_do_not_continue_income() {
    let (mut game, carrier, id) = center();
    start_earning(&mut game, id);
    let maximum = game.entities[id].max_health;
    game.entities[id].health = maximum * 0.6;
    let before = game.entities[id].health;
    game.step();
    let regenerated = maximum * 10.0 / 100.0 * crate::skirmish::STEP_SECONDS;
    let expected = before + regenerated;
    assert_eq!(game.entities[id].health.to_bits(), expected.to_bits());
    let before = game.entities[id].health;
    let damage = game.entities[carrier].health;
    game.apply_hit(carrier, 0, damage);
    let passenger_damage = maximum * 0.5;
    assert_eq!(
        game.entities[id].health.to_bits(),
        (before - passenger_damage).to_bits()
    );
    assert_eq!(game.entities[id].contained_by(), None);
    assert!(game.entities[id].hacking_condition().is_none());
    let cash = game.players[0].cash;
    for _ in 0..10 {
        game.step();
    }
    assert_eq!(game.players[0].cash, cash);
}

#[test]
fn payment_snapshots_host_position_and_scatter_before_same_frame_detachment() {
    let (mut game, carrier, id) = center();
    start_earning(&mut game, id);
    let position = game.object_position(id);
    game.step();
    game.step();
    let frame = game.tick;
    let damage = game.entities[carrier].health;
    game.apply_hit(carrier, 0, damage);
    assert!(game.entities[id].contained_by().is_none());
    assert_ne!(
        game.object_position(id).map(f32::to_bits),
        position.map(f32::to_bits)
    );
    let event = game
        .presentation
        .hacking_events
        .iter()
        .find(|event| event.object == id && event.frame == frame)
        .expect("same-frame payment event retained");
    let Effect::Cash {
        amount,
        position: captured,
        container_radii,
        ..
    } = event.effect
    else {
        panic!("expected income transaction");
    };
    assert_eq!(amount, 5);
    assert_eq!(captured.map(f32::to_bits), position.map(f32::to_bits));
    assert_eq!(
        container_radii.unwrap().map(f32::to_bits),
        [12.0_f32; 2].map(f32::to_bits)
    );
}
