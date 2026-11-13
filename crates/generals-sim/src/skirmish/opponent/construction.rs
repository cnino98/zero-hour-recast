use super::{Building, OpponentFaction, OpponentRules, State, adjusted_timer};
use crate::skirmish::{
    Kind, Order, Skirmish,
    movement::{bounding_radius, distance_squared},
};

pub(super) fn initial_buildings(
    game: &mut Skirmish,
    owner: usize,
    data: &OpponentFaction,
) -> Vec<Building> {
    // AISkirmishPlayer::adjustBuildList: RotateSkirmishBases=No still applies
    // the unconditional 3*pi/4 template-coordinate rotation; building angles stay unchanged.
    let origin = data
        .buildings
        .first()
        .map_or([0.0; 2], |entry| entry.position);
    let center = game
        .player_start(owner)
        .map_or([0.0; 2], |start| start.position);
    let (sin, cos) = (3.0 * std::f32::consts::FRAC_PI_4).sin_cos();
    let buildings = data
        .buildings
        .iter()
        .map(|entry| {
            let x = entry.position[0] - origin[0];
            let y = entry.position[1] - origin[1];
            let left = x * cos;
            let right = y * sin;
            let bottom = x * sin;
            let top = y * cos;
            let position = [left - right + center[0], bottom + top + center[1]];
            let object = game.entities.iter().position(|entity| {
                entity.owner == owner
                    && game.catalog[entity.type_index].name == entry.name
                    && game.catalog[entity.type_index].kind == Kind::CommandCenter
            });
            if let Some(id) = object {
                game.set_object_orientation(id, entry.angle);
            }
            Building {
                name: entry.name.clone(),
                position,
                angle: entry.angle,
                remaining: if entry.rebuilds < 0 {
                    entry.rebuilds
                } else {
                    entry.rebuilds + 1
                },
                automatic: entry.automatic,
                priority: false,
                object,
                destroyed_at: None,
                supply: None,
            }
        })
        .collect();
    // adjustBuildList changes the initial command center's source angle.
    game.rebuild_navigation();
    buildings
}

pub(super) fn request(state: &mut State, name: &str) {
    // AISkirmishPlayer::buildSpecificAIBuilding marks the first unbuilt, unqueued entry.
    if let Some(entry) = state
        .buildings
        .iter_mut()
        .find(|entry| entry.name == name && entry.object.is_none() && !entry.priority)
    {
        entry.priority = true;
        state.build_delay = 0;
    }
}

pub(super) fn update(game: &mut Skirmish, rules: &OpponentRules, state: &mut State) {
    if state.enabled & 1 == 0 {
        return;
    }
    state.structure_timer = (state.structure_timer - 1.0).min(90.0);
    state.build_delay -= 1;
    if state.build_delay > 0 {
        return;
    }
    state.build_delay = 60;
    if state.structure_timer > 0.0 {
        return;
    }
    maintain(game, state);
    let underpowered = game.players[state.owner].power < 0;
    let power_building = state.buildings.iter().filter_map(|b| b.object).any(|id| {
        game.entities[id].alive()
            && !game.entities[id].complete()
            && game.catalog[game.entities[id].type_index].kind == Kind::PowerPlant
    });
    let mut candidate = None;
    let mut power = None;
    for (index, entry) in state.buildings.iter().enumerate() {
        if entry.object.is_some()
            || entry.remaining == 0
            || entry.destroyed_at.is_some_and(|frame| {
                frame + u64::from(rules.rebuild_delay_seconds) * 30 > game.tick
            })
        {
            continue;
        }
        let Some(type_index) = game.catalog.iter().position(|unit| unit.name == entry.name) else {
            continue;
        };
        if !safe(
            game,
            state.owner,
            type_index,
            entry.position,
            rules.supply_center_safe_radius,
        ) {
            continue;
        }
        if entry.priority && candidate.is_none_or(|old: usize| !state.buildings[old].priority) {
            candidate = Some(index);
        }
        if power.is_none()
            && game.catalog[type_index].kind == Kind::PowerPlant
            && (underpowered || entry.automatic)
        {
            power = Some(index);
        }
        if candidate.is_none() && entry.automatic {
            candidate = Some(index);
        }
    }
    if power.is_some() && !power_building {
        candidate = power;
    }
    if let Some(index) = candidate {
        let entry = &state.buildings[index];
        let Some(type_index) = game.catalog.iter().position(|unit| unit.name == entry.name) else {
            return;
        };
        let Some(worker) = find_builder(game, state.owner, entry.position, state.repair.worker)
        else {
            return;
        };
        if let Some(position) =
            build_position(game, worker, type_index, entry.position, entry.angle)
            && let Ok(id) = game.build_at_angle(worker, type_index, position, entry.angle, true)
        {
            let entry = &mut state.buildings[index];
            entry.object = Some(id);
            entry.destroyed_at = None;
            if entry.remaining > 0 {
                entry.remaining -= 1;
            }
            state.structure_timer = adjusted_timer(
                rules.structure_seconds,
                game.players[state.owner].cash,
                rules,
                true,
            );
        }
    }
}

