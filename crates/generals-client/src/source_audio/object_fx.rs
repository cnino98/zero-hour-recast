//! `SoundFXNugget::doFXObj` uses the callback owner's player index and position.
use super::{
    SourceAudio,
    data::{ST_PLAYER, ST_SHROUDED},
};
use bevy::prelude::Vec3;

impl SourceAudio {
    pub fn load_position_sounds(
        &mut self,
        assets: &generals_assets::virtual_assets::VirtualAssets,
        effects: &std::collections::BTreeSet<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let definitions = super::data::definitions(assets)?;
        for name in crate::particles::position_sounds(assets, effects)? {
            if name.is_empty()
                || name.eq_ignore_ascii_case("NoSound")
                || self.events.contains_key(&name)
            {
                continue;
            }
            let event = definitions
                .get(&name)
                .ok_or_else(|| format!("Missing original AudioEvent {name}"))?;
            super::data::load_event(assets, &self.settings, event, &mut self.samples)?;
            self.events.insert(name, event.clone());
        }
        Ok(())
    }

    pub fn weapon_event(
        &mut self,
        game: &generals_sim::skirmish::Skirmish,
        event: &generals_sim::skirmish::WeaponFxEvent,
        viewer: usize,
    ) {
        use generals_sim::skirmish::WeaponFxEvent;
        match event {
            WeaponFxEvent::FiringSound {
                object,
                weapon,
                position,
                clear_cells,
            } => {
                self.firing_sound(
                    game,
                    *object,
                    *weapon,
                    *position,
                    clear_cells & (1 << viewer) != 0,
                );
            }
            WeaponFxEvent::StopFiringSound(object) => self.stop_firing(*object),
            WeaponFxEvent::RapidFire { object, position } => {
                self.rapid_fire(game, *object, *position, viewer);
            }
            WeaponFxEvent::AbilityFeedback(index) => {
                self.ability_feedback(&game.presentation.ability_feedback[*index], viewer);
            }
            WeaponFxEvent::ObjectSound(index) => {
                self.object_sound(&game.presentation.object_sounds[*index], viewer);
            }
            WeaponFxEvent::SuperweaponEva(index) => {
                let event = &game.presentation.superweapon_eva[*index];
                let relation = if viewer == event.owner {
                    0
                } else if game.enemies(viewer, event.owner) {
                    2
                } else {
                    1
                };
                self.launch_superweapon(relation, &event.power);
            }
            WeaponFxEvent::Pilot(index) => self.pilot(&game.presentation.pilot_events[*index]),
            WeaponFxEvent::Projectile(event) => self.projectile(game, event, viewer),
            WeaponFxEvent::ObjectFx(_)
            | WeaponFxEvent::BodyParticles(_)
            | WeaponFxEvent::BodyParticlesClear(_)
            | WeaponFxEvent::BoneFx(_)
            | WeaponFxEvent::TerrainMovement(_)
            | WeaponFxEvent::Chinook(_)
            | WeaponFxEvent::LaserInit(_)
            | WeaponFxEvent::DrawableCreated { .. }
            | WeaponFxEvent::DrawableDestroyed(_)
            | WeaponFxEvent::AttachedParticle(_)
            | WeaponFxEvent::SpectreStrafe(_)
            | WeaponFxEvent::ParticleUplinkScorch(_)
            | WeaponFxEvent::EmpSparks(_)
            | WeaponFxEvent::NeutronScorch(_)
            | WeaponFxEvent::DestroyAttachedParticles(_)
            | WeaponFxEvent::Firestorm(_)
            | WeaponFxEvent::SlaveWeld(_)
            | WeaponFxEvent::Shot(_)
            | WeaponFxEvent::Laser(_)
            | WeaponFxEvent::Structure(_) => {}
        }
    }

    pub fn position_fx(&mut self, event: &generals_sim::skirmish::object_fx::Event, viewer: usize) {
        // FXList::doFXPos tests only the position's cell. SoundFXNugget then
        // supplies a position without the object callback's player index.
        if event.clear_cells & (1 << viewer) == 0 {
            return;
        }
        let names = self.object_fx[&event.effect].clone();
        let point = Vec3::new(event.position[0], event.position[2], -event.position[1]);
        for name in names {
            let _ = self.queue(&name, None, Some(point), None, false);
        }
    }

    #[cfg(test)]
    pub(super) fn object_fx(
        &mut self,
        event: &generals_sim::skirmish::object_fx::Event,
        viewer: usize,
    ) {
        if !event.oriented_object {
            self.position_fx(event, viewer);
            return;
        }
        if event.visible_players & (1 << viewer) == 0 {
            return;
        }
        let names = self.object_fx[&event.effect].clone();
        for name in names {
            self.object_fx_sound(event, &name, viewer);
        }
    }

