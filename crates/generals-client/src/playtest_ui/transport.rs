//! Original `ControlBar` transport inventory and evacuation commands.
pub mod chinook;
pub mod heal;
pub mod overlord;
pub mod verification;
use crate::{
    control_bar::{Command, CommandAction, UiData},
    input::Mouse,
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

fn carrier(battle: &Match) -> Option<usize> {
    let [id] = battle.selected.as_slice() else {
        return None;
    };
    let actor = battle.simulation.entities.get(*id)?;
    (actor.complete()
        && (actor.owner == 0
            || (actor.owner == battle.simulation.neutral_owner()
                && battle.simulation.is_garrison(*id)))
        && !actor.masked())
    .then_some(*id)
}

pub(super) fn passenger(battle: &Match, slot: usize) -> Option<usize> {
    let carrier = carrier(battle)?;
    let id = *battle.simulation.passengers(carrier).get(slot)?;
    let actor = battle.simulation.entities.get(id)?;
    (actor.alive()
        && (battle.simulation.is_tunnel(carrier)
            || actor.contained_by() == Some(battle.simulation.redirected_container(carrier))))
    .then_some(id)
}

pub(super) fn visible(battle: &Match, slot: usize) -> bool {
    let Some(id) = carrier(battle) else {
        return false;
    };
    let game = &battle.simulation;
    let Some(rules) = game.container_rules(id) else {
        return false;
    };
    // doTransportInventoryUI subtracts extra occupied slots: each passenger
    // gets one picture even when its TransportSlotCount consumes more room.
    let extra = game.passengers(id).iter().fold(0_u32, |sum, &rider| {
        if rules.garrison.is_some() || rules.tunnel.is_some() {
            return sum;
        }
        sum.saturating_add(
            game.catalog[game.entities[rider].type_index]
                .transport_slots
                .saturating_sub(1),
        )
    });
    slot < usize::try_from(rules.capacity.saturating_sub(extra))
        .expect("transport capacity fits usize")
}

pub(super) fn image<'a>(battle: &Match, data: &'a UiData, command: &'a Command) -> &'a str {
    if let CommandAction::ExitContainer(slot) = command.action {
        if let Some(id) = passenger(battle, slot) {
            return &data.units[battle.simulation.entities[id].type_index].button_image;
        }
        // ControlBarScheme::init changes the four exit commands to the local
        // player's marker, independently of the selected carrier's faction.
        if let Some(marker) =
            &data.schemes[super::control_bar_index(&battle.simulation)].command_marker
        {
            return marker;
        }
    }
    &command.image
}

pub(super) fn available(battle: &Match, action: CommandAction) -> Option<bool> {
    match action {
        CommandAction::ExitContainer(slot) => Some(passenger(battle, slot).is_some()),
        CommandAction::CombatDrop => Some(
            battle
                .selected
                .iter()
                .any(|&id| battle.simulation.combat_drop_ready(id)),
        ),
        CommandAction::Evacuate => Some(battle.selected.iter().any(|&id| {
            battle.simulation.entities.get(id).is_some_and(|actor| {
                actor.complete()
                    && actor.owner == 0
                    && !actor.masked()
                    && !battle.simulation.passengers(id).is_empty()
            })
        })),
        _ => None,
    }
}

pub(super) fn dispatch(battle: &mut Match, mouse: &mut Mouse, action: CommandAction) {
    match action {
        CommandAction::ExitContainer(slot) => {
            // The pinned source fix treats empty/stale inventory bindings as
            // no action, never an uninitialized object ID or array index.
            if let Some(id) = passenger(battle, slot)
                && let Some(host) = carrier(battle)
                && battle.simulation.entities[id].owner == 0
            {
                drop(battle.command_message_members());
                if let Err(error) = battle.simulation.exit_container_at(id, host) {
                    eprintln!("Passenger exit rejected: {error}");
                }
            }
        }
        CommandAction::Evacuate => {
            let response =
                VoiceRequest::new(&battle.simulation, &battle.selected, Voice::Unload, false);
            let mut issued = false;
            for id in battle.command_message_members() {
                if battle
                    .simulation
                    .entities
                    .get(id)
                    .is_none_or(|actor| actor.owner != 0 || !actor.complete())
                {
                    continue;
                }
                if !battle.simulation.passengers(id).is_empty() {
                    match battle.simulation.group_evacuate(id) {
                        Ok(()) => issued = true,
                        Err(error) => eprintln!("Evacuation rejected: {error}"),
                    }
                }
            }
            if issued {
                mouse.voices.push(response);
            }
        }
        _ => {}
    }
}
