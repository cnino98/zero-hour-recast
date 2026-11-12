//! Original `OptionsMenu` edits a snapshot and applies only on Accept.
use super::{Action, Layer, Menu, ReleaseButton, UiData, at, text};
use crate::{
    input::Mouse,
    options_preferences::{Draft, Preferences},
    playtest::CameraRig,
    source_audio::SourceAudio,
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, keyboard::KeyboardInput, mouse::MouseButtonInput},
    prelude::*,
    window::PrimaryWindow,
};

mod display;
mod slider;
pub mod verification;
use slider::Slider;

pub enum Request {
    Open,
    Back,
    Accept,
    Defaults,
    Alternate,
    Retaliation,
    DoubleClickAttackMove,
    KeepResolution,
    RevertResolution,
}
#[derive(Resource)]
pub struct Editor {
    display: display::Selection,
    request: Option<Request>,
    draft: Draft,
    focused: Option<Slider>,
    dragging: Option<(Slider, f32)>,
    track: Option<Slider>,
}

pub const fn activate(action: &Action, editor: &mut Editor) -> bool {
    if editor.display.previous.is_some() {
        return matches!(
            action,
            Action::Options
                | Action::OptionsBack
                | Action::OptionsAccept
                | Action::OptionsDefaults
                | Action::OptionsAlternate
                | Action::OptionsRetaliation
                | Action::OptionsDoubleClickAttackMove
        );
    }
    editor.request = Some(match action {
        Action::Options => Request::Open,
        Action::OptionsBack => Request::Back,
        Action::OptionsAccept => Request::Accept,
        Action::OptionsDefaults => Request::Defaults,
        Action::OptionsAlternate => Request::Alternate,
        Action::OptionsRetaliation => Request::Retaliation,
        Action::OptionsDoubleClickAttackMove => Request::DoubleClickAttackMove,
        _ => return false,
    });
    true
}

#[derive(Component)]
pub struct BoxPart {
    slider: Slider,
    index: usize,
    highlight: bool,
}
#[derive(Component)]
pub struct Thumb(Slider);
#[derive(Component)]
pub enum Checkbox {
    Alternate,
    Retaliation,
    DoubleClickAttackMove,
}

impl Checkbox {
    const fn name(&self) -> &'static str {
        match self {
            Self::Alternate => "CheckAlternateMouse",
            Self::Retaliation => "Retaliation",
            Self::DoubleClickAttackMove => "CheckDoubleClickAttackMove",
        }
    }
    const fn checked(&self, draft: &Draft) -> bool {
        match self {
            Self::Alternate => draft.alternate_mouse,
            Self::Retaliation => draft.retaliation,
            Self::DoubleClickAttackMove => draft.double_click_attack_move,
        }
    }
}

pub fn setup(commands: &mut Commands, data: &UiData) {
    commands.insert_resource(Editor {
        display: display::Selection::new(),
        request: None,
        draft: Draft {
            resolution: [800, 600],
            gamma: 50,
            music: 0,
            sfx: 0,
            voice: 0,
            scroll: 0,
            alternate_mouse: false,
            retaliation: true,
            double_click_attack_move: false,
        },
        focused: None,
        dragging: None,
        track: None,
    });
    backdrop(commands, data);
    for label in &data.options.labels {
        commands
            .spawn((
                Layer::Options,
                Node {
                    align_items: AlignItems::Center,
                    ..at(label.area)
                },
                ZIndex(62),
            ))
            .with_children(|parent| {
                parent
                    .spawn(text(&label.text, label.font_size))
                    .insert(TextFont {
                        font: if label.serif {
                            data.serif_handle.clone().into()
                        } else {
                            data.font_handle.clone().into()
                        },
                        font_size: FontSize::Px(label.font_size),
                        ..default()
                    });
            });
    }
    for (action, name, label) in [
        (Action::OptionsBack, "ButtonBack", "GUI:Cancel"),
        (Action::OptionsAccept, "ButtonAccept", "GUI:Accept"),
        (Action::OptionsDefaults, "ButtonDefaults", "GUI:Defaults"),
    ] {
        let entity = super::menu::button_at_size(
            commands,
            data,
            action,
            Layer::Options,
            data.options.area(name),
            data.label(label),
            15.0,
        );
        commands.entity(entity).insert(ZIndex(65));
    }
    checkboxes(commands, data);
    display::setup(commands, data);
    // The two physical-pixel gaps only reduce the source box count.
    for slider in Slider::ALL {
        let capacity = num_traits::ToPrimitive::to_usize(
            &(slider.area(data).width() / data.image_size("hilightedbox").x).ceil(),
        )
        .expect("source box capacity")
            + 2;
        for index in 0..capacity {
            for highlight in [true, false] {
                commands.spawn((
                    BoxPart {
                        slider,
                        index,
                        highlight,
                    },
                    at(Rect::default()),
                    data.image(if highlight {
                        "linebox"
                    } else {
                        "dehilightedbox"
                    }),
                    ZIndex(if highlight { 61 } else { 62 }),
                ));
            }
        }
        commands.spawn((
            Thumb(slider),
            at(Rect::default()),
            data.image("arrow"),
            ZIndex(64),
        ));
    }
}

