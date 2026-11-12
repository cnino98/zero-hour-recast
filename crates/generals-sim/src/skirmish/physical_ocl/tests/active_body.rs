use super::*;

fn active_hull(health: f32) -> (Skirmish, usize, usize) {
    let (mut game, source) = fixture();
    let kind = game.catalog.len() - 1;
    let unit = &mut game.catalog[kind];
    unit.name = "ChinaVehicleListeningOutpostDeadHull".into();
    unit.body_kind = crate::skirmish::BodyKind::Active;
    unit.initial_health = 1.0;
    unit.max_health = 1.0;
    unit.physical.as_mut().unwrap().active_body = true;
    let mut list = list();
    let nugget = &mut list.nuggets[0];
    nugget.names = vec![unit.name.clone()];
    nugget.debris_mass = None;
    nugget.disposition = LIKE_EXISTING;
    nugget.health = [health; 2];
    let id = game.create_physical_ocl(source, &list).unwrap()[0];
    (game, source, id)
}

#[test]
fn active_hull_initial_health_uses_integer_percent_without_delivering_death() {
    let (game, _, id) = active_hull(0.379);
    let delta = 37.0_f32 / 100.0 - 1.0;
    assert_eq!(game.entities[id].health.to_bits(), (1.0 + delta).to_bits());
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Living
    ));
    let (mut game, _, id) = active_hull(0.0);
    assert_eq!(game.entities[id].health.to_bits(), 0.0_f32.to_bits());
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Living
    ));
    let random = game.random;
    game.kill_physical_object(id);
    assert!(game.entities[id].is_effectively_dead());
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Living
    ));
    assert_eq!(game.random, random);
}

#[test]
fn active_hull_rest_kills_health_and_lifetime_cannot_restart_slow_death() {
    let (mut game, _, id) = active_hull(1.0);
    let at = game.entities[id].position;
    let ground = game.ground_height(at);
    let state = game.entities[id].physical.as_mut().unwrap();
    state.motion.position[2] = ground;
    game.tick += 1; // A newborn PhysicsBehavior first wakes next frame.
    game.integrate_physical_object(id);
    assert_eq!(game.entities[id].health.to_bits(), 0.0_f32.to_bits());
    assert_eq!(game.entities[id].damage_state.last_attacker, None);
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Dying { .. }
    ));
    let random = game.random;
    game.tick += 1;
    game.update_lifetime(id);
    game.update_physical_slow_death(id);
    assert_eq!(game.random, random);
    assert!(game.entities[id].physical_present());
    game.tick += 60;
    game.update_lifetime(id);
    game.update_physical_slow_death(id);
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert!(game.entities[id].exists());
    game.process_update_destruction();
    assert!(!game.entities[id].physical_present());
}

#[test]
fn actual_damage_starts_the_same_hull_death_once_and_retains_attacker() {
    let (mut game, source, id) = active_hull(1.0);
    game.apply_hit(id, source, 1.0);
    assert_eq!(game.entities[id].damage_state.last_attacker, Some(source));
    assert!(matches!(
        game.entities[id].physical.as_ref().unwrap().phase,
        Phase::Dying { .. }
    ));
    let random = game.random;
    game.apply_hit(id, source, 1.0);
    game.kill_physical_object(id);
    assert_eq!(game.random, random);
    assert_eq!(game.entities[id].damage_state.last_attacker, Some(source));
}

#[test]
fn active_hull_damage_flags_survive_physics_while_inactive_debris_stays_pristine() {
    let (mut game, source, id) = active_hull(1.0);
    let flags = game.model_flags(&game.entities[id], [0.5, 0.1]);
    assert!(!flags.contains("DAMAGED"));
    assert!(!flags.contains("REALLYDAMAGED"));
    assert!(!flags.contains("RUBBLE"));
    game.apply_hit(id, source, 0.6);
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("DAMAGED")
    );
    game.apply_hit(id, source, 0.35);
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("REALLYDAMAGED")
    );
    game.apply_hit(id, source, 1.0);
    assert!(game.entities[id].physical_present());
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("RUBBLE")
    );
    let (mut game, source) = fixture();
    let id = game.create_physical_ocl(source, &list()).unwrap()[0];
    game.kill_physical_object(id);
    assert!(game.entities[id].physical_present());
    let flags = game.model_flags(&game.entities[id], [0.5, 0.1]);
    assert!(!flags.contains("DAMAGED"));
    assert!(!flags.contains("REALLYDAMAGED"));
    assert!(!flags.contains("RUBBLE"));
}
