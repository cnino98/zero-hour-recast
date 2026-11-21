//! SH `MissileLauncherBuildingUpdate` door, timer and launch callbacks.
use super::{Entity, Skirmish, object_sounds::Action};

pub struct Rules {
    pub power: String,
    pub open_frames: u32,
    pub wait_open_frames: u32,
    pub close_frames: u32,
    pub effects: [String; 5],
    pub open_sound: String,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Door {
    #[default]
    Closed,
    Opening,
    Open,
    WaitingToClose,
    Closing,
}

#[derive(Default)]
pub(in crate::skirmish) struct State {
    door: Door,
    next: Door,
    deadline: u32,
    animation: Option<(u64, u32, bool)>,
    playing: bool,
}

impl Entity {
    pub(super) const fn nuclear_launcher_model_condition(&self) -> Option<&'static str> {
        match self.nuclear_launcher.door {
            Door::Closed => None,
            Door::Opening => Some("DOOR_1_OPENING"),
            Door::Open => Some("DOOR_1_WAITING_OPEN"),
            Door::WaitingToClose => Some("DOOR_1_WAITING_TO_CLOSE"),
            Door::Closing => Some("DOOR_1_CLOSING"),
        }
    }

    pub(super) const fn nuclear_launcher_animation(&self) -> Option<(u64, u32, bool)> {
        self.nuclear_launcher.animation
    }
}

impl Skirmish {
    pub(in crate::skirmish) fn update_nuclear_launcher(&mut self, id: usize) -> i32 {
        if self.entities[id].under_construction() {
            return 1;
        }
        let kind = self.entities[id].type_index;
        let rules = self.catalog[kind]
            .behavior
            .nuclear_launcher
            .as_ref()
            .unwrap();
        let open_frames = rules.open_frames;
        let power = rules.power.clone();
        let Some(ready) = self.special_power_ready_frame(id, &power) else {
            return 1;
        };
        let now = self.update_frame();
        let state = &self.entities[id].nuclear_launcher;
        if state.deadline != 0 && now > state.deadline {
            self.change_nuclear_launcher_door(id, state.next, ready);
        }
        let door = self.entities[id].nuclear_launcher.door;
        if door != Door::Open && self.special_power_ready(id, &power) {
            self.change_nuclear_launcher_door(id, Door::Open, ready);
        } else if door == Door::Closed && now >= ready.saturating_sub(open_frames) {
            self.change_nuclear_launcher_door(id, Door::Opening, ready);
        }
        1
    }

    pub(in crate::skirmish) fn initiate_nuclear_launcher(
        &mut self,
        id: usize,
        power: &str,
    ) -> bool {
        let kind = self.entities[id].type_index;
        if self.catalog[kind]
            .behavior
            .nuclear_launcher
            .as_ref()
            .is_none_or(|rules| rules.power != power)
        {
            return false;
        }
        let Some(ready) = self.special_power_ready_frame(id, power) else {
            return false;
        };
        self.change_nuclear_launcher_door(id, Door::WaitingToClose, ready);
        true
    }

    fn change_nuclear_launcher_door(&mut self, id: usize, door: Door, ready: u32) {
        if self.entities[id].nuclear_launcher.door == door {
            return;
        }
        let now = self.update_frame();
        let kind = self.entities[id].type_index;
        let rules = self.catalog[kind]
            .behavior
            .nuclear_launcher
            .as_ref()
            .unwrap();
        let (next, deadline) = match door {
            Door::Closed | Door::Open => (door, 0),
            Door::Opening => (Door::Open, ready.wrapping_sub(1)),
            Door::WaitingToClose => (Door::Closing, now.wrapping_add(rules.wait_open_frames)),
            Door::Closing => {
                let delta = i32::from_ne_bytes(ready.wrapping_sub(now).to_ne_bytes());
                let limit = now.wrapping_add_signed(delta / 2);
                (
                    Door::Closed,
                    now.wrapping_add(rules.close_frames).min(limit),
                )
            }
        };
        let effect = rules.effects[door as usize].clone();
        if !effect.is_empty() {
            self.positional_fx_with_radius(id, effect, self.object_position(id), 0.0);
        }
        if door == Door::Open {
            self.start_nuclear_launcher_audio(id);
        } else {
            self.stop_nuclear_launcher_audio(id);
        }
        let state = &mut self.entities[id].nuclear_launcher;
        state.door = door;
        state.next = next;
        state.deadline = deadline;
        state.animation = (deadline != 0).then(|| (self.tick, deadline.wrapping_sub(now), false));
    }

