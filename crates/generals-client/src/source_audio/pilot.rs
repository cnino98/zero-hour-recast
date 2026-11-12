//! `Object::setDisabledUntil` plays the original pilot sound before neutral defection.
use super::SourceAudio;
use crate::skirmish_data::{definition, field, scalar};
use bevy::prelude::Vec3;
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::error::Error;

pub(super) fn sound(assets: &VirtualAssets) -> Result<String, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
    Ok(field(
        &definition(&files, "MiscAudio", "")?.lines,
        0,
        "SplatterVehiclePilotsBrain",
    )
    .map(scalar)
    .transpose()?
    .unwrap_or_default()
    .into())
}

impl SourceAudio {
    pub fn pilot(&mut self, event: &generals_sim::skirmish::crew::PilotEvent) {
        let name = self.pilot.clone();
        let [x, y, z] = event.position;
        // The native event receives only the pre-defection world position.
        let _ = self.queue(&name, None, Some(Vec3::new(x, z, -y)), None, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilot_callback_keeps_event_position_without_following_the_neutral_vehicle() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.pilot = "click".into();
        audio.pilot(&generals_sim::skirmish::crew::PilotEvent {
            frame: 12,
            object: 4,
            position: [100.0, 200.0, 30.0],
        });
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].object, None);
        assert_eq!(
            audio.requests[0].position,
            Some(Vec3::new(100.0, 30.0, -200.0))
        );
    }
}
