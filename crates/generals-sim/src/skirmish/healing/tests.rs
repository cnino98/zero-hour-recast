use crate::skirmish::{Kind, tests};

#[test]
fn inherited_veterancy_and_research_instances_keep_independent_damage_wakes() {
    let mut game = tests::game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[kind];
    unit.maintenance.auto_heal.push(tests::self_heal(
        10.0,
        30,
        150,
        &["Upgrade_Veterancy_ELITE"],
    ));
    tests::register_update(unit, "AutoHealBehavior");
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers =
        vec!["Upgrade_Veterancy_ELITE".into()];
    unit.maintenance
        .auto_heal
        .push(tests::self_heal(2.0, 30, 0, &["Upgrade_GLAJunkRepair"]));
    tests::register_update(unit, "AutoHealBehavior");
    unit.behavior_modules.last_mut().unwrap().upgrade.triggers =
        vec!["Upgrade_GLAJunkRepair".into()];
    let id = game.spawn(kind, 0, [400.0, 300.0], true);
    game.entities[id].health = 40.0;
    game.set_veterancy(id, 2);
    game.players[0]
        .upgrades
        .push("Upgrade_GLAJunkRepair".into());
    game.activate_upgrade_modules(id);
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 52.0_f32.to_bits());
    game.tick = 10;
    game.apply_hit(id, 2, 1.0);
    let last = game.update_objects[game.entities[id].update_owner]
        .handles
        .len()
        - 1;
    assert_eq!(game.update_remaining_sleep(id, last - 1), 150);
    assert_eq!(game.update_remaining_sleep(id, last), 21);
    game.tick = 31;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 53.0_f32.to_bits());
    assert_eq!(game.update_remaining_sleep(id, last - 1), 129);
}

#[test]
fn auto_heal_accepts_held_but_engine_defers_hacked_and_never_revives_effective_death() {
    let mut game = tests::game();
    let kind = game.type_for(0, Kind::Tank).unwrap();
    let unit = &mut game.catalog[kind];
    let mut heal = tests::self_heal(4.0, 1, 0, &[]);
    heal.starts_active = true;
    unit.maintenance.auto_heal.push(heal);
    tests::register_update(unit, "AutoHealBehavior");
    let id = game.spawn(kind, 0, [400.0, 300.0], true);
    game.entities[id].health = 40.0;
    game.entities[id].containment.carrier = Some(0);
    game.tick = 1;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 44.0_f32.to_bits());
    game.set_hacked_until(id, crate::skirmish::ai_dispatch::native_frame(10));
    game.tick = 2;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 44.0_f32.to_bits());
    let registration = game.update_objects[game.entities[id].update_owner]
        .handles
        .len()
        - 1;
    assert_eq!(game.update_remaining_sleep(id, registration), 1);
    game.clear_hacked_disable(id);
    game.entities[id].set_effectively_dead(true);
    game.tick = 3;
    game.run_object_updates();
    assert_eq!(game.entities[id].health.to_bits(), 44.0_f32.to_bits());
    assert_eq!(
        game.update_remaining_sleep(id, registration),
        super::FOREVER - game.update_frame()
    );
}
