//! `GadgetTextEntryInput` and `W3DGadgetTextEntryImageDraw` for the original name field.
use super::{Layer, Menu, UiData, at};
use crate::playtest::Match;
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, keyboard::Key},
    prelude::*,
    text::TextLayoutInfo,
    window::{Ime, PrimaryWindow, WindowEvent},
};

#[derive(Component)]
pub struct NameEntry;
#[derive(Component)]
pub struct NameText;
#[derive(Component)]
pub struct Caret;
#[derive(Component)]
pub struct Clip;
#[derive(Component)]
pub struct EntryPart(pub(super) [String; 2]);

#[derive(Resource)]
pub struct Edit {
    focused: bool,
    draw_count: u8,
    composition: String,
    width: i16,
    tiles: Vec<Entity>,
}

pub(super) fn setup(commands: &mut Commands, data: &UiData) {
    commands.insert_resource(Edit {
        focused: false,
        draw_count: 0,
        composition: String::new(),
        width: 0,
        tiles: Vec::new(),
    });
    let area = data.name_entry.window.area;
    let parent = commands
        .spawn((
            NameEntry,
            Button,
            Layer::SkirmishUnderlying,
            at(area),
            ZIndex(35),
        ))
        .id();
    let clip = commands
        .spawn((
            Clip,
            Node {
                left: px(5),
                right: px(5),
                top: px(5),
                position_type: PositionType::Absolute,
                overflow: Overflow::clip(),
                ..default()
            },
            ZIndex(1),
        ))
        .id();
    let label = commands
        .spawn((
            NameText,
            Text::new(&data.preferences.name),
            TextFont {
                font: data.font_handle.clone().into(),
                // Native WND fonts use GDI MulDiv(points, 96, 72).
                font_size: FontSize::Px(
                    (data.name_entry.window.font_size.min(512.0) * 96.0 / 72.0).round(),
                ),
                ..default()
            },
            TextColor(data.name_entry.window.text_color),
            Node {
                position_type: PositionType::Absolute,
                left: px(2),
                ..default()
            },
            TextShadow {
                offset: Vec2::ONE,
                color: data.name_entry.window.text_border,
            },
        ))
        .id();
    commands
        .entity(label)
        .insert(TextLayout::justify(Justify::Left).with_no_wrap());
    commands.entity(clip).add_child(label);
    commands.entity(parent).add_child(clip);
    let caret = commands
        .spawn((
            Caret,
            Node {
                position_type: PositionType::Absolute,
                top: px(3),
                bottom: px(3),
                width: px(2),
                ..default()
            },
            Visibility::Hidden,
            BackgroundColor(data.name_entry.window.text_color),
            ZIndex(2),
        ))
        .id();
    commands.entity(parent).add_child(caret);
}

fn artwork(commands: &mut Commands, data: &UiData, parent: Entity, width: i16) -> Vec<Entity> {
    let left = data.image_size("TextEntryEnabledLeftEnd").as_i16vec2().x;
    let right = data.image_size("TextEntryEnabledRightEnd").as_i16vec2().x;
    let mut parts = Vec::new();
    let mut x = left;
    for (part, limit) in [
        ("RepeatingCenter", width - right),
        (
            "SmallRepeatingCenter",
            width - right
                + data
                    .image_size("TextEntryEnabledSmallRepeatingCenter")
                    .as_i16vec2()
                    .x,
        ),
    ] {
        let size = data
            .image_size(&format!("TextEntryEnabled{part}"))
            .as_i16vec2()
            .x;
        while x + size <= limit {
            parts.push((part, x, size));
            x += size;
        }
    }
    parts.extend([("LeftEnd", 0, left), ("RightEnd", width - right, right)]);
    let mut tiles = Vec::new();
    for (part, x, size) in parts {
        let images = ["Enabled", "Hilite"].map(|state| format!("TextEntry{state}{part}"));
        let child = commands
            .spawn((
                data.image(&images[0]),
                EntryPart(images),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(f32::from(x)),
                    width: px(f32::from(size)),
                    height: percent(100),
                    ..default()
                },
            ))
            .id();
        commands.entity(parent).add_child(child);
        tiles.push(child);
    }
    tiles
}

