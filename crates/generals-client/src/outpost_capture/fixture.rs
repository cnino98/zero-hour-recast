use crate::skirmish_verification::{advance_until, require};
use generals_sim::skirmish::Skirmish;
use std::error::Error;

pub(super) struct Setup {
    pub outpost: usize,
    pub riders: Vec<usize>,
    pub enemies: [usize; 2],
}

pub(super) fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let outpost = crate::outpost_verification::prepare(game)?;
    let riders = game.passengers(outpost).to_vec();
    let enemies = crate::outpost_verification::opponents(game)?;
    let target = enemies[0];
    let position = game.entities[target].position;
    game.move_to(outpost, position)?;
    advance_until(
        game,
        18_000,
        "paid Outpost approaching enemy within its original route-reveal vision",
        |game| {
            let point = game.entities[outpost].position;
            (point[0] - position[0]).hypot(point[1] - position[1]) < 245.0
        },
    )?;
    game.stop(outpost)?;
    advance_until(
        game,
        300,
        "staged Outpost concealing before original UI acceptance",
        |game| game.entities[outpost].stealthed(),
    )?;
    require(
        game.entities[outpost].alive()
            && enemies.iter().all(|&id| game.entities[id].alive())
            && game.logically_visible(0, target),
        "Outpost native staging lost its live visible encounter",
    )?;
    Ok(Setup {
        outpost,
        riders,
        enemies,
    })
}
