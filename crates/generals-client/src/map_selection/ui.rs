use super::MapSelection;
use crate::{
    control_bar::UiData,
    playtest_ui::{Action, Layer, ReleaseButton, at, menu_button, text},
};
use bevy::prelude::*;

pub(super) const ROWS: usize = 9;
pub(super) const ROW_HEIGHT: f32 = 16.0;

#[derive(Component)]
pub enum Part {
    Row(usize),
    Preview,
    Start(usize),
    Supply(usize),
    Tech(usize),
    Thumb,
    Arrow(isize),
    Track,
    Radio(bool, &'static str),
}

pub fn setup(commands: &mut Commands, data: &UiData) {
    let window = |name: &str| data.windows[&format!("SkirmishMapSelectMenu.wnd:{name}")];
    commands.spawn((
        Layer::MapSelect,
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        Node {
            border: UiRect::all(px(1)),
            ..at(window("WinGadgetBorder"))
        },
        ZIndex(46),
    ));
    commands.spawn((
        Layer::MapSelect,
        text(data.label("GUI:SelectAMapLowerCase"), 14.0),
        at(window("")),
        ZIndex(47),
    ));
    for (action, name, label) in [
        (Action::MapAccept, "ButtonOK", "GUI:Accept"),
        (Action::MapBack, "ButtonBack", "GUI:CancelMapSelect"),
    ] {
        menu_button(
            commands,
            data,
            action,
            Layer::MapSelect,
            window(name),
            data.label(label),
        );
    }
    radios(commands, data);
    list(commands, data);
    let preview = window("WinMapPreview");
    commands.spawn((
        Layer::MapSelect,
        at(preview),
        BackgroundColor(Color::BLACK),
        ZIndex(47),
    ));
    commands.spawn((
        Part::Preview,
        Layer::MapSelect,
        ImageNode::default(),
        Node::default(),
        ZIndex(48),
    ));
    for start in 0..8 {
        commands.spawn((
            Part::Start(start),
            Layer::MapSelect,
            data.image("PlayerStartDisabled"),
            Node::default(),
            ZIndex(49),
        ));
    }
}

fn radios(commands: &mut Commands, data: &UiData) {
    let window = |name: &str| data.windows[&format!("SkirmishMapSelectMenu.wnd:{name}")];
    for (user, name, label) in [
        (false, "RadioButtonSystemMaps", "GUI:SystemMaps"),
        (true, "RadioButtonUserMaps", "GUI:UserMaps"),
    ] {
        let area = window(name);
        let parent = commands
            .spawn((
                Layer::MapSelect,
                Button,
                Action::MapCategory(user),
                ReleaseButton { armed: false },
                at(area),
                ZIndex(48),
            ))
            .id();
        let left = data.image_size("RadioButtonEnabledLeft").x;
        let right = data.image_size("RadioButtonEnabledRight").x;
        for (part, start, width) in [
            ("Left", 0.0, left),
            ("Middle", left, area.width() - left - right),
            ("Right", area.width() - right, right),
        ] {
            let child = commands
                .spawn((
                    Part::Radio(user, part),
                    data.image(&format!("RadioButtonEnabled{part}")),
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(start / area.width() * 100.0),
                        width: percent(width / area.width() * 100.0),
                        height: percent(100),
                        ..default()
                    },
                    ZIndex(1),
                ))
                .id();
            commands.entity(parent).add_child(child);
        }
        let label = commands
            .spawn((
                text(data.label(label), 13.0),
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                ZIndex(2),
            ))
            .id();
        commands.entity(parent).add_child(label);
    }
}

fn list(commands: &mut Commands, data: &UiData) {
    let window = |name: &str| data.windows[&format!("SkirmishMapSelectMenu.wnd:{name}")];
    let list = window("ListboxMap");
    commands.spawn((
        Layer::MapSelect,
        Node {
            border: UiRect::all(px(1)),
            ..at(list)
        },
        BackgroundColor(Color::srgba_u8(0, 0, 0, 126)),
        BorderColor::all(Color::srgb_u8(49, 55, 168)),
        ZIndex(47),
    ));
    let content = commands
        .spawn((
            Layer::MapSelect,
            Node {
                overflow: Overflow::clip(),
                ..at(Rect::new(
                    list.min.x + 2.0,
                    list.min.y + 2.0,
                    list.max.x - 23.0,
                    list.max.y - 2.0,
                ))
            },
            ZIndex(48),
        ))
        .id();
    rows(commands, data, list, content);
    scrollbar(commands, data, list);
}

