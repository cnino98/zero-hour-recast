use super::ModuleRegistration;
use crate::skirmish::{
    Regeneration, UnitType,
    tests::{self, game},
};

fn regeneration(unit: &mut UnitType, tags: &[&str]) {
    unit.update_modules.clear();
    unit.behavior_modules.clear();
    for tag in tags {
        tests::register_update(unit, "BaseRegenerateUpdate");
        unit.behavior_modules.last_mut().unwrap().tag = (*tag).into();
        let ModuleRegistration::Behavior {
            tag: update_tag, ..
        } = unit.update_modules.last_mut().unwrap()
        else {
            panic!("regeneration is an authored behavior update");
        };
        *update_tag = (*tag).into();
    }
}

#[test]
fn repeated_regeneration_instances_each_heal_and_damage_wakes_both() {
    let mut game = game();
    regeneration(&mut game.catalog[6], &["First", "Second"]);
    game.catalog[6].maintenance.regeneration = Some(Regeneration {
        percent_per_second: 0.1,
        delay: 9,
    });
    let id = game.spawn(6, 0, [300.0, 200.0], true);
    game.entities[id].max_health = 1000.0;
    game.entities[id].health = 900.0;
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 920.0_f32.to_bits());
    assert_eq!(game.update_remaining_sleep(id, 0), 3);
    assert_eq!(game.update_remaining_sleep(id, 1), 3);
    game.regeneration_damaged(id);
    assert_eq!(game.update_remaining_sleep(id, 0), 9);
    assert_eq!(game.update_remaining_sleep(id, 1), 9);
    game.tick = 9;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 920.0_f32.to_bits());
    game.tick = 10;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 940.0_f32.to_bits());
}

#[test]
fn disabled_due_module_does_no_healing_then_runs_after_disabled_edge() {
    let mut game = game();
    regeneration(&mut game.catalog[6], &["Regenerate"]);
    game.catalog[6].maintenance.regeneration = Some(Regeneration {
        percent_per_second: 0.1,
        delay: 9,
    });
    let id = game.spawn(6, 0, [300.0, 200.0], true);
    game.entities[id].max_health = 1000.0;
    game.entities[id].health = 900.0;
    game.set_hacked_until(id, crate::skirmish::ai_dispatch::native_frame(10));
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 900.0_f32.to_bits());
    assert_eq!(game.update_remaining_sleep(id, 0), 1);
    game.clear_hacked_disable(id);
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 910.0_f32.to_bits());
    assert_eq!(game.update_remaining_sleep(id, 0), 3);
}

#[test]
fn destruction_request_retains_lookup_and_heap_until_end_frame_drain() {
    let mut game = game();
    regeneration(&mut game.catalog[6], &["Regenerate"]);
    game.catalog[6].maintenance.regeneration = Some(Regeneration {
        percent_per_second: 0.1,
        delay: 9,
    });
    let id = game.spawn(6, 0, [300.0, 200.0], true);
    game.destroy_maintenance_object(id);
    assert!(game.entities[id].exists());
    assert!(game.update_objects[game.entities[id].update_owner].destroy_requested);
    assert_eq!(
        game.update_objects[game.entities[id].update_owner]
            .handles
            .len(),
        1
    );
    game.process_update_destruction();
    assert!(!game.entities[id].exists());
    assert!(
        game.update_objects[game.entities[id].update_owner]
            .handles
            .is_empty()
    );
    game.tick = 1;
    game.run_object_updates();
}
