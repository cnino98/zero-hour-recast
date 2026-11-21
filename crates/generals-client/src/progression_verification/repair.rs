//! Existing paid USA progression army and distinct stock Emergency Repair OCL branches.
use crate::{
    skirmish_verification::{advance_until, friendly_entity, produce_type, require},
    strategy_verification::template,
};
use generals_sim::skirmish::{Kind, PowerTarget, Skirmish};
use std::error::Error;

const POWER: &str = "SuperweaponEmergencyRepair";

pub(super) fn verify_higher_ranks(
    game: &mut Skirmish,
    provider: usize,
) -> Result<(), Box<dyn Error>> {
    let factory = friendly_entity(game, Kind::Factory, 0)?;
    let crusader = template(game, "AmericaTankCrusader")?;
    let price = game.build_cost(0, crusader);
    advance_until(
        game,
        18_000,
        "ordinary supplies funding two repair test vehicles",
        |game| game.players[0].cash >= price * 2,
    )?;
    let victim = produce_type(game, factory, crusader, 0)?;
    let attacker = produce_type(game, factory, crusader, 0)?;
    // The source tank weapon explicitly allows friendly damage. Isolating two
    // paid vehicles at their factory avoids damaging the promotion army.
    for (science, marker, amount) in [
        (
            "SCIENCE_EmergencyRepair2",
            "RepairVehiclesInArea_InvisibleMarker_Level2",
            200.0,
        ),
        (
            "SCIENCE_EmergencyRepair3",
            "RepairVehiclesInArea_InvisibleMarker_Level3",
            300.0,
        ),
    ] {
        verify_rank(game, provider, victim, attacker, science, marker, amount)?;
    }
    Ok(())
}

fn verify_rank(
    game: &mut Skirmish,
    provider: usize,
    victim: usize,
    attacker: usize,
    science: &str,
    marker: &str,
    amount: f32,
) -> Result<(), Box<dyn Error>> {
    let cooldown = game.power_cooldown_remaining(provider, POWER);
    game.purchase_science(0, science)?;
    require(
        game.power_cooldown_remaining(provider, POWER) == cooldown,
        "repair upgrades reset the existing recharge",
    )?;
    advance_until(game, 7201, "shared Emergency Repair recharge", |game| {
        game.can_use_power(provider, POWER)
    })?;
    let maximum = game.entities[victim].max_health;
    game.force_attack(attacker, victim)?;
    advance_until(
        game,
        1800,
        "ordinary tank fire creating enough damage for the repair rank",
        |game| {
            let target = &game.entities[victim];
            target.alive()
                && maximum - target.health >= amount
                && target.last_attacker() == Some(attacker)
        },
    )?;
    game.stop(attacker)?;
    let before = game.entities[victim].health;
    let first = game.entities.len();
    let marker_type = template(game, marker)?;
    game.cast_power(
        provider,
        POWER,
        PowerTarget::Position(game.entities[victim].position),
    )?;
    require(
        game.entities[first..]
            .iter()
            .any(|actor| actor.owner == 0 && actor.type_index == marker_type),
        "Emergency Repair created the wrong original rank marker",
    )?;
    game.step();
    require(
        (game.entities[victim].health - before - amount).abs() < 0.000_01,
        &format!("{science} did not restore {amount} HP through the original AutoHeal burst"),
    )?;
    advance_until(game, 2, "single-burst repair marker retiring", |game| {
        game.entities[first..]
            .iter()
            .all(|actor| actor.type_index != marker_type || !actor.exists())
    })?;
    require(
        (game.entities[victim].health - before - amount).abs() < 0.000_01,
        "Emergency Repair pulsed more than once",
    )?;
    Ok(())
}