fn rows(commands: &mut Commands, data: &UiData, list: Rect, content: Entity) {
    for row in 0..=ROWS {
        let top = f32::from(u16::try_from(row).expect("original visible rows")) * ROW_HEIGHT;
        let parent = commands
            .spawn((
                // GadgetListBoxInput selects on GWM_LEFT_UP.
                Part::Row(row),
                Layer::MapSelect,
                Button,
                Action::MapRow(0),
                ReleaseButton { armed: false },
                data.image("ListBoxHiliteSelectedItemRepeatingCenter"),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(0),
                    top: percent(top / (list.height() - 4.0) * 100.0),
                    width: percent(100),
                    height: percent(ROW_HEIGHT / (list.height() - 4.0) * 100.0),
                    ..default()
                },
                ZIndex(48),
            ))
            .id();
        commands.entity(content).add_child(parent);
        let label = commands
            .spawn((
                text("", 13.0),
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(5),
                    width: percent(95),
                    height: percent(100),
                    overflow: Overflow::clip(),
                    ..default()
                },
                ZIndex(1),
            ))
            .insert(TextLayout::justify(Justify::Left).with_no_wrap())
            .id();
        commands.entity(parent).add_child(label);
    }
}

fn scrollbar(commands: &mut Commands, data: &UiData, list: Rect) {
    for (delta, top, image) in [
        (-1, list.min.y + 2.0, "VSliderUpButtonEnabled"),
        (1, list.max.y - 24.0, "VSliderDownButtonEnabled"),
    ] {
        commands.spawn((
            Layer::MapSelect,
            Button,
            Action::MapScroll(delta),
            Part::Arrow(delta),
            ReleaseButton { armed: false },
            data.image(image),
            at(Rect::new(
                list.max.x - 23.0,
                top,
                list.max.x - 2.0,
                top + 22.0,
            )),
            ZIndex(48),
        ));
    }
    // Listbox sliders use W3D's colored track draw, with the WND's transparent
    // fill and blue border; their thumb uses the authored ScrollBarThumb images.
    commands.spawn((
        Part::Track,
        Layer::MapSelect,
        ImageNode {
            color: Color::NONE,
            ..default()
        },
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        Node::default(),
        ZIndex(49),
    ));
    commands.spawn((
        Part::Thumb,
        Layer::MapSelect,
        data.image("ScrollBarThumbEnabled")
            .with_mode(NodeImageMode::Stretch),
        Node::default(),
        ZIndex(50),
    ));
}

type PartView = (
    Entity,
    &'static Part,
    &'static mut Node,
    &'static mut Visibility,
    &'static mut ImageNode,
    Option<&'static mut Action>,
    Option<&'static Children>,
    Option<&'static Interaction>,
    Option<&'static mut BorderColor>,
);

pub fn update(
    mut commands: Commands,
    selection: Option<ResMut<MapSelection>>,
    data: Res<UiData>,
    mut keys: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut parts: Query<PartView>,
    mut labels: Query<(&mut Text, &mut TextColor)>,
    presentation: (
        Single<&Window, With<bevy::window::PrimaryWindow>>,
        Res<ButtonInput<MouseButton>>,
    ),
) {
    let data = data.into_inner();
    let Some(mut selection) = selection else {
        return;
    };
    for event in keys.read() {
        selection.key(event.key_code, event.state);
    }
    let selected = selection
        .highlighted
        .and_then(|index| selection.maps.get(index));
    let window = data.windows["SkirmishMapSelectMenu.wnd:WinMapPreview"];
    sync_markers(&mut commands, data, selected, &parts);
    let start_rects = selected.map(|map| {
        super::preview::starts(
            window,
            map.extent,
            &map.starts,
            data.windows["SkirmishMapSelectMenu.wnd:ButtonMapStartPosition0"].size(),
        )
    });
    let (view, held) = presentation;
    let view = view.into_inner();
    let held = held.into_inner();
    let cursor = view
        .cursor_position()
        .map(|point| point * Vec2::new(800.0 / view.width(), 600.0 / view.height()));
    for (_, part, mut node, mut visibility, mut image, action, children, interaction, border) in
        &mut parts
    {
        *visibility = Visibility::Hidden;
        if !selection.open {
            continue;
        }
        match *part {
            Part::Row(row) => {
                if render_row(
                    row,
                    &selection,
                    data,
                    (&mut node, &mut image),
                    (action, children),
                    &mut labels,
                ) {
                    *visibility = Visibility::Visible;
                }
            }
            Part::Preview | Part::Start(_) | Part::Supply(_) | Part::Tech(_) => {
                if preview_part(
                    part,
                    selected,
                    data,
                    &mut node,
                    &mut image,
                    start_rects.as_deref().unwrap_or(&[]),
                ) {
                    *visibility = Visibility::Visible;
                }
            }
            Part::Thumb | Part::Track => {
                scrollbar_part(part, data, &selection, &mut node, (cursor, border));
                scrollbar_image(
                    part,
                    data,
                    &selection,
                    (cursor, held.pressed(MouseButton::Left), interaction),
                    &mut image,
                );
                *visibility = Visibility::Visible;
            }
            Part::Arrow(delta) => {
                scrollbar_image(
                    &Part::Arrow(delta),
                    data,
                    &selection,
                    (cursor, held.pressed(MouseButton::Left), interaction),
                    &mut image,
                );
                *visibility = Visibility::Visible;
            }
            Part::Radio(user, part) => {
                let state = if selection.user_maps == user {
                    "Hilighted"
                } else {
                    "Enabled"
                };
                *image = data.image(&format!("RadioButton{state}{part}"));
                *visibility = Visibility::Visible;
            }
        }
    }
}

