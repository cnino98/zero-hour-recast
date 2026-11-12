//! Paid original stationary speaker, research, healing and power lifecycle.
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
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::{error::Error, path::Path};

pub struct Setup {
    pub tower: usize,
    pub worker: usize,
    pub tech: usize,
    plant: usize,
    enemy: usize,
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
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    let tower_type = template(game, "ChinaSpeakerTower")?;
    require(
        !game.prerequisites_met(0, tower_type),
        "speaker bypassed Propaganda Center prerequisite",
    )?;
    let plant = construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    construct(game, worker, Kind::Factory, 0)?;
    let tech = construct_type(game, worker, template(game, "ChinaPropagandaCenter")?, 0)?;
    let tower = construct_type(game, worker, tower_type, 0)?;
    let rules = &game.catalog[tower_type];
    require(
        rules.cost == 500
            && rules.build_seconds.to_bits() == 10.0_f32.to_bits()
            && rules.energy == -1
            && rules.maintenance.research.is_empty(),
        "speaker build cost, time, energy or original Sell-only command set differs",
    )?;
    let propaganda = rules
        .behavior
        .propaganda
        .as_ref()
        .ok_or("speaker lacks propaganda module")?;
    require(
        propaganda.delay == 60
            && propaganda.radius.to_bits() == 150.0_f32.to_bits()
            && propaganda.heal.map(f32::to_bits) == [0.02_f32, 0.04].map(f32::to_bits),
        "stationary speaker has incorrect native pulse/healing values",
    )?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    let enemy = produce_type(
        game,
        barracks,
        template(game, "AmericaInfantryMissileDefender")?,
        1,
    )?;
    let setup = Setup {
        tower,
        worker,
        tech,
        plant,
        enemy,
    };
    injure(game, &setup)?;
    Ok(setup)
}

pub fn injure(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let center = game.entities[setup.tower].position;
    game.move_to(setup.worker, [center[0] + 60.0, center[1]])?;
    advance_until(game, 9000, "worker approaching original speaker", |game| {
        matches!(game.entities[setup.worker].ai.order, Order::Idle)
    })?;
    game.attack(setup.enemy, setup.worker)?;
    advance_until(
        game,
        18000,
        "ordinary RPG fire damaging speaker beneficiary",
        |game| {
            let actor = &game.entities[setup.worker];
            actor.health < actor.max_health * 0.6
        },
    )?;
    require(
        game.entities[setup.worker].alive(),
        "speaker beneficiary died",
    )?;
    let base = game.entities[friendly_entity(game, Kind::CommandCenter, 1)?].position;
    game.move_to(setup.enemy, base)?;
    advance_until(
        game,
        3000,
        "RPG attacker leaving the speaker radius",
        |game| {
            let from = game.entities[setup.enemy].position;
            let to = game.entities[setup.worker].position;
            (from[0] - to[0]).hypot(from[1] - to[1]) > 300.0
                && game
                    .projectiles
                    .iter()
                    .all(|projectile| projectile.source != setup.enemy)
        },
    )?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    healing(game, &setup, 0.02)?;
    research(game, setup.tech, "Upgrade_ChinaSubliminalMessaging")?;
    injure(game, &setup)?;
    healing(game, &setup, 0.04)?;
    game.sell(0, setup.plant)?;
    advance_until(
        game,
        300,
        "power plant sale causing original brownout",
        |game| !game.entities[setup.plant].alive() && !game.has_operational_power(setup.tower),
    )?;
    let before = game.entities[setup.worker].health;
    for _ in 0..90 {
        game.step();
        require(
            !game
                .presentation
                .propaganda_events
                .iter()
                .any(|event| event.source == setup.tower),
            "underpowered speaker continued original FX pulses",
        )?;
    }
    require(
        game.entities[setup.worker].health.to_bits() == before.to_bits(),
        "underpowered speaker continued healing",
    )?;
    construct(game, setup.worker, Kind::PowerPlant, 0)?;
    require(
        game.has_operational_power(setup.tower),
        "new power plant did not restore speaker",
    )?;
    advance_until(
        game,
        120,
        "restored speaker original scan cadence",
        |game| {
            game.presentation
                .propaganda_events
                .iter()
                .any(|event| event.source == setup.tower && event.upgraded)
        },
    )?;
    game.sell(0, setup.tower)?;
    game.step();
    require(
        !game
            .presentation
            .propaganda_events
            .iter()
            .any(|event| event.source == setup.tower),
        "sold speaker continued pulsing",
    )?;
    println!(
        "Installed Speaker Tower: paid prerequisite/build, 2%/4% original healing, Subliminal research, power loss/restoration and sale passed"
    );
    Ok(())
}

fn healing(game: &mut Skirmish, setup: &Setup, rate: f32) -> Result<(), Box<dyn Error>> {
    let before = game.entities[setup.worker].health;
    advance_until(game, 120, "original speaker healing damaged unit", |game| {
        game.entities[setup.worker].health > before
    })?;
    let actor = &game.entities[setup.worker];
    let amount = rate / 30.0 * actor.max_health;
    require(
        actor.health.to_bits() == (before + amount).min(actor.max_health).to_bits(),
        "speaker health increment differs from native per-frame percent",
    )
}
