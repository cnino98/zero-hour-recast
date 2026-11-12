use super::*;
use crate::skirmish::turret;

mod scheduled;

fn rules() -> Rules {
    Rules {
        pack: 30,
        unpack: 30,
        manual: false,
        turret_only_deployed: true,
        center_before_packing: true,
        sounds: ["Deploy".into(), "Undeploy".into()],
    }
}

#[test]
fn moving_path_takes_priority_over_guard_deployment_and_reverses_elapsed_time() {
    let (mut game, id) = scheduled::setup(true);
    game.guard(id, game.entities[id].position).unwrap();
    for _ in 0..120 {
        game.step();
        if matches!(
            game.entities[id].deployment.as_ref().unwrap().phase,
            Phase::Deploy
        ) {
            break;
        }
    }
    let entered = game.tick;
    let first_deadline = game.entities[id].deployment.as_ref().unwrap().deadline;
    assert_eq!(u64::from(first_deadline), entered + 30);
    for _ in 0..10 {
        game.step();
    }
    game.move_to(id, [450.0, 150.0]).unwrap();
    assert!(
        game.entities[id].ai.path_request.pending.is_some()
            || game.entities[id].ai.ground_path.is_some()
    );
    game.step();
    let state = game.entities[id].deployment.as_ref().unwrap();
    assert!(matches!(state.phase, Phase::Undeploy));
    assert_eq!(u64::from(state.deadline), game.tick + 11);
}

#[test]
fn zero_duration_waits_for_next_callback_and_idle_does_not_force_packing() {
    let (mut game, id) = scheduled::setup(false);
    let index = game.entities[id].type_index;
    let rules = game.catalog[index].deployment.as_mut().unwrap();
    rules.pack = 0;
    rules.unpack = 0;
    game.guard(id, game.entities[id].position).unwrap();
    for _ in 0..120 {
        game.step();
        if matches!(
            game.entities[id].deployment.as_ref().unwrap().phase,
            Phase::Deploy
        ) {
            break;
        }
    }
    assert!(matches!(
        game.entities[id].deployment.as_ref().unwrap().phase,
        Phase::Deploy
    ));
    game.step();
    assert!(matches!(
        game.entities[id].deployment.as_ref().unwrap().phase,
        Phase::Attack
    ));
    game.stop(id).unwrap();
    for _ in 0..10 {
        game.step();
    }
    assert_eq!(game.entities[id].deployment_condition(), Some("DEPLOYED"));
    assert_ne!(game.entities[id].object_status & (1 << 44), 0);
}

#[test]
fn manual_reversal_uses_unpack_time_in_both_directions_and_keeps_unsigned_deadline() {
    let (mut game, id) = scheduled::setup(true);
    let index = game.entities[id].type_index;
    let rules = game.catalog[index].deployment.as_mut().unwrap();
    rules.pack = 100;
    rules.unpack = 10;
    rules.manual = true;
    rules.center_before_packing = false;
    scheduled::guard_deploy(&mut game, id);
    game.move_to(id, [450.0, 150.0]).unwrap();
    game.step();
    assert!(matches!(
        game.entities[id].deployment.as_ref().unwrap().phase,
        Phase::Undeploy
    ));
    assert_eq!(game.entities[id].animation_frame(), Some((game.tick, 100)));
    let deadline = game.entities[id].deployment.as_ref().unwrap().deadline;
    game.group_force_attack_position(&[id], [350.0, 150.0, 0.0])
        .unwrap();
    assert!(matches!(
        game.entities[id].ai.order,
        crate::skirmish::Order::FireAt { .. }
    ));
    // The subclass sees the old move path until the base consumes onExit.
    assert!(game.entities[id].ai.ground_path.is_some());
    game.step();
    assert!(matches!(
        game.entities[id].deployment.as_ref().unwrap().phase,
        Phase::Undeploy
    ));
    assert!(game.entities[id].ai.ground_path.is_none());
    game.step();
    let state = game.entities[id].deployment.as_ref().unwrap();
    assert!(matches!(state.phase, Phase::Deploy));
    let now = game.update_frame();
    assert_eq!(
        state.deadline,
        now.wrapping_add(10)
            .wrapping_sub(deadline.wrapping_sub(now))
    );
    // The update's manual DEPLOY frame deliberately uses PackTime, unlike reversal.
    assert_eq!(
        game.entities[id].animation_frame(),
        Some((
            game.tick,
            i32::from_ne_bytes(
                100_u32
                    .wrapping_sub(state.deadline.wrapping_sub(now))
                    .to_ne_bytes()
            )
        ))
    );
}

#[test]
fn temporary_busy_preserves_underlying_idle_and_does_not_report_explicit_ai_busy() {
    let (mut game, id) = scheduled::setup(false);
    game.stop(id).unwrap();
    game.set_temporary_busy(id);
    assert!(game.entities[id].ai_idle());
    assert!(!game.entities[id].ai_busy());
    assert!(!game.object_busy(id));
    assert!(game.update_move_aside(id));
    game.tick += 1;
    assert!(game.update_move_aside(id));
    game.tick += 1;
    assert!(!game.update_move_aside(id));
}

#[test]
fn deployment_controls_the_current_weapon_secondary_turret() {
    let (mut game, old) = scheduled::setup(true);
    let index = game.entities[old].type_index;
    let position = game.entities[old].position;
    game.destroy_maintenance_object(old);
    game.process_update_destruction();
    let unit = &mut game.catalog[index];
    unit.turret.as_mut().unwrap().controlled_slots = 0;
    let mut secondary = turret::Rules::source_defaults();
    secondary.controlled_slots = 1;
    secondary.turn_rate = 0.01;
    secondary.idle_intervals = [10_000; 2];
    secondary.idle_angles = [0.0; 2];
    unit.secondary_turret = Some(secondary);
    let id = game.spawn(index, 0, position, true);
    scheduled::guard_deploy(&mut game, id);
    game.entities[id]
        .ai
        .secondary_turret
        .as_mut()
        .unwrap()
        .angle = 1.0;
    game.move_to(id, [450.0, 150.0]).unwrap();
    for _ in 0..10 {
        game.step();
        if matches!(
            game.entities[id].deployment.as_ref().unwrap().phase,
            Phase::Align
        ) {
            break;
        }
    }
    assert!(matches!(
        game.entities[id].deployment.as_ref().unwrap().phase,
        Phase::Align
    ));
    assert!(game.entities[id].ai.turret.enabled);
    assert!(
        game.entities[id]
            .ai
            .secondary_turret
            .as_ref()
            .unwrap()
            .angle
            < 1.0
    );
    for _ in 0..600 {
        game.step();
        if matches!(
            game.entities[id].deployment.as_ref().unwrap().phase,
            Phase::Undeploy
        ) {
            break;
        }
    }
    assert!(matches!(
        game.entities[id].deployment.as_ref().unwrap().phase,
        Phase::Undeploy
    ));
    assert!(game.entities[id].ai.turret.enabled);
    assert!(
        !game.entities[id]
            .ai
            .secondary_turret
            .as_ref()
            .unwrap()
            .enabled
    );
}
