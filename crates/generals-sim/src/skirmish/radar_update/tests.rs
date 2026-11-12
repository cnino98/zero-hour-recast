use super::{EXTENDING, Rules, UPGRADED};
use crate::skirmish::{Kind, Skirmish, module_updates::ModuleRegistration, tests};

fn template(game: &mut Skirmish, frames: &[f32], granted: bool) -> usize {
    let index = game.type_for(0, Kind::Factory).unwrap();
    tests::radar_upgrade(
        &mut game.catalog[index],
        "Upgrade_RadarTest",
        granted,
        false,
    );
    for &extend_frames in frames {
        game.catalog[index]
            .behavior
            .radar_updates
            .push(Rules { extend_frames });
        tests::register_update(&mut game.catalog[index], "RadarUpdate");
    }
    index
}

fn registration(game: &Skirmish, index: usize) -> usize {
    game.catalog[index].update_modules.iter().position(|entry| matches!(entry, ModuleRegistration::Behavior { family, .. } if family == "RadarUpdate")).unwrap()
}

#[test]
fn actual_constructors_keep_idle_wakes_and_upgrade_extends_only_first_authored_instance() {
    let mut game = tests::game();
    let index = template(&mut game, &[2.5, 9.0], false);
    let mut control = tests::game();
    control.spawn(index, 0, [350.0, 250.0], true);
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    // Actual shared AI constructors consume the same RNG with or without Radar.
    assert_eq!(game.random, control.random);
    assert_eq!(game.entities[id].radar.modules.len(), 2);
    for module in &game.entities[id].radar.modules {
        assert_eq!(module.extend_done, 0);
        assert!(!module.complete);
    }
    let first = registration(&game, index);
    assert_eq!(game.update_remaining_sleep(id, first), 1);
    assert_eq!(game.update_remaining_sleep(id, first + 1), 1);
    game.grant_native_object_upgrade(id, "Upgrade_RadarTest");
    game.activate_upgrade_modules(id);
    assert_eq!(game.entities[id].radar.modules[0].extend_done, 2);
    assert_eq!(game.entities[id].radar.modules[1].extend_done, 0);
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    for tick in 1..=2 {
        game.tick = tick;
        game.run_object_updates();
        assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    }
    game.tick = 3;
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, UPGRADED);
    assert!(
        game.model_flags(&game.entities[id], [0.5, 0.1])
            .contains("RADAR_UPGRADED")
    );
    assert!(!game.entities[id].radar.modules[1].complete);
    assert_eq!(game.update_remaining_sleep(id, first), 1);
    assert_eq!(game.update_remaining_sleep(id, first + 1), 1);
}

#[test]
fn disabled_callback_polls_then_finishes_overdue_extension_after_recovery() {
    let (mut game, carrier) = crate::skirmish::containment::tests::setup(0.0, 1.0);
    let carrier_type = game.entities[carrier].type_index;
    game.catalog[carrier_type]
        .contain
        .as_mut()
        .unwrap()
        .allowed
        .push("STRUCTURE".into());
    let index = template(&mut game, &[1.0], true);
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    let first = registration(&game, index);
    game.set_hacked_until(id, crate::skirmish::ai_dispatch::native_frame(100));
    for tick in 1..=3 {
        game.tick = tick;
        game.run_object_updates();
        assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
        assert_eq!(game.update_remaining_sleep(id, first), 1);
    }
    game.clear_hacked_disable(id);
    game.add_to_contain(id, carrier);
    assert!(game.entities[id].disabled_held());
    game.tick = 4;
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    game.remove_passenger(id, carrier);
    assert!(!game.entities[id].disabled_held());
    game.tick = 5;
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, UPGRADED);
}

#[test]
fn paid_foundation_grant_waits_for_build_complete_and_keeps_native_extension_delay() {
    let mut game = tests::game();
    let index = template(&mut game, &[2.5], true);
    game.catalog[index].build_seconds = 1.0 / 30.0;
    let builder = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let position = game.placement_near(index, [350.0, 250.0]).unwrap();
    game.step();
    let id = game.build(builder, index, position).unwrap();
    assert_eq!(game.entities[id].radar.model_flags, 0);
    assert!(!game.entities[id].radar.upgrades[0]);
    for _ in 0..600 {
        game.step();
        if game.entities[id].complete() {
            break;
        }
    }
    assert!(game.entities[id].complete());
    assert!(game.entities[id].radar.upgrades[0]);
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    for _ in 0..2 {
        game.step();
    }
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    game.step();
    assert_eq!(game.entities[id].radar.model_flags, UPGRADED);
}

#[test]
fn no_drawable_does_not_replay_flags_and_reextension_keeps_native_complete_marker() {
    let mut game = tests::game();
    let index = template(&mut game, &[1.0], false);
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    game.entities[id].drawable.present = false;
    game.grant_native_object_upgrade(id, "Upgrade_RadarTest");
    game.activate_upgrade_modules(id);
    game.tick = 2;
    game.run_object_updates();
    assert!(game.entities[id].radar.modules[0].complete);
    assert_eq!(game.entities[id].radar.model_flags, 0);
    game.entities[id].drawable.present = true;
    game.tick = 3;
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, 0);
    game.extend_radar(id);
    game.tick = 10;
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    assert!(game.entities[id].radar.modules[0].complete);
}

