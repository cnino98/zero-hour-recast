//! Installed maintenance exercised through ordinary player commands.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;
#[cfg(test)]
mod tests;
mod upgrades;
pub use upgrades::prepare;

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    for faction in [Faction::Usa, Faction::China, Faction::Gla] {
        game.reset(faction, Faction::Usa)?;
        let builder = friendly_entity(game, Kind::Builder, 0)?;
        if faction != Faction::Gla {
            construct(game, builder, Kind::PowerPlant, 0)?;
        }
        let barracks = construct(game, builder, Kind::Barracks, 0)?;
        let infantry = capture_infantry(game, barracks, 0)?;
        research(game, barracks, "Upgrade_InfantryCaptureBuilding")?;
        if faction == Faction::China {
            let center = friendly_entity(game, Kind::CommandCenter, 0)?;
            research(game, center, "Upgrade_ChinaRadar")?;
            require(
                game.object_has_upgrade(center, "Upgrade_ChinaRadar"),
                "China radar research did not attach to its source object",
            )?;
        }
        let oil = scout_oil(game, infantry)?;
        let attacker = enemy_infantry(game)?;
        repair_target(game, builder, attacker, barracks)?;
        if faction == Faction::Gla {
            rebuild(game, builder, attacker, barracks)?;
        }
        let sale = friendly_entity(game, Kind::Barracks, 0)?;
        let before = game.players[0].cash;
        let rules = &game.catalog[game.entities[sale].type_index];
        let cost = f32::from(u16::try_from(rules.cost)?);
        let refund = if rules.maintenance.refund == 0 {
            (cost * rules.maintenance.sell_fraction)
                .trunc()
                .to_string()
                .parse::<u32>()?
        } else {
            rules.maintenance.refund
        };
        game.sell(0, sale)?;
        advance_until(game, 300, "original structure sale completion", |game| {
            !game.entities[sale].alive()
        })?;
        require(
            game.players[0].cash == before + refund,
            "installed sale refund differs from its source percentage",
        )?;
        capture_oil(game, infantry, oil)?;
        let name = game.catalog[game.entities[infantry].type_index]
            .name
            .as_str();
        println!(
            "Installed maintenance: {name} research, repair, sale, capture and oil income passed"
        );
    }
    upgrades::verify(game)?;
    Ok(())
}

