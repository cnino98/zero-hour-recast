//! Paid capture on original Rocky Rampage and El Scorcho objects.
use crate::skirmish_verification::{advance_until, construct, friendly_entity, produce, require};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

#[must_use]
pub const fn map_name(refinery: bool) -> &'static str {
    if refinery {
        "Maps\\El Scorcho\\El Scorcho.map"
    } else {
        "Maps\\Rocky Rampage\\Rocky Rampage.map"
    }
}

pub fn run(root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    for refinery in [false, true] {
        let bytes = assets.read(map_name(refinery))?.bytes;
        let map = MapData::parse(&bytes)?;
        let mut data = crate::skirmish_data::load(&assets, &map.objects)?;
        drop(crate::control_bar::load(
            &assets,
            &data.types,
            &root.join("UserData/Options.ini"),
        )?);
        drop(crate::source_audio::load(
            &assets,
            &data.types,
            &root.join("UserData/Options.ini"),
        )?);
        let settings = crate::world_data::camera_settings(&assets, root)?;
        let terrain = crate::terrain::Terrain::new(
            map.heightmap,
            map.tiles,
            settings.adjust_cliffs,
            settings.three_way_blends,
        )?;
        let (mut layout, _) = crate::world_data::load(
            &assets,
            &map.objects,
            &terrain,
            &data.types,
            &mut data.models,
        )?;
        let metadata = generals_assets::scripts::map_scripts(&bytes)?;
        crate::map_scripts::load_startup(&metadata, &data.types, &assets)?.apply(&mut layout);
        let animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
        let projectiles =
            crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
        let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
        let mut game = data.into_simulation(movement, layout)?;
        terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
        if refinery {
            verify_refinery(&mut game)?;
        } else {
            verify_hospital(&mut game)?;
        }
    }
    Ok(())
}

pub fn civilian_structure(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    // PlayerList::validateTeam resolves named civilian teams separately from
    // the default neutral team when the map declares that side.
    game.entities
        .iter()
        .position(|actor| {
            actor.alive()
                && game.players[actor.owner].faction == Faction::Civilian
                && game.catalog[actor.type_index].name == name
        })
        .ok_or_else(|| format!("original map lacks civilian structure {name}").into())
}

pub fn prepare(game: &mut Skirmish, refinery: bool) -> Result<(usize, usize), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let name = if refinery {
        "TechOilRefinery"
    } else {
        "TechHospital"
    };
    let target = civilian_structure(game, name)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let ranger = produce(game, barracks, Kind::Infantry, 0)?;
    let research = game.catalog[game.entities[barracks].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_InfantryCaptureBuilding")
        .ok_or("installed capture research missing")?;
    game.queue_upgrade(barracks, research)?;
    advance_until(game, 3600, "paid capture research", |game| {
        game.entities[barracks].queue.is_empty()
    })?;
    let point = game.entities[target].position;
    game.move_to(ranger, [point[0] + 75.0, point[1]])?;
    advance_until(
        game,
        18000,
        "paid Ranger scouting authored tech record",
        |game| game.can_capture(ranger, target),
    )?;
    game.capture(ranger, target)?;
    advance_until(
        game,
        18000,
        "paid Ranger capturing authored tech building",
        |game| game.entities[target].owner == 0,
    )?;
    game.stop(ranger)?;
    require(game.entities[target].alive(), "captured tech building died")?;
    println!(
        "Installed neutral tech: paid Ranger captured original {name} map record at {},{}",
        point[0], point[1]
    );
    Ok((target, ranger))
}

fn verify_refinery(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (refinery, _) = prepare(game, true)?;
    let tank = game.type_for(0, Kind::Tank).ok_or("USA tank missing")?;
    let infantry = game
        .type_for(0, Kind::Infantry)
        .ok_or("USA infantry missing")?;
    require(
        game.build_cost(0, tank) == game.catalog[tank].cost * 9 / 10,
        "captured refinery did not apply original vehicle discount",
    )?;
    require(
        game.build_cost(0, infantry) == game.catalog[infantry].cost,
        "refinery discounted infantry",
    )?;
    require(
        game.entities[refinery].owner == 0,
        "refinery ownership changed",
    )?;
    println!(
        "Installed Refinery: paid capture applies original vehicle price and excludes infantry"
    );
    Ok(())
}

fn verify_hospital(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (hospital, ranger) = prepare(game, false)?;
    let enemy_builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_builder, Kind::PowerPlant, 1)?;
    let enemy_barracks = construct(game, enemy_builder, Kind::Barracks, 1)?;
    let enemy = produce(game, enemy_barracks, Kind::Infantry, 1)?;
    game.attack(enemy, ranger)?;
    advance_until(
        game,
        18000,
        "real rifle injury under hospital ownership",
        |game| game.entities[ranger].health < game.entities[ranger].max_health,
    )?;
    let injured_health = game.entities[ranger].health;
    game.stop(enemy)?;
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    game.move_to(enemy, base)?;
    advance_until(
        game,
        9000,
        "enemy rifle retreating from hospital trial",
        |game| {
            let a = game.entities[enemy].position;
            let b = game.entities[ranger].position;
            (a[0] - b[0]).hypot(a[1] - b[1]) > 300.0
                && game
                    .projectiles
                    .iter()
                    .all(|projectile| projectile.source != enemy)
        },
    )?;
    require(
        game.entities[ranger].alive(),
        "hospital trial infantry died",
    )?;
    let health = game.entities[ranger].health;
    if health <= injured_health {
        advance_until(game, 31, "hospital original healing pulse", |game| {
            game.entities[ranger].health > health
        })?;
    }
    require(
        game.entities[hospital].owner == 0,
        "hospital healing ownership lost",
    )?;
    println!(
        "Installed Hospital: actual paid infantry rifle damage and original whole-player healing passed"
    );
    Ok(())
}
