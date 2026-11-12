//! Ordinary builder commands keep promotion practice separate from idle repair.
use crate::skirmish_verification::{advance_until, friendly_entity, produce};
use generals_sim::skirmish::{Kind, Order, Skirmish};
use std::error::Error;

pub(super) fn builder(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    if let Ok(builder) = friendly_entity(game, Kind::Builder, 1) {
        return Ok(builder);
    }
    let center = friendly_entity(game, Kind::CommandCenter, 1).map_err(|error| {
        format!(
            "Promotion construction lost both enemy builder and Command Center at tick {}: {error}",
            game.tick
        )
    })?;
    let index = game
        .type_for(1, Kind::Builder)
        .ok_or("Missing enemy builder template")?;
    let cost = game.catalog[index].cost;
    advance_until(
        game,
        18_000,
        "ordinary income for replacement practice builder",
        |game| game.players[1].cash >= cost,
    )?;
    let builder = produce(game, center, Kind::Builder, 1)?;
    println!(
        "  Paid replacement {} {builder} after real combat loss",
        game.catalog[index].name
    );
    Ok(builder)
}

pub(super) fn retreat(
    game: &mut Skirmish,
    builder: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let actor = &game.entities[builder];
    let idle_range = game.catalog[actor.type_index]
        .maintenance
        .idle_repair
        .as_ref()
        .map_or(0.0, |rules| rules.range);
    let tank = game
        .type_for(0, Kind::Tank)
        .ok_or("Missing promotion army tank")?;
    let clearance = idle_range
        + game.catalog[tank].ai.vision_range
        + game.catalog[game.entities[target].type_index]
            .geometry
            .major;
    let center = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    let position = game.entities[target].position;
    let delta = [position[0] - center[0], position[1] - center[1]];
    let length = delta[0].hypot(delta[1]);
    let away = [delta[0] / length, delta[1] / length];
    // DozerPrimaryIdleState may return to any damaged building in BoredRange.
    // Leave both that repair radius and the commanded tanks' sight envelope.
    for direction in [
        away,
        [-away[1], away[0]],
        [away[1], -away[0]],
        [-away[0], -away[1]],
    ] {
        let offset = direction.map(|value| value * (clearance + 20.0));
        let destination = [position[0] + offset[0], position[1] + offset[1]];
        if game.move_to(builder, destination).is_err() {
            continue;
        }
        advance_until(
            game,
            18_000,
            "practice builder leaving the live-fire and idle-repair area",
            |game| {
                let actor = &game.entities[builder];
                !actor.alive() || (matches!(actor.ai.order, Order::Idle) && !actor.moving())
            },
        )
        .map_err(|error| {
            format!(
                "{error}; builder {builder}, target {target}, clearance {clearance}: {}",
                generals_sim::skirmish::diagnostics::movement_details(game, builder)
            )
        })?;
        let actor = &game.entities[builder];
        if !actor.alive() {
            return Err(
                format!("Practice builder {builder} died before target {target} attack").into(),
            );
        }
        if (actor.position[0] - position[0]).hypot(actor.position[1] - position[1]) > clearance {
            return Ok(());
        }
    }
    Err(format!(
        "Practice builder {builder} could not leave target {target} repair/fire area: {}",
        generals_sim::skirmish::diagnostics::movement_details(game, builder)
    )
    .into())
}
