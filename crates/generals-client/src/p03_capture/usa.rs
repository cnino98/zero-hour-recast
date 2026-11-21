use super::Scene;
use crate::skirmish_verification::friendly_entity;
use crate::usa_research_verification::{PARTICLE_POWER, prepare_particle_uplink};
use generals_sim::skirmish::{Kind, Skirmish};
use std::error::Error;

pub(super) fn prepare(game: &mut Skirmish) -> Result<Scene, Box<dyn Error>> {
    let provider = prepare_particle_uplink(game)?;
    let scout = friendly_entity(game, Kind::Tank, 0)?;
    let xy = crate::usa_research_verification::scout_open_target(game, provider, scout)?;
    Ok(Scene {
        provider,
        power: PARTICLE_POWER,
        target: [xy[0], xy[1], game.ground_height(xy)],
        subject: provider,
    })
}
