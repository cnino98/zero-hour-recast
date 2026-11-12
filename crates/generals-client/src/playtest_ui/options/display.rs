//! `OptionsMenu` resolution modes use the current monitor and actual physical window size.
use super::{Action, Editor, Layer, Request, UiData, at, text};
use bevy::{prelude::*, window::Monitor};

#[derive(Component)]
pub(super) enum Part {
    Combo,
    Row(usize),
    Arrow,
    Confirmation,
    ConfirmationMessage,
}

pub(super) struct Selection {
    pub modes: Vec<[u32; 2]>,
    pub open: bool,
    pub first: usize,
    pub enabled: bool,
    pub previous: Option<[u32; 2]>,
}

impl Selection {
    pub const fn new() -> Self {
        Self {
            modes: Vec::new(),
            open: false,
            first: 0,
            enabled: false,
            previous: None,
        }
    }

    pub fn load(&mut self, monitor: Option<&Monitor>, current: [u32; 2], enabled: bool) {
        self.modes.clear();
        if let Some(monitor) = monitor {
            for mode in &monitor.video_modes {
                let size = mode.physical_size.to_array();
                // W3DDisplay::isResolutionSupported no longer filters aspect ratios.
                if size[0] >= 800 && mode.bit_depth >= 24 && !self.modes.contains(&size) {
                    self.modes.push(size);
                }
            }
        }
        if !self.modes.contains(&current) {
            self.modes.push(current);
        }
        self.first = self
            .modes
            .iter()
            .position(|size| *size == current)
            .unwrap_or(0);
        self.first = self.first.min(self.modes.len().saturating_sub(5));
        self.open = false;
        self.enabled = enabled;
        self.previous = None;
    }

    pub fn click(&mut self, data: &UiData, point: Vec2, resolution: &mut [u32; 2]) -> bool {
        if !self.enabled || self.previous.is_some() {
            return false;
        }
        let area = data.options.area("ComboBoxResolution");
        if area.contains(point) {
            self.open = !self.open;
            return true;
        }
        if self.open {
            for row in 0..5 {
                if row_area(data, row).contains(point) {
                    if let Some(size) = self.modes.get(self.first + row) {
                        *resolution = *size;
                    }
                    self.open = false;
                    return true;
                }
            }
            self.open = false;
        }
        false
    }

    pub fn scroll(&mut self, forward: bool) {
        if forward {
            self.first = (self.first + 1).min(self.modes.len().saturating_sub(5));
        } else {
            self.first = self.first.saturating_sub(1);
        }
    }
}

fn row_area(data: &UiData, row: usize) -> Rect {
    let area = data.options.area("ComboBoxResolution");
    let offset =
        num_traits::ToPrimitive::to_f32(&(row + 1)).expect("five resolution rows") * area.height();
    Rect::from_corners(area.min + Vec2::Y * offset, area.max + Vec2::Y * offset)
}

pub(super) fn setup(commands: &mut Commands, data: &UiData) {
    commands
        .spawn((
            Part::Combo,
            Visibility::Hidden,
            at(data.options.area("ComboBoxResolution")),
            data.image("BlackSquare"),
            ZIndex(66),
        ))
        .with_children(|parent| {
            parent
                .spawn((Part::Combo, Visibility::Hidden, text("", 10.0)))
                .insert(TextLayout::justify(Justify::Left));
        });
    let area = data.options.area("ComboBoxResolution");
    let arrow = Rect::from_corners(Vec2::new(area.max.x - area.height(), area.min.y), area.max);
    commands.spawn((
        Part::Arrow,
        Visibility::Hidden,
        at(arrow),
        data.image("VSliderDownButtonEnabled"),
        ZIndex(67),
    ));
    confirmation(commands, data);
    for row in 0..5 {
        commands
            .spawn((
                Part::Row(row),
                Visibility::Hidden,
                at(row_area(data, row)),
                data.image("BlackSquare"),
                ZIndex(68),
            ))
            .with_children(|parent| {
                parent
                    .spawn((Part::Row(row), Visibility::Hidden, text("", 10.0)))
                    .insert(TextLayout::justify(Justify::Left));
            });
    }
}

pub(super) fn confirmation_request(data: &UiData, point: Vec2) -> Option<Request> {
    for (name, request) in [
        ("ButtonOk", Request::KeepResolution),
        ("ButtonCancel", Request::RevertResolution),
    ] {
        if data.options.confirmation[&format!("MessageBox.wnd:{name}")].contains(point) {
            return Some(request);
        }
    }
    None
}

