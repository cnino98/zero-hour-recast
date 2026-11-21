//! Bomb Truck object power, weapon and object-upgrade commands.
use super::{CommandMode, Mouse, locally_controlled};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};

#[cfg(test)]
mod tests;

pub fn can_target(battle: &mut Match, target: usize) -> bool {
    let game = &mut battle.simulation;
    battle.selected.iter().any(|&id| {
        game.entities.get(id).is_some_and(|actor| actor.owner == 0)
            && game.bomb_truck_disguise_target(id, target)
    })
}

pub(super) fn issue_disguise(battle: &mut Match, mouse: &mut Mouse, target: usize) {
    if !can_target(battle, target) {
        return;
    }
    let response = VoiceRequest::special_power(
        &battle.simulation,
        battle.command_drawables(),
        &["SPECIAL_DISGUISE_AS_VEHICLE"],
    );
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle
                .simulation
                .bomb_truck_order_disguise(id, target)
                .is_ok();
        }
    }
    if issued {
        mouse.voices.extend(response);
        mouse.pending_command = CommandMode::Normal;
    }
}

pub fn detonate(battle: &mut Match) {
    for id in battle.command_message_members() {
        let _ = battle.simulation.detonate_bomb_truck(0, id);
    }
}

fn payload(battle: &Match, producer: usize, index: usize) -> Option<(&'static str, Voice)> {
    let game = &battle.simulation;
    let upgrades = &game.catalog[game.entities[producer].type_index]
        .maintenance
        .research;
    match upgrades.get(index)?.name.as_str() {
        "Upgrade_GLABombTruckBioBomb" => {
            Some(("Upgrade_GLABombTruckBioBomb", Voice::BombTruckBioBombMode))
        }
        "Upgrade_GLABombTruckHighExplosiveBomb" => Some((
            "Upgrade_GLABombTruckHighExplosiveBomb",
            Voice::BombTruckHighExplosiveMode,
        )),
        _ => None,
    }
}

fn research_index(battle: &Match, id: usize, upgrade: &str) -> Option<usize> {
    let game = &battle.simulation;
    game.catalog[game.entities[id].type_index]
        .maintenance
        .research
        .iter()
        .position(|research| research.name == upgrade)
}

pub fn payload_available(battle: &Match, producer: usize, index: usize) -> Option<bool> {
    let (upgrade, _) = payload(battle, producer, index)?;
    Some(battle.selected.iter().any(|&id| {
        research_index(battle, id, upgrade)
            .is_some_and(|index| battle.simulation.can_research(id, index))
    }))
}

pub fn research_payload(
    battle: &mut Match,
    mouse: &mut Mouse,
    producer: usize,
    index: usize,
) -> bool {
    let Some((upgrade, voice)) = payload(battle, producer, index) else {
        return false;
    };
    mouse.voices.push(VoiceRequest::new(
        &battle.simulation,
        battle.command_drawables(),
        voice,
        false,
    ));
    for id in battle.command_message_members() {
        if let Some(index) = research_index(battle, id, upgrade) {
            let _ = battle.simulation.queue_upgrade(id, index);
        }
    }
    true
}
