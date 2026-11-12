use super::scene::Scene;
use crate::{
    playtest::Match,
    skirmish_verification::{construction_position, friendly_entity, require},
};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub(super) struct Setup {
    pub tank: usize,
    pub focus: [f32; 3],
    pub destination: [f32; 3],
    pub layer: u8,
    arrival: [f32; 3],
}

enum State {
    Choose(usize),
    Reveal {
        next: usize,
        kind: usize,
        position: [f32; 2],
    },
    Building {
        next: usize,
        id: usize,
    },
    Producing(Production),
    Scouting(Setup),
}

struct Production {
    factory: usize,
    kind: usize,
    first: usize,
}

pub(super) struct Preparation {
    builder: usize,
    home: [f32; 2],
    state: Option<State>,
    since: u64,
}

impl Preparation {
    pub(super) fn begin(battle: &mut Match) -> Result<Self, Box<dyn Error>> {
        if !battle.started {
            battle.start(Faction::Usa);
        }
        let game = &battle.simulation;
        let builder = friendly_entity(game, Kind::Builder, 0)?;
        Ok(Self {
            builder,
            home: game.entities[builder].position,
            state: Some(State::Choose(0)),
            since: game.tick,
        })
    }

    pub(super) fn focus(&self, game: &Skirmish) -> [f32; 3] {
        game.object_position(self.builder)
    }

