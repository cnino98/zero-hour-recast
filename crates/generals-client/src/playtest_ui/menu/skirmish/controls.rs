//! Original map start buttons and available-color combo rows.
use super::{Action, Dropdown, Field, Layer, Menu, ReleaseButton, UiData, at, text};
use crate::playtest::Match;
use bevy::prelude::*;

impl Menu {
    pub fn color_available(&self, slot: usize, color: Option<usize>) -> bool {
        color.is_none()
            || !self
                .colors
                .iter()
                .take(self.capacity)
                .enumerate()
                .any(|(other, value)| other != slot && *value == color)
    }

    fn next_unassigned(&self, first: usize) -> Option<usize> {
        (first..self.capacity).find(|&owner| {
            self.starts[owner].is_none()
                && (owner == 0 || self.opponents[owner].difficulty().is_some())
        })
    }

    pub fn cycle_start(&mut self, position: usize) {
        if position >= self.capacity {
            return;
        }
        // getNextSelectablePlayer runs BEFORE releasing the occupied position.
        let owner = self
            .starts
            .iter()
            .position(|start| *start == Some(position));
        let next = if let Some(owner) = owner {
            let next = self.next_unassigned(owner + 1);
            self.starts[owner] = None;
            next
        } else {
            Some(self.next_unassigned(0).unwrap_or(0))
        };
        if let Some(next) = next {
            self.starts[next] = Some(position);
        }
    }
}