fn render_row(
    row: usize,
    selection: &MapSelection,
    data: &UiData,
    (node, image): (&mut Node, &mut ImageNode),
    (action, children): (Option<Mut<Action>>, Option<&Children>),
    labels: &mut Query<(&mut Text, &mut TextColor)>,
) -> bool {
    let list = data.windows["SkirmishMapSelectMenu.wnd:ListboxMap"];
    let top_pixels =
        f32::from(u16::try_from(selection.top).expect("original list capacity")) * ROW_HEIGHT;
    let offset = selection.display_pos - top_pixels;
    let row_pixels = f32::from(u16::try_from(row).expect("visible row")) * ROW_HEIGHT;
    node.top = percent((row_pixels - offset) / (list.height() - 4.0) * 100.0);
    let visible_row = selection.top + row;
    let Some(index) = selection.index(visible_row) else {
        return false;
    };
    let Some(map) = selection.maps.get(index) else {
        return false;
    };
    if let Some(mut action) = action {
        *action = Action::MapRow(visible_row);
    }
    image.color = if selection.highlighted == Some(index) {
        Color::WHITE
    } else {
        Color::NONE
    };
    if let Some(children) = children {
        for child in children.iter() {
            if let Ok((mut text, mut color)) = labels.get_mut(child) {
                text.0.clone_from(&map.title);
                color.0 = if map.admission.rejection().is_none() {
                    Color::srgb_u8(254, 254, 254)
                } else {
                    Color::srgb_u8(192, 192, 192)
                };
            }
        }
    }
    true
}

fn sync_markers(
    commands: &mut Commands,
    data: &UiData,
    selected: Option<&super::MapEntry>,
    parts: &Query<PartView>,
) {
    let counts = selected.map_or((0, 0), |map| (map.supplies.len(), map.tech.len()));
    let existing = parts.iter().fold((0, 0), |(supply, tech), (_, part, ..)| {
        (
            supply + usize::from(matches!(part, Part::Supply(_))),
            tech + usize::from(matches!(part, Part::Tech(_))),
        )
    });
    if counts != existing {
        for (entity, part, ..) in parts.iter() {
            if matches!(part, Part::Supply(_) | Part::Tech(_)) {
                commands.entity(entity).despawn();
            }
        }
        for (count, supply) in [(counts.1, false), (counts.0, true)] {
            for index in (0..count).rev() {
                if let Some(image) =
                    data.optional_image(if supply { "Cash" } else { "TecBuilding" })
                {
                    commands.spawn((
                        if supply {
                            Part::Supply(index)
                        } else {
                            Part::Tech(index)
                        },
                        Layer::MapSelect,
                        image.with_mode(NodeImageMode::Stretch),
                        Node::default(),
                        Visibility::Hidden,
                        ZIndex(48),
                    ));
                }
            }
        }
    }
}

