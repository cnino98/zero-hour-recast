//! Paid installed Paladin, Avenger, interception, painting and attached AA checks.
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

pub struct Setup {
    pub paladin: usize,
    pub avenger: usize,
    pub turret: usize,
    pub factory: usize,
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
    let mut animations = crate::model_animation::load(&assets, &mut data.models, &data.types)?;
    animations.bind_logical_bones(&mut data.types);
    let projectiles =
        crate::projectile_data::load(&assets, &mut data.types, &data.models, &animations)?;
    animations.verify_point_defense_art(&data.models, &data.types)?;
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

fn prepare(game: &mut Skirmish, enemy: Faction) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, enemy)?;
    let paladin_type = template(game, "AmericaTankPaladin")?;
    let avenger_type = template(game, "AmericaTankAvenger")?;
    require(
        !game.prerequisites_met(0, paladin_type) && !game.prerequisites_met(0, avenger_type),
        "new tanks bypass original War Factory prerequisites",
    )?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let cash = game.players[0].cash;
    require(
        game.train(factory, paladin_type).is_err() && game.players[0].cash == cash,
        "Paladin production bypassed its unpaid original science",
    )?;
    let points = game.players[0].science_points;
    game.purchase_science(0, "SCIENCE_PaladinTank")?;
    require(
        game.players[0].science_points + 1 == points,
        "Paladin science did not spend its original promotion point",
    )?;
    let paladin = produce_type(game, factory, paladin_type, 0)?;
    let avenger = produce_type(game, factory, avenger_type, 0)?;
    let turret = game.entities[avenger]
        .attachment_child()
        .ok_or("paid Avenger did not create its original initial turret")?;
    require(
        game.catalog[game.entities[turret].type_index].name == "AmericaTankAvengerLaserTurret"
            && game.entities[turret].attachment_parent() == Some(avenger)
            && game.entities[turret].owner == 0
            && !game.entities[turret].selectable(&game.catalog[game.entities[turret].type_index]),
        "Avenger initial payload has the wrong template, owner or containment",
    )?;
    require(
        game.train(factory, game.entities[turret].type_index)
            .is_err(),
        "internal Avenger turret can be bought as a separate factory product",
    )?;
    Ok(Setup {
        paladin,
        avenger,
        turret,
        factory,
    })
}

fn stage_avenger(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let home = friendly_entity(game, Kind::CommandCenter, 0)?;
    let enemy = friendly_entity(game, Kind::CommandCenter, 1)?;
    let a = game.entities[home].position;
    let b = game.entities[enemy].position;
    let destination = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    game.move_to(setup.avenger, destination)?;
    advance_until(game, 18_000, "Avenger driving to the battle site", |game| {
        distance(game.entities[setup.avenger].position, destination) < 50.0
    })?;
    game.stop(setup.avenger)?;
    Ok(())
}

pub fn prepare_missile(game: &mut Skirmish) -> Result<(Setup, usize), Box<dyn Error>> {
    let setup = prepare(game, Faction::Gla)?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    let barracks = construct(game, builder, Kind::Barracks, 1)?;
    let rpg_type = template(game, "GLAInfantryTunnelDefender")?;
    let rpg = produce_type(game, barracks, rpg_type, 1)?;
    let second = produce_type(game, barracks, rpg_type, 1)?;
    stage_avenger(game, &setup)?;
    game.attack(rpg, setup.avenger)?;
    game.attack(second, setup.avenger)?;
    advance_until(game, 18_000, "RPG approaching the Avenger", |game| {
        distance(
            game.entities[rpg].position,
            game.entities[setup.avenger].position,
        ) < 230.0
    })?;
    Ok((setup, rpg))
}

