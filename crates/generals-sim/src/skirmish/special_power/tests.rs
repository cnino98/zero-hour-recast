use super::*;
use crate::skirmish::{
    Kind,
    tests::{game, register_behavior},
};

fn rules(shared: bool) -> Rules {
    Rules {
        tag: "SpectrePower".into(),
        name: "Spectre".into(),
        power_type: "SPECIAL_SPECTRE_GUNSHIP".into(),
        required_science: String::new(),
        timer: Timer {
            reload_frames: 60,
            shared,
            public: false,
        },
        view_range: 0.0,
        view_frames: 0,
        view_template: None,
        initiate_sound: String::new(),
        target_sound: String::new(),
        module_initiate_sound: String::new(),
        academy_classification: "ACT_NONE".into(),
        update_starts_attack: true,
        startup: Startup {
            paused: false,
            ready: false,
        },
        script_only: false,
    }
}

fn fixture(rules: Rules) -> (Skirmish, usize) {
    let mut game = game();
    let kind = game.type_for(0, Kind::CommandCenter).unwrap();
    game.catalog[kind].special_powers.push(rules);
    register_behavior(&mut game.catalog[kind], "SpecialAbility", 0x0100);
    game.catalog[kind].behavior_modules.last_mut().unwrap().tag = "SpectrePower".into();
    let id = game.spawn(kind, 0, [200.0; 2], true);
    (game, id)
}

#[test]
fn shared_clock_first_trigger_uses_now_then_recharges_and_script_ready_is_local() {
    let (mut game, id) = fixture(rules(true));
    game.tick = 9;
    // No earlier isReady query: native resetOrStart inserts an absent timer at now.
    game.mark_special_power_triggered(id, "Spectre", Some([200.0, 200.0, 0.0]));
    assert_eq!(game.players[0].special_power_ready["Spectre"], 9);
    game.mark_special_power_triggered(id, "Spectre", Some([200.0, 200.0, 0.0]));
    assert_eq!(game.players[0].special_power_ready["Spectre"], 69);
    game.set_special_power_ready_frame(id, "Spectre", 9);
    assert!(!game.special_power_ready(id, "Spectre"));
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 9);
    assert_eq!(game.special_power_triggers.len(), 2);
    game.special_power_on_creation(id, 0);
    assert!(game.special_power_ready(id, "Spectre"));
}

#[test]
fn local_countdown_nested_pause_extends_once_and_crc_zero_starts_ready_is_honored() {
    let (mut game, id) = fixture(rules(false));
    assert!(!game.special_power_ready(id, "Spectre"));
    game.tick = 30;
    game.pause_special_power(id, 0, true);
    assert_eq!(
        game.special_power_percent(id, "Spectre").to_bits(),
        0.5_f32.to_bits()
    );
    game.tick = 40;
    game.pause_special_power(id, 0, true);
    game.tick = 50;
    game.pause_special_power(id, 0, false);
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 60);
    game.tick = 70;
    game.pause_special_power(id, 0, false);
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 100);
    game.tick = 100;
    assert!(game.special_power_ready(id, "Spectre"));
    let mut ready = rules(false);
    ready.startup.ready = true;
    ready.startup.paused = true;
    let (mut game, id) = fixture(ready);
    assert_eq!(
        game.special_power_percent(id, "Spectre").to_bits(),
        0.99999_f32.to_bits()
    );
    game.pause_special_power(id, 0, false);
    assert!(game.special_power_ready(id, "Spectre"));
}

