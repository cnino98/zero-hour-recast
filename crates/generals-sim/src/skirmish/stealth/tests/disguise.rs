use super::{Detector, DisguiseRules, Event, Skirmish, rules, tests};

#[test]
fn desired_disguise_controls_targeting_before_the_drawable_swap() {
    let mut game = tests::game();
    let observer_type = 6;
    game.catalog[observer_type]
        .behavior
        .detectors
        .push(Detector {
            module_order: 0,
            rate: 89,
            range: 100.0,
            initially_disabled: false,
            required_kind: Vec::new(),
            forbidden_kind: Vec::new(),
            while_garrisoned: false,
            while_contained: false,
            feedback: None,
        });
    tests::register_update(&mut game.catalog[observer_type], "StealthDetectorUpdate");
    let truck_type = game.catalog.len();
    game.catalog.push(tests::fixture_type(
        crate::skirmish::Faction::Gla,
        crate::skirmish::Kind::Tank,
    ));
    let mut stealth = rules();
    stealth.forbidden = 0;
    stealth.delay = 0;
    stealth.disguise = Some(DisguiseRules {
        transition: 60,
        reveal_transition: 30,
        effect: "Disguise".into(),
        reveal_effect: "Reveal".into(),
        sounds: ["Start".into(), "Success".into(), "Failure".into()],
    });
    game.catalog[truck_type].behavior.stealth = Some(stealth);
    game.catalog[truck_type].kind_of.push("DISGUISER".into());
    tests::register_update(&mut game.catalog[truck_type], "StealthUpdate");
    let observer = game.spawn(observer_type, 0, [400.0, 300.0], true);
    let truck = game.spawn(truck_type, 1, [450.0, 300.0], true);
    game.update_ground_contacts();
    assert!(game.weapon_can_target(observer, truck));

    game.disguise_as_vehicle(truck, observer);
    update(&mut game, truck, 29);
    assert!(game.entities[truck].disguised_appearance().is_none());
    let hover = game.entities[truck].disguise_hover_appearance().unwrap();
    assert_eq!((hover.type_index, hover.owner), (observer_type, 0));
    assert_eq!(game.entities[truck].disguised_player(), Some(0));
    assert!(!game.weapon_can_target(observer, truck));
    assert!(!game.partition_stealth_visible(observer, truck));
    update(&mut game, truck, 1);
    let appearance = game.entities[truck].disguised_appearance().unwrap();
    assert_eq!(
        (appearance.type_index, appearance.owner),
        (observer_type, 0)
    );
    assert_eq!(game.entities[truck].owner, 1);
    assert_eq!(game.apparent_disguise_owner(truck, 0), 0);
    assert_eq!(game.apparent_disguise_owner(truck, 1), 1);
    assert!(game.weapon_can_target_mode(observer, truck, true));
    assert!(game.presentation.stealth_events.iter().any(|event| {
        matches!(event, Event::DisguiseOpacity { object, opacity }
            if *object == truck && opacity.to_bits() == 0.0_f32.to_bits())
    }));
    assert!(
        game.presentation
            .object_sounds
            .iter()
            .any(|event| event.name == "Start")
    );

    game.update_stealth_detector(observer, 0);
    assert!(game.presentation.stealth_events.iter().any(|event| {
        matches!(event, Event::Detection { detector, target } if *detector == observer && *target == truck)
    }));
    assert_eq!(game.entities[truck].disguised_player(), None);
    assert!(game.entities[truck].disguise_hover_appearance().is_none());
    assert!(game.entities[truck].disguised_appearance().is_some());
    assert!(game.weapon_can_target(observer, truck));
    assert!(game.partition_stealth_visible(observer, truck));
    update(&mut game, truck, 14);
    assert!(game.entities[truck].disguised_appearance().is_some());
    update(&mut game, truck, 1);
    assert!(game.entities[truck].disguised_appearance().is_none());
    assert_eq!(game.apparent_disguise_owner(truck, 0), 1);
    assert!(
        game.presentation
            .object_sounds
            .iter()
            .any(|event| event.name == "Failure")
    );
    update(&mut game, truck, 15);
    assert!(!game.entities[truck].stealthed());
    assert!(!game.entities[truck].detected());
}

fn update(game: &mut Skirmish, truck: usize, frames: u32) {
    for _ in 0..frames {
        game.tick += 1;
        game.update_concealment(truck);
    }
}