fn preview_part(
    part: &Part,
    selected: Option<&super::MapEntry>,
    data: &UiData,
    node: &mut Node,
    image: &mut ImageNode,
    starts: &[Rect],
) -> bool {
    let Some(map) = selected else {
        return false;
    };
    let window = data.windows["SkirmishMapSelectMenu.wnd:WinMapPreview"];
    let bounds = crate::radar::projection::fit(window, map.extent);
    match *part {
        Part::Preview => {
            let preview = data
                .handles
                .get(&map.preview)
                .map(|handle| ImageNode::new(handle.clone()).with_mode(NodeImageMode::Stretch))
                .or_else(|| data.optional_image("UnknownMap"));
            let Some(preview) = preview else {
                return false;
            };
            *image = preview;
            *node = at(bounds);
        }
        Part::Start(index) => {
            let Some(&rect) = starts.get(index) else {
                return false;
            };
            *node = at(rect);
        }
        Part::Supply(index) | Part::Tech(index) => {
            let positions = if matches!(part, Part::Supply(_)) {
                &map.supplies
            } else {
                &map.tech
            };
            let Some(&position) = positions.get(index) else {
                return false;
            };
            *node = at(super::preview::marker(
                bounds,
                map.extent,
                position,
                Vec2::splat(super::preview::MARKER_SIZE),
            ));
        }
        _ => return false,
    }
    true
}

fn scrollbar_image(
    part: &Part,
    data: &UiData,
    selection: &MapSelection,
    pointer: (Option<Vec2>, bool, Option<&Interaction>),
    image: &mut ImageNode,
) {
    let (cursor, held, interaction) = pointer;
    match *part {
        Part::Thumb => {
            let list = data.windows["SkirmishMapSelectMenu.wnd:ListboxMap"];
            let track = super::pointer::track(list);
            let max = super::pointer::maximum(selection.count(), list);
            let thumb = super::pointer::thumb(track, selection.display_pos, max);
            let hovered = cursor.is_some_and(|cursor| thumb.contains(cursor));
            let state = if hovered && held {
                "HiliteSelected"
            } else if hovered {
                "Hilite"
            } else {
                "Enabled"
            };
            *image = data.image(&format!("ScrollBarThumb{state}"));
        }
        Part::Arrow(delta) => {
            let direction = if delta < 0 { "Up" } else { "Down" };
            let state = match interaction {
                Some(Interaction::Pressed) => "HiliteSelected",
                Some(Interaction::Hovered) => "Hilite",
                _ => "Enabled",
            };
            *image = data.image(&format!("VSlider{direction}Button{state}"));
        }
        _ => {}
    }
}

