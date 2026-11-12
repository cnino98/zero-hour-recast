//! Native team recruitment and reinforcement work orders.
use super::{OpponentRules, RequestPhase, State, Team, WorkOrder, lifecycle, teams};
use crate::skirmish::{Skirmish, movement::distance_squared};

pub(super) fn candidate(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &State,
    destination: usize,
    type_index: usize,
    home: [f32; 2],
    radius: f32,
) -> Option<usize> {
    let template = state.teams[destination].template?;
    let priority = state.priorities[template];
    let mut closest = radius * radius;
    let mut recruit = None;
    // GameLogic::registerObject prepends objects. Equality replaces the current
    // choice, so an older equally distant candidate wins.
    for (id, actor) in game.entities.iter().enumerate().rev() {
        if actor.owner != state.owner
            || !game.catalog[actor.type_index].equivalent_to(&game.catalog[type_index])
            || !actor.complete()
        {
            continue;
        }
        let member = state
            .teams
            .iter()
            .find(|team| !team.deleted && team.members.contains(&id));
        let default_team = member.is_none_or(|team| team.template.is_none());
        let source_priority = member
            .and_then(|team| team.template)
            .map_or(0, |template| state.priorities[template]);
        let recruitable = member.is_none_or(|team| {
            team.recruitable.unwrap_or_else(|| {
                default_team
                    || team.template.is_some_and(|template| {
                        rules.factions[state.faction].teams[template].recruitment & 1 != 0
                    })
            })
        });
        if member.is_some_and(|team| {
            !team
                .native_instance
                .is_some_and(|native| game.team_membership.active(native))
                && team.template.is_some()
        }) || source_priority >= priority
            || !recruitable
            || actor.ai.registration.is_some() && !actor.ai.recruitable
        {
            continue;
        }
        let distance = distance_squared(home, actor.position);
        // Source default-team exception: its first candidate can expand the
        // search radius. Do not silently clamp it to MaxRecruitRadius.
        if default_team && recruit.is_none() {
            recruit = Some(id);
            closest = distance;
        }
        if distance <= closest {
            closest = distance;
            recruit = Some(id);
        }
    }
    recruit
}

pub(super) fn transfer(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    id: usize,
    destination: usize,
) {
    for team in &mut state.teams {
        team.members.retain(|&member| member != id);
    }
    let native = if state.teams[destination].template.is_some() {
        state.teams[destination]
            .native_instance
            .expect("created native AI team")
    } else {
        game.team_membership.default_team(state.owner)
    };
    game.team_membership.set_team(id, Some(native));
    state.teams[destination].members.push(id);
    if let Some(template) = state.teams[destination].template {
        if let Some(priority) = state.attack_priorities[template] {
            game.entities[id].ai.attack_priority = Some(priority);
        }
        game.entities[id].ai.mood.attitude =
            rules.factions[state.faction].teams[template].aggression;
    }
}

pub(super) fn disband(game: &mut Skirmish, team: &mut Team) {
    // Object::setTeam restores the default team's normal attitude. Its empty
    // priority name leaves the unit's existing attack information intact.
    for &id in &team.members {
        let native = game.team_membership.default_team(game.entities[id].owner);
        game.team_membership.set_team(id, Some(native));
        game.entities[id].ai.mood.attitude = 0;
    }
    team.members.clear();
}

fn move_home(game: &mut Skirmish, id: usize, home: [f32; 2]) {
    game.construction_move(id, [home[0], home[1], game.ground_height(home)]);
}

pub(super) fn fill(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    request: usize,
    order: usize,
) {
    let team = state.requests[request].team;
    let Some(template) = state.teams[team].template else {
        return;
    };
    let home = rules.factions[state.faction].teams[template]
        .home
        .unwrap_or(state.base_center);
    let type_index = state.requests[request].orders[order].type_index;
    while state.requests[request].orders[order].remaining > 0
        && state.requests[request].orders[order].producer.is_none()
    {
        let Some(id) = candidate(
            game,
            rules,
            state,
            team,
            type_index,
            home,
            rules.max_recruit_distance,
        ) else {
            break;
        };
        transfer(game, rules, state, id, team);
        state.requests[request].orders[order].remaining -= 1;
        move_home(game, id, home);
    }
}

