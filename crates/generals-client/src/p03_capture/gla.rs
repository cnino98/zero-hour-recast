use super::Scene;
use crate::{
    skirmish_verification::{advance_until, construct, construct_type, friendly_entity},
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, Skirmish, visibility::Status};
use std::error::Error;

pub const SCUD: &str = "SuperweaponScudStorm";

pub fn gla_technology(game: &mut Skirmish) -> Result<(usize, usize, usize, usize), Box<dyn Error>> {
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let palace = construct_type(game, worker, template(game, "GLAPalace")?, 0)?;
    Ok((worker, barracks, factory, palace))
}

pub fn build_scud(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (worker, _, _, _) = gla_technology(game)?;
    let storm_type = template(game, "GLAScudStorm")?;
    let cost = game.catalog[storm_type].cost;
    advance_until(game, 18_000, "supplies funding Scud Storm", |game| {
        game.players[0].cash >= cost
    })?;
    Ok((construct_type(game, worker, storm_type, 0)?, worker))
}

pub(super) fn prepare(game: &mut Skirmish) -> Result<Scene, Box<dyn Error>> {
    if game.players[0].faction != Faction::Gla {
        return Err("Scud Storm capture requires a GLA match".into());
    }
    let (provider, worker) = build_scud(game)?;
    let target = scout_scud_target(game, worker)?;
    advance_until(game, 9001, "Scud Storm countdown", |game| {
        game.can_use_power_at_location(provider, SCUD, target)
    })?;
    Ok(Scene {
        provider,
        power: SCUD,
        target,
        subject: provider,
    })
}

pub fn scout_scud_target(game: &mut Skirmish, worker: usize) -> Result<[f32; 3], Box<dyn Error>> {
    let home = friendly_entity(game, Kind::CommandCenter, 0)?;
    let retreat = game.entities[worker].position;
    let point = target_site(game, home, worker).ok_or("no reachable Scud Storm capture site")?;
    if matches!(game.visibility_cell(0, point), Status::Shrouded) {
        game.move_to(worker, point)?;
        advance_until(
            game,
            7200,
            "Worker revealing the Scud Storm target",
            |game| !matches!(game.visibility_cell(0, point), Status::Shrouded),
        )?;
        game.move_to(worker, retreat)?;
    }
    Ok([point[0], point[1], game.ground_height(point)])
}

fn target_site(game: &Skirmish, home: usize, worker: usize) -> Option<[f32; 2]> {
    let base = game.entities[home].position;
    let distance = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let cell = game.visibility_cell_size();
    let [width, height] = game.visibility_dimensions();
    let axis = |cells| (0..cells).map(move |i| (f32::from(i) + 0.5) * cell);
    let mut candidates: Vec<_> = axis(height)
        .flat_map(|y| axis(width).map(move |x| [x, y]))
        .filter(|&point| {
            let radial = distance(point, base);
            (320.0..620.0).contains(&radial)
                && game.entities.iter().all(|actor| {
                    actor.owner != 0 || !actor.alive() || distance(point, actor.position) >= 260.0
                })
        })
        .collect();
    candidates.sort_by(|&a, &b| distance(a, base).total_cmp(&distance(b, base)));
    candidates
        .into_iter()
        .find(|&point| game.build_site_reachable(worker, point))
}