fn research(game: &mut Skirmish, producer: usize, name: &str) -> Result<(), Box<dyn Error>> {
    let index = game.catalog[game.entities[producer].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .ok_or("installed research command missing")?;
    game.queue_upgrade(producer, index)?;
    advance_until(game, 3600, "source research completion", |game| {
        game.entities[producer].queue.is_empty()
    })?;
    Ok(())
}

fn capture_infantry(
    game: &mut Skirmish,
    barracks: usize,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let name = match game.players[owner].faction {
        Faction::Usa => "AmericaInfantryRanger",
        Faction::China => "ChinaInfantryRedguard",
        Faction::Gla => "GLAInfantryRebel",
        Faction::Civilian => return Err("Civilian side has no capture infantry".into()),
    };
    let template = crate::strategy_verification::template(game, name)?;
    produce_type(game, barracks, template, owner)
}

fn enemy_infantry(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    capture_infantry(game, barracks, 1)
}

fn retreat(game: &mut Skirmish, attacker: usize) -> Result<(), Box<dyn Error>> {
    let center = friendly_entity(game, Kind::CommandCenter, 1)?;
    let point = game.entities[center].position;
    game.move_to(attacker, [point[0] + 120.0, point[1]])?;
    advance_until(game, 18_000, "attacker returning to its base", |game| {
        matches!(
            game.entities[attacker].ai.order,
            generals_sim::skirmish::Order::Idle
        )
    })?;
    Ok(())
}

fn repair_target(
    game: &mut Skirmish,
    builder: usize,
    attacker: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    game.attack(attacker, target)?;
    advance_until(
        game,
        18_000,
        "ordinary attack damaging repair target",
        |game| game.entities[target].health < game.entities[target].max_health,
    )
    .map_err(|error| format!("{error}; {}", attack_state(game, attacker, target)))?;
    let cash = game.players[0].cash;
    game.repair(builder, target)?;
    require(game.players[0].cash == cash, "repair charged money")?;
    retreat(game, attacker)?;
    advance_until(
        game,
        18_000,
        "builder repairing original structure",
        |game| game.entities[target].health.to_bits() == game.entities[target].max_health.to_bits(),
    )?;
    Ok(())
}

fn remove_repairer(
    game: &mut Skirmish,
    attacker: usize,
    builder: usize,
) -> Result<(), Box<dyn Error>> {
    // An idle Worker legitimately repairs faster than this Ranger damages the barracks.
    // Remove that defender through normal combat before testing the free hole reconstruction.
    game.move_to(attacker, game.entities[builder].position)?;
    advance_until(game, 18_000, "Ranger scouting the repair Worker", |game| {
        game.logically_visible(game.entities[attacker].owner, builder)
    })?;
    game.attack(attacker, builder)?;
    advance_until(game, 18_000, "Ranger defeating the repair Worker", |game| {
        !game.entities[builder].alive()
    })
    .map_err(|error| format!("{error}; {}", attack_state(game, attacker, builder)))?;
    Ok(())
}

fn rebuild(
    game: &mut Skirmish,
    builder: usize,
    attacker: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    remove_repairer(game, attacker, builder)?;
    game.attack(attacker, target)?;
    advance_until(
        game,
        30_000,
        "ordinary attack exposing the GLA hole",
        |game| !game.entities[target].alive(),
    )
    .map_err(|error| format!("{error}; {}", attack_state(game, attacker, target)))?;
    let hole = game
        .entities
        .iter()
        .position(|entity| {
            entity.alive()
                && entity.owner == 0
                && !game.catalog[entity.type_index]
                    .maintenance
                    .rebuild
                    .is_empty()
        })
        .ok_or("destroyed GLA structure did not create its original hole")?;
    let original_type = game.entities[target].type_index;
    retreat(game, attacker)?;
    let cash = game.players[0].cash;
    advance_until(
        game,
        9000,
        "original GLA hole rebuilding its structure",
        |game| {
            !game.entities[hole].alive()
                && game.entities.iter().enumerate().any(|(id, entity)| {
                    id != target
                        && entity.owner == 0
                        && entity.type_index == original_type
                        && entity.complete()
                })
        },
    )?;
    require(
        game.players[0].cash == cash,
        "GLA hole reconstruction charged player money",
    )?;
    println!(
        "Installed maintenance: GLA worker/hole/foundation reconstructed through normal destruction"
    );
    Ok(())
}

fn attack_state(game: &Skirmish, attacker: usize, target: usize) -> String {
    let actor = &game.entities[attacker];
    let victim = &game.entities[target];
    let order = match actor.ai.order {
        generals_sim::skirmish::Order::Attack(_) => "attack",
        generals_sim::skirmish::Order::Idle => "idle",
        _ => "other",
    };
    let repairing: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter_map(|(id, entity)| (entity.repair_task_target() == Some(target)).then_some(id))
        .collect();
    format!(
        "attacker {attacker} {} hp={} at {:?}, order={order}, facing={}, weapon={:?}; target {target} hp={} at {:?}; repairing={repairing:?}",
        game.catalog[actor.type_index].name,
        actor.health,
        actor.position,
        actor.facing,
        actor.weapon_condition(),
        victim.health,
        victim.position
    )
}

fn scout_oil(game: &mut Skirmish, infantry: usize) -> Result<usize, Box<dyn Error>> {
    let oil = crate::neutral_tech_verification::civilian_structure(game, "TechOilDerrick")?;
    let position = game.entities[oil].position;
    game.move_to(infantry, [position[0] + 75.0, position[1]])?;
    advance_until(
        game,
        18_000,
        "infantry revealing original oil derrick",
        |game| game.can_capture(infantry, oil),
    )?;
    Ok(oil)
}

fn capture_oil(game: &mut Skirmish, infantry: usize, oil: usize) -> Result<(), Box<dyn Error>> {
    let cash = game.players[0].cash;
    let source = game.catalog[game.entities[oil].type_index]
        .maintenance
        .deposits
        .first()
        .ok_or("oil has no deposit module")?;
    let bonus = u32::from_ne_bytes(source.first_capture.to_ne_bytes());
    let amount = u32::from_ne_bytes(source.amount.to_ne_bytes());
    let interval = source.interval;
    game.capture(infantry, oil)?;
    capture_sequence(game, infantry, oil)?;
    require(
        game.players[0].cash == cash + bonus,
        "original first-capture bonus missing",
    )?;
    for elapsed in 0..interval {
        game.step();
        if elapsed == 0 {
            require(
                game.presentation.capture_events.iter().any(|event| {
                    event.frame == game.tick
                        && event.target == oil
                        && matches!(
                            event.effect,
                            generals_sim::skirmish::CaptureEffect::Detected
                        )
                }),
                "captured oil lacks its next-frame native defection ding/white flash",
            )?;
        }
    }
    require(
        game.players[0].cash == cash + bonus + amount,
        "captured oil did not deposit its original periodic amount",
    )?;
    Ok(())
}

pub fn prepare_capture(game: &mut Skirmish) -> Result<[usize; 2], Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::Gla)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let infantry = capture_infantry(game, barracks, 0)?;
    research(game, barracks, "Upgrade_InfantryCaptureBuilding")?;
    let target = scout_oil(game, infantry)?;
    game.stop(infantry)?;
    Ok([infantry, target])
}

fn capture_sequence(
    game: &mut Skirmish,
    actor: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    use generals_sim::skirmish::{
        CaptureEffect,
        special_ability::{FeedbackEffect, PowerType},
    };
    let mut flashes = 0;
    for _ in 0..18_000 {
        game.step();
        flashes += game
            .presentation
            .ability_feedback
            .iter()
            .filter(|event| {
                event.object == actor
                    && event.frame == game.tick
                    && matches!(event.effect, FeedbackEffect::CaptureFlash)
            })
            .count();
        if game.entities[target].owner == game.entities[actor].owner {
            break;
        }
    }
    require(
        game.entities[target].owner == game.entities[actor].owner,
        "installed infantry capture did not complete",
    )?;
    let rules = game.catalog[game.entities[actor].type_index]
        .behavior
        .special_abilities
        .iter()
        .find(|rules| {
            rules.power.as_ref().is_some_and(|power| {
                matches!(
                    power.kind,
                    PowerType::SpecialInfantryCaptureBuilding
                        | PowerType::SpecialBlacklotusCaptureBuilding
                )
            })
        })
        .ok_or("capture module missing")?;
    require(
        !rules.preparation.capture_fx || flashes > 0,
        "authored DoCaptureFX never emitted original flash ticks",
    )?;
    require(
        game.presentation.capture_events.iter().any(|event| {
            event.object == target
                && event.target == target
                && event.frame == game.tick
                && matches!(event.effect, CaptureEffect::Captured)
        }),
        "source Object::defect capture callback absent",
    )?;
    require(
        game.entities[actor].capture_condition() == Some("PACKING"),
        "source successful capture did not enter packing",
    )?;
    println!(
        "Installed capture: {} emitted {flashes} source float-phase flashes, transfer and successful pack callbacks",
        game.catalog[game.entities[actor].type_index].name
    );
    Ok(())
}
