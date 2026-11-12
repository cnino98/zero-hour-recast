//! `doTeamUseCommandButtonAbility` and the player's source default team.
use super::{AiCall, OpponentRules, State, teams};
use crate::skirmish::{Skirmish, battle_plan::Plan};

pub(super) fn button_action(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    caller: Option<usize>,
) -> Option<()> {
    match call.name.as_str() {
        "TEAM_HUNT_WITH_COMMAND_BUTTON" => team_hunt_command(game, rules, state, call, caller),
        "TEAM_USE_COMMANDBUTTON_ABILITY" => team_command(game, rules, state, call, caller),
        _ => None,
    }
}

fn team_command(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    caller: Option<usize>,
) -> Option<()> {
    let name = &call.parameters.first()?.text;
    let button = &call.parameters.get(1)?.text;
    let plan = match button.as_str() {
        "Command_InitiateBattlePlanBombardment" => Some(Plan::Bombardment),
        "Command_InitiateBattlePlanHoldTheLine" => Some(Plan::HoldTheLine),
        "Command_InitiateBattlePlanSearchAndDestroy" => Some(Plan::SearchAndDestroy),
        "Command_ChinaInfantryHackerInternetHack" => None,
        _ => return None,
    };
    let members = if name == &rules.factions[state.faction].default_team {
        // Native membership is prepended. Recruited teams no longer belong to
        // the default team; newly created buildings do until explicitly moved.
        game.entities
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(id, actor)| {
                (actor.alive()
                    && actor.owner == state.owner
                    && !state
                        .teams
                        .iter()
                        .any(|team| !team.deleted && team.members.contains(&id)))
                .then_some(id)
            })
            .collect::<Vec<_>>()
    } else if let Some(team) = teams::named_team(game, rules, state, name, caller) {
        state.teams[team].members.clone()
    } else {
        return Some(());
    };
    for id in members {
        if let Some(plan) = plan {
            game.choose_battle_plan(id, plan);
        } else {
            let _ = game.hack_internet_actor(id);
        }
    }
    Some(())
}

fn team_hunt_command(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    caller: Option<usize>,
) -> Option<()> {
    let name = &call.parameters.first()?.text;
    let button = &call.parameters.get(1)?.text;
    let native = if name == &rules.factions[state.faction].default_team {
        Some(game.team_membership.default_team(state.owner))
    } else {
        teams::named_team(game, rules, state, name, caller)
            .and_then(|team| state.teams[team].native_instance)
    };
    if let Some(native) = native {
        let members = game.team_membership.team_objects(native).to_vec();
        game.team_command_button_hunt(&members, button);
    }
    Some(())
}
