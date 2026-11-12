//! Native retained-civilian `AIPlayer` supply-center quotas and free collector callbacks.
use super::{State, work};
use crate::skirmish::{
    Skirmish, UnitAiUpdate,
    economy::DockTarget,
    movement::{bounding_radius, distance_squared},
};

pub(super) struct Quota {
    desired: i32,
    current: i32,
}

pub(super) fn quota(game: &Skirmish, id: usize) -> Option<Quota> {
    let unit = &game.catalog[game.entities[id].type_index];
    // ScriptEngine global difficulty defaults to NORMAL for the admitted map
    // scripts; AIPlayer captures it when created, independent of playable AI.
    let desired = game
        .layout
        .map_sides
        .iter()
        .find(|side| game.map_side_state.owners.get(&side.name) == Some(&game.entities[id].owner))
        .map_or(0, |side| side.gatherers[1]);
    (unit.supply.dock.is_some() && unit.supply.capacity == 0).then_some(Quota {
        desired: desired.wrapping_add(1),
        current: -1,
    })
}

pub(super) fn queue(game: &mut Skirmish, state: &mut State) {
    if state.work.iter().any(|order| order.resource_gatherer) {
        return;
    }
    let total = game
        .entities
        .iter()
        .filter(|actor| {
            actor.exists() && actor.owner == state.owner && collector(game, actor.type_index)
        })
        .count();
    for index in 0..state.records.len() {
        let Some(quota) = &state.records[index].supply else {
            continue;
        };
        let (desired, current) = (quota.desired, quota.current);
        if current >= desired {
            recount(game, state, index);
            continue;
        }
        if let Some(center) = state.records[index]
            .object
            .filter(|&id| game.entities[id].exists())
            && let Some(id) = loose(game, state.owner)
        {
            state.records[index]
                .supply
                .as_mut()
                .expect("retained supply quota")
                .current += 1;
            dock(game, id, center);
            return;
        }
        if i64::try_from(total).unwrap_or(i64::MAX) >= i64::from(desired.wrapping_mul(3)) {
            continue;
        }
        queue_one(game, state, index);
    }
}

fn queue_one(game: &mut Skirmish, state: &mut State, index: usize) {
    let allowed = game.map_build_allowed(state.owner, true);
    game.set_map_build_allowed(state.owner, true, true);
    for template in &game.layout.map_template_order {
        let Some(type_index) = game
            .catalog
            .iter()
            .position(|unit| unit.name == template.name)
        else {
            continue;
        };
        if !template.harvester {
            continue;
        }
        let Some(producer) = work::factory(game, state, type_index, false) else {
            continue;
        };
        let quota = state.records[index]
            .supply
            .as_mut()
            .expect("retained supply quota");
        let producer = if quota.current == -1 {
            quota.current = 0;
            Some(producer)
        } else {
            None
        };
        state.work.insert(
            0,
            work::WorkOrder {
                type_index,
                producer,
                complete: false,
                resource_gatherer: true,
            },
        );
        state.team_delay = 0;
        if producer.is_none() {
            work::start_training(game, state, 0);
        }
        break;
    }
    game.set_map_build_allowed(state.owner, true, allowed);
}

fn collector(game: &Skirmish, kind: usize) -> bool {
    let unit = &game.catalog[kind];
    unit.kind_of.iter().any(|flag| flag == "HARVESTER")
        && unit.supply.capacity > 0
        && matches!(
            unit.ai.update,
            UnitAiUpdate::Worker | UnitAiUpdate::SupplyTruck
        )
}

fn loose(game: &Skirmish, owner: usize) -> Option<usize> {
    game.entities
        .iter()
        .enumerate()
        .rev()
        .find_map(|(id, actor)| {
            (actor.exists()
                && actor.owner == owner
                && collector(game, actor.type_index)
                && !dock_exists(game, actor.ai.supply.preferred_dock)
                && (actor.ai.supply.ferrying() || actor.ai.supply.force_wanting))
                .then_some(id)
        })
}

