//! Original listbox release/double-click, wheel and vertical-slider input.
use super::MapSelection;
use crate::{control_bar::UiData, playtest_ui::Action};
use bevy::{
    ecs::system::SystemParam,
    input::{
        ButtonState,
        mouse::{MouseButtonInput, MouseWheel},
    },
    prelude::*,
    window::PrimaryWindow,
};
use std::time::Duration;

#[derive(Resource)]
pub struct DoubleClickInterval(pub Duration);

#[derive(Default)]
pub struct Pointer {
    release: Option<f64>,
    dragging: Option<f32>,
    track_down: bool,
}

#[derive(SystemParam)]
pub struct Input<'w, 's> {
    selection: Option<ResMut<'w, MapSelection>>,
    data: Res<'w, UiData>,
    window: Single<'w, 's, (Entity, &'static Window), With<PrimaryWindow>>,
    buttons: MessageReader<'w, 's, MouseButtonInput>,
    wheel: MessageReader<'w, 's, MouseWheel>,
    held: Res<'w, ButtonInput<MouseButton>>,
    time: Res<'w, Time<Real>>,
    interval: Option<Res<'w, DoubleClickInterval>>,
}

pub fn input(mut input: Input, mut pointer: Local<Pointer>) {
    let Some(selection) = input.selection.as_deref_mut() else {
        return;
    };
    if !selection.open {
        *pointer = Pointer::default();
        input.buttons.clear();
        input.wheel.clear();
        return;
    }
    let (entity, window) = *input.window;
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let point = cursor * Vec2::new(800.0 / window.width(), 600.0 / window.height());
    let list = input.data.windows["SkirmishMapSelectMenu.wnd:ListboxMap"];
    let track = track(list);
    for event in input.wheel.read() {
        if event.window == entity && list.contains(point) && event.y != 0.0 {
            selection.activate(&Action::MapScroll(if event.y > 0.0 { -1 } else { 1 }));
        }
    }
    for event in input.buttons.read() {
        if event.window != entity || event.button != MouseButton::Left {
            continue;
        }
        if event.state == ButtonState::Pressed {
            let thumb = thumb(
                track,
                selection.display_pos,
                maximum(selection.count(), list),
            );
            pointer.dragging = thumb.contains(point).then_some(point.y - thumb.min.y);
            pointer.track_down = pointer.dragging.is_none() && track.contains(point);
        } else {
            let dragging = pointer.dragging.take().is_some();
            if !dragging && pointer.track_down && track.contains(point) {
                let max = maximum(selection.count(), list);
                let centre = thumb(track, selection.display_pos, max).center().y;
                let change = (track.height() / 5.0).trunc();
                let centre = if point.y >= centre {
                    (centre + change).min(point.y)
                } else {
                    (centre - change).max(point.y)
                };
                set_position(selection, drag_position(track, centre - 11.0, max), max);
            } else if !dragging
                && !pointer.track_down
                && point.x < list.max.x - 23.0
                && list.contains(point)
            {
                let row = ((point.y - list.min.y + selection.display_pos) / super::ui::ROW_HEIGHT)
                    .floor();
                if let Some(row) =
                    num_traits::ToPrimitive::to_usize(&row).filter(|&row| row < selection.count())
                {
                    release(
                        selection,
                        &mut pointer,
                        row,
                        input.time.elapsed_secs_f64(),
                        input.interval.as_deref(),
                    );
                }
            }
            pointer.track_down = false;
        }
    }
    if let Some(offset) = pointer.dragging {
        if input.held.pressed(MouseButton::Left) {
            let max = maximum(selection.count(), list);
            set_position(selection, drag_position(track, point.y - offset, max), max);
        } else {
            pointer.dragging = None;
        }
    }
}

fn release(
    selection: &mut MapSelection,
    pointer: &mut Pointer,
    row: usize,
    now: f64,
    interval: Option<&DoubleClickInterval>,
) {
    let double = pointer
        .release
        .zip(interval)
        .is_some_and(|(previous, interval)| {
            previous + interval.0.as_secs_f64() > now
                && (selection.highlighted == selection.index(row)
                    || selection.highlighted.is_none())
        });
    selection.activate(&Action::MapRow(row));
    if double {
        selection.activate(&Action::MapAccept);
    }
    pointer.release = Some(now);
}

pub(super) fn row_position(top: usize) -> f32 {
    if top == 0 {
        0.0
    } else {
        let pixels =
            f32::from(u16::try_from(top).expect("original list capacity")) * super::ui::ROW_HEIGHT;
        pixels + 1.0
    }
}

pub(super) fn maximum(count: usize, list: Rect) -> f32 {
    let pixels =
        f32::from(u16::try_from(count).expect("original list capacity")) * super::ui::ROW_HEIGHT;
    (pixels - (list.height() - 2.0) + 1.0).max(0.0)
}

pub(super) fn track(list: Rect) -> Rect {
    Rect::new(
        list.max.x - 23.0,
        list.min.y + 25.0,
        list.max.x - 2.0,
        list.max.y - 25.0,
    )
}

pub(super) fn thumb(track: Rect, display: f32, maximum: f32) -> Rect {
    let offset = if maximum > 0.0 {
        let ticks = (track.height() - 22.0) / maximum;
        (display * ticks).trunc()
    } else {
        0.0
    };
    Rect::new(
        track.min.x,
        track.min.y + offset,
        track.max.x,
        track.min.y + offset + 22.0,
    )
}

fn drag_position(track: Rect, top: f32, maximum: f32) -> f32 {
    if maximum <= 0.0 {
        return 0.0;
    }
    let ticks = (track.height() - 22.0) / maximum;
    ((top - track.min.y) / ticks).trunc().clamp(0.0, maximum)
}

fn set_position(selection: &mut MapSelection, position: f32, maximum: f32) {
    selection.display_pos = position.clamp(0.0, maximum);
    selection.top =
        num_traits::ToPrimitive::to_usize(&(selection.display_pos / super::ui::ROW_HEIGHT).floor())
            .expect("nonnegative bounded list position");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_selection::MapEntry;

    fn selection() -> MapSelection {
        MapSelection {
            maps: (0..20)
                .map(|index| MapEntry {
                    path: index.to_string(),
                    title: index.to_string(),
                    preview: String::new(),
                    capacity: 2,
                    extent: Vec2::splat(100.0),
                    starts: Vec::new(),
                    supplies: Vec::new(),
                    tech: Vec::new(),
                    admission: crate::map_selection::Admission::Unknown,
                })
                .collect(),
            rejected: Vec::new(),
            current: 0,
            requested: None,
            open: true,
            user_maps: false,
            catalog_state: crate::map_selection::CatalogState::Ready,
            unlisted: None,
            highlighted: Some(0),
            top: 0,
            display_pos: 0.0,
            list_focused: false,
        }
    }

    #[test]
    fn release_double_click_requires_original_selected_row_and_strict_system_interval() {
        let mut maps = selection();
        let mut pointer = Pointer::default();
        let interval = DoubleClickInterval(Duration::from_millis(500));
        release(&mut maps, &mut pointer, 1, 1.0, Some(&interval));
        assert_eq!(maps.highlighted, Some(1));
        assert!(maps.requested.is_none());
        release(&mut maps, &mut pointer, 2, 1.1, Some(&interval));
        assert!(maps.requested.is_none());
        release(&mut maps, &mut pointer, 2, 1.6, Some(&interval));
        assert!(maps.requested.is_none());
        release(&mut maps, &mut pointer, 2, 1.7, Some(&interval));
        assert_eq!(maps.requested, Some(2));
        maps.requested = None;
        maps.maps[2].admission =
            crate::map_selection::Admission::Rejected("mandatory data unavailable".to_owned());
        release(&mut maps, &mut pointer, 2, 1.8, Some(&interval));
        assert!(maps.requested.is_none());
        maps.maps[2].admission = crate::map_selection::Admission::Unknown;
        release(&mut maps, &mut pointer, 2, 1.9, None);
        assert!(maps.requested.is_none());
    }

    #[test]
    fn native_slider_uses_fixed_thumb_pixels_and_clamps_partial_row_positions() {
        let list = Rect::new(10.0, 20.0, 210.0, 166.0);
        let track = track(list);
        assert_eq!(track, Rect::new(187.0, 45.0, 208.0, 141.0));
        let max = maximum(20, list);
        assert_eq!(max.to_bits(), 177.0_f32.to_bits());
        assert_eq!(thumb(track, 0.0, max), Rect::new(187.0, 45.0, 208.0, 67.0));
        assert_eq!(
            thumb(track, max, max),
            Rect::new(187.0, 119.0, 208.0, 141.0)
        );
        let mut maps = selection();
        set_position(&mut maps, drag_position(track, 200.0, max), max);
        assert_eq!(maps.display_pos.to_bits(), max.to_bits());
        assert_eq!(maps.top, 11);
        set_position(&mut maps, drag_position(track, -1.0, max), max);
        assert_eq!(maps.top, 0);
        assert_eq!(maps.display_pos.to_bits(), 0.0_f32.to_bits());
        set_position(&mut maps, 23.0, max);
        assert_eq!(maps.top, 1);
        assert_eq!(
            (maps.display_pos - row_position(maps.top)).to_bits(),
            6.0_f32.to_bits()
        );
        assert_eq!(maximum(2, list).to_bits(), 0.0_f32.to_bits());
    }
}
