//! Original `CommandXlat` capture context precedence and first-selected command lookup.
use super::{CommandMode, Mouse, locally_controlled};
use crate::playtest::Match;

pub fn cursor(battle: &Match) -> usize {
    let lotus = battle.selected.first().is_some_and(|&id| {
        battle.simulation.catalog[battle.simulation.entities[id].type_index]
            .behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power|
                power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding))
    });
    if lotus { 6 } else { 7 }
}

pub fn can_target(battle: &mut Match, target: usize) -> bool {
    // ActionManager permits Partial; can_capture applies that human fog gate.
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if locally_controlled(battle, id) && battle.simulation.can_capture(id, target) {
            return true;
        }
    }
    false
}

pub fn context_target(battle: &mut Match, mouse: &Mouse, target: Option<usize>) -> Option<usize> {
    if mouse.waypoint_mode || !matches!(mouse.pending_command, CommandMode::Normal) {
        return None;
    }
    let target = target?;
    battle.simulation.entities.get(target)?;
    // Unlike handleHackCommand, capture's hint also needs the first command set.
    if !battle.selected.first().is_some_and(|&id| {
        battle.simulation.catalog[battle.simulation.entities[id].type_index]
            .behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power|
                matches!(power.kind, generals_sim::skirmish::special_ability::PowerType::SpecialInfantryCaptureBuilding
                    | generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusCaptureBuilding)))
    }) {
        return None;
    }
    if battle.selected.iter().any(|&id| {
        locally_controlled(battle, id)
            && (battle.simulation.can_enter(id, target)
                || battle.simulation.can_repair(id, target)
                || battle.simulation.player_attack_target_reachable(id, target))
    }) {
        return None;
    }
    can_target(battle, target).then_some(target)
}

pub fn issue(battle: &mut Match, mouse: &mut Mouse, target: usize) -> bool {
    if !can_target(battle, target) {
        return false;
    }
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle.simulation.capture(id, target).is_ok();
        }
    }
    if issued {
        mouse.voices.extend(super::VoiceRequest::special_power(
            &battle.simulation,
            &battle.selected,
            &[
                "SPECIAL_INFANTRY_CAPTURE_BUILDING",
                "SPECIAL_BLACKLOTUS_CAPTURE_BUILDING",
            ],
        ));
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}
