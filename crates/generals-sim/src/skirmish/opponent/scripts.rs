use super::{
    AiCall, AiScript, Counter, OpponentRules, State, construction, defenses, map, priorities,
    recruitment, teams,
};
use crate::skirmish::Skirmish;

pub(super) fn update(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    // ScriptEngine::update decrements timers before evaluating this frame's scripts.
    for counter in state.counters.values_mut() {
        if counter.countdown && counter.value >= 0 {
            counter.value -= 1;
        }
    }
    let data = &rules.factions[state.faction];
    for (index, script) in data.scripts.iter().enumerate() {
        if script.flags & 32 != 0
            || script
                .group
                .is_some_and(|group| !state.groups[group] || data.groups[group].subroutine)
            || !state.scripts[index].active
            || !difficulty(script, state)
            || game.tick < state.scripts[index].next
        {
            continue;
        }
        if script.delay_frames > 0 {
            state.scripts[index].next = game.tick + u64::from(script.delay_frames);
        }
        let Some(result) = conditions(game, rules, state, index, None) else {
            continue;
        };
        let actions = if result {
            &script.actions
        } else {
            &script.false_actions
        };
        for call in actions {
            execute(game, rules, state, call, None);
        }
        // ScriptEngine::executeScript also deactivates a one-shot with ELSE actions.
        if script.flags & 2 != 0 && (result || !actions.is_empty()) {
            state.scripts[index].active = false;
        }
    }
}

pub(super) const fn difficulty(script: &AiScript, state: &State) -> bool {
    script.flags & (4 << state.difficulty_index()) != 0
}

pub(super) fn conditions(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    script_index: usize,
    team: Option<usize>,
) -> Option<bool> {
    let script = &rules.factions[state.faction].scripts[script_index];
    let mut unknown = false;
    for (branch_index, branch) in script.conditions.iter().enumerate() {
        if branch.is_empty() {
            continue;
        }
        let mut accepted = true;
        for (call_index, call) in branch.iter().enumerate() {
            let key = (script_index, branch_index, call_index);
            let result = if call.name == "SKIRMISH_TECH_BUILDING_WITHIN_DISTANCE" {
                tech_within_distance(game, rules, state, call, key)
            } else {
                condition(game, rules, state, call, team)
            };
            match result {
                Some(true) => {}
                Some(false) => {
                    accepted = false;
                    break;
                }
                None => {
                    state.report(&call.name);
                    unknown = true;
                    accepted = false;
                    break;
                }
            }
        }
        if accepted {
            return Some(true);
        }
    }
    if unknown { None } else { Some(false) }
}
fn condition(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &State,
    call: &AiCall,
    team: Option<usize>,
) -> Option<bool> {
    let p = &call.parameters;
    Some(match call.name.as_str() {
        "CONDITION_TRUE" => true,
        "CONDITION_FALSE" => false,
        "COUNTER" => compare(
            i64::from(state.counters.get(&p.first()?.text).map_or(0, |c| c.value)),
            p.get(1)?.integer,
            i64::from(p.get(2)?.integer),
        ),
        "FLAG" => *state.flags.get(&p.first()?.text).unwrap_or(&false) == (p.get(1)?.integer != 0),
        "TIMER_EXPIRED" => state
            .counters
            .get(&p.first()?.text)
            .is_some_and(|counter| counter.countdown && counter.value < 1),
        "BUILT_BY_PLAYER" => {
            count(
                game,
                state,
                owner(state, &p.get(1)?.text)?,
                &p.first()?.text,
            ) > 0
        }
        "PLAYER_HAS_OBJECT_COMPARISON" => compare(
            count(
                game,
                state,
                owner(state, &p.first()?.text)?,
                &p.get(3)?.text,
            ),
            p.get(1)?.integer,
            i64::from(p.get(2)?.integer),
        ),
        // ScriptConditions::evaluatePlayerHasCredits compares the authored
        // threshold on the left, unlike the object-count comparison above.
        "PLAYER_HAS_CREDITS" => compare(
            i64::from(p.first()?.integer),
            p.get(1)?.integer,
            i64::from(game.players[owner(state, &p.get(2)?.text)?].cash),
        ),
        "PLAYER_HAS_NO_POWER" => game.players[owner(state, &p.first()?.text)?].power < 0,
        "TEAM_HAS_UNITS" => state
            .teams
            .get(team?)?
            .members
            .iter()
            .any(|&id| game.entities[id].alive()),
        _ => return teams::condition(game, rules, state, call, team),
    })
}
// ScriptConditions caches this result on the condition itself, even after capture.
fn tech_within_distance(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    key: (usize, usize, usize),
) -> Option<bool> {
    let Some(owner) = owner(state, &call.parameters.first()?.text) else {
        return Some(false);
    };
    if let Some(&cached) = state.tech_conditions.get(&key) {
        return Some(cached);
    }
    let Some(area) = map::area(rules, state, &call.parameters.get(2)?.text) else {
        return Some(false);
    };
    let (center, radius) = map::bounds(area);
    let radius = radius + call.parameters.get(1)?.real;
    let found = game
        .partition
        .outward_range_rings(center, radius)
        .into_iter()
        .any(|(_, id)| {
            let actor = &game.entities[id];
            let allied = game
                .map_side_relationship(owner, actor.owner)
                .map_or_else(|| game.allied(owner, actor.owner), |relation| relation == 2);
            game.attack_kind(id, "TECH_BUILDING")
                && actor.owner != owner
                && !allied
                && !actor.is_off_map()
                && crate::skirmish::movement::distance_squared(center, actor.position)
                    < radius * radius
        });
    state.tech_conditions.insert(key, found);
    Some(found)
}

