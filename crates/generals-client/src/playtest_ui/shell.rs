//! Original shell and skirmish setup operate on menu/catalog data before a Match exists.
mod loading;
pub mod verification;
use super::{
    Action, Dropdown, Field, Layer, Menu, UiData,
    menu::{self, Confirmation, Page},
    options,
};
use crate::{map_selection::MapSelection, playtest::startup::Shell};
use bevy::prelude::*;

pub fn configure(app: &mut App) {
    app.add_systems(
        Update,
        (
            (options::pointer, buttons, options::update)
                .chain()
                .run_if(crate::playtest::startup::interactive),
            (
                super::name_input,
                crate::map_selection::pointer_input,
                super::setup_controls,
                super::update_options,
            )
                .chain()
                .run_if(resource_exists::<MapSelection>)
                .run_if(crate::playtest::startup::interactive),
            (
                super::option_labels,
                super::option_checkbox,
                super::option_boxes,
                super::option_thumb,
                super::name_presentation,
            )
                .chain()
                .run_if(resource_exists::<MapSelection>),
            loading::update,
            update,
            super::appearance,
            crate::map_selection::update,
        )
            .chain()
            .run_if(crate::playtest::startup::shell_active),
    );
}

pub fn setup(
    mut commands: Commands,
    mut data: ResMut<UiData>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
    maps: Option<Res<MapSelection>>,
) {
    super::register_assets(&mut commands, &mut data, &mut images, &mut fonts);
    menu::initialize_shell(&mut commands, &data);
    if let Some(maps) = maps {
        menu::setup_skirmish(&mut commands, &data, maps.maps[maps.current].capacity);
    }
    loading::setup(&mut commands, &mut data, &mut fonts);
}

pub fn enter_setup(world: &mut World) {
    let mut menu = world.resource_mut::<Menu>();
    menu.page = Page::Skirmish;
    menu.clear_confirmation();
}

pub fn map_capacity(world: &mut World, capacity: usize, initial: bool) {
    let mut menu = world.resource_mut::<Menu>();
    if initial {
        menu.capacity = capacity;
        for start in &mut menu.starts {
            *start = start.filter(|start| *start < capacity);
        }
    } else {
        menu.apply_map(capacity);
    }
}

fn buttons(
    mut query: Query<super::ButtonState<'_>, Changed<Interaction>>,
    mut menu: ResMut<Menu>,
    mut shell: ResMut<Shell>,
    controls: (
        ResMut<crate::source_audio::SourceAudio>,
        ResMut<ButtonInput<MouseButton>>,
        ResMut<options::Editor>,
    ),
    mut maps: Option<ResMut<MapSelection>>,
    data: Res<UiData>,
    capture: Option<Res<crate::playtest_view::Capture>>,
) {
    let (mut audio, mut pointer, mut options) = controls;
    let data = data.into_inner();
    let capture = capture.map(Res::into_inner);
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
        if maps.as_deref().is_some_and(|maps| {
            (maps.open && matches!(action, Action::MainMenu))
                || (matches!(action, Action::Start)
                    && maps.maps[maps.current].admission.rejection().is_some())
        }) {
            continue;
        }
        if pointer.just_pressed(MouseButton::Left)
            && *interaction == Interaction::Pressed
            && !matches!(action, Action::ToggleDropdown(_))
        {
            audio.ui("GUIClick");
        }
        if let Some(mut image) = image {
            image.color = if *interaction == Interaction::Hovered {
                Color::srgb(1.15, 1.15, 1.15)
            } else {
                Color::WHITE
            };
        }
        let activated = release.map_or_else(
            || *interaction == Interaction::Pressed,
            |mut release| super::released_button(*interaction, &mut release, &mut pointer),
        );
        if !activated {
            continue;
        }
        if matches!(action, Action::ToggleDropdown(_)) {
            audio.ui("GUIClick");
        }
        pointer.clear_just_pressed(MouseButton::Left);
        pointer.clear_just_released(MouseButton::Left);
        if matches!(action, Action::Start | Action::MainMenu)
            && matches!(menu.page, Page::Skirmish)
            && capture.is_none_or(|capture| !capture.is_pending())
        {
            menu.save_seed(data, shell.seed());
        }
        if options::activate(action, &mut options)
            || maps
                .as_deref_mut()
                .is_some_and(|maps| maps.activate(action))
            || menu::activate_setup(action, &mut menu)
        {
            continue;
        }
        match action {
            Action::Skirmish => shell.open_setup(),
            Action::Start => shell.start(),
            Action::CancelConfirmation => menu.clear_confirmation(),
            _ => {}
        }
    }
}

fn update(
    menu: Res<Menu>,
    data: Res<UiData>,
    shell: Res<Shell>,
    maps: Option<Res<MapSelection>>,
    mut layers: Query<(&Layer, &mut Visibility), Without<loading::Loading>>,
    mut fields: Query<(&Field, &mut Text, &mut TextColor), Without<loading::Loading>>,
) {
    let menu = menu.into_inner();
    let data = data.into_inner();
    let shell = shell.into_inner();
    let maps = maps.map(Res::into_inner);
    let skirmish = matches!(menu.page, Page::Skirmish);
    let chooser = maps.is_some_and(|maps| maps.open);
    for (layer, mut visibility) in &mut layers {
        let visible = !shell.loading()
            && match layer {
                Layer::Shell => true,
                Layer::Menu => !skirmish,
                Layer::SinglePlayer => matches!(menu.page, Page::SinglePlayer),
                Layer::Options => menu.options_open,
                Layer::SkirmishUnderlying => skirmish,
                Layer::SkirmishPlayer(slot) => skirmish && *slot < menu.capacity,
                Layer::Skirmish => skirmish && !chooser,
                Layer::SkirmishSlot(slot) => skirmish && *slot < menu.capacity && !chooser,
                Layer::MapSelect => skirmish && chooser,
                Layer::Dropdown(dropdown) => {
                    skirmish
                        && menu.open == Some(*dropdown)
                        && (!chooser
                            || matches!(dropdown, Dropdown::Player(_) | Dropdown::StartingCash))
                }
                Layer::Confirmation(kind) => menu.confirmation == Some(*kind),
                _ => false,
            };
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (field, mut text, mut color) in &mut fields {
        let value = match field {
            Field::Faction(slot) => {
                color.set_if_neq(TextColor(if menu.enabled(Dropdown::Faction(*slot)) {
                    Color::srgb_u8(255, 254, 254)
                } else {
                    Color::srgb_u8(122, 48, 1)
                }));
                menu.factions[*slot].map_or_else(
                    || data.label("GUI:Random").to_owned(),
                    |faction| data.factions[super::faction_index(faction)].clone(),
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
            Field::Money | Field::Tooltip => continue,
        };
        text.0 = value;
    }
}