    pub(crate) fn object_fx_sound(
        &mut self,
        event: &generals_sim::skirmish::object_fx::Event,
        name: &str,
        viewer: usize,
    ) {
        let Some(sound) = self.events.get(name) else {
            return;
        };
        if sound.restrictions & ST_PLAYER != 0 && event.owner != viewer {
            return;
        }
        let hidden = sound.world
            && sound.restrictions & ST_SHROUDED != 0
            && sound.priority != 4
            && event.clear_cells & (1 << viewer) == 0;
        let point = Vec3::new(event.position[0], event.position[2], -event.position[1]);
        let count = self.requests.len();
        let _ = self.queue(name, None, Some(point), None, false);
        // SoundManager checks the position cell after filename/play-info RNG.
        if hidden {
            self.requests.truncate(count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bone_position_sound_does_not_inherit_object_owner_restriction() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.events.get_mut("click").unwrap().restrictions = ST_PLAYER;
        let event = generals_sim::skirmish::object_fx::Event {
            frame: 7,
            object: 9,
            type_index: 2,
            owner: 2,
            position: [10.0, 20.0, 30.0],
            basis: generals_sim::skirmish::physical_ocl::transform::yaw(0.0),
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 100.0,
            override_radius: 0.0,
            visible_players: 5,
            clear_cells: 5,
            effect: "bone".into(),
        };
        audio.object_fx_sound(&event, "click", 0);
        assert!(audio.requests.is_empty());
        audio.fx("click", [40.0, 50.0, 60.0]);
        audio.fx("click", [70.0, 80.0, 90.0]);
        audio.object_fx_sound(&event, "click", 2);
        assert_eq!(audio.requests.len(), 3);
        assert!(
            audio
                .requests
                .iter()
                .all(|request| request.object.is_none())
        );
        assert_eq!(
            audio
                .requests
                .iter()
                .map(|request| request.position)
                .collect::<Vec<_>>(),
            [
                Some(Vec3::new(40.0, 60.0, -50.0)),
                Some(Vec3::new(70.0, 90.0, -80.0)),
                Some(Vec3::new(10.0, 30.0, -20.0))
            ]
        );
    }

    #[test]
    fn object_fx_preserves_source_owner_gate_before_rng_and_world_position_without_object_binding()
    {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.events.get_mut("click").unwrap().restrictions = ST_PLAYER;
        let untouched = crate::source_audio::speech_tests::audio();
        audio
            .object_fx
            .insert("conversion".into(), vec!["click".into()]);
        let event = generals_sim::skirmish::object_fx::Event {
            frame: 7,
            object: 9,
            type_index: 2,
            owner: 2,
            position: [10.0, 20.0, 30.0],
            basis: generals_sim::skirmish::physical_ocl::transform::yaw(0.0),
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 5,
            clear_cells: 5,
            effect: "conversion".into(),
        };
        audio.object_fx(&event, 0);
        assert!(audio.requests.is_empty());
        assert_eq!(
            audio.random.lock().unwrap().index(10_000),
            untouched.random.lock().unwrap().index(10_000)
        );
        audio.object_fx(&event, 2);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].object, None);
        assert_eq!(
            audio.requests[0].position,
            Some(Vec3::new(10.0, 30.0, -20.0))
        );
    }

    #[test]
    fn partial_object_with_hidden_position_consumes_rng_but_does_not_play_shrouded_sound() {
        let mut hidden = crate::source_audio::speech_tests::audio();
        let mut visible = crate::source_audio::speech_tests::audio();
        for audio in [&mut hidden, &mut visible] {
            let sound = audio.events.get_mut("click").unwrap();
            sound.world = true;
            sound.restrictions = ST_SHROUDED;
            sound.pitch = [0.8, 1.2];
            audio.object_fx.insert("death".into(), vec!["click".into()]);
            audio.ui("click");
        }
        let mut event = generals_sim::skirmish::object_fx::Event {
            frame: 7,
            object: 9,
            type_index: 2,
            owner: 1,
            position: [10.0, 20.0, 30.0],
            basis: generals_sim::skirmish::physical_ocl::transform::yaw(0.0),
            oriented_object: true,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 1,
            clear_cells: 0,
            effect: "death".into(),
        };
        hidden.object_fx(&event, 0);
        event.clear_cells = 1;
        visible.object_fx(&event, 0);
        assert_eq!(hidden.requests.len(), 1);
        assert_eq!(visible.requests.len(), 2);
        assert_eq!(
            hidden.random.lock().unwrap().index(10_000),
            visible.random.lock().unwrap().index(10_000)
        );
        event.clear_cells = 0;
        hidden.events.get_mut("click").unwrap().priority = 4;
        hidden.object_fx(&event, 0);
        assert_eq!(hidden.requests.len(), 2);
        hidden.events.get_mut("click").unwrap().priority = 2;
        hidden.events.get_mut("click").unwrap().restrictions = 0;
        hidden.object_fx(&event, 0);
        assert_eq!(hidden.requests.len(), 3);
    }
    #[test]
    fn positional_fx_has_no_owner_gate_and_uses_cell_visibility() {
        let mut audio = crate::source_audio::speech_tests::audio();
        audio.events.get_mut("click").unwrap().restrictions = ST_PLAYER;
        audio.object_fx.insert("death".into(), vec!["click".into()]);
        let mut event = generals_sim::skirmish::object_fx::Event {
            frame: 1,
            object: 7,
            type_index: 0,
            owner: 2,
            position: [10.0, 20.0, 30.0],
            basis: generals_sim::skirmish::physical_ocl::transform::yaw(1.0),
            oriented_object: false,
            secondary_position: None,
            primary_geometry_radius: 0.0,
            override_radius: 0.0,
            visible_players: 0,
            clear_cells: 1,
            effect: "death".into(),
        };
        audio.object_fx(&event, 0);
        assert_eq!(audio.requests.len(), 1);
        assert_eq!(audio.requests[0].object, None);
        assert_eq!(
            audio.requests[0].position,
            Some(Vec3::new(10.0, 30.0, -20.0))
        );
        event.visible_players = 1;
        event.clear_cells = 0;
        audio.object_fx(&event, 0);
        assert_eq!(audio.requests.len(), 1);
    }
}
