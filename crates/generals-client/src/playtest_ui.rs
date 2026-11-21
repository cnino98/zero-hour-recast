use crate::{
    control_bar::{Command, CommandAction, PowerAim, PowerCommand, UiData},
    input::{CommandMode, Mouse},
    playtest::{CameraRig, Match},
};
use bevy::{prelude::*, window::PrimaryWindow};
use generals_sim::skirmish::Faction;

mod cancellation;
mod command_icons;
mod menu;
mod ocl_timer;
pub mod options;
pub mod shell;
pub use cancellation::update as cancellation_update;
pub use command_icons::update as update_command_icons;
pub use ocl_timer::update as ocl_timer_update;
#[cfg(test)]
mod changes_tests;
pub mod observer;
mod power_shortcuts;
mod promotion_icon;
mod promotions;
mod results;
#[cfg(test)]
mod selected_command_tests;
mod shortcuts;
mod superweapon_timers;
mod transport;
pub use menu::button as menu_button;
use menu::{Confirmation, Page};
pub use menu::{Dropdown, Menu, OpponentSlot, verify as verify_setup};
pub use menu::{
    name_input, name_presentation, option_boxes, option_checkbox, option_labels, option_thumb,
    update_options, update_setup as setup_controls,
};
pub use promotion_icon::{PromotionIcon, PromotionIconPlugin};
pub use promotions::{
    PromotionTransition, panel as promotion_panel, transition as promotion_transition,
    update as promotion_icons,
};
pub use results::{
    Cell as ResultCell, Label as ResultLabel, ResultAction, ResultsScreen, keys as result_keys,
    player_name as result_player_name, tick as result_tick, update as result_update,
};
pub use shortcuts::command_keys;
pub use transport::chinook::prepare as prepare_chinook;
pub use transport::heal::{
    frame_camera as frame_barracks_healing, prepare as prepare_barracks_healing,
};

pub use transport::overlord::{frame_camera as frame_overlord_camera, prepare as prepare_overlord};
pub use transport::verification::{
    prepare as prepare_transports, structure_inventory as verify_structure_inventory,
};

#[derive(Component)]
pub enum Action {
    Observe(Option<usize>),
    Start,
    SinglePlayer,
    Skirmish,
    MainMenu,
    Options,
    OptionsBack,
    OptionsAccept,
    OptionsDefaults,
    OptionsAlternate,
    OptionsRetaliation,
    OptionsDoubleClickAttackMove,
    SelectMap,
    MapBack,
    MapAccept,
    MapCategory(bool),
    MapRow(usize),
    MapScroll(isize),
    ChooseFaction {
        slot: usize,
        faction: Option<Faction>,
    },
    ToggleDropdown(Dropdown),
    CloseDropdown,
    ChooseOpponent {
        slot: usize,
        opponent: OpponentSlot,
    },
    ChooseTeam {
        slot: usize,
        team: Option<u8>,
    },
    ChooseColor {
        slot: usize,
        color: Option<usize>,
    },
    ChooseStart(usize),
    ChooseStartingCash(u32),
    ToggleSuperweaponLimit,
    ConfirmMenu,
    ConfirmRestart,
    CancelConfirmation,
    Modal,
    Slot(usize),
    PowerShortcut {
        faction: usize,
        slot: usize,
    },
    CancelConstruction,
    OclTimer,
    CancelProduction(Option<(usize, i32)>),
    CancelUpgrade(Option<(usize, usize)>),
    Menu,
    Pause,
    Restart,
    Worker,
    Promotions,
    ClosePromotions,
    Science {
        faction: usize,
        index: usize,
    },
}

#[derive(Component)]
pub enum Field {
    Money,
    Tooltip,
    Faction(usize),
    Opponent(usize),
    Team(usize),
    Color(usize),
    Start(usize),
}

#[derive(Component)]
pub enum Picture {
    Bar,
    Portrait,
    Logo,
    Power(u16),
    PowerNeedle,
}

#[derive(Component)]
pub enum Layer {
    ObserverList(usize),
    ObserverInfo,
    Game,
    Results,
    ResultBanner(bool),
    LocalDefeat,
    ObserverQuit,
    Promotions,
    Shell,
    Menu,
    SinglePlayer,
    Skirmish,
    SkirmishUnderlying,
    SkirmishPlayer(usize),
    MapSelect,
    SkirmishSlot(usize),
    Dropdown(Dropdown),
    Pause,
    Confirmation(Confirmation),
    Options,
}

#[derive(Component)]
pub struct MenuButtonPart {
    parent: Entity,
    images: [String; 3],
}

#[derive(Component)]
pub struct FontSizeAt600(f32);

#[derive(Component)]
pub struct ReleaseButton {
    pub(crate) armed: bool,
}

pub fn captures_pointer(
    battle: &Match,
    window: &Window,
    cursor: Vec2,
    data: &UiData,
    transition: &PromotionTransition,
) -> bool {
    if !battle.started || battle.paused || battle.simulation.end_game_at.is_some() {
        return true;
    }
    if over_promotions(battle, window, cursor, data, transition) {
        return true;
    }
    let top = data.schemes[control_bar_index(&battle.simulation)]
        .area
        .min
        .y;
    cursor.y >= window.height() * top / 600.0
        || power_shortcuts::pointer_over(battle, data, cursor, window)
}

pub fn over_promotions(
    battle: &Match,
    window: &Window,
    cursor: Vec2,
    data: &UiData,
    transition: &PromotionTransition,
) -> bool {
    battle.promotions_open && transition.visible() && {
        let area =
            promotions::panel_area(data, faction_index(battle.simulation.players[0].faction));
        let source = cursor / Vec2::new(window.width() / 800.0, window.height() / 600.0);
        area.contains(source)
    }
}

pub fn setup(
    battle: Res<Match>,
    mut commands: Commands,
    mut data: ResMut<UiData>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
    mut icons: ResMut<Assets<PromotionIcon>>,
) {
    let battle = battle.into_inner();
    register_assets(&mut commands, &mut data, &mut images, &mut fonts);
    let data = data.into_inner();
    let scheme = &data.schemes[0];
    commands.spawn((
        Picture::Bar,
        Layer::Game,
        data.image(&scheme.background),
        at(scheme.area),
        ZIndex(1),
    ));
    commands.spawn((
        Picture::Logo,
        Layer::Game,
        data.image(&scheme.logo),
        at(data.windows["ControlBar.wnd:RightHUD"]),
        ZIndex(3),
    ));
    commands.spawn((
        Button,
        Picture::Portrait,
        Layer::Game,
        data.image(&data.units[0].portrait),
        at(data.windows["ControlBar.wnd:CameoWindow"]),
        ZIndex(4),
    ));
    commands.spawn((
        Field::Money,
        Layer::Game,
        text("", 12.0),
        at(scheme.money),
        ZIndex(4),
    ));
    for index in 0..47 {
        commands.spawn((
            Picture::Power(index),
            Layer::Game,
            data.image("PowerPointG"),
            at(scheme.power),
            ZIndex(4),
        ));
    }
    commands.spawn((
        Picture::PowerNeedle,
        Layer::Game,
        data.image("PowerBarSlider"),
        at(scheme.power),
        ZIndex(5),
    ));
    command_icons::setup(&mut commands, data, &mut icons);
    power_shortcuts::setup(&mut commands, data, &mut icons);
    superweapon_timers::setup(&mut commands, data);
    cancellation::setup(&mut commands, data, &mut icons);
    ocl_timer::setup(&mut commands, data, &mut icons);
    commands.spawn((
        Button,
        Action::Pause,
        Layer::Game,
        data.image(&scheme.options[0]),
        at(scheme.options_area),
        ZIndex(5),
    ));
    commands.spawn((
        Button,
        Action::Worker,
        Layer::Game,
        data.image(&scheme.worker[0]),
        at(scheme.worker_area),
        ZIndex(5),
    ));
    tooltip(&mut commands, data);
    menu::setup(&mut commands, data, battle);
    promotions::setup(&mut commands, data, &mut icons);
    results::setup(&mut commands, data);
    observer::setup(&mut commands, data, &mut icons);
}

