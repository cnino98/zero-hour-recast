use super::{
    AiCall, AiTeam, OpponentRules, State, Team, WorkOrder, adjusted_timer, collectors, garrisons,
    lifecycle, map, priorities, recruitment, scripts,
};
use crate::skirmish::Skirmish;

pub(super) fn update(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    lifecycle::check_ready(game, rules, state);
    lifecycle::check_building(game, rules, state);
    schedule(game, rules, state);
    run_teams(game, rules, state);
}

fn schedule(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    if state.enabled & 2 == 0 {
        return;
    }
    if state.team_timer > 0.0 {
        state.team_timer = (state.team_timer - 1.0).min(90.0);
        if state.team_timer <= 0.0 {
            state.team_delay = 0;
        }
    }
    state.team_delay -= 1;
    if state.team_delay > 0 {
        return;
    }
    collectors::queue(game, rules, state);
    queue_units(game, rules, state);
    if state.team_timer > 0.0 {
        state.team_delay = 60;
        return;
    }
    let data = &rules.factions[state.faction];
    let mut choices = Vec::new();
    let mut priority = -99_999;
    for (index, template) in data.teams.iter().enumerate() {
        if template.units.is_empty()
            || state
                .teams
                .iter()
                .filter(|team| {
                    team.template == Some(index)
                        && !team.deleted
                        && (!team
                            .native_instance
                            .is_some_and(|native| game.team_membership.active(native))
                            || team.members.iter().any(|&id| game.entities[id].alive()))
                })
                .count()
                >= usize::try_from(template.max_instances).unwrap_or(usize::MAX)
        {
            continue;
        }
        let Some(script) = data
            .scripts
            .iter()
            .find(|script| script.name == template.condition)
        else {
            continue;
        };
        if !scripts::difficulty(script, state)
            || scripts::conditions(game, rules, state, script, None) != Some(true)
            || !possible(game, state, template, rules)
        {
            continue;
        }
        if state.priorities[index] > priority {
            choices.clear();
            priority = state.priorities[index];
        }
        if state.priorities[index] == priority {
            choices.push(index);
        }
    }
    if recruitment::reinforce(game, rules, state, priority) {
        state.team_timer = adjusted_timer(
            rules.team_seconds,
            game.players[state.owner].cash,
            rules,
            false,
        );
        collectors::queue(game, rules, state);
        queue_units(game, rules, state);
    } else if let Ok(maximum) = u32::try_from(choices.len().saturating_sub(1))
        && !choices.is_empty()
    {
        let selected = choices[usize::try_from(game.random_between(maximum)).unwrap_or(0)];
        queue(game, rules, state, selected, false);
        state.team_timer = adjusted_timer(
            rules.team_seconds,
            game.players[state.owner].cash,
            rules,
            false,
        );
        // AISkirmishPlayer::processTeamBuilding queues the new recipe immediately.
        collectors::queue(game, rules, state);
        queue_units(game, rules, state);
    }
    state.team_delay = 60;
}
fn possible(game: &Skirmish, state: &State, template: &AiTeam, rules: &OpponentRules) -> bool {
    let mut cost = 0.0;
    let mut idle = false;
    for entry in &template.units {
        let Some(index) = game.catalog.iter().position(|unit| unit.name == entry.name) else {
            return false;
        };
        if factory(game, state, index, true).is_none() {
            return false;
        }
        idle |= factory(game, state, index, false).is_some();
        let Ok(cost_each) = u16::try_from(game.build_cost(state.owner, index)) else {
            return false;
        };
        let Ok(total) = u16::try_from(entry.maximum.saturating_add(entry.minimum)) else {
            return false;
        };
        // AIPlayer::isPossibleToBuildTeam accumulates into Int after each source float product.
        let unit_cost = f32::from(cost_each) * (f32::from(total) / 2.0);
        cost = (cost + unit_cost).trunc();
    }
    let available = f64::from(game.players[state.owner].cash);
    idle && available >= f64::from((cost * rules.team_resources_to_start).trunc())
}
pub(super) fn factory(
    game: &Skirmish,
    state: &State,
    type_index: usize,
    busy: bool,
) -> Option<usize> {
    let mut last_busy = None;
    for entry in &state.buildings {
        let Some(id) = entry.object else {
            continue;
        };
        let actor = &game.entities[id];
        if !actor.exists()
            || actor.owner != state.owner
            || actor.under_construction()
            || actor.selling()
            || game.catalog[actor.type_index].production.is_none()
            || !game.production_possible(id, type_index)
        {
            continue;
        }
        if actor.queue.is_empty() {
            return Some(id);
        }
        if busy {
            last_busy = Some(id);
        }
    }
    last_busy
}
pub(super) fn queue(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    index: usize,
    priority: bool,
) {
    if state.enabled & 2 == 0 {
        return;
    }
    let template = &rules.factions[state.faction].teams[index];
    let mut orders = Vec::new();
    for required in [false, true] {
        for unit in &template.units {
            let Some(type_index) = game.catalog.iter().position(|kind| kind.name == unit.name)
            else {
                state.report(&format!("Team {}: missing {}", template.name, unit.name));
                return;
            };
            let count = if required {
                unit.minimum
            } else {
                unit.maximum - unit.minimum
            };
            if count > 0 {
                orders.insert(
                    0,
                    WorkOrder {
                        type_index,
                        remaining: count,
                        producer: None,
                        required,
                        resource_gatherer: false,
                    },
                );
            }
        }
    }
    if orders.is_empty() {
        return;
    }
    let existing = template
        .singleton
        .then(|| {
            state
                .teams
                .iter()
                .position(|team| team.template == Some(index) && !team.deleted)
        })
        .flatten();
    if priority
        && existing.is_some_and(|team| {
            state.teams[team]
                .members
                .iter()
                .any(|&id| game.entities[id].alive())
        })
    {
        return;
    }
    let team = existing.unwrap_or_else(|| {
        let team = state.teams.len();
        state.teams.push(Team::new(
            Some(index),
            game.tick,
            rules.factions[state.faction].scripts.len(),
        ));
        let prototype = state.team_prototypes[index];
        state.teams[team].native_instance = Some(if template.singleton {
            game.team_membership
                .named_team(state.owner, &template.name)
                .expect("singleton AI team")
        } else {
            game.team_membership.create_team(prototype)
        });
        team
    });
    // TeamFactory::createInactiveTeam executes once without the calling team;
    // AIPlayer::buildSpecificAITeam then executes again with the queued team.
    lifecycle::production_actions(game, rules, state, team, None);
    state.queue_request(team, orders, game.tick, priority);
    lifecycle::production_actions(game, rules, state, team, Some(team));
}

