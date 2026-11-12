//! Original `ControlBarObserver` selection and player statistics.
use super::{Action, FontSizeAt600, Layer, ReleaseButton, UiData, at, faction_index, menu};
use super::{PromotionIcon, promotion_icon};
use crate::playtest::Match;
use bevy::{prelude::*, ui_render::ui_material::MaterialNode};

#[derive(Component)]
pub struct Label(pub String);

#[derive(Component)]
pub struct Flag;

#[derive(Component)]
pub struct RightPress(pub bool);

#[derive(Component)]
pub struct Overlay(Entity);

pub fn setup(commands: &mut Commands, data: &UiData, icons: &mut Assets<PromotionIcon>) {
    for (name, source) in &data.observer.windows {
        let list_slot = name
            .strip_prefix("StaticTextPlayer")
            .and_then(|index| index.parse::<usize>().ok());
        if let Some(index) = name
            .strip_prefix("ButtonPlayer")
            .and_then(|index| index.parse::<usize>().ok())
        {
            let parent = commands
                .spawn((
                    Button,
                    Action::Observe(Some(index)),
                    RightPress(false),
                    ReleaseButton { armed: false },
                    Layer::ObserverList(index),
                    MaterialNode(icons.add(data.icon(&data.observer.buttons[0]))),
                    at(source.area),
                    ZIndex(7),
                ))
                .id();
            commands.spawn((
                Overlay(parent),
                data.image("Cameo_hilited"),
                Node {
                    width: percent(100.0),
                    height: percent(100.0),
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
                ChildOf(parent),
                ZIndex(1),
            ));
        } else if name == "ButtonCancel" {
            let entity = menu::button_at_size(
                commands,
                data,
                Action::Observe(None),
                Layer::ObserverInfo,
                source.area,
                &source.text,
                source.font_size,
            );
            commands.entity(entity).insert(RightPress(false));
        } else if name.starts_with("StaticText") {
            let layer = list_slot.map_or(Layer::ObserverInfo, Layer::ObserverList);
            label(commands, name, source, layer);
        } else if name == "WinFlag" {
            commands.spawn((
                Flag,
                Layer::ObserverInfo,
                MaterialNode(icons.add(data.icon(&data.observer.flags[0]))),
                at(source.area),
                ZIndex(6),
            ));
        } else if let Some(image) = &source.image {
            commands.spawn((
                Layer::ObserverInfo,
                data.image(image),
                at(source.area),
                ZIndex(6),
            ));
        }
    }
}

fn label(
    commands: &mut Commands,
    name: &str,
    source: &crate::control_bar::results::Window,
    layer: Layer,
) {
    let parent = commands
        .spawn((
            layer,
            Node {
                align_items: AlignItems::Center,
                justify_content: if source.centered {
                    JustifyContent::Center
                } else {
                    JustifyContent::FlexStart
                },
                border: UiRect::all(px(1.0)),
                padding: UiRect {
                    left: px(if source.centered { 4.0 } else { 6.0 }),
                    right: px(if source.centered { 4.0 } else { 2.0 }),
                    ..default()
                },
                overflow: Overflow::clip(),
                ..at(source.area)
            },
            BackgroundColor(source.background),
            BorderColor::all(source.border),
            ZIndex(8),
        ))
        .id();
    commands.spawn((
        Label(name.to_owned()),
        Text::new(&source.text),
        TextFont {
            font_size: FontSize::Px(source.font_size),
            ..default()
        },
        FontSizeAt600(source.font_size),
        TextColor(source.text_color),
        TextShadow {
            offset: Vec2::ONE,
            color: source.text_border,
        },
        TextLayout::justify(if source.wrap_centered {
            Justify::Center
        } else {
            Justify::Left
        }),
        Node {
            max_width: percent(100.0),
            ..default()
        },
        ChildOf(parent),
    ));
}

pub fn overlays(
    data: Res<UiData>,
    buttons: Query<(&Interaction, &RightPress)>,
    mut overlays: Query<(&Overlay, &mut ImageNode, &mut Visibility)>,
) {
    let data = data.into_inner();
    for (overlay, mut image, mut visible) in &mut overlays {
        let Ok((interaction, right)) = buttons.get(overlay.0) else {
            continue;
        };
        visible.set_if_neq(if *interaction == Interaction::None {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        super::set_image(
            &mut image,
            data.image(if *interaction == Interaction::Pressed || right.0 {
                "Cameo_push"
            } else {
                "Cameo_hilited"
            }),
        );
    }
}

pub fn populate(battle: &mut Match) {
    battle.observer_players = (0..battle.simulation.slots.len())
        .filter(|&owner| battle.simulation.player_active(owner))
        .collect();
}

pub fn activate(action: &Action, battle: &mut Match) -> bool {
    let Action::Observe(index) = *action else {
        return false;
    };
    if battle.simulation.player_active(0)
        || !battle.started
        || battle.simulation.end_game_at.is_some()
    {
        return true;
    }
    let player = index.and_then(|index| battle.observer_players.get(index).copied());
    if index.is_some() && player.is_none() {
        return true;
    }
    battle.observer_player = player;
    // changeObservedPlayer always sets observerLookAtPlayer. Its render-view
    // branch requires trackAllPlayers, including W3DGhostObject snapshots.
    // Those per-player building snapshots are not yet represented here.
    if player.is_none() {
        populate(battle);
    }
    true
}

pub fn right_buttons(
    mut buttons: Query<(&Action, &Interaction, &mut RightPress)>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut battle: ResMut<Match>,
) {
    for (action, interaction, mut armed) in &mut buttons {
        if *interaction == Interaction::None {
            armed.0 = false;
            continue;
        }
        if mouse.just_pressed(MouseButton::Right) {
            armed.0 = true;
        }
        if mouse.just_released(MouseButton::Right) && std::mem::take(&mut armed.0) {
            activate(action, &mut battle);
            mouse.clear_just_pressed(MouseButton::Right);
            mouse.clear_just_released(MouseButton::Right);
        }
    }
}

pub fn update(
    mut battle: ResMut<Match>,
    data: Res<UiData>,
    mut labels: Query<(&Label, &mut Text, &mut TextColor)>,
    pictures: Query<(&Action, &MaterialNode<PromotionIcon>), Without<Flag>>,
    flag: Query<&MaterialNode<PromotionIcon>, With<Flag>>,
    mut icons: ResMut<Assets<PromotionIcon>>,
    mut last: Local<Option<(u32, Option<usize>, u64)>>,
) {
    let data = data.into_inner();
    if !battle.started || battle.simulation.player_active(0) {
        return;
    }
    if battle.observer_players.is_empty() {
        populate(&mut battle);
    }
    let key = (
        battle.generation,
        battle.observer_player,
        battle.simulation.tick / 15,
    );
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    let game = &battle.simulation;
    let counts = battle.observer_player.map(|owner| counts(game, owner));
    for (label, mut text, mut color) in &mut labels {
        if let Some(index) = label
            .0
            .strip_prefix("StaticTextPlayer")
            .and_then(|index| index.parse::<usize>().ok())
        {
            if let Some(&owner) = battle.observer_players.get(index) {
                let team = game.slots[owner].team.map_or_else(
                    || {
                        if game.slots[owner].difficulty.is_some() {
                            "Team:AI".into()
                        } else {
                            "Team:0".into()
                        }
                    },
                    |team| format!("Team:{}", team + 1),
                );
                let name = super::results::player_name(&battle, data, owner);
                let value = data
                    .label("CONTROLBAR:ObsPlayerLabel")
                    .replacen("%ls", name, 1)
                    .replacen("%ls", data.label(&team), 1);
                text.reborrow()
                    .map_unchanged(|text| &mut text.0)
                    .set_if_neq(value);
                color.set_if_neq(TextColor(player_color(&battle, data, owner)));
            }
            continue;
        }
        let Some(owner) = battle.observer_player else {
            continue;
        };
        let counts = counts.expect("observed player counts");
        let value = match label.0.as_str() {
            "StaticTextPlayerName" => {
                color.set_if_neq(TextColor(player_color(&battle, data, owner)));
                super::results::player_name(&battle, data, owner).to_owned()
            }
            "StaticTextNumberOfUnits" => counts[0].to_string(),
            "StaticTextNumberOfBuildings" => counts[1].to_string(),
            "StaticTextNumberOfUnitsKilled" => {
                game.players[owner].statistics.units_destroyed.to_string()
            }
            "StaticTextNumberOfUnitsLost" => game.players[owner].statistics.units_lost.to_string(),
            _ => continue,
        };
        text.reborrow()
            .map_unchanged(|text| &mut text.0)
            .set_if_neq(value);
    }
    for (action, material) in &pictures {
        if let Action::Observe(Some(index)) = action
            && let Some(&owner) = battle.observer_players.get(*index)
        {
            promotion_icon::update(
                &mut icons,
                &material.0,
                data.icon(&data.observer.buttons[faction_index(game.players[owner].faction)]),
            );
        }
    }
    if let Some(owner) = battle.observer_player {
        for material in &flag {
            promotion_icon::update(
                &mut icons,
                &material.0,
                data.icon(&data.observer.flags[faction_index(game.players[owner].faction)]),
            );
        }
    }
}

fn player_color(battle: &Match, data: &UiData, owner: usize) -> Color {
    battle.simulation.player_colors[owner].map_or(Color::WHITE, |index| {
        Color::srgb_from_array(data.results.colors[index])
    })
}

fn counts(game: &generals_sim::skirmish::Skirmish, owner: usize) -> [usize; 2] {
    let mut counts = [0, 0];
    for (id, actor) in game.entities.iter().enumerate() {
        if actor.owner != owner || !actor.alive() && !game.retained_rubble(id) {
            continue;
        }
        let value = kind_counts(&game.catalog[actor.type_index].kind_of);
        counts[0] += value[0];
        counts[1] += value[1];
    }
    counts
}

fn kind_counts(flags: &[String]) -> [usize; 2] {
    let has = |name| flags.iter().any(|flag| flag == name);
    if has("STRUCTURE") {
        // Native code sums three independent queries, including overlapping flags.
        [
            0,
            ["SCORE", "SCORE_CREATE", "SCORE_DESTROY"]
                .into_iter()
                .filter(|name| has(name))
                .count(),
        ]
    } else {
        [usize::from(has("SCORE")), 0]
    }
}

#[cfg(test)]
mod tests {
    use super::kind_counts;

    #[test]
    fn observer_counts_native_score_masks_without_deduplicating_structure_queries() {
        for (flags, expected) in [
            (vec!["SCORE"], [1, 0]),
            (vec!["SCORE_CREATE"], [0, 0]),
            (vec!["STRUCTURE", "SCORE"], [0, 1]),
            (vec!["STRUCTURE", "SCORE_CREATE", "SCORE_DESTROY"], [0, 2]),
            (
                vec!["STRUCTURE", "SCORE", "SCORE_CREATE", "SCORE_DESTROY"],
                [0, 3],
            ),
        ] {
            assert_eq!(
                kind_counts(&flags.into_iter().map(str::to_owned).collect::<Vec<_>>()),
                expected
            );
        }
    }
}
