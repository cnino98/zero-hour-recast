//! Authored finite hack and stealth callbacks; native loop handles end with preparation.
use super::{Match, SourceAudio};
use crate::{
    playtest_view,
    skirmish_data::{blocks, field, scalar},
};
use generals_assets::content::ContentLine;
use generals_sim::skirmish::{UnitType, stealth};
use std::error::Error;

#[derive(bevy::ecs::system::SystemParam)]
pub struct Inspection<'w, 's> {
    audio: bevy::prelude::Res<'w, SourceAudio>,
    sinks: bevy::prelude::Query<
        'w,
        's,
        (
            &'static super::playback::Playing,
            &'static bevy::prelude::AudioSink,
        ),
    >,
}

impl Inspection<'_, '_> {
    pub fn sample(self, actor: usize, unit: usize, ability: usize) -> ([bool; 5], bool) {
        use bevy::audio::AudioSinkPlayback;
        let voice = [
            super::Voice::LotusCaptureMode,
            super::Voice::LotusVehicleMode,
            super::Voice::LotusCashMode,
        ][ability];
        let mode = &self.audio.units[unit].voices[voice.index()].name;
        let binding = &self.audio.units[unit].lotus[ability];
        let sounds = [
            (None, mode),
            (Some(actor), &binding[0]),
            (Some(actor), &binding[4]),
            (Some(actor), &binding[5]),
            (Some(actor), &binding[3]),
        ];
        let heard = sounds.map(|(object, name)| {
            self.sinks.iter().any(|(playing, sink)| {
                playing.object == object
                    && playing.event == *name
                    && sink.position().as_secs_f64() > 0.01
            })
        });
        let finished = !self.sinks.iter().any(|(playing, sink)| {
            playing.object.is_none() && playing.event == *mode && !sink.empty()
        });
        (heard, finished)
    }
}

pub(super) fn stealth_bindings(lines: &[ContentLine]) -> Result<[String; 2], Box<dyn Error>> {
    let name = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    Ok([name("SoundStealthOn")?, name("SoundStealthOff")?])
}

pub(super) fn detector_bindings(unit: &UnitType) -> Vec<[String; 2]> {
    unit.behavior
        .detectors
        .iter()
        .map(|rules| {
            rules.feedback.as_ref().map_or_else(
                || [String::new(), String::new()],
                |feedback| feedback.sounds.clone(),
            )
        })
        .collect()
}

pub(super) fn bindings(
    unit: &UnitType,
    lines: &[ContentLine],
) -> Result<Vec<[String; 6]>, Box<dyn Error>> {
    if !unit.behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding)) {
        return Ok(Vec::new());
    }
    let modules = blocks(lines, 0, "Behavior")?;
    let sounds = blocks(lines, 0, "UnitSpecificSounds")?;
    let mut result = Vec::new();
    for (power, complete) in [
        (
            "SpecialAbilityBlackLotusCaptureBuilding",
            "VoiceCaptureBuildingComplete",
        ),
        (
            "SpecialAbilityBlackLotusDisableVehicleHack",
            "VoiceDisableVehicleComplete",
        ),
        (
            "SpecialAbilityBlackLotusStealCashHack",
            "VoiceStealCashComplete",
        ),
    ] {
        let mut names = std::array::from_fn(|_| String::new());
        for module in &modules {
            if field(module, 1, "SpecialPowerTemplate")
                .map(scalar)
                .transpose()?
                != Some(power)
            {
                continue;
            }
            for (index, key) in [
                (0, "InitiateSound"),
                (1, "UnpackSound"),
                (2, "PackSound"),
                (4, "PrepSoundLoop"),
                (5, "TriggerSound"),
            ] {
                if let Some(line) = field(module, 1, key) {
                    names[index] = scalar(line)?.into();
                }
            }
        }
        for module in &sounds {
            if let Some(line) = field(module, 1, complete) {
                names[3] = scalar(line)?.into();
            }
        }
        result.push(names);
    }
    Ok(result)
}

impl SourceAudio {
    pub(super) fn update_stealth_audio(&mut self, battle: &Match) {
        let game = &battle.simulation;
        if self.capture.stealth_frame == Some(game.tick) {
            return;
        }
        self.capture.stealth_frame = Some(game.tick);
        for event in &game.presentation.stealth_events {
            if let stealth::Event::Scan {
                detector,
                module,
                found,
                feedback: Some(scan),
            } = event
                && scan.visible_players & 1 != 0
            {
                let name =
                    self.units[scan.type_index].detectors[*module][usize::from(*found)].clone();
                let [x, y, z] = scan.position;
                let _ = self.queue(
                    &name,
                    Some(*detector),
                    Some(bevy::prelude::Vec3::new(x, z, -y)),
                    None,
                    false,
                );
            }
            let stealth::Event::Sound { object, on } = *event else {
                continue;
            };
            let actor = &game.entities[object];
            let name = self.units[actor.type_index].stealth[usize::from(!on)].clone();
            let point = playtest_view::unit_position(game, object) * 10.0;
            let _ = self.queue(&name, Some(object), Some(point), None, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{infiltrator_verification, input, source_audio};
    use bevy::prelude::Vec2;
    use std::path::Path;

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_lotus_vehicle_hack_plays_the_selected_modules_command_voice() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let setup = infiltrator_verification::prepare(&mut battle.simulation).unwrap();
        battle.select_objects(&[setup.actor], true);
        let mut mouse = input::load(&assets, Path::new(""), &battle.simulation.catalog).unwrap();
        mouse.pending_command = input::CommandMode::Hack(1);
        let target = Vec2::from_array(battle.simulation.entities[setup.vehicle].position);
        input::targeted_order(&mut battle, &mut mouse, target, Some(setup.actor));
        assert!(mouse.voices.is_empty());
        input::targeted_order(&mut battle, &mut mouse, target, Some(setup.vehicle));
        assert!(matches!(mouse.pending_command, input::CommandMode::Normal));
        battle.clear_drawable_selection();
        input::commands::step(&mut battle, &mut mouse);

        let mut audio =
            source_audio::load(&assets, &battle.simulation.catalog, Path::new("")).unwrap();
        for request in mouse.voices {
            audio.voice(&request);
        }
        assert_eq!(audio.requests.len(), 1);
        let request = &audio.requests[0];
        assert_eq!(request.object, Some(setup.actor));
        assert_eq!(request.event, "BlackLotusVoiceHackVehicle");
        assert!(!request.samples.is_empty());
        assert!(
            request
                .samples
                .iter()
                .all(|sample| sample.clone().next().is_some())
        );
    }
}
