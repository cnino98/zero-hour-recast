//! Native starting-money combo, superweapon checkbox and horizontal speed gadget.
use super::{Action, Dropdown, Layer, Menu, ReleaseButton, UiData, at, text};
use bevy::{prelude::*, window::PrimaryWindow};

#[derive(Component)]
pub enum OptionText {
    Cash,
    Speed,
}

#[derive(Component)]
pub struct Checkbox;

#[derive(Component)]
pub struct SpeedBox {
    index: u16,
    width: f32,
}

#[derive(Component)]
pub struct SpeedHighlight;

#[derive(Component)]
pub struct SpeedThumb;

#[derive(Resource, Default)]
pub struct SpeedInput {
    dragging: bool,
    focused: bool,
}

fn cash_label(format: &str, cash: u32) -> String {
    format.replace("%u", &cash.to_string())
}

pub(super) fn setup(commands: &mut Commands, data: &UiData) {
    commands.init_resource::<SpeedInput>();
    commands.spawn((
        Layer::SkirmishUnderlying,
        text(data.label("GUI:GameSpeed"), 10.0),
        at(data.windows["SkirmishGameOptionsMenu.wnd:StaticTextBattleHonors1"]),
        ZIndex(34),
    ));
    cash_setup(commands, data);
    checkbox_setup(commands, data);
    speed_setup(commands, data);
}

fn cash_setup(commands: &mut Commands, data: &UiData) {
    // The authored starting-money label has no name in the WND.
    commands.spawn((
        Layer::SkirmishUnderlying,
        text(data.label("GUI:StartingMoney"), 10.0),
        at(Rect::new(360.0, 334.0, 453.0, 358.0)),
        ZIndex(34),
    ));
    let area = data.windows["SkirmishGameOptionsMenu.wnd:ComboBoxStartingCash"];
    let parent = commands
        .spawn((
            Button,
            Action::ToggleDropdown(Dropdown::StartingCash),
            ReleaseButton { armed: false },
            Layer::SkirmishUnderlying,
            at(area),
            ZIndex(35),
        ))
        .id();
    super::combo_art(commands, data, parent, Dropdown::StartingCash, area);
    let child = commands
        .spawn((
            OptionText::Cash,
            text("", 10.0),
            Node {
                position_type: PositionType::Absolute,
                left: percent(5.0 / area.width() * 100.0),
                top: percent(5.0 / area.height() * 100.0),
                width: percent((area.width() - 31.0) / area.width() * 100.0),
                overflow: Overflow::clip(),
                ..default()
            },
            ZIndex(1),
        ))
        .id();
    commands
        .entity(child)
        .insert(TextLayout::justify(Justify::Left).with_no_wrap());
    commands.entity(parent).add_child(child);
    commands.spawn((
        Button,
        Action::CloseDropdown,
        Layer::Dropdown(Dropdown::StartingCash),
        at(Rect::new(0.0, 0.0, 800.0, 600.0)),
        ZIndex(43),
    ));
    let mut choices = data.starting_cash.clone();
    let cash = data
        .preferences
        .starting_cash
        .unwrap_or(data.default_starting_cash);
    if !choices.contains(&cash) {
        choices.push(cash);
    }
    let count = f32::from(u16::try_from(choices.len()).expect("native money choices"));
    let list_height = count * 14.0;
    let bottom = area.max.y + list_height + 4.0;
    commands.spawn((
        Layer::Dropdown(Dropdown::StartingCash),
        Node {
            border: UiRect::all(px(1)),
            ..at(Rect::new(area.min.x, area.max.y, area.max.x, bottom))
        },
        BackgroundColor(Color::BLACK),
        BorderColor::all(Color::srgb_u8(49, 55, 168)),
        ZIndex(44),
    ));
    for (index, cash) in choices.into_iter().enumerate() {
        let offset = f32::from(u16::try_from(index).expect("native money choices")) * 14.0;
        let top = area.max.y + offset + 2.0;
        super::row(
            commands,
            data,
            Dropdown::StartingCash,
            Rect::new(area.min.x + 2.0, top, area.max.x - 2.0, top + 14.0),
            Action::ChooseStartingCash(cash),
            &cash_label(data.label("GUI:StartingMoneyFormat"), cash),
        );
    }
}

