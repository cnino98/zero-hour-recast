use super::{Action, Field, Layer, MenuButtonPart, ReleaseButton, UiData, at, text};
use crate::playtest::Match;
use bevy::prelude::*;
use generals_sim::skirmish::Faction;

mod skirmish;
pub(super) use skirmish::DisabledPart;
pub use skirmish::{Dropdown, OpponentSlot, verify};
pub use skirmish::{
    name_input, name_presentation, option_boxes, option_checkbox, option_labels, option_thumb,
    update_options, update_setup,
};

pub(super) enum Page {
    Main,
    SinglePlayer,
    Skirmish,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Confirmation {
    Exit,
    Restart,
    MapUnavailable,
}

#[derive(Resource)]
pub struct Menu {
    pub(super) page: Page,
    pub(crate) options_open: bool,
    pub(super) factions: Vec<Option<Faction>>,
    pub(super) opponents: Vec<OpponentSlot>,
    pub(super) teams: Vec<Option<u8>>,
    pub(super) colors: Vec<Option<usize>>,
    pub(super) starts: Vec<Option<usize>>,
    pub(super) starting_cash: u32,
    pub(super) superweapon_restricted: bool,
    pub(super) game_speed: u8,
    pub(super) open: Option<Dropdown>,
    pub(super) confirmation: Option<Confirmation>,
    pub(super) capacity: usize,
}

impl Menu {
    pub(crate) const fn map_unavailable(&mut self) {
        self.confirmation = Some(Confirmation::MapUnavailable);
        self.open = None;
    }

    pub(crate) fn apply_map(&mut self, capacity: usize) {
        self.capacity = capacity;
        self.starts.fill(None);
        self.open = None;
    }
    pub(super) fn save(&self, data: &UiData, battle: &Match) {
        self.save_seed(data, battle.simulation.random_seed());
    }

    pub(super) fn save_seed(&self, data: &UiData, seed: u32) {
        let selections: Vec<_> = self
            .factions
            .iter()
            .enumerate()
            .map(|(owner, &faction)| crate::skirmish_preferences::Slot {
                faction,
                opponent: self.opponents[owner],
                color: self.colors[owner],
                start: self.starts[owner],
                team: self.teams[owner],
            })
            .collect();
        if let Err(error) = data.preferences.save(
            &selections,
            &data.map_preview.replace(".tga", ".map"),
            seed,
            self.starting_cash,
            self.superweapon_restricted,
            self.game_speed,
        ) {
            eprintln!("Unable to save skirmish preferences: {error}");
        }
    }

    pub(crate) fn configuration(&self, seed: u32) -> crate::playtest::startup::Configuration {
        crate::playtest::startup::Configuration {
            seed,
            starting_cash: self.starting_cash,
            superweapon_restriction: u32::from(self.superweapon_restricted),
            game_speed: self.game_speed,
            slots: self
                .factions
                .iter()
                .enumerate()
                .take(self.capacity)
                .map(|(owner, &faction)| generals_sim::skirmish::PlayerSlot {
                    faction,
                    occupied: owner == 0 || self.opponents[owner].difficulty().is_some(),
                    difficulty: if owner == 0 {
                        None
                    } else {
                        self.opponents[owner].difficulty()
                    },
                    team: self.teams[owner],
                    start: self.starts[owner],
                    color: self.colors[owner],
                })
                .collect(),
        }
    }

    pub(crate) fn start(&self, battle: &mut Match) {
        if let Err(error) = self
            .configuration(battle.simulation.random_seed())
            .start(battle)
        {
            eprintln!("Unable to start match: {error}");
        }
    }