fn scrollbar_part(
    part: &Part,
    data: &UiData,
    selection: &MapSelection,
    node: &mut Node,
    (cursor, border): (Option<Vec2>, Option<Mut<BorderColor>>),
) {
    let list = data.windows["SkirmishMapSelectMenu.wnd:ListboxMap"];
    let track = super::pointer::track(list);
    if let Some(mut border) = border {
        let hovered = cursor.is_some_and(|point| track.contains(point));
        *border = BorderColor::all(if hovered {
            Color::srgb_u8(49, 55, 168)
        } else {
            Color::srgb_u8(47, 55, 168)
        });
    }
    let max = super::pointer::maximum(selection.count(), list);
    let rect = if matches!(part, Part::Track) {
        track
    } else {
        super::pointer::thumb(track, selection.display_pos, max)
    };
    *node = at(rect);
    if matches!(part, Part::Track) {
        node.border = UiRect::all(Val::Px(1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_chooser_setup_applies_bundles_and_preserves_label_layouts() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let mut data =
            crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let mut maps = super::super::load(&assets, &mut data, crate::playtest::MAP_NAME).unwrap();
        let mut images = Assets::<Image>::default();
        for (name, image) in std::mem::take(&mut data.textures) {
            data.handles.insert(name, images.add(image));
        }
        maps.open = true;
        maps.highlighted = Some(maps.current);
        let expected = (
            maps.maps[maps.current].supplies.len(),
            maps.maps[maps.current].tech.len(),
        );
        assert!(
            expected.0 > 0,
            "installed Tournament Desert cache must show supply sources"
        );
        let title = data.label("GUI:SelectAMapLowerCase").to_owned();
        let mut world = World::new();
        world.insert_resource(data);
        world.insert_resource(images);
        world.insert_resource(maps);
        world.insert_resource(Messages::<bevy::input::keyboard::KeyboardInput>::default());
        world.insert_resource(ButtonInput::<MouseButton>::default());
        world.spawn((Window::default(), bevy::window::PrimaryWindow));
        // RunSystemOnce applies deferred Commands, exercising every actual spawn bundle.
        world
            .run_system_once(|mut commands: Commands, data: Res<UiData>| {
                setup(&mut commands, data.into_inner());
            })
            .unwrap();
        let mut rows = world.query::<(&Part, &Children)>();
        let labels: Vec<Entity> = rows
            .iter(&world)
            .filter(|(part, _)| matches!(part, Part::Row(_)))
            .map(|(_, children)| children[0])
            .collect();
        assert_eq!(labels.len(), ROWS + 1);
        for label in labels {
            let layout = world.get::<TextLayout>(label).unwrap();
            assert_eq!(layout.justify, Justify::Left);
            assert_eq!(layout.linebreak, bevy::text::LineBreak::NoWrap);
            assert_eq!(world.get::<Text>(label).unwrap().0, "");
            assert_eq!(world.get::<Node>(label).unwrap().overflow, Overflow::clip());
        }
        check_markers(&mut world, expected);
        let mut texts = world.query::<(&Text, &TextLayout)>();
        let title_layout = texts
            .iter(&world)
            .find_map(|(text, layout)| (text.0 == title).then_some(layout))
            .unwrap();
        assert_eq!(title_layout.justify, Justify::Center);
        for key in [
            "GUI:SystemMaps",
            "GUI:UserMaps",
            "GUI:Accept",
            "GUI:CancelMapSelect",
        ] {
            let value = world.resource::<UiData>().label(key);
            let layout = texts
                .iter(&world)
                .find_map(|(text, layout)| (text.0 == value).then_some(layout))
                .unwrap();
            assert_eq!(layout.justify, Justify::Center);
        }
    }

    fn check_scrollbar(world: &mut World) {
        let original = world.resource::<UiData>().image("ScrollBarThumbEnabled");
        let mut parts = world.query::<(&Part, &Node, &ImageNode, Option<&BorderColor>)>();
        let mut tracks = 0;
        let mut thumbs = 0;
        for (part, node, image, border) in parts.iter(world) {
            match part {
                Part::Track => {
                    tracks += 1;
                    assert_eq!(image.color, Color::NONE);
                    assert_eq!(image.image, ImageNode::default().image);
                    assert_eq!(node.border, UiRect::all(Val::Px(1.0)));
                    assert_eq!(
                        *border.unwrap(),
                        BorderColor::all(Color::srgb_u8(47, 55, 168))
                    );
                }
                Part::Thumb => {
                    thumbs += 1;
                    assert_eq!(image.image, original.image);
                    assert_eq!(image.rect, original.rect);
                    assert_eq!(image.image_mode, NodeImageMode::Stretch);
                    assert_eq!(node.width, percent(21.0 / 800.0 * 100.0));
                    assert_eq!(node.height, percent(22.0 / 600.0 * 100.0));
                }
                _ => {}
            }
        }
        assert_eq!((tracks, thumbs), (1, 1));
    }

    fn check_markers(world: &mut World, expected: (usize, usize)) {
        world.run_system_once(update).unwrap();
        world.run_system_once(update).unwrap();
        check_scrollbar(world);
        let cash = world.resource::<UiData>().image("Cash");
        let tech = world.resource::<UiData>().image("TecBuilding");
        let mut parts = world.query::<(&Part, &Node, &ImageNode, &Visibility)>();
        let mut counts = (0, 0);
        for (part, node, image, visibility) in parts.iter(world) {
            let original = match part {
                Part::Supply(_) => {
                    counts.0 += 1;
                    &cash
                }
                Part::Tech(_) => {
                    counts.1 += 1;
                    &tech
                }
                _ => continue,
            };
            assert_eq!(image.image, original.image);
            assert_eq!(image.rect, original.rect);
            assert_eq!(image.image_mode, NodeImageMode::Stretch);
            assert_eq!(node.width, percent(15.0 / 800.0 * 100.0));
            assert_eq!(node.height, percent(15.0 / 600.0 * 100.0));
            assert_eq!(*visibility, Visibility::Visible);
        }
        assert_eq!(counts, expected);
        let entities: Vec<_> = world
            .query_filtered::<Entity, With<Part>>()
            .iter(world)
            .collect();
        world.run_system_once(update).unwrap();
        assert_eq!(
            world
                .query_filtered::<Entity, With<Part>>()
                .iter(world)
                .collect::<Vec<_>>(),
            entities
        );
        world
            .resource_mut::<MapSelection>()
            .activate(&Action::MapCategory(true));
        world.run_system_once(update).unwrap();
        assert!(
            !world
                .query::<&Part>()
                .iter(world)
                .any(|part| matches!(part, Part::Supply(_) | Part::Tech(_)))
        );
    }
}
