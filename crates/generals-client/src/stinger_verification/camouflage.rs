use super::{Error, Kind, Skirmish, advance_until, construct_type, friendly_entity, require};

pub(super) fn verify(game: &mut Skirmish, site: usize) -> Result<(), Box<dyn Error>> {
    let index = research(game, site)?;
    let owner = game.entities[site].owner;
    let cash = game.players[owner].cash;
    game.queue_upgrade(site, index)?;
    require(
        game.players[owner].cash == cash - 500,
        "CamoNetting did not charge $500",
    )?;
    require(
        !game.can_research(site, index),
        "CamoNetting accepted a duplicate object upgrade",
    )?;
    for _ in 0..60 {
        game.step();
    }
    require(
        !game.object_has_upgrade(site, NAME),
        "CamoNetting completed before five seconds",
    )?;
    let cash = game.players[owner].cash;
    game.cancel_upgrade(owner, site, index)?;
    require(
        game.players[owner].cash == cash + 500,
        "CamoNetting cancellation lost its original refund",
    )?;
    for _ in 0..90 {
        game.step();
    }
    require(
        !concealed(game, site),
        "Cancelled CamoNetting granted stealth",
    )?;
    crate::strategy_verification::research(game, site, NAME)?;
    advance_until(
        game,
        90,
        "CamoNetting concealing the original site and soldiers",
        |game| concealed(game, site),
    )?;
    require(
        !game.has_upgrade(owner, NAME),
        "Object CamoNetting became a player upgrade",
    )?;
    for &child in game.spawned_children(site) {
        require(
            !game.object_has_upgrade(child, NAME),
            "Parent upgrade was copied into a soldier's research inventory",
        )?;
    }
    let builder = friendly_entity(game, Kind::Builder, owner)?;
    let other = construct_type(game, builder, game.entities[site].type_index, owner)?;
    advance_until(game, 90, "unupgraded comparison site soldiers", |game| {
        game.spawned_children(other).len() == 3
    })?;
    for _ in 0..90 {
        game.step();
    }
    require(
        !game.object_has_upgrade(other, NAME) && !concealed(game, other),
        "CamoNetting transferred to another site",
    )?;
    require(
        game.can_research(other, index),
        "Unupgraded site lost its individual research command",
    )?;
    println!(
        "Original CamoNetting: paid $500/five-second object research, duplicate rejection, cancellation/refund, current soldiers concealed and another paid site unupgraded"
    );
    Ok(())
}

pub const NAME: &str = "Upgrade_GLACamoNetting";

pub fn research(game: &Skirmish, site: usize) -> Result<usize, Box<dyn Error>> {
    let upgrades = &game.catalog[game.entities[site].type_index]
        .maintenance
        .research;
    let index = upgrades
        .iter()
        .position(|upgrade| upgrade.name == NAME)
        .ok_or("Stinger lacks original CamoNetting research")?;
    let upgrade = &upgrades[index];
    require(
        upgrade.cost == 500 && upgrade.frames == 150 && !upgrade.player,
        "Stinger CamoNetting original cost/duration/scope differs",
    )?;
    Ok(index)
}

pub fn concealed(game: &Skirmish, site: usize) -> bool {
    game.entities[site].stealthed()
        && !game.entities[site].detected()
        && game.spawned_children(site).len() == 3
        && game
            .spawned_children(site)
            .iter()
            .all(|&child| game.entities[child].stealthed() && !game.entities[child].detected())
}
