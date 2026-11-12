//! Original container effect callbacks, independent from passenger command voices.
use super::{Match, SourceAudio, data::ST_SHROUDED};
use crate::skirmish_data::{blocks, field, scalar};
use bevy::prelude::{ResMut, Vec3};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::{
    containment::{Sound, SoundHook},
    visibility::Status,
};
use std::error::Error;

pub(super) fn bindings(
    represented: bool,
    lines: &[ContentLine],
) -> Result<[String; 5], Box<dyn Error>> {
    let mut sounds = std::array::from_fn(|_| String::new());
    for (sound, key) in
        sounds[2..]
            .iter_mut()
            .zip(["SoundEnter", "SoundExit", "SoundFallingFromPlane"])
    {
        *sound = field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into();
    }
    if !represented {
        return Ok(sounds);
    }
    for module in blocks(lines, 0, "Behavior")? {
        if !module[0]
            .tokens
            .get(1)
            .is_some_and(|name| name.ends_with("Contain"))
        {
            continue;
        }
        for (sound, key) in sounds[..2].iter_mut().zip(["EnterSound", "ExitSound"]) {
            *sound = field(module, 1, key)
                .map(scalar)
                .transpose()?
                .unwrap_or_default()
                .into();
        }
    }
    Ok(sounds)
}

pub fn record(mut battle: ResMut<Match>, mut audio: ResMut<SourceAudio>) {
    if !battle.started || battle.paused {
        return;
    }
    for event in std::mem::take(&mut battle.simulation.presentation.container_sounds) {
        let index = match event.hook {
            SoundHook::EnterSound => 0,
            SoundHook::ExitSound => 1,
            SoundHook::SoundEnter => 2,
            SoundHook::SoundExit => 3,
            SoundHook::SoundFalling => 4,
        };
        let name = audio.units[event.type_index].contain[index].clone();
        let [x, y, _] = event.position;
        let clear = matches!(
            battle
                .simulation
                .visibility_cell(battle.observer_player.unwrap_or(0), [x, y]),
            Status::Clear
        );
        play(&mut audio, &name, &event, clear);
    }
}

fn play(audio: &mut SourceAudio, name: &str, callback: &Sound, clear: bool) {
    let hidden = audio.events.get(name).is_some_and(|event| {
        event.world && event.restrictions & ST_SHROUDED != 0 && event.priority != 4 && !clear
    });
    let requests = audio.requests.len();
    let [x, y, z] = callback.position;
    let _ = audio.queue(
        name,
        Some(callback.object),
        Some(Vec3::new(x, z, -y)),
        None,
        false,
    );
    // SoundManager::canPlayNow tests shroud after filename/play-info selection.
    if hidden {
        audio.requests.truncate(requests);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_effect_bindings_keep_native_empty_defaults_and_distinct_directions() {
        let file = generals_assets::content::scan(generals_assets::virtual_assets::Asset {
            path: "container.ini".into(),
            origin: "source audio fixture".into(),
            bytes: b"Object Transport\nSoundEnter = RiderEnter\nSoundExit = RiderExit\nSoundFallingFromPlane = RiderFall\nBehavior = TransportContain ModuleTag_Contain\n  EnterSound = LoadTroops\n  ExitSound = UnloadTroops\nEnd\nEnd\n".to_vec(),
        }).unwrap();
        let lines = &file.declarations[0].lines;
        assert_eq!(
            bindings(true, lines).unwrap(),
            [
                "LoadTroops",
                "UnloadTroops",
                "RiderEnter",
                "RiderExit",
                "RiderFall"
            ]
        );
        assert_eq!(
            bindings(false, lines).unwrap(),
            ["", "", "RiderEnter", "RiderExit", "RiderFall"]
        );
        assert_eq!(bindings(true, &[]).unwrap(), ["", "", "", "", ""]);
    }

    #[test]
    fn container_effects_keep_native_position_and_cull_fog_after_audio_random_selection() {
        let mut audio = crate::source_audio::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.world = true;
        event.restrictions = ST_SHROUDED;
        event.pitch = [0.9, 1.1];
        let callback = Sound {
            frame: 1,
            object: 9,
            type_index: 0,
            hook: SoundHook::EnterSound,
            position: [100.0, 200.0, 30.0],
        };
        play(&mut audio, "click", &callback, false);
        assert!(audio.requests.is_empty());
        play(&mut audio, "click", &callback, true);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(
            audio.requests[0].position,
            Some(Vec3::new(100.0, 30.0, -200.0))
        );
        assert_eq!(audio.requests[0].object, Some(9));
        let mut expected = super::super::random::Random::new(74);
        expected.real(0.9, 1.1);
        let second_pitch = expected.real(0.9, 1.1);
        assert_eq!(audio.requests[0].pitch.to_bits(), second_pitch.to_bits());
        audio.events.get_mut("click").unwrap().priority = 4;
        play(&mut audio, "click", &callback, false);
        assert_eq!(audio.requests.len(), 2);
    }
}