#[derive(SystemParam)]
pub struct InputEvents<'w, 's> {
    text: MessageReader<'w, 's, WindowEvent>,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
}

pub fn input(
    context: (Option<Res<Match>>, Res<Menu>),
    mut data: ResMut<UiData>,
    mut edit: ResMut<Edit>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    entry: Single<&Interaction, With<NameEntry>>,
    mut events: InputEvents,
) {
    let (battle, menu) = context;
    let menu = menu.into_inner();
    let buttons = buttons.into_inner();
    let keys = keys.into_inner();
    let (window_entity, window) = &mut *events.window;
    if battle.as_ref().is_some_and(|battle| battle.started)
        || !matches!(menu.page, super::super::Page::Skirmish)
        || !window.focused
    {
        edit.focused = false;
        edit.composition.clear();
        window.ime_enabled = false;
        events.text.clear();
        return;
    }
    if buttons.just_pressed(MouseButton::Left) {
        edit.focused = *entry.into_inner() == Interaction::Pressed;
    }
    window.ime_enabled = edit.focused;
    if !edit.focused {
        edit.composition.clear();
    }
    let modified = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let maximum = data.name_entry.max_length - 1;
    // WindowEvent retains OS order across IME and keyboard events. A commit or
    // cancelled preedit must release ordinary input immediately, even when the
    // next key arrives in this same display frame.
    for event in events.text.read() {
        if edit.focused {
            edit_text(
                &mut edit.composition,
                &mut data.preferences.name,
                event,
                *window_entity,
                maximum,
                modified,
            );
        }
    }
}

fn edit_text(
    composition: &mut String,
    name: &mut String,
    event: &WindowEvent,
    window: Entity,
    maximum: usize,
    modified: bool,
) {
    match event {
        WindowEvent::Ime(Ime::Preedit {
            window: target,
            value,
            ..
        }) if *target == window => {
            composition.clone_from(value);
        }
        WindowEvent::Ime(Ime::Commit {
            window: target,
            value,
        }) if *target == window => {
            composition.clear();
            append_bounded(name, value, maximum);
        }
        WindowEvent::Ime(Ime::Disabled { window: target }) if *target == window => {
            composition.clear();
        }
        WindowEvent::KeyboardInput(event)
            if event.window == window
                && composition.is_empty()
                && event.state == ButtonState::Pressed
                && !modified =>
        {
            match event.logical_key {
                Key::Backspace => {
                    name.pop();
                }
                // Skirmish never registers a tab list; winNextTab/winPrevTab return.
                Key::Tab
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::Enter => {}
                _ => {
                    if let Some(value) = &event.text {
                        append_bounded(name, value, maximum);
                    }
                }
            }
        }
        _ => {}
    }
}

fn append_bounded(name: &mut String, value: &str, maximum: usize) {
    let mut count = name.encode_utf16().count();
    for character in value.chars().filter(|character| !character.is_control()) {
        if count + character.len_utf16() > maximum {
            break;
        }
        name.push(character);
        count += character.len_utf16();
    }
}

type CaretView<'w, 's> = Single<
    'w,
    's,
    (
        &'static mut Node,
        &'static mut Visibility,
        &'static mut BackgroundColor,
    ),
    (With<Caret>, Without<NameText>),
>;