fn register_assets(
    commands: &mut Commands,
    data: &mut UiData,
    images: &mut Assets<Image>,
    fonts: &mut Assets<Font>,
) {
    register_camera(commands);
    for (name, image) in std::mem::take(&mut data.textures) {
        data.handles.insert(name, images.add(image));
    }
    if let Some(font) = data.font.take() {
        data.font_handle = fonts.add(font);
    }
    if let Some(font) = data.serif_font.take() {
        data.serif_handle = fonts.add(font);
    }
    if let Some(countdown) = data
        .power_shortcuts
        .as_mut()
        .map(|shortcuts| &mut shortcuts.countdown)
        && let Some(font) = countdown.ready_font.take()
    {
        countdown.ready_handle = fonts.add(font);
    }
}

fn register_camera(commands: &mut Commands) {
    commands.spawn((
        Camera2d,
        // The native default is single-sample for the whole frame, including UI.
        Msaa::Off,
        Camera {
            order: 100,
            // World uses an encoded Rgba8Unorm framebuffer; this linear HUD
            // owns a different texture and must initialize its transparent area.
            clear_color: ClearColorConfig::Custom(Color::NONE),
            output_mode: bevy::camera::CameraOutputMode::Write {
                // UI's source-alpha draws over transparent black already
                // premultiply the accumulated RGB; preserve edge coverage once.
                blend_state: Some(
                    bevy::render::render_resource::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                ),
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
        IsDefaultUiCamera,
    ));
}

fn tooltip(commands: &mut Commands, data: &UiData) {
    let area = data.windows["ControlBarPopupDescription.wnd:Parent"];
    // The source popup grows upward from its original bottom edge to fit text.
    commands.spawn((
        Field::Tooltip,
        Layer::Game,
        text("", 12.0),
        Node {
            position_type: PositionType::Absolute,
            left: percent(area.min.x / 8.0),
            bottom: percent((600.0 - area.max.y) / 6.0),
            width: percent(area.width() / 8.0),
            min_height: percent(area.height() / 6.0),
            padding: UiRect::all(px(8)),
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.015, 0.025, 0.02, 0.94)),
        BorderColor::all(Color::srgb(0.42, 0.48, 0.30)),
        ZIndex(15),
    ));
}

pub fn at(rect: Rect) -> Node {
    // ControlBarScheme scales each axis from its 800×600 creation resolution.
    Node {
        position_type: PositionType::Absolute,
        left: percent(rect.min.x / 8.0),
        top: percent(rect.min.y / 6.0),
        width: percent(rect.width() / 8.0),
        height: percent(rect.height() / 6.0),
        ..default()
    }
}

pub fn text(value: &str, size: f32) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::srgb(0.99, 0.99, 0.99)),
        TextLayout::justify(Justify::Center),
        FontSizeAt600(size),
    )
}

const fn faction_index(faction: Faction) -> usize {
    match faction {
        Faction::Usa => 0,
        Faction::China => 1,
        Faction::Gla => 2,
        Faction::Civilian => panic!("Neutral has no player control bar"),
    }
}

fn control_bar_index(game: &generals_sim::skirmish::Skirmish) -> usize {
    if game.player_active(0) {
        faction_index(game.players[0].faction)
    } else {
        3
    }
}

pub fn selected_command<'a>(battle: &Match, data: &'a UiData, slot: usize) -> Option<&'a Command> {
    if !battle.simulation.player_active(0) {
        return None;
    }
    let mut common: Option<&Command> = None;
    let mut first = true;
    for &id in &battle.selected {
        let unit = battle.simulation.entities.get(id)?;
        // ControlBarMultiSelect excludes the selected Mob members from its buttons.
        if battle.simulation.catalog[unit.type_index]
            .progression
            .ignored_in_gui
        {
            continue;
        }
        if !unit.complete()
            || (unit.owner != 0
                && !(unit.owner == battle.simulation.neutral_owner()
                    && data.units.get(unit.type_index)?.structure_inventory))
            || (unit.masked() && !unit.spawn_nexus())
        {
            return None;
        }
        let candidate = data
            .commands(&battle.simulation, id)
            .iter()
            .find(|command| command.slot == slot)
            .filter(|command| command_icons::visible(battle, command))
            .filter(|command| power_science_visible(battle, id, command));
        if first {
            common = candidate.filter(|command| battle.selected.len() == 1 || command.multi_select);
            first = false;
        } else {
            // ControlBarMultiSelect::addCommonCommands preserves attack-move if
            // any selected unit offers it; other slots require the same button.
            let attack_move = candidate
                .is_some_and(|command| matches!(command.action, CommandAction::AttackMove))
                || common
                    .is_some_and(|command| matches!(command.action, CommandAction::AttackMove));
            if attack_move && common.is_none() {
                common = candidate;
            } else if !attack_move
                && candidate.map(|command| &command.name) != common.map(|command| &command.name)
            {
                common = None;
            }
        }
    }
    common
}

fn power_science_visible(battle: &Match, caster: usize, command: &Command) -> bool {
    let Some(power) = command.action.power() else {
        return true;
    };
    let actor = &battle.simulation.entities[caster];
    let unit = &battle.simulation.catalog[actor.type_index];
    let science = unit
        .special_powers
        .iter()
        .find(|rules| rules.name == power)
        .map_or_else(
            || command.sciences.first().map_or("", String::as_str),
            |rules| &rules.required_science,
        );
    science.is_empty()
        || science.eq_ignore_ascii_case("NONE")
        || battle.simulation.players[actor.owner].has_science(science)
}

type ButtonState<'a> = (
    &'a Interaction,
    &'a Action,
    Option<&'a mut ImageNode>,
    Option<&'a mut ReleaseButton>,
);

type MenuOverlays<'w> = (
    Option<ResMut<'w, crate::map_selection::MapSelection>>,
    Option<Res<'w, crate::playtest_view::Capture>>,
    Option<ResMut<'w, options::Editor>>,
);