    pub const fn clear_confirmation(&mut self) {
        self.confirmation = None;
    }
}

const fn activate_options(action: &Action, menu: &mut Menu) -> bool {
    match *action {
        Action::ChooseStartingCash(cash) => {
            menu.starting_cash = cash;
            menu.open = None;
        }
        Action::ToggleSuperweaponLimit => {
            menu.superweapon_restricted = !menu.superweapon_restricted;
        }
        _ => return false,
    }
    true
}

pub(super) fn activate_setup(action: &Action, menu: &mut Menu) -> bool {
    if activate_options(action, menu) {
        return true;
    }
    match *action {
        Action::SinglePlayer => {
            menu.page = if matches!(menu.page, Page::SinglePlayer) {
                Page::Main
            } else {
                Page::SinglePlayer
            };
        }
        Action::MainMenu => {
            menu.clear_confirmation();
            menu.page = Page::Main;
            menu.open = None;
        }
        Action::ToggleDropdown(dropdown) => {
            if menu.enabled(dropdown) {
                menu.open = if menu.open == Some(dropdown) {
                    None
                } else {
                    Some(dropdown)
                };
            }
        }
        Action::CloseDropdown => menu.open = None,
        Action::ChooseFaction { slot, faction } => {
            if menu.enabled(Dropdown::Faction(slot)) {
                menu.factions[slot] = faction;
            }
            menu.open = None;
        }
        Action::ChooseOpponent { slot, opponent } => {
            menu.opponents[slot] = opponent;
            if opponent.difficulty().is_none() {
                menu.starts[slot] = None;
                menu.colors[slot] = None;
            }
            menu.open = None;
        }
        Action::ChooseTeam { slot, team } => {
            if menu.enabled(Dropdown::Team(slot)) {
                menu.teams[slot] = team;
            }
            menu.open = None;
        }
        Action::ChooseColor { slot, color } => {
            if menu.enabled(Dropdown::Color(slot)) && menu.color_available(slot, color) {
                menu.colors[slot] = color;
            }
            menu.open = None;
        }
        Action::ChooseStart(position) => menu.cycle_start(position),
        _ => return false,
    }
    true
}

pub(super) fn activate(action: &Action, menu: &mut Menu, battle: &mut Match) -> bool {
    if activate_setup(action, menu) {
        return true;
    }
    match *action {
        Action::Skirmish => {
            menu.page = Page::Skirmish;
            // Native menu initialization uses GetTickCount's 32-bit clock seed.
            // Rust uses the portable wall-clock origin; restart keeps this seed.
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_le_bytes();
            battle.simulation.set_random_seed(u32::from_le_bytes(
                millis[..4].try_into().expect("four seed bytes"),
            ));
        }
        Action::Start => {
            menu.clear_confirmation();
            menu.open = None;
            menu.start(battle);
        }
        Action::Pause => {
            battle.promotions_open = false;
            menu.clear_confirmation();
            if battle.started {
                battle.paused = !battle.paused;
                battle.placing = None;
            }
        }
        Action::Menu => menu.confirmation = Some(Confirmation::Exit),
        Action::Restart => menu.confirmation = Some(Confirmation::Restart),
        Action::CancelConfirmation => menu.confirmation = None,
        Action::Modal => {}
        Action::ConfirmMenu => {
            menu.confirmation = None;
            menu.page = Page::Main;
            menu.open = None;
            battle.started = false;
            battle.paused = false;
            battle.promotions_open = false;
            battle.placing = None;
            battle.clear_drawable_selection();
        }
        Action::ConfirmRestart => {
            menu.confirmation = None;
            battle.restart();
        }
        _ => return false,
    }
    true
}

fn initialize(commands: &mut Commands, data: &UiData, capacity: usize) {
    commands.insert_resource(Menu {
        page: Page::Main,
        options_open: false,
        factions: data
            .preferences
            .slots
            .iter()
            .map(|slot| slot.faction)
            .collect(),
        opponents: data
            .preferences
            .slots
            .iter()
            .map(|slot| slot.opponent)
            .collect(),
        teams: data
            .preferences
            .slots
            .iter()
            .map(|slot| slot.team)
            .collect(),
        colors: data
            .preferences
            .slots
            .iter()
            .map(|slot| slot.color)
            .collect(),
        starts: data
            .preferences
            .slots
            .iter()
            .map(|slot| slot.start.filter(|start| *start < capacity))
            .collect(),
        starting_cash: data
            .preferences
            .starting_cash
            .unwrap_or(data.default_starting_cash),
        superweapon_restricted: data.preferences.superweapon_restricted,
        game_speed: data.preferences.game_speed,
        open: None,
        confirmation: None,
        capacity,
    });
}

pub(super) fn initialize_shell(commands: &mut Commands, data: &UiData) {
    initialize(commands, data, data.preferences.slots.len());
    setup_shell(commands, data);
    super::options::setup(commands, data);
    confirmation(commands, data, Confirmation::MapUnavailable);
}

pub(super) fn setup(commands: &mut Commands, data: &UiData, battle: &Match) {
    initialize(commands, data, battle.simulation.slots.len());
    setup_shell(commands, data);
    super::options::setup(commands, data);
    skirmish::setup(commands, data, battle.simulation.slots.len());
    crate::map_selection::setup(commands, data);
    pause(commands, data);
    confirmation(commands, data, Confirmation::Exit);
    confirmation(commands, data, Confirmation::Restart);
    confirmation(commands, data, Confirmation::MapUnavailable);
}

pub(super) fn setup_skirmish(commands: &mut Commands, data: &UiData, capacity: usize) {
    skirmish::setup(commands, data, capacity);
    crate::map_selection::setup(commands, data);
}

fn setup_shell(commands: &mut Commands, data: &UiData) {
    for (image, window, layer, order) in [
        (
            "MainMenuBackdrop",
            "MainMenu.wnd:MainMenuParent",
            Layer::Shell,
            30,
        ),
        (
            "MainMenuRuler",
            "MainMenu.wnd:MainMenuRuler",
            Layer::Shell,
            31,
        ),
        ("GeneralsLogo", "MainMenu.wnd:Logo", Layer::Menu, 32),
    ] {
        commands.spawn((
            layer,
            data.image(image),
            at(data.windows[window]),
            ZIndex(order),
        ));
    }
    button(
        commands,
        data,
        Action::SinglePlayer,
        Layer::Menu,
        data.windows["MainMenu.wnd:ButtonSinglePlayer"],
        data.label("GUI:SinglePlayer"),
    );
    // MainMenu::MainMenuSystem opens MapBorder, then pushes SkirmishGameOptionsMenu.
    commands.spawn((
        Layer::SinglePlayer,
        at(data.windows["MainMenu.wnd:MapBorder"]),
        BackgroundColor(Color::srgba_u8(0, 0, 0, 126)),
        ZIndex(33),
    ));
    button(
        commands,
        data,
        Action::Skirmish,
        Layer::SinglePlayer,
        data.windows["MainMenu.wnd:ButtonSkirmish"],
        data.label("GUI:Skirmish"),
    );
    button(
        commands,
        data,
        Action::Options,
        Layer::Menu,
        data.windows["MainMenu.wnd:ButtonOptions"],
        data.label("GUI:Options"),
    );
}

fn pause(commands: &mut Commands, data: &UiData) {
    commands.spawn((
        Layer::Pause,
        Node {
            border: UiRect::all(px(1)),
            ..at(data.windows["QuitMenu.wnd:QuitMenuParent"])
        },
        BackgroundColor(Color::srgba_u8(2, 2, 2, 190)),
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        ZIndex(40),
    ));
    commands.spawn((
        Layer::Pause,
        data.image("GeneralsLogo"),
        at(data.windows["QuitMenu.wnd:WinLoad"]),
        ZIndex(41),
    ));
    for (action, window, label) in [
        (Action::Options, "ButtonOptions", "GUI:Options"),
        (Action::Restart, "ButtonRestart", "GUI:RestartGame"),
        (Action::Menu, "ButtonExit", "GUI:Exit"),
        (Action::Pause, "ButtonReturn", "GUI:ReturnToGame"),
    ] {
        button(
            commands,
            data,
            action,
            Layer::Pause,
            data.windows[&format!("QuitMenu.wnd:{window}")],
            data.label(label),
        );
    }
}

fn confirmation(commands: &mut Commands, data: &UiData, kind: Confirmation) {
    // QuitMenuSystem uses a logo dialog for exit, the standard dialog for restart.
    let (window, title, message, action) = match kind {
        Confirmation::Exit => (
            "QuitMessageBox.wnd",
            "GUI:QuitPopupTitle",
            "GUI:QuitPopupMessage",
            Action::ConfirmMenu,
        ),
        Confirmation::Restart => (
            "MessageBox.wnd",
            "GUI:RestartConfirmationTitle",
            "GUI:RestartConfirmation",
            Action::ConfirmRestart,
        ),
        Confirmation::MapUnavailable => (
            "MessageBox.wnd",
            "GUI:ErrorStartingGame",
            "This map could not be loaded. Please select another map.",
            Action::CancelConfirmation,
        ),
    };
    commands.spawn((
        Button,
        Action::Modal,
        Layer::Confirmation(kind),
        at(Rect::new(0.0, 0.0, 800.0, 600.0)),
        BackgroundColor(Color::srgba_u8(2, 2, 2, 115)),
        ZIndex(50),
    ));
    commands.spawn((
        Layer::Confirmation(kind),
        Node {
            border: UiRect::all(px(1)),
            ..at(data.windows[&format!("{window}:MessageBoxParent")])
        },
        BackgroundColor(Color::srgb_u8(2, 2, 2)),
        BorderColor::all(Color::srgb_u8(47, 55, 168)),
        ZIndex(51),
    ));
    for (name, label, size) in [
        ("StaticTextTitle", title, 20.0),
        ("StaticTextMessage", message, 10.0),
    ] {
        commands.spawn((
            Layer::Confirmation(kind),
            text(data.label(label), size),
            at(data.windows[&format!("{window}:{name}")]),
            ZIndex(52),
        ));
    }
    if kind == Confirmation::Exit {
        commands.spawn((
            Layer::Confirmation(kind),
            data.image("GeneralsLogo"),
            at(data.windows[&format!("{window}:Logo")]),
            ZIndex(52),
        ));
    }
    if kind == Confirmation::MapUnavailable {
        button(
            commands,
            data,
            Action::CancelConfirmation,
            Layer::Confirmation(kind),
            data.windows["MessageBox.wnd:ButtonOk"],
            data.label("GUI:Ok"),
        );
        return;
    }
    // gogoMessageBox moves Yes to the OK position and No to Cancel, keeping sizes.
    for (action, name, position, label) in [
        (action, "ButtonYes", "ButtonOk", "GUI:Yes"),
        (
            Action::CancelConfirmation,
            "ButtonNo",
            "ButtonCancel",
            "GUI:No",
        ),
    ] {
        let area = data.windows[&format!("{window}:{name}")];
        let start = data.windows[&format!("{window}:{position}")].min;
        button(
            commands,
            data,
            action,
            Layer::Confirmation(kind),
            Rect::from_corners(start, start + area.size()),
            data.label(label),
        );
    }
}

pub fn button(
    commands: &mut Commands,
    data: &UiData,
    action: Action,
    layer: Layer,
    area: Rect,
    label: &str,
) {
    button_at_size(commands, data, action, layer, area, label, 15.0);
}

pub(super) fn button_at_size(
    commands: &mut Commands,
    data: &UiData,
    action: Action,
    layer: Layer,
    area: Rect,
    label: &str,
    font_size: f32,
) -> Entity {
    let order = match layer {
        Layer::Results => 63,
        Layer::Confirmation(_) => 53,
        _ => 42,
    };
    let parent = commands
        .spawn((
            Button,
            ReleaseButton { armed: false },
            action,
            layer,
            Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..at(area)
            },
            ZIndex(order),
        ))
        .id();
    let left = data.image_size("Buttons-Left").x;
    let right = data.image_size("Buttons-Right").x;
    for (part, start, width) in [
        ("Left", 0.0, left),
        ("Middle", left, area.width() - left - right),
        ("Right", area.width() - right, right),
    ] {
        art(
            commands,
            data,
            parent,
            ["", "-HiLite", "-Pushed"].map(|state| format!("Buttons{state}-{part}")),
            Rect::new(
                start / area.width() * 100.0,
                0.0,
                (start + width) / area.width() * 100.0,
                100.0,
            ),
        );
    }
    let child = commands
        .spawn((
            text(label, font_size),
            Node {
                width: percent(100),
                ..default()
            },
            ZIndex(1),
        ))
        .id();
    commands.entity(parent).add_child(child);
    parent
}

