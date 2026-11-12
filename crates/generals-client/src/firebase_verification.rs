//! Paid Fire Base construction, stationed infantry, actual howitzer fire and sale.
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
use generals_sim::skirmish::{Faction, Kind, Order, ProjectileFxStage, Skirmish, WeaponFxEvent};
use std::{error::Error, path::Path};

pub struct Setup {
    pub firebase: usize,
    pub riders: Vec<usize>,
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
    game.reset(Faction::Usa, Faction::China)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let kind = template(game, "AmericaFireBase")?;
    require(
        !game.prerequisites_met(0, kind),
        "Fire Base bypassed original power-plant prerequisite",
    )?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let firebase = construct_type(game, worker, kind, 0)?;
    let unit = &game.catalog[kind];
    let rules = unit.contain.as_ref().ok_or("Fire Base garrison missing")?;
    require(
        unit.cost == 1000
            && unit.build_seconds.to_bits() == 25.0_f32.to_bits()
            && rules.capacity == 4
            && rules.garrison.as_ref().is_some_and(|rules| {
                rules.immune_to_clear
                    && rules
                        .stations
                        .as_ref()
                        .is_some_and(|points| points.len() == 4)
            }),
        "Fire Base original cost/time/stations/immunity differs",
    )?;
    let riders = station_rangers(game, barracks, firebase)?;
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::PowerPlant, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    let factory = construct(game, worker, Kind::Factory, 1)?;
    let enemy = produce_type(game, factory, template(game, "ChinaTankBattleMaster")?, 1)?;
    let center = game.entities[firebase].position;
    // The howitzer's 50-unit minimum measures separation after both bounding
    // circles. Keep the tank outside that exclusion while Rangers can fire.
    game.move_to(enemy, [center[0] + 120.0, center[1]])?;
    advance_until(
        game,
        18000,
        "paid enemy moving into Fire Base range",
        |game| {
            game.entities[enemy].alive()
                && game.weapon_target_reachable(firebase, enemy)
                && riders
                    .iter()
                    .any(|&rider| game.weapon_target_reachable(rider, enemy))
        },
    )
    .map_err(|error| {
        format!(
            "{error}; host={} enemy={}; gun_reachable={} rider_reachable={}",
            generals_sim::skirmish::diagnostics::movement_details(game, firebase),
            generals_sim::skirmish::diagnostics::movement_details(game, enemy),
            game.weapon_target_reachable(firebase, enemy),
            riders
                .iter()
                .any(|&rider| game.weapon_target_reachable(rider, enemy))
        )
    })?;
    game.stop(enemy)?;
    game.attack(firebase, enemy)?;
    Ok(Setup {
        firebase,
        riders,
        enemy,
    })
}

fn station_rangers(
    game: &mut Skirmish,
    barracks: usize,
    firebase: usize,
) -> Result<Vec<usize>, Box<dyn Error>> {
    let mut riders = Vec::new();
    for _ in 0..4 {
        let rider = produce_type(game, barracks, template(game, "AmericaInfantryRanger")?, 0)?;
        game.enter(rider, firebase)?;
        advance_until(game, 18000, "paid infantry approaching Fire Base", |game| {
            game.entities[rider].contained_by() == Some(firebase)
        })
        .map_err(|error| {
            format!(
                "{error}; rider={rider} position={:?} health={} entering={} can_enter={} host={firebase} position={:?} facing={}; {}",
                game.object_position(rider),
                game.entities[rider].health,
                matches!(game.entities[rider].ai.order, Order::Enter(target) if target == firebase),
                game.can_enter(rider, firebase),
                game.object_position(firebase),
                game.entities[firebase].facing,
                generals_sim::skirmish::diagnostics::movement_details(game, rider)
            )
        })?;
        riders.push(rider);
    }
    game.step();
    require(
        game.passengers(firebase) == riders
            && riders
                .iter()
                .all(|&id| game.entities[id].stationed() && !game.entities[id].masked()),
        "Fire Base occupants not visible in original entry order",
    )?;
    require(
        game.garrison_points(firebase).is_none(),
        "non-enclosing Fire Base created hidden-garrison gun drawables",
    )?;
    Ok(riders)
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let before = game.entities[setup.enemy].health;
    let mut gun = false;
    let mut infantry = false;
    let mut impact = false;
    for _ in 0..900 {
        game.step();
        gun |= game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == setup.firebase);
        infantry |= game
            .presentation
            .shots
            .iter()
            .any(|shot| setup.riders.contains(&shot.source));
        impact |= game.presentation.weapon_fx.iter().any(|event| {
            matches!(event, WeaponFxEvent::Projectile(event)
            if event.source == setup.firebase
                && matches!(event.stage, ProjectileFxStage::Detonation))
        });
        if gun && infantry && impact && game.entities[setup.enemy].health < before {
            break;
        }
    }
    require(
        gun && infantry && impact && game.entities[setup.enemy].health < before,
        "Fire Base howitzer/occupant firing or original shell damage did not occur",
    )?;
    if game.entities[setup.enemy].alive() {
        game.move_to(
            setup.enemy,
            game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position,
        )?;
    }
    game.exit_container(setup.riders[0])?;
    advance_until(game, 120, "individual station exit", |game| {
        game.entities[setup.riders[0]].contained_by().is_none()
    })?;
    let surviving = game.passengers(setup.firebase).to_vec();
    game.sell(0, setup.firebase)?;
    require(
        game.passengers(setup.firebase).is_empty()
            && surviving.iter().all(|&id| {
                game.entities[id].contained_by().is_none()
                    && !game.entities[id].masked()
                    && !matches!(game.entities[id].ai.order, Order::ExitContainer)
            }),
        "Fire Base sale retained stationed infantry",
    )?;
    println!(
        "Installed Fire Base: paid prerequisite/build/four infantry, visible stations, howitzer shell impact, passenger fire, individual exit and sale passed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_fire_base_boards_original_infantry_fires_and_releases_its_stations() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