#[derive(SystemParam)]
pub struct View<'w, 's> {
    window: Single<'w, 's, &'static mut Window, With<PrimaryWindow>>,
    entry: Single<'w, 's, (Entity, &'static Interaction), With<NameEntry>>,
    label: Single<
        'w,
        's,
        (
            &'static mut Text,
            &'static mut TextColor,
            &'static mut TextShadow,
            &'static mut Node,
            &'static TextLayoutInfo,
        ),
        With<NameText>,
    >,
    caret: CaretView<'w, 's>,
    clip: ClipView<'w, 's>,
    parts: Query<'w, 's, (&'static EntryPart, &'static mut ImageNode)>,
}

type ClipView<'w, 's> =
    Single<'w, 's, &'static mut Node, (With<Clip>, Without<NameText>, Without<Caret>)>;

pub fn presentation(
    context: (Option<Res<Match>>, Res<Menu>, Res<UiData>),
    mut edit: ResMut<Edit>,
    mut commands: Commands,
    view: View,
) {
    let View {
        window,
        entry,
        label,
        caret,
        clip,
        mut parts,
    } = view;
    let (battle, menu, data) = context;
    let menu = menu.into_inner();
    if battle.as_ref().is_some_and(|battle| battle.started)
        || !matches!(menu.page, super::super::Page::Skirmish)
    {
        return;
    }
    let data = data.into_inner();
    let mut window = window.into_inner();
    let (entry, interaction) = entry.into_inner();
    let hovered = *interaction != Interaction::None;
    let source = &data.name_entry;
    let color = if hovered {
        source.hilite
    } else {
        source.window.text_color
    };
    let border = if hovered {
        source.hilite_border
    } else {
        source.window.text_border
    };
    let size =
        source.window.area.size() * Vec2::new(window.width() / 800.0, window.height() / 600.0);
    let width = size.as_i16vec2().x;
    if edit.width != width {
        for tile in edit.tiles.drain(..) {
            commands.entity(tile).despawn();
        }
        edit.tiles = artwork(&mut commands, data, entry, width);
        edit.width = width;
    }
    let (mut label, mut label_color, mut shadow, mut node, layout) = label.into_inner();
    let mut displayed = data.preferences.name.clone();
    append_bounded(&mut displayed, &edit.composition, source.max_length - 1);
    label
        .reborrow()
        .map_unchanged(|label| &mut label.0)
        .set_if_neq(displayed);
    label_color.set_if_neq(TextColor(color));
    shadow.set_if_neq(TextShadow {
        offset: Vec2::ONE,
        color: border,
    });
    let width = i32::from(width) - 10;
    // Bevy UI converts TextLayoutInfo.size back to logical pixels after shaping.
    let measured = layout.size.as_ivec2();
    let offset = text_offset(measured.x, width);
    let positions = IVec2::new(2 - offset, 7 + measured.x - offset).as_vec2();
    window.ime_position = source.window.area.min
        * Vec2::new(window.width() / 800.0, window.height() / 600.0)
        + Vec2::new(positions.y, 3.0 + measured.as_vec2().y);
    node.reborrow()
        .map_unchanged(|node| &mut node.left)
        .set_if_neq(px(positions.x));
    clip.into_inner()
        .map_unchanged(|node| &mut node.height)
        .set_if_neq(px(measured.as_vec2().y));
    let (mut caret, mut visible, mut background) = caret.into_inner();
    caret
        .reborrow()
        .map_unchanged(|node| &mut node.left)
        .set_if_neq(px(positions.y));
    background.set_if_neq(BackgroundColor(color));
    visible.set_if_neq(if edit.focused && (edit.draw_count >> 3) & 1 != 0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if edit.focused {
        edit.draw_count = edit.draw_count.wrapping_add(1);
    }
    for (part, mut image) in &mut parts {
        crate::playtest_ui::set_image(&mut image, data.image(&part.0[usize::from(hovered)]));
    }
}

fn text_offset(text_width: i32, width: i32) -> i32 {
    let half = (width / 2).max(1);
    if text_width < width {
        0
    } else {
        (text_width / half - 1) * half
    }
}

#[cfg(test)]
mod tests {
    use super::{append_bounded, edit_text, text_offset};
    use bevy::{
        input::{
            ButtonState,
            keyboard::{Key, KeyboardInput},
        },
        prelude::{Entity, KeyCode},
        window::{Ime, WindowEvent},
    };

    fn key(window: Entity, text: Option<&str>) -> WindowEvent {
        WindowEvent::KeyboardInput(KeyboardInput {
            key_code: if text.is_some() {
                KeyCode::KeyA
            } else {
                KeyCode::Backspace
            },
            logical_key: text.map_or(Key::Backspace, |value| Key::Character(value.into())),
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window,
        })
    }

    #[test]
    fn ordered_ime_commit_and_cancel_release_keyboard_input_in_the_same_frame() {
        let window = Entity::from_bits(1);
        let mut name = "P".to_owned();
        let mut composition = String::new();
        let events = [
            key(window, Some("A")),
            WindowEvent::Ime(Ime::Preedit {
                window,
                value: "仮".into(),
                cursor: Some((0, 3)),
            }),
            key(window, None),
            WindowEvent::Ime(Ime::Preedit {
                window,
                value: String::new(),
                cursor: None,
            }),
            WindowEvent::Ime(Ime::Commit {
                window,
                value: "文".into(),
            }),
            // Winit suppresses the committing key itself. A later ordinary key
            // is independent even when its text equals the committed character.
            key(window, Some("文")),
            WindowEvent::Ime(Ime::Preedit {
                window,
                value: "消".into(),
                cursor: Some((0, 3)),
            }),
            WindowEvent::Ime(Ime::Preedit {
                window,
                value: String::new(),
                cursor: None,
            }),
            key(window, None),
            key(window, Some("Z")),
        ];
        for event in &events {
            edit_text(&mut composition, &mut name, event, window, 11, false);
        }
        assert_eq!(name, "PA文Z");
        assert!(composition.is_empty());
    }

    #[test]
    fn composition_replacement_disabled_and_foreign_window_keep_committed_text_exact() {
        let window = Entity::from_bits(1);
        let other = Entity::from_bits(2);
        let mut name = "Player".to_owned();
        let mut composition = String::new();
        for value in ["あ", "候補"] {
            edit_text(
                &mut composition,
                &mut name,
                &WindowEvent::Ime(Ime::Preedit {
                    window,
                    value: value.to_owned(),
                    cursor: Some((0, 3)),
                }),
                window,
                11,
                false,
            );
            assert_eq!(name, "Player");
            assert_eq!(composition, value);
        }
        for event in [
            WindowEvent::Ime(Ime::Commit {
                window: other,
                value: "別".into(),
            }),
            key(other, Some("X")),
        ] {
            edit_text(&mut composition, &mut name, &event, window, 11, false);
        }
        assert_eq!(composition, "候補");
        edit_text(
            &mut composition,
            &mut name,
            &WindowEvent::Ime(Ime::Disabled { window }),
            window,
            11,
            false,
        );
        edit_text(
            &mut composition,
            &mut name,
            &key(window, Some("🚀X")),
            window,
            11,
            false,
        );
        assert_eq!(name, "Player🚀X");
        assert!(composition.is_empty());
    }

    #[test]
    fn ime_commit_respects_remaining_utf16_capacity_without_splitting_a_scalar() {
        let window = Entity::from_bits(1);
        let mut name = "123456789".to_owned();
        let mut composition = "候補".to_owned();
        edit_text(
            &mut composition,
            &mut name,
            &WindowEvent::Ime(Ime::Commit {
                window,
                value: "🚀X".into(),
            }),
            window,
            11,
            false,
        );
        assert_eq!(name, "123456789🚀");
        assert!(composition.is_empty());
    }

    #[test]
    fn original_scroll_uses_integer_half_width_steps() {
        assert_eq!(text_offset(134, 135), 0);
        assert_eq!(text_offset(135, 135), 67);
        assert_eq!(text_offset(201, 135), 134);
    }

    #[test]
    fn native_limit_counts_utf16_units_and_backspace_preserves_valid_text() {
        let mut name = "123456789".to_owned();
        append_bounded(&mut name, "🚀X", 11);
        assert_eq!(name, "123456789🚀");
        name.pop();
        append_bounded(&mut name, "AB\nC", 11);
        assert_eq!(name, "123456789AB");
    }
}