pub fn buttons(
    mut query: Query<ButtonState<'_>, Changed<Interaction>>,
    ui: (
        Res<UiData>,
        ResMut<PromotionTransition>,
        ResMut<crate::source_audio::SourceAudio>,
        ResMut<ResultsScreen>,
    ),
    state: (ResMut<Menu>, ResMut<Match>, ResMut<CameraRig>),
    input: (ResMut<ButtonInput<MouseButton>>, ResMut<Mouse>),
    map_state: MenuOverlays<'_>,
) {
    let (mut menu, mut battle, mut rig) = state;
    let (mut mouse, mut pointer) = input;
    let (mut maps, capture, mut options) = map_state;
    let capture = capture.map(Res::into_inner);
    let (data, mut transition, mut audio, mut results) = ui;
    let data = data.into_inner();
    let pressed = mouse.just_pressed(MouseButton::Left);
    for (interaction, action, image, release) in &mut query {
        if menu.confirmation == Some(Confirmation::MapUnavailable)
            && !matches!(action, Action::CancelConfirmation | Action::Modal)
        {
            continue;
        }
        if menu.options_open
            && !matches!(
                action,
                Action::OptionsBack
                    | Action::OptionsAccept
                    | Action::OptionsDefaults
                    | Action::OptionsAlternate
                    | Action::OptionsRetaliation
                    | Action::OptionsDoubleClickAttackMove
            )
        {
            continue;
        }
        // SkirmishMapSelectMenu keeps the original Back visible but disabled.
        if maps.as_deref().is_some_and(|maps| {
            (maps.open && matches!(action, Action::MainMenu))
                || (matches!(action, Action::Start)
                    && !maps.maps[maps.current].admission.is_verified())
        }) {
            continue;
        }
        if battle.started && battle.simulation.end_game_at.is_some() && !results.scores() {
            continue;
        }
        if pressed
            && *interaction == Interaction::Pressed
            && button_enabled(action, &mut battle, &menu, data, &pointer)
        {
            play_press_sound(action, &mut audio);
        }
        if let Some(mut image) = image {
            image.color = if *interaction == Interaction::Hovered
                && button_enabled(action, &mut battle, &menu, data, &pointer)
                && !matches!(action, Action::Observe(_))
            {
                Color::srgb(1.15, 1.15, 1.15)
            } else {
                Color::WHITE
            };
        }
        let activated = release.map_or_else(
            || *interaction == Interaction::Pressed,
            |mut release| released_button(*interaction, &mut release, &mut mouse),
        );
        if !activated {
            continue;
        }
        if matches!(action, Action::ToggleDropdown(_))
            && button_enabled(action, &mut battle, &menu, data, &pointer)
        {
            audio.ui("GUIClick");
        }
        // Closing the source pause window must not also click the world behind it.
        mouse.clear_just_pressed(MouseButton::Left);
        mouse.clear_just_released(MouseButton::Left);
        if matches!(action, Action::ConfirmMenu) && menu.confirmation == Some(Confirmation::Exit) {
            results.enter(&mut battle, 120);
            menu.clear_confirmation();
            continue;
        }
        if matches!(action, Action::Start | Action::ConfirmRestart) {
            results.elapsed = None;
        }
        if matches!(action, Action::Start | Action::MainMenu)
            && matches!(menu.page, Page::Skirmish)
            && capture.is_none_or(|capture| !capture.is_pending())
        {
            menu.save(data, &battle);
        }
        if options
            .as_deref_mut()
            .is_some_and(|editor| options::activate(action, editor))
        {
            continue;
        }
        if maps
            .as_deref_mut()
            .is_some_and(|maps| maps.activate(action))
            || observer::activate(action, &mut battle)
            || menu::activate(action, &mut menu, &mut battle)
            || promotions::activate(action, data, &mut battle, &mut transition)
            || cancellation::activate(action, &mut battle)
        {
            continue;
        }
        gameplay_button(action, &mut battle, &mut pointer, &mut rig, &mut menu, data);
    }
}

fn play_press_sound(action: &Action, audio: &mut crate::source_audio::SourceAudio) {
    match action {
        Action::ToggleDropdown(_) => {}
        Action::PowerShortcut { .. } => audio.ui("GUIGenShortcutClick"),
        _ => audio.ui("GUIClick"),
    }
}

fn gameplay_button(
    action: &Action,
    battle: &mut Match,
    pointer: &mut Mouse,
    rig: &mut CameraRig,
    menu: &mut Menu,
    data: &UiData,
) {
    match *action {
        Action::Slot(slot) => {
            if battle.started
                && !battle.paused
                && let Some(command) = selected_command(battle, data, slot)
                && command_icons::available(battle, command)
            {
                dispatch_command(battle, pointer, command.action);
            }
        }
        Action::OclTimer => {
            if !battle.paused
                && let Some(command) = ocl_timer::command(battle, data)
                && command_icons::available(battle, command)
            {
                dispatch_command(battle, pointer, command.action);
            }
        }
        Action::PowerShortcut { faction, slot } => {
            power_shortcuts::activate(battle, pointer, data, faction, slot);
        }
        Action::Worker => select_idle_worker(battle, rig, pointer, menu),
        _ => {}
    }
}

fn select_idle_worker(
    battle: &mut Match,
    rig: &mut CameraRig,
    pointer: &mut Mouse,
    menu: &mut Menu,
) {
    if let Some(id) = pointer.next_idle_worker(&battle.selected) {
        // ControlBarSystem's idle-worker action calls HideQuitMenu.
        battle.paused = false;
        menu.clear_confirmation();
        let worker = &battle.simulation.entities[id];
        rig.focus = Vec2::from_array(worker.position) * 0.1;
        battle.select_objects(&[id], true);
        pointer.selection_voice(battle);
    }
}

fn button_enabled(
    action: &Action,
    battle: &mut Match,
    menu: &Menu,
    data: &UiData,
    pointer: &Mouse,
) -> bool {
    match *action {
        Action::Modal | Action::CloseDropdown => false,
        Action::ToggleDropdown(dropdown) => menu.enabled(dropdown),
        Action::ChooseColor { slot, color } => menu.color_available(slot, color),
        Action::Slot(slot) => selected_command(battle, data, slot)
            .is_some_and(|command| command_icons::available(battle, command)),
        Action::OclTimer => ocl_timer::command(battle, data)
            .is_some_and(|command| command_icons::available(battle, command)),
        Action::PowerShortcut { faction, slot } => power_shortcuts::available(battle, data, faction, slot),
        Action::Science { faction, index } => battle
            .simulation
            .can_purchase_science(0, &data.promotions.buttons[faction][index].science),
        Action::Worker => battle.resource_owner().is_some() && !pointer.idle_workers.is_empty(),
        Action::Promotions => battle.simulation.player_active(0),
        Action::CancelConstruction => cancellation::foundation(battle).is_some(),
        Action::CancelUpgrade(binding) => binding.is_some_and(|(producer, index)| {
            battle.simulation.entities.get(producer).is_some_and(|actor| actor.alive() && actor.owner == 0
                && actor.queue.iter().any(|entry| matches!(entry.item, generals_sim::skirmish::ProductionItem::Upgrade(value) if value == index)))
        }),
        Action::CancelProduction(binding) => binding.is_some_and(|(producer, id)| {
            cancellation::queued_entry(battle, producer, id).is_some()
        }),
        _ => true,
    }
}

