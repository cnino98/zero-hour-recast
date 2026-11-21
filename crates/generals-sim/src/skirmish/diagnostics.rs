//! Read-only performance trace shared by fixture and installed-map workloads.
//!
//! Records exact movement, orders, weapons, economy and RNG; this is not a save
//! format or a claim to hash every private engine state. Call outside timers.

use super::{Order, Skirmish};
use std::hash::{DefaultHasher, Hash};

/// Read-only path-service context; these counters are not CPU timing measurements.
pub struct NavigationTickContext {
    pub zone_recalculate_at: Option<u64>,
    /// Retained service counter. The full-zone-rebuild branch returns before
    /// resetting it, so this can describe the preceding search tick instead.
    pub cumulative_cells: usize,
}

#[must_use]
pub const fn navigation_tick_context(game: &Skirmish) -> NavigationTickContext {
    NavigationTickContext {
        zone_recalculate_at: game.path_service.zone_recalculate_at,
        cumulative_cells: game.path_service.cumulative_cells,
    }
}

/// Completed queued ground search, retained after the movement state exits.
pub struct GroundRouteResult {
    pub requested: [f32; 2],
    /// `None` means no route; a different endpoint can be a valid closest path.
    pub endpoint: Option<[f32; 2]>,
    /// Layers in the actual currently retained raw path, including repeated nodes.
    pub active_layers: Vec<u8>,
}

#[must_use]
pub fn ground_route_result(game: &Skirmish, id: usize) -> Option<GroundRouteResult> {
    let actor = game.entities.get(id)?;
    let state = &actor.ai.path_request;
    Some(GroundRouteResult {
        requested: state.last_goal?,
        endpoint: state.last_endpoint,
        active_layers: actor.ai.path.raw.iter().map(|node| node.layer).collect(),
    })
}

/// Current source weapon state and target eligibility for an attack timeout.
#[must_use]
pub fn weapon_details(game: &Skirmish, id: usize, target: usize) -> String {
    let actor = &game.entities[id];
    let unit = &game.catalog[actor.type_index];
    let weapon = actor.active_weapon(unit);
    let state = &actor.weapon_set.weapons[actor.weapon_slot()];
    let mut origin = game.object_position(id);
    origin[2] += unit.geometry_bounds.max_height;
    let terrain_clear = game
        .projectile_terrain
        .as_ref()
        .is_none_or(|terrain| terrain.line_of_sight(origin, game.target_center(target)));
    let mut obstacles = std::collections::BTreeSet::new();
    if let (Some(start), Some(end)) = (
        game.cell_at(actor.position),
        game.cell_at(game.entities[target].position),
    ) {
        let layer = if game.entities[target].layer == 1 {
            actor.layer
        } else {
            game.entities[target].layer
        };
        for cell in game.grid.line_cells(start, end) {
            if let Some(owner) = game.attack_obstacle(layer, cell) {
                use super::movement::obstacles::OwnerId;
                let identity = match owner.id {
                    OwnerId::Actor(other) => format!(
                        "Actor({other},{},{:?})",
                        game.catalog[game.entities[other].type_index].name,
                        game.object_position(other)
                    ),
                    OwnerId::Static(other) => format!("Static({other})"),
                    OwnerId::Fence(other) => format!("Fence({other})"),
                    OwnerId::Neutral(other) => format!("Neutral({other})"),
                };
                obstacles.insert(format!("{identity} transparent={}", owner.transparent));
            }
        }
    }
    format!(
        "order={} facing={} slot={} weapon={} range={} clip={} rounds={} cooldown={} retained_status={} next_shot={} aiming={} max_shots={:?} target_reachable={} target_pitch={} in_range={} attack_out_of_range={} attack_view_blocked={} target_position={:?} target_center={:?} terrain_clear={terrain_clear} ray_obstacles={obstacles:?} turret={:?}",
        order_details(actor.ai.order),
        actor.facing,
        actor.weapon_slot(),
        weapon.name,
        game.effective_weapon_range(id),
        weapon.clip_size,
        state.clip_remaining,
        state.cooldown,
        match state.status {
            super::ai_dispatch::WeaponStatus::PreAttack => "PRE_ATTACK",
            super::ai_dispatch::WeaponStatus::Ready => "READY_TO_FIRE",
            super::ai_dispatch::WeaponStatus::OutOfAmmo => "OUT_OF_AMMO",
            super::ai_dispatch::WeaponStatus::Reloading => "RELOADING_CLIP",
            super::ai_dispatch::WeaponStatus::BetweenShots => "BETWEEN_FIRING_SHOTS",
        },
        state.possible_next_shot,
        state.aiming,
        state.max_shots,
        game.weapon_target_reachable(id, target),
        game.within_target_pitch(id, target, weapon),
        game.within_attack_range(id, target),
        game.attack_out_of_range(id, super::combat::attack::Goal::Object(target)),
        game.attack_view_blocked(
            id,
            Some(target),
            game.object_position(id),
            game.object_position(target)
        ),
        game.object_position(target),
        game.target_center(target),
        unit.turret.as_ref().map(|rules| (
            rules.controlled_slots,
            rules.turn_rate,
            actor.ai.turret.angle
        )),
    )
}