fn count(game: &Skirmish, state: &State, owner: usize, name: &str) -> i64 {
    let templates: Vec<_> = game
        .catalog
        .iter()
        .filter(|unit| {
            unit.name == name
                || state
                    .object_lists
                    .get(name)
                    .is_some_and(|list| list.contains(&unit.name))
        })
        .collect();
    i64::try_from(
        game.entities
            .iter()
            .filter(|entity| {
                entity.owner == owner
                    && entity.complete()
                    && templates
                        .iter()
                        .any(|template| game.catalog[entity.type_index].equivalent_to(template))
            })
            .count(),
    )
    .unwrap_or(i64::MAX)
}
const fn compare(value: i64, op: i32, other: i64) -> bool {
    match op {
        0 => value < other,
        1 => value <= other,
        2 => value == other,
        3 => value >= other,
        4 => value > other,
        5 => value != other,
        _ => false,
    }
}
fn owner(state: &State, name: &str) -> Option<usize> {
    match name {
        "<This Player>" => Some(state.owner),
        "<This Player's Enemy>" => Some(state.enemy.unwrap_or(0)),
        _ => None,
    }
}

pub(super) fn execute(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    team: Option<usize>,
) {
    if action(game, rules, state, call, team).is_none() {
        state.report(&call.name);
    }
}
fn action(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    team: Option<usize>,
) -> Option<()> {
    let p = &call.parameters;
    match call.name.as_str() {
        "NO_OP" => {}
        "SET_COUNTER" => {
            counter(state, &p.first()?.text).value = p.get(1)?.integer;
        }
        "INCREMENT_COUNTER" => {
            let counter = counter(state, &p.get(1)?.text);
            counter.value = counter.value.wrapping_add(p.first()?.integer);
        }
        "DECREMENT_COUNTER" => {
            let counter = counter(state, &p.get(1)?.text);
            counter.value = counter.value.wrapping_sub(p.first()?.integer);
        }
        "SET_FLAG" => {
            state
                .flags
                .insert(p.first()?.text.clone(), p.get(1)?.integer != 0);
        }
        "SET_TIMER" | "SET_MILLISECOND_TIMER" | "SET_RANDOM_TIMER" | "SET_RANDOM_MSEC_TIMER" => {
            let value = timer_value(game, call)?;
            let counter = counter(state, &p.first()?.text);
            counter.value = value;
            counter.countdown = true;
        }
        "ENABLE_SCRIPT" | "DISABLE_SCRIPT" => {
            let name = &p.first()?.text;
            if let Some(id) = rules.factions[state.faction]
                .groups
                .iter()
                .position(|group| group.name == *name)
            {
                state.groups[id] = call.name == "ENABLE_SCRIPT";
            }
            if let Some(id) = rules.factions[state.faction]
                .scripts
                .iter()
                .position(|script| script.name == *name)
            {
                state.scripts[id].active = call.name == "ENABLE_SCRIPT";
            }
        }
        "PLAYER_HUNT" => {
            let player = owner(state, &p.first()?.text)?;
            game.set_player_units_should_hunt(player, true, 1);
        }
        "PLAYER_ENABLE_UNIT_CONSTRUCTION" => {
            if owner(state, &p.first()?.text)? == state.owner {
                state.enabled |= 2;
            }
        }
        "PLAYER_ENABLE_BASE_CONSTRUCTION" => {
            if owner(state, &p.first()?.text)? == state.owner {
                state.enabled |= 1;
            }
        }
        "PLAYER_GIVE_MONEY" => {
            let owner = owner(state, &p.first()?.text)?;
            game.players[owner].cash = game.players[owner]
                .cash
                .saturating_add(u32::try_from(p.get(1)?.integer).ok()?);
        }
        "OBJECTLIST_ADDOBJECTTYPE" => {
            let list = state
                .object_lists
                .entry(p.first()?.text.clone())
                .or_default();
            let name = &p.get(1)?.text;
            if !list.contains(name) {
                list.push(name.clone());
            }
        }
        _ => return production_action(game, rules, state, call, team),
    }
    Some(())
}

