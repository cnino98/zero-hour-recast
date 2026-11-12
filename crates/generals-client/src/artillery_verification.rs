//! Paid original artillery production, research, missiles and toxin-field acceptance.
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
use generals_sim::skirmish::{Faction, Kind, ProjectileFxStage, Skirmish, WeaponFxEvent};
use std::{error::Error, path::Path};

pub struct Setup {
    pub actor: usize,
    pub target: usize,
    pub factory: usize,
    pub tech: usize,
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
    animations.verify_artillery_art(&data.models, &data.types)?;
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    drop(crate::laser_beam::load(&assets, &data.types)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

pub fn prepare(game: &mut Skirmish, name: &str) -> Result<Setup, Box<dyn Error>> {
    let usa = name == "AmericaVehicleTomahawk";
    game.reset(
        if usa { Faction::Usa } else { Faction::Gla },
        Faction::China,
    )?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if usa {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let index = template(game, name)?;
    let cash = game.players[0].cash;
    require(
        game.train(factory, index).is_err() && game.players[0].cash == cash,
        "artillery bypassed its original technology prerequisite",
    )?;
    let tech_type = template(
        game,
        if usa {
            "AmericaStrategyCenter"
        } else {
            "GLAPalace"
        },
    )?;
    let tech = construct_type(game, builder, tech_type, 0)?;
    if name == "GLAVehicleScudLauncher" {
        require(
            game.train(factory, index).is_err(),
            "SCUD bypassed unpaid science",
        )?;
        let points = game.players[0].science_points;
        game.purchase_science(0, "SCIENCE_ScudLauncher")?;
        require(
            game.players[0].science_points + 1 == points,
            "SCUD science was not paid",
        )?;
    }
    let actor = produce_type(game, factory, index, 0)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let from = game.entities[actor].position;
    let to = game.entities[target].position;
    let delta = [from[0] - to[0], from[1] - to[1]];
    let length = delta[0].hypot(delta[1]);
    let point = [
        (delta[0] / length).mul_add(340.0, to[0]),
        (delta[1] / length).mul_add(340.0, to[1]),
    ];
    game.move_to(actor, point)?;
    advance_until(
        game,
        18_000,
        "paid artillery approaching source firing range",
        |game| {
            let position = game.entities[actor].position;
            (position[0] - point[0]).hypot(position[1] - point[1]) < 25.0
        },
    )?;
    game.stop(actor)?;
    Ok(Setup {
        actor,
        target,
        factory,
        tech,
    })
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    for name in [
        "AmericaVehicleTomahawk",
        "GLAVehicleRocketBuggy",
        "GLAVehicleScudLauncher",
    ] {
        let setup = prepare(game, name)?;
        verify_shot(game, &setup)?;
        if name == "GLAVehicleRocketBuggy" {
            let builder = friendly_entity(game, Kind::Builder, 0)?;
            let market = construct_type(game, builder, template(game, "GLABlackMarket")?, 0)?;
            research(game, market, "Upgrade_GLABuggyAmmo")?;
            research(game, market, "Upgrade_GLAAPRockets")?;
            let unit = &game.catalog[game.entities[setup.actor].type_index];
            require(
                game.entities[setup.actor].active_weapon(unit).clip_size == 12,
                "paid Buggy Ammo failed to replace the six-rocket clip",
            )?;
            let future = produce_type(
                game,
                setup.factory,
                game.entities[setup.actor].type_index,
                0,
            )?;
            require(
                game.entities[future]
                    .active_weapon(&game.catalog[game.entities[future].type_index])
                    .clip_size
                    == 12,
                "future Buggy did not inherit paid ammo research",
            )?;
        }
        if name == "GLAVehicleScudLauncher" {
            research(game, setup.tech, "Upgrade_GLAAnthraxBeta")?;
            game.switch_weapon(setup.actor, 1)?;
            game.attack(setup.actor, setup.target)?;
            advance_until(
                game,
                3000,
                "original SCUD Anthrax projectile detonation OCL",
                |game| game.hazards.iter().any(|field| field.source == setup.actor),
            )?;
            let field = game
                .hazards
                .iter()
                .find(|field| field.source == setup.actor)
                .ok_or("SCUD field disappeared")?;
            let rules = game.catalog[game.entities[setup.actor].type_index]
                .hazard_rules(field.origin)
                .ok_or("SCUD field metadata missing")?;
            require(
                matches!(
                    rules.death_type,
                    generals_sim::skirmish::DeathType::PoisonedBeta
                ) && rules.lifetime == 900
                    && field.owner == 0,
                "SCUD field lost original Anthrax upgrade, duration or ownership",
            )?;
            game.stop(setup.actor)?;
            advance_until(game, 1000, "original toxic-field expiration", |game| {
                game.hazards.is_empty()
            })?;
        }
    }
    println!(
        "Installed artillery: paid technology/science/production, articulated missile launch and impact, Buggy Ammo/AP Rockets and SCUD Anthrax field lifecycle passed"
    );
    Ok(())
}

fn verify_shot(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let health = game.entities[setup.target].health;
    game.attack(setup.actor, setup.target)?;
    let mut launched = false;
    let mut detonated = false;
    for _ in 0..3000 {
        game.step();
        launched |= game.projectiles.iter().any(|projectile| {
            projectile.source == setup.actor
                && projectile.render_transform(&game.entities).is_some()
        });
        detonated |= game.presentation.weapon_fx.iter().any(|event| {
            matches!(event,
            WeaponFxEvent::Projectile(event) if event.source == setup.actor
                && matches!(event.stage, ProjectileFxStage::Detonation))
        });
        if launched && detonated && game.entities[setup.target].health < health {
            game.stop(setup.actor)?;
            return Ok(());
        }
    }
    Err(format!("{} failed original missile launch/impact: launch={launched} detonation={detonated} health={}",
        game.catalog[game.entities[setup.actor].type_index].name, game.entities[setup.target].health).into())
}