fn maintain(game: &mut Skirmish, state: &mut State) {
    for entry in &mut state.buildings {
        if let Some(id) = entry.object {
            if !game.entities[id].alive() || game.entities[id].owner != state.owner {
                entry.object = None;
                entry.destroyed_at = Some(game.tick + 1);
            } else if !game.entities[id].complete() {
                let working = game
                    .entities
                    .iter()
                    .any(|entity| entity.alive() && entity.build_task_target() == Some(id));
                if !working
                    && let Some(worker) =
                        find_builder(game, state.owner, entry.position, state.repair.worker)
                {
                    let _ = game.resume_build(worker, id);
                }
            }
        }
    }
}
fn find_builder(
    game: &Skirmish,
    owner: usize,
    position: [f32; 2],
    repair_worker: Option<usize>,
) -> Option<usize> {
    game.entities
        .iter()
        .enumerate()
        .filter(|(id, e)| {
            Some(*id) != repair_worker
                && e.owner == owner
                && e.complete()
                && game.catalog[e.type_index].kind == Kind::Builder
                && e.build_task_target().is_none()
                && !matches!(
                    e.ai.order,
                    Order::Harvest(_) | Order::ReturnSupply | Order::Exit(_)
                )
        })
        .min_by(|(_, a), (_, b)| {
            distance_squared(a.position, position)
                .total_cmp(&distance_squared(b.position, position))
        })
        .map(|(id, _)| id)
}
fn safe(
    game: &Skirmish,
    owner: usize,
    type_index: usize,
    position: [f32; 2],
    safe_radius: f32,
) -> bool {
    // AIPlayer::isLocationSafe excludes allies AND neutral objects. Its range
    // and PartitionManager's FROM_BOUNDINGSPHERE_2D both use bounding circles.
    let radius = safe_radius + bounding_radius(&game.catalog[type_index].footprint(position, 0.0));
    !game.entities.iter().any(|e| {
        if !game.enemies(owner, e.owner) || !e.alive() {
            return false;
        }
        let unit = &game.catalog[e.type_index];
        if matches!(unit.kind, Kind::Builder | Kind::Harvester) {
            return false;
        }
        let other_radius = bounding_radius(&unit.footprint(e.position, e.facing));
        let distance = (distance_squared(e.position, position).sqrt() - other_radius).max(0.0);
        distance * distance < radius * radius
    })
}
fn build_position(
    game: &Skirmish,
    worker: usize,
    type_index: usize,
    position: [f32; 2],
    angle: f32,
) -> Option<[f32; 2]> {
    let valid = |point| {
        game.can_place_at_angle(type_index, point, angle, true)
            && game.build_site_reachable(worker, point)
    };
    if valid(position) {
        return Some(position);
    }
    // AIPlayer::buildStructureWithDozer skirmish branch: the increment inside
    // the original loop produces spans 20,60,...1180 and 20-unit edge spacing.
    for span in (20_u16..1200).step_by(40) {
        let half = f32::from(span) * 0.5;
        for vertical in [false, true] {
            for along in (10..=span).step_by(20) {
                let along = f32::from(along);
                let points = if vertical {
                    [
                        [position[0] - half, position[1] - half + along],
                        [position[0] + half, position[1] - half + along],
                    ]
                } else {
                    [
                        [position[0] - half + along, position[1] - half],
                        [position[0] - half + along, position[1] + half],
                    ]
                };
                for point in points {
                    if valid(point) {
                        return Some(point);
                    }
                }
            }
        }
    }
    None
}

