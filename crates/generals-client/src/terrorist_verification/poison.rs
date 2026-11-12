//! Paid SCUD anthrax exposure creates the original toxin-death object.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{DeathType, Faction, Kind, Skirmish};
use std::error::Error;

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Gla)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let actor = produce_type(game, barracks, template(game, "GLAInfantryTerrorist")?, 0)?;
    let enemy = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy, Kind::SupplyCenter, 1)?;
    let factory = construct(game, enemy, Kind::Factory, 1)?;
    construct_type(game, enemy, template(game, "GLAPalace")?, 1)?;
    game.purchase_science(1, "SCIENCE_ScudLauncher")?;
    let scud = produce_type(game, factory, template(game, "GLAVehicleScudLauncher")?, 1)?;
    let center = friendly_entity(game, Kind::CommandCenter, 0)?;
    let position = game.entities[center].position;
    approach(game, scud, position)?;
    approach(game, actor, position)?;
    game.switch_weapon(scud, 1)?;
    game.attack(scud, center)?;
    advance_until(
        game,
        3000,
        "paid SCUD producing real anthrax field",
        |game| game.hazards.iter().any(|field| field.source == scud),
    )?;
    game.stop(scud)?;
    let field = game
        .hazards
        .iter()
        .find(|field| field.source == scud)
        .ok_or("SCUD field disappeared")?;
    let position = field.position;
    let field_rules = game.catalog[game.entities[scud].type_index]
        .hazard_rules(field.origin)
        .ok_or("SCUD field lost source rules")?;
    let radius = field_rules.radius;
    let field_damage = field_rules.damage;
    let field_interval = field_rules.interval;
    let retreat = game.entities[actor].position;
    let direction = [retreat[0] - position[0], retreat[1] - position[1]];
    let length = direction[0].hypot(direction[1]);
    let edge = radius
        + game.catalog[game.entities[actor].type_index]
            .geometry_bounds
            .sphere_radius
        - 4.0;
    game.move_to(
        actor,
        [
            (direction[0] / length).mul_add(edge, position[0]),
            (direction[1] / length).mul_add(edge, position[1]),
        ],
    )?;
    advance_until(
        game,
        1800,
        "Terrorist touching original poison field",
        |game| game.entities[actor].poisoned(),
    )?;
    linger_before_retreat(game, actor, field_damage, field_interval)?;
    game.move_to(actor, retreat)?;
    advance_until(
        game,
        91,
        "Terrorist retreating under native lingering poison",
        |game| !game.entities[actor].alive(),
    )?;
    require(
        matches!(game.entities[actor].death_type(), DeathType::Poisoned),
        "SCUD field did not preserve native POISONED death",
    )?;
    verify_poison_corpse(game, actor, scud, position, radius)
}

fn verify_poison_corpse(
    game: &mut Skirmish,
    actor: usize,
    scud: usize,
    position: [f32; 3],
    radius: f32,
) -> Result<(), Box<dyn Error>> {
    let child = game
        .entities
        .iter()
        .enumerate()
        .find(|(_, entity)| {
            entity.owner == 0
                && game.catalog[entity.type_index].name == "ToxicInfantry_TerroristOnly"
        })
        .map(|(id, _)| id)
        .ok_or("original poison OCL did not create Terrorist-only replacement")?;
    require(
        game.entities[actor].masked() && game.entities[child].alive(),
        "zero-delay original corpse or living toxic replacement differs",
    )?;
    let child_position = game.entities[child].position;
    require(
        (child_position[0] - position[0]).hypot(child_position[1] - position[1])
            > radius
                + game.catalog[game.entities[child].type_index]
                    .geometry_bounds
                    .sphere_radius,
        "retreat must place toxic replacement outside continuing field damage",
    )?;
    game.stop(scud)?;
    advance_until(game, 91, "original toxic replacement lifetime", |game| {
        !game.entities[child].alive()
    })?;
    require(
        matches!(game.entities[child].death_type(), DeathType::Normal),
        "toxic lifetime failed to use normal death",
    )?;
    advance_until(game, 286, "original toxic corpse timed deletion", |game| {
        game.entities[child].masked()
    })?;
    println!(
        "Original Terrorist poison: paid SCUD field, POISONED death, native OCL reskin, live lifetime and subsequent normal corpse deletion passed"
    );
    Ok(())
}

fn linger_before_retreat(
    game: &mut Skirmish,
    id: usize,
    field_damage: f32,
    field_interval: u32,
) -> Result<(), Box<dyn Error>> {
    let unit = &game.catalog[game.entities[id].type_index];
    let poison = unit
        .behavior
        .poisoned
        .first()
        .ok_or("missing Terrorist poison rules")?;
    // A normal SCUD makes the original medium field. Its two-point dose cannot
    // kill a fresh Terrorist after a single exposure. Stay in the field until
    // even the oldest possible renewed dose can finish killing after retreat.
    let ticks = poison.duration.saturating_sub(field_interval) / poison.interval;
    let health = field_damage
        * unit.armor[generals_sim::skirmish::DamageType::Poison as usize]
        * f32::from(u16::try_from(ticks)?);
    require(
        health > 0.0,
        "poison retreat fixture has no recurring damage",
    )?;
    advance_until(game, 900, "source poison exposure before retreat", |game| {
        game.entities[id].health <= health
    })?;
    require(game.entities[id].alive(), "Terrorist died before retreat")
}

fn approach(game: &mut Skirmish, actor: usize, target: [f32; 2]) -> Result<(), Box<dyn Error>> {
    let source = game.entities[actor].position;
    let delta = [source[0] - target[0], source[1] - target[1]];
    let length = delta[0].hypot(delta[1]);
    let goal = [
        (delta[0] / length).mul_add(340.0, target[0]),
        (delta[1] / length).mul_add(340.0, target[1]),
    ];
    game.move_to(actor, goal)?;
    advance_until(
        game,
        18_000,
        "paid unit approaching toxin fixture",
        |game| {
            let position = game.entities[actor].position;
            (position[0] - goal[0]).hypot(position[1] - goal[1]) < 25.0
        },
    )?;
    game.stop(actor)?;
    Ok(())
}
