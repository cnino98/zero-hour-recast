//! Original occupied-map slots from `SkirmishGameOptionsMenu` and `GUIUtil`.
use super::{Action, Field, Layer, Menu, ReleaseButton, UiData, art, at, button, text};
use bevy::prelude::*;
use generals_sim::skirmish::{Faction, OpponentDifficulty};

mod controls;
mod options;
pub use options::{
    boxes as option_boxes, checkbox as option_checkbox, labels as option_labels,
    thumb as option_thumb, update as update_options,
};
mod name_entry;
mod name_verification;
mod options_verification;
pub use name_entry::{input as name_input, presentation as name_presentation};
mod verification;
pub use controls::update as update_setup;
pub use verification::enable as verify;

// Actions and layer components each retain the same concrete dropdown identity.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Dropdown {
    Faction(usize),
    Player(usize),
    Team(usize),
    Color(usize),
    StartingCash,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum OpponentSlot {
    Open,
    Closed,
    Easy,
    Medium,
    Hard,
}

impl OpponentSlot {
    pub const fn difficulty(self) -> Option<OpponentDifficulty> {
        match self {
            Self::Open | Self::Closed => None,
            Self::Easy => Some(OpponentDifficulty::Easy),
            Self::Medium => Some(OpponentDifficulty::Normal),
            Self::Hard => Some(OpponentDifficulty::Hard),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "GUI:Open",
            Self::Closed => "GUI:Closed",
            Self::Easy => "GUI:EasyAI",
            Self::Medium => "GUI:MediumAI",
            Self::Hard => "GUI:HardAI",
        }
    }
}

impl Menu {
    pub fn selected(&self, action: &Action) -> bool {
        match *action {
            Action::ChooseFaction { slot, faction } => self.factions[slot] == faction,
            Action::ChooseOpponent { slot, opponent } => self.opponents[slot] == opponent,
            Action::ChooseTeam { slot, team } => self.teams[slot] == team,
            Action::ChooseColor { slot, color } => self.colors[slot] == color,
            Action::ChooseStartingCash(cash) => self.starting_cash == cash,
            _ => false,
        }
    }

    pub fn enabled(&self, dropdown: Dropdown) -> bool {
        match dropdown {
            Dropdown::Faction(slot)
            | Dropdown::Team(slot)
            | Dropdown::Color(slot)
            | Dropdown::Player(slot)
                if slot >= self.capacity =>
            {
                false
            }
            Dropdown::Faction(slot) | Dropdown::Team(slot) | Dropdown::Color(slot) if slot != 0 => {
                self.opponents[slot].difficulty().is_some()
            }
            _ => true,
        }
    }
}

#[derive(Component)]
pub struct DisabledPart {
    pub dropdown: Dropdown,
    pub image: String,
}

pub(super) fn setup(commands: &mut Commands, data: &UiData, capacity: usize) {
    controls::preview(commands, data);
    name_entry::setup(commands, data);
    options::setup(commands, data);
    commands.spawn((
        Layer::SkirmishUnderlying,
        Node {
            border: UiRect::all(px(1)),
            ..at(data.windows["SkirmishGameOptionsMenu.wnd:SubParent"])
        },
        BackgroundColor(Color::srgba_u8(2, 2, 2, 193)),
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        ZIndex(32),
    ));
    for (window, label, size) in [
        ("StaticTextTitle", "GUI:Skirmish", 20.0),
        ("StaticTextFaction", "GUI:Faction", 14.0),
        ("StaticTextPlayers", "GUI:Players", 14.0),
        ("StaticTextColor", "GUI:Color", 14.0),
        ("StaticTextTeam", "GUI:Team", 14.0),
        (
            "TextEntryMapDisplay",
            if capacity == 4 {
                "MAP:FallenEmpire"
            } else {
                "MAP:TourneyArena"
            },
            10.0,
        ),
    ] {
        commands.spawn((
            if matches!(window, "StaticTextTitle" | "StaticTextPlayers") {
                Layer::SkirmishUnderlying
            } else {
                Layer::Skirmish
            },
            text(
                if window == "TextEntryMapDisplay" {
                    &data.map_title
                } else {
                    data.label(label)
                },
                size,
            ),
            at(data.windows[&format!("SkirmishGameOptionsMenu.wnd:{window}")]),
            ZIndex(34),
        ));
    }
    for (action, window, label) in [
        (Action::Start, "ButtonStart", "GUI:Start"),
        (Action::SelectMap, "ButtonSelectMap", "GUI:SelectMap"),
        (Action::MainMenu, "ButtonBack", "GUI:GotoMainMenu"),
    ] {
        let layer = if matches!(action, Action::MainMenu) {
            Layer::SkirmishUnderlying
        } else {
            Layer::Skirmish
        };
        button(
            commands,
            data,
            action,
            layer,
            data.windows[&format!("SkirmishGameOptionsMenu.wnd:{window}")],
            data.label(label),
        );
    }
    for slot in 0..8 {
        combo(commands, data, Dropdown::Faction(slot));
        combo(commands, data, Dropdown::Team(slot));
        combo(commands, data, Dropdown::Color(slot));
        if slot > 0 {
            combo(commands, data, Dropdown::Player(slot));
        }
    }
}

