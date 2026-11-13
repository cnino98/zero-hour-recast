//! Paid capture and real weapon impact at the original El Scorcho artillery platform.
use crate::skirmish_verification::{advance_until, construct, friendly_entity, produce, require};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::error::Error;

pub const fn map_name() -> &'static str {
    "Maps\\El Scorcho\\El Scorcho.map"
}

pub const fn repair_bay_map_name() -> &'static str {
    "Maps\\Hostile Dawn\\Hostile Dawn.map"
}

pub const fn repair_pad_map_name() -> &'static str {
    "user-maps/repair pad fixture/repair pad fixture.map"
}

pub fn run(root: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )?;
    for (path, bay) in [(map_name(), false), (repair_bay_map_name(), true)] {
        let bytes = assets.read(path)?.bytes;
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
        if bay {
            verify_repair_bay(&mut game)?;
        } else {
            verify_artillery(&mut game)?;
        }
    }
    Ok(())
}

pub fn prepare(game: &mut Skirmish, name: &str) -> Result<(usize, usize), Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let target = crate::neutral_tech_verification::civilian_structure(game, name)?;
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

pub fn prepare_shot(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (platform, ranger) = prepare(game, "TechArtilleryPlatform")?;
    game.stop(ranger)?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    let infantry = produce(game, barracks, Kind::Infantry, 1)?;
    let position = game.entities[platform].position;
    game.move_to(infantry, [position[0] + 150.0, position[1]])?;
    advance_until(
        game,
        18000,
        "captured original artillery platform launching shell",
        |game| {
            game.projectiles.iter().any(|projectile| {
                projectile.source == platform
                    && projectile.render_transform(&game.entities).is_some()
            })
        },
    )?;
    Ok((platform, infantry))
}

fn verify_artillery(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (platform, infantry) = prepare_shot(game)?;
    let health = game.entities[infantry].health;
    advance_until(
        game,
        9000,
        "captured original artillery shell impact on paid enemy",
        |game| game.entities[infantry].health < health,
    )?;
    require(
        game.entities[platform].owner == 0,
        "artillery platform capture lost",
    )?;
    require(
        game.entities[platform].alive(),
        "artillery platform died during trial",
    )?;
    println!(
        "Installed TechArtilleryPlatform: paid original capture and actual StrategyCenterArtilleryShell enemy damage passed"
    );
    Ok(())
}

pub fn prepare_repair_bay(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let (bay, ranger) = prepare(game, "TechRepairbay")?;
    let (vehicle, enemy, factory, enemy_barracks) = prepare_repair_vehicles(game)?;
    let injured_health = injure_bay_vehicle(game, vehicle, enemy, factory)?;
    retreat_repair_attacker(game, vehicle, enemy)?;
    let damaged = game.entities[vehicle].health;
    if damaged <= injured_health {
        advance_until(
            game,
            31,
            "original Repair Bay whole-player vehicle healing pulse",
            |game| game.entities[vehicle].health > damaged,
        )?;
    }
    require(
        game.entities[bay].owner == 0,
        "original Repair Bay capture lost",
    )?;
    let bay_position = game.entities[bay].position;
    let vehicle_position = game.entities[vehicle].position;
    require(
        (bay_position[0] - vehicle_position[0]).hypot(bay_position[1] - vehicle_position[1])
            > 300.0,
        "Repair Bay whole-player vehicle trial remained near building",
    )?;
    println!(
        "Installed TechRepairbay: paid original Hostile Dawn capture, actual paid tank damage and remote whole-player healing passed"
    );

    transfer_repair_bay(game, bay, ranger, enemy_barracks, enemy)?;
    Ok(bay)
}

fn prepare_repair_vehicles(
    game: &mut Skirmish,
) -> Result<(usize, usize, usize, usize), Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let vehicle = produce(game, factory, Kind::Tank, 0)?;
    let enemy_builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_builder, Kind::PowerPlant, 1)?;
    let enemy_barracks = construct(game, enemy_builder, Kind::Barracks, 1)?;
    construct(game, enemy_builder, Kind::SupplyCenter, 1)?;
    let enemy_factory = construct(game, enemy_builder, Kind::Factory, 1)?;
    let enemy = produce(game, enemy_factory, Kind::Tank, 1)?;
    Ok((vehicle, enemy, factory, enemy_barracks))
}

