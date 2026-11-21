//! Stock right-side faction shortcuts, packed in original `CommandSet` order.
use super::{
    Action, Command, Layer, Match, Mouse, PromotionIcon, UiData, at, command_icons, dispatch_power,
    faction_index,
};
use crate::control_bar::RequiredUpgrade;
use bevy::{prelude::*, ui::FocusPolicy, ui_render::ui_material::MaterialNode};

#[derive(Component)]
pub struct Tray {
    faction: usize,
    slot: usize,
}

pub(super) fn setup(commands: &mut Commands, data: &UiData, icons: &mut Assets<PromotionIcon>) {
    let Some(trays) = &data.power_shortcuts else {
        return;
    };
    for (faction, layout) in trays.factions.iter().enumerate() {
        for (slot, details) in layout.slots.iter().enumerate() {
            commands.spawn((
                Tray { faction, slot },
                Layer::Game,
                data.image(&details.image),
                at(details.parent),
                FocusPolicy::Pass,
                ZIndex(7),
            ));
            commands.spawn((
                Button,
                Action::PowerShortcut { faction, slot },
                Layer::Game,
                MaterialNode(icons.add(data.icon(&details.image))),
                at(details.button),
                ZIndex(8),
            ));
        }
    }
}

pub(super) fn command<'a>(
    battle: &Match,
    data: &'a UiData,
    faction: usize,
    slot: usize,
) -> Option<&'a Command> {
    if !battle.started
        || !battle.simulation.player_active(0)
        || faction != faction_index(battle.simulation.players[0].faction)
    {
        return None;
    }
    data.power_shortcuts
        .as_ref()?
        .factions
        .get(faction)?
        .commands
        .iter()
        .filter(|button| {
            battle
                .simulation
                .entities
                .iter()
                .enumerate()
                .any(|(id, _)| eligible(battle, id, button))
        })
        .nth(slot)
}

fn eligible(battle: &Match, id: usize, command: &Command) -> bool {
    let actor = &battle.simulation.entities[id];
    if actor.owner != 0 || !actor.complete() || actor.masked() {
        return false;
    }
    let Some(power) = command.action.power() else {
        return false;
    };
    let unit = &battle.simulation.catalog[actor.type_index];
    let provider = unit
        .special_powers
        .iter()
        .any(|rules| rules.name == power && !rules.script_only)
        || unit.progression.powers.iter().any(|name| name == power);
    provider
        && super::power_science_visible(battle, id, command)
        && match &command.required_upgrade {
            Some(RequiredUpgrade::Player(upgrade)) => battle.simulation.has_upgrade(0, upgrade),
            Some(RequiredUpgrade::Object(upgrade)) => {
                battle.simulation.object_has_upgrade(id, upgrade)
            }
            None => true,
        }
}

fn most_ready(battle: &mut Match, command: &Command) -> Option<usize> {
    let power = command.action.power()?;
    let mut best = None;
    let mut best_time = u32::MAX;
    for id in (0..battle.simulation.entities.len()).rev() {
        if !eligible(battle, id, command) {
            continue;
        }
        let remaining = if battle.simulation.object_disabled(id) {
            u32::MAX - 10
        } else {
            battle.simulation.power_cooldown_remaining(id, power)
        };
        if remaining < best_time {
            best = Some(id);
            best_time = remaining;
        }
    }
    best
}

pub(super) fn pointer_over(battle: &Match, data: &UiData, point: Vec2, window: &Window) -> bool {
    let Some(trays) = &data.power_shortcuts else {
        return false;
    };
    let faction = faction_index(battle.simulation.players[0].faction);
    let point = point / Vec2::new(window.width() / 800.0, window.height() / 600.0);
    trays.factions[faction]
        .slots
        .iter()
        .enumerate()
        .any(|(slot, position)| {
            position.button.contains(point) && command(battle, data, faction, slot).is_some()
        })
}

pub(super) fn available(battle: &mut Match, data: &UiData, faction: usize, slot: usize) -> bool {
    !battle.paused
        && command(battle, data, faction, slot).is_some_and(|command| {
            most_ready(battle, command).is_some_and(|caster| {
                battle
                    .simulation
                    .can_use_power(caster, command.action.power().expect("shortcut power"))
            })
        })
}

pub(super) fn activate(
    battle: &mut Match,
    mouse: &mut Mouse,
    data: &UiData,
    faction: usize,
    slot: usize,
) {
    if let Some(command) = command(battle, data, faction, slot)
        && let Some(caster) = most_ready(battle, command)
        && let Some(power) = command.action.power_command()
        && battle.simulation.can_use_power(caster, power.name)
        && !battle.paused
    {
        dispatch_power(battle, mouse, caster, power, true);
    }
}

pub(super) fn update_button(
    battle: &mut Match,
    data: &UiData,
    faction: usize,
    slot: usize,
    icons: &mut Assets<PromotionIcon>,
    handle: &MaterialNode<PromotionIcon>,
    node: &mut Node,
) {
    let command = command(battle, data, faction, slot);
    node.display = if command.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    let Some(command) = command else {
        return;
    };
    let Some(caster) = most_ready(battle, command) else {
        return;
    };
    let power = command.action.power().expect("original shortcut power");
    let mut icon = data.icon(command_icons::command_image(battle, data, command));
    if !battle.simulation.can_use_power(caster, power) {
        let progress = battle.simulation.power_progress(caster, power);
        if progress >= 1.0 {
            icon.state.x = 1.0;
        } else {
            icon.state.z = (progress * 100.0).trunc();
            icon.clock_color = data.schemes[faction].clock_color;
        }
    }
    super::promotion_icon::update(icons, &handle.0, icon);
}

pub(super) fn update_tray(battle: &Match, data: &UiData, tray: &Tray, node: &mut Node) {
    node.display = if command(battle, data, tray.faction, tray.slot).is_some() {
        Display::Flex
    } else {
        Display::None
    };
}
