//! Paid installed Scorpion Rocket/TOW research and actual original missile firing.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

fn template(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing {name}").into())
}

fn research(game: &mut Skirmish, factory: usize, name: &str) -> Result<(), Box<dyn Error>> {
    let index = game.catalog[game.entities[factory].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .ok_or_else(|| format!("missing paid {name}"))?;
    let cost = game.catalog[game.entities[factory].type_index]
        .maintenance
        .research[index]
        .cost;
    let before = game.players[0].cash;
    game.queue_upgrade(factory, index)?;
    require(
        game.players[0].cash == before - cost,
        "weapon research did not charge original price",
    )?;
    advance_until(game, 6000, "paid source weapon research", |game| {
        game.has_upgrade(0, name)
    })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    verify_tow(game)?;
    let (scorpion, _) = prepare_scorpion(game)?;
    let center = friendly_entity(game, Kind::CommandCenter, 1)?;
    let victim = template(game, "AmericaVehicleDozer")?;
    let stage = game.entities[scorpion].position;
    for level in 1..=2 {
        crate::skirmish_verification::technical::salvage_one(
            game, scorpion, center, victim, stage,
        )?;
        let actor = &game.entities[scorpion];
        let unit = &game.catalog[actor.type_index];
        require(
            actor.salvage_level() == level,
            "Scorpion lost crate tier with researched weapons",
        )?;
        let rocket = unit
            .weapon_template(actor.weapon_index(1))
            .ok_or("researched Scorpion rocket absent")?;
        require(
            rocket.clip_size == u32::from(level) && rocket.damage.to_bits() == 100.0_f32.to_bits(),
            "combined Scorpion research/crate set differs from source",
        )?;
        require(
            unit.weapon_template(actor.weapon_index(0))
                .is_some_and(|weapon| weapon.name == "ScorpionTankGunPlusOne"),
            "Scorpion crate cannon lost during research",
        )?;
    }
    println!(
        "Installed weapon research: paid TOW and Scorpion Rocket, existing/future vehicles, actual missile flight, two natural salvage pickups and combined two-rocket set passed"
    );
    Ok(())
}

pub fn prepare_scorpion(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let index = template(game, "GLATankScorpion")?;
    let scorpion = produce_type(game, factory, index, 0)?;
    require(
        !game.weapon_available(scorpion, 1),
        "unresearched Scorpion has rockets",
    )?;
    research(game, factory, "Upgrade_GLAScorpionRocket")?;
    require(
        game.weapon_available(scorpion, 1),
        "existing Scorpion did not receive rocket research",
    )?;
    let future = produce_type(game, factory, index, 0)?;
    require(
        game.weapon_available(future, 1),
        "future Scorpion did not receive rocket research",
    )?;
    Ok((scorpion, future))
}

fn verify_tow(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let index = template(game, "AmericaVehicleHumvee")?;
    let humvee = produce_type(game, factory, index, 0)?;
    require(
        !game.weapon_available(humvee, 2),
        "unresearched Humvee has TOW air weapon",
    )?;
    require(
        game.catalog[index]
            .weapon_template(game.entities[humvee].weapon_index(1))
            .is_some_and(|weapon| weapon.name == "HumveeMissileWeaponAirDummy"),
        "source pre-upgrade Humvee AA dummy absent",
    )?;
    research(game, factory, "Upgrade_AmericaTOWMissile")?;
    require(
        game.weapon_available(humvee, 1) && game.weapon_available(humvee, 2),
        "Humvee ground/air TOW sets absent",
    )?;
    let future = produce_type(game, factory, index, 0)?;
    require(
        game.weapon_available(future, 2),
        "future Humvee lacks TOW research",
    )?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    game.attack(humvee, target)?;
    advance_until(game, 18_000, "original Humvee TOW launch", |game| {
        game.projectiles.iter().any(|projectile| {
            projectile.source == humvee
                && projectile.weapon_slot == 1
                && projectile.render_transform(&game.entities).is_some()
        })
    })?;
    require(
        game.entities[humvee].weapon_rounds_remaining(1) == 0,
        "TOW clip did not enter its original reload",
    )?;
    let first_launch = game.tick;
    advance_until(
        game,
        100,
        "original TOW clip reload and second launch",
        |game| {
            game.tick > first_launch
                && game
                    .presentation
                    .shots
                    .iter()
                    .any(|shot| shot.source == humvee && shot.weapon_slot == 1)
        },
    )?;
    require(
        game.tick - first_launch >= 60,
        "TOW fired before its original two-second reload",
    )?;
    game.stop(humvee)?;
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    verify_tow_air(game, humvee, worker)
}

fn verify_tow_air(game: &mut Skirmish, humvee: usize, worker: usize) -> Result<(), Box<dyn Error>> {
    construct(game, worker, Kind::PowerPlant, 1)?;
    let center = construct(game, worker, Kind::SupplyCenter, 1)?;
    let chinook = super::airborne_supply_chinook(game, center)?;
    game.move_to(chinook, game.entities[humvee].position)?;
    advance_until(
        game,
        18_000,
        "Humvee original TOW anti-air weapon",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == humvee && shot.target == Some(chinook))
        },
    )?;
    require(
        game.presentation
            .shots
            .iter()
            .filter(|shot| shot.source == humvee && shot.target == Some(chinook))
            .all(|shot| shot.weapon_slot == 2),
        "upgraded Humvee used ground weapon against source airborne Chinook",
    )?;
    require(
        game.projectiles.iter().any(|projectile| {
            projectile.source == humvee
                && projectile.weapon_slot == 2
                && projectile.render_transform(&game.entities).is_some()
        }),
        "TOW anti-air shot lacks original missile flight",
    )?;
    Ok(())
}