fn queue_units(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    for request in state.build_queue.clone() {
        for order in 0..state.requests[request].orders.len() {
            recruitment::fill(game, rules, state, request, order);
            let item = &state.requests[request].orders[order];
            if item.remaining == 0 || item.producer.is_some() {
                // WorkOrder::validateFactory runs in queueUnits, after expiry/readiness.
                if item.producer.is_some_and(|id| {
                    !game.entities[id].alive() || game.entities[id].owner != state.owner
                }) {
                    state.requests[request].orders[order].producer = None;
                }
                continue;
            }
            let type_index = item.type_index;
            if let Some(producer) =
                factory(game, state, type_index, state.requests[request].priority)
                && game.queue_unit(producer, type_index).is_ok()
            {
                state.requests[request].orders[order].producer = Some(producer);
            }
        }
    }
}
fn run_teams(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    for index in 0..state.teams.len() {
        let Some(template) = state.teams[index]
            .template
            .filter(|_| !state.teams[index].deleted)
        else {
            continue;
        };
        let template = &rules.factions[state.faction].teams[template];
        for name in &template.generic_scripts {
            if let Some(script) = rules.factions[state.faction]
                .scripts
                .iter()
                .position(|script| script.name == *name)
            {
                run_generic_script(game, rules, state, script, index);
            }
        }
        map::advance(game, rules, state, index);
    }
}
pub(super) fn run_script(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    script: usize,
    team: usize,
) {
    let definition = &rules.factions[state.faction].scripts[script];
    let runtime = &state.scripts[script];
    if !runtime.active || game.tick < runtime.next || !scripts::difficulty(definition, state) {
        return;
    }
    state.scripts[script].next = game.tick + u64::from(definition.delay_frames);
    let Some(result) = scripts::conditions(game, rules, state, definition, Some(team)) else {
        return;
    };
    let actions = if result {
        &definition.actions
    } else {
        &definition.false_actions
    };
    for action in actions {
        scripts::execute(game, rules, state, action, Some(team));
    }
    if definition.flags & 2 != 0 && (result || !actions.is_empty()) {
        state.scripts[script].active = false;
    }
}

