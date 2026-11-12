//! `GAME_SKIRMISH` uses `MultiPlayerLoadScreen`, including its local-only branch.
use super::{Menu, UiData};
use crate::playtest_ui::at;
use crate::{control_bar::loading::Window as SourceWindow, playtest::startup::Shell};
use bevy::{prelude::*, window::PrimaryWindow};
use generals_sim::skirmish::Faction;

#[derive(Component)]
pub(super) struct Loading;

#[derive(Component)]
pub(super) struct Progress;

#[derive(Resource)]
pub(super) struct Display {
    shown: bool,
    completed: usize,
    size: Vec2,
}

pub(super) fn setup(commands: &mut Commands, data: &mut UiData, fonts: &mut Assets<Font>) {
    if let Some(font) = data.loading.bold_font.take() {
        data.loading.bold_handle = fonts.add(font);
    }
    commands.insert_resource(Display {
        shown: false,
        completed: usize::MAX,
        size: Vec2::ZERO,
    });
}

const fn template(faction: Faction) -> &'static str {
    match faction {
        Faction::Usa => "FactionAmerica",
        Faction::China => "FactionChina",
        Faction::Gla => "FactionGLA",
        Faction::Civilian => panic!("local loading screen requires a playable faction"),
    }
}

const fn apparent<T: Copy>(
    selected: Option<T>,
    resolved: Option<T>,
    local_ally: bool,
    show_random: bool,
) -> Option<T> {
    if show_random && !local_ally {
        selected
    } else {
        resolved
    }
}

fn ally(menu: &Menu, slot: usize) -> bool {
    slot == 0 || menu.teams[slot].is_some_and(|team| menu.teams[0] == Some(team))
}

fn rows(menu: &Menu) -> Vec<usize> {
    (0..menu.capacity)
        .filter(|&slot| slot == 0 || menu.opponents[slot].difficulty().is_some())
        .collect()
}

fn font_pixels(authored_points: f32, width: f32) -> f32 {
    // GlobalLanguage::ClassicNoCeiling retains separate binary32 operations.
    let mut adjustment = width / 800.0;
    adjustment -= 1.0;
    adjustment *= 0.7;
    adjustment += 1.0;
    let points = (authored_points * adjustment.max(1.0)).floor().min(512.0);
    // FontCharsClass uses GDI MulDiv(points, 96, 72).
    (points * 96.0 / 72.0).round()
}