    pub(super) const fn description(&self) -> &'static str {
        match self.state {
            Some(State::Choose(_)) => "construction site selection",
            Some(State::Reveal { .. }) => "builder revealing construction site",
            Some(State::Building { .. }) => "paid construction",
            Some(State::Producing(_)) => "paid production and native exit",
            Some(State::Scouting(_)) => "paid shoreline/bridge approach",
            None => "complete",
        }
    }

    pub(super) fn advance(
        &mut self,
        battle: &mut Match,
        scene: &Scene,
        bridge: bool,
    ) -> Result<Option<Setup>, Box<dyn Error>> {
        let limit = if matches!(self.state, Some(State::Reveal { .. })) {
            7200
        } else {
            18000
        };
        if battle.simulation.tick - self.since > limit {
            return Err(format!(
                "world preparation {} exceeded {limit} ordinary ticks; builder {}",
                self.description(),
                generals_sim::skirmish::diagnostics::movement_details(
                    &battle.simulation,
                    self.builder
                )
            )
            .into());
        }
        // Native Dozer commands stamped at frame zero are not selected. Let the
        // actual first FixedUpdate drain startup events and run that logic frame.
        if battle.simulation.tick == 0 {
            return Ok(None);
        }
        let state = self
            .state
            .take()
            .ok_or("world preparation already completed")?;
        self.state = Some(match state {
            State::Choose(next) => self.choose(&mut battle.simulation, next)?,
            State::Reveal {
                next,
                kind,
                position,
            } => {
                let game = &mut battle.simulation;
                if game.can_build_facing(
                    self.builder,
                    kind,
                    position,
                    game.catalog[kind].placement_angle,
                ) {
                    self.build(game, next, kind, position)?
                } else {
                    State::Reveal {
                        next,
                        kind,
                        position,
                    }
                }
            }
            State::Building { next, id } => {
                let game = &mut battle.simulation;
                require(game.entities[id].alive(), "paid world foundation died")?;
                if !game.entities[id].complete() {
                    State::Building { next, id }
                } else if next < 2 {
                    self.since = game.tick;
                    State::Choose(next + 1)
                } else {
                    self.produce(game, id)?
                }
            }
            State::Producing(production) => {
                self.poll_production(battle, scene, bridge, production)?
            }
            State::Scouting(setup) => {
                if arrive(&mut battle.simulation, &setup)? {
                    return Ok(Some(setup));
                }
                State::Scouting(setup)
            }
        });
        Ok(None)
    }

    fn poll_production(
        &mut self,
        battle: &mut Match,
        scene: &Scene,
        bridge: bool,
        production: Production,
    ) -> Result<State, Box<dyn Error>> {
        let Production {
            factory,
            kind,
            first,
        } = production;
        let game = &battle.simulation;
        let tank = (first..game.entities.len()).find(|&id| {
            let actor = &game.entities[id];
            actor.producer() == Some(factory)
                && actor.type_index == kind
                && actor.complete()
                && !matches!(actor.ai.order, Order::Exit(_))
        });
        if let Some(tank) = tank {
            require(
                game.entities[tank].alive(),
                "paid world scout died leaving factory",
            )?;
            self.since = game.tick;
            Ok(State::Scouting(scout(
                battle, scene, bridge, self.home, tank,
            )?))
        } else {
            Ok(State::Producing(production))
        }
    }

    fn choose(&mut self, game: &mut Skirmish, next: usize) -> Result<State, Box<dyn Error>> {
        let kind = [Kind::PowerPlant, Kind::SupplyCenter, Kind::Factory][next];
        let kind = game.type_for(0, kind).ok_or("USA world building missing")?;
        let position = construction_position(game, self.builder, kind, 0)?;
        if game.can_build_facing(
            self.builder,
            kind,
            position,
            game.catalog[kind].placement_angle,
        ) {
            self.build(game, next, kind, position)
        } else {
            game.move_to(self.builder, position)?;
            self.since = game.tick;
            Ok(State::Reveal {
                next,
                kind,
                position,
            })
        }
    }

    fn build(
        &mut self,
        game: &mut Skirmish,
        next: usize,
        kind: usize,
        position: [f32; 2],
    ) -> Result<State, Box<dyn Error>> {
        let cash = game.players[0].cash;
        let building = game.build(self.builder, kind, position)?;
        require(
            cash.checked_sub(game.catalog[kind].cost) == Some(game.players[0].cash),
            "world construction did not deduct installed BuildCost",
        )?;
        require(
            !game.entities[building].complete(),
            "world foundation completed instantly",
        )?;
        self.since = game.tick;
        Ok(State::Building { next, id: building })
    }

    fn produce(&mut self, game: &mut Skirmish, factory: usize) -> Result<State, Box<dyn Error>> {
        let kind = game
            .type_for(0, Kind::Tank)
            .ok_or("USA world tank missing")?;
        let first = game.entities.len();
        let cash = game.players[0].cash;
        game.train(factory, kind)?;
        require(
            cash.checked_sub(game.catalog[kind].cost) == Some(game.players[0].cash),
            "world production did not deduct installed BuildCost",
        )?;
        self.since = game.tick;
        Ok(State::Producing(Production {
            factory,
            kind,
            first,
        }))
    }
}

fn arrive(game: &mut Skirmish, setup: &Setup) -> Result<bool, Box<dyn Error>> {
    require(
        game.entities[setup.tank].alive(),
        "paid world scout died approaching feature",
    )?;
    if squared(
        game.entities[setup.tank].position,
        [setup.arrival[0], setup.arrival[1]],
    ) >= 25.0 * 25.0
    {
        return Ok(false);
    }
    game.stop(setup.tank)?;
    println!(
        "World paid preparation complete: tick={} tank={} position={:?} destination={:?}",
        game.tick,
        setup.tank,
        game.object_position(setup.tank),
        setup.destination
    );
    Ok(true)
}