pub fn prepare_air(game: &mut Skirmish) -> Result<(Setup, usize), Box<dyn Error>> {
    let setup = prepare(game, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    let center = construct(game, builder, Kind::SupplyCenter, 1)?;
    let chinook = crate::weapon_verification::airborne_supply_chinook(game, center)?;
    stage_avenger(game, &setup)?;
    game.move_to(chinook, game.entities[setup.avenger].position)?;
    advance_until(game, 18_000, "enemy Chinook approaching AA range", |game| {
        distance(
            game.entities[chinook].position,
            game.entities[setup.avenger].position,
        ) < 180.0
    })?;
    game.stop(chinook)?;
    Ok((setup, chinook))
}

pub fn prepare_paladin(game: &mut Skirmish) -> Result<(Setup, usize), Box<dyn Error>> {
    let setup = prepare(game, Faction::Gla)?;
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    game.move_to(setup.paladin, game.entities[worker].position)?;
    advance_until(
        game,
        18_000,
        "Paladin approaching original infantry laser range",
        |game| {
            distance(
                game.entities[setup.paladin].position,
                game.entities[worker].position,
            ) < 110.0
        },
    )?;
    Ok((setup, worker))
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    verify_interception(game)?;
    verify_air(game)?;
    verify_paint(game)?;
    verify_research(game)?;
    println!(
        "Installed point defense: paid science/prerequisites/production, initial mounted AA, original RPG interception, AA laser damage, target-paint status/rate and Paladin armor research passed"
    );
    Ok(())
}

fn verify_interception(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (setup, rpg) = prepare_missile(game)?;
    let health = game.entities[setup.avenger].health;
    let mut missiles = Vec::new();
    for _ in 0..3000 {
        game.step();
        for missile in &game.projectiles {
            if missile.source == rpg && !missiles.iter().any(|(serial, _)| *serial == missile.id) {
                require(
                    missile
                        .object
                        .is_some_and(|id| game.entities[id].health > 0.0)
                        && missile.render_transform(&game.entities).is_some(),
                    "original RPG missile lacks a live damage body or flight transform",
                )?;
                missiles.push((
                    missile.id,
                    missile.object.expect("validated original missile object"),
                ));
            }
        }
        let intercepted = game
            .presentation
            .laser_shots
            .iter()
            .filter(|shot| shot.source == setup.avenger)
            .find_map(|shot| {
                missiles
                    .iter()
                    .find(|(_, object)| shot.target == Some(*object))
                    .copied()
            });
        if let Some((id, object)) = intercepted {
            require(
                game.projectiles.iter().all(|missile| missile.id != id)
                    && !game.entities[object].alive()
                    && game.entities[setup.avenger].health.to_bits() == health.to_bits(),
                "Avenger did not intercept the actual RPG missile before payload damage",
            )?;
            return Ok(());
        }
    }
    Err("original RPG attack did not produce an Avenger missile interception".into())
}

fn verify_air(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (setup, chinook) = prepare_air(game)?;
    let health = game.entities[chinook].health;
    let mut slots = [false; 2];
    for _ in 0..3000 {
        game.step();
        for shot in &game.presentation.shots {
            if shot.source == setup.turret && shot.target == Some(chinook) {
                let weapon = game.catalog[game.entities[setup.turret].type_index]
                    .weapon_template(shot.weapon_index)
                    .ok_or("attached AA shot has no original weapon")?;
                match weapon.name.as_str() {
                    "AvengerAirLaserOne" => slots[0] = true,
                    "AvengerAirLaserTwo" => slots[1] = true,
                    _ => return Err("attached Avenger fired a non-AA weapon".into()),
                }
            }
        }
        if slots == [true, true] {
            return require(
                game.entities[chinook].health < health
                    && game.entities[setup.turret].attachment_parent() == Some(setup.avenger),
                "independent attached AA did not damage the airborne Chinook",
            );
        }
    }
    Err("original independent Avenger turret did not fire both AA lasers".into())
}

fn verify_paint(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game, Faction::Gla)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let health = game.entities[target].health;
    game.attack(setup.avenger, target)?;
    advance_until(
        game,
        18_000,
        "Avenger painting enemy command center",
        |game| game.entities[target].painted(),
    )?;
    require(
        game.entities[target].health.to_bits() == health.to_bits(),
        "FAERIE_FIRE duration was applied as ordinary health damage",
    )?;
    game.attack(setup.paladin, target)?;
    let mut frames = Vec::new();
    for _ in 0..18_000 {
        game.step();
        if game.presentation.shots.iter().any(|shot| {
            shot.source == setup.paladin && shot.target == Some(target) && shot.weapon_slot == 0
        }) {
            require(
                game.entities[target].painted(),
                "Paladin fired at an unpainted target",
            )?;
            frames.push(game.tick);
            if frames.len() == 3 {
                return require(
                    frames[1] - frames[0] == 40 && frames[2] - frames[1] == 40,
                    "Paladin target-paint callback did not apply original 1.5 firing rate",
                );
            }
        }
    }
    Err("painted target did not receive three original Paladin cannon shots".into())
}

fn verify_research(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game, Faction::Gla)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let index = template(game, "AmericaStrategyCenter")?;
    let cost = game.catalog[index].cost;
    advance_until(game, 18_000, "supplies funding Strategy Center", |game| {
        game.players[0].cash >= cost
    })?;
    let center = construct_type(game, builder, index, 0)?;
    let paladin_health = game.entities[setup.paladin].max_health;
    let avenger_health = game.entities[setup.avenger].max_health;
    research(game, center, "Upgrade_AmericaCompositeArmor")?;
    research(game, center, "Upgrade_AmericaAdvancedTraining")?;
    require(
        game.entities[setup.paladin].max_health.to_bits() == (paladin_health + 100.0).to_bits()
            && game.entities[setup.avenger].max_health.to_bits() == avenger_health.to_bits(),
        "Composite Armor did not preserve Paladin's single module and Avenger exclusion",
    )?;
    let index = game.entities[setup.paladin].type_index;
    let cost = game.catalog[index].cost;
    advance_until(game, 18_000, "supplies funding upgraded Paladin", |game| {
        game.players[0].cash >= cost
    })?;
    let future = produce_type(game, setup.factory, index, 0)?;
    require(
        game.entities[future].max_health.to_bits() == (paladin_health + 100.0).to_bits(),
        "future Paladin lost researched Composite Armor",
    )?;
    verify_paladin_infantry(game, future)
}

fn verify_paladin_infantry(game: &mut Skirmish, paladin: usize) -> Result<(), Box<dyn Error>> {
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    let value = game.catalog[game.entities[worker].type_index]
        .veterancy
        .experience_value[usize::from(game.entities[worker].veterancy)];
    let experience = game.entities[paladin].experience;
    let health = game.entities[worker].health;
    game.move_to(paladin, game.entities[worker].position)?;
    advance_until(
        game,
        18_000,
        "Paladin independent infantry point defense",
        |game| {
            game.presentation
                .laser_shots
                .iter()
                .any(|shot| shot.source == paladin && shot.target == Some(worker))
        },
    )?;
    require(
        game.entities[worker].health < health,
        "Paladin secondary infantry laser did not apply original LASER armor damage",
    )?;
    game.attack(paladin, worker)?;
    advance_until(
        game,
        3000,
        "researched Paladin earning original worker XP",
        |game| !game.entities[worker].alive(),
    )?;
    require(
        game.entities[paladin].experience == experience + value * 2,
        "future Paladin did not receive Advanced Training's doubled original kill XP",
    )
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