fn released_button(
    interaction: Interaction,
    button: &mut ReleaseButton,
    mouse: &mut ButtonInput<MouseButton>,
) -> bool {
    // GadgetPushButtonInput dispatches ordinary menu buttons on LEFT_UP. Leaving
    // the button cancels its selected state; a held pointer cannot re-arm it.
    match interaction {
        Interaction::Pressed => {
            button.armed = mouse.just_pressed(MouseButton::Left);
            mouse.clear_just_pressed(MouseButton::Left);
            false
        }
        Interaction::Hovered => {
            let armed = std::mem::take(&mut button.armed);
            armed && mouse.just_released(MouseButton::Left)
        }
        Interaction::None => {
            button.armed = false;
            false
        }
    }
}

pub fn dispatch_command(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) {
    mouse.pending_command = CommandMode::Normal;
    if lotus_targeting(battle, mouse, action) {
        return;
    }
    if queue_command(battle, mouse, action) {
        return;
    }
    execute_command(battle, mouse, action);
}

pub fn execute_command(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) {
    match action {
        CommandAction::Produce(product) => product_order(battle, product),
        CommandAction::DetonateFake => {
            for id in battle.command_message_members() {
                if let Err(error) = battle.simulation.detonate_fake(0, id) {
                    eprintln!("Fake-building detonation rejected: {error}");
                }
            }
        }
        CommandAction::BattlePlan(plan) => {
            battle.placing = None;
            if let Some(&id) = battle.selected.first() {
                drop(battle.command_message_members());
                battle.simulation.choose_battle_plan(id, plan);
            }
        }
        CommandAction::Evacuate | CommandAction::ExitContainer(_) => {
            battle.placing = None;
            transport::dispatch(battle, mouse, action);
        }
        CommandAction::SwitchWeapon(slot) => switch_weapon(battle, mouse, slot),
        CommandAction::HackInternet => hack_internet(battle, mouse),
        CommandAction::CarBomb
        | CommandAction::Hijack
        | CommandAction::HelixBomb
        | CommandAction::Sabotage
        | CommandAction::DisableBuilding
        | CommandAction::Hack(_)
        | CommandAction::AttackMove
        | CommandAction::CombatDrop
        | CommandAction::Guard
        | CommandAction::GuardFlying
        | CommandAction::Rally
        | CommandAction::Capture => {
            battle.placing = None;
            mouse.pending_command = command_target_mode(action);
        }
        CommandAction::FireWeapon(slot) => target_location_weapon(battle, mouse, slot),
        CommandAction::ClearMines(slot) => {
            crate::input::contamination::mine_mode(battle, mouse, slot);
        }
        CommandAction::CashHack
        | CommandAction::EmergencyRepair
        | CommandAction::SpectreGunship
        | CommandAction::AirForceSpectreGunship
        | CommandAction::NativePower(_) => {
            let power = action.power_command().expect("power action");
            if let Some(caster) = battle
                .selected
                .iter()
                .copied()
                .find(|&caster| battle.simulation.can_use_power(caster, power.name))
            {
                if power.name == "SpecialAbilityBoobyTrap" {
                    mouse.voices.push(crate::source_audio::VoiceRequest::new(
                        &battle.simulation,
                        &[caster],
                        crate::source_audio::Voice::BoobyTrapMode,
                        false,
                    ));
                }
                dispatch_power(battle, mouse, caster, power, false);
            }
        }
        CommandAction::Stop => stop_selected(battle),
        CommandAction::Research(index) => research_order(battle, mouse, index),
        CommandAction::Sell => {
            for id in battle.command_message_members() {
                if let Err(error) = battle.simulation.sell(0, id) {
                    eprintln!("Sell command rejected: {error}");
                }
            }
        }
        CommandAction::CancelConstruction => {
            cancellation::activate(&Action::CancelConstruction, battle);
        }
        CommandAction::LaserLock
        | CommandAction::BurtonKnife
        | CommandAction::BurtonCharge(_)
        | CommandAction::BurtonDetonate
        | CommandAction::BombTruckDisguise
        | CommandAction::BombTruckDetonate
        | CommandAction::SnipeVehicle(_)
        | CommandAction::Cleanup => dispatch_unit_command(battle, mouse, action),
    }
}

fn queue_command(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) -> bool {
    if !battle.commands.executing
        && let CommandAction::ExitContainer(slot) = action
    {
        battle.placing = None;
        if let Some(passenger) = transport::passenger(battle, slot)
            && crate::input::locally_controlled(battle, passenger)
        {
            crate::input::commands::enqueue(
                battle,
                Some(mouse),
                crate::input::commands::Kind::ExitContainer(passenger),
            );
        }
        return true;
    }
    if !battle.commands.executing
        && let CommandAction::Research(index) = action
    {
        queue_research(battle, mouse, index);
        return true;
    }
    if !battle.commands.executing && matches!(action, CommandAction::CancelConstruction) {
        cancellation::activate(&Action::CancelConstruction, battle);
        return true;
    }
    let world_action = !matches!(action, CommandAction::Produce(product) if battle.simulation.catalog[product].kind.is_building());
    if !battle.commands.executing
        && world_action
        && matches!(
            action,
            CommandAction::Produce(_)
                | CommandAction::DetonateFake
                | CommandAction::BattlePlan(_)
                | CommandAction::Evacuate
                | CommandAction::ExitContainer(_)
                | CommandAction::SwitchWeapon(_)
                | CommandAction::HackInternet
                | CommandAction::Stop
                | CommandAction::Research(_)
                | CommandAction::Sell
                | CommandAction::CancelConstruction
                | CommandAction::BurtonDetonate
                | CommandAction::BombTruckDetonate
        )
    {
        if matches!(
            action,
            CommandAction::BattlePlan(_)
                | CommandAction::Evacuate
                | CommandAction::ExitContainer(_)
        ) {
            battle.placing = None;
        }
        crate::input::commands::enqueue(
            battle,
            Some(mouse),
            crate::input::commands::Kind::Action(action),
        );
        return true;
    }
    if !battle.commands.executing
        && let CommandAction::ClearMines(slot) = action
    {
        let mut accepted = false;
        for index in 0..battle.selected.len() {
            let id = battle.selected[index];
            accepted |= crate::input::locally_controlled(battle, id)
                && battle.simulation.mine_clearing_ready(id, slot);
        }
        if accepted {
            crate::input::commands::enqueue(
                battle,
                Some(mouse),
                crate::input::commands::Kind::Action(action),
            );
            battle.placing = None;
            mouse.pending_command = CommandMode::ClearMines(slot);
        }
        return true;
    }
    false
}

