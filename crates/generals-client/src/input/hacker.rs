//! `CommandXlat::handleHackCommand` and the ordinary context precedence.
use super::{CommandMode, Mouse, locally_controlled};
use crate::playtest::Match;

pub fn context_target(battle: &mut Match, mouse: &Mouse, target: Option<usize>) -> Option<usize> {
    let target = context_candidate(battle, mouse, target)?;
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if locally_controlled(battle, id)
            && battle.simulation.can_context_disable_building(id, target)
        {
            return Some(target);
        }
    }
    None
}

pub fn finite_context_target(
    battle: &mut Match,
    mouse: &Mouse,
    target: Option<usize>,
) -> Option<(usize, usize)> {
    let target = context_candidate(battle, mouse, target)?;
    for ability in 1..=2 {
        for index in 0..battle.selected.len() {
            let id = battle.selected[index];
            if locally_controlled(battle, id) && battle.simulation.can_hack(id, target, ability) {
                return Some((target, ability));
            }
        }
    }
    None
}

fn context_candidate(battle: &mut Match, mouse: &Mouse, target: Option<usize>) -> Option<usize> {
    if mouse.waypoint_mode || !matches!(mouse.pending_command, CommandMode::Normal) {
        return None;
    }
    let target = target?;
    // Native selection-wide boarding/attack/capture choices precede hacking.
    // Do not split a mixed army into an attack and a different special command.
    for index in 0..battle.selected.len() {
        let id = battle.selected[index];
        if locally_controlled(battle, id)
            && (battle.simulation.can_enter(id, target)
                || battle.simulation.can_repair(id, target)
                || super::attack::ordinary_target(battle, id, target)
                || battle.simulation.can_capture(id, target))
        {
            return None;
        }
    }
    Some(target)
}

pub fn issue_finite_context(
    battle: &mut Match,
    mouse: &mut Mouse,
    target: usize,
    ability: usize,
) -> bool {
    let first_has_command = battle.command_drawables().first().is_some_and(|&id| {
        let actor = &battle.simulation.entities[id];
        actor.owner == 0
            && battle.simulation.catalog[actor.type_index]
                .behavior.special_abilities.iter().any(|rules| rules.power.as_ref().is_some_and(|power|
                    matches!((ability, power.kind),
                        (1, generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusDisableVehicleHack)
                        | (2, generals_sim::skirmish::special_ability::PowerType::SpecialBlacklotusStealCashHack))))
    });
    first_has_command && issue_finite(battle, mouse, target, ability)
}

pub fn issue(battle: &mut Match, mouse: &mut Mouse, target: usize, context: bool) -> bool {
    // handleHackCommand chooses the actual button from the first selected
    // drawable. DO_HINT deliberately bypasses that first-button lookup.
    let first_has_command = battle.command_drawables().first().is_some_and(|&id| {
        battle.simulation.entities.get(id).is_some_and(|actor| {
            actor.owner == 0
                && battle.simulation.catalog[actor.type_index]
                    .behavior
                    .special_abilities
                    .iter().any(|rules| rules.power.as_ref().is_some_and(|power| power.kind == generals_sim::skirmish::special_ability::PowerType::SpecialHackerDisableBuilding))
        })
    });
    if !battle.simulation.visible_to(0, target) || (context && !first_has_command) {
        return false;
    }
    let mut issued = false;
    for id in battle.command_message_members() {
        if !locally_controlled(battle, id) {
            continue;
        }
        let result = if context {
            battle.simulation.context_disable_building(id, target)
        } else {
            battle.simulation.disable_building(id, target)
        };
        issued |= result.is_ok();
    }
    if issued {
        mouse.voices.extend(super::VoiceRequest::special_power(
            &battle.simulation,
            battle.command_drawables(),
            &["SPECIAL_HACKER_DISABLE_BUILDING"],
        ));
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}

pub fn issue_finite(battle: &mut Match, mouse: &mut Mouse, target: usize, ability: usize) -> bool {
    if !battle.simulation.visible_to(0, target) {
        return false;
    }
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle.simulation.hack(id, target, ability).is_ok();
        }
    }
    if issued {
        let power = if ability == 1 {
            "SPECIAL_BLACKLOTUS_DISABLE_VEHICLE_HACK"
        } else {
            "SPECIAL_BLACKLOTUS_STEAL_CASH_HACK"
        };
        mouse.voices.extend(super::VoiceRequest::special_power(
            &battle.simulation,
            battle.command_drawables(),
            &[power],
        ));
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}