fn confirmation(commands: &mut Commands, data: &UiData) {
    let area = |name| data.options.confirmation[&format!("MessageBox.wnd:{name}")];
    commands.spawn((
        Part::Confirmation,
        Visibility::Hidden,
        at(Rect::new(0.0, 0.0, 800.0, 600.0)),
        bevy::ui::FocusPolicy::Block,
        ZIndex(79),
    ));
    commands.spawn((
        Part::Confirmation,
        Visibility::Hidden,
        Node {
            border: UiRect::all(px(1)),
            ..at(area("MessageBoxParent"))
        },
        BackgroundColor(Color::srgb_u8(2, 2, 2)),
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        ZIndex(80),
    ));
    for (kind, name, label, size) in [
        (
            Part::Confirmation,
            "StaticTextTitle",
            data.label("GUI:Resolution"),
            20.0,
        ),
        (Part::ConfirmationMessage, "StaticTextMessage", "", 10.0),
    ] {
        commands.spawn((
            kind,
            Visibility::Hidden,
            at(area(name)),
            text(label, size),
            ZIndex(81),
        ));
    }
    for (action, name, label) in [
        (Action::OptionsAccept, "ButtonOk", "GUI:Ok"),
        (Action::OptionsBack, "ButtonCancel", "GUI:Cancel"),
    ] {
        let entity = super::super::menu::button_at_size(
            commands,
            data,
            action,
            Layer::Options,
            area(name),
            data.label(label),
            15.0,
        );
        commands.entity(entity).remove::<Layer>().insert((
            Part::Confirmation,
            Visibility::Hidden,
            ZIndex(82),
        ));
    }
}

pub(super) fn present(
    editor: &Editor,
    options_open: bool,
    data: &UiData,
    images: &mut Query<(&Part, &mut ImageNode)>,
    texts: &mut Query<(&Part, &mut Text, &mut TextColor)>,
    visibility: &mut Query<(&Part, &mut Visibility)>,
) {
    for (part, mut visible) in visibility {
        let shown = options_open
            && match part {
                Part::Confirmation | Part::ConfirmationMessage => editor.display.previous.is_some(),
                Part::Row(row) => {
                    editor.display.open
                        && editor
                            .display
                            .modes
                            .get(editor.display.first + row)
                            .is_some()
                }
                _ => true,
            };
        *visible = if shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for (part, mut image) in images {
        if matches!(part, Part::Arrow) {
            *image = data.image(if editor.display.enabled {
                "VSliderDownButtonEnabled"
            } else {
                "VSliderDownButtonDisabled"
            });
        }
    }
    for (part, mut label, mut color) in texts {
        let size = match part {
            Part::Combo => Some(&editor.draft.resolution),
            Part::Row(row) => editor.display.modes.get(editor.display.first + row),
            Part::Arrow | Part::Confirmation | Part::ConfirmationMessage => None,
        };
        if let Some([width, height]) = size {
            label.0 = format!("{width} x {height}");
        } else if matches!(part, Part::ConfirmationMessage) {
            let [width, height] = editor.draft.resolution;
            label.0 = format!("{}: {width}x{height}\n", data.label("GUI:Resolution"));
        }
        color.0 = if editor.display.enabled {
            Color::WHITE
        } else {
            Color::srgb_u8(192, 192, 192)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::VideoMode;

    #[test]
    fn original_mode_filter_preserves_widescreen_and_active_custom_mode() {
        let monitor = Monitor {
            name: None,
            physical_width: 1920,
            physical_height: 1080,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: None,
            scale_factor: 1.0,
            video_modes: vec![
                VideoMode {
                    physical_size: UVec2::new(640, 480),
                    bit_depth: 32,
                    refresh_rate_millihertz: 60000,
                },
                VideoMode {
                    physical_size: UVec2::new(1920, 1080),
                    bit_depth: 16,
                    refresh_rate_millihertz: 60000,
                },
                VideoMode {
                    physical_size: UVec2::new(1920, 1080),
                    bit_depth: 32,
                    refresh_rate_millihertz: 60000,
                },
                VideoMode {
                    physical_size: UVec2::new(1920, 1080),
                    bit_depth: 32,
                    refresh_rate_millihertz: 120_000,
                },
            ],
        };
        let mut selection = Selection::new();
        selection.load(Some(&monitor), [1217, 763], false);
        assert_eq!(selection.modes, vec![[1920, 1080], [1217, 763]]);
        assert!(!selection.enabled);
        assert!(!selection.open);
    }
}