fn label(
    commands: &mut Commands,
    data: &UiData,
    source: &SourceWindow,
    value: &str,
    color: Color,
    width: f32,
) {
    let layout = &source.layout;
    let parent = commands
        .spawn((
            Loading,
            Name::new(format!("MultiplayerLoadScreen.wnd:{}", source.name)),
            Node {
                align_items: AlignItems::Center,
                justify_content: if layout.centered {
                    JustifyContent::Center
                } else {
                    JustifyContent::FlexStart
                },
                padding: UiRect {
                    left: px(if layout.centered { 5.0 } else { 7.0 }),
                    right: px(if layout.centered { 5.0 } else { 3.0 }),
                    ..default()
                },
                overflow: Overflow::clip(),
                ..at(layout.area)
            },
            ZIndex(74),
        ))
        .id();
    let font = if source.bold {
        &data.loading.bold_handle
    } else {
        &data.font_handle
    };
    let child = commands
        .spawn((
            Text::new(value),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(font_pixels(layout.font_size, width)),
                ..default()
            },
            TextColor(color),
            TextShadow {
                offset: Vec2::ONE,
                color: layout.text_border,
            },
            TextLayout::justify(if layout.wrap_centered {
                Justify::Center
            } else {
                Justify::Left
            }),
            Node {
                max_width: percent(100),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(child);
}

fn image(commands: &mut Commands, data: &UiData, name: &str, area: Rect, layer: i32) {
    commands.spawn((
        Loading,
        Name::new(name.to_owned()),
        data.image(name),
        at(area),
        ZIndex(layer),
    ));
}

fn present(
    commands: &mut Commands,
    data: &UiData,
    menu: &Menu,
    slots: &generals_sim::skirmish::ResolvedSlots,
    map: &crate::map_selection::MapEntry,
    size: Vec2,
) {
    let general = &data.loading.generals[template(slots.factions[0])];
    for window in &data.loading.windows {
        let source = &window.layout;
        if window.name == "ParentMultiplayerLoadScreen" {
            image(
                commands,
                data,
                source.image.as_deref().expect("loading background"),
                source.area,
                70,
            );
        } else if window.name.is_empty() && !window.see_through {
            commands.spawn((
                Loading,
                at(source.area),
                BackgroundColor(source.background),
                BorderColor::all(source.border),
                ZIndex(71),
            ));
        } else if window.name.ends_with("Title") {
            label(
                commands,
                data,
                window,
                &source.text,
                source.text_color,
                size.x,
            );
        }
    }
    let portrait = data.loading.window("LocalGeneralPortrait");
    image(commands, data, &general.portrait, portrait.layout.area, 73);
    for (name, value) in [
        ("LocalGeneralName", &general.name),
        ("LocalGeneralFeatures", &general.features),
    ] {
        let source = data.loading.window(name);
        label(
            commands,
            data,
            source,
            value,
            source.layout.text_color,
            size.x,
        );
    }
    for (row, slot) in rows(menu).into_iter().enumerate() {
        let color = apparent(
            menu.colors[slot],
            slots.colors[slot],
            ally(menu, slot),
            data.loading.show_random_color,
        )
        .map_or(Color::WHITE, |index| {
            Color::srgb_from_array(data.results.colors[index])
        });
        let name = if slot == 0 {
            &data.preferences.name
        } else {
            data.label(menu.opponents[slot].label())
        };
        let faction_label = apparent(
            menu.factions[slot],
            Some(slots.factions[slot]),
            ally(menu, slot),
            data.loading.show_random_faction,
        )
        .map_or_else(
            || data.label("GUI:Random"),
            |faction| data.loading.generals[template(faction)].side.as_str(),
        );
        let team = data
            .label(&format!(
                "Team:{}",
                menu.teams[slot].map_or(0, |team| team + 1)
            ))
            .to_owned();
        for (field, value) in [
            ("Player", name),
            ("Side", faction_label),
            ("Team", team.as_str()),
        ] {
            label(
                commands,
                data,
                data.loading.window(&format!("StaticText{field}{row}")),
                value,
                color,
                size.x,
            );
        }
    }
    preview(commands, data, menu, slots, map, size);
}

fn preview(
    commands: &mut Commands,
    data: &UiData,
    menu: &Menu,
    slots: &generals_sim::skirmish::ResolvedSlots,
    map: &crate::map_selection::MapEntry,
    size: Vec2,
) {
    let preview = data.loading.window("WinMapPreview").layout.area;
    let scale = size / Vec2::new(800.0, 600.0);
    let screen = Rect::from_corners((preview.min * scale).trunc(), (preview.max * scale).trunc());
    let fitted = crate::radar::projection::fit(screen, data.map_extent);
    let bounds = Rect::from_corners(fitted.min / scale, fitted.max / scale);
    commands.spawn((
        Loading,
        at(preview),
        BackgroundColor(Color::BLACK),
        ZIndex(71),
    ));
    commands.spawn((
        Loading,
        at(bounds),
        BackgroundColor(Color::srgb_u8(50, 50, 50)),
        ZIndex(72),
    ));
    if let Some(handle) = data.handles.get(&data.map_preview) {
        commands.spawn((
            Loading,
            ImageNode::new(handle.clone()),
            at(bounds),
            ZIndex(73),
        ));
    }
    preview_symbols(commands, data, map, screen, fitted, scale);
    preview_border(commands, data, screen, scale);
    let marker = data.loading.window("ButtonMapStartPosition0");
    let starts = crate::map_selection::preview::starts(
        screen,
        data.map_extent,
        &data.map_starts,
        (marker.layout.area.size() * scale).trunc(),
    );
    for (position, area) in starts.into_iter().enumerate() {
        let area = Rect::from_corners(area.min / scale, area.max / scale);
        image(
            commands,
            data,
            marker.layout.image.as_deref().expect("map start image"),
            area,
            75,
        );
        if let Some(slot) = rows(menu).into_iter().find(|&slot| {
            apparent(
                menu.starts[slot],
                slots.start_positions[slot],
                ally(menu, slot),
                data.loading.show_random_start,
            ) == Some(position)
        }) {
            let text = data.label(&format!("NUMBER:{}", slot + 1)).to_owned();
            let parent = commands
                .spawn((
                    Loading,
                    Node {
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..at(area)
                    },
                    ZIndex(76),
                ))
                .id();
            let child = commands
                .spawn((
                    Text::new(text),
                    TextFont {
                        font: data.font_handle.clone().into(),
                        font_size: FontSize::Px(font_pixels(8.0, size.x)),
                        ..default()
                    },
                    TextColor(marker.layout.text_color),
                    TextShadow {
                        offset: Vec2::ONE,
                        color: marker.layout.text_border,
                    },
                ))
                .id();
            commands.entity(parent).add_child(child);
        }
    }
}

fn preview_symbols(
    commands: &mut Commands,
    data: &UiData,
    map: &crate::map_selection::MapEntry,
    screen: Rect,
    fitted: Rect,
    scale: Vec2,
) {
    let local = Rect::from_corners(fitted.min - screen.min, fitted.max - screen.min);
    for (name, positions) in [("TecBuilding", &map.tech), ("Cash", &map.supplies)] {
        for &position in positions.iter().rev() {
            let relative = position / map.extent;
            // Native SUPPLY_TECH_SIZE / 2 is integer 7, not floating 7.5.
            let start = (Vec2::new(relative.x, 1.0 - relative.y) * local.size() - Vec2::splat(7.0)
                + local.min)
                .trunc()
                + screen.min;
            image(
                commands,
                data,
                name,
                Rect::from_corners(start / scale, (start + Vec2::splat(15.0)) / scale),
                74,
            );
        }
    }
    let horizontal = map.extent.x / screen.width() >= map.extent.y / screen.height();
    for edge in [false, true] {
        let (start, end) = if horizontal {
            let y = if edge {
                fitted.max.y + 1.0
            } else {
                fitted.min.y
            };
            (Vec2::new(screen.min.x, y), Vec2::new(screen.max.x, y + 1.0))
        } else {
            let x = if edge {
                fitted.max.x + 1.0
            } else {
                fitted.min.x
            };
            (Vec2::new(x, screen.min.y), Vec2::new(x + 1.0, screen.max.y))
        };
        commands.spawn((
            Loading,
            at(Rect::from_corners(start / scale, end / scale)),
            BackgroundColor(Color::srgb_u8(50, 50, 50)),
            ZIndex(72),
        ));
    }
}

// W3DControlBar::drawSkinnyBorder: authored five-pixel pieces, two-pixel remainders.
fn preview_border(commands: &mut Commands, data: &UiData, bounds: Rect, scale: Vec2) {
    let low = bounds.min.as_ivec2() - IVec2::ONE;
    let high = bounds.max.as_ivec2() + IVec2::ONE;
    let draw = |commands: &mut Commands, name: &str, start: IVec2, end: IVec2| {
        image(
            commands,
            data,
            name,
            Rect::from_corners(start.as_vec2() / scale, end.as_vec2() / scale),
            74,
        );
    };
    for horizontal in [true, false] {
        let (begin, end, first, second, names) = if horizontal {
            (low.x, high.x, low.y - 2, high.y - 5, ["FrameT", "FrameB"])
        } else {
            (low.y, high.y, low.x - 2, high.x - 5, ["FrameL", "FrameR"])
        };
        let piece = |commands: &mut Commands, position: i32, length: i32| {
            for (cross, name) in [(first, names[0]), (second, names[1])] {
                let (start, extent) = if horizontal {
                    (IVec2::new(position, cross), IVec2::new(length, 5))
                } else {
                    (IVec2::new(cross, position), IVec2::new(5, length))
                };
                draw(commands, name, start, start + extent);
            }
        };
        let mut position = begin + 3;
        while position <= end - 10 {
            piece(commands, position, 5);
            position += 5;
        }
        if end - 5 - position >= 2 {
            piece(commands, position, 2);
            position += 2;
        }
        if position < end - 5 {
            position -= 2 - ((end - 5 - position + 1) & !1);
            piece(commands, position, 2);
        }
    }
    for (name, start) in [
        ("FrameCornerUL", low - IVec2::splat(2)),
        ("FrameCornerUR", IVec2::new(high.x - 5, low.y - 2)),
        ("FrameCornerLL", IVec2::new(low.x - 2, high.y - 5)),
        ("FrameCornerLR", high - IVec2::splat(5)),
    ] {
        draw(commands, name, start, start + IVec2::splat(5));
    }
}

fn progress(commands: &mut Commands, data: &UiData, color: usize, completed: usize, size: Vec2) {
    let source = data.loading.window("ProgressLoad0").layout.area;
    let scale = size / Vec2::new(800.0, 600.0);
    let origin = (source.min * scale).as_ivec2();
    let extent = (source.size() * scale).as_ivec2();
    let image = format!("LoadingBar_ProgressCenter{color}");
    let image = if data.optional_image(&image).is_some() {
        image.as_str()
    } else {
        "LoadingBar_Progress"
    };
    let tile = data.image_size(image).as_ivec2().x;
    let depowered = data.image_size("LoadingBar_DePowered").as_ivec2().x;
    let percent = i32::try_from(completed.min(100)).expect("bounded load percent");
    let count = (extent.x - 20) / tile;
    let filled = ((extent.x - 20) * percent / 100) / tile;
    for index in 0..count {
        let (name, offset, width) = if index < filled {
            (image, index * tile, tile)
        } else {
            (
                "LoadingBar_DePowered",
                filled * tile + (index - filled) * depowered,
                depowered,
            )
        };
        let start = origin + IVec2::new(10 + offset, 5);
        let end = origin + IVec2::new(10 + offset + width, extent.y - 5);
        let area = Rect::from_corners(start.as_vec2() / scale, end.as_vec2() / scale);
        commands.spawn((Loading, Progress, data.image(name), at(area), ZIndex(74)));
    }
}

pub(super) fn update(
    mut commands: Commands,
    resources: (
        Res<UiData>,
        Res<Menu>,
        Res<Shell>,
        Option<Res<crate::map_selection::MapSelection>>,
    ),
    mut display: ResMut<Display>,
    window: Single<&Window, With<PrimaryWindow>>,
    entities: Query<(Entity, Option<&Progress>), With<Loading>>,
) {
    let (data, menu, shell, maps) = resources;
    let window = window.into_inner();
    let size = Vec2::new(window.width(), window.height());
    if !shell.loading() {
        if display.shown {
            for (entity, _) in &entities {
                commands.entity(entity).despawn();
            }
            display.shown = false;
        }
        return;
    }
    let Some(slots) = shell.loading_slots() else {
        return;
    };
    let maps = maps.expect("selected map exists while loading");
    let rebuild = !display.shown || display.size != size;
    if rebuild {
        for (entity, _) in &entities {
            commands.entity(entity).despawn();
        }
        present(
            &mut commands,
            &data,
            &menu,
            slots,
            &maps.maps[maps.current],
            size,
        );
    }
    if rebuild || display.completed != shell.completed() {
        if !rebuild {
            for (entity, progress) in &entities {
                if progress.is_some() {
                    commands.entity(entity).despawn();
                }
            }
        }
        progress(
            &mut commands,
            &data,
            slots.colors[0].expect("local color"),
            shell.completed(),
            size,
        );
    }
    display.shown = true;
    display.completed = shell.completed();
    display.size = size;
}

pub(super) fn ready(world: &mut World) -> bool {
    if !world.resource::<Shell>().loading() {
        return false;
    }
    let mut query = world.query_filtered::<(&Name, &ComputedNode), With<Loading>>();
    query
        .iter(world)
        .any(|(name, node)| name.as_str() == "MDSkirmishLoadB" && node.size().min_element() > 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playtest_ui::menu::{OpponentSlot, Page};

    #[test]
    fn original_loading_fonts_keep_point_truncation_then_gdi_pixel_rounding() {
        assert_eq!(font_pixels(10.0, 800.0).to_bits(), 13.0_f32.to_bits());
        assert_eq!(font_pixels(10.0, 1280.0).to_bits(), 19.0_f32.to_bits());
        assert_eq!(font_pixels(8.0, 1280.0).to_bits(), 15.0_f32.to_bits());
        assert_eq!(font_pixels(20.0, 1920.0).to_bits(), 52.0_f32.to_bits());
    }

    fn menu() -> Menu {
        Menu {
            page: Page::Skirmish,
            options_open: false,
            factions: vec![None, None, Some(Faction::China), None],
            opponents: vec![
                OpponentSlot::Open,
                OpponentSlot::Closed,
                OpponentSlot::Hard,
                OpponentSlot::Easy,
            ],
            teams: vec![Some(0), None, Some(0), None],
            colors: vec![None; 4],
            starts: vec![None; 4],
            starting_cash: 10000,
            superweapon_restricted: false,
            game_speed: 30,
            open: None,
            confirmation: None,
            capacity: 4,
        }
    }

    #[test]
    fn closed_slots_are_compacted_and_random_enemy_choices_stay_hidden() {
        let menu = menu();
        assert_eq!(rows(&menu), [0, 2, 3]);
        assert!(ally(&menu, 0));
        assert!(ally(&menu, 2));
        assert!(!ally(&menu, 3));
        assert!(apparent(None, Some(Faction::Gla), ally(&menu, 3), true).is_none());
        assert!(apparent(None, Some(Faction::Gla), ally(&menu, 0), true) == Some(Faction::Gla));
        assert_eq!(apparent(None, Some(2), ally(&menu, 2), true), Some(2));
        assert_eq!(apparent(Some(1), Some(1), ally(&menu, 3), true), Some(1));
        assert_eq!(apparent(None, Some(2), ally(&menu, 3), false), Some(2));
    }
}