fn retreat_repair_attacker(
    game: &mut Skirmish,
    vehicle: usize,
    enemy: usize,
) -> Result<(), Box<dyn Error>> {
    game.stop(enemy)?;
    game.stop(vehicle)?;
    let enemy_base = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    game.move_to(enemy, enemy_base)?;
    advance_until(
        game,
        18000,
        "enemy tank retreating from original Repair Bay trial",
        |game| {
            let a = game.entities[enemy].position;
            let b = game.entities[vehicle].position;
            (a[0] - b[0]).hypot(a[1] - b[1]) > 400.0
                && game
                    .projectiles
                    .iter()
                    .all(|projectile| projectile.source != enemy)
        },
    )?;
    require(game.entities[vehicle].alive(), "repair trial vehicle died")
}

pub fn prepare_repair_pad(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let (pad, ranger) = prepare(game, "TechRepairPad")?;
    let home = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    game.move_to(ranger, home)?;
    let (vehicle, enemy, factory, _) = prepare_repair_vehicles(game)?;
    injure_bay_vehicle(game, vehicle, enemy, factory)?;
    retreat_repair_attacker(game, vehicle, enemy)?;
    let [x, y] = game.entities[pad].position;
    game.move_to(vehicle, [x + 75.0, y])?;
    advance_until(game, 18_000, "scouting the captured Repair Pad", |game| {
        game.can_repair(vehicle, pad)
    })?;
    let damaged = game.entities[vehicle].health;
    require(
        damaged < game.entities[vehicle].max_health,
        "repair trial lacks vehicle damage",
    )?;
    game.repair(vehicle, pad)?;
    advance_until(
        game,
        18_000,
        "paid tank docking and healing at Repair Pad",
        |game| game.entities[vehicle].health > damaged,
    )
    .map_err(|error| bay_tank_error(game, vehicle, enemy, error.as_ref()))?;
    Ok((pad, vehicle))
}

fn injure_bay_vehicle(
    game: &mut Skirmish,
    vehicle: usize,
    enemy: usize,
    factory: usize,
) -> Result<f32, Box<dyn Error>> {
    let a = game.entities[vehicle].position;
    let b = game.entities[enemy].position;
    let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    let field = game
        .placement_near(game.entities[factory].type_index, midpoint)
        .ok_or("original terrain lacks clear field for paid Repair Bay tank trial")?;
    game.move_to(vehicle, field)?;
    advance_until(
        game,
        18000,
        "paid USA tank moving to clear original terrain",
        |game| {
            let position = game.entities[vehicle].position;
            (position[0] - field[0]).hypot(position[1] - field[1]) < 20.0
        },
    )
    .map_err(|error| bay_tank_error(game, vehicle, enemy, error.as_ref()))?;
    game.stop(vehicle)?;
    let approach = [field[0] + 100.0, field[1]];
    game.move_to(enemy, approach)?;
    advance_until(
        game,
        18000,
        "paid China tank moving and scouting actual enemy tank",
        |game| {
            let a = game.entities[vehicle].position;
            let b = game.entities[enemy].position;
            (a[0] - b[0]).hypot(a[1] - b[1]) < 175.0
                && matches!(
                    game.visibility_status(1, vehicle),
                    generals_sim::skirmish::visibility::Status::Clear
                )
        },
    )
    .map_err(|error| bay_tank_error(game, vehicle, enemy, error.as_ref()))?;
    require(
        game.entities[enemy].alive() && game.entities[vehicle].alive(),
        "paid tank died while scouting Repair Bay trial",
    )?;
    game.stop(enemy)?;
    let original_health = game.entities[vehicle].health;
    game.attack(enemy, vehicle).map_err(|error| {
        let error: Box<dyn Error> = error.into();
        bay_tank_error(game, vehicle, enemy, error.as_ref())
    })?;
    advance_until(
        game,
        18000,
        "paid enemy tank injuring Repair Bay owner's vehicle",
        |game| game.entities[vehicle].health < original_health,
    )
    .map_err(|error| bay_tank_error(game, vehicle, enemy, error.as_ref()))?;
    Ok(game.entities[vehicle].health)
}