fn backdrop(commands: &mut Commands, data: &UiData) {
    commands.spawn((
        Layer::Options,
        at(data.options.background),
        BackgroundColor(Color::srgba_u8(2, 2, 2, 126)),
        ZIndex(59),
    ));
    commands.spawn((
        Layer::Options,
        Node {
            border: UiRect::all(px(1)),
            ..at(data.options.area("OptionsMenuParentOld"))
        },
        BackgroundColor(Color::srgba_u8(2, 2, 2, 175)),
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        ZIndex(60),
    ));
}

fn checkboxes(commands: &mut Commands, data: &UiData) {
    for (kind, action, label) in [
        (
            Checkbox::Alternate,
            Action::OptionsAlternate,
            "GUI:AlternateMouse",
        ),
        (
            Checkbox::Retaliation,
            Action::OptionsRetaliation,
            "GUI:RetaliationTitle",
        ),
        (
            Checkbox::DoubleClickAttackMove,
            Action::OptionsDoubleClickAttackMove,
            "GUI:DoubleClickAttackMove",
        ),
    ] {
        checkbox(commands, data, kind, action, label);
    }
}

fn checkbox(commands: &mut Commands, data: &UiData, kind: Checkbox, action: Action, label: &str) {
    let area = data.options.area(kind.name());
    commands
        .spawn((
            Layer::Options,
            action,
            Button,
            ReleaseButton { armed: false },
            at(area),
            ZIndex(65),
        ))
        .with_children(|parent| {
            parent.spawn((kind, Node::default(), data.image("Active-Unchecked")));
            parent
                .spawn(Node {
                    left: percent(area.height() / area.width() * 100.0),
                    width: percent((area.width() - area.height()) / area.width() * 100.0),
                    height: percent(100),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|parent| {
                    parent
                        .spawn(text(data.label(label), 14.0))
                        .insert(TextLayout::justify(Justify::Left).with_no_wrap());
                });
        });
}

#[derive(SystemParam)]
pub struct Input<'w, 's> {
    editor: ResMut<'w, Editor>,
    menu: Res<'w, Menu>,
    data: Res<'w, UiData>,
    window: Single<'w, 's, (Entity, &'static Window), With<PrimaryWindow>>,
    buttons: MessageReader<'w, 's, MouseButtonInput>,
    keys: MessageReader<'w, 's, KeyboardInput>,
    held: Res<'w, ButtonInput<MouseButton>>,
    wheel: MessageReader<'w, 's, bevy::input::mouse::MouseWheel>,
    key_state: ResMut<'w, ButtonInput<KeyCode>>,
}

pub fn pointer(mut input: Input) {
    if !input.menu.options_open {
        input.editor.dragging = None;
        input.editor.track = None;
        input.buttons.clear();
        input.keys.clear();
        input.wheel.clear();
        return;
    }
    let (window_entity, window) = *input.window;
    let scale = window.physical_size().as_vec2() / Vec2::new(800.0, 600.0);
    let point = window
        .cursor_position()
        .map(|point| (point * window.scale_factor()).trunc());
    for event in input.buttons.read() {
        if event.window != window_entity || event.button != MouseButton::Left {
            continue;
        }
        let Some(point) = point else {
            continue;
        };
        if input.editor.display.previous.is_some() {
            if event.state == ButtonState::Released {
                input.editor.request = display::confirmation_request(&input.data, point / scale);
            }
            continue;
        }
        if event.state == ButtonState::Released {
            let editor = &mut *input.editor;
            if editor
                .display
                .click(&input.data, point / scale, &mut editor.draft.resolution)
            {
                continue;
            }
        }
        if event.state == ButtonState::Pressed {
            for slider in Slider::ALL {
                let area = slider::scale(slider.area(&input.data), scale);
                let thumb = slider::thumb(area, slider.value(&input.editor.draft), slider.bounds());
                if thumb.contains(point) {
                    input.editor.dragging = Some((slider, point.x - thumb.min.x));
                    input.editor.focused = Some(slider);
                } else if area.contains(point) {
                    input.editor.track = Some(slider);
                    input.editor.focused = Some(slider);
                }
            }
        } else {
            if input.editor.dragging.take().is_none()
                && let Some(slider) = input.editor.track.take()
            {
                let area = slider::scale(slider.area(&input.data), scale);
                let value = slider::page(
                    area,
                    slider.value(&input.editor.draft),
                    point.x,
                    slider.bounds(),
                );
                slider.set(&mut input.editor.draft, value);
            }
            input.editor.track = None;
        }
    }
    if let Some((slider, offset)) = input.editor.dragging
        && let Some(point) = point
    {
        if input.held.pressed(MouseButton::Left) {
            let value = slider::drag(
                slider::scale(slider.area(&input.data), scale),
                point.x - offset,
                point.x,
                slider.bounds(),
            );
            slider.set(&mut input.editor.draft, value);
        } else {
            input.editor.dragging = None;
        }
    }
    for event in input.wheel.read() {
        if input.editor.display.open {
            input.editor.display.scroll(event.y < 0.0);
        }
    }
    keyboard(&mut input, window_entity);
}

fn keyboard(input: &mut Input, window_entity: Entity) {
    for event in input.keys.read() {
        if event.window != window_entity {
            continue;
        }
        if event.key_code == KeyCode::Escape && event.state == ButtonState::Released {
            input.editor.request = Some(if input.editor.display.previous.is_some() {
                Request::RevertResolution
            } else {
                Request::Back
            });
            input.key_state.clear_just_pressed(KeyCode::Escape);
            input.key_state.clear_just_released(KeyCode::Escape);
        } else if event.state == ButtonState::Pressed && input.editor.display.open {
            if matches!(event.key_code, KeyCode::ArrowDown | KeyCode::ArrowUp) {
                input
                    .editor
                    .display
                    .scroll(event.key_code == KeyCode::ArrowDown);
            }
        } else if event.state == ButtonState::Pressed
            && input.editor.display.previous.is_none()
            && let Some(slider) = input.editor.focused
        {
            let value = slider.value(&input.editor.draft);
            let [min, max] = slider.bounds();
            // Native horizontal-slider arrows are reversed and move by two.
            if event.key_code == KeyCode::ArrowRight && value > min + 1 {
                slider.set(&mut input.editor.draft, value - 2);
            } else if event.key_code == KeyCode::ArrowLeft && value < max - 1 {
                slider.set(&mut input.editor.draft, value + 2);
            }
        }
    }
}

type BoxView<'a> = (
    &'a BoxPart,
    &'a mut Node,
    &'a mut ImageNode,
    &'a mut Visibility,
);
type ThumbView<'a> = (&'a Thumb, &'a mut Node, &'a mut Visibility);
type CheckView<'a> = (&'a Checkbox, &'a mut Node, &'a mut ImageNode);
type BoxFilter = (Without<Thumb>, Without<Checkbox>, Without<display::Part>);
type ThumbFilter = (Without<BoxPart>, Without<Checkbox>, Without<display::Part>);
type CheckFilter = (
    With<Checkbox>,
    Without<BoxPart>,
    Without<Thumb>,
    Without<display::Part>,
);

