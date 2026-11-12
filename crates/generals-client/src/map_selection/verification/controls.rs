use super::{Check, MapSelection};
use crate::control_bar::UiData;
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
    },
    prelude::*,
    window::PrimaryWindow,
};

pub(super) fn scroll(world: &mut World, check: &mut Check) -> Result<bool, &'static str> {
    if check.controls >= 15 {
        return Ok(true);
    }
    let (entity, scale) = {
        let mut windows = world.query_filtered::<(Entity, &Window), With<PrimaryWindow>>();
        let (entity, window) = windows
            .single(world)
            .map_err(|_| "native primary window missing")?;
        (
            entity,
            Vec2::new(window.width() / 800.0, window.height() / 600.0),
        )
    };
    let list = world.resource::<UiData>().windows["SkirmishMapSelectMenu.wnd:ListboxMap"];
    let track = super::super::pointer::track(list);
    let selection = world.resource::<MapSelection>();
    let max = super::super::pointer::maximum(selection.count(), list);
    let thumb = super::super::pointer::thumb(track, selection.display_pos, max);
    match check.controls {
        0 => {
            check.scroll_top = selection.top;
            world
                .get_mut::<Window>(entity)
                .unwrap()
                .set_cursor_position(Some(list.center() * scale));
        }
        1 | 3 => {
            world.write_message(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: if check.controls == 1 { -1.0 } else { 1.0 },
                window: entity,
                phase: TouchPhase::Moved,
            });
        }
        2 => {
            if selection.top
                != (check.scroll_top + 1)
                    .min(selection.count().saturating_sub(super::super::ui::ROWS))
            {
                return Err("original wheel down did not move one list row");
            }
        }
        4 => {
            if selection.top != check.scroll_top {
                return Err("original wheel up did not restore list row");
            }
        }
        5 | 10 => {
            world
                .get_mut::<Window>(entity)
                .unwrap()
                .set_cursor_position(Some(thumb.center() * scale));
        }
        6 | 8 | 11 | 13 => {
            world.write_message(MouseButtonInput {
                button: MouseButton::Left,
                state: if matches!(check.controls, 6 | 11) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
                window: entity,
            });
        }
        7 | 12 => {
            let point = Vec2::new(
                track.center().x,
                if check.controls == 7 {
                    track.max.y
                } else {
                    track.min.y
                },
            );
            world
                .get_mut::<Window>(entity)
                .unwrap()
                .set_cursor_position(Some(point * scale));
        }
        9 => {
            if selection.display_pos.to_bits() != max.to_bits() {
                return Err("held original thumb did not reach lower pixel endpoint");
            }
        }
        14 => {
            if selection.display_pos.to_bits() != 0.0_f32.to_bits() || selection.top != 0 {
                return Err("held original thumb did not return to upper pixel endpoint");
            }
            println!("MAP_CHOOSER_SCROLL original_wheel=true held_thumb=true pixel_endpoints=true");
        }
        _ => unreachable!("bounded native scrollbar phases"),
    }
    check.controls += 1;
    Ok(false)
}

pub(super) fn keyboard(
    world: &mut World,
    check: &mut Check,
    target: usize,
) -> Result<bool, &'static str> {
    if check.controls >= 20 {
        return Ok(true);
    }
    let mut windows = world.query_filtered::<Entity, With<PrimaryWindow>>();
    let window = windows
        .single(world)
        .map_err(|_| "native primary window missing")?;
    let target_row = world
        .resource::<MapSelection>()
        .row(target)
        .ok_or("native target outside category")?;
    let down_first = target_row + 1 < world.resource::<MapSelection>().count();
    match check.controls {
        15..=18 => {
            let down = if check.controls < 17 {
                down_first
            } else {
                !down_first
            };
            world.write_message(KeyboardInput {
                key_code: if down {
                    KeyCode::ArrowDown
                } else {
                    KeyCode::ArrowUp
                },
                logical_key: if down { Key::ArrowDown } else { Key::ArrowUp },
                state: if matches!(check.controls, 15 | 17) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
                text: None,
                repeat: false,
                window,
            });
        }
        19 => {
            if world.resource::<MapSelection>().highlighted != Some(target) {
                return Err("original list keyboard traversal did not preserve restored selection");
            }
            println!("MAP_CHOOSER_KEYS original_arrows=true selected_row_restored=true");
        }
        _ => return Err("native keyboard phases preceded scrollbar proof"),
    }
    check.controls += 1;
    Ok(false)
}
