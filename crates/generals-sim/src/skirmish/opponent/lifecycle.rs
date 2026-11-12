//! `AIPlayer` build/ready queues and `Team::updateState`, at their native outer phases.
use super::{OpponentRules, RequestPhase, State, recruitment, scripts, teams};
use crate::skirmish::{Kind, Skirmish};

// Team::updateState retains callback history independently of queue activation.
pub(super) struct Callbacks {
    pub(super) was_idle: bool,
    pub(super) previous_units: usize,
    pub(super) destroy_threshold: Option<f32>,
}

pub(super) fn production_actions(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    team: usize,
    caller: Option<usize>,
) {
    let Some(template) = state.teams[team].template else {
        return;
    };
    let definition = &rules.factions[state.faction].teams[template];
    if !definition.execute_actions {
        return;
    }
    if let Some(script) = rules.factions[state.faction]
        .scripts
        .iter()
        .find(|script| script.name == definition.condition)
    {
        // friend_executeAction bypasses condition/difficulty/active/one-shot gates.
        for action in &script.actions {
            scripts::execute(game, rules, state, action, caller);
        }
    }
}

fn clear_team_flags(state: &mut State) {
    // ScriptEngine::clearTeamFlags names are case-sensitive and fixed in source.
    for flag in [
        "USA Team is Building",
        "USA Air Team Is Building",
        "USA Inf Team Is Building",
        "China Team is Building",
        "China Air Team Is Building",
        "China Inf Team Is Building",
        "GLA Team is Building",
        "GLA Inf Team is Building",
    ] {
        state.flags.insert(flag.into(), false);
    }
}

pub(super) fn check_ready(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    for index in state.ready_queue.clone() {
        let request = &state.requests[index];
        let team = &state.teams[request.team];
        let mut any_idle = team
            .members
            .iter()
            .any(|&id| game.entities[id].alive() && game.entities[id].ai_idle());
        let mut all_idle = team
            .members
            .iter()
            .all(|&id| !game.entities[id].alive() || game.entities[id].ai_idle());
        if request.reinforcement {
            all_idle = request
                .reinforcement_unit
                .is_none_or(|id| !game.entities[id].alive() || game.entities[id].ai_idle());
            any_idle = all_idle;
        }
        let start_action = team.template.is_some_and(|template| {
            let definition = &rules.factions[state.faction].teams[template];
            definition.execute_actions
                && rules.factions[state.faction]
                    .scripts
                    .iter()
                    .any(|script| script.name == definition.condition && !script.actions.is_empty())
        });
        if all_idle || (any_idle && start_action) || game.tick > request.created_at + 1800 {
            if request.reinforcement {
                recruitment::join(game, state, index);
            } else {
                if let Some(native) = state.teams[request.team].native_instance {
                    game.team_membership.activate(native);
                }
                clear_team_flags(state);
            }
            state.requests[index].phase = RequestPhase::Finished;
        }
    }
    state
        .ready_queue
        .retain(|&index| matches!(state.requests[index].phase, RequestPhase::Ready));
}

pub(super) fn check_building(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    // AIPlayer::checkQueuedTeams handles expiry before ordinary completion.
    for index in state.build_queue.clone() {
        let request = &state.requests[index];
        let template = state.teams[request.team].template;
        let timeout = template.map_or(0, |template| {
            rules.factions[state.faction].teams[template].initial_idle_frames
        });
        if timeout == 0 || game.tick <= request.created_at + u64::from(timeout) {
            continue;
        }
        if request
            .orders
            .iter()
            .any(|order| order.required && order.remaining > u32::from(order.producer.is_some()))
        {
            // TeamInQueue::disband keeps its members alive on the default team.
            // A singleton instance survives and the queue destructor activates it.
            let team = &mut state.teams[request.team];
            recruitment::disband(game, team);
            team.deleted = template
                .is_none_or(|template| !rules.factions[state.faction].teams[template].singleton);
            if !team.deleted
                && let Some(native) = team.native_instance
            {
                game.team_membership.activate(native);
            }
            team.sequence = None;
            team.follow = None;
            state.requests[index].orders.clear();
            state.requests[index].phase = RequestPhase::Finished;
            clear_team_flags(state);
        } else if request.orders.iter().all(|order| order.producer.is_none()) {
            ready(state, index);
        }
    }
    for index in state.build_queue.clone() {
        let request = &state.requests[index];
        if !matches!(request.phase, RequestPhase::Building) {
            continue;
        }
        if request.orders.iter().all(|order| order.remaining == 0) {
            ready(state, index);
        } else if state.teams[request.team]
            .members
            .iter()
            .any(|&id| game.entities[id].alive() && game.entities[id].ai_idle())
        {
            let team = request.team;
            production_actions(game, rules, state, team, Some(team));
        }
    }
    state
        .build_queue
        .retain(|&index| matches!(state.requests[index].phase, RequestPhase::Building));
}

