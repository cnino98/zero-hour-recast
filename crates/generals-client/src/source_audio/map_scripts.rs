//! `ScriptActions::doPlaySoundEffect` and `AudioManager` volume overrides, SH 2bfaea60.
use super::{SourceAudio, data};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::map_scripts::{Action, AudioAction, Script};
use std::error::Error;

impl SourceAudio {
    pub fn load_map_sounds(
        &mut self,
        assets: &VirtualAssets,
        scripts: &[Script],
    ) -> Result<(), Box<dyn Error>> {
        let definitions = data::definitions(assets)?;
        for action in scripts
            .iter()
            .flat_map(|script| script.actions.iter().chain(&script.false_actions))
        {
            let (Action::Sound(name) | Action::AudioVolume(name, _)) = action else {
                continue;
            };
            if name.is_empty() || self.events.contains_key(name) {
                continue;
            }
            let event = definitions
                .get(name)
                .ok_or_else(|| format!("Missing map-script AudioEvent {name}"))?;
            data::load_event(assets, &self.settings, event, &mut self.samples)?;
            self.events.insert(name.clone(), event.clone());
        }
        Ok(())
    }

    pub fn map_script_actions(&mut self, actions: &[AudioAction]) {
        for action in actions {
            match action {
                AudioAction::Play(name) => {
                    self.queue(name, None, None, None, false);
                }
                AudioAction::OverrideVolume(name, volume) => {
                    if name.is_empty() {
                        for event in self.events.values_mut() {
                            event.volume_override = None;
                        }
                    } else if let Some(event) = self.events.get_mut(name) {
                        if volume.to_bits() == (-1.0_f32).to_bits() {
                            event.volume_override = None;
                        } else {
                            event.volume_override = Some(*volume);
                            self.volume_changes.push((name.clone(), *volume));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_override_preserves_volume_shift_and_random_draws() {
        let mut audio = super::super::speech_tests::audio();
        audio.events.get_mut("click").unwrap().volume = 0.3;
        audio.events.get_mut("click").unwrap().volume_shift = -0.2;
        let mut expected = super::super::random::Random::new(74);
        // Literal filename, equal delay and pitch endpoints consume no draws.
        // AudioEventRTS retains the sampled shift when setVolume changes its base.
        let shift = expected.real(0.8, 1.0);
        let next = expected.real(0.0, 1.0);
        audio.map_script_actions(&[
            AudioAction::OverrideVolume("click".into(), 2.0),
            AudioAction::Play("click".into()),
        ]);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].volume.to_bits(), (2.0 * shift).to_bits());
        assert_eq!(audio.requests[0].volume_shift.to_bits(), shift.to_bits());
        assert_eq!(
            audio.random.lock().unwrap().real(0.0, 1.0).to_bits(),
            next.to_bits()
        );
        assert_eq!(audio.volume_changes, [("click".into(), 2.0)]);
        audio.map_script_actions(&[AudioAction::OverrideVolume("click".into(), -1.0)]);
        assert!(audio.events["click"].volume_override.is_none());
        // Removing the override only affects future plays; currently playing
        // samples keep the last volume explicitly applied by Miles.
        assert_eq!(audio.volume_changes.len(), 1);
    }
}
