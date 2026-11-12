//! Installed ground transports through normal paid construction and commands.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;
pub mod chinook;
mod garrison;
pub mod heal;
mod map;
mod rubble;
pub use garrison::{engage as engage_garrison, prepare_palace, verify_palace};
pub use map::prepare_neutral_display;

pub struct Capture {
    pub humvee: usize,
    pub medic: usize,
    pub riders: Vec<usize>,
}

pub fn prepare(game: &mut Skirmish) -> Result<Capture, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let mut result = Capture {
        humvee: 0,
        medic: 0,
        riders: Vec::new(),
    };
    for (name, count) in [("AmericaVehicleHumvee", 5), ("AmericaVehicleMedic", 3)] {
        let kind = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .ok_or("missing installed transport")?;
        let carrier = produce_type(game, factory, kind, 0)?;
        let expected: (f32, f32) = if count == 5 { (1.0, 0.0) } else { (0.1, 25.0) };
        let rules = game.catalog[kind]
            .contain
            .as_ref()
            .ok_or("missing supported ground container")?;
        require(
            rules.capacity == count
                && rules.exit_frames == 8
                && rules.exit_paths == 3
                && rules.death.damage.to_bits() == expected.0.to_bits()
                && rules.regen_percent.to_bits() == expected.1.to_bits(),
            "installed original transport rules changed",
        )?;
        for passenger in 0..count {
            let name = if count == 5 && passenger == 0 {
                "AmericaInfantryMissileDefender"
            } else {
                "AmericaInfantryRanger"
            };
            let infantry = game
                .catalog
                .iter()
                .position(|unit| unit.name == name)
                .ok_or("missing installed infantry passenger")?;
            let id = produce_type(game, barracks, infantry, 0)?;
            game.enter(id, carrier)?;
            advance_until(game, 3000, "passenger approach and boarding", |game| {
                game.entities[id].contained_by() == Some(carrier)
            })
            .map_err(|error| boarding_error(game, id, carrier, &error.to_string()))?;
            require(
                game.entities[id].masked()
                    && !game.entities[id].selectable(&game.catalog[game.entities[id].type_index]),
                "enclosed passenger remains in world selection",
            )?;
            result.riders.push(id);
        }
        if count == 5 {
            result.humvee = carrier;
            // Keep the next produced vehicle's exit clear, as a player would.
            // The native Enter state does not force boarding through blockers.
            let position = game.entities[carrier].position;
            game.move_to(carrier, [position[0] - 120.0, position[1] + 100.0])?;
            advance_until(
                game,
                3000,
                "loaded Humvee clearing the factory exit",
                |game| {
                    !game.entities[carrier].moving()
                        && matches!(
                            game.entities[carrier].ai.order,
                            generals_sim::skirmish::Order::Idle
                        )
                },
            )?;
            require(
                game.passengers(carrier).len() == 5,
                "moving Humvee lost its passengers",
            )?;
        } else {
            result.medic = carrier;
        }
    }
    Ok(result)
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    heal::verify(game)?;
    chinook::verify(game)?;
    map::verify(game)?;
    let capture = prepare(game)?;
    let full = game.passengers(capture.humvee).len();
    let medic_rider = game.passengers(capture.medic)[0];
    game.exit_container(medic_rider)?;
    advance_until(game, 100, "individual passenger exit", |game| {
        game.entities[medic_rider].contained_by().is_none()
    })?;
    require(
        !game.can_enter(medic_rider, capture.humvee) && full == 5,
        "Humvee exceeded original slot capacity",
    )?;
    game.enter(medic_rider, capture.medic)?;
    advance_until(game, 3000, "passenger reboarding", |game| {
        game.entities[medic_rider].contained_by() == Some(capture.medic)
    })?;
    game.evacuate(capture.humvee)?;
    game.evacuate(capture.medic)?;
    advance_until(game, 1000, "timed evacuation", |game| {
        game.passengers(capture.humvee).is_empty() && game.passengers(capture.medic).is_empty()
    })?;
    require(
        capture
            .riders
            .iter()
            .all(|&id| game.entities[id].alive() && !game.entities[id].masked()),
        "evacuation lost or hid a living passenger",
    )?;
    println!(
        "Installed transports: paid Humvee/Medic and eight infantry, original capacities/exit timing, ordinary approach/boarding, hidden selection, individual exit/reboarding and evacuation passed"
    );
    Ok(())
}

fn boarding_error(game: &Skirmish, id: usize, carrier: usize, error: &str) -> String {
    let rider = &game.entities[id];
    let host = &game.entities[carrier];
    let order = match rider.ai.order {
        generals_sim::skirmish::Order::Enter(_) => "Enter",
        generals_sim::skirmish::Order::Idle => "Idle",
        _ => "other",
    };
    let nearby = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| {
            let x = actor.position[0] - host.position[0];
            let y = actor.position[1] - host.position[1];
            actor.alive() && !actor.masked() && x.mul_add(x, y * y) < 22_500.0
        })
        .map(|(other, actor)| {
            format!(
                "{other} {} at {:?}",
                game.catalog[actor.type_index].name, actor.position
            )
        })
        .collect::<Vec<_>>();
    format!(
        "{error}; rider {id} {} at {:?}, order {order}, moving {}, enter valid {}; host {carrier} {} at {:?}, moving {}, passengers {:?}; nearby {nearby:?}",
        game.catalog[rider.type_index].name,
        rider.position,
        rider.moving(),
        game.can_enter(id, carrier),
        game.catalog[host.type_index].name,
        host.position,
        host.moving(),
        game.passengers(carrier)
    )
}
