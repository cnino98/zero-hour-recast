use super::*;
use crate::skirmish::{DeathType, tests::game};

fn fixture() -> (Skirmish, usize, usize, usize) {
    let mut game = game();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    game.slots[0].difficulty = None;
    game.set_retaliation(0, true);
    game.catalog[6].locomotor.ground = Some(crate::skirmish::GroundRules {
        wander: None,
        is_dozer: false,
        downhill_only: false,
        appearance: crate::skirmish::GroundAppearance::Treads,
        acceleration: 100.0 / 900.0,
        acceleration_damaged: 100.0 / 900.0,
        braking: 100.0 / 900.0,
        min_speed: 0.0,
        close_enough: 1.0,
        mass: 50.0,
        forward_friction: 0.1,
        lateral_friction: 0.2,
        extra_friction: 0.0,
    });
    let victim = game.spawn(5, 0, [400.0, 400.0], true);
    let ally = game.spawn(6, 0, [430.0, 420.0], true);
    let attacker = game.spawn(6, 1, [520.0, 400.0], true);
    game.update_ground_contacts();
    game.update_visibility();
    (game, victim, ally, attacker)
}

#[test]
fn real_damage_recruits_idle_ally_even_when_victim_dies_and_option_off_blocks_recruitment() {
    let (mut game, victim, ally, attacker) = fixture();
    game.apply_native_damage(victim, Some(attacker), 10000.0, DeathType::Normal);
    assert!(!game.entities[victim].alive());
    assert!(game.entities[ally].ai.guard.retaliating);
    assert!(matches!(
        game.entities[ally].ai.order,
        Order::Guard([430.0, 420.0])
    ));
    assert_eq!(game.entities[ally].ai.ai_target, Some(attacker));
    assert!(game.entities[ally].ai.command.from_ai);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Aggressor
    ));
    let (mut game, victim, ally, attacker) = fixture();
    game.set_retaliation(0, false);
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    assert!(!game.entities[ally].ai.guard.retaliating);
    assert!(matches!(game.entities[ally].ai.order, Order::Idle));
}

#[test]
fn human_damage_admission_rejects_airborne_far_friendly_and_drone_aggressions() {
    for case in 0..4 {
        let (mut game, victim, ally, attacker) = fixture();
        match case {
            0 => {
                // Installed ChinookLocomotor uses AirborneTargetingHeight=30.
                // Use its airborne height classification with the actual flight
                // pose consumer, rather than the fixture ground threshold 2^31.
                let type_index = game.entities[attacker].type_index;
                game.catalog[type_index].locomotor.airborne_targeting_height = 30.0;
                let height = game.ground_height(game.entities[attacker].position) + 31.0;
                game.entities[attacker].flight =
                    Some(Box::new(crate::skirmish::flight::Motion::new(height)));
                assert!(game.height_above_ground(attacker) > 30.0);
            }
            1 => game.entities[attacker].position = [900.0, 400.0],
            2 => game.entities[attacker].owner = 0,
            _ => game.catalog[5].kind_of.push("DRONE".into()),
        }
        game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
        assert!(
            !game.entities[ally].ai.guard.retaliating,
            "admission case {case}"
        );
    }
}

#[test]
fn busy_immobile_and_cannot_retaliate_helpers_keep_their_orders() {
    for case in 0..3 {
        let (mut game, victim, ally, attacker) = fixture();
        match case {
            0 => game.move_to(ally, [430.0, 520.0]).unwrap(),
            1 => game.catalog[6].kind_of.push("IMMOBILE".into()),
            _ => game.catalog[6].kind_of.push("CANNOT_RETALIATE".into()),
        }
        game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
        assert!(
            !game.entities[ally].ai.guard.retaliating,
            "helper case {case}"
        );
    }
}