/// Read-only locomotor pose and retained sampler output; never advances the path cache.
#[must_use]
pub fn movement_sample(game: &Skirmish, id: usize) -> String {
    let actor = &game.entities[id];
    let mut blockers = Vec::new();
    let position = game.object_position(id);
    let ground_height = game.ground_height(actor.position);
    let layer_height = game.movement_layer_height(actor.layer, actor.position);
    let unit = &game.catalog[actor.type_index];
    let (radius, centered) =
        super::startup::radius_and_center(&unit.footprint(actor.position, 0.0));
    let ignored = game.ignored_navigation(id);
    let navigation = ignored.as_ref().unwrap_or_else(|| game.navigation_for(id));
    let sample = actor.ai.ground_path.as_ref().map(|path| {
        path.sampling_details(position, |layer, from, to| {
            let passable = super::ground_movement::line_passable_on(
                navigation,
                |node| game.fixed_unit_blocks_on(id, node),
                radius,
                centered,
                from,
                to,
                layer,
            );
            if !passable {
                blockers.push((layer, to, game.ground_line_details(id, from, to, layer)));
            }
            passable
        })
    });
    format!(
        "position={position:?} layer={} ground_height={ground_height} layer_height={layer_height} facing={} velocity={:?} sample={sample:?} blockers={blockers:?}",
        actor.layer, actor.facing, actor.ai.motion.velocity
    )
}

/// Read-only retained movement state for a timed-out installed command check.
#[must_use]
pub fn movement_details(game: &Skirmish, id: usize) -> String {
    let actor = &game.entities[id];
    let raw_route: Vec<_> = actor
        .ai
        .path
        .raw
        .iter()
        .map(|node| (node.cell.x, node.cell.y, node.layer))
        .collect();
    let route: Vec<_> = actor
        .ai
        .path
        .optimized
        .iter()
        .map(|cell| (cell.cell.x, cell.cell.y, cell.layer))
        .collect();
    let path: Vec<_> = actor
        .ai
        .ground_path
        .as_ref()
        .map_or_else(Vec::new, |path| path.positions().collect());
    let goal = actor.ai.path_goal.map(|cell| [cell.x, cell.y]);
    let foundations: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, target)| target.under_construction() && target.maintenance.builder == Some(id))
        .map(|(index, target)| (index, target.owner, target.position, target.build_progress))
        .collect();
    format!(
        "order={} assigned_foundations={foundations:?} position={:?} health={} velocity={:?} ignored={:?} following_rally={:?} endpoint={:?} goal={goal:?} pending={} failed={} path_frame={} has_motion_goal={} raw_route={raw_route:?} route={route:?} ground_path={path:?} extra_distance={} blocked_frames={} stuck={} retry={:?} temporary={:?}; {}",
        order_details(actor.ai.order),
        actor.position,
        actor.health,
        actor.ai.motion.velocity,
        actor.ai.ignored_producer,
        actor.ai.waypoints.points.get(1).copied(),
        actor.ai.path_endpoint,
        actor.ai.path_request.pending.is_some(),
        actor.ai.path_request.failed,
        actor.ai.blocking.path_frame,
        actor.ai.motion.has_goal(),
        actor.ai.command.path_extra_distance,
        actor.ai.blocking.frames,
        actor.ai.blocking.stuck,
        actor.ai.blocking.retry_at,
        actor.ai.blocking.temporary_goal(),
        movement_sample(game, id),
    )
}

