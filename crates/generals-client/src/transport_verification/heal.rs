//! Paid base barracks and infantry, using ordinary approach and healing exits.
use super::{
    Error, Faction, Kind, Skirmish, advance_until, construct, friendly_entity, produce_type,
    require,
};

pub(super) fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    for (faction, name) in [
        (Faction::Usa, "AmericaInfantryRanger"),
        (Faction::China, "ChinaInfantryRedguard"),
        (Faction::Gla, "GLAInfantryRebel"),
    ] {
        game.reset(faction, Faction::China)?;
        let builder = friendly_entity(game, Kind::Builder, 0)?;
        if faction != Faction::Gla {
            construct(game, builder, Kind::PowerPlant, 0)?;
        }
        let host = construct(game, builder, Kind::Barracks, 0)?;
        let index = game
            .catalog
            .iter()
            .position(|unit| unit.name == name)
            .ok_or("missing installed healing infantry")?;
        let rider = produce_type(game, host, index, 0)?;
        let rules = game.catalog[game.entities[host].type_index]
            .contain
            .as_ref()
            .and_then(|rules| rules.heal.as_ref())
            .ok_or("barracks lacks HealContain")?;
        require(
            rules.full_heal_frames == 60,
            "installed barracks changed two-second full-heal duration",
        )?;
        require(
            !game.can_enter(rider, host),
            "full-health infantry entered healing barracks",
        )?;
        injure(game, host, rider)?;
        game.enter(rider, host)?;
        advance_until(
            game,
            3000,
            "injured infantry entering paid barracks",
            |game| game.entities[rider].contained_by() == Some(host),
        )?;
        require(
            game.entities[rider].masked()
                && !game.entities[rider].selectable(&game.catalog[game.entities[rider].type_index]),
            "healing infantry remained visible/selectable",
        )?;
        let entered = game.tick;
        for _ in 0..58 {
            game.step();
        }
        require(
            game.entities[rider].contained_by() == Some(host),
            "healed infantry exited before its source elapsed-time deadline",
        )?;
        advance_until(game, 4, "automatic barracks healing exit", |game| {
            game.entities[rider].contained_by().is_none()
        })?;
        require(
            game.tick - entered <= 60
                && game.entities[rider].health.to_bits()
                    == game.entities[rider].max_health.to_bits()
                && !game.entities[rider].masked(),
            "barracks automatic full-heal exit changed",
        )?;
    }
    println!(
        "Installed barracks healing: paid USA/China/GLA infantry, full-health admission rejection, enclosed approach, two-second elapsed healing and automatic authored exit passed"
    );
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    let host = construct(game, builder, Kind::Barracks, 0)?;
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "AmericaInfantryRanger")
        .ok_or("missing installed Ranger")?;
    let rider = produce_type(game, host, index, 0)?;
    let position = game.entities[host].position;
    game.move_to(rider, [position[0] + 100.0, position[1]])?;
    advance_until(game, 3000, "paid Ranger clearing barracks exit", |game| {
        !game.entities[rider].moving()
            && matches!(
                game.entities[rider].ai.order,
                generals_sim::skirmish::Order::Idle
            )
    })?;
    injure(game, host, rider)?;
    Ok((host, rider))
}

fn injure(game: &mut Skirmish, host: usize, rider: usize) -> Result<(), Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaInfantryRedguard")
        .ok_or("missing installed enemy Red Guard")?;
    let enemy = produce_type(game, barracks, index, 1)?;
    game.attack(enemy, rider)?;
    advance_until(
        game,
        18000,
        "ordinary enemy rifle damaging healing infantry",
        |game| game.entities[rider].health < game.entities[rider].max_health,
    )?;
    require(
        game.entities[rider].alive(),
        "healing infantry died in rifle damage trial",
    )?;
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    game.move_to(enemy, [base[0] + 120.0, base[1]])?;
    // The injured Red Guard can pursue its attacker by default. Give both
    // infantry their retreat orders before waiting for the rifle to clear.
    let position = game.entities[host].position;
    game.move_to(rider, [position[0] + 100.0, position[1]])?;
    advance_until(
        game,
        9000,
        "enemy rifle threat clearing from the barracks healing trial",
        |game| {
            let from = game.entities[enemy].position;
            let to = game.entities[rider].position;
            // China trains two Red Guards per production command. The other
            // defender can kill the rifleman on retreat before he clears range.
            (!game.entities[enemy].alive() || (from[0] - to[0]).hypot(from[1] - to[1]) > 300.0)
                && game
                    .projectiles
                    .iter()
                    .all(|projectile| projectile.source != enemy)
        },
    )
    .map_err(|error| {
        let detail = generals_sim::skirmish::diagnostics::movement_details;
        format!(
            "{error}; attacker {}; injured infantry {}",
            detail(game, enemy),
            detail(game, rider)
        )
    })?;
    advance_until(
        game,
        9000,
        "injured infantry returning to barracks entry",
        |game| {
            !game.entities[rider].moving()
                && matches!(
                    game.entities[rider].ai.order,
                    generals_sim::skirmish::Order::Idle
                )
        },
    )?;
    require(
        game.entities[rider].alive()
            && game.entities[rider].health < game.entities[rider].max_health,
        "rifle damage trial did not leave a living injured infantryman",
    )?;
    Ok(())
}