#[test]
fn zero_deadline_sentinel_and_real_frame_rounding_match_native_assignment() {
    let mut game = tests::game();
    let index = template(&mut game, &[0.0], true);
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    game.tick = 10;
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    assert_eq!(game.entities[id].radar.modules[0].extend_done, 0);
    game.tick = 16_777_217;
    game.extend_radar(id);
    assert_eq!(game.entities[id].radar.modules[0].extend_done, 16_777_216);
    game.run_object_updates();
    assert_eq!(game.entities[id].radar.model_flags, UPGRADED);
}

#[test]
fn owned_player_trigger_cannot_extend_an_unfinished_paid_foundation_or_destroyed_object() {
    let mut game = tests::game();
    let index = template(&mut game, &[2.5], false);
    game.catalog[index].build_seconds = 1.0 / 30.0;
    game.players[0].upgrades.push("Upgrade_RadarTest".into());
    let builder = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let position = game.placement_near(index, [350.0, 250.0]).unwrap();
    game.step();
    let id = game.build(builder, index, position).unwrap();
    assert_eq!(game.entities[id].radar.model_flags, 0);
    assert!(!game.entities[id].radar.upgrades[0]);
    game.activate_upgrade_modules(id);
    assert!(!game.entities[id].radar.upgrades[0]);
    for _ in 0..600 {
        game.step();
        if game.entities[id].complete() {
            break;
        }
    }
    assert!(game.entities[id].complete());
    assert!(game.entities[id].radar.upgrades[0]);
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
    game.players[0].upgrades.clear();
    let doomed = game.spawn(index, 0, [500.0, 250.0], true);
    game.destroy_maintenance_object(doomed);
    game.players[0].upgrades.push("Upgrade_RadarTest".into());
    game.activate_upgrade_modules(doomed);
    assert_eq!(game.entities[doomed].radar.model_flags, 0);
    assert!(!game.entities[doomed].radar.upgrades[0]);
}

#[test]
fn repeated_upgrade_instances_keep_independent_latches_and_only_first_disability_adjustment() {
    let mut game = tests::game();
    let index = template(&mut game, &[2.0], false);
    tests::radar_upgrade(&mut game.catalog[index], "Upgrade_SecondRadar", false, true);
    game.catalog[index].behavior.radar_upgrades[1]
        .triggers
        .push("Upgrade_ThirdRadar".into());
    game.catalog[index].behavior.radar_upgrades[1].requires_all = true;
    game.catalog[index].behavior.radar_upgrades[1]
        .conflicts
        .push("Upgrade_NoRadar".into());
    let data = &mut game.catalog[index]
        .behavior_modules
        .iter_mut()
        .filter(|module| module.family == "RadarUpgrade")
        .nth(1)
        .unwrap()
        .upgrade;
    data.triggers.push("Upgrade_ThirdRadar".into());
    data.requires_all = true;
    data.conflicts.push("Upgrade_NoRadar".into());
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    game.grant_native_object_upgrade(id, "Upgrade_RadarTest");
    game.activate_upgrade_modules(id);
    assert_eq!(game.entities[id].radar.upgrades, [true, false]);
    assert_eq!(game.players[0].radar.count, 1);
    game.grant_native_object_upgrade(id, "Upgrade_NoRadar");
    game.grant_native_object_upgrade(id, "Upgrade_SecondRadar");
    game.grant_native_object_upgrade(id, "Upgrade_ThirdRadar");
    game.activate_upgrade_modules(id);
    assert_eq!(game.players[0].radar.count, 1);
    game.remove_native_object_upgrade(id, "Upgrade_NoRadar");
    game.tick = 1;
    game.activate_upgrade_modules(id);
    assert_eq!(game.entities[id].radar.upgrades, [true, true]);
    assert_eq!(game.entities[id].radar.modules[0].extend_done, 3);
    assert_eq!(game.players[0].radar.count, 2);
    assert_eq!(game.players[0].radar.disable_proof, 1);
    game.set_hacked_until(id, crate::skirmish::ai_dispatch::native_frame(100));
    assert_eq!(game.players[0].radar.count, 1);
    assert!(game.has_radar(0));
    game.clear_hacked_disable(id);
    game.transfer_maintenance_owner(id, 1);
    assert_eq!(game.players[0].radar.count, 0);
    assert_eq!(game.players[1].radar.count, 2);
    assert_eq!(game.entities[id].radar.modules[0].extend_done, 3);
    game.destroy_maintenance_object(id);
    assert_eq!(game.players[1].radar.count, 0);
    assert_eq!(game.entities[id].radar.upgrades, [false, false]);
}

#[test]
fn reset_only_clears_latch_and_retains_native_counter_history() {
    let mut game = tests::game();
    let index = template(&mut game, &[2.0], true);
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    game.remove_native_object_upgrade(id, "Upgrade_RadarTest");
    assert!(!game.entities[id].radar.upgrades[0]);
    assert_eq!(game.players[0].radar.count, 1);
    game.grant_native_object_upgrade(id, "Upgrade_RadarTest");
    game.activate_upgrade_modules(id);
    assert_eq!(game.players[0].radar.count, 2);
    game.destroy_maintenance_object(id);
    assert_eq!(game.players[0].radar.count, 1);
    assert!(game.has_radar(0));
}

