//! Native team approach, garrison assignment and evacuation script actions.
use super::{AiCall, OpponentRules, State, map};
use crate::skirmish::{Skirmish, movement::distance_squared};

pub(super) fn action(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    team: usize,
) -> Option<()> {
    match call.name.as_str() {
        "TEAM_MOVE_TOWARDS_NEAREST_OBJECT_TYPE" => {
            approach(
                game,
                rules,
                state,
                team,
                &call.parameters.get(1)?.text,
                &call.parameters.get(2)?.text,
            )?;
        }
        "TEAM_GARRISON_NEAREST_BUILDING" => assign(game, state, team)?,
        "TEAM_EXIT_ALL" => {
            for &id in &state.teams[team].members {
                let _ = game.group_evacuate(id);
            }
        }
        "TEAM_EXIT_ALL_BUILDINGS" => {
            for &id in &state.teams[team].members {
                let _ = game.exit_container(id);
            }
        }
        _ => return None,
    }
    Some(())
}

fn approach(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    team: usize,
    name: &str,
    region: &str,
) -> Option<()> {
    let Some(area) = map::area(rules, state, region) else {
        return Some(());
    };
    let members = &state.teams[team].members;
    let Some(&leader) = members.first() else {
        return Some(());
    };
    if !game.catalog.iter().any(|unit| unit.name == name) {
        // ObjectTypes lists are handled below without inventing a template for
        // missing authored units such as the Fire Base.
        return approach_list(game, rules, state, team, name, region);
    }
    let point = game.entities[leader].position;
    let target = nearest_type(game, point, name, area);
    if let Some(target) = target {
        // These represented templates are immobile garrisons. AIMoveToState's
        // per-frame goal-object position is therefore the same fixed position.
        if !game.catalog[game.entities[target].type_index]
            .kind
            .is_building()
        {
            return None;
        }
        state.teams[team].follow = None;
        for &id in &state.teams[team].members {
            let _ = game.move_to(id, game.entities[target].position);
        }
    }
    Some(())
}

fn approach_list(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    team: usize,
    name: &str,
    region: &str,
) -> Option<()> {
    let names = state.object_lists.get(name)?;
    let area = map::area(rules, state, region)?;
    let leader = *state.teams[team].members.first()?;
    let point = game.entities[leader].position;
    let best = names
        .iter()
        .filter_map(|name| nearest_type(game, point, name, area))
        .min_by(|&left, &right| {
            distance_squared(point, game.entities[left].position)
                .total_cmp(&distance_squared(point, game.entities[right].position))
        });
    if let Some(id) = best {
        let template = game.catalog[game.entities[id].type_index].name.clone();
        approach(game, rules, state, team, &template, region)?;
    }
    Some(())
}

fn nearest_type(
    game: &Skirmish,
    point: [f32; 2],
    name: &str,
    area: &super::AiArea,
) -> Option<usize> {
    let template = game.catalog.iter().find(|unit| unit.name == name)?;
    let mut best = None;
    let mut distance = 1.0e20_f32;
    for id in game.partition.outward_objects(point) {
        let actor = &game.entities[id];
        if actor.masked()
            || !template.equivalent_to(&game.catalog[actor.type_index])
            || !map::inside(area, actor.position)
        {
            continue;
        }
        let current = distance_squared(point, actor.position);
        if current < distance {
            distance = current;
            best = Some(id);
        }
    }
    best
}

fn assign(game: &mut Skirmish, state: &mut State, team: usize) -> Option<()> {
    let members = &state.teams[team].members;
    let Some(&leader) = members.first() else {
        return Some(());
    };
    let hacker = game.catalog[game.entities[leader].type_index]
        .kind_of
        .iter()
        .any(|kind| kind == "MONEY_HACKER");
    let point = game.object_position(leader);
    let mut targets = game
        .partition
        .outward_objects([point[0], point[1]])
        .into_iter()
        .filter(|&host| {
            if hacker {
                let actor = &game.entities[host];
                // The native Hacker branch replaces GarrisonableByPlayer with
                // a KindOf-only filter. AI_ENTER handles ownership/admission.
                !actor.masked()
                    && game.catalog[actor.type_index]
                        .kind_of
                        .iter()
                        .any(|kind| kind == "FS_INTERNET_CENTER")
            } else {
                garrisonable(game, state.owner, host)
            }
        })
        .map(|host| {
            let other = game.object_position(host);
            let delta = [
                other[0] - point[0],
                other[1] - point[1],
                other[2] - point[2],
            ];
            let y_squared = delta[1] * delta[1];
            let x_squared = delta[0] * delta[0];
            let z_squared = delta[2] * delta[2];
            let horizontal = x_squared + y_squared;
            (host, horizontal + z_squared)
        })
        .collect::<Vec<_>>();
    // SimpleObjectIterator prepends each result, then performs a stable merge sort.
    targets.reverse();
    targets.sort_by(|left, right| left.1.total_cmp(&right.1));
    if !targets.is_empty() {
        state.teams[team].follow = None;
    }
    let mut members = state.teams[team].members.iter().copied();
    for (host, _) in targets {
        let Some(contain) = &game.catalog[game.entities[host].type_index].contain else {
            continue;
        };
        let capacity = contain.capacity;
        let available = usize::try_from(capacity)
            .ok()?
            .saturating_sub(game.passengers(host).len());
        for _ in 0..available {
            let Some(id) = members.find(|&id| {
                let unit = &game.catalog[game.entities[id].type_index];
                game.entities[id].alive()
                    && unit.speed > 0.0
                    && unit.kind_of.iter().any(|kind| kind == "INFANTRY")
                    && !unit.kind_of.iter().any(|kind| kind == "NO_GARRISON")
            }) else {
                return Some(());
            };
            // Source consumes this assignment even if AI_ENTER rejects damage,
            // capacity or another admission rule. Do not silently try another host.
            game.native_enter_from(id, host, crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT);
        }
    }
    Some(())
}

fn garrisonable(game: &Skirmish, owner: usize, host: usize) -> bool {
    let actor = &game.entities[host];
    actor.alive()
        && !actor.masked()
        && game.is_garrison(host)
        && game.catalog[actor.type_index]
            .kind_of
            .iter()
            .any(|kind| kind == "STRUCTURE")
        && (actor.owner == owner || (actor.owner == game.neutral_owner() && !actor.loaded()))
}

#[cfg(test)]
pub(super) mod tests;