fn order_details(order: Order) -> String {
    match order {
        Order::Enter(id) => format!("Enter({id})"),
        Order::Attack(id) => format!("Attack({id})"),
        Order::AttackArea => "AttackArea".into(),
        Order::Build(id) => format!("Build({id})"),
        Order::Repair(id) => format!("Repair({id})"),
        Order::RepairAt(id) => format!("RepairAt({id})"),
        Order::Harvest(id) => format!("Harvest({id})"),
        Order::CleanHazard(id) => format!("CleanHazard({id})"),
        Order::Move(point) => format!("Move({point:?})"),
        Order::Exit(point) => format!("Exit({point:?})"),
        Order::AttackMove(point) => format!("AttackMove({point:?})"),
        Order::Guard(point) => format!("Guard({point:?})"),
        Order::FireAt { position, weapon } => format!("FireAt({position:?}, {weapon})"),
        Order::ExitContainer => "ExitContainer".into(),
        Order::HackInternet => "HackInternet".into(),
        Order::Idle => "Idle".into(),
        Order::FollowPath => "FollowPath".into(),
        Order::ReturnSupply => "ReturnSupply".into(),
    }
}

/// Appends one frame and returns its six-word logic RNG without advancing it.
pub fn record(game: &Skirmish, trace: &mut DefaultHasher) -> [u32; 6] {
    game.tick.hash(trace);
    game.random.hash(trace);
    game.outcome.hash(trace);
    game.ended_at.hash(trace);
    for actor in &game.entities {
        actor.type_index.hash(trace);
        actor.owner.hash(trace);
        actor.position.map(f32::to_bits).hash(trace);
        actor.facing.to_bits().hash(trace);
        actor.ai.motion.velocity.map(f32::to_bits).hash(trace);
        actor.health.to_bits().hash(trace);
        actor.experience.hash(trace);
        actor.cargo.hash(trace);
        actor.ai.mood.next_check.hash(trace);
        actor.ai.ai_target.hash(trace);
        actor.ai.ignored_producer.hash(trace);
        record_order(actor.ai.order, trace);
        actor.ai.path.raw.len().hash(trace);
        for cell in &actor.ai.path.raw {
            [cell.cell.x, cell.cell.y].hash(trace);
        }
        actor.ai.path.optimized.len().hash(trace);
        for cell in &actor.ai.path.optimized {
            [cell.cell.x, cell.cell.y].hash(trace);
        }
        actor
            .ai
            .path_endpoint
            .map(|point| point.map(f32::to_bits))
            .hash(trace);
        actor.ai.waypoints.index.hash(trace);
        for point in &actor.ai.waypoints.points {
            point.map(f32::to_bits).hash(trace);
        }
        actor.weapon_slot().hash(trace);
        for weapon in &actor.weapon_set.weapons {
            weapon.cooldown.to_bits().hash(trace);
            weapon.clip_remaining.hash(trace);
        }
    }
    for player in &game.players {
        player.cash.hash(trace);
        player.power.hash(trace);
        player.science_points.hash(trace);
    }
    game.presentation.shots.len().hash(trace);
    for shot in &game.presentation.shots {
        [
            shot.source,
            shot.target.unwrap_or(usize::MAX),
            shot.weapon_slot,
            shot.barrel,
        ]
        .hash(trace);
        shot.source_position.map(f32::to_bits).hash(trace);
        shot.target_position.map(f32::to_bits).hash(trace);
    }
    game.projectiles.len().hash(trace);
    for projectile in &game.projectiles {
        projectile.id.hash(trace);
        projectile.position.map(f32::to_bits).hash(trace);
        projectile.destination.map(f32::to_bits).hash(trace);
        projectile.target.hash(trace);
        projectile.damage_bonus.to_bits().hash(trace);
        projectile.weapon_type.hash(trace);
        projectile.object.hash(trace);
    }
    game.random
}

fn record_order(order: Order, trace: &mut DefaultHasher) {
    std::mem::discriminant(&order).hash(trace);
    match order {
        Order::CleanHazard(id) => {
            id.hash(trace);
        }
        Order::Enter(id)
        | Order::Attack(id)
        | Order::Build(id)
        | Order::Repair(id)
        | Order::RepairAt(id)
        | Order::Harvest(id) => id.hash(trace),
        Order::Move(point) | Order::AttackMove(point) | Order::Guard(point) => {
            point.map(f32::to_bits).hash(trace);
        }
        Order::Exit(point) => point.map(f32::to_bits).hash(trace),
        Order::FireAt { position, weapon } => {
            position.map(f32::to_bits).hash(trace);
            weapon.hash(trace);
        }
        Order::AttackArea
        | Order::ExitContainer
        | Order::HackInternet
        | Order::Idle
        | Order::FollowPath
        | Order::ReturnSupply => {}
    }
}