fn bay_tank_error(
    game: &Skirmish,
    vehicle: usize,
    enemy: usize,
    error: &dyn Error,
) -> Box<dyn Error> {
    let visible = |owner, target| match game.visibility_status(owner, target) {
        generals_sim::skirmish::visibility::Status::Clear => "Clear",
        generals_sim::skirmish::visibility::Status::Partial => "Partial",
        generals_sim::skirmish::visibility::Status::Fogged => "Fogged",
        generals_sim::skirmish::visibility::Status::Shrouded => "Shrouded",
    };
    format!("{error}; tank={vehicle} alive={} health={} enemy={enemy} alive={} health={} visibility USA->enemy={} China->tank={}; USA {}; China {}; enemy weapon {}",
        game.entities[vehicle].alive(), game.entities[vehicle].health,
        game.entities[enemy].alive(), game.entities[enemy].health,
        visible(0, enemy), visible(1, vehicle),
        generals_sim::skirmish::diagnostics::movement_details(game, vehicle),
        generals_sim::skirmish::diagnostics::movement_details(game, enemy),
        generals_sim::skirmish::diagnostics::weapon_details(game, enemy, vehicle)).into()
}

fn transfer_repair_bay(
    game: &mut Skirmish,
    bay: usize,
    ranger: usize,
    enemy_barracks: usize,
    enemy: usize,
) -> Result<(), Box<dyn Error>> {
    let bay_position = game.entities[bay].position;
    // Transfer through the ordinary opposing capture command, with its paid research.
    let enemy_capturer = produce(game, enemy_barracks, Kind::Infantry, 1)?;
    let research = game.catalog[game.entities[enemy_barracks].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_InfantryCaptureBuilding")
        .ok_or("China capture research missing")?;
    game.queue_upgrade(enemy_barracks, research)?;
    advance_until(
        game,
        3600,
        "paid China capture research for Repair Bay transfer",
        |game| game.entities[enemy_barracks].queue.is_empty(),
    )?;
    let friendly_base = game.entities[friendly_entity(game, Kind::CommandCenter, 0)?].position;
    game.move_to(ranger, friendly_base)?;
    game.move_to(enemy_capturer, [bay_position[0] + 75.0, bay_position[1]])?;
    advance_until(
        game,
        18000,
        "paid enemy infantry scouting original Repair Bay",
        |game| game.can_capture(enemy_capturer, bay),
    )?;
    let enemy_injured_health = game.entities[enemy].health;
    require(
        enemy_injured_health < game.entities[enemy].max_health,
        "opposing paid tank lacks actual counterfire injury for ownership healing trial",
    )?;
    game.capture(enemy_capturer, bay)?;
    advance_until(
        game,
        18000,
        "paid enemy infantry recapturing original Repair Bay",
        |game| game.entities[bay].owner == 1,
    )?;
    game.stop(enemy_capturer)?;
    require(
        game.entities[bay].alive(),
        "original Repair Bay died during recapture",
    )?;
    if game.entities[enemy].health <= enemy_injured_health {
        advance_until(
            game,
            31,
            "recaptured Repair Bay healing its new owner's damaged vehicle",
            |game| game.entities[enemy].health > enemy_injured_health,
        )?;
    }
    println!(
        "Installed TechRepairbay: paid enemy infantry research, ordinary ownership recapture and new owner's vehicle healing passed"
    );
    Ok(())
}

fn verify_repair_bay(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    prepare_repair_bay(game)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use generals_sim::skirmish::Order;

    #[ignore = "requires original installation and declared Repair Pad custom map"]
    #[test]
    fn installed_declared_repair_pad_captures_repairs_and_releases_paid_tank() {
        let root = generals_assets::game_root().unwrap();
        let fixture =
            crate::playtest::map_fixture::Fixture::load(&root, repair_pad_map_name()).unwrap();
        let mut game = fixture.into_simulation().unwrap();
        let (pad, vehicle) = prepare_repair_pad(&mut game).unwrap();
        let actor = &game.entities[vehicle];
        assert!(matches!(actor.ai.order, Order::RepairAt(target) if target == pad));
        assert!(actor.health < actor.max_health);
        advance_until(
            &mut game,
            1_200,
            "Repair Pad full repair and normal dock exit",
            |game| {
                let actor = &game.entities[vehicle];
                actor.health >= actor.max_health && matches!(actor.ai.order, Order::Idle)
            },
        )
        .unwrap();
        assert!(!game.can_repair(vehicle, pad));
        assert_eq!(game.entities[pad].owner, 0);
    }
}