fn queue_research(battle: &mut Match, mouse: &mut Mouse, index: usize) {
    if let Some(&producer) = battle.selected.first() {
        let kind = battle.simulation.entities[producer].type_index;
        if let Some(upgrade) = battle.simulation.catalog[kind]
            .maintenance
            .research
            .get(index)
        {
            let name = upgrade.name.clone();
            let voice = match name.as_str() {
                "Upgrade_GLABombTruckBioBomb" => {
                    Some(crate::source_audio::Voice::BombTruckBioBombMode)
                }
                "Upgrade_GLABombTruckHighExplosiveBomb" => {
                    Some(crate::source_audio::Voice::BombTruckHighExplosiveMode)
                }
                _ => None,
            };
            if let Some(voice) = voice {
                mouse.voices.push(crate::source_audio::VoiceRequest::new(
                    &battle.simulation,
                    battle.command_drawables(),
                    voice,
                    false,
                ));
            }
            crate::input::commands::enqueue(
                battle,
                Some(mouse),
                crate::input::commands::Kind::Research(name),
            );
        }
    }
}

fn dispatch_power(
    battle: &mut Match,
    mouse: &mut Mouse,
    caster: usize,
    command: PowerCommand,
    specific_source: bool,
) {
    battle.placing = None;
    mouse.pending_command = CommandMode::Normal;
    match command.aim {
        PowerAim::Immediate => {
            crate::input::cast_command_power(
                battle,
                caster,
                command.name,
                || generals_sim::skirmish::PowerTarget::None,
                specific_source,
            );
        }
        PowerAim::Construct(kind) => {
            battle.placing = Some(crate::input::placement::Placement::special_power(
                kind,
                caster,
                battle.simulation.catalog[kind].placement_angle,
                command.name,
            ));
        }
        PowerAim::Object | PowerAim::Location => {
            mouse.pending_command = CommandMode::Power {
                caster,
                name: command.name,
                target_object: matches!(command.aim, PowerAim::Object),
                specific_source,
            };
        }
    }
}

fn dispatch_unit_command(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) {
    match action {
        CommandAction::LaserLock => {
            targeting_voice(
                battle,
                mouse,
                CommandMode::LaserLock,
                crate::source_audio::Voice::LaserMode,
            );
        }
        CommandAction::BurtonKnife => targeting_voice(
            battle,
            mouse,
            CommandMode::Burton(crate::input::burton::TargetCommand::Knife),
            crate::source_audio::Voice::BurtonKnifeMode,
        ),
        CommandAction::BurtonCharge(charge) => {
            use generals_sim::skirmish::special_ability::BurtonCharge;
            let voice = match charge {
                BurtonCharge::Timed => crate::source_audio::Voice::BurtonTimedMode,
                BurtonCharge::Remote => crate::source_audio::Voice::BurtonRemoteMode,
            };
            targeting_voice(
                battle,
                mouse,
                CommandMode::Burton(crate::input::burton::TargetCommand::Charge(charge)),
                voice,
            );
        }
        CommandAction::BurtonDetonate => crate::input::burton::detonate(battle, mouse),
        CommandAction::BombTruckDisguise => targeting_voice(
            battle,
            mouse,
            CommandMode::BombTruckDisguise,
            crate::source_audio::Voice::BombTruckDisguiseMode,
        ),
        CommandAction::BombTruckDetonate => crate::input::bomb_truck::detonate(battle),
        CommandAction::SnipeVehicle(slot) => {
            targeting_voice(
                battle,
                mouse,
                CommandMode::SnipeVehicle(slot),
                crate::source_audio::Voice::SnipeMode,
            );
        }
        CommandAction::Cleanup => {
            targeting_voice(
                battle,
                mouse,
                CommandMode::Cleanup,
                crate::source_audio::Voice::CleanupMode,
            );
        }
        _ => unreachable!("dispatched unit command"),
    }
}

fn research_order(battle: &mut Match, mouse: &mut Mouse, index: usize) {
    if let Some(&producer) = battle.selected.first() {
        if crate::input::bomb_truck::research_payload(battle, mouse, producer, index) {
            return;
        }
        drop(battle.command_message_members());
        if let Err(error) = battle.simulation.queue_upgrade(producer, index) {
            eprintln!("Research rejected: {error}");
        }
    }
}

fn command_target_mode(action: CommandAction) -> CommandMode {
    match action {
        CommandAction::HelixBomb => CommandMode::HelixBomb,
        CommandAction::Hijack => CommandMode::Hijack,
        CommandAction::CarBomb => CommandMode::CarBomb,
        CommandAction::Sabotage => CommandMode::Sabotage,
        CommandAction::DisableBuilding => CommandMode::DisableBuilding,
        CommandAction::Hack(ability) => CommandMode::Hack(ability),
        CommandAction::AttackMove => CommandMode::AttackMove,
        CommandAction::CombatDrop => CommandMode::CombatDrop,
        CommandAction::Guard => CommandMode::Guard,
        CommandAction::GuardFlying => CommandMode::GuardFlying,
        CommandAction::Rally => CommandMode::Rally,
        CommandAction::Capture => CommandMode::Capture,
        _ => unreachable!("targeted command mode"),
    }
}

fn target_location_weapon(battle: &mut Match, mouse: &mut Mouse, slot: usize) {
    let voice = if slot == 2 {
        crate::source_audio::Voice::RocketPodMode
    } else if battle.selected.iter().any(|&id| {
        battle.simulation.catalog[battle.simulation.entities[id].type_index].name
            == "ChinaTankDragon"
    }) {
        crate::source_audio::Voice::FlameMode
    } else {
        crate::source_audio::Voice::ContaminateMode
    };
    targeting_voice(battle, mouse, CommandMode::FireWeapon(slot), voice);
}

fn stop_selected(battle: &mut Match) {
    for id in battle.command_message_members() {
        if battle
            .simulation
            .entities
            .get(id)
            .is_some_and(|unit| unit.owner == 0)
            && let Err(error) = battle.simulation.stop(id)
        {
            eprintln!("Stop command rejected: {error}");
        }
    }
}

fn targeting_voice(
    battle: &mut Match,
    mouse: &mut Mouse,
    mode: CommandMode,
    voice: crate::source_audio::Voice,
) {
    battle.placing = None;
    mouse.pending_command = mode;
    mouse.voices.push(crate::source_audio::VoiceRequest::new(
        &battle.simulation,
        battle.command_drawables(),
        voice,
        false,
    ));
}

fn lotus_targeting(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) -> bool {
    use crate::source_audio::Voice;
    if !battle.selected.first().is_some_and(|&id| {
        battle.simulation.catalog[battle.simulation.entities[id].type_index].behavior.special_abilities.iter()
            .any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding))
    }) {
        return false;
    }
    let (mode, voice) = match action {
        CommandAction::Capture => (CommandMode::Capture, Voice::LotusCaptureMode),
        CommandAction::Hack(1) => (CommandMode::Hack(1), Voice::LotusVehicleMode),
        CommandAction::Hack(2) => (CommandMode::Hack(2), Voice::LotusCashMode),
        _ => return false,
    };
    targeting_voice(battle, mouse, mode, voice);
    true
}