pub(super) fn recruit(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    template: usize,
    radius: f32,
) {
    let definition = &rules.factions[state.faction].teams[template];
    let existing = definition
        .singleton
        .then(|| {
            state
                .teams
                .iter()
                .position(|team| team.template == Some(template) && !team.deleted)
        })
        .flatten();
    if existing.is_some_and(|team| {
        state.teams[team]
            .members
            .iter()
            .any(|&id| game.entities[id].alive())
    }) {
        return;
    }
    let team = existing.unwrap_or_else(|| {
        let team = state.teams.len();
        state.teams.push(Team::new(
            Some(template),
            game.tick,
            rules.factions[state.faction].scripts.len(),
        ));
        let prototype = state.team_prototypes[template];
        state.teams[team].native_instance = Some(if definition.singleton {
            game.team_membership
                .named_team(state.owner, &definition.name)
                .expect("singleton AI team")
        } else {
            game.team_membership.create_team(prototype)
        });
        team
    });
    lifecycle::production_actions(game, rules, state, team, None);
    let home = definition.home.unwrap_or([0.0; 2]);
    let radius = if radius < 1.0 { 99_999.0 } else { radius };
    for entry in &definition.units {
        let Some(type_index) = game.catalog.iter().position(|unit| unit.name == entry.name) else {
            continue;
        };
        for _ in 0..entry.maximum {
            let Some(id) = candidate(game, rules, state, team, type_index, home, radius) else {
                break;
            };
            transfer(game, rules, state, id, team);
            move_home(game, id, home);
        }
    }
    if state.teams[team].members.is_empty() {
        state.teams[team].deleted = !definition.singleton;
    } else {
        let request = state.requests.len();
        state.queue_request(team, Vec::new(), game.tick, false);
        state.build_queue.retain(|&index| index != request);
        state.requests[request].phase = RequestPhase::Ready;
        state.ready_queue.insert(0, request);
    }
}

pub(super) fn reinforce(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    minimum: i32,
) -> bool {
    let mut priority = minimum;
    let mut chosen = None;
    for (template, definition) in rules.factions[state.faction].teams.iter().enumerate() {
        if definition.recruitment & 2 == 0
            || state.priorities[template] <= priority
            || state.build_queue.iter().any(|&request| {
                state.teams[state.requests[request].team].template == Some(template)
            })
        {
            continue;
        }
        for (team, instance) in state.teams.iter().enumerate().rev().filter(|(_, team)| {
            team.template == Some(template)
                && !team.deleted
                && team.members.iter().any(|&id| game.entities[id].alive())
        }) {
            for entry in &definition.units {
                let Some(type_index) = game.catalog.iter().position(|unit| unit.name == entry.name)
                else {
                    continue;
                };
                let count = instance
                    .members
                    .iter()
                    .filter(|&&id| {
                        let actor = &game.entities[id];
                        actor.alive()
                            && game.catalog[actor.type_index]
                                .equivalent_to(&game.catalog[type_index])
                    })
                    .count();
                if count < usize::try_from(entry.maximum).unwrap_or(usize::MAX)
                    && teams::factory(game, state, type_index, false).is_some()
                {
                    chosen = Some((team, type_index));
                    priority = state.priorities[template];
                }
            }
        }
    }
    let Some((team, type_index)) = chosen else {
        return false;
    };
    let request = state.requests.len();
    state.queue_request(
        team,
        vec![WorkOrder {
            type_index,
            remaining: 1,
            producer: None,
            required: true,
            resource_gatherer: false,
        }],
        game.tick,
        true,
    );
    state.requests[request].priority = false;
    state.requests[request].reinforcement = true;
    let template = state.teams[team]
        .template
        .expect("Reinforced team has a template");
    let home = state.teams[team].members.last().map_or_else(
        || {
            rules.factions[state.faction].teams[template]
                .home
                .unwrap_or([0.0; 2])
        },
        |&id| game.entities[id].position,
    );
    if let Some(id) = candidate(
        game,
        rules,
        state,
        team,
        type_index,
        home,
        rules.max_recruit_distance,
    ) {
        transfer(game, rules, state, id, team);
        state.requests[request].orders[0].remaining = 0;
        state.requests[request].reinforcement_unit = Some(id);
        let _ = game.stop_actor(id);
    } else if let Some(producer) = teams::factory(game, state, type_index, false)
        && game.queue_unit(producer, type_index).is_ok()
    {
        state.requests[request].orders[0].producer = Some(producer);
    }
    true
}

pub(super) fn join(game: &mut Skirmish, state: &State, request: usize) {
    let request = &state.requests[request];
    let Some(id) = request
        .reinforcement_unit
        .filter(|&id| game.entities[id].complete())
    else {
        return;
    };
    let _ = game.stop_actor(id);
    if let Some(&other) = state.teams[request.team]
        .members
        .iter()
        .rev()
        .find(|&&other| other != id && game.entities[other].complete())
        && game.entities[other].ai_idle()
    {
        move_home(game, id, game.entities[other].position);
    }
    // AIUpdateInterface::joinTeam clears its machine, then reads its own state
    // (not the other member's). Preserve its idle result for a busy teammate.
}
