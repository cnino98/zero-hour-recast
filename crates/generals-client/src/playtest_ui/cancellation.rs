//! Original under-construction and unit-queue controls, retaining displayed IDs.
use super::{Action, Layer, Match, PromotionIcon, UiData, at, text};
use bevy::{prelude::*, ui::FocusPolicy, ui_render::ui_material::MaterialNode};
use generals_sim::skirmish::{Production, ProductionItem};

#[derive(Component)]
pub struct QueueSlot(usize);

#[derive(Component)]
pub struct ConstructionText;

#[derive(Component)]
pub struct Overlay(Entity);

type CancelButtons<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Action,
        &'static mut Node,
        &'static MaterialNode<PromotionIcon>,
        Option<&'static QueueSlot>,
        &'static Interaction,
    ),
>;
type Description<'w, 's> = Query<
    'w,
    's,
    (&'static mut Text, &'static mut Node),
    (With<ConstructionText>, Without<Action>),
>;
type Overlays<'w, 's> = Query<
    'w,
    's,
    (&'static Overlay, &'static mut ImageNode, &'static mut Node),
    (Without<Action>, Without<ConstructionText>),
>;

pub(super) fn setup(commands: &mut Commands, data: &UiData, icons: &mut Assets<PromotionIcon>) {
    let area = data.windows["ControlBar.wnd:ButtonCancelConstruction"];
    let button = commands
        .spawn((
            Button,
            Action::CancelConstruction,
            Layer::Game,
            MaterialNode(icons.add(data.icon(&data.cancel_construction.image))),
            at(area),
            ZIndex(6),
        ))
        .id();
    overlay(commands, data, button, area);
    construction_description(
        commands,
        data.windows["ControlBar.wnd:UnderConstructionDesc"],
    );
    for index in 0..9 {
        let area = data.windows[&format!("ControlBar.wnd:ButtonQueue{:02}", index + 1)];
        let button = commands
            .spawn((
                Button,
                QueueSlot(index),
                Action::CancelProduction(None),
                Layer::Game,
                MaterialNode(icons.add(data.icon(&data.units[0].button_image))),
                at(area),
                ZIndex(6),
            ))
            .id();
        overlay(commands, data, button, area);
    }
}

fn construction_description(commands: &mut Commands, area: Rect) {
    commands
        .spawn((
            ConstructionText,
            Layer::Game,
            text("", 14.0),
            at(area),
            ZIndex(6),
        ))
        .insert(TextColor(Color::srgb_u8(254, 254, 254)));
}

fn overlay(commands: &mut Commands, data: &UiData, button: Entity, area: Rect) {
    commands.spawn((
        Overlay(button),
        Layer::Game,
        data.image("Cameo_hilited"),
        at(area),
        FocusPolicy::Pass,
        ZIndex(7),
    ));
}

pub(super) fn foundation(battle: &Match) -> Option<usize> {
    if !battle.simulation.player_active(0) {
        return None;
    }
    let [id] = battle.selected.as_slice() else {
        return None;
    };
    let unit = battle.simulation.entities.get(*id)?;
    (unit.alive()
        && unit.owner == 0
        && unit.build_progress < 1.0
        && battle.simulation.catalog[unit.type_index]
            .kind
            .is_building())
    .then_some(*id)
}

pub(super) fn queued_entry(battle: &Match, producer: usize, id: i32) -> Option<&Production> {
    battle
        .simulation
        .entities
        .get(producer)
        .filter(|unit| unit.alive() && unit.owner == 0)?
        .queue
        .iter()
        .find(|entry| entry.id == id)
}

pub(super) fn activate(action: &Action, battle: &mut Match) -> bool {
    if !matches!(
        action,
        Action::CancelConstruction | Action::CancelProduction(_) | Action::CancelUpgrade(_)
    ) {
        return false;
    }
    if !battle.started || battle.paused || battle.simulation.end_game_at.is_some() {
        return true;
    }
    let result = match *action {
        Action::CancelConstruction => foundation(battle).map(|building| {
            drop(battle.command_message_members());
            battle.simulation.cancel_construction(0, building)
        }),
        Action::CancelUpgrade(Some((producer, index))) => {
            drop(battle.command_message_members());
            Some(battle.simulation.cancel_upgrade(0, producer, index))
        }
        Action::CancelProduction(Some((producer, id))) => {
            drop(battle.command_message_members());
            Some(battle.simulation.cancel_production(0, producer, id))
        }
        _ => None,
    };
    if let Some(Err(error)) = result {
        eprintln!("Cancellation rejected: {error}");
    }
    true
}

pub fn update(
    battle: Res<Match>,
    data: Res<UiData>,
    mut icons: ResMut<Assets<PromotionIcon>>,
    mut buttons: CancelButtons,
    mut description: Description,
    mut overlays: Overlays,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let foundation = foundation(battle);
    let producer = if let [id] = battle.selected.as_slice() {
        battle
            .simulation
            .entities
            .get(*id)
            .filter(|unit| unit.complete() && unit.owner == 0 && battle.simulation.player_active(0))
            .map(|unit| (*id, unit))
    } else {
        None
    };
    for (mut action, mut node, material, slot, _) in &mut buttons {
        let image = match *action {
            Action::CancelConstruction if foundation.is_some() => {
                Some(&data.cancel_construction.image)
            }
            Action::CancelConstruction => None,
            Action::CancelProduction(_) | Action::CancelUpgrade(_) => {
                let entry = producer
                    .and_then(|(id, unit)| unit.queue.get(slot?.0).map(|entry| (id, entry)));
                *action = match entry {
                    Some((id, entry)) => match entry.item {
                        ProductionItem::Unit(_) => Action::CancelProduction(Some((id, entry.id))),
                        ProductionItem::Upgrade(index) => Action::CancelUpgrade(Some((id, index))),
                    },
                    None => Action::CancelProduction(None),
                };
                entry.and_then(|(producer, entry)| match entry.item {
                    ProductionItem::Unit(index) => Some(&data.units[index].button_image),
                    ProductionItem::Upgrade(index) => data.units[battle.simulation.entities[producer].type_index].commands.iter()
                        .find(|command| matches!(command.action, crate::control_bar::CommandAction::Research(value) if value == index))
                        .map(|command| &command.image),
                })
            }
            _ => continue,
        };
        node.display = if image.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(image) = image
            && let Some(mut icon) = icons.get_mut(&material.0)
        {
            *icon = data.icon(image);
        }
    }
    for (mut text, mut node) in &mut description {
        node.display = if foundation.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(id) = foundation {
            text.0 = data
                .label("CONTROLBAR:UnderConstructionDesc")
                .replace(
                    "%.0f",
                    &format!("{:.0}", battle.simulation.entities[id].construction_percent),
                )
                .replace("%%", "%");
        }
    }
    for (overlay, mut image, mut node) in &mut overlays {
        let visible = buttons
            .get(overlay.0)
            .ok()
            .and_then(|(_, node, _, _, interaction)| {
                (node.display != Display::None && *interaction != Interaction::None)
                    .then_some(*interaction)
            });
        node.display = if visible.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        *image = data.image(if visible == Some(Interaction::Pressed) {
            "Cameo_push"
        } else {
            "Cameo_hilited"
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_description_spawns_without_duplicate_text_components() {
        let mut app = App::new();
        app.add_systems(Startup, |mut commands: Commands| {
            construction_description(&mut commands, Rect::new(323.0, 519.0, 599.0, 567.0));
        });
        app.update();
        let mut query = app
            .world_mut()
            .query_filtered::<(&TextColor, &TextFont), With<ConstructionText>>();
        let (color, font) = query
            .single(app.world())
            .expect("source construction description");
        assert_eq!(color.0, Color::srgb_u8(254, 254, 254));
        let bevy::text::FontSize::Px(size) = font.font_size else {
            panic!("source construction font must use pixels")
        };
        assert!((size - 14.0).abs() < f32::EPSILON);
    }
}
