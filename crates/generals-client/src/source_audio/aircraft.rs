//! `Drawable` ambient loops for admitted jets and Helix; source range/retry lifecycle.
use super::{SourceAudio, data};
use crate::skirmish_data::{definition, field, scalar};
use bevy::prelude::Vec3;
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{Skirmish, UnitType, visibility::Status};
use std::{
    collections::BTreeMap,
    error::Error,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct State {
    sounds: Vec<[String; 4]>,
    active: BTreeMap<usize, Playing>,
}

struct Playing {
    category: usize,
    handle: Weak<()>,
    stop: Arc<AtomicBool>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            sounds: Vec::new(),
            active: BTreeMap::new(),
        }
    }

    pub(super) fn load(
        assets: &VirtualAssets,
        catalog: &[UnitType],
    ) -> Result<Self, Box<dyn Error>> {
        let selected: Vec<_> = catalog
            .iter()
            .filter(|unit| {
                unit.aircraft.is_some() || unit.helicopter_death_rules().next().is_some()
            })
            .map(|unit| unit.name.as_str())
            .collect();
        let (files, _) = crate::skirmish_data::source_files::effective(assets, &selected)?;
        let mut state = Self::new();
        for unit in catalog {
            let mut names = std::array::from_fn(|_| String::new());
            if unit.aircraft.is_some() || unit.helicopter_death_rules().next().is_some() {
                let object = definition(&files, "Object", &unit.name)?;
                for (index, key) in [
                    "SoundAmbient",
                    "SoundAmbientDamaged",
                    "SoundAmbientReallyDamaged",
                    "SoundAmbientRubble",
                ]
                .into_iter()
                .enumerate()
                {
                    names[index] = field(&object.lines, 0, key)
                        .map(scalar)
                        .transpose()?
                        .unwrap_or_default()
                        .into();
                }
                // Drawable falls back only for living damage states. NoSound is
                // an explicit binding and must not be replaced with pristine.
                for index in [1, 2] {
                    if names[index].is_empty() {
                        let (pristine, damaged) = names.split_at_mut(index);
                        damaged[0].clone_from(&pristine[0]);
                    }
                }
            }
            state.sounds.push(names);
        }
        Ok(state)
    }

    pub(super) fn names(&self) -> impl Iterator<Item = &str> {
        self.sounds.iter().flatten().map(String::as_str)
    }

    pub(super) fn validate(
        &self,
        events: &BTreeMap<String, data::Event>,
    ) -> Result<(), Box<dyn Error>> {
        for name in self
            .names()
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NoSound"))
        {
            let event = events.get(name).ok_or("missing aircraft ambient sound")?;
            if !event.world
                || event.speech
                || event.control & data::LOOP == 0
                || event.loop_count != 0
                || event.restrictions & data::ST_PLAYER != 0
            {
                return Err(format!("untranslated aircraft ambient sound {name}").into());
            }
        }
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        self.retain(|_, _| false);
    }

    fn retain(&mut self, keep: impl Fn(usize, usize) -> bool) {
        self.active.retain(|&id, entry| {
            let retain = keep(id, entry.category);
            if !retain {
                entry.stop.store(true, Ordering::Release);
            }
            retain
        });
    }
}

fn category(game: &Skirmish, id: usize) -> usize {
    let actor = &game.entities[id];
    let thresholds = game.catalog[actor.type_index].attachment_thresholds;
    if !actor.alive() {
        3
    } else if actor.health <= actor.max_health * thresholds[1] {
        2
    } else {
        usize::from(actor.health <= actor.max_health * thresholds[0])
    }
}

fn ambient_enabled(game: &Skirmish, id: usize) -> bool {
    let actor = &game.entities[id];
    // Ordinary helicopter Drawable ambient has no JetAI engine toggle. Its rubble
    // category has no pristine fallback; that category change retires the
    // living loop stopped by HelicopterSlowDeath without inventing a flag.
    actor.aircraft_engine_on()
        || game.catalog[actor.type_index]
            .helicopter_death_rules()
            .next()
            .is_some()
}