#[derive(SystemParam)]
pub struct Presentation<'w, 's> {
    editor: ResMut<'w, Editor>,
    menu: ResMut<'w, Menu>,
    data: Res<'w, UiData>,
    visible: Local<'s, bool>,
    preferences: Option<ResMut<'w, Preferences>>,
    audio: Option<ResMut<'w, SourceAudio>>,
    mouse: Option<ResMut<'w, Mouse>>,
    camera: Option<ResMut<'w, CameraRig>>,
    battle: Option<ResMut<'w, crate::playtest::Match>>,
    window: Single<'w, 's, &'static mut Window, With<PrimaryWindow>>,
    monitors: Query<'w, 's, &'static bevy::window::Monitor>,
    current_monitor: Query<'w, 's, &'static bevy::window::OnMonitor, With<PrimaryWindow>>,
    display_images: Query<'w, 's, (&'static display::Part, &'static mut ImageNode)>,
    display_texts: Query<
        'w,
        's,
        (
            &'static display::Part,
            &'static mut Text,
            &'static mut TextColor,
        ),
    >,
    display_visibility: Query<'w, 's, (&'static display::Part, &'static mut Visibility)>,
    boxes: Query<'w, 's, BoxView<'static>, BoxFilter>,
    thumbs: Query<'w, 's, ThumbView<'static>, ThumbFilter>,
    checks: Query<'w, 's, CheckView<'static>, CheckFilter>,
}