#[test]
fn retaliation_pursuit_uses_distinct_radius_returns_without_changing_center_and_user_move_interrupts()
 {
    let (mut game, victim, ally, attacker) = fixture();
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    let center = game.entities[ally].position;
    // Standard inner=150: retaliation aggressor radius=150+150, but the
    // responding unit never leaves standard inner range around its home.
    game.entities[attacker].position = [center[0] + 220.0, center[1]];
    game.update_guard(ally, center);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Aggressor
    ));
    game.entities[attacker].position = [center[0] + 301.0, center[1]];
    game.update_guard(ally, center);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Return
    ));
    assert!(game.entities[ally].ai.guard.retaliating);
    assert!(
        matches!(game.entities[ally].ai.order, Order::Guard(point) if point.map(f32::to_bits) == center.map(f32::to_bits))
    );
    game.entities[ally].ai.guard.next_scan = u64::MAX;
    for _ in 0..60 {
        // The return now owns a real queued path; service and movement share the native frame.
        game.step();
        if matches!(game.entities[ally].ai.guard.phase, GuardPhase::Idle) {
            break;
        }
    }
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Idle
    ));
    game.entities[ally].ai.guard.next_scan = game.tick;
    game.update_guard(ally, center);
    assert!(matches!(game.entities[ally].ai.order, Order::Idle));
    assert!(!game.entities[ally].ai.guard.retaliating);
    game.entities[attacker].position = [520.0, 400.0];
    game.update_ground_contacts();
    game.update_visibility();
    game.retaliate_after_damage(victim, Some(attacker));
    assert!(game.entities[ally].ai.guard.retaliating);
    game.move_to(ally, [430.0, 520.0]).unwrap();
    assert!(!game.entities[ally].ai.guard.retaliating);
    assert!(matches!(game.entities[ally].ai.order, Order::Move(_)));
}

fn conceal(game: &mut Skirmish, actor: usize) {
    use crate::skirmish::stealth::{Rules, State, Wake};
    let unit = &mut game.catalog[game.entities[actor].type_index];
    unit.behavior.stealth = Some(Rules {
        module_order: 0,
        delay: 0,
        forbidden: 0,
        threshold_speed: 0.1,
        required_status: Vec::new(),
        forbidden_status: Vec::new(),
        hint_status: Vec::new(),
        innate: true,
        use_rider: false,
        idle_enemies_on_reveal: false,
        wake: Wake::Automatic,
        upgrades: Vec::new(),
        reveal_distance: 0.0,
        black_market_frames: 0,
        friendly_opacity: [0.5, 1.0],
        pulse_frames: 30,
        disguise: None,
    });
    game.entities[actor].stealth = State::for_unit(unit, game.tick);
    game.grant_stealth(actor);
    assert!(game.entities[actor].stealthed());
    assert!(!game.entities[actor].detected());
}

#[test]
fn hidden_helper_does_not_reveal_itself_and_hidden_aggressor_cannot_be_acquired() {
    for concealed_helper in [true, false] {
        let (mut game, victim, ally, attacker) = fixture();
        conceal(&mut game, if concealed_helper { ally } else { attacker });
        game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
        assert!(!game.entities[ally].ai.guard.retaliating);
        assert!(matches!(game.entities[ally].ai.order, Order::Idle));
        if concealed_helper {
            assert!(game.entities[ally].stealthed());
            game.mark_detected(ally, 30);
            game.update_stealth_module(ally);
            assert!(game.entities[ally].detected());
            game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
            assert!(game.entities[ally].ai.guard.retaliating);
        }
    }
}

#[test]
fn an_allied_player_helpers_revenge_human_damage_but_cpu_victim_does_not_recruit() {
    use crate::skirmish::{Faction, OpponentDifficulty, PlayerStart, setup::PlayerSlot};
    let mut game = game();
    game.layout.starts = [[140.0, 140.0], [660.0, 140.0], [660.0, 460.0]]
        .into_iter()
        .map(|position| PlayerStart {
            position,
            rally: None,
        })
        .collect();
    game.reset_slots(
        (0..3)
            .map(|owner| PlayerSlot {
                faction: Some(Faction::Gla),
                occupied: true,
                difficulty: None,
                team: (owner < 2).then_some(0),
                start: Some(owner),
                color: Some(owner),
            })
            .collect(),
    )
    .unwrap();
    for unit in &mut game.catalog {
        unit.ai.acquisition = 0;
    }
    game.set_retaliation(0, true);
    let victim = game.spawn(5, 0, [400.0, 400.0], true);
    let ally = game.spawn(6, 1, [430.0, 420.0], true);
    let attacker = game.spawn(6, 2, [520.0, 400.0], true);
    game.update_ground_contacts();
    game.update_visibility();
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    assert!(game.entities[ally].ai.guard.retaliating);
    assert_eq!(game.entities[ally].ai.ai_target, Some(attacker));
    game.stop(ally).unwrap();
    game.slots[0].difficulty = Some(OpponentDifficulty::Hard);
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    assert!(!game.entities[ally].ai.guard.retaliating);
}

