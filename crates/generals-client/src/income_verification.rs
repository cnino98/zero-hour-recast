//! Actual installed Hacker construction, commands, income and earned veterancy.
use crate::skirmish_verification::{
    advance_until, construct, construct_type, friendly_entity, produce_type, require,
};
use crate::strategy_verification::template;
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub mod disable;
pub mod internet;

pub fn prepare(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaInfantryHacker")
        .ok_or("missing original Hacker")?;
    require(
        !game.prerequisites_met(0, index),
        "Hacker bypassed the Propaganda Center prerequisite",
    )?;
    construct(game, worker, Kind::Factory, 0)?;
    let propaganda = template(game, "ChinaPropagandaCenter")?;
    construct_type(game, worker, propaganda, 0)?;
    let id = produce_type(game, barracks, index, 0)?;
    for collector in 0..game.entities.len() {
        if game.entities[collector].owner == 0
            && game.catalog[game.entities[collector].type_index]
                .supply
                .capacity
                > 0
        {
            game.stop(collector)?;
        }
    }
    let rules = game.catalog[index]
        .behavior
        .hacking
        .as_ref()
        .ok_or("missing original Hacker AI module")?;
    require(
        rules.unpack == 219
            && rules.pack == 154
            && rules.delay == 60
            && rules.amounts == [5, 6, 8, 10]
            && rules.experience == 1,
        "installed Hacker values changed",
    )?;
    game.hack_internet(id)?;
    advance_until(game, 500, "original Hacker unpack", |game| {
        game.entities[id].hacking_condition() == Some("FIRING_A")
    })?;
    Ok(id)
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let id = prepare(game)?;
    let cash = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    let started = game.tick;
    for _ in 0..60 {
        game.step();
    }
    require(
        game.players[0].cash == cash,
        "Hacker paid before the native zero-counter update",
    )?;
    game.step();
    require(
        game.players[0].cash == cash + 5
            && game.players[0].statistics.money_earned == earned + 5
            && game.entities[id].experience == 1,
        "Hacker cash/statistics/XP transaction differs from source",
    )?;
    game.hack_internet(id)?;
    require(
        game.entities[id].hacking_condition() == Some("FIRING_A"),
        "repeat Hack restarted an active Hacker despite SH fix",
    )?;
    advance_until(game, 31_000, "income-earned heroic Hacker", |game| {
        game.entities[id].veterancy == 3
    })?;
    require(
        game.tick - started == 500 * 61,
        "Hacker's source income/XP timer drifted",
    )?;
    // Cash uses the rank before XP is granted on that same transaction.
    require(
        game.players[0].cash - cash == 100 * 5 + 200 * 6 + 200 * 8,
        "Hacker level cash amounts or XP ordering differ from source",
    )?;
    let before = game.players[0].cash;
    for _ in 0..61 {
        game.step();
    }
    require(
        game.players[0].cash == before + 10,
        "Heroic Hacker did not earn the original amount",
    )?;
    verify_packing(game, id)?;
    println!(
        "Installed Hacker: original prerequisites/production, unpack/hack/pack commands, 61-frame cash/XP, earned heroic income, repeat-command fix and pending movement passed"
    );
    internet::verify(game)?;
    disable::verify(game)
}

fn verify_packing(game: &mut Skirmish, id: usize) -> Result<(), Box<dyn Error>> {
    let origin = game.entities[id].position;
    let goal = [origin[0] + 70.0, origin[1]];
    game.move_to(id, goal)?;
    let (_, frames, completion) = game.entities[id]
        .animation_duration()
        .ok_or("missing source pack animation duration")?;
    require(
        !completion && (77..=231).contains(&frames),
        "Hacker pack duration is not the original randomized loop duration",
    )?;
    let cash = game.players[0].cash;
    for _ in 0..=frames {
        game.step();
    }
    require(
        game.entities[id].position.map(f32::to_bits) == origin.map(f32::to_bits)
            && game.players[0].cash == cash,
        "Hacker moved or earned cash before packing finished",
    )?;
    game.step();
    advance_until(game, 1800, "Hacker pending move", |game| {
        matches!(game.entities[id].ai.order, Order::Idle)
    })?;
    require(
        (game.entities[id].position[0] - origin[0]).abs() > 10.0,
        "Hacker discarded its pending movement",
    )?;
    game.hack_internet(id)?;
    game.stop(id)?;
    require(
        matches!(game.entities[id].ai.order, Order::Idle)
            && game.entities[id].hacking_condition().is_none(),
        "Stop failed to cancel Hacker unpack immediately",
    )?;
    Ok(())
}
