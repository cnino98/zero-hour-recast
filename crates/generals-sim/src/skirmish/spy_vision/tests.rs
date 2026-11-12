use super::*;
use crate::skirmish::{
    tests::game,
    visibility::{Status, VisibilityRules},
};

fn fixture() -> (Skirmish, usize, usize) {
    let mut game = game();
    // The shared minimal fixture labels buildings STRUCTURE only; native
    // Satellite Hack One filters the additional COMMANDCENTER KindOf bit.
    game.catalog[0].kind_of.push("COMMANDCENTER".into());
    game.catalog[3].behavior.spy_vision = vec![
        Rules {
            needs_upgrade: true,
            self_powered: true,
            trigger: "One".into(),
            duration: 0,
            interval: 0,
            command_centers_only: true,
        },
        Rules {
            needs_upgrade: true,
            self_powered: true,
            trigger: "Two".into(),
            duration: 600,
            interval: 7200,
            command_centers_only: false,
        },
    ];
    for (tag, trigger) in [("HackOne", "One"), ("HackTwo", "Two")] {
        crate::skirmish::tests::register_update(&mut game.catalog[3], "SpyVisionUpdate");
        let module = game.catalog[3].behavior_modules.last_mut().unwrap();
        module.tag = tag.into();
        module.upgrade.triggers = vec![trigger.into()];
        if let Some(crate::skirmish::module_updates::ModuleRegistration::Behavior {
            tag: registered,
            ..
        }) = game.catalog[3].update_modules.last_mut()
        {
            *registered = tag.into();
        }
    }
    game.catalog[3].sight.clearing_range = 0.0;
    for actor in &mut game.entities {
        actor.position = [100.0, 100.0];
    }
    game.configure_visibility(VisibilityRules {
        cell_size: 40.0,
        unlook_persist_frames: 30,
        use_shroud: true,
    })
    .unwrap();
    // The shared 80x60 grid covers 800x600 native world units.
    let center = game.spawn(0, 1, [400.0, 200.0], true);
    let infantry = game.spawn(5, 1, [650.0, 450.0], true);
    assert!(
        game.catalog[game.entities[center].type_index]
            .kind_of
            .iter()
            .any(|flag| flag == "COMMANDCENTER")
    );
    assert!(
        !game.catalog[game.entities[infantry].type_index]
            .kind_of
            .iter()
            .any(|flag| flag == "COMMANDCENTER")
    );
    (game, center, infantry)
}

#[test]
fn original_spy_counts_stack_existing_looker_ranges_and_expire_at_native_deadline() {
    let (mut game, center, infantry) = fixture();
    game.players[0]
        .upgrades
        .extend(["One".into(), "Two".into()]);
    let spy = game.spawn(3, 0, [150.0, 100.0], true);
    assert_eq!(game.entities[center].spy_vision.by[0], 2);
    assert_eq!(game.entities[infantry].spy_vision.by[0], 1);
    game.update_visibility();
    assert!(matches!(
        game.visibility_cell(0, [650.0, 450.0]),
        Status::Clear
    ));
    let future = game.spawn(5, 1, [750.0, 450.0], true);
    assert_eq!(game.entities[future].spy_vision.by[0], 0);
    let deadline = u64::from(game.entities[spy].spy_vision.modules[1].deactivate);
    game.tick = deadline - 1;
    game.run_object_updates();
    assert_eq!(game.entities[infantry].spy_vision.by[0], 1);
    game.tick = deadline;
    game.run_object_updates();
    assert_eq!(game.entities[center].spy_vision.by[0], 1);
    assert_eq!(game.entities[infantry].spy_vision.by[0], 0);
    assert_eq!(game.entities[future].spy_vision.by[0], -1);
    game.update_visibility();
    game.tick += 31;
    game.update_visibility();
    assert!(matches!(
        game.visibility_cell(0, [650.0, 450.0]),
        Status::Fogged
    ));
    game.tick = deadline + 7200;
    game.run_object_updates();
    assert_eq!(game.entities[infantry].spy_vision.by[0], 1);
    assert_eq!(game.entities[future].spy_vision.by[0], 0);
}

#[test]
fn capture_and_actual_delete_transfer_stacked_spy_effects_without_reapplying_upgrade() {
    let (mut game, center, _) = fixture();
    game.players[0].upgrades.push("One".into());
    let first = game.spawn(3, 0, [150.0, 100.0], true);
    let second = game.spawn(3, 0, [250.0, 100.0], true);
    assert_eq!(game.entities[center].spy_vision.by[0], 2);
    game.activate_upgrade_modules(first);
    assert_eq!(game.entities[center].spy_vision.by[0], 2);
    let friendly_center = game.spawn(0, 0, [500.0, 500.0], true);
    game.transfer_maintenance_owner(first, 1);
    assert_eq!(game.entities[center].spy_vision.by[0], 1);
    assert_eq!(game.entities[friendly_center].spy_vision.by[1], 1);
    game.entities[second].health = 0.0;
    game.run_object_updates();
    assert_eq!(game.entities[center].spy_vision.by[0], 1);
    game.delete_spy_vision(second);
    assert_eq!(game.entities[center].spy_vision.by[0], 0);
    game.delete_spy_vision(first);
    assert_eq!(game.entities[friendly_center].spy_vision.by[1], 0);
}

#[test]
fn sabotage_disables_all_centers_and_restarts_periodic_interval_after_disabled_edge() {
    let (mut game, center, infantry) = fixture();
    game.players[0]
        .upgrades
        .extend(["One".into(), "Two".into()]);
    let first = game.spawn(3, 0, [150.0, 100.0], true);
    let second = game.spawn(3, 0, [250.0, 100.0], true);
    let until = game.tick + 450;
    game.sabotage_spy_vision(0, until);
    game.set_hacked_until(first, crate::skirmish::ai_dispatch::native_frame(until));
    game.observe_spy_vision_disabled(first);
    assert_eq!(game.entities[center].spy_vision.by[0], 0);
    assert_eq!(game.entities[infantry].spy_vision.by[0], 0);
    game.tick = until;
    game.run_object_updates();
    assert!(!game.entities[first].spy_vision.modules[0].active);
    assert!(game.entities[second].spy_vision.modules[0].active);
    game.clear_hacked_disable(first);
    game.observe_spy_vision_disabled(first);
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[center].spy_vision.by[0], 2);
    assert_eq!(game.entities[infantry].spy_vision.by[0], 0);
    game.tick += 7200;
    game.run_object_updates();
    assert_eq!(game.entities[infantry].spy_vision.by[0], 2);
}

#[test]
fn native_disabled_wakeup_can_activate_an_unresearched_self_powered_module() {
    let (mut game, center, _) = fixture();
    let spy = game.spawn(3, 0, [150.0, 100.0], true);
    assert!(!game.entities[spy].spy_vision.modules[0].upgraded);
    game.set_hacked_until(spy, u32::MAX);
    assert!(game.entities[spy].hacked());
    assert!(game.clear_hacked_disable(spy));
    assert!(!game.entities[spy].hacked());
    game.tick += 1;
    game.run_object_updates();
    assert_eq!(game.entities[center].spy_vision.by[0], 1);
    assert!(!game.entities[spy].spy_vision.modules[0].upgraded);
}