impl SourceAudio {
    pub(super) fn stop_aircraft_ambient(&mut self, id: usize) {
        self.aircraft.retain(|other, _| other != id);
    }

    pub(super) fn update_aircraft(&mut self, game: &Skirmish, observer: usize) {
        self.aircraft.retain(|id, previous| {
            game.entities.get(id).is_some_and(|actor| {
                !actor.masked() && ambient_enabled(game, id) && category(game, id) == previous
            })
        });
        for (id, actor) in game.entities.iter().enumerate() {
            if actor.masked() || !ambient_enabled(game, id) {
                continue;
            }
            let category = category(game, id);
            let name = self.aircraft.sounds[actor.type_index][category].clone();
            let [x, y, z] = game.object_position(id);
            let clear = matches!(game.visibility_cell(observer, [x, y]), Status::Clear);
            self.aircraft_sound(id, category, &name, Vec3::new(x, z, -y), clear);
        }
    }

    fn aircraft_sound(
        &mut self,
        id: usize,
        category: usize,
        name: &str,
        position: Vec3,
        clear: bool,
    ) {
        let Some(event) = self.events.get(name) else {
            return;
        };
        // Drawable checks distance before constructing an audio request. Shroud
        // is tested later by SoundManager, after native sample/pitch RNG.
        if event.priority != 4
            && position.distance_squared(self.listener) >= event.max_range * event.max_range
        {
            self.aircraft.retain(|other, _| other != id);
            return;
        }
        if self
            .aircraft
            .active
            .get(&id)
            .is_some_and(|entry| entry.handle.strong_count() > 0)
        {
            return;
        }
        let hidden = event.priority != 4 && event.restrictions & data::ST_SHROUDED != 0 && !clear;
        let count = self.requests.len();
        let _ = self.queue(name, Some(id), Some(position), None, false);
        if hidden {
            self.requests.truncate(count);
        }
        if let Some(request) = self.requests.get_mut(count) {
            let handle = Arc::new(());
            self.aircraft.active.insert(
                id,
                Playing {
                    category,
                    handle: Arc::downgrade(&handle),
                    stop: request
                        .looping
                        .as_ref()
                        .expect("validated aircraft ambient loop")
                        .stop
                        .clone(),
                },
            );
            request.handle = Some(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_helix_ambient_starts_once_and_stops_on_actual_aa_death() {
        use crate::{helix_verification, skirmish_verification::advance_until};
        use rodio::Source;
        use std::path::Path;

        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = helix_verification::prepare(game).unwrap();
        let id = setup.helicopters[0];
        let index = game.entities[id].type_index;
        assert!(game.catalog[index].aircraft.is_none());
        assert!(!game.entities[id].aircraft_engine_on());
        let mut audio = super::super::load(&assets, &game.catalog, Path::new("")).unwrap();
        for name in [
            "vhello1a", "vhello2a", "vhello2b", "vhello2c", "vhello2d", "vhello3a",
        ] {
            let original = audio.samples[name].as_ref().unwrap();
            assert_eq!(original.channels().get(), 1);
            assert_eq!(
                original.sample_rate().get(),
                if name == "vhello2d" { 22050 } else { 44100 }
            );
        }
        check_original_helix_mixed_duration(&audio);
        assert_eq!(audio.aircraft.sounds[index][0], "HelixAmbientLoop");
        assert_eq!(audio.aircraft.sounds[index][1], "HelixAmbientLoop");
        assert_eq!(audio.aircraft.sounds[index][2], "HelixAmbientLoop");
        assert!(audio.aircraft.sounds[index][3].is_empty());
        let [x, y, z] = game.object_position(id);
        audio.listener = Vec3::new(x, z, -y);
        audio.update_aircraft(game, 0);
        let request = audio
            .requests
            .iter()
            .find(|request| request.object == Some(id) && request.event == "HelixAmbientLoop")
            .unwrap();
        assert!(!request.sample_names.is_empty());
        assert!(
            request
                .samples
                .iter()
                .all(|sample| sample.clone().next().is_some())
        );
        let stop = request.looping.as_ref().unwrap().stop.clone();
        assert!(!stop.load(Ordering::Acquire));
        let count = audio.requests.len();
        audio.update_aircraft(game, 0);
        assert_eq!(audio.requests.len(), count);
        helix_verification::crash::approach(game, &setup).unwrap();
        advance_until(game, 9000, "paid Patriot kills ambient Helix", |game| {
            game.entities[id].helicopter_corpse()
        })
        .unwrap();
        assert!(
            game.entities[id]
                .last_attacker()
                .is_some_and(|source| setup.defenders.contains(&source))
        );
        assert!(game.entities[id].helicopter_corpse());
        assert!(!game.entities[id].masked());
        audio.update_aircraft(game, 0);
        assert!(stop.load(Ordering::Acquire));
        assert!(!audio.aircraft.active.contains_key(&id));
        audio.update_aircraft(game, 0);
        assert_eq!(
            audio
                .requests
                .iter()
                .filter(|request| request.object == Some(id) && request.event == "HelixAmbientLoop")
                .count(),
            1
        );
        audio.aircraft.reset();
        assert!(audio.requests.iter().all(|request| {
            request
                .looping
                .as_ref()
                .unwrap()
                .stop
                .load(Ordering::Acquire)
        }));
    }

    fn check_original_helix_mixed_duration(audio: &SourceAudio) {
        use rodio::Source;
        use std::{sync::Mutex, time::Duration};
        let attack = audio.samples["vhello1a"].as_ref().unwrap().clone();
        let body = audio.samples["vhello2d"].as_ref().unwrap().clone();
        let attack_frames = attack.clone().count();
        let body_frames = body.clone().count();
        assert_eq!(body_frames, 42_131);
        let template = super::super::looping::Template {
            attack: Some(attack),
            bodies: vec![Some(body)],
            decay: None,
            previous: 0,
            delay: [0.0; 2],
            pending: Arc::new(Mutex::new(None)),
            random: Arc::new(Mutex::new(super::super::random::Random::new(5))),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let mut decoder = template.decoder();
        assert_eq!(decoder.by_ref().take(attack_frames).count(), attack_frames);
        assert!(decoder.next().is_some());
        template.stop.store(true, Ordering::Release);
        let output_frames = 1 + decoder.count();
        // Pinned Rodio limits input spans to 32768 samples. Each 2x-upsampled
        // span drains its last input frame once: two output frames are omitted.
        assert_eq!(
            output_frames,
            body_frames * 2 - body_frames.div_ceil(32_768)
        );
        let actual =
            Duration::from_secs_f64(f64::from(u32::try_from(output_frames).unwrap()) / 44_100.0);
        let authored = template.bodies[0]
            .as_ref()
            .unwrap()
            .total_duration()
            .unwrap();
        assert!(actual.abs_diff(authored) <= Duration::from_micros(46));
    }

    #[test]
    fn ambient_loop_failed_admission_retries_and_range_or_engine_stop_retires_it() {
        let mut audio = super::super::speech_tests::audio();
        let event = audio.events.get_mut("click").unwrap();
        event.control = data::LOOP | data::ALL | data::RANDOM;
        event.world = true;
        event.max_range = 100.0;
        event.restrictions = data::ST_SHROUDED;
        audio.aircraft_sound(7, 0, "click", Vec3::ZERO, false);
        assert!(audio.requests.is_empty());
        audio.aircraft_sound(7, 0, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        audio.aircraft_sound(7, 0, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        audio.requests.clear();
        audio.aircraft_sound(7, 0, "click", Vec3::ZERO, true);
        assert_eq!(audio.requests.len(), 1);
        let stop = audio.requests[0].looping.as_ref().unwrap().stop.clone();
        audio.aircraft_sound(7, 0, "click", Vec3::new(100.0, 0.0, 0.0), true);
        assert!(stop.load(Ordering::Acquire));
        audio.requests.clear();
        audio.aircraft_sound(7, 0, "click", Vec3::ZERO, true);
        audio.aircraft.reset();
        assert!(
            audio.requests[0]
                .looping
                .as_ref()
                .unwrap()
                .stop
                .load(Ordering::Acquire)
        );
    }
}
