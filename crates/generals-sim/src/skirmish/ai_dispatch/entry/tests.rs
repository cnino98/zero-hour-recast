use crate::skirmish::{
    locomotor::SavedGroundState,
    point_defense::tests::{missile_rules, missile_template},
    startup,
    tests::{game, register_update},
    turret,
};

#[test]
fn ai_constructor_draws_turret_then_locomotor_between_authored_deletions() {
    for ai_first in [false, true] {
        let mut game = game();
        let mut unit = missile_template(missile_rules(), 1.0);
        unit.update_modules.clear();
        unit.behavior_modules.clear();
        unit.behavior.instant_death.clear();
        unit.behavior.deletions = vec![[10, 20], [30, 40]];
        let mut turret = turret::Rules::source_defaults();
        turret.controlled_slots = 1;
        turret.idle_intervals = [7, 15];
        unit.turret = Some(turret);
        if ai_first {
            register_update(&mut unit, "MissileAIUpdate");
        }
        register_update(&mut unit, "DeletionUpdate");
        if !ai_first {
            register_update(&mut unit, "MissileAIUpdate");
        }
        register_update(&mut unit, "DeletionUpdate");
        register_update(&mut unit, "PhysicsBehavior");
        crate::skirmish::module_updates::validate_registrations(&unit).unwrap();
        let mut expected = game.random;
        let mut deletion = Vec::new();
        if !ai_first {
            deletion.push(10 + startup::random_between(&mut expected, 10));
        }
        let _ = startup::random_between(&mut expected, 8);
        let _ = SavedGroundState::from_template(game.tick, &unit.locomotor, &mut expected);
        if ai_first {
            deletion.push(10 + startup::random_between(&mut expected, 10));
        }
        deletion.push(30 + startup::random_between(&mut expected, 10));
        // AIUpdate::onObjectCreated creates three nested Idle machines and enters primary Idle.
        let offsets: [u32; 4] = std::array::from_fn(|_| startup::random_between(&mut expected, 60));
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [300.0, 200.0], true);
        assert_eq!(game.entities[id].behaviors.deletion_frames, deletion);
        assert_eq!(
            game.random, expected,
            "constructor must not run again onObjectCreated"
        );
        assert_eq!(game.entities[id].ai.idle.sleep_offset, offsets[3]);
        assert_eq!(game.entities[id].ai.surface_motion.len(), 1);
        assert_eq!(
            game.entities[id].ai.registration,
            Some(usize::from(!ai_first))
        );
    }
}

#[test]
fn waking_registered_ai_changes_only_its_deadline() {
    let mut game = game();
    let mut unit = missile_template(missile_rules(), 1.0);
    unit.turret = Some(turret::Rules::source_defaults());
    unit.turret.as_mut().unwrap().controlled_slots = 1;
    unit.turret.as_mut().unwrap().idle_intervals = [7, 15];
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [300.0, 200.0], true);
    let registration = game.entities[id].ai.registration.unwrap();
    game.entities[id].ai.mood.next_check = 88;
    game.entities[id].ai.mood.offset_check = true;
    game.entities[id].ai.motion.set_no_slow_down(true);
    game.entities[id].ai.motion.velocity = [1.0, 2.0];
    game.wake_update(id, registration, 90);
    let random = game.random;
    game.wake_ai_now(id);
    assert_eq!(game.update_remaining_sleep(id, registration), 1);
    assert_eq!(game.random, random);
    assert_eq!(game.entities[id].ai.mood.next_check, 88);
    assert!(game.entities[id].ai.mood.offset_check);
    assert!(game.entities[id].ai.motion.no_slow_down());
    assert_eq!(
        game.entities[id].ai.motion.velocity.map(f32::to_bits),
        [1.0_f32, 2.0].map(f32::to_bits)
    );
}