fn art(
    commands: &mut Commands,
    data: &UiData,
    parent: Entity,
    images: [String; 3],
    rect: Rect,
) -> Entity {
    let child = commands
        .spawn((
            data.image(&images[0]),
            MenuButtonPart { parent, images },
            Node {
                position_type: PositionType::Absolute,
                left: percent(rect.min.x),
                top: percent(rect.min.y),
                width: percent(rect.width()),
                height: percent(rect.height()),
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(child);
    child
}

#[cfg(test)]
mod map_failure_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[ignore = "requires original game installation"]
    #[test]
    fn original_map_failure_messagebox_has_one_acknowledgment_and_retains_setup() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        battle.started = false;
        let root = &generals_assets::game_root().unwrap();
        let mut data = crate::control_bar::load(
            &assets,
            &battle.simulation.catalog,
            &root.join("UserData/Options.ini"),
        )
        .unwrap();
        let mut images = Assets::<Image>::default();
        for (name, image) in std::mem::take(&mut data.textures) {
            data.handles.insert(name, images.add(image));
        }
        assert_ne!(data.label("GUI:ErrorStartingGame"), "GUI:ErrorStartingGame");
        let original_button = data.windows["MessageBox.wnd:ButtonOk"];
        let mut world = World::new();
        world.insert_resource(data);
        world.insert_resource(images);
        world.insert_resource(battle);
        world
            .run_system_once(
                |mut commands: Commands, data: Res<UiData>, battle: Res<Match>| {
                    setup(&mut commands, &data, &battle);
                },
            )
            .unwrap();
        let mut menu = world.remove_resource::<Menu>().unwrap();
        menu.page = Page::Skirmish;
        let capacity = menu.capacity;
        let starts = menu.starts.clone();
        let factions = menu.factions.clone();
        menu.map_unavailable();
        assert!(menu.confirmation == Some(Confirmation::MapUnavailable));
        let mut buttons = world.query::<(&Action, &Layer, &Node)>();
        let acknowledgments: Vec<_> = buttons
            .iter(&world)
            .filter_map(|(action, layer, node)| {
                matches!(
                    (action, layer),
                    (
                        Action::CancelConfirmation,
                        Layer::Confirmation(Confirmation::MapUnavailable)
                    )
                )
                .then_some(node)
            })
            .collect();
        assert_eq!(acknowledgments.len(), 1);
        let expected = at(original_button);
        assert_eq!(acknowledgments[0].left, expected.left);
        assert_eq!(acknowledgments[0].top, expected.top);
        assert!(activate(
            &Action::CancelConfirmation,
            &mut menu,
            &mut world.resource_mut::<Match>()
        ));
        assert!(menu.confirmation.is_none());
        assert!(matches!(menu.page, Page::Skirmish));
        assert_eq!(menu.capacity, capacity);
        assert_eq!(menu.starts, starts);
        assert!(menu.factions == factions);
        assert!(!world.resource::<Match>().started);
    }
}