    fn start_nuclear_launcher_audio(&mut self, id: usize) {
        if let Some((index, sound)) = self.nuclear_launcher_audio(id) {
            self.entities[id].nuclear_launcher.playing = true;
            self.object_sound(id, sound, Action::PlayLoop(index));
        }
    }

    pub(in crate::skirmish) fn stop_nuclear_launcher_audio(&mut self, id: usize) {
        if self.entities[id].nuclear_launcher.playing
            && let Some((index, sound)) = self.nuclear_launcher_audio(id)
        {
            self.entities[id].nuclear_launcher.playing = false;
            self.object_sound(id, sound, Action::StopLoop(index));
        }
    }

    fn nuclear_launcher_audio(&self, id: usize) -> Option<(usize, String)> {
        let unit = &self.catalog[self.entities[id].type_index];
        let sound = &unit.behavior.nuclear_launcher.as_ref()?.open_sound;
        let index = unit
            .behavior_modules
            .iter()
            .position(|module| module.family == "MissileLauncherBuildingUpdate")?;
        Some((index, sound.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish::{Faction, Kind, special_power, tests};

    #[test]
    fn door_enters_ready_state_at_timer_boundary_and_retires_ready_hiss_on_fire() {
        let mut game = tests::game();
        let mut unit = tests::fixture_type(Faction::China, Kind::TechBuilding);
        let power = "SuperweaponNeutronMissile";
        unit.special_powers.push(special_power::Rules {
            tag: "Weapon".into(),
            name: power.into(),
            power_type: "SPECIAL_NEUTRON_MISSILE".into(),
            required_science: String::new(),
            timer: special_power::Timer {
                reload_frames: 10800,
                shared: false,
                public: true,
            },
            view_range: 0.0,
            view_frames: 0,
            view_template: None,
            initiate_sound: String::new(),
            target_sound: String::new(),
            module_initiate_sound: String::new(),
            academy_classification: String::new(),
            update_starts_attack: false,
            startup: special_power::Startup {
                paused: false,
                ready: false,
            },
            script_only: false,
        });
        unit.behavior.nuclear_launcher = Some(Rules {
            power: power.into(),
            open_frames: 240,
            wait_open_frames: 60,
            close_frames: 240,
            effects: [
                "",
                "ChinaNuclearMissileLauncherDoorOpen",
                "",
                "ChinaNuclearMissileLauncherLaunch",
                "",
            ]
            .map(str::to_owned),
            open_sound: "BuildingNeutronMissileHiss".into(),
        });
        tests::register_behavior(&mut unit, "OCLSpecialPower", 0x0100);
        unit.behavior_modules.last_mut().unwrap().tag = "Weapon".into();
        tests::register_update(&mut unit, "MissileLauncherBuildingUpdate");
        let audio_module = unit.behavior_modules.len() - 1;
        let kind = game.catalog.len();
        game.catalog.push(unit);
        let id = game.spawn(kind, 0, [250.0; 2], true);
        game.tick = 10_559;
        game.update_nuclear_launcher(id);
        assert!(
            game.entities[id]
                .nuclear_launcher_model_condition()
                .is_none()
        );
        game.tick += 1;
        game.update_nuclear_launcher(id);
        assert_eq!(
            game.entities[id].nuclear_launcher_model_condition(),
            Some("DOOR_1_OPENING")
        );
        game.tick = 10_799;
        game.update_nuclear_launcher(id);
        assert_eq!(
            game.entities[id].nuclear_launcher_model_condition(),
            Some("DOOR_1_OPENING")
        );
        game.tick += 1;
        game.update_nuclear_launcher(id);
        assert_eq!(
            game.entities[id].nuclear_launcher_model_condition(),
            Some("DOOR_1_WAITING_OPEN")
        );
        assert!(matches!(
            game.presentation.object_sounds.last().unwrap().action,
            Action::PlayLoop(index) if index == audio_module
        ));
        assert!(game.initiate_nuclear_launcher(id, power));
        game.mark_special_power_triggered(id, power, None);
        assert!(matches!(
            game.presentation.object_sounds.last().unwrap().action,
            Action::StopLoop(index) if index == audio_module
        ));
        game.tick += 60;
        game.update_nuclear_launcher(id);
        assert_eq!(
            game.entities[id].nuclear_launcher_model_condition(),
            Some("DOOR_1_WAITING_TO_CLOSE")
        );
        game.tick += 1;
        game.update_nuclear_launcher(id);
        assert_eq!(
            game.entities[id].nuclear_launcher_model_condition(),
            Some("DOOR_1_CLOSING")
        );
        assert_eq!(
            game.entities[id].nuclear_launcher_animation(),
            Some((10_861, 240, false))
        );
    }
}
