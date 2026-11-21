//! Original `CommandXlat` special-power response survives simulation frame clearing.
use super::*;
use crate::{
    helix_verification,
    input::{self, CommandMode},
    skirmish_verification::friendly_entity,
};
use generals_sim::skirmish::Kind;
use std::path::Path;

#[ignore = "requires original game installation"]
#[test]
fn installed_helix_command_voice_survives_step_cancel_and_selection_change_without_replay() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let setup = helix_verification::prepare(&mut battle.simulation).unwrap();
    let caster = setup.helicopters[1];
    let unready = setup.helicopters[0];
    helix_verification::arm_bomb(&mut battle.simulation, caster).unwrap();
    let headquarters = friendly_entity(&battle.simulation, Kind::CommandCenter, 0).unwrap();
    let mut mouse = input::load(&assets, Path::new(""), &battle.simulation.catalog).unwrap();
    let mut audio = load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
    let index = battle.simulation.entities[unready].type_index;
    let voice = VoiceRequest::special_power(
        &battle.simulation,
        &[unready],
        &["SPECIAL_HELIX_NAPALM_BOMB"],
    )
    .unwrap()
    .voice;
    assert_eq!(
        audio.units[index].voices[voice.index()].name,
        "HelixVoiceAttack"
    );
    assert!(!battle.simulation.helix_bomb_ready(unready));
    let position = battle.simulation.entities[caster].position;
    // Native response picks the first matching SpecialAbility module, even if
    // another selected Helix is the object that accepted the command.
    battle.select_objects(&[headquarters, unready, caster], true);
    for offset in [100.0, 120.0] {
        mouse.pending_command = CommandMode::HelixBomb;
        input::targeted_order(
            &mut battle,
            &mut mouse,
            Vec2::new(position[0] + offset, position[1]),
            None,
        );
        assert!(matches!(mouse.pending_command, CommandMode::Normal));
    }
    input::commands::step(&mut battle, &mut mouse);
    assert_eq!(mouse.voices.len(), 2);
    assert_eq!(mouse.voices[0].candidates.first().unwrap().0, unready);
    assert!(
        battle
            .simulation
            .presentation
            .object_sounds
            .iter()
            .all(|event| event.name != "HelixVoiceAttack")
    );
    crate::playtest_ui::dispatch_command(
        &mut battle,
        &mut mouse,
        crate::control_bar::CommandAction::Stop,
    );
    battle.clear_drawable_selection();
    input::commands::step(&mut battle, &mut mouse);
    deliver(&mut audio, &mut mouse);
    assert_eq!(audio.requests.len(), 2);
    for request in &audio.requests {
        assert_eq!(request.event, "HelixVoiceAttack");
        assert_eq!(request.object, Some(unready));
        assert!(!request.samples.is_empty());
        assert!(
            request
                .samples
                .iter()
                .all(|sample| sample.clone().next().is_some())
        );
    }
    for _ in 0..3 {
        battle.simulation.step();
        deliver(&mut audio, &mut mouse);
        assert_eq!(audio.requests.len(), 2);
    }
    battle.select_objects(&[unready], true);
    mouse.pending_command = CommandMode::HelixBomb;
    input::targeted_order(&mut battle, &mut mouse, Vec2::from_array(position), None);
    assert!(mouse.voices.is_empty());
    assert!(matches!(mouse.pending_command, CommandMode::HelixBomb));
    mouse.pending_command = CommandMode::Normal;
    input::targeted_order(&mut battle, &mut mouse, Vec2::from_array(position), None);
    deliver(&mut audio, &mut mouse);
    assert_eq!(audio.requests.len(), 2);
}

fn deliver(audio: &mut SourceAudio, mouse: &mut input::Mouse) {
    for response in mouse.voices.drain(..) {
        audio.voice(&response);
    }
}
