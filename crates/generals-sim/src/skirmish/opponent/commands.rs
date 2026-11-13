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
        "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE" => {
            nearest_type_power(game, rules, state, call, caller)
        }
        "TEAM_HUNT_WITH_COMMAND_BUTTON" => team_hunt_command(game, rules, state, call, caller),
        "TEAM_USE_COMMANDBUTTON_ABILITY" => team_command(game, rules, state, call, caller),
        _ => None,
    }
}

fn nearest_type_power(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    caller: Option<usize>,
) -> Option<()> {
    let team = &call.parameters.first()?.text;
    let button = &call.parameters.get(1)?.text;
    let object_type = &call.parameters.get(2)?.text;
    let native = if team == &rules.factions[state.faction].default_team {
        Some(game.team_membership.default_team(state.owner))
    } else {
        teams::named_team(game, rules, state, team, caller)
            .and_then(|index| state.teams[index].native_instance)
    };
    let Some(native) = native else {
        return Some(());
    };
    let members = game.team_membership.team_objects(native).to_vec();
    if game.catalog.iter().any(|unit| unit.name == *object_type) {
        game.team_power_on_nearest_type(&members, button, std::slice::from_ref(object_type))
    } else if let Some(types) = state.object_lists.get(object_type) {
        game.team_power_on_nearest_type(&members, button, types)
    } else {
        Some(())
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

// ScriptEngine handles this wait before dispatching ordinary sequential actions.
pub(super) fn button_ready(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    caller: Option<usize>,
) -> Option<bool> {
    let team = &call.parameters.get(1)?.text;
    let name = &call.parameters.get(2)?.text;
    let native = if team == &rules.factions[state.faction].default_team {
        Some(game.team_membership.default_team(state.owner))
    } else {
        teams::named_team(game, rules, state, team, caller)
            .and_then(|index| state.teams[index].native_instance)
    };
    let Some(native) = native else {
        return Some(false);
    };
    let Some(button) = game
        .command_buttons
        .buttons
        .iter()
        .find(|button| button.name == *name)
    else {
        return Some(false);
    };
    let power = button
        .power
        .as_ref()
        .map(|power| (power.name.clone(), power.kind));
    let upgrade = button.upgrade.clone();
    let all = call.name == "SKIRMISH_WAIT_FOR_COMMANDBUTTON_AVAILABLE_ALL";
    let members = game.team_membership.team_objects(native).to_vec();
    for id in members {
        if let Some((_, kind)) = &power {
            if !game.catalog[game.entities[id].type_index]
                .special_powers
                .iter()
                .any(|rules| {
                    crate::skirmish::special_ability::PowerType::parse(&rules.power_type)
                        == Some(*kind)
                })
            {
                continue;
            }
        } else if upgrade.is_none() {
            continue;
        }
        let ready = power.as_ref().is_some_and(|(name, _)| {
            game.special_power_percent(id, name).to_bits() == 1.0_f32.to_bits()
        }) || upgrade
            .as_ref()
            .is_some_and(|name| upgrade_ready(game, id, name));
        if ready != all {
            return Some(ready);
        }
    }
    Some(all)
}

fn upgrade_ready(game: &Skirmish, id: usize, name: &str) -> bool {
    if game.object_has_upgrade(id, name) {
        return false;
    }
    let actor = &game.entities[id];
    let has = |upgrade: &String| {
        upgrade == name
            || game.has_upgrade(actor.owner, upgrade)
            || game.object_has_upgrade(id, upgrade)
    };
    game.catalog[actor.type_index]
        .behavior_modules
        .iter()
        .enumerate()
        .any(|(index, module)| {
            let data = &module.upgrade;
            module.interfaces & 0x80 != 0
                && !game.update_objects[actor.update_owner].upgrade_executed[index]
                && !data.triggers.is_empty()
                && !data.conflicts.iter().any(has)
                && if data.requires_all {
                    data.triggers.iter().all(has)
                } else {
                    data.triggers.iter().any(has)
                }
        })
}