fn production_action(
    game: &mut Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    call: &AiCall,
    team: Option<usize>,
) -> Option<()> {
    let p = &call.parameters;
    match call.name.as_str() {
        "SKIRMISH_BUILD_BUILDING" => construction::request(state, &p.first()?.text),
        "SKIRMISH_BUILD_BASE_DEFENSE_FRONT" | "SKIRMISH_BUILD_BASE_DEFENSE_FLANK" => {
            let name = &rules.factions[state.faction].defense_template;
            if !defenses::request(game, rules, state, name, call.name.ends_with("FLANK")) {
                return None;
            }
        }
        "SKIRMISH_BUILD_STRUCTURE_FRONT" | "SKIRMISH_BUILD_STRUCTURE_FLANK" => {
            if !defenses::request(
                game,
                rules,
                state,
                &p.first()?.text,
                call.name.ends_with("FLANK"),
            ) {
                return None;
            }
        }
        "SET_ATTACK_PRIORITY_THING" | "SET_DEFAULT_ATTACK_PRIORITY" => {
            priorities::action(game, state, call)?;
        }
        "AI_PLAYER_BUILD_UPGRADE" => {
            if owner(state, &p.first()?.text)? == state.owner {
                let name = &p.get(1)?.text;
                let supported = game.catalog.iter().any(|unit| {
                    unit.maintenance
                        .research
                        .iter()
                        .any(|research| research.name == *name)
                });
                if !supported {
                    return None;
                }
                for building in &state.buildings {
                    let Some(id) = building.object.filter(|&id| game.entities[id].complete())
                    else {
                        continue;
                    };
                    let Some(index) = game.catalog[game.entities[id].type_index]
                        .maintenance
                        .research
                        .iter()
                        .position(|upgrade| upgrade.name == *name && upgrade.player)
                    else {
                        continue;
                    };
                    if game.queue_upgrade(id, index).is_ok() {
                        break;
                    }
                }
            }
        }
        "AI_PLAYER_BUILD_SUPPLY_CENTER" => {
            if owner(state, &p.first()?.text)? == state.owner {
                construction::supply(
                    game,
                    state,
                    &p.get(1)?.text,
                    u32::try_from(p.get(2)?.integer).ok()?,
                );
            }
        }
        "BUILD_TEAM" => {
            let index = rules.factions[state.faction]
                .teams
                .iter()
                .position(|template| template.name == p.first().map_or("", |p| p.text.as_str()))?;
            teams::queue(game, rules, state, index, true);
        }
        "RECRUIT_TEAM" => {
            let name = &p.first()?.text;
            let index = rules.factions[state.faction]
                .teams
                .iter()
                .position(|template| template.name == *name)?;
            let radius = p.get(1)?.real;
            if !radius.is_finite() {
                return None;
            }
            recruitment::recruit(game, rules, state, index, radius);
        }
        _ => return teams::action(game, rules, state, call, team),
    }
    Some(())
}