fn combo(commands: &mut Commands, data: &UiData, dropdown: Dropdown) {
    let (window, field, initial) = match dropdown {
        Dropdown::StartingCash => unreachable!("cash has its native option label"),
        Dropdown::Color(slot) => (
            format!("ComboBoxColor{slot}"),
            Field::Color(slot),
            data.label("GUI:???"),
        ),
        Dropdown::Faction(slot) => (
            format!("ComboBoxPlayerTemplate{slot}"),
            Field::Faction(slot),
            data.factions[slot % 3].as_str(),
        ),
        Dropdown::Player(slot) => (
            format!("ComboBoxPlayer{slot}"),
            Field::Opponent(slot),
            data.label(if slot == 1 { "GUI:EasyAI" } else { "GUI:Open" }),
        ),
        Dropdown::Team(slot) => (
            format!("ComboBoxTeam{slot}"),
            Field::Team(slot),
            data.label("Team:0"),
        ),
    };
    let area = data.windows[&format!("SkirmishGameOptionsMenu.wnd:{window}")];
    let parent = commands
        .spawn((
            Button,
            Action::ToggleDropdown(dropdown),
            ReleaseButton { armed: false },
            match dropdown {
                Dropdown::Player(slot) => Layer::SkirmishPlayer(slot),
                Dropdown::Faction(slot) | Dropdown::Team(slot) | Dropdown::Color(slot) => {
                    Layer::SkirmishSlot(slot)
                }
                Dropdown::StartingCash => Layer::SkirmishUnderlying,
            },
            at(area),
            ZIndex(35),
        ))
        .id();
    combo_art(commands, data, parent, dropdown, area);
    let label = commands
        .spawn((
            field,
            text(initial, 10.0),
            Node {
                left: percent(5.0 / area.width() * 100.0),
                width: percent((area.width() - 21.0 - 10.0) / area.width() * 100.0),
                top: percent(5.0 / area.height() * 100.0),
                height: percent((area.height() - 10.0) / area.height() * 100.0),
                position_type: PositionType::Absolute,
                overflow: Overflow::clip(),
                ..default()
            },
            ZIndex(1),
        ))
        .id();
    commands
        .entity(label)
        .insert(TextLayout::justify(Justify::Left).with_no_wrap());
    commands.entity(parent).add_child(label);
    if let Dropdown::Color(slot) = dropdown {
        controls::color_list(commands, data, slot, area);
    } else {
        list(commands, data, dropdown, area);
    }
}

fn combo_art(
    commands: &mut Commands,
    data: &UiData,
    parent: Entity,
    dropdown: Dropdown,
    area: Rect,
) {
    // GameWindowManager::gogoGadgetComboBox reserves 21 source pixels for its arrow.
    let entry_width = area.width() - 21.0;
    let left = data.image_size("TextEntryEnabledLeftEnd").x;
    let right = data.image_size("TextEntryEnabledRightEnd").x;
    for (part, x, width) in [
        ("LeftEnd", 0.0, left),
        ("RepeatingCenter", left, entry_width - left - right),
        ("RightEnd", entry_width - right, right),
        ("Arrow", entry_width, 21.0),
    ] {
        let (images, disabled) = if part == "Arrow" {
            (
                ["Enabled", "Hilite", "HiliteSelected"]
                    .map(|state| format!("VSliderDownButton{state}")),
                "VSliderDownButtonDisabled".into(),
            )
        } else {
            (
                ["Enabled", "Hilite", "Hilite"].map(|state| format!("TextEntry{state}{part}")),
                format!("TextEntryDisabled{part}"),
            )
        };
        let child = art(
            commands,
            data,
            parent,
            images,
            Rect::new(
                x / area.width() * 100.0,
                0.0,
                (x + width) / area.width() * 100.0,
                100.0,
            ),
        );
        commands.entity(child).insert(DisabledPart {
            dropdown,
            image: disabled,
        });
    }
}

