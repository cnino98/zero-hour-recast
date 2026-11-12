//! Original Internet Center built and filled through ordinary player commands.
use crate::skirmish_verification::{
    advance_until, construct, construct_type, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, MaintenanceRules, Skirmish, income::Effect};
use std::error::Error;

#[cfg(test)]
mod tests;

pub fn prepare(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let center_type = type_index(game, "ChinaInternetCenter")?;
    verify_rules(game, center_type)?;
    require(
        !game.prerequisites_met(0, center_type),
        "Internet Center skipped War Factory",
    )?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    construct(game, worker, Kind::Factory, 0)?;
    require(
        game.prerequisites_met(0, center_type),
        "Internet Center wrongly requires an upgrade or Propaganda Center",
    )?;
    fund(game, center_type)?;
    let center = construct_type(game, worker, center_type, 0)?;
    require(
        !game.prerequisites_met(0, center_type),
        "Internet Center exceeded native one-building cap",
    )?;
    let cash = game.players[0].cash;
    require(
        game.build(worker, center_type, [150.0, 150.0]).is_err() && game.players[0].cash == cash,
        "Second Internet Center was admitted or spent money",
    )?;
    let propaganda = type_index(game, "ChinaPropagandaCenter")?;
    fund(game, propaganda)?;
    construct_type(game, worker, propaganda, 0)?;
    let hacker = type_index(game, "ChinaInfantryHacker")?;
    for _ in 0..8 {
        fund(game, hacker)?;
        let id = produce_type(game, barracks, hacker, 0)?;
        game.enter(id, center)?;
        advance_until(game, 9000, "Hacker entering Internet Center", |game| {
            game.entities[id].contained_by() == Some(center)
        })?;
        require(
            game.entities[id].hacking_condition() == Some("UNPACKING"),
            "InternetHackContain did not issue the full source Hack command",
        )?;
    }
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
    require(
        game.passengers(center).len() == 8,
        "Internet Center inventory differs from eight source slots",
    )?;
    // OpenContain::addToContain appends riders. Earlier Hackers have already
    // earned while their later companions were produced and walked inside.
    let last = *game
        .passengers(center)
        .last()
        .ok_or("Internet Center is empty")?;
    advance_until(game, 500, "contained Hacker unpack", |game| {
        game.entities[last].hacking_condition() == Some("FIRING_A")
    })?;
    Ok(center)
}

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let center = prepare(game)?;
    let id = *game
        .passengers(center)
        .last()
        .ok_or("Internet Center is empty")?;
    let cash = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    let experience = game.entities[id].experience;
    let mut deposits = 0;
    for _ in 0..54 {
        game.step();
        deposits += deposited_this_frame(game);
        require(
            game.entities[id].experience == experience,
            "Contained Hacker paid before the fast zero-counter update",
        )?;
    }
    game.step();
    deposits += deposited_this_frame(game);
    require(
        game.entities[id].experience == experience + 1
            && game.players[0].cash == cash + deposits
            && game.players[0].statistics.money_earned == earned + deposits,
        "Contained Hacker cash, statistics or XP drifted from 55-frame source timer",
    )?;
    game.exit_container(id)?;
    require(
        game.entities[id]
            .animation_duration()
            .is_some_and(|(_, frames, _)| frames == 0),
        "Contained Hacker must pack for zero frames before reissuing exit",
    )?;
    game.step();
    require(
        game.entities[id].contained_by() == Some(center),
        "Contained pack skipped source pending-command update",
    )?;
    game.step();
    require(
        game.entities[id].contained_by().is_none(),
        "Internet Center failed native inventory exit",
    )?;
    advance_until(game, 1800, "Hacker exit movement", |game| {
        matches!(
            game.entities[id].ai.order,
            generals_sim::skirmish::Order::Idle
        )
    })?;
    game.enter(id, center)?;
    advance_until(game, 9000, "Hacker reentry", |game| {
        game.entities[id].contained_by() == Some(center)
    })?;
    game.evacuate(center)?;
    advance_until(game, 1800, "Internet Center evacuation", |game| {
        game.passengers(center).is_empty()
    })?;
    println!(
        "Installed Internet Center: paid War Factory prerequisite, native cap, eight produced Hackers, full unpack, 55-frame contained cash/XP, inventory exit/reentry and evacuation passed"
    );
    Ok(())
}

fn verify_rules(game: &Skirmish, index: usize) -> Result<(), Box<dyn Error>> {
    let unit = &game.catalog[index];
    let contain = unit.contain.as_ref().ok_or("InternetHackContain missing")?;
    require(
        unit.cost == 2500
            && unit.build_seconds.to_bits() == 30.0_f32.to_bits()
            && matches!(
                unit.build_rules.limit,
                generals_sim::skirmish::TypeLimit::Count(1)
            )
            && unit.maintenance.kind_flags & MaintenanceRules::IMMUNE_TO_CAPTURE != 0
            && unit
                .maintenance
                .research
                .iter()
                .map(|upgrade| upgrade.name.as_str())
                .eq([
                    "Upgrade_ChinaSatelliteHackOne",
                    "Upgrade_ChinaMines",
                    "Upgrade_ChinaSatelliteHackTwo",
                ])
            && unit
                .behavior_modules
                .iter()
                .any(|module| module.family == "InternetHackContain")
            && contain.capacity == 8
            && contain.allowed == ["MONEY_HACKER"]
            && !contain.weapons.fire
            && contain.exit_frames == 15
            && contain.regen_percent.to_bits() == 10.0_f32.to_bits()
            && contain.death.damage.to_bits() == 0.5_f32.to_bits(),
        "Internet Center source economy, capacity, capture/healing or research commands differ",
    )?;
    let hacker = type_index(game, "ChinaInfantryHacker")?;
    require(
        game.catalog[hacker]
            .behavior
            .hacking
            .as_ref()
            .is_some_and(|rules| rules.contained_delay == 54),
        "Hacker lacks native fast contained delay",
    )
}

fn deposited_this_frame(game: &Skirmish) -> u32 {
    game.presentation
        .hacking_events
        .iter()
        .filter_map(|event| {
            if event.frame == game.tick
                && let Effect::Cash { amount, .. } = event.effect
            {
                Some(amount)
            } else {
                None
            }
        })
        .sum()
}

fn type_index(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing original {name}").into())
}

fn fund(game: &mut Skirmish, index: usize) -> Result<(), Box<dyn Error>> {
    let cost = game.catalog[index].cost;
    advance_until(
        game,
        36_000,
        "normal income funding Internet Center economy",
        |game| game.players[0].cash >= cost,
    )
}
