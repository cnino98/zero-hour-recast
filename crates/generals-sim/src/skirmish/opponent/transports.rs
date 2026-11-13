//! `ScriptActions::doLoadAllTransports` and the native fast partition solver.
use super::{AiCall, State};
use crate::skirmish::Skirmish;

pub(super) fn action(game: &mut Skirmish, state: &State, call: &AiCall, team: usize) -> Option<()> {
    match call.name.as_str() {
        "TEAM_LOAD_TRANSPORTS" => load(game, state, team),
        "TEAM_ENTER_NAMED" => {
            if let Some(host) = game.named_map_object(&call.parameters.get(1)?.text) {
                for &id in &state.teams[team].members {
                    if game.entities[id].alive()
                        && game.catalog[game.entities[id].type_index].speed > 0.0
                    {
                        game.native_enter_from(
                            id,
                            host,
                            crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT,
                        );
                    }
                }
            }
        }
        _ => return None,
    }
    Some(())
}

fn load(game: &mut Skirmish, state: &State, team: usize) {
    let mut entries = Vec::new();
    let mut spaces = Vec::new();
    for &id in &state.teams[team].members {
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        if unit.kind_of.iter().any(|kind| kind == "TRANSPORT") {
            if let Some(contain) = &unit.contain {
                // The source deliberately uses total capacity, not remaining
                // capacity. AI_ENTER applies actual admission afterward.
                spaces.push((id, contain.capacity));
            }
        } else {
            entries.push((id, unit.transport_slots));
        }
    }
    for (id, host) in partition(entries, spaces) {
        if game.entities[id].alive() && game.catalog[game.entities[id].type_index].speed > 0.0 {
            game.native_enter_from(id, host, crate::skirmish::ai_goals::COMMAND_FROM_SCRIPT);
        }
    }
}

