//! SH 2bfaea60 `CommandButtonHuntUpdate` and `doTeamHuntWithCommandButton`.
use super::{Skirmish, ai_goals::COMMAND_FROM_AI, module_updates::FOREVER, special_ability::Power};
use std::collections::BTreeMap;
mod actions;
mod scan;
#[cfg(test)]
mod tests;

pub struct Rules {
    pub scan_frames: u32,
    pub scan_range: f32,
}

#[derive(Clone, Copy)]
pub enum Command {
    SpecialPower,
    SwitchWeapon,
    FireWeapon,
    Hijack,
    CarBomb,
    Sabotage,
    Other,
}

pub struct Button {
    pub name: String,
    pub command: Command,
    pub weapon_slot: usize,
    pub power: Option<Power>,
    pub needs_object: bool,
}

pub struct Definitions {
    pub buttons: Vec<Button>,
    pub sets: BTreeMap<String, [Option<usize>; 18]>,
}

impl Definitions {
    pub(super) const fn empty() -> Self {
        Self {
            buttons: Vec::new(),
            sets: BTreeMap::new(),
        }
    }
}

pub(super) struct State {
    name: String,
    button: Option<usize>,
}

impl Skirmish {
    pub fn configure_command_buttons(&mut self, definitions: Definitions) {
        self.command_buttons = definitions;
    }

    pub(in crate::skirmish) fn construct_command_button_hunt(&mut self, id: usize, ordinal: usize) {
        assert_eq!(
            self.entities[id].behaviors.command_button_hunts.len(),
            ordinal
        );
        self.entities[id]
            .behaviors
            .command_button_hunts
            .push(State {
                name: String::new(),
                button: None,
            });
        self.wake_family_update(id, "CommandButtonHuntUpdate", ordinal, FOREVER);
    }

    fn current_command_buttons(&self, id: usize) -> Option<&[Option<usize>; 18]> {
        let actor = &self.entities[id];
        let set = actor
            .command_set_override()
            .unwrap_or(&self.catalog[actor.type_index].command_set);
        self.command_buttons.sets.get(set)
    }

    pub(in crate::skirmish) fn team_command_button_hunt(&mut self, members: &[usize], name: &str) {
        let Some(button) = self
            .command_buttons
            .buttons
            .iter()
            .position(|button| button.name == name)
        else {
            return;
        };
        let definition = &self.command_buttons.buttons[button];
        let permitted = match definition.command {
            Command::SpecialPower => definition.power.is_some() && definition.needs_object,
            Command::SwitchWeapon
            | Command::FireWeapon
            | Command::Hijack
            | Command::CarBomb
            | Command::Sabotage => true,
            Command::Other => false,
        };
        if !permitted {
            return;
        }
        for &id in members {
            if self.entities[id].ai.registration.is_none()
                || !self
                    .current_command_buttons(id)
                    .is_some_and(|set| set.contains(&Some(button)))
            {
                continue;
            }
            if !self.entities[id].behaviors.command_button_hunts.is_empty() {
                self.set_hunt_command_button(id, 0, name);
            }
        }
    }

    fn set_hunt_command_button(&mut self, id: usize, ordinal: usize, name: &str) {
        let button = self.current_command_buttons(id).and_then(|set| {
            set.iter()
                .flatten()
                .copied()
                .find(|&index| self.command_buttons.buttons[index].name == name)
        });
        let state = &mut self.entities[id].behaviors.command_button_hunts[ordinal];
        name.clone_into(&mut state.name);
        state.button = button;
        if button.is_none() || self.entities[id].ai.registration.is_none() {
            return;
        }
        self.ai_idle_from_ai(id);
        // Native direct update's return value is discarded, then wake is explicitly one.
        self.update_command_button_hunt(id, ordinal);
        self.wake_family_update(id, "CommandButtonHuntUpdate", ordinal, 1);
    }

    pub(in crate::skirmish) fn update_command_button_hunt(
        &mut self,
        id: usize,
        ordinal: usize,
    ) -> i32 {
        let Some(button) = self.entities[id].behaviors.command_button_hunts[ordinal].button else {
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        };
        if self.entities[id].ai.registration.is_none() {
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        }
        if !self.entities[id].last_command_from_ai() {
            let state = &mut self.entities[id].behaviors.command_button_hunts[ordinal];
            state.button = None;
            state.name.clear();
            return i32::from_ne_bytes(FOREVER.to_ne_bytes());
        }
        match self.command_buttons.buttons[button].command {
            Command::SwitchWeapon | Command::FireWeapon => {
                if self.entities[id].ai_idle() {
                    self.native_hunt_from(id, COMMAND_FROM_AI);
                }
                let slot = self.command_buttons.buttons[button].weapon_slot;
                if self.allocated_weapon(id, slot).is_some()
                    && !matches!(
                        self.entities[id].weapon_set.lock,
                        super::weapons::Lock::Permanent
                    )
                {
                    self.entities[id].weapon_set.current = slot;
                    self.entities[id].weapon_set.lock = super::weapons::Lock::Temporary;
                }
                1
            }
            Command::SpecialPower | Command::Hijack | Command::CarBomb | Command::Sabotage => {
                self.hunt_button_target(id, ordinal, button)
            }
            Command::Other => i32::from_ne_bytes(FOREVER.to_ne_bytes()),
        }
    }

    fn hunt_button_target(&mut self, id: usize, ordinal: usize, button: usize) -> i32 {
        let frames = self.catalog[self.entities[id].type_index]
            .behavior
            .command_button_hunts[ordinal]
            .scan_frames;
        if !self.entities[id].ai_idle() {
            return i32::from_ne_bytes(frames.to_ne_bytes());
        }
        let definition = &self.command_buttons.buttons[button];
        if matches!(definition.command, Command::SpecialPower)
            && let Some(power) = &definition.power
        {
            let Some(ability) = self.first_special_ability(id, power.kind) else {
                return i32::from_ne_bytes(FOREVER.to_ne_bytes());
            };
            if self.native_ability_active(id, ability) {
                return i32::from_ne_bytes(frames.to_ne_bytes());
            }
        }
        if let Some(target) = self.scan_command_button_target(id, ordinal, button) {
            self.perform_hunt_button_at_object(id, button, target);
        }
        i32::from_ne_bytes(frames.to_ne_bytes())
    }

    fn perform_hunt_button_at_object(&mut self, id: usize, button: usize, target: usize) {
        if self.object_disabled(id) {
            return;
        }
        let definition = &self.command_buttons.buttons[button];
        match definition.command {
            Command::SpecialPower => {
                if let Some(power) = &definition.power {
                    let name = power.name.clone();
                    self.special_power_at_object(id, &name, target);
                }
            }
            Command::Hijack | Command::CarBomb | Command::Sabotage => {
                self.native_enter_from(id, target, COMMAND_FROM_AI);
            }
            _ => unreachable!("target hunting command family"),
        }
    }
}
