//! Original Inferno shell Drawable ambient one-shot, bound to its projectile.
use super::{State, data};
use crate::{
    skirmish_data::{definition, field, scalar},
    source_audio::SourceAudio,
};
use bevy::prelude::Vec3;
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{
    ProjectileFxEvent, ProjectileFxStage, Skirmish, UnitType, visibility::Status,
};
use std::{collections::BTreeMap, error::Error};

impl State {
    pub(in crate::source_audio) fn load(
        assets: &VirtualAssets,
        catalog: &[UnitType],
    ) -> Result<Self, Box<dyn Error>> {
        let files = [
            content::scan(assets.read("data/ini/weapon.ini")?)?,
            content::scan(assets.read("data/ini/object/weaponobjects.ini")?)?,
        ];
        let mut state = Self::new();
        for (kind, unit) in catalog.iter().enumerate() {
            for slot in 0..unit.weapon_template_count() {
                let Some(weapon) = unit
                    .weapon_template(slot)
                    .filter(|weapon| !weapon.name.is_empty())
                else {
                    continue;
                };
                let weapon = definition(&files, "Weapon", &weapon.name)?;
                let Some(name) = field(&weapon.lines, 0, "ProjectileObject")
                    .map(scalar)
                    .transpose()?
                else {
                    continue;
                };
                if !matches!(name, "InfernoTankShell" | "InfernoTankShellUpgraded") {
                    continue;
                }
                let object = definition(&files, "Object", name)?;
                let sound = scalar(
                    field(&object.lines, 0, "SoundAmbient")
                        .ok_or("Inferno shell lacks original ambient sound")?,
                )?;
                state.projectiles.insert((kind, slot), sound.into());
            }
        }
        Ok(state)
    }

    pub(in crate::source_audio) fn names<'a>(
        &'a self,
        catalog: &'a [UnitType],
    ) -> impl Iterator<Item = &'a str> {
        catalog
            .iter()
            .filter_map(|unit| unit.fire.as_ref())
            .map(|rules| rules.ambient.as_str())
            .chain(self.projectiles.values().map(String::as_str))
            .filter(|name| !name.is_empty())
    }

    pub(in crate::source_audio) fn validate_projectiles(
        &self,
        events: &BTreeMap<String, data::Event>,
    ) -> Result<(), Box<dyn Error>> {
        for name in self.projectiles.values() {
            let event = events
                .get(name)
                .ok_or("missing original projectile ambient sound")?;
            if !event.world
                || event.speech
                || event.control & data::LOOP != 0
                || event.restrictions & data::ST_PLAYER != 0
            {
                return Err(format!("untranslated projectile ambient controls for {name}").into());
            }
        }
        Ok(())
    }
}

impl SourceAudio {
    pub fn projectile(&mut self, game: &Skirmish, event: &ProjectileFxEvent, observer: usize) {
        if !matches!(event.stage, ProjectileFxStage::Created) {
            return;
        }
        let kind = event.weapon_type;
        let Some(name) = self
            .fires
            .projectiles
            .get(&(kind, event.weapon_index))
            .cloned()
        else {
            return;
        };
        let [x, y, z] = event.position;
        let clear = matches!(game.visibility_cell(observer, [x, y]), Status::Clear);
        self.projectile_ambient(event.projectile, &name, Vec3::new(x, z, -y), clear);
    }

    fn projectile_ambient(&mut self, id: u64, name: &str, position: Vec3, clear: bool) {
        let event = &self.events[name];
        // Drawable constructor tries one-shot ambient only once. Its update
        // retries only permanent loops, even if range/shroud/output rejected it.
        if event.priority != 4
            && position.distance_squared(self.listener) >= event.max_range * event.max_range
        {
            return;
        }
        let hidden = event.priority != 4 && event.restrictions & data::ST_SHROUDED != 0 && !clear;
        let count = self.requests.len();
        let _ = self.queue(name, None, Some(position), None, false);
        if hidden {
            self.requests.truncate(count);
        }
        if let Some(request) = self.requests.get_mut(count) {
            request.projectile = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_sound_range_precedes_rng_and_hidden_admission_follows_rng() {
        let mut hidden = crate::source_audio::speech_tests::audio();
        let mut audible = crate::source_audio::speech_tests::audio();
        for audio in [&mut hidden, &mut audible] {
            let event = audio.events.get_mut("click").unwrap();
            event.world = true;
            event.max_range = 100.0;
            event.restrictions = data::ST_SHROUDED;
            event.pitch = [0.8, 1.2];
        }
        hidden.projectile_ambient(4, "click", Vec3::new(100.0, 0.0, 0.0), true);
        assert!(hidden.requests.is_empty());
        hidden.projectile_ambient(5, "click", Vec3::ZERO, false);
        audible.projectile_ambient(5, "click", Vec3::ZERO, true);
        assert!(hidden.requests.is_empty());
        assert_eq!(audible.requests[0].projectile, Some(5));
        assert_eq!(audible.requests[0].object, None);
        assert_eq!(
            hidden.random.lock().unwrap().index(10_000),
            audible.random.lock().unwrap().index(10_000)
        );
    }
}