fn ready(state: &mut State, index: usize) {
    state.requests[index].phase = RequestPhase::Ready;
    state.ready_queue.insert(0, index);
}

pub(super) fn update_states(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    for index in 0..state.teams.len() {
        let Some(template) = state.teams[index].template else {
            continue;
        };
        let Some(native) = state.teams[index].native_instance else {
            continue;
        };
        if !game.team_membership.active(native) || state.teams[index].deleted {
            continue;
        }
        let definition = &rules.factions[state.faction].teams[template];
        if game.team_membership.take_created(native) {
            callback(game, rules, state, &definition.on_create, index);
            if !definition.on_destroyed.is_empty() {
                let count = state.teams[index].members.len();
                state.teams[index].callbacks.previous_units = count;
                let count = count_real(count);
                let lost = count * definition.destroyed_threshold;
                state.teams[index].callbacks.destroy_threshold =
                    Some((count - lost).trunc().min(count - 1.0).max(0.0));
            }
        }
        let count = state.teams[index]
            .members
            .iter()
            .filter(|&&id| game.entities[id].alive())
            .count();
        if count != state.teams[index].callbacks.previous_units
            && state.teams[index]
                .callbacks
                .destroy_threshold
                .is_some_and(|threshold| count_real(count) <= threshold)
        {
            callback(game, rules, state, &definition.on_destroyed, index);
            state.teams[index].callbacks.destroy_threshold = None;
        }
        state.teams[index].callbacks.previous_units = count;
        idle_callback(game, rules, state, &definition.on_idle, index);
    }
    // ScriptEngine progresses its sequential scripts after Team::updateState.
    for index in 0..state.teams.len() {
        if !state.teams[index].deleted {
            teams::sequence(game, rules, state, index);
        }
    }
}

fn idle_callback(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    name: &str,
    team: usize,
) {
    if name.is_empty() {
        return;
    }
    let mut idle = true;
    let mut any_alive = false;
    for &id in &state.teams[team].members {
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        if !actor.alive() || unit.speed == 0.0 && unit.kind != Kind::Defense {
            continue;
        }
        any_alive = true;
        idle &= actor.ai_idle();
    }
    // Source requires both current and preceding update to be idle, and keeps
    // calling on later idle updates; this is not an idle-transition callback.
    if any_alive && idle && state.teams[team].callbacks.was_idle {
        callback(game, rules, state, name, team);
    }
    state.teams[team].callbacks.was_idle = idle;
}

fn callback(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    name: &str,
    team: usize,
) {
    let data = &rules.factions[state.faction];
    if let Some(group) = data.groups.iter().position(|group| group.name == name) {
        if data.groups[group].subroutine && state.groups[group] {
            for (index, script) in data.scripts.iter().enumerate() {
                if script.group == Some(group) {
                    teams::run_script(game, rules, state, index, team);
                }
            }
        }
    } else if let Some(script) = data
        .scripts
        .iter()
        .position(|script| script.name == name && script.flags & 32 != 0)
    {
        teams::run_script(game, rules, state, script, team);
    }
}

fn count_real(count: usize) -> f32 {
    let bytes = u32::try_from(count)
        .expect("Native object IDs bound the team count")
        .to_le_bytes();
    let high = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
    high + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]))
}
