//! Original `HotKeyTranslator` key-up dispatch for visible control-bar buttons.
use super::{
    Command, Match, Mouse, PromotionTransition, UiData, command_icons, dispatch_command,
    selected_command,
};
use crate::source_audio::SourceAudio;
use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};

pub fn command_keys(
    mut events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    data: Res<UiData>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut transition: ResMut<PromotionTransition>,
    mut audio: ResMut<SourceAudio>,
) {
    let keys = keys.into_inner();
    let data = data.into_inner();
    for event in events.read() {
        if !battle.started
            || battle.paused
            || !battle.simulation.player_active(0)
            || battle.simulation.end_game_at.is_some()
        {
            continue;
        }
        let Some(letter) = accelerator(event, keys) else {
            continue;
        };
        if data
            .promotions
            .accelerator
            .is_some_and(|key| key.eq_ignore_ascii_case(&letter))
        {
            transition.toggle(&mut battle);
            battle.placing = None;
            audio.ui("GUIClick");
            continue;
        }
        let construction =
            super::cancellation::foundation(&battle).map(|_| (&data.cancel_construction, true));
        let commands = construction.into_iter().chain((1..=14).filter_map(|slot| {
            selected_command(&battle, data, slot)
                .map(|command| (command, command_icons::available(&mut battle, command)))
        }));
        if let Some((command, enabled)) = matching_command(commands, letter) {
            // executeHotKey sends the same GBM_SELECTED as a button click.
            // A disabled match stays registered and does not fall through.
            if enabled {
                dispatch_command(&mut battle, &mut mouse, command.action);
                audio.ui("GUIClick");
            } else {
                audio.ui("GUIClickDisabled");
            }
        }
    }
}

fn accelerator(event: &KeyboardInput, keys: &ButtonInput<KeyCode>) -> Option<char> {
    // MetaEvent's E/Q/W mappings consume DOWN only. HotKeyTranslator runs next
    // and accepts UP with no Ctrl, Shift or Alt, including production buttons.
    if event.state.is_pressed()
        || event.repeat
        || keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ])
    {
        return None;
    }
    let Key::Character(text) = &event.logical_key else {
        return None;
    };
    let mut letters = text.chars();
    let letter = letters.next()?;
    letters.next().is_none().then_some(letter)
}

fn matching_command<'a>(
    mut commands: impl Iterator<Item = (&'a Command, bool)>,
    letter: char,
) -> Option<(&'a Command, bool)> {
    commands.find(|(command, _)| {
        command
            .accelerator
            .is_some_and(|key| key.eq_ignore_ascii_case(&letter))
    })
}

#[cfg(test)]
mod tests {
    use super::{accelerator, matching_command};
    use crate::control_bar::{Command, CommandAction};
    use bevy::{
        input::{ButtonState, keyboard::KeyboardInput},
        prelude::*,
    };

    fn event(state: ButtonState) -> KeyboardInput {
        KeyboardInput {
            key_code: KeyCode::KeyE,
            logical_key: bevy::input::keyboard::Key::Character("e".into()),
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }

    #[test]
    fn button_release_does_not_duplicate_selection_key_down_or_modified_shortcuts() {
        let mut keys = ButtonInput::default();
        assert!(accelerator(&event(ButtonState::Pressed), &keys).is_none());
        assert_eq!(accelerator(&event(ButtonState::Released), &keys), Some('e'));
        for modifier in [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ] {
            keys.press(modifier);
            assert!(accelerator(&event(ButtonState::Released), &keys).is_none());
            keys.release(modifier);
        }
        let mut repeated = event(ButtonState::Released);
        repeated.repeat = true;
        assert!(accelerator(&repeated, &keys).is_none());
    }

    fn production(slot: usize, product: usize) -> Command {
        Command {
            name: format!("Production{product}"),
            slot,
            action: CommandAction::Produce(product),
            accelerator: Some('R'),
            multi_select: false,
            image: String::new(),
            label: String::new(),
            description: String::new(),
            sciences: Vec::new(),
            required_upgrade: None,
        }
    }

    #[test]
    fn production_hotkey_uses_visible_button_and_retains_disabled_registration() {
        let building = production(1, 7);
        let unit = production(2, 11);
        let Some((command, enabled)) =
            matching_command([(&building, false), (&unit, true)].into_iter(), 'r')
        else {
            panic!("visible production button must register its accelerator");
        };
        assert!(!enabled);
        assert!(matches!(command.action, CommandAction::Produce(7)));
        let Some((command, enabled)) = matching_command([(&unit, true)].into_iter(), 'R') else {
            panic!("remaining visible production button must register");
        };
        assert!(enabled);
        assert!(matches!(command.action, CommandAction::Produce(11)));
        assert!(matching_command([(&unit, true)].into_iter(), 'z').is_none());
    }
}