fn scout(
    battle: &mut Match,
    scene: &Scene,
    bridge: bool,
    home: [f32; 2],
    tank: usize,
) -> Result<Setup, Box<dyn Error>> {
    battle.select_objects(&[tank], true);
    require(
        battle.command_message_members() == [tank],
        "paid world scout did not enter its player command group",
    )?;
    let game = &mut battle.simulation;
    let distance = |point: [f32; 2]| squared(point, home);
    let (position, destination, layer, focus) = if bridge {
        require(
            !scene.roads.is_empty(),
            "original bridge map has no road records",
        )?;
        let &(layer, mut ends) = scene
            .bridge_staging
            .iter()
            .min_by(|(_, a), (_, b)| {
                distance([a[0][0], a[0][1]])
                    .min(distance([a[1][0], a[1][1]]))
                    .total_cmp(&distance([b[0][0], b[0][1]]).min(distance([b[1][0], b[1][1]])))
            })
            .ok_or("original bridge capture map has no classified bridge")?;
        if distance([ends[1][0], ends[1][1]]) < distance([ends[0][0], ends[0][1]]) {
            ends.swap(0, 1);
        }
        game.move_to_world(tank, ends[1])?;
        require(
            exact_goal(game, tank, ends[1]),
            "bridge far-bank staging target was adjusted",
        )
        .map_err(|error| scouting_failure(game, tank, ends[1], error))?;
        game.stop(tank)?;
        game.move_to_world(tank, ends[0])?;
        require(
            exact_goal(game, tank, ends[0]),
            "bridge near-bank staging target was adjusted",
        )
        .map_err(|error| scouting_failure(game, tank, ends[0], error))?;
        let (_, native_ends) = scene
            .bridges
            .iter()
            .find(|(candidate, _)| *candidate == layer)
            .ok_or("bridge staging has no original geometry")?;
        let focus = std::array::from_fn(|axis| (native_ends[0][axis] + native_ends[1][axis]) * 0.5);
        (ends[0], ends[1], layer, focus)
    } else {
        let mut candidates = scene.shores.clone();
        candidates.sort_by(|a, b| distance([a[0], a[1]]).total_cmp(&distance([b[0], b[1]])));
        let mut selected = None;
        for point in candidates {
            match game.move_to_world(tank, point) {
                Ok(()) => {}
                Err("Destination is blocked or unreachable") => continue,
                Err(error) => return Err(scouting_failure(game, tank, point, error)),
            }
            if exact_goal(game, tank, point) {
                selected = Some(point);
                break;
            }
            game.stop(tank)?;
        }
        let point =
            selected.ok_or("original rainy map has no unadjusted paid-tank shoreline staging")?;
        (point, point, 1, point)
    };
    println!(
        "World scout request: bridge={bridge} layer={layer} home={home:?} tank={tank} departure={:?} requested={position:?} destination={destination:?} shoreline_candidates={}",
        game.entities[tank].position,
        scene.shores.len()
    );
    Ok(Setup {
        tank,
        focus,
        destination,
        layer,
        arrival: position,
    })
}

// Staging selection only; simulation movement uses its original native routines.
fn squared(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let horizontal = dx * dx;
    let vertical = dy * dy;
    horizontal + vertical
}

fn scouting_failure(
    game: &generals_sim::skirmish::Skirmish,
    tank: usize,
    requested: [f32; 3],
    error: impl std::fmt::Display,
) -> Box<dyn Error> {
    let actor = &game.entities[tank];
    let route = generals_sim::skirmish::diagnostics::ground_route_result(game, tank)
        .map(|route| (route.requested, route.endpoint));
    let movement = generals_sim::skirmish::diagnostics::movement_details(game, tank);
    format!(
        "{error}; scout={tank}:{} requested={requested:?} actual={:?} goal={:?} layer={} distance_squared={} completed_route={route:?}; {movement}",
        game.catalog[actor.type_index].name,
        game.object_position(tank), actor.ai_goal_position(), actor.layer,
        squared(actor.position, [requested[0], requested[1]])
    ).into()
}

fn exact_goal(game: &generals_sim::skirmish::Skirmish, tank: usize, requested: [f32; 3]) -> bool {
    let goal = game.entities[tank].ai_goal_position();
    [goal[0], goal[1]].map(f32::to_bits) == [requested[0], requested[1]].map(f32::to_bits)
}
