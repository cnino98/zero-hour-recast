use super::*;

#[test]
fn scans_keep_callback_pose_and_hide_enemy_feedback_while_detector_is_cloaked() {
    let (mut game, _, _) = fixture();
    let mut unit = tests::fixture_type(crate::skirmish::Faction::Usa, crate::skirmish::Kind::Tank);
    unit.behavior.stealth = Some(rules());
    unit.behavior.detectors.push(Detector {
        module_order: 1,
        rate: 3,
        range: 100.0,
        initially_disabled: false,
        required_kind: vec![],
        forbidden_kind: vec![],
        while_garrisoned: false,
        while_contained: false,
        feedback: Some(DetectorFeedback {
            bone: "IRFX".into(),
            particles: [String::new(), String::new(), "IRLenzflare".into()],
            sounds: ["IRPing".into(), "IRPingLoud".into()],
        }),
    });
    tests::register_update(&mut unit, "StealthUpdate");
    tests::register_update(&mut unit, "StealthDetectorUpdate");
    let kind = game.catalog.len();
    game.catalog.push(unit);
    let detector = game.spawn(kind, 0, [400.0, 300.0], true);
    game.wake_family_update(detector, "StealthDetectorUpdate", 0, 2);
    // Newly spawned lookers have not yet passed the normal partition update.
    // Both live observers see this location; only the native stealth gate
    // should hide the first scan from the enemy.
    game.update_visibility();
    assert!(game.visible_to(0, detector));
    assert!(game.visible_to(1, detector));
    game.tick = 2;
    game.update_concealment(detector);
    assert!(game.entities[detector].stealthed());
    let original = game.object_position(detector);
    game.run_object_updates();
    let scans = game
        .presentation
        .stealth_events
        .iter()
        .filter(|event| matches!(event, Event::Scan { .. }))
        .count();
    game.run_object_updates();
    assert_eq!(
        game.presentation
            .stealth_events
            .iter()
            .filter(|event| matches!(event, Event::Scan { .. }))
            .count(),
        scans
    );
    game.entities[detector].position = [420.0, 300.0];
    game.entities[detector].ai.motion.velocity = [0.2, 0.0];
    game.observe_visibility(detector);
    game.update_visibility();
    game.tick = 5;
    game.update_concealment(detector);
    assert!(!game.entities[detector].stealthed());
    game.run_object_updates();
    let scans: Vec<_> = game
        .presentation
        .stealth_events
        .iter()
        .filter_map(|event| match event {
            Event::Scan {
                feedback: Some(scan),
                ..
            } => Some(scan),
            _ => None,
        })
        .collect();
    assert_eq!(scans.len(), 2);
    assert_eq!(
        scans[0].position.map(f32::to_bits),
        original.map(f32::to_bits)
    );
    assert_eq!(
        scans[0].visible_players & 3,
        1,
        "native stealth gate permits the controller, not enemy viewers"
    );
    assert_eq!(scans[1].visible_players & 3, 3);
    assert_eq!(scans[1].position[0].to_bits(), 420.0_f32.to_bits());
}
