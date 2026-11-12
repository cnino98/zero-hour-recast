//! Exercise refunds against installed costs through ordinary player commands.
use super::{
    Faction, Kind, Order, Skirmish, TICKS_PER_MINUTE, advance_until, friendly_entity, require,
};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let center = friendly_entity(game, Kind::CommandCenter, 0)?;
    let kind = if game.players[0].faction == Faction::Gla {
        Kind::Barracks
    } else {
        Kind::PowerPlant
    };
    let building_type = game
        .type_for(0, kind)
        .ok_or("missing cancellable building")?;
    // getMostRecentCommand compares each task frame strictly against zero.
    // Submit the player's cancellable foundation after the first logic frame.
    game.step();
    let site = super::construction_position(game, builder, building_type, 0)?;
    super::scout_construction_site(game, builder, building_type, site)?;
    let initial_cash = game.players[0].cash;
    let spent = game.players[0].statistics.money_spent;
    let foundation = game.build(builder, building_type, site)?;
    advance_until(
        game,
        10 * TICKS_PER_MINUTE,
        "cancellable construction progress",
        |game| game.entities[foundation].build_progress > 0.0,
    )?;
    require(
        !game.entities[foundation].complete(),
        "cancellation missed construction",
    )?;
    game.cancel_construction(0, foundation)?;
    game.step();
    require(
        game.players[0].cash == initial_cash
            && game.players[0].statistics.money_earned == 0
            && game.players[0].statistics.money_spent == spent
            && !game.entities[foundation].alive()
            && matches!(game.entities[builder].ai.order, Order::Idle),
        "foundation cancellation did not refund its installed cost or release its builder",
    )?;
    verify_queue(game, center, initial_cash)?;
    println!("  installed foundation/queue refunds and stale production IDs passed");
    Ok(())
}

fn verify_queue(game: &mut Skirmish, center: usize, cash: u32) -> Result<(), Box<dyn Error>> {
    let builder_type = game.type_for(0, Kind::Builder).ok_or("missing builder")?;
    let cost = game.catalog[builder_type].cost;
    for _ in 0..3 {
        game.train(center, builder_type)?;
    }
    let head = game.entities[center].queue[0].id;
    let second = game.entities[center].queue[1].id;
    let tail = game.entities[center].queue[2].id;
    for _ in 0..7 {
        game.step();
    }
    let remaining = game.entities[center].queue[0].remaining_seconds;
    game.cancel_production(0, center, tail)?;
    require(
        game.entities[center].queue.len() == 2
            && (game.entities[center].queue[0].remaining_seconds - remaining).abs() < f32::EPSILON
            && game.players[0].cash == cash - 2 * cost,
        "tail cancellation changed head progress or its refund",
    )?;
    advance_until(
        game,
        TICKS_PER_MINUTE,
        "completed head production",
        |game| {
            game.entities[center]
                .queue
                .front()
                .is_some_and(|entry| entry.id == second)
        },
    )?;
    require(
        game.cancel_production(0, center, head).is_err()
            && game.cancel_production(1, center, second).is_err(),
        "a stale or foreign production command was accepted",
    )?;
    game.cancel_production(0, center, second)?;
    require(
        game.entities[center].queue.is_empty()
            && game.players[0].cash == cash - cost
            && game.players[0].statistics.money_earned == 0,
        "production cancellation refunded the completed unit or counted income",
    )
}