fn run_generic_script(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    script: usize,
    team: usize,
) {
    // Team::updateGenericScripts tests conditions directly. Global script active,
    // difficulty and periodic-delay fields do not gate this per-team hook.
    let definition = &rules.factions[state.faction].scripts[script];
    if !state.teams[team].scripts[script].active
        || scripts::conditions(game, rules, state, definition, Some(team)) != Some(true)
    {
        return;
    }
    if definition.flags & 2 != 0 {
        state.teams[team].scripts[script].active = false;
    }
    for action in &definition.actions {
        scripts::execute(game, rules, state, action, Some(team));
    }
}
pub(super) fn sequence(game: &mut Skirmish, rules: &OpponentRules, state: &mut State, team: usize) {
    if state.teams[team].follow.is_some() {
        return;
    }
    let Some((script, mut instruction)) = state.teams[team].sequence else {
        return;
    };
    // ScriptEngine::processSequentialScripts waits for the native group to become idle.
    while state.teams[team]
        .members
        .iter()
        .all(|&id| !game.entities[id].alive() || game.entities[id].ai_idle())
    {
        let Some(call) = rules.factions[state.faction].scripts[script]
            .actions
            .get(instruction)
        else {
            state.teams[team].sequence = None;
            break;
        };
        scripts::execute(game, rules, state, call, Some(team));
        instruction += 1;
        state.teams[team].sequence = Some((script, instruction));
    }
}
pub(super) fn action(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    team: Option<usize>,
) -> Option<()> {
    if super::commands::button_action(game, rules, state, call, team).is_some() {
        return Some(());
    }
    if !matches!(
        call.name.as_str(),
        "TEAM_STOP"
            | "TEAM_STOP_AND_DISBAND"
            | "TEAM_INCREASE_PRIORITY"
            | "TEAM_DECREASE_PRIORITY"
            | "TEAM_EXECUTE_SEQUENTIAL_SCRIPT"
            | "TEAM_HUNT"
            | "SKIRMISH_MOVE_TO_APPROACH_PATH"
            | "SKIRMISH_FOLLOW_APPROACH_PATH"
            | "TEAM_EXIT_ALL"
            | "TEAM_EXIT_ALL_BUILDINGS"
            | "TEAM_LOAD_TRANSPORTS"
            | "TEAM_ENTER_NAMED"
            | "TEAM_MOVE_TOWARDS_NEAREST_OBJECT_TYPE"
            | "TEAM_GARRISON_NEAREST_BUILDING"
            | "TEAM_GUARD_AREA"
            | "TEAM_APPLY_ATTACK_PRIORITY_SET"
            | "TEAM_AVAILABLE_FOR_RECRUITMENT"
            | "TEAM_SET_ATTITUDE"
    ) {
        return None;
    }
    let Some(team) = named_team(game, rules, state, &call.parameters.first()?.text, team) else {
        return Some(());
    };
    if super::transports::action(game, state, call, team).is_some() {
        return Some(());
    }
    match call.name.as_str() {
        "TEAM_STOP" | "TEAM_STOP_AND_DISBAND" => {
            let members = &state.teams[team].members;
            for &id in members {
                let _ = game.stop_actor(id);
            }
            state.teams[team].follow = None;
            if call.name == "TEAM_STOP_AND_DISBAND" {
                // doTeamStop(TRUE) merges members into the player's default team.
                // The singleton instance survives.
                super::recruitment::disband(game, &mut state.teams[team]);
            }
        }
        "TEAM_INCREASE_PRIORITY" | "TEAM_DECREASE_PRIORITY" => {
            let index = state.teams[team].template?;
            let definition = &rules.factions[state.faction].teams[index];
            state.priorities[index] = if call.name == "TEAM_INCREASE_PRIORITY" {
                state.priorities[index].wrapping_add(definition.priority_success)
            } else {
                state.priorities[index].wrapping_sub(definition.priority_failure)
            };
        }
        "TEAM_EXECUTE_SEQUENTIAL_SCRIPT" => {
            let name = &call.parameters.get(1)?.text;
            let script = rules.factions[state.faction]
                .scripts
                .iter()
                .position(|script| script.name == *name)?;
            // doTeamStartSequentialScript idles the group before appending it.
            // Otherwise a previous guard order prevents its first instruction.
            for &id in &state.teams[team].members {
                let _ = game.stop_actor(id);
            }
            state.teams[team].follow = None;
            state.teams[team].sequence = Some((script, 0));
        }
        "TEAM_HUNT" => {
            state.teams[team].follow = None;
            let Some(native) = state.teams[team].native_instance else {
                return Some(());
            };
            let members = game.team_membership.team_objects(native).to_vec();
            for id in game.materialize_native_group(&members) {
                if game.entities[id].ai.registration.is_some() {
                    game.native_hunt_from(id, 1);
                }
            }
        }
        "SKIRMISH_MOVE_TO_APPROACH_PATH" => {
            map::begin(
                game,
                rules,
                state,
                team,
                &call.parameters.get(1)?.text,
                false,
            );
        }
        "SKIRMISH_FOLLOW_APPROACH_PATH" => {
            map::begin(
                game,
                rules,
                state,
                team,
                &call.parameters.get(1)?.text,
                true,
            );
        }
        _ => return configure_team(game, rules, state, call, team),
    }
    Some(())
}

