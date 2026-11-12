//! `AIPlayer::queueSupplyTruck` and the resource branch of `onUnitProduced`.
use super::{OpponentRules, State, SupplyQuota, Team, WorkOrder, teams};
use crate::skirmish::{
    Skirmish,
    economy::DockTarget,
    movement::{bounding_radius, distance_squared},
};

pub(super) fn queue(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    if state.build_queue.iter().any(|&index| {
        state.requests[index]
            .orders
            .iter()
            .any(|order| order.resource_gatherer)
    }) {
        return;
    }
    let total = game
        .entities
        .iter()
        .filter(|actor| {
            actor.alive()
                && actor.owner == state.owner
                && game.catalog[actor.type_index].supply.capacity > 0
        })
        .count();
    for index in 0..state.buildings.len() {
        let Some(quota) = &state.buildings[index].supply else {
            continue;
        };
        if i64::from(quota.current) >= i64::from(quota.desired) {
            recount(game, rules, state, index);
            continue;
        }
        let center = state.buildings[index]
            .object
            .filter(|&id| game.entities[id].alive());
        if let Some(center) = center {
            let loose = game.entities.iter().position(|actor| {
                actor.alive()
                    && actor.owner == state.owner
                    && game.catalog[actor.type_index].supply.capacity > 0
                    && !dock_exists(game, actor.ai.supply.preferred_dock)
                    && (actor.ai.supply.ferrying() || actor.ai.supply.force_wanting)
            });
            if let Some(id) = loose {
                state.buildings[index]
                    .supply
                    .as_mut()
                    .expect("supply quota")
                    .current += 1;
                dock(game, id, center);
                return;
            }
        }
        let quota = state.buildings[index]
            .supply
            .as_ref()
            .expect("supply quota");
        if u64::try_from(total).unwrap_or(u64::MAX) >= u64::from(quota.desired) * 3 {
            continue;
        }
        let candidate = game.catalog.iter().enumerate().find_map(|(kind, unit)| {
            (unit.supply.capacity > 0)
                .then(|| teams::factory(game, state, kind, false).map(|factory| (kind, factory)))
                .flatten()
        });
        let Some((type_index, producer)) = candidate else {
            continue;
        };
        let producer = if quota.current == -1 {
            // The first order waits for SpawnBehavior's free collector callback.
            state.buildings[index]
                .supply
                .as_mut()
                .expect("supply quota")
                .current = 0;
            Some(producer)
        } else {
            game.queue_unit(producer, type_index)
                .ok()
                .map(|()| producer)
        };
        let team = state.teams.len();
        state.teams.push(Team::new(None, game.tick, 0));
        state.queue_request(
            team,
            vec![WorkOrder {
                type_index,
                remaining: 1,
                producer,
                required: true,
                resource_gatherer: true,
            }],
            game.tick,
            true,
        );
    }
}

fn recount(game: &mut Skirmish, rules: &OpponentRules, state: &mut State, index: usize) {
    let Some(center) = state.buildings[index]
        .object
        .filter(|&id| game.entities[id].alive())
    else {
        return;
    };
    let actor = &game.entities[center];
    let radius = 200.0
        + bounding_radius(&game.catalog[actor.type_index].footprint(actor.position, actor.facing));
    let closest = game
        .supplies
        .iter()
        .filter(|source| game.entities[source.object].exists())
        .map(|source| {
            (
                source,
                (distance_squared(actor.position, game.entities[source.object].position).sqrt()
                    - bounding_radius(&game.object_footprint(source.object)))
                .max(0.0),
            )
        })
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|(_, left), (_, right)| left.total_cmp(right));
    if closest.is_none_or(|(source, _)| source.boxes == 0) {
        return;
    }
    let mut current = 0;
    for id in 0..game.entities.len() {
        let actor = &game.entities[id];
        if actor.alive()
            && actor.owner == state.owner
            && game.catalog[actor.type_index].supply.capacity > 0
            && actor.ai.supply.preferred_dock == Some(DockTarget::Center(center))
        {
            current += 1;
            if !actor.ai.supply.ferrying() {
                dock(game, id, center);
            }
        }
    }
    state.buildings[index].supply = Some(SupplyQuota {
        desired: rules.factions[state.faction].gatherers[state.difficulty_index()] + 1,
        current,
    });
}

pub(super) fn assign_produced(game: &mut Skirmish, state: &mut State, unit: usize) {
    // Native onUnitProduced intentionally has no break after assigning a center.
    for entry in &mut state.buildings {
        let Some(quota) = &mut entry.supply else {
            continue;
        };
        if quota.desired == 0 || i64::from(quota.desired) <= i64::from(quota.current) {
            continue;
        }
        if let Some(center) = entry.object.filter(|&id| game.entities[id].alive()) {
            quota.current += 1;
            dock(game, unit, center);
        }
    }
}

fn dock(game: &mut Skirmish, unit: usize, center: usize) {
    // CMD_FROM_PLAYER makes the center sticky even for an empty gatherer.
    game.ai_dock(
        unit,
        DockTarget::Center(center),
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
    );
}

fn dock_exists(game: &Skirmish, target: Option<DockTarget>) -> bool {
    match target {
        Some(DockTarget::Center(id) | DockTarget::Repair(id)) => game
            .entities
            .get(id)
            .is_some_and(crate::skirmish::Entity::alive),
        Some(DockTarget::Warehouse(id)) => game
            .supplies
            .get(id)
            .is_some_and(|source| game.entities[source.object].exists()),
        None => false,
    }
}
