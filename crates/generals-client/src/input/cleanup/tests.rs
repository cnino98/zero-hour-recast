use super::*;
use crate::{
    cleanup_verification, input,
    skirmish_verification::friendly_entity,
    source_audio::{Voice, VoiceRequest, cleanup_verification::tests::assert_candidates},
};
use generals_sim::skirmish::{Kind, Order};

#[ignore = "requires original game installation"]
#[test]
fn cleanup_targets_only_ready_local_ambulances_without_falling_back_to_movement() {
    let (assets, mut battle) = cleanup_verification::tests::installed();
    let [first, second] = cleanup_verification::ambulances(&mut battle.simulation).unwrap();
    let builder = friendly_entity(&battle.simulation, Kind::Builder, 0).unwrap();
    battle.simulation.stop(builder).unwrap();
    let options = generals_assets::game_root()
        .unwrap()
        .join("UserData/Options.ini");
    let mut mouse = input::load(&assets, &options, &battle.simulation.catalog).unwrap();
    let center = battle.simulation.entities[builder].position;
    battle.select_objects(&[builder, first, second], true);
    assert_candidates(
        &VoiceRequest::new(&battle.simulation, &battle.selected, Voice::Move, false),
        &[builder, first, second],
    );
    battle.simulation.entities[first].build_progress = 0.5;
    assert!(!battle.simulation.cleanup_ready(first));
    assert_candidates(
        &VoiceRequest::new(
            &battle.simulation,
            &battle.selected,
            Voice::CleanupArea,
            false,
        ),
        &[first, second],
    );
    battle.simulation.entities[first].build_progress = 1.0;
    mouse.pending_command = CommandMode::Cleanup;
    super::super::targeted_order(&mut battle, &mut mouse, center.into(), None);
    crate::input::commands::step(&mut battle, &mut mouse);
    for id in [first, second] {
        assert_eq!(battle.simulation.entities[id].cleaning_area(), Some(center));
    }
    assert!(matches!(
        battle.simulation.entities[builder].ai.order,
        Order::Idle
    ));
    assert!(matches!(mouse.pending_command, CommandMode::Normal));
    assert_eq!(mouse.voices.len(), 1);
    assert!(matches!(mouse.voices[0].voice(), Voice::CleanupArea));
    assert_candidates(&mouse.voices[0], &[first, second]);
    assert!(mouse.move_hints.is_empty());
    battle.simulation.entities[first].health = 0.0;
    battle.simulation.entities[second].owner = 1;
    mouse.voices.clear();
    mouse.pending_command = CommandMode::Cleanup;
    super::super::targeted_order(
        &mut battle,
        &mut mouse,
        [center[0] + 50.0, center[1]].into(),
        None,
    );
    assert!(matches!(mouse.pending_command, CommandMode::Cleanup));
    assert!(matches!(
        battle.simulation.entities[builder].ai.order,
        Order::Idle
    ));
    assert!(mouse.voices.is_empty() && mouse.move_hints.is_empty());
    assert_eq!(
        battle.simulation.entities[second].cleaning_area(),
        Some(center)
    );
    battle.simulation.entities[second].owner = 0;
    battle.select_objects(&[second], true);
    super::super::targeted_order(&mut battle, &mut mouse, [-10.0, -10.0].into(), None);
    assert!(matches!(mouse.pending_command, CommandMode::Cleanup));
    assert_eq!(
        battle.simulation.entities[second].cleaning_area(),
        Some(center)
    );
    assert!(mouse.voices.is_empty());
}