fn configure_team(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    team: usize,
) -> Option<()> {
    match call.name.as_str() {
        "TEAM_GUARD_AREA" => {
            let Some(area) = map::area(rules, state, &call.parameters.get(1)?.text) else {
                return Some(());
            };
            let area = rules
                .map
                .areas
                .iter()
                .position(|candidate| candidate.name == area.name)?;
            state.teams[team].follow = None;
            for &id in &state.teams[team].members {
                let _ = game.guard_area(id, area);
            }
        }
        "TEAM_APPLY_ATTACK_PRIORITY_SET" => {
            let index = priorities::find(game, &call.parameters.get(1)?.text);
            if let Some(template) = state.teams[team].template
                && index.is_some()
            {
                state.attack_priorities[template] = index;
            }
            for &id in &state.teams[team].members {
                game.entities[id].ai.attack_priority = index;
            }
        }
        "TEAM_AVAILABLE_FOR_RECRUITMENT" => {
            state.teams[team].recruitable = Some(call.parameters.get(1)?.integer != 0);
        }
        "TEAM_SET_ATTITUDE" => {
            let attitude = call.parameters.get(1)?.integer;
            if !(-2..=2).contains(&attitude) {
                return None;
            }
            for &id in &state.teams[team].members {
                game.entities[id].ai.mood.attitude = attitude;
            }
        }
        _ => return garrisons::action(game, rules, state, call, team),
    }
    Some(())
}

pub(super) fn condition(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    team: Option<usize>,
) -> Option<bool> {
    let team = named_team(game, rules, state, &call.parameters.first()?.text, team)?;
    match call.name.as_str() {
        "TEAM_INSIDE_AREA_PARTIALLY" => {
            let area = map::area(rules, state, &call.parameters.get(1)?.text)?;
            Some(state.teams[team].members.iter().any(|&id| {
                game.entities[id].alive() && map::inside(area, game.entities[id].position)
            }))
        }
        _ => None,
    }
}

pub(super) fn named_team(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &State,
    name: &str,
    caller: Option<usize>,
) -> Option<usize> {
    // ScriptEngine::getTeamNamed: calling instance precedes prototype lookup.
    if name == "<This Team>" {
        return caller;
    }
    let template = rules.factions[state.faction]
        .teams
        .iter()
        .position(|team| team.name == name)?;
    if caller.is_some_and(|id| state.teams[id].template == Some(template)) {
        return caller;
    }
    state.teams.iter().rposition(|team| {
        team.template == Some(template)
            && !team.deleted
            && (!rules.factions[state.faction].teams[template].singleton
                || team
                    .native_instance
                    .is_some_and(|native| game.team_membership.active(native)))
    })
}
