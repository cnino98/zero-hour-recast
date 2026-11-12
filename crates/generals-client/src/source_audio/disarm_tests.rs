use super::*;
use crate::skirmish_verification::friendly_entity;
use generals_sim::skirmish::Kind;

#[ignore = "requires original game installation"]
#[test]
fn installed_disarm_voice_overrides_normal_attack_with_the_selected_builder() {
    let (assets, battle) = crate::cleanup_verification::tests::installed();
    let game = &battle.simulation;
    let builder = friendly_entity(game, Kind::Builder, 0).unwrap();
    let headquarters = friendly_entity(game, Kind::CommandCenter, 0).unwrap();
    let request = VoiceRequest::new(game, &[headquarters, builder], Voice::Attack, false);
    assert!(matches!(request.voice(), Voice::Disarm));
    assert_eq!(
        request.candidates,
        [(builder, game.entities[builder].type_index)]
    );
    let units = data::units(&assets, &game.catalog).unwrap();
    for (name, expected) in [
        ("AmericaVehicleDozer", "DozerUSAVoiceClearMine"),
        ("ChinaVehicleDozer", "DozerChinaVoiceClearMine"),
        ("GLAInfantryWorker", "WorkerVoiceClearMine"),
    ] {
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .unwrap();
        assert_eq!(units[index].voices[Voice::Disarm.index()].name, expected);
    }
    let mut audio = speech_tests::audio();
    audio.units = units;
    audio.units[game.entities[builder].type_index].voices[Voice::Disarm.index()].name =
        "click".into();
    audio.voice(&request);
    assert_eq!(audio.requests.len(), 1);
    assert_eq!(audio.requests[0].object, Some(builder));
    assert_eq!(audio.requests[0].event, "click");
    let movement = VoiceRequest::new(game, &[builder], Voice::Move, false);
    assert!(matches!(movement.voice(), Voice::Move));
}
