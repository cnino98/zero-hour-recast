//! Paid Listening Outpost payload, concealment, detector and physical death.
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type, require},
    strategy_verification::template,
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Skirmish, stealth};
use std::{error::Error, path::Path};

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    let map = MapData::parse(&assets.read(MAP_NAME)?.bytes)?;
    let mut data = skirmish_data::load(&assets, &map.objects)?;
    let settings = world_data::camera_settings(&assets, root)?;
    let terrain = Terrain::new(
        map.heightmap,
        map.tiles,
        settings.adjust_cliffs,
        settings.three_way_blends,
    )?;
    let (layout, _) = world_data::load(
        &assets,
        &map.objects,
        &terrain,
        &data.types,
        &mut data.models,
    )?;
    let animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::input::load(&assets, &options, &data.types)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    drop(crate::physical_debris::load(
        &assets,
        &data.types,
        &mut data.models,
    )?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

pub fn prepare(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let index = template(game, "ChinaVehicleListeningOutpost")?;
    let rules = &game.catalog[index];
    require(
        rules.cost == 800
            && rules.max_health.to_bits() == 240.0_f32.to_bits()
            && rules.build_seconds.to_bits() == 15.0_f32.to_bits(),
        "Outpost original cost/body/build duration differs",
    )?;
    let outpost = produce_type(game, factory, index, 0)?;
    require(
        game.passengers(outpost).len() == 2
            && game.passengers(outpost).iter().all(|&id| {
                game.catalog[game.entities[id].type_index].name == "ChinaInfantryTankHunter"
            }),
        "paid Outpost lacks two original Tank Hunters",
    )?;
    require(
        game.entities[outpost]
            .active_weapon(&game.catalog[index])
            .name
            == "ListeningOutpostUpgradedDummyWeapon",
        "armed initial payload did not enable original dummy weapon set",
    )?;
    Ok(outpost)
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let outpost = prepare(game)?;
    let riders = game.passengers(outpost).to_vec();
    game.stop(outpost)?;
    advance_until(game, 300, "native stationary Outpost concealment", |game| {
        game.entities[outpost].stealthed()
    })?;
    advance_until(game, 60, "Outpost original detector callback", |game| {
        game.presentation.stealth_events.iter().any(|event|
        matches!(event, stealth::Event::Scan { detector, feedback: Some(_), .. } if *detector == outpost))
    })?;
    game.evacuate(outpost)?;
    advance_until(game, 600, "two original Outpost exit paths", |game| {
        riders
            .iter()
            .all(|&id| game.entities[id].contained_by().is_none())
    })?;
    let index = game.entities[outpost].type_index;
    require(
        game.entities[outpost]
            .active_weapon(&game.catalog[index])
            .name
            .is_empty(),
        "empty Outpost kept its rider weapon upgrade",
    )?;
    for &rider in &riders {
        game.enter(rider, outpost)?;
    }
    advance_until(game, 1200, "Outpost context reentry", |game| {
        game.passengers(outpost).len() == 2
    })?;
    battle_death(game, outpost, &riders)?;
    println!(
        "Original Outpost paid production, armed payload, exits, concealment, scan, rider combat and physical death passed"
    );
    Ok(())
}

fn battle_death(
    game: &mut Skirmish,
    outpost: usize,
    riders: &[usize],
) -> Result<(), Box<dyn Error>> {
    let enemies = opponents(game)?;
    let target = enemies[0];
    let position = game.entities[target].position;
    game.move_to(outpost, position)?;
    advance_until(
        game,
        18_000,
        "moving Outpost entering real enemy weapon range",
        |game| {
            game.logically_visible(1, outpost)
                && enemies.iter().any(|&id| {
                    game.entities[id]
                        .position
                        .into_iter()
                        .zip(game.entities[outpost].position)
                        .map(|(left, right)| (left - right) * (left - right))
                        .sum::<f32>()
                        < 200.0 * 200.0
                })
        },
    )?;
    for &enemy in &enemies {
        game.attack(enemy, outpost)?;
    }
    game.attack(outpost, target)?;
    let mut fired = false;
    let deadline = game.tick + 3000;
    while game.entities[outpost].alive() && game.tick < deadline {
        game.step();
        fired |= game
            .presentation
            .shots
            .iter()
            .any(|shot| riders.contains(&shot.source));
    }
    require(
        fired,
        "contained Tank Hunters did not fire against the actual hostile tank",
    )?;
    require(
        !game.entities[outpost].alive(),
        "paid Crusaders did not destroy the Outpost",
    )?;
    require(
        game.entities[outpost].corpse_present(),
        "Outpost skipped its original delayed death body",
    )?;
    let first = game.tick;
    advance_until(
        game,
        120,
        "Outpost original final OCL/hull callback",
        |game| !game.entities[outpost].corpse_present(),
    )?;
    require(
        game.tick - first == 54,
        "Outpost original 1789ms death delay changed",
    )?;
    require(
        game.entities.iter().any(|actor| {
            game.catalog[actor.type_index].name == "ChinaVehicleListeningOutpostDeadHull"
        }),
        "Outpost final physical hull was never created",
    )?;
    Ok(())
}

pub fn opponents(game: &mut Skirmish) -> Result<[usize; 2], Box<dyn Error>> {
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::PowerPlant, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    let factory = construct(game, worker, Kind::Factory, 1)?;
    let tank = template(game, "AmericaTankCrusader")?;
    Ok([
        produce_type(game, factory, tank, 1)?,
        produce_type(game, factory, tank, 1)?,
    ])
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_outpost_uses_original_payload_concealment_scan_and_physical_death() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