pub(super) fn supply(game: &Skirmish, state: &mut State, name: &str, mut minimum: u32) {
    let Some(type_index) = game
        .catalog
        .iter()
        .position(|unit| unit.name == name && unit.kind == Kind::SupplyCenter)
    else {
        state.report("AI_PLAYER_BUILD_SUPPLY_CENTER: unsupported non-supply template");
        return;
    };
    let center = game.layout.starts[state.start].position;
    let enemy = game.layout.starts[state.enemy_start].position;
    let mut best = None;
    loop {
        for (id, depot) in game.supplies.iter().enumerate() {
            if depot.boxes.saturating_mul(game.rules.supply_value) < minimum {
                continue;
            }
            let mine = distance_squared(center, game.entities[depot.object].position);
            let theirs = distance_squared(enemy, game.entities[depot.object].position);
            if mine * 0.4 > theirs * 0.6 {
                continue;
            }
            if game.entities.iter().any(|entity| {
                entity.owner == state.owner
                    && entity.alive()
                    && game.catalog[entity.type_index].kind == Kind::SupplyCenter
                    && distance_squared(entity.position, game.entities[depot.object].position)
                        .sqrt()
                        < 200.0
                            + game.catalog[game.entities[depot.object].type_index]
                                .geometry
                                .major
                            + game.catalog[entity.type_index].geometry.major
            }) {
                continue;
            }
            if best
                .is_none_or(|old: usize| mine < distance_squared(center, game.supply_position(old)))
            {
                best = Some(id);
            }
        }
        if best.is_some() || minimum <= 200 {
            break;
        }
        minimum /= 2;
    }
    let Some(id) = best else {
        return;
    };
    let position = game.supply_position(id);
    let delta = [position[0] - center[0], position[1] - center[1]];
    let length = delta[0].hypot(delta[1]);
    let point = if length > 0.0 {
        let offset = [delta[0] / length * 30.0, delta[1] / length * 30.0];
        [position[0] - offset[0], position[1] - offset[1]]
    } else {
        position
    };
    let angle = game.catalog[type_index].placement_angle;
    let point = supply_position(game, type_index, point, angle);
    // Player::addToPriorityBuildList prepends after the supply-placement scan.
    state.buildings.insert(
        0,
        Building {
            name: name.to_owned(),
            position: point,
            angle,
            remaining: 1,
            automatic: false,
            priority: true,
            object: None,
            destroyed_at: None,
            supply: None,
        },
    );
    state.build_delay = 0;
}

fn supply_position(game: &Skirmish, type_index: usize, position: [f32; 2], angle: f32) -> [f32; 2] {
    // AIPlayer.cpp::buildBySupplies, lines 1929–2009: test NO_OBJECT_OVERLAP
    // first, then scan spans 0,20,...380 with 10-unit edges. The source's null
    // builder means CLEAR_PATH is inactive; terrain restrictions apply in the scan.
    if game.placement_objects_clear(type_index, position, angle, true) {
        return position;
    }
    for span in (0_u16..400).step_by(20) {
        let half = f32::from(span) / 2.0;
        for vertical in [false, true] {
            for along in (0..=span).step_by(10) {
                let along = f32::from(along);
                let points = if vertical {
                    [
                        [position[0] - half, position[1] - half + along],
                        [position[0] + half, position[1] - half + along],
                    ]
                } else {
                    [
                        [position[0] - half + along, position[1] - half],
                        [position[0] - half + along, position[1] + half],
                    ]
                };
                for point in points {
                    if game.can_place_at_angle(type_index, point, angle, true) {
                        return point;
                    }
                }
            }
        }
    }
    // The original enqueues its initial candidate when the fine scan fails;
    // buildStructureWithDozer performs the later ordinary skirmish search.
    position
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_geometry::Shape;

    #[test]
    fn construction_safety_ignores_neutral_and_allied_structures() {
        let mut game = crate::skirmish::tests::game();
        let factory = game.type_for(0, Kind::Factory).unwrap();
        let point = [300.0, 100.0];
        let object = game.spawn(factory, game.neutral_owner(), point, true);
        assert!(safe(&game, 0, factory, point, 10.0));
        game.entities[object].owner = 1;
        game.slots[0].team = Some(0);
        game.slots[1].team = Some(0);
        assert!(safe(&game, 0, factory, point, 10.0));
        game.slots[1].team = None;
        assert!(!safe(&game, 0, factory, point, 10.0));
    }

    #[test]
    fn safety_uses_both_box_bounding_circles_and_strict_source_radius() {
        let mut game = crate::skirmish::tests::game();
        let factory = game.type_for(0, Kind::Factory).unwrap();
        let tank = game.type_for(1, Kind::Tank).unwrap();
        game.catalog[factory].geometry.shape = Shape::Box;
        game.catalog[factory].geometry.major = 12.0;
        game.catalog[factory].geometry.minor = 5.0;
        game.catalog[tank].geometry.shape = Shape::Box;
        game.catalog[tank].geometry.major = 3.0;
        game.catalog[tank].geometry.minor = 4.0;
        let enemy = game.spawn(tank, 1, [328.0, 100.0], true);
        // Query circle13 + safe10 + enemy circle5: the exact28 boundary is safe.
        assert!(safe(&game, 0, factory, [300.0, 100.0], 10.0));
        game.entities[enemy].position[0] = 327.5;
        assert!(!safe(&game, 0, factory, [300.0, 100.0], 10.0));
    }
}
