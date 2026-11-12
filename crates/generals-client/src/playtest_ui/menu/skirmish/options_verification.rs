//! Real pointer messages through the installed option gadgets.
use super::{Dropdown, Menu, UiData};
use bevy::{
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
};

pub(super) fn step(
    frame: u8,
    menu: &mut Menu,
    data: &UiData,
    window: (Entity, &mut Window),
    mouse: &mut MessageWriter<MouseButtonInput>,
    original: &mut Option<(u32, bool, u8)>,
) -> Result<(), &'static str> {
    let (entity, window) = window;
    let cash = data.windows["SkirmishGameOptionsMenu.wnd:ComboBoxStartingCash"];
    let checkbox = data.windows["SkirmishGameOptionsMenu.wnd:CheckboxLimitSuperweapons"];
    let slider = data.windows["SkirmishGameOptionsMenu.wnd:SliderGameSpeed"];
    let point = match frame {
        0 => {
            *original = Some((
                menu.starting_cash,
                menu.superweapon_restricted,
                menu.game_speed,
            ));
            menu.open = None;
            Some(cash.center())
        }
        3 => {
            if menu.open != Some(Dropdown::StartingCash) {
                return Err("pointer release did not open starting-money combo");
            }
            let index = data
                .starting_cash
                .iter()
                .position(|&value| value == 20_000)
                .ok_or("installed 20000 cash choice missing")?;
            let offset =
                f32::from(u16::try_from(index).expect("original money choice count")) * 14.0;
            Some(Vec2::new(cash.center().x, cash.max.y + offset + 9.0))
        }
        6 => {
            let saved = original.expect("initial options saved");
            if menu.starting_cash != 20_000
                || menu.open.is_some()
                || menu.superweapon_restricted != saved.1
            {
                return Err("pointer money selection failed or changed independent checkbox");
            }
            Some(checkbox.center())
        }
        9 => {
            if menu.superweapon_restricted == original.expect("initial options saved").1 {
                return Err("pointer checkbox release did not toggle superweapon limit");
            }
            let thumb = (f32::from(menu.game_speed - 15) * (slider.width() - 13.0) / 46.0).trunc();
            Some(Vec2::new(slider.min.x + thumb + 6.0, slider.min.y + 16.0))
        }
        11 => Some(Vec2::new(slider.max.x - 2.0, slider.min.y + 16.0)),
        13 => {
            if menu.game_speed != 61 {
                return Err("held pointer speed-thumb drag did not reach unlimited endpoint");
            }
            (
                menu.starting_cash,
                menu.superweapon_restricted,
                menu.game_speed,
            ) = original.expect("initial options saved");
            Some(Vec2::new(330.0, 380.0))
        }
        _ => None,
    };
    if let Some(point) = point {
        window.set_cursor_position(Some(
            point * Vec2::new(window.width() / 800.0, window.height() / 600.0),
        ));
    }
    let state = match frame {
        1 | 4 | 7 | 10 => Some(ButtonState::Pressed),
        2 | 5 | 8 | 12 => Some(ButtonState::Released),
        _ => None,
    };
    if let Some(state) = state {
        mouse.write(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: entity,
        });
    }
    Ok(())
}
