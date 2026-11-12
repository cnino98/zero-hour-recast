//! Paid Stinger construction, original station soldiers and missile behavior.
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::{error::Error, path::Path};
pub mod camouflage;

pub struct Setup {
    pub site: usize,
    pub soldiers: Vec<usize>,
    pub enemy: usize,
}

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
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let setup = prepare_quiet(game)?;
    camouflage::verify(game, setup.site)?;
    approach(game, &setup)?;
    advance_until(game, 18000, "enemy approaching Stinger", |game| {
        ready_to_attack(game, &setup)
    })?;
    engage(game, &setup)?;
    Ok(setup)
}

pub fn prepare_quiet(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Barracks, 0)?;
    let site_type = template(game, "GLAStingerSite")?;
    let site = construct_type(game, worker, site_type, 0)?;
    advance_until(game, 60, "Stinger initial soldiers", |game| {
        game.spawned_children(site).len() == 3
    })?;
    let soldiers = game.spawned_children(site).to_vec();
    require(
        game.catalog[site_type].cost == 900
            && game.catalog[site_type]
                .behavior
                .spawns
                .first()
                .is_some_and(|rules| rules.replace_frames == 900)
            && game.catalog[site_type]
                .behavior
                .spawn_exits
                .first()
                .is_some_and(|points| points.len() == 3),
        "Stinger source price/replacement/stations differ",
    )?;
    for &child in &soldiers {
        require(
            game.catalog[game.entities[child].type_index].name == "GLAInfantryStingerSoldier"
                && game.entities[child].disabled_held()
                && game.entities[child].drone_parent() == Some(site),
            "Stinger child lacks its held station or native slaver",
        )?;
    }
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::PowerPlant, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    let factory = construct(game, worker, Kind::Factory, 1)?;
    let enemy = produce_type(game, factory, template(game, "AmericaTankCrusader")?, 1)?;
    Ok(Setup {
        site,
        soldiers,
        enemy,
    })
}

pub fn approach(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let center = game.entities[setup.site].position;
    game.move_to(setup.enemy, [center[0] + 120.0, center[1]])?;
    Ok(())
}

pub fn ready_to_attack(game: &Skirmish, setup: &Setup) -> bool {
    setup
        .soldiers
        .iter()
        .all(|&id| game.weapon_target_reachable(id, setup.enemy))
}

pub fn engage(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    game.stop(setup.enemy)?;
    game.attack(setup.site, setup.enemy)?;
    Ok(())
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let positions: Vec<_> = setup
        .soldiers
        .iter()
        .map(|&id| game.entities[id].position)
        .collect();
    let before = game.entities[setup.enemy].health;
    let mut launches = std::collections::BTreeSet::new();
    for _ in 0..600 {
        game.step();
        for shot in &game.presentation.shots {
            if setup.soldiers.contains(&shot.source) {
                launches.insert(shot.source);
            }
        }
        if launches.len() == 3 && game.entities[setup.enemy].health < before {
            break;
        }
    }
    if launches.len() != 3 {
        eprintln!(
            "Stinger target position={:?} health={before}/{} launches={launches:?}",
            game.entities[setup.enemy].position, game.entities[setup.enemy].health
        );
        for &id in &setup.soldiers {
            eprintln!(
                "Stinger soldier {id} position={:?} health={} reachable={}",
                game.entities[id].position,
                game.entities[id].health,
                game.weapon_target_reachable(id, setup.enemy)
            );
        }
    }
    require(
        launches.len() == 3,
        "not all three original Stinger soldiers fired",
    )?;
    require(
        game.entities[setup.enemy].health < before,
        "Stinger missiles caused no damage",
    )?;
    require(
        !game.entities[setup.site].stealthed(),
        "Stinger stayed concealed while its spawned weapons attacked",
    )?;
    for (&id, position) in setup.soldiers.iter().zip(positions) {
        require(
            game.entities[id].position.map(f32::to_bits) == position.map(f32::to_bits),
            "held Stinger soldier left its station",
        )?;
    }
    game.stop(setup.site)?;
    println!(
        "Paid Stinger construction, authored stations, three held/slaved soldiers, native site attack dispatch, actual missile hits and stop passed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_stinger_keeps_three_stations_and_fires_original_missiles() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