fn hack_internet(battle: &mut Match, mouse: &mut Mouse) {
    for id in battle.command_message_members() {
        if battle
            .simulation
            .entities
            .get(id)
            .is_some_and(|unit| unit.owner == 0)
            && let Err(error) = battle.simulation.hack_internet(id)
        {
            eprintln!("Internet hack rejected: {error}");
        }
    }
    mouse.voices.push(crate::source_audio::VoiceRequest::new(
        &battle.simulation,
        battle.command_drawables(),
        crate::source_audio::Voice::HackInternet,
        false,
    ));
}

fn switch_weapon(battle: &mut Match, mouse: &mut Mouse, slot: usize) {
    let response = crate::source_audio::VoiceRequest::new(
        &battle.simulation,
        battle.command_drawables(),
        if slot == 0 {
            crate::source_audio::Voice::PrimaryWeaponMode
        } else {
            crate::source_audio::Voice::SecondaryWeaponMode
        },
        false,
    );
    for id in battle.command_message_members() {
        if battle
            .simulation
            .entities
            .get(id)
            .is_some_and(|actor| actor.owner == 0 && actor.complete())
            && let Err(error) = battle.simulation.switch_weapon(id, slot)
        {
            eprintln!("Weapon switch rejected: {error}");
        }
    }
    mouse.voices.push(response);
}

fn product_order(battle: &mut Match, type_index: usize) {
    let Some(&selected) = battle.selected.first() else {
        return;
    };
    if battle.simulation.catalog[type_index].kind.is_building() {
        battle.placing = Some(crate::input::placement::Placement::new(
            type_index,
            selected,
            battle.simulation.catalog[type_index].placement_angle,
        ));
    } else {
        drop(battle.command_message_members());
        if let Err(error) = battle.simulation.train(selected, type_index) {
            eprintln!("Production order rejected: {error}");
        }
    }
}

pub fn update(
    battle: Res<Match>,
    ui: (Res<UiData>, Res<Menu>, Res<ResultsScreen>, Res<Mouse>),
    mut layers: Query<(&Layer, &mut Visibility)>,
    portraits: Query<(&Picture, &Interaction)>,
    mut actions: Query<(&Action, &Interaction, Option<&mut ImageNode>, &mut Node)>,
    mut fields: Query<(&Field, &mut Text, &mut Node, &mut TextColor), Without<Action>>,
    maps: Option<Res<crate::map_selection::MapSelection>>,
) {
    let (data, menu, screen, pointer) = ui;
    let battle = battle.into_inner();
    let data = data.into_inner();
    let simulation = &battle.simulation;
    let scheme = &data.schemes[control_bar_index(simulation)];
    let menu = menu.into_inner();
    let maps = maps.map(Res::into_inner);
    update_layers(battle, menu, &screen, maps, &mut layers);
    let mut tooltip = selection_tooltip(battle, data, &portraits);
    update_actions(battle, data, &pointer, &mut actions, &mut tooltip);
    for (field, mut text, mut node, mut color) in &mut fields {
        let value = match field {
            Field::Faction(slot) => {
                color.set_if_neq(TextColor(if menu.enabled(Dropdown::Faction(*slot)) {
                    Color::srgb_u8(255, 254, 254)
                } else {
                    Color::srgb_u8(122, 48, 1)
                }));
                menu.factions[*slot].map_or_else(
                    || data.label("GUI:Random").to_owned(),
                    |faction| data.factions[faction_index(faction)].clone(),
                )
            }
            Field::Opponent(slot) => data.label(menu.opponents[*slot].label()).to_owned(),
            Field::Team(slot) => data
                .label(&format!(
                    "Team:{}",
                    menu.teams[*slot].map_or(0, |team| team + 1)
                ))
                .to_owned(),
            Field::Color(slot) => {
                color.set_if_neq(TextColor(
                    menu.colors[*slot].map_or(Color::WHITE, |index| {
                        Color::srgb_from_array(data.results.colors[index])
                    }),
                ));
                menu.colors[*slot].map_or_else(
                    || data.label("GUI:???").to_owned(),
                    |index| data.color_labels[index].clone(),
                )
            }
            Field::Start(position) => menu
                .starts
                .iter()
                .position(|start| *start == Some(*position))
                .map_or_else(String::new, |owner| {
                    data.label(&format!("NUMBER:{}", owner + 1)).to_owned()
                }),
            Field::Money => {
                let mut next = at(scheme.money);
                if battle.resource_owner().is_none() {
                    next.display = Display::None;
                }
                node.set_if_neq(next);
                format!(
                    "${}",
                    battle
                        .resource_owner()
                        .map_or(0, |owner| simulation.players[owner].cash)
                )
            }
            Field::Tooltip => {
                node.reborrow()
                    .map_unchanged(|node| &mut node.display)
                    .set_if_neq(if tooltip.is_empty() || battle.paused {
                        Display::None
                    } else {
                        Display::Flex
                    });
                tooltip.clone()
            }
        };
        text.reborrow()
            .map_unchanged(|text| &mut text.0)
            .set_if_neq(value);
    }
}

fn update_actions(
    battle: &Match,
    data: &UiData,
    pointer: &Mouse,
    actions: &mut Query<(&Action, &Interaction, Option<&mut ImageNode>, &mut Node)>,
    tooltip: &mut String,
) {
    let simulation = &battle.simulation;
    let scheme = &data.schemes[control_bar_index(simulation)];
    for (action, interaction, image, mut node) in actions {
        if *interaction != Interaction::None
            && let Some(description) = action_tooltip(action, battle, data)
        {
            *tooltip = description;
        }
        let Some(mut image) = image else {
            continue;
        };
        let state = match interaction {
            Interaction::None => 0,
            Interaction::Hovered => 1,
            Interaction::Pressed => 2,
        };
        let appearance = match *action {
            Action::Promotions => Some((
                &scheme.general[if simulation.player_active(0) {
                    state
                } else {
                    3
                }],
                scheme.general_area,
            )),
            Action::Pause => Some((&scheme.options[state], scheme.options_area)),
            Action::Worker => {
                let state = if pointer.idle_workers.is_empty() {
                    3
                } else {
                    state
                };
                Some((&scheme.worker[state], scheme.worker_area))
            }
            _ => None,
        };
        if let Some((name, area)) = appearance {
            set_image(&mut image, data.image(name));
            let mut next = at(area);
            if matches!(action, Action::Worker) && battle.resource_owner().is_none() {
                next.display = Display::None;
            }
            node.set_if_neq(next);
        }
    }
}