#[test]
fn science_purchase_activates_existing_native_modules_and_sets_local_ready_frame() {
    let mut metadata = rules(false);
    metadata.required_science = "SCIENCE_Spectre".into();
    let (mut game, id) = fixture(metadata);
    let kind = game.entities[id].type_index;
    // This fixture installs the template after the initial map objects exist;
    // construct the newly installed behavior for those objects as well.
    for object in 0..game.entities.len() {
        if game.entities[object].type_index == kind
            && game.entities[object].behaviors.special_powers.is_empty()
        {
            game.construct_special_power(object, 0);
        }
    }
    let unfinished = game.spawn(kind, 0, [400.0; 2], false);
    game.configure_progression(crate::skirmish::ProgressionRules {
        multiplayer_colors: crate::skirmish::tests::original_multiplayer_colors(),
        ranks: vec![crate::skirmish::Rank {
            skill_points: 0,
            science_points: 1,
            sciences: Vec::new(),
        }],
        sciences: vec![crate::skirmish::Science {
            name: "SCIENCE_Spectre".into(),
            prerequisites: Vec::new(),
            cost: 1,
        }],
        factions: vec![crate::skirmish::FactionProgression {
            faction: crate::skirmish::Faction::Gla,
            side: "GLA".into(),
            preferred_color: 0xff00_ff00,
            playable_side: true,
            intrinsic_points: 0,
            intrinsic_sciences: Vec::new(),
        }],
        powers: Vec::new(),
    })
    .unwrap();
    assert!(!game.can_use_power(id, "Spectre"));
    game.tick = 20;
    game.purchase_science(0, "SCIENCE_Spectre").unwrap();
    assert!(game.can_use_power(id, "Spectre"));
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 20);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_frame,
        20
    );
    assert_eq!(
        game.entities[unfinished].behaviors.special_powers[0].available,
        20
    );
    assert!(!game.can_use_power(unfinished, "Spectre"));
    game.recharge_special_power(id, 0);
    assert!(game.purchase_science(0, "SCIENCE_Spectre").is_err());
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 80);
}

#[test]
fn per_type_disable_pauses_once_held_does_not_and_last_clear_resumes() {
    let (mut game, id) = fixture(rules(false));
    game.tick = 5;
    game.entities[id].disabled_status |= 1 << 3;
    game.observe_spy_vision_disabled(id);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        0
    );
    game.tick = 10;
    game.entities[id].disabled_status |= 1 << 2;
    game.observe_spy_vision_disabled(id);
    game.observe_spy_vision_disabled(id);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        1
    );
    let percent = game.special_power_percent(id, "Spectre").to_bits();
    game.tick = 20;
    game.entities[id].disabled_status |= 1;
    game.observe_spy_vision_disabled(id);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        2
    );
    game.tick = 30;
    game.entities[id].disabled_status &= !(1 << 2);
    game.observe_spy_vision_disabled(id);
    assert_eq!(game.special_power_percent(id, "Spectre").to_bits(), percent);
    game.tick = 40;
    game.entities[id].disabled_status &= !1;
    game.observe_spy_vision_disabled(id);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        0
    );
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 90);
    // Both edges in one logic frame remain separate Object callbacks.
    game.entities[id].disabled_status |= 1 << 2;
    game.observe_spy_vision_disabled(id);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        1
    );
    game.entities[id].disabled_status &= !(1 << 2);
    game.observe_spy_vision_disabled(id);
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        0
    );
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 90);
}