fn timer_value(game: &mut Skirmish, call: &AiCall) -> Option<i32> {
    let parameters = &call.parameters;
    let low = parameters.get(1)?;
    if matches!(
        call.name.as_str(),
        "SET_MILLISECOND_TIMER" | "SET_RANDOM_MSEC_TIMER"
    ) {
        let seconds = if call.name == "SET_RANDOM_MSEC_TIMER" {
            let high = parameters.get(2)?.real;
            if !low.real.is_finite() || !high.is_finite() {
                return None;
            }
            crate::skirmish::startup::random_real(&mut game.random, low.real, high)
        } else {
            low.real
        };
        // ScriptEngine::setTimer: ceil(ConvertDurationFromMsecsToFrames(value*1000)).
        let milliseconds = seconds * 1000.0;
        let frames = (milliseconds * (30.0 / 1000.0)).ceil();
        if !frames.is_finite() {
            return None;
        }
        format!("{frames:.0}").parse().ok()
    } else if call.name == "SET_RANDOM_TIMER" {
        let high = parameters.get(2)?.integer;
        if low.integer >= high {
            return Some(high);
        }
        let width = u32::try_from(i64::from(high) - i64::from(low.integer)).ok()?;
        i32::try_from(i64::from(low.integer) + i64::from(game.random_between(width))).ok()
    } else {
        Some(low.integer)
    }
}

fn counter<'a>(state: &'a mut State, name: &str) -> &'a mut Counter {
    state.counters.entry(name.to_owned()).or_insert(Counter {
        value: 0,
        countdown: false,
    })
}

#[cfg(test)]
mod credit_tests {
    use super::*;
    use crate::skirmish::{AiParameter, OpponentDifficulty};

    #[test]
    fn credits_compare_original_threshold_against_player_money_for_all_six_operators() {
        let mut game = crate::skirmish::tests::game();
        let rules = super::super::tests::rules(Vec::new());
        let state = State::new(&mut game, &rules, 1, OpponentDifficulty::Hard, 0);
        game.players[1].cash = 50_000;
        for (threshold, expected) in [
            (49_999, [true, true, false, false, false, true]),
            (50_000, [false, true, true, true, false, false]),
            (50_001, [false, false, false, true, true, true]),
        ] {
            for (operator, accepted) in expected.into_iter().enumerate() {
                let parameter = |text: &str, integer| AiParameter {
                    text: text.into(),
                    integer,
                    real: 0.0,
                    position: None,
                };
                let call = AiCall {
                    name: "PLAYER_HAS_CREDITS".into(),
                    parameters: vec![
                        parameter("", threshold),
                        parameter("", i32::try_from(operator).unwrap()),
                        parameter("<This Player>", 0),
                    ],
                };
                assert_eq!(
                    condition(&game, &rules, &state, &call, None),
                    Some(accepted),
                    "threshold {threshold}, operator {operator}, cash 50000"
                );
            }
        }
    }
}