#[test]
fn radar_audio_edges_require_logic_update_and_preserve_each_owner_transition() {
    let mut game = tests::game();
    let index = template(&mut game, &[], true);
    let initial = game.spawn(index, 0, [350.0, 250.0], true);
    assert!(game.presentation.radar_events.is_empty());
    game.destroy_maintenance_object(initial);
    game.tick = 10;
    game.logic_update = true;
    let id = game.spawn(index, 0, [400.0, 250.0], true);
    game.transfer_maintenance_owner(id, 1);
    game.set_player_radar_disabled(1, true);
    game.set_player_radar_disabled(1, true);
    let edges: Vec<_> = game
        .presentation
        .radar_events
        .iter()
        .map(|event| (event.owner, event.online))
        .collect();
    assert_eq!(edges, [(0, true), (0, false), (1, true), (1, false)]);
    game.logic_update = false;
    game.set_player_radar_disabled(1, false);
    assert_eq!(game.presentation.radar_events.len(), 4);
}

#[test]
#[should_panic(expected = "native UInt deadline conversion is undefined")]
fn runtime_deadline_overflow_is_reported_without_inventing_native_conversion() {
    let mut game = tests::game();
    let index = template(&mut game, &[1.0], false);
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    game.tick = u64::from(u32::MAX);
    game.grant_native_object_upgrade(id, "Upgrade_RadarTest");
    game.activate_upgrade_modules(id);
}

#[test]
fn completion_power_adjustment_precedes_create_grant_without_spurious_audio_edges() {
    let mut game = tests::game();
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    let index = template(&mut game, &[2.5], true);
    game.catalog[index].build_seconds = 1.0 / 30.0;
    game.catalog[index].energy = -10;
    game.update_power();
    let builder = game
        .entities
        .iter()
        .position(|actor| actor.owner == 0 && game.catalog[actor.type_index].kind == Kind::Builder)
        .unwrap();
    let position = game.placement_near(index, [350.0, 250.0]).unwrap();
    game.step();
    let id = game.build(builder, index, position).unwrap();
    for _ in 0..600 {
        game.step();
        assert!(game.presentation.radar_events.is_empty());
        if game.entities[id].complete() {
            break;
        }
    }
    assert!(game.entities[id].complete());
    assert_eq!(game.players[0].radar.count, 1);
    assert!(game.players[0].radar.disabled);
    assert!(!game.has_radar(0));
    assert_eq!(game.entities[id].radar.model_flags, EXTENDING);
}

#[test]
fn zero_energy_capture_retains_underpowered_bit_until_a_real_energy_callback() {
    let mut game = tests::game();
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    let index = template(&mut game, &[], true);
    game.catalog[index].kind_of.push("POWERED".into());
    game.catalog[index].behavior.radar_upgrades[0].disable_proof = true;
    let consumer = game.type_for(1, Kind::Tank).unwrap();
    game.catalog[consumer].energy = -10;
    game.spawn(consumer, 1, [600.0, 250.0], true);
    game.update_power();
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    assert!(game.has_operational_power(id));
    game.transfer_maintenance_owner(id, 1);
    assert!(game.has_operational_power(id));
    assert!(game.has_radar(1));
    assert_eq!(game.players[1].radar.count, 1);
    // A later nonzero influence refreshes the native retained disabled status.
    game.spawn(consumer, 1, [650.0, 250.0], true);
    game.update_power();
    assert!(!game.has_operational_power(id));
    assert!(!game.has_radar(1));
    assert_eq!(game.players[1].radar.count, 0);
}

#[test]
fn nonzero_energy_capture_refreshes_disabled_status_after_team_influence_switch() {
    let mut game = tests::game();
    for unit in &mut game.catalog {
        unit.energy = 0;
    }
    let index = template(&mut game, &[], false);
    game.catalog[index].kind_of.push("POWERED".into());
    game.catalog[index].energy = -10;
    let plant = game.type_for(0, Kind::Tank).unwrap();
    game.catalog[plant].energy = 20;
    game.spawn(plant, 0, [600.0, 250.0], true);
    game.update_power();
    let id = game.spawn(index, 0, [350.0, 250.0], true);
    game.update_power();
    assert!(game.has_operational_power(id));
    game.transfer_maintenance_owner(id, 1);
    assert!(!game.has_operational_power(id));
    assert_eq!(game.players[0].energy().1.to_bits(), 0.0_f32.to_bits());
    assert_eq!(game.players[1].energy().1.to_bits(), 10.0_f32.to_bits());
    game.transfer_maintenance_owner(id, 0);
    assert!(game.has_operational_power(id));
    assert_eq!(game.players[0].energy().1.to_bits(), 10.0_f32.to_bits());
    assert_eq!(game.players[1].energy().1.to_bits(), 0.0_f32.to_bits());
}
