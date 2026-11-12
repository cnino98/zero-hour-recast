//! `SalvageCrateCollide` and `Object::onVeterancyLevelChanged` audio callbacks.
use super::{
    SourceAudio,
    data::{ST_PLAYER, ST_SHROUDED},
};
use crate::skirmish_data::{definition, field, scalar};
use bevy::prelude::Vec3;
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::salvage::Reward;
use std::error::Error;

pub(super) fn sounds(assets: &VirtualAssets) -> Result<[String; 3], Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/miscaudio.ini")?)?];
    let lines = &definition(&files, "MiscAudio", "")?.lines;
    let sound = |key| -> Result<String, Box<dyn Error>> {
        Ok(field(lines, 0, key)
            .map(scalar)
            .transpose()?
            .unwrap_or_default()
            .into())
    };
    Ok([
        sound("CrateSalvage")?,
        sound("CrateMoney")?,
        sound("UnitPromoted")?,
    ])
}
impl SourceAudio {
    pub fn salvage(&mut self, reward: &Reward, object: usize, position: [f32; 3]) {
        let index = match reward {
            Reward::Weapon => 0,
            Reward::Money(_) => 1,
            // The experience callback owns this sound and only plays it when
            // the reward actually crosses a veterancy threshold.
            Reward::Level => return,
        };
        let name = self.salvage[index].clone();
        let point = Vec3::new(position[0], position[2], -position[1]);
        let _ = self.queue(&name, Some(object), Some(point), None, false);
    }

    pub fn promoted(
        &mut self,
        object: usize,
        position: [f32; 3],
        owner: usize,
        viewed: Option<usize>,
        clear: bool,
    ) {
        let name = self.salvage[2].clone();
        let Some(event) = self.events.get(&name) else {
            return;
        };
        // AudioManager::shouldPlayLocally precedes filename/play-info RNG.
        if event.restrictions & ST_PLAYER != 0 && viewed != Some(owner) {
            return;
        }
        let hidden =
            event.world && event.restrictions & ST_SHROUDED != 0 && event.priority != 4 && !clear;
        let point = Vec3::new(position[0], position[2], -position[1]);
        let requests = self.requests.len();
        let _ = self.queue(&name, Some(object), Some(point), None, false);
        // SoundManager::canPlayNow applies shroud after sample/pitch selection.
        if hidden {
            self.requests.truncate(requests);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salvage_uses_source_world_units_for_distance_and_request_position() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.salvage[0] = "click".into();
        let event = audio.events.get_mut("click").expect("fixture event");
        event.world = true;
        event.min_range = 0.0;
        event.max_range = 100.0;
        audio.listener = Vec3::new(1000.0, 20.0, -2000.0);
        audio.salvage(&Reward::Weapon, 12, [1000.0, 2000.0, 20.0]);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].object, Some(12));
        assert_eq!(audio.requests[0].position, Some(audio.listener));
        assert!(audio.volume(&audio.events["click"], 1.0, audio.requests[0].position) > 0.0);
        audio.requests.clear();
        audio.salvage(&Reward::Weapon, 12, [1200.0, 2000.0, 20.0]);
        assert_eq!(audio.requests.len(), 1);
        // Queue retains requests; the source distance gate runs during playback.
        assert_eq!(
            audio
                .volume(&audio.events["click"], 1.0, audio.requests[0].position)
                .to_bits(),
            0.0_f32.to_bits()
        );
    }

    #[test]
    fn experience_crate_does_not_play_promotion_without_the_level_callback() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.salvage[2] = "click".into();
        audio.salvage(&Reward::Level, 12, [100.0, 200.0, 20.0]);
        assert!(audio.requests.is_empty());
        audio.promoted(12, [100.0, 200.0, 20.0], 0, Some(0), true);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].object, Some(12));
        assert_eq!(
            audio.requests[0].position,
            Some(Vec3::new(100.0, 20.0, -200.0))
        );
    }

    #[test]
    fn promotion_sound_obeys_player_observer_and_clear_shroud_gates() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.salvage[2] = "click".into();
        let event = audio.events.get_mut("click").unwrap();
        event.world = true;
        event.restrictions = ST_PLAYER | ST_SHROUDED;
        for (owner, viewed, clear) in [(1, Some(0), true), (0, None, true), (0, Some(0), false)] {
            audio.promoted(12, [100.0, 200.0, 20.0], owner, viewed, clear);
            assert!(audio.requests.is_empty());
        }
        audio.promoted(12, [100.0, 200.0, 20.0], 1, Some(1), true);
        assert_eq!(audio.requests.len(), 1);
        audio.events.get_mut("click").unwrap().priority = 4;
        audio.promoted(12, [100.0, 200.0, 20.0], 1, Some(1), false);
        assert_eq!(audio.requests.len(), 2);
    }
}