fn action_tooltip(action: &Action, battle: &Match, data: &UiData) -> Option<String> {
    match *action {
        Action::Science { faction, index }
            if battle.promotions_open
                && faction == faction_index(battle.simulation.players[0].faction) =>
        {
            let science = &data.promotions.buttons[faction][index];
            let cost = data
                .label("TOOLTIP:ScienceCost")
                .replace("%d", &science.cost.to_string());
            Some(format!(
                "{}\n{cost}\n{}",
                science.label, science.description
            ))
        }
        Action::Promotions => Some(format!(
            "{}\n{}",
            data.label("CONTROLBAR:GeneralsExperienceMenu"),
            data.label("CONTROLBAR:GeneralsExperienceMenuDescription")
        )),
        Action::Slot(slot) => {
            let command = selected_command(battle, data, slot)?;
            Some(command.action.product().map_or_else(
                || format!("{}\n{}", command.label, command.description),
                |index| {
                    format!(
                        "{}\n${}\n{}",
                        command.label,
                        battle.simulation.build_cost(0, index),
                        command.description
                    )
                },
            ))
        }
        Action::CancelConstruction => Some(format!(
            "{}\n{}",
            data.cancel_construction.label, data.cancel_construction.description
        )),
        Action::OclTimer => {
            let command = ocl_timer::command(battle, data)?;
            Some(format!("{}\n{}", command.label, command.description))
        }
        Action::PowerShortcut { faction, slot } => {
            let command = power_shortcuts::command(battle, data, faction, slot)?;
            let (label, description) = command_icons::power_text(battle, data, command);
            Some(format!("{label}\n{description}"))
        }
        _ => None,
    }
}