#[test]
fn return_phase_ignores_new_damage_aggressor_with_crc_compatibility_disabled() {
    let (mut game, victim, ally, attacker) = fixture();
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    game.entities[attacker].position = [900.0, 400.0];
    let center = game.entities[ally].position;
    game.update_guard(ally, center);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Return
    ));
    game.entities[ally].position = [center[0] + 80.0, center[1]];
    game.entities[ally].ai.guard.next_scan = u64::MAX;
    let second = game.spawn(6, 1, [center[0] + 90.0, center[1]], true);
    game.apply_native_damage(ally, Some(second), 1.0, DeathType::Normal);
    game.update_guard(ally, center);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Return
    ));
    assert_ne!(game.entities[ally].ai.ai_target, Some(second));
    assert_eq!(game.entities[ally].damage_state.last_attacker, Some(second));
}

#[test]
fn partition_query_recruits_nearby_helpers_once_without_remote_entities() {
    let (mut game, victim, ally, attacker) = fixture();
    let remote = (0..32)
        .map(|offset| {
            game.spawn(
                6,
                0,
                [700.0 + f32::from(u16::try_from(offset).unwrap()), 520.0],
                true,
            )
        })
        .collect::<Vec<_>>();
    game.update_ground_contacts();
    let radius = game.ai_rules.retaliation_friends_radius + game.object_radius(victim);
    let candidates = game
        .partition
        .range_objects(game.entities[victim].position, radius);
    assert!(candidates.contains(&ally));
    assert!(remote.iter().all(|id| !candidates.contains(id)));
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    assert!(game.entities[ally].ai.guard.retaliating);
    assert!(
        remote
            .iter()
            .all(|&id| !game.entities[id].ai.guard.retaliating)
    );
}

#[test]
fn outer_deadline_uses_saved_home_xyz_but_pursuit_exit_remains_xy() {
    let (mut game, victim, ally, attacker) = fixture();
    game.entities[ally].placed_height_offset = Some(25.0);
    game.apply_native_damage(victim, Some(attacker), 1.0, DeathType::Normal);
    assert_eq!(
        game.entities[ally].ai.guard.home_height.to_bits(),
        25.0_f32.to_bits()
    );
    let center = game.entities[ally].position;
    game.ai_rules.guard_chase_frames = 7;
    game.tick = 10;
    game.enter_guard_phase(ally, GuardPhase::Outer);
    game.entities[ally].ai.guard.deadline = 15;
    game.entities[attacker].position = [center[0] + 100.0, center[1]];
    game.entities[attacker].placed_height_offset = Some(225.0);
    // This fixture's locomotor classifies 225 as a ground target, but its
    // 200-unit elevation from the saved home is outside the XYZ refresh sphere.
    let threshold = game.entities[attacker]
        .active_locomotor(&game.catalog[game.entities[attacker].type_index])
        .airborne_targeting_height;
    assert!(game.height_above_ground(attacker) < threshold);
    game.update_guard(ally, center);
    assert_eq!(game.entities[ally].ai.guard.deadline, 15);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Outer
    ));
    game.entities[attacker].placed_height_offset = Some(25.0);
    game.update_guard(ally, center);
    assert_eq!(game.entities[ally].ai.guard.deadline, 17);
    assert!(matches!(
        game.entities[ally].ai.guard.phase,
        GuardPhase::Outer
    ));
}