fn checkbox_setup(commands: &mut Commands, data: &UiData) {
    let area = data.windows["SkirmishGameOptionsMenu.wnd:CheckboxLimitSuperweapons"];
    let parent = commands
        .spawn((
            Button,
            Action::ToggleSuperweaponLimit,
            ReleaseButton { armed: false },
            Layer::SkirmishUnderlying,
            at(area),
            ZIndex(35),
        ))
        .id();
    let child = commands
        .spawn((
            Checkbox,
            data.image("Active-Unchecked"),
            Node {
                position_type: PositionType::Absolute,
                top: percent(3.0 / area.height() * 100.0),
                width: percent((area.height() - 6.0) / area.width() * 100.0),
                height: percent((area.height() - 6.0) / area.height() * 100.0),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(child);
    let child = commands
        .spawn((
            text(data.label("GUI:LimitSuperweapons"), 10.0),
            Node {
                position_type: PositionType::Absolute,
                left: percent(area.height() / area.width() * 100.0),
                top: percent(5.0 / area.height() * 100.0),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(child);
}

fn speed_setup(commands: &mut Commands, data: &UiData) {
    commands.spawn((
        OptionText::Speed,
        Layer::SkirmishUnderlying,
        text("", 10.0),
        at(data.windows["SkirmishGameOptionsMenu.wnd:StaticTextGameSpeed"]),
        ZIndex(35),
    ));
    let area = data.windows["SkirmishGameOptionsMenu.wnd:SliderGameSpeed"];
    let width = data.image_size("hilightedbox").x;
    let mut count = 0_u16;
    let mut end = width;
    loop {
        if end >= area.width() {
            break;
        }
        count += 1;
        end += width + 2.0;
    }
    let offset = ((area.width() - (end - width)) / 2.0).trunc();
    for index in 0..=count {
        let stride = width + 2.0;
        let half_stride = (stride / 2.0).trunc();
        let box_offset = f32::from(index) * stride;
        let x = area.min.x + offset - half_stride + box_offset;
        commands.spawn((
            SpeedHighlight,
            Layer::SkirmishUnderlying,
            data.image("linebox"),
            at(Rect::new(
                x,
                area.min.y + (width / 3.0).trunc(),
                x + width + 2.0,
                area.min.y + (width / 3.0).trunc() + width + 2.0,
            )),
            ZIndex(34),
        ));
    }
    for index in 0..count {
        let box_offset = f32::from(index) * (width + 2.0);
        let x = area.min.x + offset + box_offset;
        commands.spawn((
            SpeedBox { index, width },
            Layer::SkirmishUnderlying,
            data.image("dehilightedbox"),
            at(Rect::new(x, area.min.y, x + width, area.min.y + width)),
            ZIndex(35),
        ));
    }
    commands.spawn((
        SpeedThumb,
        Layer::SkirmishUnderlying,
        data.image("arrow"),
        at(Rect::new(
            area.min.x,
            area.min.y + 10.0,
            area.min.x + 13.0,
            area.min.y + 26.0,
        )),
        ZIndex(36),
    ));
}

fn thumb_position(area: Rect, speed: u8) -> f32 {
    (f32::from(speed - 15) * (area.width() - 13.0) / 46.0).trunc()
}

fn dragged_speed(area: Rect, x: f32) -> u8 {
    if x < area.min.x + 6.0 {
        return 15;
    }
    if x > area.max.x - 6.0 {
        return 61;
    }
    // GadgetHorizontalSliderSystem: integer cursor coordinate and truncated ticks.
    let delta = (x - area.min.x - 6.0).trunc();
    let position = (delta / ((area.width() - 13.0) / 46.0)).trunc() + 15.0;
    (15_u8..=61)
        .find(|&value| f32::from(value) >= position)
        .unwrap_or(61)
}

fn speed_input(
    menu: &mut Menu,
    input: &mut SpeedInput,
    buttons: &ButtonInput<MouseButton>,
    keys: &ButtonInput<KeyCode>,
    area: Rect,
    cursor: Option<Vec2>,
    active: bool,
) {
    let hovered = cursor.is_some_and(|cursor| area.contains(cursor));
    if !active {
        input.dragging = false;
        input.focused = false;
    }
    if active && menu.open.is_none() {
        if buttons.just_pressed(MouseButton::Left) {
            input.focused = hovered;
        }
        if input.focused {
            if keys.just_pressed(KeyCode::ArrowRight) && menu.game_speed > 16 {
                menu.game_speed -= 2;
            }
            if keys.just_pressed(KeyCode::ArrowLeft) && menu.game_speed < 60 {
                menu.game_speed += 2;
            }
            if keys.just_pressed(KeyCode::Tab) || keys.just_pressed(KeyCode::Escape) {
                input.focused = false;
            }
        }
        if buttons.just_pressed(MouseButton::Left) && hovered {
            let x = cursor.expect("hovered cursor").x - area.min.x;
            let thumb = thumb_position(area, menu.game_speed);
            input.dragging = (thumb..=thumb + 13.0).contains(&x);
        }
        if input.dragging && buttons.pressed(MouseButton::Left) {
            if let Some(cursor) = cursor {
                menu.game_speed = dragged_speed(area, cursor.x);
            }
        } else if buttons.just_released(MouseButton::Left) && hovered && !input.dragging {
            let center = thumb_position(area, menu.game_speed) + 6.0;
            let x = cursor.expect("hovered cursor").x - area.min.x;
            let click = if x >= center {
                (center + (area.width() / 5.0).trunc()).min(x)
            } else {
                (center - (area.width() / 5.0).trunc()).max(x)
            };
            menu.game_speed = dragged_speed(area, area.min.x + click);
        }
    }
    if buttons.just_released(MouseButton::Left) {
        input.dragging = false;
    }
}

fn pointer(window: &Window) -> Option<Vec2> {
    window
        .cursor_position()
        .map(|cursor| cursor / Vec2::new(window.width(), window.height()) * Vec2::new(800.0, 600.0))
}

pub fn update(
    mut menu: ResMut<Menu>,
    data: Res<UiData>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    battle: Option<Res<crate::playtest::Match>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut input: ResMut<SpeedInput>,
) {
    let data = data.into_inner();
    let buttons = buttons.into_inner();
    let keys = keys.into_inner();
    let battle = battle.map(Res::into_inner);
    let active = battle.as_ref().is_none_or(|battle| !battle.started)
        && matches!(menu.page, super::super::Page::Skirmish);
    let area = data.windows["SkirmishGameOptionsMenu.wnd:SliderGameSpeed"];
    let cursor = windows.single().ok().and_then(pointer);
    speed_input(&mut menu, &mut input, buttons, keys, area, cursor, active);
}

pub fn labels(
    menu: Res<Menu>,
    data: Res<UiData>,
    mut labels: Query<(&OptionText, &mut Text, &mut TextColor)>,
) {
    let menu = menu.into_inner();
    let data = data.into_inner();
    for (field, mut label, mut color) in &mut labels {
        let value = match field {
            OptionText::Cash => {
                cash_label(data.label("GUI:StartingMoneyFormat"), menu.starting_cash)
            }
            OptionText::Speed => {
                color.set_if_neq(TextColor(if menu.game_speed == data.default_game_speed {
                    Color::srgba_u8(255, 255, 255, 231)
                } else {
                    Color::srgb_u8(255, 254, 252)
                }));
                if menu.game_speed > 60 {
                    "--".into()
                } else {
                    format!("{:2}", menu.game_speed)
                }
            }
        };
        label.0 = value;
    }
}

pub fn checkbox(
    menu: Res<Menu>,
    data: Res<UiData>,
    mut checks: Query<&mut ImageNode, With<Checkbox>>,
    actions: Query<(&Action, &Interaction)>,
) {
    let menu = menu.into_inner();
    let data = data.into_inner();
    let hovered = actions.iter().any(|(action, interaction)| {
        matches!(action, Action::ToggleSuperweaponLimit) && *interaction != Interaction::None
    });
    for mut image in &mut checks {
        image.image = data
            .image(if menu.superweapon_restricted {
                "Active-Checked"
            } else if hovered {
                "Active-HiLighted"
            } else {
                "Active-Unchecked"
            })
            .image;
    }
}

pub fn boxes(
    menu: Res<Menu>,
    data: Res<UiData>,
    battle: Option<Res<crate::playtest::Match>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut boxes: Query<(&SpeedBox, &mut ImageNode)>,
    mut highlights: Query<&mut Visibility, With<SpeedHighlight>>,
) {
    let menu = menu.into_inner();
    let data = data.into_inner();
    let area = data.windows["SkirmishGameOptionsMenu.wnd:SliderGameSpeed"];
    let battle = battle.map(Res::into_inner);
    let hovered = battle.as_ref().is_none_or(|battle| !battle.started)
        && matches!(menu.page, super::super::Page::Skirmish)
        && windows
            .single()
            .ok()
            .and_then(pointer)
            .is_some_and(|cursor| area.contains(cursor));
    let selected_x = (f32::from(menu.game_speed - 15) / 46.0 * area.width()).trunc();
    for (part, mut image) in &mut boxes {
        let selected =
            menu.game_speed != 15 && f32::from(part.index) * (part.width + 2.0) <= selected_x;
        image.image = data
            .image(if selected {
                "hilightedbox"
            } else {
                "dehilightedbox"
            })
            .image;
    }
    for mut visibility in &mut highlights {
        *visibility = if hovered {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub fn thumb(
    menu: Res<Menu>,
    data: Res<UiData>,
    battle: Option<Res<crate::playtest::Match>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut thumbs: Query<(&mut Node, &mut Visibility), With<SpeedThumb>>,
) {
    let menu = menu.into_inner();
    let data = data.into_inner();
    let area = data.windows["SkirmishGameOptionsMenu.wnd:SliderGameSpeed"];
    let battle = battle.map(Res::into_inner);
    let hovered = battle.as_ref().is_none_or(|battle| !battle.started)
        && matches!(menu.page, super::super::Page::Skirmish)
        && windows
            .single()
            .ok()
            .and_then(pointer)
            .is_some_and(|cursor| area.contains(cursor));
    for (mut node, mut visibility) in &mut thumbs {
        let x = area.min.x + thumb_position(area, menu.game_speed);
        *node = at(Rect::new(x, area.min.y + 10.0, x + 13.0, area.min.y + 26.0));
        *visibility = if hovered {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "requires original game installation"]
    #[test]
    fn installed_unsigned_starting_money_format_draws_selected_amount() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let labels =
            crate::control_bar::strings(&assets.read("data/english/generals.csf").unwrap().bytes)
                .unwrap();
        assert_eq!(labels["gui:startingmoneyformat"], "$%u");
        assert_eq!(
            cash_label(&labels["gui:startingmoneyformat"], 20_000),
            "$20000"
        );
    }

    #[test]
    fn native_speed_drag_preserves_bounds_and_truncated_thumb_coordinates() {
        let area = Rect::new(176.0, 344.0, 266.0, 371.0);
        assert_eq!(dragged_speed(area, 170.0), 15);
        assert_eq!(dragged_speed(area, 270.0), 61);
        assert_eq!(dragged_speed(area, 207.0), 29);
        assert_eq!(thumb_position(area, 30).to_bits(), 25.0_f32.to_bits());
        assert_eq!(thumb_position(area, 61).to_bits(), 77.0_f32.to_bits());
    }
}
