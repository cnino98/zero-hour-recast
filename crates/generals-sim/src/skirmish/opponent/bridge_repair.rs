//! `AISkirmishPlayer::checkBridges` and the priority default-team dozer order.
use super::{OpponentRules, State, Team, WorkOrder, teams};
use crate::skirmish::Skirmish;

pub(super) fn preflight(
    game: &Skirmish,
    rules: &OpponentRules,
    state: &mut State,
    first_member: usize,
    waypoint: usize,
) {
    if game.entities[first_member].ai.registration.is_none() {
        return;
    }
    // Source walks the waypoint list's next pointer, not authored path links.
    for waypoint in &rules.map.waypoints[waypoint..] {
        if let Some(bridge) = game.broken_bridge_towards(first_member, waypoint.position) {
            state.repair.enqueue(game, bridge);
            break;
        }
    }
}

pub(super) fn update(game: &mut Skirmish, state: &mut State) {
    if state.repair.update(game, state.owner, state.repair_center) {
        queue_dozer(game, state);
    }
}

fn queue_dozer(game: &mut Skirmish, state: &mut State) {
    if state.build_queue.iter().any(|&index| {
        state.requests[index].orders.iter().any(|order| {
            game.catalog[order.type_index]
                .kind_of
                .iter()
                .any(|kind| kind == "DOZER")
        })
    }) {
        return;
    }
    let allowed = game.map_build_allowed(state.owner, true);
    game.set_map_build_allowed(state.owner, true, true);
    start_dozer_order(game, state);
    game.set_map_build_allowed(state.owner, true, allowed);
}