fn partition(mut entries: Vec<(usize, u32)>, mut spaces: Vec<(usize, u32)>) -> Vec<(usize, usize)> {
    // PartitionSolver uses descending size only; no distance or identity key.
    // Equal sizes retain incoming order here. C++ std::sort ties depend on its
    // STL; the represented small authored teams are checked against C++ fixtures.
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    spaces.sort_by_key(|space| std::cmp::Reverse(space.1));
    let mut result = Vec::new();
    for (id, size) in entries {
        if let Some((host, remaining)) = spaces.iter_mut().find(|(_, room)| size <= *room) {
            *remaining -= size;
            result.push((id, *host));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::partition;
    use crate::skirmish::opponent::{AiCall, AiParameter, AiScript, AiWaypoint, teams};

    fn command(name: &str, labels: &[&str]) -> AiCall {
        AiCall {
            name: name.into(),
            parameters: labels
                .iter()
                .map(|label| AiParameter {
                    integer: 0,
                    real: 0.0,
                    text: (*label).into(),
                    position: None,
                })
                .collect(),
        }
    }

    fn transport_fixture() -> (
        crate::skirmish::Skirmish,
        super::super::OpponentRules,
        super::State,
        usize,
    ) {
        let (mut game, rules, state, kind) = super::super::garrisons::tests::fixture();
        let mut unit = crate::skirmish::tests::fixture_type(
            game.players[1].faction,
            crate::skirmish::Kind::Vehicle,
        );
        unit.contain = game.catalog[kind].contain.take();
        unit.contain.as_mut().unwrap().garrison = None;
        unit.kind_of.push("TRANSPORT".into());
        crate::skirmish::tests::register_update(&mut unit, "TransportContain");
        game.catalog[kind] = unit;
        (game, rules, state, kind)
    }

    #[test]
    fn largest_entries_use_largest_capacity_first_and_skip_unplaceable_units() {
        assert_eq!(
            partition(
                vec![(0, 1), (1, 6), (2, 3), (3, 2), (4, 0)],
                vec![(10, 2), (11, 5)]
            ),
            [(2, 11), (3, 11), (0, 10), (4, 11)]
        );
    }

    #[test]
    fn equal_size_small_teams_retain_incoming_assignment_order() {
        assert_eq!(
            partition(vec![(4, 1), (2, 1), (9, 1)], vec![(8, 2), (7, 2)]),
            [(4, 8), (2, 8), (9, 7)]
        );
    }

    #[test]
    fn loading_uses_full_capacity_and_admission_rejects_overbooking_after_assignment() {
        use crate::skirmish::{Kind, Order};
        let (mut game, _, mut state, kind) = transport_fixture();
        let host = game.spawn(kind, 1, [400.0, 300.0], true);
        let other = game.spawn(kind, 1, [600.0, 300.0], true);
        let infantry = game.type_for(1, Kind::Infantry).unwrap();
        game.catalog[infantry].transport_slots = 1;
        let already = game.spawn(infantry, 1, [400.0, 300.0], true);
        game.enter(already, host).unwrap();
        for _ in 0..30 {
            game.step();
        }
        assert_eq!(game.entities[already].contained_by(), Some(host));
        let first = game.spawn(infantry, 1, [400.0, 300.0], true);
        let second = game.spawn(infantry, 1, [400.0, 300.0], true);
        state.teams[0].members = vec![host, first, second, other];
        super::load(&mut game, &state, 0);
        assert!(matches!(game.entities[first].ai.order, Order::Enter(id) if id == host));
        assert!(matches!(game.entities[second].ai.order, Order::Enter(id) if id == host));
        for _ in 0..30 {
            game.step();
        }
        assert_eq!(game.passengers(host).len(), 2);
        assert!(game.passengers(other).is_empty());
        assert!(
            game.entities[first].contained_by().is_none()
                || game.entities[second].contained_by().is_none()
        );
    }

    #[test]
    fn sequence_waits_for_boarding_then_moves_carrier_without_commanding_held_members() {
        use crate::skirmish::{Kind, Order};
        let (mut game, mut rules, mut state, kind) = transport_fixture();
        let host = game.spawn(kind, 1, [400.0, 300.0], true);
        let infantry = game.type_for(1, Kind::Infantry).unwrap();
        let rider = game.spawn(infantry, 1, [400.0, 300.0], true);
        let prototype = game
            .team_membership
            .add_prototype(1, "USA attack team".into());
        let native = game.team_membership.create_team(prototype);
        game.team_membership.set_team(rider, Some(native));
        game.team_membership.set_team(host, Some(native));
        state.teams[0].native_instance = Some(native);
        state.teams[0].members = vec![host, rider];
        rules.map.waypoints.push(AiWaypoint {
            id: 1,
            position: [700.0, 500.0],
            labels: vec![format!("Center{}", state.enemy_start + 1)],
            links: vec![],
            bidirectional: false,
        });
        rules.factions[0].scripts.push(AiScript {
            group: None,
            name: "Original USA attack sequence".into(),
            flags: 61,
            delay_frames: 0,
            conditions: vec![],
            actions: vec![
                command("TEAM_LOAD_TRANSPORTS", &["<This Team>"]),
                command("SKIRMISH_MOVE_TO_APPROACH_PATH", &["<This Team>", "Center"]),
            ],
            false_actions: vec![],
        });
        state.teams[0].sequence = Some((0, 0));
        teams::sequence(&mut game, &rules, &mut state, 0);
        assert_eq!(state.teams[0].sequence, Some((0, 1)));
        assert!(!game.entities[rider].ai_idle());
        assert!(matches!(game.entities[host].ai.order, Order::Idle));
        teams::sequence(&mut game, &rules, &mut state, 0);
        assert_eq!(state.teams[0].sequence, Some((0, 1)));
        for _ in 0..30 {
            game.step();
        }
        assert_eq!(game.entities[rider].contained_by(), Some(host));
        assert!(game.entities[rider].ai_idle());
        assert_eq!(game.team_membership.team_of(rider), Some(native));
        teams::sequence(&mut game, &rules, &mut state, 0);
        assert_eq!(state.teams[0].sequence, Some((0, 2)));
        assert!(matches!(game.entities[host].ai.order, Order::Move(_)));
        assert!(matches!(game.entities[rider].ai.order, Order::Idle));
        for _ in 0..120 {
            game.step();
        }
        assert!(game.entities[host].position[0] > 400.0);
        assert_eq!(game.entities[rider].contained_by(), Some(host));
    }
}