fn list(commands: &mut Commands, data: &UiData, dropdown: Dropdown, area: Rect) {
    commands.spawn((
        Button,
        Action::CloseDropdown,
        Layer::Dropdown(dropdown),
        at(Rect::new(0.0, 0.0, 800.0, 600.0)),
        ZIndex(43),
    ));
    // GadgetComboBoxInput: 2px row spacing and a 4px border allowance.
    let items: Vec<_> = match dropdown {
        Dropdown::StartingCash => unreachable!("cash list retains native money labels"),
        Dropdown::Color(_) => unreachable!("color lists are rebuilt from available colors"),
        Dropdown::Faction(slot) => [
            None,
            Some(Faction::Usa),
            Some(Faction::China),
            Some(Faction::Gla),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, faction)| {
            (
                Action::ChooseFaction { slot, faction },
                if index == 0 {
                    data.label("GUI:Random")
                } else {
                    data.factions[index - 1].as_str()
                },
            )
        })
        .collect(),
        Dropdown::Player(owner) => [
            OpponentSlot::Open,
            OpponentSlot::Closed,
            OpponentSlot::Easy,
            OpponentSlot::Medium,
            OpponentSlot::Hard,
        ]
        .into_iter()
        .map(|opponent| {
            (
                Action::ChooseOpponent {
                    slot: owner,
                    opponent,
                },
                data.label(opponent.label()),
            )
        })
        .collect(),
        Dropdown::Team(slot) => [None, Some(0), Some(1), Some(2), Some(3)]
            .into_iter()
            .map(|team| {
                (
                    Action::ChooseTeam { slot, team },
                    data.label(
                        ["Team:0", "Team:1", "Team:2", "Team:3", "Team:4"]
                            [team.map_or(0, |team| usize::from(team) + 1)],
                    ),
                )
            })
            .collect(),
    };
    let row_height = 14.0;
    let count = f32::from(u16::try_from(items.len()).expect("source dropdown item count"));
    let list_height = count * row_height;
    let bounds = Rect::new(
        area.min.x,
        area.max.y,
        area.max.x,
        area.max.y + list_height + 4.0,
    );
    commands.spawn((
        Layer::Dropdown(dropdown),
        Node {
            border: UiRect::all(px(1)),
            ..at(bounds)
        },
        BackgroundColor(Color::BLACK),
        BorderColor::all(Color::srgb_u8(49, 55, 168)),
        ZIndex(44),
    ));
    for (index, (action, label)) in items.into_iter().enumerate() {
        let offset =
            f32::from(u16::try_from(index).expect("source dropdown item count")) * row_height;
        let top = area.max.y + offset + 2.0;
        row(
            commands,
            data,
            dropdown,
            Rect::new(area.min.x + 2.0, top, area.max.x - 2.0, top + row_height),
            action,
            label,
        );
    }
}

fn row(
    commands: &mut Commands,
    data: &UiData,
    dropdown: Dropdown,
    area: Rect,
    action: Action,
    label: &str,
) -> Entity {
    let color = match action {
        Action::ChooseColor {
            color: Some(index), ..
        } => Color::srgb_from_array(data.results.colors[index]),
        _ => Color::srgb_u8(255, 254, 254),
    };
    let parent = commands
        .spawn((
            Button,
            action,
            ReleaseButton { armed: false },
            Layer::Dropdown(dropdown),
            at(area),
            ZIndex(45),
        ))
        .id();
    let left = data.image_size("ListBoxHiliteItemLeftEnd").x;
    let right = data.image_size("ListBoxHiliteItemRightEnd").x;
    for (image, x, width) in [
        ("ListBoxHiliteItemLeftEnd", 0.0, left),
        (
            "ListBoxHiliteItemRepeatingCenter",
            left,
            area.width() - left - right,
        ),
        ("ListBoxHiliteItemRightEnd", area.width() - right, right),
    ] {
        art(
            commands,
            data,
            parent,
            [
                "BlackSquare".into(),
                image.into(),
                image.replace("HiliteItem", "HiliteSelectedItem"),
            ],
            Rect::new(
                x / area.width() * 100.0,
                0.0,
                (x + width) / area.width() * 100.0,
                100.0,
            ),
        );
    }
    let child = commands
        .spawn((
            text(label, 10.0),
            Node {
                width: percent(100),
                ..default()
            },
            ZIndex(1),
        ))
        .id();
    commands
        .entity(child)
        .insert(TextLayout::justify(Justify::Left).with_no_wrap());
    commands.entity(child).insert(TextColor(color));
    commands.entity(parent).add_child(child);
    parent
}
