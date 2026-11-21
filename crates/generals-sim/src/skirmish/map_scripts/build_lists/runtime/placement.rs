//! `AIPlayer::buildStructureWithDozer` for ordinary map NPC players.
pub(in crate::skirmish::map_scripts::build_lists) use crate::skirmish::ai_repair::select_dozer;
use crate::skirmish::{Order, Skirmish, movement::footprints_overlap, visibility::Status};

pub(super) fn build(
    game: &mut Skirmish,
    state: &mut super::super::State,
    type_index: usize,
    requested: [f32; 2],
    angle: f32,
) -> Option<(usize, usize)> {
    let owner = state.owner;
    let (worker, need_dozer) = select_dozer(game, owner, requested, state.repair.worker);
    if need_dozer {
        super::super::work::queue_dozer(game, state);
    }
    let worker = worker?;
    let cost = game.build_cost(owner, type_index);
    if game.players[owner].cash < cost || !legal(game, worker, type_index, requested, angle, false)
    {
        return None;
    }
    let position = if legal(game, worker, type_index, requested, angle, true) {
        requested
    } else {
        wiggle(game, worker, type_index, requested, angle).unwrap_or(requested)
    };
    if !game.build_site_reachable(worker, position) {
        game.entities[worker].position = position;
        game.entities[worker].layer = 1;
        game.observe_ground_partition(worker);
    }
    game.clear_rubble_for_construction(type_index, position, angle);
    let world = [position[0], position[1], game.ground_height(position)];
    game.move_objects_for_construction(owner, type_index, world, angle);
    game.set_ai_order(worker, Order::Idle);
    game.clear_ai_goal(worker);
    game.entities[worker].ai.command.from_ai = true;
    let building = game.spawn(type_index, owner, position, false);
    game.players[owner].cash = game.players[owner].cash.saturating_sub(cost);
    game.set_object_orientation(building, angle);
    game.rebuild_navigation();
    game.entities[building].producer = Some(worker);
    game.entities[building].maintenance.builder = Some(worker);
    if game.entities[worker].ai.dozer.is_some() {
        game.new_dozer_task(worker, crate::skirmish::ai_dispatch::dozer::BUILD, building);
    } else {
        game.set_order(worker, Order::Build(building));
        game.entities[worker].ai.construction_dock = game.construction_approach(worker, building);
    }
    Some((building, worker))
}

fn wiggle(
    game: &Skirmish,
    worker: usize,
    kind: usize,
    center: [f32; 2],
    angle: f32,
) -> Option<[f32; 2]> {
    let mut span = 0.0;
    loop {
        if span.partial_cmp(&100.0) != Some(std::cmp::Ordering::Less) {
            break;
        }
        let offset = span / 2.0;
        let mut along = center[0] - offset;
        loop {
            if !matches!(
                along.partial_cmp(&(center[0] + offset)),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ) {
                break;
            }
            for y in [center[1] - offset, center[1] - offset + span] {
                let position = [along, y];
                if legal(game, worker, kind, position, angle, true) {
                    return Some(position);
                }
            }
            along += 10.0;
        }
        along = center[1] - offset;
        loop {
            if !matches!(
                along.partial_cmp(&(center[1] + offset)),
                Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            ) {
                break;
            }
            for x in [center[0] - offset, center[0] - offset + span] {
                let position = [x, along];
                if legal(game, worker, kind, position, angle, true) {
                    return Some(position);
                }
            }
            along += 10.0;
        }
        span += 20.0;
    }
    None
}

fn legal(
    game: &Skirmish,
    worker: usize,
    kind: usize,
    position: [f32; 2],
    angle: f32,
    terrain: bool,
) -> bool {
    if game.cell_at(position).is_none() {
        return false;
    }
    let unit = &game.catalog[kind];
    let footprint = unit.footprint(position, angle);
    for (id, actor) in game.entities.iter().enumerate() {
        let other = &game.catalog[actor.type_index];
        if actor.masked()
            || !actor.alive()
            || other.kind_of.iter().any(|kind| {
                matches!(
                    kind.as_str(),
                    "INERT" | "MINE" | "SHRUBBERY" | "CLEARED_BY_BUILD"
                )
            })
            || !footprints_overlap(&footprint, &other.footprint(actor.position, actor.facing))
        {
            continue;
        }
        let relationship = game.object_relationship(worker, id);
        let immobile = other.kind_of.iter().any(|kind| kind == "IMMOBILE");
        if !terrain && immobile && relationship != 0 {
            continue;
        }
        if matches!(
            game.visibility_status(game.entities[worker].owner, id),
            Status::Fogged | Status::Shrouded
        ) || (relationship == 2 && game.object_busy(id))
            || immobile
            || game.object_disabled(id)
            || relationship == 0
        {
            return false;
        }
    }
    if unit.avoid_supply() && game.too_close_to_supply(&footprint) {
        return false;
    }
    if terrain {
        game.exit_areas_clear(kind, position, angle, false)
            && game.build_site_reachable(worker, position)
            && game.native_build_terrain_clear(
                kind,
                [position[0], position[1], game.ground_height(position)],
                angle,
            )
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_dozer_lookup_retains_dead_objects_until_deletion_and_requires_the_interface() {
        let mut game = super::super::super::tests::game(Vec::new());
        let owner = game.map_side_state.owners["NPC"];
        let worker = game.spawn(1, owner, [450.0, 200.0], true);
        game.entities[worker].health = 0.0;
        game.entities[worker].maintenance.masked = true;
        assert_eq!(
            super::select_dozer(&game, owner, [500.0, 200.0], None),
            (Some(worker), false)
        );
        // A mobile ordinary AI bearing the DOZER flag has no Dozer interface.
        game.catalog[5].kind_of.push("DOZER".into());
        let ordinary = game.spawn(5, owner, [495.0, 200.0], true);
        assert!(game.entities[ordinary].ai.registration.is_some());
        assert!(game.entities[ordinary].ai.dozer.is_none());
        game.destroy_maintenance_object(worker);
        assert_eq!(
            super::select_dozer(&game, owner, [500.0, 200.0], None),
            (Some(worker), false)
        );
        game.process_update_destruction();
        assert_eq!(
            super::select_dozer(&game, owner, [500.0, 200.0], None),
            (None, true)
        );
    }
}