fn start_dozer_order(game: &mut Skirmish, state: &mut State) {
    let Some((type_index, producer)) = game.layout.map_template_order.iter().find_map(|template| {
        if !template.dozer {
            return None;
        }
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == template.name)?;
        Some((index, teams::factory(game, state, index, true)?))
    }) else {
        return;
    };
    let team = state.teams.len();
    state.teams.push(Team::new(None, game.tick, 0));
    state.queue_request(
        team,
        vec![WorkOrder {
            type_index,
            remaining: 1,
            producer: game
                .queue_unit(producer, type_index)
                .ok()
                .map(|()| producer),
            required: true,
            resource_gatherer: false,
        }],
        game.tick,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        grid::Cell,
        skirmish::{
            AiWaypoint, Faction, Kind, OpponentDifficulty, map_scripts::TemplateDefinition,
        },
        terrain::{BridgeGeometry, TerrainMap},
    };
    use std::sync::Arc;

    fn river() -> (Skirmish, usize, usize, u8) {
        let mut game = crate::skirmish::tests::game();
        let mut bridge =
            crate::skirmish::tests::fixture_type(Faction::Civilian, Kind::TechBuilding);
        bridge.name = "MonumentTrainBridge".into();
        bridge.kind_of = [
            "STRUCTURE",
            "IMMOBILE",
            "BRIDGE",
            "LANDMARK_BRIDGE",
            "UNATTACKABLE",
        ]
        .map(str::to_owned)
        .to_vec();
        let bridge_type = game.catalog.len();
        game.catalog.push(bridge);
        let bridge = game.spawn(bridge_type, game.neutral_owner(), [355.0, 205.0], true);
        game.change_body_health(bridge, -game.entities[bridge].max_health);
        let water = (31..=39).flat_map(|x| (0..60).map(move |y| Cell { x, y }));
        let mut terrain = TerrainMap::classify(80, 60, [], water).unwrap();
        let layer = terrain
            .add_bridge(
                BridgeGeometry {
                    from: [285.0, 205.0, 60.0],
                    to: [425.0, 205.0, 60.0],
                    corners: [
                        [285.0, 230.0, 60.0],
                        [285.0, 180.0, 60.0],
                        [425.0, 230.0, 60.0],
                        [425.0, 180.0, 60.0],
                    ],
                },
                |_| 0.0,
            )
            .unwrap();
        terrain.set_bridge_destroyed(layer, true).unwrap();
        game.configure_movement_terrain(terrain).unwrap();
        let infantry = game.type_for(1, Kind::Infantry).unwrap();
        let scout = game.spawn(infantry, 1, [245.0, 205.0], true);
        (game, bridge, scout, layer)
    }

    fn issue_approach(game: &mut Skirmish, scout: usize) -> (Arc<OpponentRules>, State) {
        let mut rules = super::super::tests::rules(Vec::new());
        rules.factions[0].teams.push(super::super::tests::recipe());
        rules.map.waypoints = vec![
            AiWaypoint {
                id: 1,
                position: [265.0, 205.0],
                labels: vec!["Attack1".into()],
                links: vec![2],
                bidirectional: false,
            },
            AiWaypoint {
                id: 2,
                position: [535.0, 205.0],
                labels: Vec::new(),
                links: Vec::new(),
                bidirectional: false,
            },
        ];
        let rules = Arc::new(rules);
        game.opponent_rules = Some(Arc::clone(&rules));
        let mut state = State::new(game, &rules, 1, OpponentDifficulty::Hard, 0);
        state.enemy_start = 0;
        let prototype = game
            .team_membership
            .add_prototype(1, "Bridge Approach".into());
        let native = game.team_membership.create_team(prototype);
        game.team_membership.set_team(scout, Some(native));
        state.teams.push(Team::new(Some(0), 0, 0));
        state.teams[0].native_instance = Some(native);
        super::super::map::begin(game, &rules, &mut state, 0, "Attack", Some(true));
        (rules, state)
    }

    #[test]
    fn stock_follow_detects_broken_bridge_and_pays_for_replacement_dozer() {
        let (mut game, bridge, scout, layer) = river();
        assert_eq!(
            game.broken_bridge_towards(scout, [535.0, 205.0]),
            Some(bridge)
        );
        assert_eq!(game.broken_bridge_towards(scout, [260.0, 205.0]), None);
        let (rules, mut state) = issue_approach(&mut game, scout);
        assert_eq!(state.repair.queue, [bridge]);

        let builder_type = game.type_for(1, Kind::Builder).unwrap();
        game.catalog[builder_type].name = "RepairDozer".into();
        for id in 0..game.entities.len() {
            if game.entities[id].owner == 1 && game.attack_kind(id, "DOZER") {
                game.destroy_maintenance_object(id);
            }
        }
        game.process_update_destruction();
        let original = game.spawn(builder_type, 1, [255.0, 230.0], true);
        game.catalog[0].energy = 1;
        let factory = game.spawn(0, 1, [135.0, 340.0], true);
        game.update_power();
        let builder_name = game.catalog[builder_type].name.clone();
        game.catalog[0].buildable_objects = vec![builder_name.clone()];
        game.layout.map_template_order.push(TemplateDefinition {
            name: builder_name,
            dozer: true,
            harvester: false,
        });
        state.buildings.push(super::super::Building {
            name: game.catalog[0].name.clone(),
            position: game.entities[factory].position,
            angle: 0.0,
            remaining: 0,
            automatic: false,
            priority: false,
            object: Some(factory),
            destroyed_at: None,
            supply: None,
        });
        update(&mut game, &mut state);
        assert_eq!(state.repair.worker, Some(original));
        assert!(game.entities[original].repair_task_target().is_none());
        game.destroy_maintenance_object(original);
        game.process_update_destruction();
        for _ in 0..30 {
            update(&mut game, &mut state);
        }
        assert_eq!(state.repair.worker, None);
        let cash = game.players[1].cash;
        let cost = game.build_cost(1, builder_type);
        update(&mut game, &mut state);
        assert_eq!(game.players[1].cash, cash - cost);
        assert_eq!(game.entities[factory].queue.len(), 1);
        game.opponents[1] = Some(super::super::Opponent { rules, state });
        for _ in 0..300 {
            game.step();
            if game.opponents[1]
                .as_ref()
                .unwrap()
                .state
                .repair
                .worker
                .is_some()
            {
                break;
            }
        }
        let state = &game.opponents[1].as_ref().unwrap().state;
        let replacement = state.repair.worker.unwrap_or_else(|| {
            let progress = game.entities[factory]
                .queue
                .front()
                .map(|item| (item.elapsed_frames, item.remaining_seconds, item.produced));
            panic!("dozer production did not register after stepping; queue progress {progress:?}");
        });
        assert_ne!(replacement, original);
        assert_eq!(game.entities[replacement].producer, Some(factory));
        assert_eq!(game.entities[replacement].owner, 1);
        assert!(game.entities[replacement].repair_task_target().is_none());
        assert_eq!(state.repair.queue, [bridge]);
        assert_eq!(game.body_damage_state(bridge), 3);
        assert_eq!(
            game.movement_terrain.ground_bridge(Cell { x: 29, y: 20 }),
            None
        );
        assert_eq!(
            game.broken_bridge_towards(scout, [535.0, 205.0]),
            Some(bridge)
        );
        assert_eq!(
            game.movement_terrain
                .destroyed_bridge_connections()
                .next()
                .unwrap()
                .0,
            layer
        );
    }
}
