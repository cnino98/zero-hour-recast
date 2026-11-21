//! Original paid China launcher ready for the command-bar launch capture.
use super::Scene;
use crate::{
    china_power_verification::{MISSILE_POWER, prepare_launcher, scout_launcher_target},
    skirmish_verification::{advance_until, friendly_entity, require},
};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub(super) fn prepare(game: &mut Skirmish) -> Result<Scene, Box<dyn Error>> {
    let (provider, builder, _) = prepare_launcher(game)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    scout_launcher_target(game, builder, target)?;
    advance_until(game, 10_801, "normal nuclear launcher cooldown", |game| {
        game.can_use_power(provider, MISSILE_POWER)
    })?;
    let target = game.object_position(target);
    require(
        game.model_flags(&game.entities[provider], [0.5, 0.25])
            .contains("DOOR_1_WAITING_OPEN")
            && game.can_use_power_at_location(provider, MISSILE_POWER, target),
        "the launch-ready silo or its scouted target was unavailable",
    )?;
    Ok(Scene {
        provider,
        power: MISSILE_POWER,
        target,
        subject: provider,
    })
}
