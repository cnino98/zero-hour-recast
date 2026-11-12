//! Ordinary paid tank combat; no altered health, weapons or light lifetimes.
use crate::skirmish_verification::{
    advance_until, construct, friendly_entity, produce_type, require,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub(super) struct Setup {
    pub attacker: usize,
    pub target: usize,
    pub focus: [f32; 3],
    pub health: f32,
}

pub(super) fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let kind = crate::strategy_verification::template(game, "AmericaTankCrusader")?;
    let attacker = produce_type(game, factory, kind, 0)?;
    let target = produce_type(game, factory, kind, 0)?;
    let home = game.entities[factory].position;
    stage(game, target, [home[0] + 240.0, home[1] + 180.0])?;
    let point = game.entities[target].position;
    stage(game, attacker, [point[0] - 120.0, point[1]])?;
    require(
        game.entities[attacker].alive()
            && game.entities[target].alive()
            && game.force_attack_target_reachable(attacker, target),
        "paid Crusaders lack an ordinary force-attack line",
    )?;
    Ok(Setup {
        attacker,
        target,
        focus: game.object_position(target),
        health: game.entities[target].health,
    })
}

fn stage(game: &mut Skirmish, actor: usize, requested: [f32; 2]) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, requested)?;
    let accepted = game.entities[actor].ai_goal_position();
    advance_until(game, 18_000, "paid LightPulse tank staging", |game| {
        let point = game.entities[actor].position;
        (point[0] - accepted[0]).hypot(point[1] - accepted[1]) < 20.0
    })?;
    game.stop(actor)?;
    Ok(())
}