pub fn update(mut view: Presentation) {
    if !requests(&mut view) {
        return;
    }
    if !view.menu.options_open && !*view.visible {
        return;
    }
    *view.visible = view.menu.options_open;
    display::present(
        &view.editor,
        view.menu.options_open,
        &view.data,
        &mut view.display_images,
        &mut view.display_texts,
        &mut view.display_visibility,
    );
    let window = &*view.window;
    let scale = window.physical_size().as_vec2() / Vec2::new(800.0, 600.0);
    let cursor = window
        .cursor_position()
        .map(|point| point * window.scale_factor());
    for (part, mut node, mut image, mut visibility) in &mut view.boxes {
        let area = slider::scale(part.slider.area(&view.data), scale);
        let hovered = cursor.is_some_and(|point| area.contains(point));
        let layout = slider::boxes(
            area,
            scale.x,
            view.data.image_size("hilightedbox").x,
            part.slider.value(&view.editor.draft),
            part.slider.bounds(),
        );
        let rect = if part.highlight && hovered {
            layout.highlight(part.index)
        } else if !part.highlight {
            layout.box_rect(part.index)
        } else {
            None
        };
        *visibility = if view.menu.options_open && rect.is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if let Some(rect) = rect {
            *node = at(slider::scale(rect, Vec2::ONE / scale));
            *image = view.data.image(if part.highlight {
                "linebox"
            } else if part.index < layout.selected {
                "hilightedbox"
            } else {
                "dehilightedbox"
            });
        }
    }
    for (part, mut node, mut visibility) in &mut view.thumbs {
        *node = at(slider::scale(
            slider::thumb(
                slider::scale(part.0.area(&view.data), scale),
                part.0.value(&view.editor.draft),
                part.0.bounds(),
            ),
            Vec2::ONE / scale,
        ));
        let hovered = cursor
            .is_some_and(|point| slider::scale(part.0.area(&view.data), scale).contains(point));
        *visibility = if view.menu.options_open && hovered {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for (kind, mut node, mut image) in &mut view.checks {
        let area = slider::scale(view.data.options.area(kind.name()), scale);
        let size = area.height() - 6.0;
        *node = Node {
            position_type: PositionType::Absolute,
            left: percent(0),
            top: percent(3.0 / area.height() * 100.0),
            width: percent(size / area.width() * 100.0),
            height: percent(size / area.height() * 100.0),
            ..default()
        };
        let hovered = cursor.is_some_and(|point| area.contains(point));
        *image = view.data.image(if kind.checked(&view.editor.draft) {
            "Active-Checked"
        } else if hovered {
            "Active-HiLighted"
        } else {
            "Active-Unchecked"
        });
    }
}

fn restore_resolution(
    editor: &mut Editor,
    preferences: &mut Preferences,
    window: &mut Window,
) -> bool {
    let Some(previous) = editor.display.previous.take() else {
        return true;
    };
    let mut draft = preferences.draft();
    draft.resolution = previous;
    match preferences.save(&draft) {
        Ok(_) => window
            .resolution
            .set_physical_resolution(previous[0], previous[1]),
        Err(error) => {
            editor.display.previous = Some(previous);
            error!("Unable to revert display options: {error}");
            return false;
        }
    }
    true
}

fn requests(view: &mut Presentation) -> bool {
    if let Some(request) = view.editor.request.take() {
        let Some(preferences) = view.preferences.as_deref_mut() else {
            return false;
        };
        if view.editor.display.previous.is_some()
            && !matches!(request, Request::RevertResolution | Request::KeepResolution)
        {
            return true;
        }
        match request {
            Request::Open => {
                view.editor.draft = preferences.draft();
                let current = view.window.physical_size().to_array();
                view.editor.draft.resolution = current;
                let monitor = view
                    .current_monitor
                    .iter()
                    .next()
                    .and_then(|id| view.monitors.get(id.0).ok());
                view.editor.display.load(
                    monitor,
                    current,
                    view.battle.as_ref().is_none_or(|battle| !battle.started),
                );
                view.editor.focused = None;
                view.menu.options_open = true;
            }
            Request::KeepResolution => {
                view.editor.display.previous = None;
                view.menu.options_open = false;
            }
            Request::Back | Request::RevertResolution => {
                if !restore_resolution(&mut view.editor, preferences, &mut view.window) {
                    return false;
                }
                view.editor.display.open = false;
                view.menu.options_open = false;
                view.editor.dragging = None;
            }
            Request::Defaults => {
                if view.editor.display.previous.is_none() {
                    let resolution = view.editor.draft.resolution;
                    view.editor.draft = preferences.defaults();
                    view.editor.draft.resolution = resolution;
                }
            }
            Request::Alternate => {
                view.editor.draft.alternate_mouse = !view.editor.draft.alternate_mouse;
            }
            Request::Retaliation => view.editor.draft.retaliation = !view.editor.draft.retaliation,
            Request::DoubleClickAttackMove => {
                view.editor.draft.double_click_attack_move =
                    !view.editor.draft.double_click_attack_move;
            }
            Request::Accept => {
                view.editor.display.open = false;
                view.editor.dragging = None;
                view.editor.track = None;
                if !view.editor.display.enabled {
                    view.editor.draft.resolution = view.window.physical_size().to_array();
                }
                match preferences.save(&view.editor.draft) {
                    Ok(values) => {
                        if let Some(audio) = view.audio.as_deref_mut() {
                            audio.apply_volumes(
                                values.music,
                                values.sound,
                                values.sound3d,
                                values.voice,
                            );
                            audio.apply_money_transaction_volume(values.money_transaction_volume);
                        }
                        if let Some(mouse) = view.mouse.as_deref_mut() {
                            mouse.set_alternate_mouse(values.alternate_mouse);
                            mouse.set_double_click_attack_move(values.double_click_attack_move);
                        }
                        if let Some(camera) = view.camera.as_deref_mut() {
                            camera.settings.scroll_factor = values.scroll_factor;
                        }
                        if let Some(battle) = view.battle.as_deref_mut() {
                            battle.simulation.set_retaliation(0, values.retaliation);
                        }
                        let previous = view.window.physical_size().to_array();
                        if view.editor.display.enabled && values.resolution != previous {
                            view.window.resolution.set_physical_resolution(
                                values.resolution[0],
                                values.resolution[1],
                            );
                            view.editor.display.previous = Some(previous);
                        } else {
                            view.menu.options_open = false;
                        }
                    }
                    Err(error) => error!("Unable to save original options: {error}"),
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests;