fn update_layers(
    battle: &Match,
    menu: &Menu,
    screen: &ResultsScreen,
    maps: Option<&crate::map_selection::MapSelection>,
    layers: &mut Query<(&Layer, &mut Visibility)>,
) {
    for (layer, mut visibility) in layers {
        let visible = match layer {
            Layer::Options => menu.options_open,
            Layer::ObserverList(index) => {
                battle.started
                    && !screen.scores()
                    && !battle.simulation.player_active(0)
                    && battle.observer_player.is_none()
                    && *index < battle.observer_players.len()
            }
            Layer::ObserverInfo => {
                battle.started
                    && !screen.scores()
                    && !battle.simulation.player_active(0)
                    && battle.observer_player.is_some()
            }
            Layer::Game => battle.started && !screen.scores(),
            Layer::Results => battle.started && screen.scores(),
            Layer::ResultBanner(victorious) => battle.started && screen.banner(*victorious),
            Layer::LocalDefeat => battle.started && screen.local_defeat(),
            Layer::ObserverQuit => battle.started && screen.observer_quit(),
            Layer::Promotions => {
                battle.started
                    && !battle.paused
                    && battle.promotions_open
                    && battle.simulation.player_active(0)
                    && battle.simulation.end_game_at.is_none()
            }
            Layer::Shell => !battle.started,
            Layer::Menu => !battle.started && !matches!(menu.page, Page::Skirmish),
            Layer::SinglePlayer => !battle.started && matches!(menu.page, Page::SinglePlayer),
            Layer::SkirmishUnderlying => !battle.started && matches!(menu.page, Page::Skirmish),
            Layer::SkirmishPlayer(slot) => {
                !battle.started
                    && matches!(menu.page, Page::Skirmish)
                    && *slot < battle.simulation.slots.len()
            }
            Layer::Skirmish => {
                !battle.started
                    && matches!(menu.page, Page::Skirmish)
                    && maps.is_none_or(|maps| !maps.open)
            }
            Layer::SkirmishSlot(slot) => {
                !battle.started
                    && matches!(menu.page, Page::Skirmish)
                    && *slot < battle.simulation.slots.len()
                    && maps.is_none_or(|maps| !maps.open)
            }
            Layer::MapSelect => {
                !battle.started
                    && matches!(menu.page, Page::Skirmish)
                    && maps.is_some_and(|maps| maps.open)
            }
            Layer::Dropdown(dropdown) => {
                !battle.started
                    && (maps.is_none_or(|maps| !maps.open)
                        || matches!(dropdown, Dropdown::Player(_) | Dropdown::StartingCash))
                    && matches!(menu.page, Page::Skirmish)
                    && menu.open == Some(*dropdown)
            }
            Layer::Pause => battle.started && battle.paused && !screen.scores(),
            Layer::Confirmation(kind) => {
                menu.confirmation == Some(*kind)
                    && if *kind == Confirmation::MapUnavailable {
                        !battle.started
                    } else {
                        battle.started && battle.paused
                    }
            }
        };
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn selection_tooltip(
    battle: &Match,
    data: &UiData,
    portraits: &Query<(&Picture, &Interaction)>,
) -> String {
    if portraits.iter().any(|(picture, interaction)| {
        matches!(picture, Picture::Portrait) && *interaction == Interaction::Hovered
    }) && let Some(unit) = battle
        .selected
        .first()
        .and_then(|&id| battle.simulation.entities.get(id))
        .filter(|unit| unit.alive())
    {
        return format!(
            "{}\n{:.0} / {:.0}",
            data.units[unit.type_index].name, unit.health, unit.max_health
        );
    }
    String::new()
}

fn show_portrait(
    game: &generals_sim::skirmish::Skirmish,
    unit: &generals_sim::skirmish::Entity,
    ui: &crate::control_bar::UnitUi,
) -> bool {
    (unit.owner == 0
        || !game.catalog[unit.type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "SHOW_PORTRAIT_WHEN_CONTROLLED"))
        && (unit.queue.is_empty() || !game.player_active(0))
        && !ui.portrait.is_empty()
}

pub fn pictures(
    battle: Res<Match>,
    data: Res<UiData>,
    mut pictures: Query<(&Picture, &mut ImageNode, &mut Node), Without<Action>>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let simulation = &battle.simulation;
    let scheme = &data.schemes[control_bar_index(simulation)];
    let selected = battle
        .selected
        .first()
        .and_then(|&id| simulation.entities.get(id))
        .filter(|unit| unit.alive());
    let (production, consumption) = power(battle);
    let show_power = battle
        .resource_owner()
        .is_some_and(|owner| simulation.players[owner].faction != Faction::Gla);
    for (picture, mut image, mut node) in &mut pictures {
        match *picture {
            Picture::Bar => {
                set_image(&mut image, data.image(&scheme.background));
                node.set_if_neq(at(scheme.area));
            }
            Picture::Logo => {
                set_image(&mut image, data.image(&scheme.logo));
                node.reborrow()
                    .map_unchanged(|node| &mut node.display)
                    .set_if_neq(if selected.is_none() {
                        Display::Flex
                    } else {
                        Display::None
                    });
            }
            Picture::Portrait => {
                node.reborrow()
                    .map_unchanged(|node| &mut node.display)
                    .set_if_neq(
                        if selected.is_some_and(|unit| {
                            show_portrait(simulation, unit, &data.units[unit.type_index])
                        }) {
                            Display::Flex
                        } else {
                            Display::None
                        },
                    );
                if let Some(unit) =
                    selected.filter(|unit| !data.units[unit.type_index].portrait.is_empty())
                {
                    set_image(
                        &mut image,
                        data.image(&data.units[unit.type_index].portrait),
                    );
                }
            }
            Picture::Power(index) => {
                let width = data.image_size("PowerPointG").x;
                let start = f32::from(index) * width;
                let range = power_range(production, data) * scheme.power.width();
                let image_name = if consumption > production {
                    "PowerPointR"
                } else if consumption > production - f32::from(data.power_yellow) {
                    "PowerPointY"
                } else {
                    "PowerPointG"
                };
                set_image(&mut image, data.image(image_name));
                let mut next = at(Rect::new(
                    scheme.power.min.x + start,
                    scheme.power.min.y,
                    scheme.power.min.x + (start + width).min(range),
                    scheme.power.max.y,
                ));
                next.display = if start < range && show_power {
                    Display::Flex
                } else {
                    Display::None
                };
                node.set_if_neq(next);
            }
            Picture::PowerNeedle => {
                let size = data.image_size("PowerBarSlider");
                let consumption = if (consumption - 1.0).abs() < f32::EPSILON {
                    1.5
                } else {
                    consumption
                };
                let position = power_range(consumption, data) * scheme.power.width();
                let half_width = size.x * 0.5;
                let x = (position - half_width).clamp(0.0, scheme.power.width() - size.x);
                let mut next = at(Rect::from_corners(
                    Vec2::new(scheme.power.min.x + x, scheme.power.max.y - size.y),
                    Vec2::new(scheme.power.min.x + x + size.x, scheme.power.max.y),
                ));
                next.display = if production > 0.0 && show_power {
                    Display::Flex
                } else {
                    Display::None
                };
                node.set_if_neq(next);
            }
        }
    }
}

fn power(battle: &Match) -> (f32, f32) {
    battle.resource_owner().map_or((0.0, 0.0), |owner| {
        battle.simulation.players[owner].energy()
    })
}

fn power_range(value: f32, data: &UiData) -> f32 {
    // W3DPowerDraw uses logarithmic production and consumption markers.
    if value <= 0.0 {
        0.0
    } else {
        (value.log(data.power_base) / data.power_intervals).clamp(0.0, 1.0)
    }
}

pub fn appearance(
    resources: (Res<UiData>, Res<Menu>),
    battle: Option<Res<Match>>,
    maps: Option<Res<crate::map_selection::MapSelection>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut fonts: Query<(
        &FontSizeAt600,
        &mut TextFont,
        Option<&promotions::SerifFont>,
    )>,
    interactions: Query<(&Action, &Interaction, Option<&observer::RightPress>)>,
    mut parts: Query<(&MenuButtonPart, Option<&menu::DisabledPart>, &mut ImageNode)>,
) {
    let (data, menu) = resources;
    let battle = battle.map(Res::into_inner);
    let maps = maps.map(Res::into_inner);
    let data = data.into_inner();
    let menu = menu.into_inner();
    let window = window.into_inner();
    for (size, mut font, serif) in &mut fonts {
        let handle = if serif.is_some() {
            data.serif_handle.clone()
        } else {
            data.font_handle.clone()
        };
        set_font(&mut font, handle, size.0 * window.height() / 600.0);
    }
    let list_hovered = interactions.iter().any(|(action, interaction, _)| {
        *interaction != Interaction::None
            && match *action {
                Action::ChooseFaction { slot, .. } => menu.open == Some(Dropdown::Faction(slot)),
                Action::ChooseOpponent { slot, .. } => menu.open == Some(Dropdown::Player(slot)),
                Action::ChooseTeam { slot, .. } => menu.open == Some(Dropdown::Team(slot)),
                Action::ChooseColor { slot, .. } => menu.open == Some(Dropdown::Color(slot)),
                _ => false,
            }
    });
    for (part, disabled, mut image) in &mut parts {
        if interactions.get(part.parent).is_ok_and(|(action, _, _)| {
            maps.is_some_and(|maps| {
                (matches!(action, Action::Start)
                    && if battle.is_some() {
                        !maps.maps[maps.current].admission.is_verified()
                    } else {
                        maps.maps[maps.current].admission.rejection().is_some()
                    })
                    || (maps.open && matches!(action, Action::MainMenu))
            })
        }) {
            set_image(
                &mut image,
                data.image(&part.images[0].replacen("Buttons-", "Buttons-Disabled-", 1)),
            );
            continue;
        }
        if let Some(disabled) = disabled
            && !menu.enabled(disabled.dropdown)
        {
            set_image(&mut image, data.image(&disabled.image));
            continue;
        }
        let state = match interactions.get(part.parent) {
            Ok((
                action @ (Action::ChooseFaction { .. }
                | Action::ChooseOpponent { .. }
                | Action::ChooseTeam { .. }
                | Action::ChooseColor { .. }),
                _,
                _,
            )) => {
                if menu.selected(action) {
                    1 + usize::from(list_hovered)
                } else {
                    0
                }
            }
            Ok((_, _, Some(observer::RightPress(true))) | (_, Interaction::Pressed, _)) => 2,
            Ok((_, Interaction::Hovered, _)) => 1,
            _ => 0,
        };
        set_image(&mut image, data.image(&part.images[state]));
    }
}

fn set_font(font: &mut Mut<TextFont>, handle: Handle<Font>, size: f32) {
    let mut next = font.clone();
    next.font_size = FontSize::Px(size);
    next.font = FontSource::Handle(handle);
    font.set_if_neq(next);
}

fn set_image(image: &mut Mut<ImageNode>, next: ImageNode) {
    if image.image != next.image
        || image.color != next.color
        || image.rect != next.rect
        || image.texture_atlas != next.texture_atlas
        || image.flip_x != next.flip_x
        || image.flip_y != next.flip_y
        || image.image_mode != next.image_mode
        || image.visual_box != next.visual_box
    {
        **image = next;
    }
}

#[cfg(test)]
mod tests {
    use super::{ReleaseButton, released_button};
    use bevy::prelude::{ButtonInput, Interaction, MouseButton};

    #[test]
    fn menu_button_releases_once_and_dragging_away_cancels_confirmation() {
        let mut button = ReleaseButton { armed: false };
        let mut mouse = ButtonInput::default();
        mouse.press(MouseButton::Left);
        assert!(!released_button(
            Interaction::Pressed,
            &mut button,
            &mut mouse
        ));
        mouse.release(MouseButton::Left);
        assert!(released_button(
            Interaction::Hovered,
            &mut button,
            &mut mouse
        ));
        assert!(!released_button(
            Interaction::Hovered,
            &mut button,
            &mut mouse
        ));

        mouse.clear();
        mouse.press(MouseButton::Left);
        assert!(!released_button(
            Interaction::Pressed,
            &mut button,
            &mut mouse
        ));
        assert!(!released_button(Interaction::None, &mut button, &mut mouse));
        assert!(!released_button(
            Interaction::Pressed,
            &mut button,
            &mut mouse
        ));
        mouse.release(MouseButton::Left);
        assert!(!released_button(
            Interaction::Hovered,
            &mut button,
            &mut mouse
        ));
    }
}
