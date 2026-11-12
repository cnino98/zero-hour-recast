//! Paid original Dragon/Inferno production, research and real incendiary payloads.
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::{research, template},
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
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
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    drop(crate::projectile_stream::load(&assets, &data.types)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let dragon = produce_type(game, factory, template(game, "ChinaTankDragon")?, 0)?;
    let inferno = template(game, "ChinaVehicleInfernoCannon")?;
    let cash = game.players[0].cash;
    require(
        game.train(factory, inferno).is_err() && game.players[0].cash == cash,
        "Inferno bypassed native Propaganda Center prerequisite",
    )?;
    fire_wall(game, dragon, "FireWallSegment")?;
    research(game, factory, "Upgrade_ChinaBlackNapalm")?;
    require(
        game.entities[dragon]
            .active_weapon(&game.catalog[game.entities[dragon].type_index])
            .name
            .ends_with("Upgraded"),
        "Black Napalm did not replace Dragon weapon",
    )?;
    fire_wall(game, dragon, "FireWallSegmentUpgraded")?;
    construct_type(game, worker, template(game, "ChinaPropagandaCenter")?, 0)?;
    let mut guns = Vec::new();
    for _ in 0..3 {
        guns.push(produce_type(game, factory, inferno, 0)?);
    }
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    for &gun in &guns {
        game.attack(gun, target)?;
    }
    advance_until(
        game,
        18000,
        "native Black Napalm field and three-impact firestorm",
        |game| {
            ["FireFieldUpgradedSmall", "BlackNapalmFirestormSmall"]
                .iter()
                .all(|name| {
                    game.entities
                        .iter()
                        .any(|actor| game.catalog[actor.type_index].name == *name)
                })
        },
    )?;
    let storm = game
        .entities
        .iter()
        .position(|actor| game.catalog[actor.type_index].name == "BlackNapalmFirestormSmall")
        .ok_or("firestorm object missing")?;
    for gun in guns {
        game.stop(gun)?;
    }
    advance_until(game, 300, "native firestorm reversal", |game| {
        game.entities[storm].fire_reversed().is_some()
    })?;
    advance_until(
        game,
        300,
        "native firestorm LifetimeUpdate destruction",
        |game| game.entities[storm].masked(),
    )?;
    println!(
        "Fire: paid Dragon wall, Black Napalm replacement, three paid Infernos, OCL fields, shared historic firestorm, reversal and expiry passed at {}",
        game.tick
    );
    Ok(())
}

fn fire_wall(game: &mut Skirmish, actor: usize, name: &str) -> Result<(), Box<dyn Error>> {
    let position = game.entities[actor].position;
    let point = [position[0], position[1] + 65.0];
    game.fire_weapon_at(actor, 1, point)?;
    advance_until(
        game,
        1800,
        "original Dragon secondary projectile OCL",
        |game| {
            game.entities
                .iter()
                .any(|entity| game.catalog[entity.type_index].name == name)
        },
    )?;
    game.stop(actor)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_original_incendiary_production_and_payloads() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