pub(super) fn preview(commands: &mut Commands, data: &UiData) {
    let window = data.windows["SkirmishGameOptionsMenu.wnd:MapWindow"];
    let bounds = crate::radar::projection::fit(window, data.map_extent);
    commands.spawn((
        Layer::Skirmish,
        at(window),
        BackgroundColor(Color::BLACK),
        ZIndex(33),
    ));
    let preview = data
        .handles
        .get(&data.map_preview)
        .map(|handle| ImageNode::new(handle.clone()).with_mode(NodeImageMode::Stretch))
        .or_else(|| data.optional_image("UnknownMap"));
    if let Some(image) = preview {
        commands.spawn((Layer::Skirmish, image, at(bounds), ZIndex(34)));
    }
    let size = data.windows["SkirmishGameOptionsMenu.wnd:ButtonMapStartPosition0"].size();
    for (index, start) in data.map_starts.iter().enumerate() {
        let ratio = *start / data.map_extent;
        let position =
            bounds.min + (Vec2::new(ratio.x, 1.0 - ratio.y) * bounds.size() - size / 2.0).trunc();
        let parent = commands
            .spawn((
                Button,
                Action::ChooseStart(index),
                ReleaseButton { armed: false },
                Layer::Skirmish,
                at(Rect::from_corners(position, position + size)),
                ZIndex(35),
            ))
            .id();
        super::super::art(
            commands,
            data,
            parent,
            [
                "PlayerStart",
                "PlayerStartHilite",
                "PlayerStartHilitePushed",
            ]
            .map(str::to_owned),
            Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        let child = commands
            .spawn((
                Field::Start(index),
                text("", 8.0),
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                ZIndex(1),
            ))
            .id();
        commands.entity(parent).add_child(child);
    }
}

#[derive(Component)]
pub struct ColorRow {
    slot: usize,
    color: Option<usize>,
    area: Rect,
}

#[derive(Component)]
pub struct ColorList {
    slot: usize,
    area: Rect,
}

pub(super) fn color_list(commands: &mut Commands, data: &UiData, slot: usize, area: Rect) {
    let dropdown = Dropdown::Color(slot);
    commands.spawn((
        Button,
        Action::CloseDropdown,
        Layer::Dropdown(dropdown),
        at(Rect::new(0.0, 0.0, 800.0, 600.0)),
        ZIndex(43),
    ));
    commands.spawn((
        ColorList { slot, area },
        Layer::Dropdown(dropdown),
        Node {
            border: UiRect::all(px(1)),
            ..at(area)
        },
        BackgroundColor(Color::BLACK),
        BorderColor::all(Color::srgb_u8(49, 55, 168)),
        ZIndex(44),
    ));
    for color in std::iter::once(None).chain((0..data.color_labels.len()).map(Some)) {
        let label = color.map_or_else(
            || data.label("GUI:???"),
            |index| data.color_labels[index].as_str(),
        );
        let parent = super::row(
            commands,
            data,
            dropdown,
            area,
            Action::ChooseColor { slot, color },
            label,
        );
        commands
            .entity(parent)
            .insert(ColorRow { slot, color, area });
    }
}

pub fn update(
    mut menu: ResMut<Menu>,
    battle: Option<Res<Match>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut rows: Query<(&ColorRow, &mut Node)>,
    mut lists: Query<(&ColorList, &mut Node), Without<ColorRow>>,
    actions: Query<(&Action, &Interaction)>,
) {
    let buttons = buttons.into_inner();
    let battle = battle.map(Res::into_inner);
    for (row, mut node) in &mut rows {
        if !menu.color_available(row.slot, row.color) {
            node.display = Display::None;
            continue;
        }
        let offset = row.color.map_or(0, |color| {
            1 + (0..color)
                .filter(|&value| menu.color_available(row.slot, Some(value)))
                .count()
        });
        let top =
            row.area.max.y + f32::from(u16::try_from(offset).expect("eight colors")) * 14.0 + 2.0;
        *node = at(Rect::new(
            row.area.min.x + 2.0,
            top,
            row.area.max.x - 2.0,
            top + 14.0,
        ));
    }
    for (list, mut node) in &mut lists {
        let count = 1
            + (0..8)
                .filter(|&color| menu.color_available(list.slot, Some(color)))
                .count();
        let height = f32::from(u16::try_from(count).expect("nine color rows")) * 14.0 + 4.0;
        node.top = percent(list.area.max.y / 6.0);
        node.height = percent(height / 6.0);
    }
    if battle.as_ref().is_none_or(|battle| !battle.started)
        && matches!(menu.page, super::super::Page::Skirmish)
        && buttons.just_released(MouseButton::Right)
    {
        for (action, interaction) in &actions {
            if *interaction != Interaction::None
                && let Action::ChooseStart(position) = *action
                && let Some(owner) = menu
                    .starts
                    .iter()
                    .position(|start| *start == Some(position))
            {
                menu.starts[owner] = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playtest_ui::{OpponentSlot, menu::Page};
    use generals_sim::skirmish::Faction;

    #[test]
    fn start_cycle_skips_closed_players_and_returns_last_to_random() {
        let mut menu = Menu {
            options_open: false,
            page: Page::Skirmish,
            factions: vec![Some(Faction::Usa); 4],
            opponents: vec![
                OpponentSlot::Open,
                OpponentSlot::Hard,
                OpponentSlot::Closed,
                OpponentSlot::Easy,
            ],
            teams: vec![None; 4],
            colors: vec![None; 4],
            starts: vec![None; 4],
            starting_cash: 10_000,
            superweapon_restricted: false,
            game_speed: 30,
            open: None,
            confirmation: None,
            capacity: 4,
        };
        menu.cycle_start(2);
        assert_eq!(menu.starts, [Some(2), None, None, None]);
        menu.cycle_start(2);
        assert_eq!(menu.starts, [None, Some(2), None, None]);
        menu.cycle_start(2);
        assert_eq!(menu.starts, [None, None, None, Some(2)]);
        menu.cycle_start(2);
        assert_eq!(menu.starts, [None; 4]);
        menu.colors[1] = Some(3);
        assert!(!menu.color_available(0, Some(3)));
        assert!(menu.color_available(1, Some(3)));
        assert!(menu.color_available(0, None));
        menu.factions.extend([Some(Faction::China); 4]);
        menu.opponents.extend([OpponentSlot::Hard; 4]);
        menu.teams.extend([Some(2); 4]);
        menu.colors.extend([Some(7); 4]);
        menu.starts.extend([Some(7); 4]);
        menu.apply_map(2);
        assert!(menu.starts.iter().all(Option::is_none));
        assert_eq!(menu.factions.len(), 8);
        assert!(menu.color_available(0, Some(7)));
        assert!(!menu.enabled(Dropdown::Faction(7)));
        menu.apply_map(8);
        assert!(menu.factions[7] == Some(Faction::China));
        assert!(menu.opponents[7] == OpponentSlot::Hard);
        assert_eq!(menu.teams[7], Some(2));
        assert_eq!(menu.colors[7], Some(7));
    }
}