#[test]
fn unpause_upgrade_visits_all_matching_instances_and_ready_resets_each_local_clock() {
    let (mut game, first) = fixture(rules(true));
    let kind = game.entities[first].type_index;
    game.catalog[kind].special_powers[0].startup.paused = true;
    game.catalog[kind].special_powers[0].startup.ready = true;
    let mut second = rules(true);
    second.tag = "SecondPower".into();
    second.startup.paused = true;
    second.startup.ready = true;
    game.catalog[kind].special_powers.push(second);
    register_behavior(&mut game.catalog[kind], "SpecialAbility", 0x0100);
    game.catalog[kind].behavior_modules.last_mut().unwrap().tag = "SecondPower".into();
    let mut other = rules(false);
    other.tag = "UnrelatedPower".into();
    other.name = "Other".into();
    other.startup.paused = true;
    game.catalog[kind].special_powers.push(other);
    register_behavior(&mut game.catalog[kind], "SpecialAbility", 0x0100);
    game.catalog[kind].behavior_modules.last_mut().unwrap().tag = "UnrelatedPower".into();
    game.catalog[kind].unpause_special_powers = vec![Some("Spectre".into()), None];
    let id = game.spawn(kind, 0, [400.0; 2], true);
    game.pause_special_power(id, 1, true);
    game.players[0]
        .special_power_ready
        .insert("Spectre".into(), 900);
    game.tick = 37;
    game.apply_unpause_special_power_upgrade(id, 0);
    for state in &game.entities[id].behaviors.special_powers[..2] {
        assert_eq!(state.available, 37);
        assert_eq!(state.paused_frame, 37);
    }
    assert_eq!(
        game.entities[id].behaviors.special_powers[0].paused_count,
        0
    );
    assert_eq!(
        game.entities[id].behaviors.special_powers[1].paused_count,
        1
    );
    assert_eq!(
        game.entities[id].behaviors.special_powers[2].paused_count,
        1
    );
    assert_eq!(game.players[0].special_power_ready["Spectre"], 900);
    game.apply_unpause_special_power_upgrade(id, 1);
    assert_eq!(
        game.entities[id].behaviors.special_powers[1].paused_count,
        1
    );
}

#[test]
fn science_ready_notification_updates_each_matching_module_instance() {
    let mut game = game();
    let mut unit =
        crate::skirmish::tests::fixture_type(crate::skirmish::Faction::Gla, Kind::CommandCenter);
    for tag in ["FirstPower", "SecondPower"] {
        let mut metadata = rules(false);
        metadata.tag = tag.into();
        metadata.required_science = "SCIENCE_Spectre".into();
        unit.special_powers.push(metadata);
        register_behavior(&mut unit, "SpecialAbility", 0x0100);
        unit.behavior_modules.last_mut().unwrap().tag = tag.into();
    }
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let id = game.spawn(kind, 0, [400.0; 2], true);
    game.tick = 20;
    game.special_power_science_acquired(0, "SCIENCE_Spectre");
    for state in &game.entities[id].behaviors.special_powers {
        assert_eq!(state.available, 20);
        assert_eq!(state.paused_frame, 20);
    }
}

#[test]
fn creation_recharges_local_timer_and_expresses_only_shared_timers_ready_now() {
    for shared in [false, true] {
        let mut metadata = rules(shared);
        metadata.startup.paused = true;
        metadata.startup.ready = true;
        let (mut game, id) = fixture(metadata);
        game.tick = 20;
        game.special_power_on_creation(id, 0);
        let state = &game.entities[id].behaviors.special_powers[0];
        assert_eq!(state.available, if shared { 20 } else { 80 });
        // The constructor and onSpecialPowerCreation each call pauseCountdown.
        assert_eq!(state.paused_count, 2);
        if shared {
            assert_eq!(game.players[0].special_power_ready["Spectre"], 20);
        } else {
            assert!(!game.players[0].special_power_ready.contains_key("Spectre"));
        }
    }
}

#[test]
fn native_ready_frame_reports_shared_lazy_clock_and_paused_local_elapsed_time() {
    let (mut game, id) = fixture(rules(true));
    let owner = game.entities[id].owner;
    game.players[owner].special_power_ready.clear();
    game.tick = 17;
    assert_eq!(game.special_power_ready_frame(id, "Spectre"), Some(17));
    assert_eq!(
        game.players[owner].special_power_ready.get("Spectre"),
        Some(&17)
    );
    game.mark_special_power_triggered(id, "Spectre", None);
    game.tick = 18;
    assert_eq!(game.special_power_ready_frame(id, "Spectre"), Some(77));

    let (mut game, id) = fixture(rules(false));
    game.tick = 30;
    game.pause_special_power(id, 0, true);
    game.tick = 50;
    assert_eq!(game.special_power_ready_frame(id, "Spectre"), Some(80));
    assert_eq!(game.entities[id].behaviors.special_powers[0].available, 60);
    game.pause_special_power(id, 0, false);
    assert_eq!(game.special_power_ready_frame(id, "Spectre"), Some(80));
}
