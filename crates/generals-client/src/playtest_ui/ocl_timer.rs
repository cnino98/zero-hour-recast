//! Original `ControlBar` OCL context for timed cargo delivery.
use super::{Action, Layer, Match, PromotionIcon, UiData, at};
use crate::control_bar::{Command, CommandAction};
use bevy::{prelude::*, ui::FocusPolicy, ui_render::ui_material::MaterialNode};

#[derive(Component)]
pub enum Part {
    Bar,
    Fill,
    Description,
    Button,
    Highlight(Entity),
}

pub(super) fn setup(commands: &mut Commands, data: &UiData, icons: &mut Assets<PromotionIcon>) {
    let source = &data.ocl_timer;
    let bar_node = Node {
        border: UiRect::all(px(1)),
        display: Display::None,
        ..at(source.bar)
    };
    commands.spawn((
        Part::Bar,
        Layer::Game,
        bar_node.clone(),
        BackgroundColor(source.bar_background),
        BorderColor::all(source.bar_border),
        FocusPolicy::Pass,
        ZIndex(5),
    ));
    commands.spawn((
        Part::Fill,
        Layer::Game,
        bar_node,
        BackgroundColor(source.fill),
        BorderColor {
            top: Color::WHITE,
            left: Color::srgb_u8(200, 200, 200),
            ..BorderColor::all(source.fill_border)
        },
        FocusPolicy::Pass,
        ZIndex(6),
    ));
    let description = &source.description;
    let area = description.area;
    commands.spawn((
        Part::Description,
        Layer::Game,
        Text::default(),
        TextFont {
            font_size: FontSize::Px(description.font_size),
            ..default()
        },
        super::FontSizeAt600(description.font_size),
        TextColor(description.text_color),
        TextShadow {
            offset: Vec2::ONE,
            color: description.text_border,
        },
        BackgroundColor(description.background),
        BorderColor::all(description.border),
        TextLayout::justify(Justify::Left),
        Node {
            align_items: AlignItems::Center,
            padding: UiRect {
                left: px(6),
                right: px(2),
                ..default()
            },
            border: UiRect::all(px(1)),
            overflow: Overflow::clip(),
            display: Display::None,
            ..at(area)
        },
        FocusPolicy::Pass,
        ZIndex(7),
    ));
    let area = data.windows["ControlBar.wnd:OCLTimerSellButton"];
    let node = Node {
        display: Display::None,
        ..at(area)
    };
    let button = commands
        .spawn((
            Part::Button,
            Button,
            Action::OclTimer,
            Layer::Game,
            MaterialNode(icons.add(data.icon(&source.sell.image))),
            node.clone(),
            ZIndex(6),
        ))
        .id();
    commands.spawn((
        Part::Highlight(button),
        Layer::Game,
        data.image("Cameo_hilited"),
        node,
        FocusPolicy::Pass,
        ZIndex(7),
    ));
}

fn selected(battle: &Match) -> Option<usize> {
    let [id] = battle.selected.as_slice() else {
        return None;
    };
    let actor = battle.simulation.entities.get(*id)?;
    (battle.started && battle.simulation.player_active(0) && actor.complete() && !actor.masked())
        .then_some(*id)
}

pub(super) fn command<'a>(battle: &Match, data: &'a UiData) -> Option<&'a Command> {
    let id = selected(battle)?;
    let actor = &battle.simulation.entities[id];
    if actor.owner != 0 {
        return None;
    }
    let unit = &battle.simulation.catalog[actor.type_index];
    unit.behavior.ocl_update.as_ref()?;
    if !unit.kind_of.iter().any(|flag| flag == "TECH_BUILDING") {
        Some(&data.ocl_timer.sell)
    } else if unit.kind_of.iter().any(|flag| flag == "AUTO_RALLYPOINT") {
        Some(&data.ocl_timer.rally)
    } else {
        None
    }
}

fn format_countdown(format: &str, seconds: u32) -> String {
    format
        .replacen("%d", &(seconds / 60).to_string(), 1)
        .replacen("%d", &(seconds % 60).to_string(), 1)
}

type Widgets<'w, 's> = Query<
    'w,
    's,
    (
        &'static Part,
        &'static mut Node,
        Option<&'static mut Text>,
        Option<&'static MaterialNode<PromotionIcon>>,
    ),
>;

pub fn update(
    battle: Res<Match>,
    data: Res<UiData>,
    mut widgets: Widgets,
    interactions: Query<&Interaction>,
    mut icons: ResMut<Assets<PromotionIcon>>,
    mut displayed: Local<Option<(u32, usize, u32, u8)>>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let countdown = selected(battle).and_then(|id| {
        let (seconds, percent) = battle.simulation.delivery_countdown(id)?;
        let key = (battle.generation, id, seconds);
        let progress = displayed
            .filter(|&(generation, id, seconds, _)| (generation, id, seconds) == key)
            .map_or(percent, |(_, _, _, progress)| progress);
        Some((battle.generation, id, seconds, progress))
    });
    let redraw = countdown != *displayed;
    *displayed = countdown;
    let command = command(battle, data);
    for (part, mut node, description, icon) in &mut widgets {
        let visible = match part {
            Part::Bar | Part::Description => countdown.is_some(),
            Part::Fill => countdown.is_some_and(|(_, _, _, progress)| progress > 0),
            Part::Button => command.is_some(),
            Part::Highlight(button) => {
                command.is_some()
                    && interactions
                        .get(*button)
                        .is_ok_and(|interaction| *interaction != Interaction::None)
            }
        };
        node.reborrow()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(if visible {
                Display::Flex
            } else {
                Display::None
            });
        match part {
            Part::Fill if redraw => {
                if let Some((_, _, _, progress)) = countdown {
                    node.width =
                        percent(data.ocl_timer.bar.width() / 8.0 * f32::from(progress) / 100.0);
                }
            }
            Part::Description if redraw => {
                if let (Some((_, _, seconds, _)), Some(mut description)) = (countdown, description)
                {
                    let source = &data.ocl_timer;
                    let format = if seconds % 60 < 10 {
                        &source.padded_description
                    } else {
                        &source.description.text
                    };
                    description.0 = format_countdown(format, seconds);
                }
            }
            Part::Button => {
                if let (Some(command), Some(handle)) = (command, icon) {
                    let mut icon = data.icon(&command.image);
                    if matches!(command.action, CommandAction::Sell)
                        && !battle.simulation.can_sell(0, battle.selected[0])
                    {
                        icon.state.x = 1.0;
                    }
                    super::promotion_icon::update(&mut icons, &handle.0, icon);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_localized_format_keeps_explicit_single_digit_padding() {
        assert_eq!(
            super::format_countdown("Time Until Next Drop: %d:0%d", 65),
            "Time Until Next Drop: 1:05"
        );
        assert_eq!(
            super::format_countdown("Time Until Next Drop: %d:%d", 111),
            "Time Until Next Drop: 1:51"
        );
    }
}