fn recount(game: &mut Skirmish, state: &mut State, index: usize) {
    let Some(center) = state.records[index].object.filter(|&id| {
        game.entities[id].exists()
            && !game.catalog[game.entities[id].type_index]
                .kind_of
                .iter()
                .any(|flag| flag == "REBUILD_HOLE")
    }) else {
        return;
    };
    let actor = &game.entities[center];
    let radius = 200.0 + game.object_radius(center);
    let closest = game
        .supplies
        .iter()
        .enumerate()
        .filter(|(index, source)| {
            game.map_supply_owner(*index) != state.owner && (game.entities[source.object].exists())
        })
        .map(|(index, source)| {
            (
                index,
                (distance_squared(actor.position, game.entities[source.object].position).sqrt()
                    - bounding_radius(&game.object_footprint(source.object)))
                .max(0.0),
            )
        })
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|(_, first), (_, second)| first.total_cmp(second));
    let Some((source, _)) = closest else {
        return;
    };
    if game.supplies[source].boxes == 0 || game.enemies(state.owner, game.map_supply_owner(source))
    {
        return;
    }
    let Some(mut quota) = quota(game, center) else {
        return;
    };
    quota.current = 0;
    for id in (0..game.entities.len()).rev() {
        let actor = &game.entities[id];
        if actor.exists()
            && actor.owner == state.owner
            && collector(game, actor.type_index)
            && actor.ai.supply.preferred_dock == Some(DockTarget::Center(center))
        {
            quota.current += 1;
            if !actor.ai.supply.ferrying() {
                dock(game, id, center);
            }
        }
    }
    state.records[index].supply = Some(quota);
}

pub(super) fn assign(game: &mut Skirmish, state: &mut State, unit: usize) {
    // Native intentionally does not break after assigning a deficient center.
    for record in &mut state.records {
        let Some(quota) = &mut record.supply else {
            continue;
        };
        if quota.desired <= 0 || quota.desired <= quota.current {
            continue;
        }
        if let Some(center) = record.object.filter(|&id| game.entities[id].exists()) {
            quota.current += 1;
            dock(game, unit, center);
        }
    }
}

fn dock(game: &mut Skirmish, id: usize, center: usize) {
    game.ai_dock(
        id,
        DockTarget::Center(center),
        crate::skirmish::ai_goals::COMMAND_FROM_PLAYER,
    );
}

fn dock_exists(game: &Skirmish, target: Option<DockTarget>) -> bool {
    match target {
        Some(DockTarget::Center(id) | DockTarget::Repair(id)) => game
            .entities
            .get(id)
            .is_some_and(crate::skirmish::Entity::exists),
        Some(DockTarget::Warehouse(id)) => game
            .supplies
            .get(id)
            .is_some_and(|source| game.entities[source.object].exists()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_civilian_collector_is_matched_without_charging_a_second_unit() {
        let mut game = super::super::tests::game(Vec::new());
        let owner = game.map_side_state.owners["NPC"];
        game.catalog[1].kind_of.push("HARVESTER".into());
        game.layout
            .map_template_order
            .iter_mut()
            .find(|template| template.name == game.catalog[1].name)
            .unwrap()
            .harvester = true;
        game.catalog[2].production = Some(Vec::new());
        game.catalog[2].buildable_objects = vec![game.catalog[1].name.clone()];
        let position = game.supply_position(0);
        let center = game.spawn(2, owner, [position[0] + 50.0, position[1]], true);
        let mut states = std::mem::take(&mut game.map_scripts.npc_builds);
        let state = &mut states[0];
        state.records[0].object = Some(center);
        state.records[0].supply = quota(&game, center);
        game.set_map_build_allowed(owner, true, true);
        let cash = game.players[owner].cash;
        queue(&mut game, state);
        assert_eq!(state.work.len(), 1);
        assert_eq!(state.work[0].producer, Some(center));
        assert!(state.work[0].resource_gatherer);
        assert!(game.entities[center].queue.is_empty());
        assert_eq!(game.players[owner].cash, cash);
        game.map_scripts.npc_builds = states;
        let worker = game.spawn(1, owner, position, true);
        game.map_unit_produced(Some(center), worker);
        assert!(game.entities[worker].ai.supply.force_wanting);
        assert!(game.entities[worker].ai.supply.preferred_dock == Some(DockTarget::Center(center)));
        let state = &game.map_scripts.npc_builds[0];
        assert_eq!(state.records[0].supply.as_ref().unwrap().current, 1);
        assert!(state.work[0].complete);
    }
}
