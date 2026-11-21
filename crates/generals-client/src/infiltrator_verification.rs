//! Paid original Black Lotus and Scout Drone source-command acceptance.
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
use generals_sim::skirmish::{Faction, Kind, Skirmish, special_ability::FeedbackEffect};
use std::{error::Error, path::Path};

pub struct Setup {
    pub actor: usize,
    pub cash: usize,
    pub vehicle: usize,
    pub building: usize,
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = load_game(root)?;
    verify(&mut game)?;
    crate::jarmen_verification::verify(&mut game)?;
    crate::demo_trap_verification::verify(&mut game)
}

pub fn load_game(root: &Path) -> Result<Skirmish, Box<dyn Error>> {
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
    animations.verify_infiltrator_art(&data.models, &data.types)?;
    animations.verify_jarmen_art(&data.models, &data.types)?;
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    drop(crate::laser_beam::load(&assets, &data.types)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    Ok(game)
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    prepare_for(game, 0)
}

fn prepare_for(game: &mut Skirmish, china: usize) -> Result<Setup, Box<dyn Error>> {
    let usa = 1 - china;
    if china == 0 {
        game.reset(Faction::China, Faction::Usa)?;
    } else {
        game.reset(Faction::Usa, Faction::China)?;
    }
    let worker = friendly_entity(game, Kind::Builder, china)?;
    construct(game, worker, Kind::PowerPlant, china)?;
    construct(game, worker, Kind::SupplyCenter, china)?;
    let barracks = construct(game, worker, Kind::Barracks, china)?;
    let lotus = template(game, "ChinaInfantryBlackLotus")?;
    let before = game.players[china].cash;
    require(
        game.train(barracks, lotus).is_err() && game.players[china].cash == before,
        "Lotus bypassed original Propaganda Center prerequisite",
    )?;
    construct(game, worker, Kind::Factory, china)?;
    let propaganda = template(game, "ChinaPropagandaCenter")?;
    construct_type(game, worker, propaganda, china)?;
    let actor = produce_type(game, barracks, lotus, china)?;
    let before = game.players[china].cash;
    require(
        game.train(barracks, lotus).is_err() && game.players[china].cash == before,
        "Lotus original one-per-player production limit failed",
    )?;
    let vehicle = friendly_entity(game, Kind::Builder, usa)?;
    let building = construct(game, vehicle, Kind::PowerPlant, usa)?;
    let cash = construct(game, vehicle, Kind::SupplyCenter, usa)?;
    game.stop(vehicle)?;
    approach(game, actor, vehicle)?;
    advance_until(
        game,
        600,
        "stationary Lotus concealing through original delay",
        |game| game.entities[actor].stealthed(),
    )?;
    require(
        !game.logically_visible(usa, actor),
        "enemy can see an undetected concealed Lotus",
    )?;
    Ok(Setup {
        actor,
        cash,
        vehicle,
        building,
    })
}

/// Leave the original Scout button unpurchased for the rendered command check.
pub fn prepare_scout(game: &mut Skirmish) -> Result<(usize, usize), Box<dyn Error>> {
    let setup = prepare_for(game, 1)?;
    let factory = construct(game, setup.vehicle, Kind::Factory, 0)?;
    let parent = produce_type(game, factory, template(game, "AmericaVehicleHumvee")?, 0)?;
    advance_until(
        game,
        18_000,
        "normal income funding Scout command",
        |game| game.players[0].cash >= 100,
    )?;
    // Keep the source research voice separate from movement/detection. The
    // existing paid Lotus is near this base, so leave detector range normally
    // before creating the Scout; never suppress ordinary Humvee targeting.
    let origin = game.entities[parent].position;
    let target = game.entities[setup.actor].position;
    let distance = (origin[0] - target[0]).hypot(origin[1] - target[1]);
    require(
        distance > 1.0,
        "Scout parent overlaps its concealed staging target",
    )?;
    let point = [
        ((origin[0] - target[0]) / distance).mul_add(400.0, target[0]),
        ((origin[1] - target[1]) / distance).mul_add(400.0, target[1]),
    ];
    game.move_to(parent, point)?;
    advance_until(
        game,
        18_000,
        "Scout parent leaving detector range before purchase",
        |game| {
            let actor = &game.entities[parent];
            matches!(actor.ai.order, generals_sim::skirmish::Order::Idle)
                && !actor.moving()
                && (actor.position[0] - point[0]).hypot(actor.position[1] - point[1]) < 20.0
        },
    )?;
    require(
        game.entities[setup.actor].stealthed() && !game.logically_visible(0, setup.actor),
        "Scout capture must begin with a genuinely concealed enemy",
    )?;
    Ok((parent, setup.actor))
}

fn approach(game: &mut Skirmish, actor: usize, target: usize) -> Result<(), Box<dyn Error>> {
    game.move_to(actor, game.entities[target].position)?;
    advance_until(
        game,
        18_000,
        "Lotus approaching the original finite hack range",
        |game| {
            let a = game.entities[actor].position;
            let b = game.entities[target].position;
            (a[0] - b[0]).hypot(a[1] - b[1]) < 190.0
                && game.visible_to(game.entities[actor].owner, target)
        },
    )?;
    game.stop(actor)?;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let health = game.entities[setup.vehicle].health;
    game.hack(setup.actor, setup.vehicle, 1)?;
    advance_until(game, 1800, "real Lotus vehicle preparation", |game| {
        game.entities[setup.actor].lotus_stream_target() == Some(setup.vehicle)
            && !game.entities[setup.actor].stealthed()
    })?;
    require(
        !game.entities[setup.actor].stealthed(),
        "finite hack preparation did not break Lotus concealment",
    )?;
    advance_until(game, 1800, "original finite vehicle hack effect", |game| {
        game.entities[setup.vehicle].hacked()
    })?;
    require(
        game.entities[setup.vehicle].health.to_bits() == health.to_bits(),
        "Lotus vehicle hack applied invented weapon damage",
    )?;
    advance_until(game, 600, "original vehicle hack expiration", |game| {
        !game.entities[setup.vehicle].hacked()
    })?;
    verify_cash(game, &setup)?;
    approach(game, setup.actor, setup.building)?;
    game.capture(setup.actor, setup.building)?;
    advance_until(
        game,
        2400,
        "Lotus capture changes the real building owner",
        |game| game.entities[setup.building].owner == 0,
    )?;
    advance_until(
        game,
        600,
        "Lotus completed original capture packing",
        |game| game.entities[setup.actor].capture_condition().is_none(),
    )?;
    verify_scout(game, &setup)?;
    println!(
        "Paid infiltrators: Lotus prerequisite/limit, stealth, three finite abilities and Scout detection passed at tick {}",
        game.tick
    );
    Ok(())
}

fn verify_cash(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let harvesters: Vec<_> = game
        .entities
        .iter()
        .enumerate()
        .filter(|(_, actor)| {
            actor.alive() && game.catalog[actor.type_index].kind == Kind::Harvester
        })
        .map(|(id, _)| id)
        .collect();
    for id in harvesters {
        game.stop(id)?;
    }
    approach(game, setup.actor, setup.cash)?;
    game.hack(setup.actor, setup.cash, 2)?;
    // Supply trucks may finish deliveries during the unpack animation. Measure the
    // transfer against the balances immediately before the actual cash-hack callback.
    let mut before = [game.players[0].cash, game.players[1].cash];
    let mut earned = game.players[0].statistics.money_earned;
    advance_until(game, 2400, "Lotus original cash transfer event", |game| {
        if game.presentation.ability_feedback.iter().any(|event| {
            event.object == setup.actor
                && matches!(event.effect, FeedbackEffect::CashStolen(value) if value == 1000.min(before[1]))
        }) {
            return true;
        }
        before = [game.players[0].cash, game.players[1].cash];
        earned = game.players[0].statistics.money_earned;
        false
    })?;
    let amount = 1000.min(before[1]);
    require(
        game.players[0].cash == before[0] + amount
            && game.players[1].cash == before[1] - amount
            && game.players[0].statistics.money_earned == earned + amount,
        "Lotus cash effect did not transfer and credit the original amount",
    )?;
    advance_until(game, 600, "Lotus original cash packing", |game| {
        game.entities[setup.actor].capture_condition().is_none()
    })?;
    Ok(())
}

fn verify_scout(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    // The opposite player purchases the detector normally; no detection status
    // is granted and the concealed target remains stationary throughout the scan.
    let worker = setup.vehicle;
    construct(game, worker, Kind::PowerPlant, 1)?;
    let factory = construct(game, worker, Kind::Factory, 1)?;
    let parent = produce_type(game, factory, template(game, "AmericaVehicleHumvee")?, 1)?;
    research(game, parent, "Upgrade_AmericaScoutDrone")?;
    let scout = game
        .produced_drone(parent)
        .ok_or("paid Scout Drone missing")?;
    let unit = &game.catalog[game.entities[scout].type_index];
    require(
        unit.name == "AmericaVehicleScoutDrone"
            && unit.kind_of.iter().any(|flag| flag == "NO_SELECT")
            && !game.entities[scout].selectable(unit)
            && game.entities[scout].drone_parent() == Some(parent),
        "Scout OCL template, parent or original NO_SELECT flag differs",
    )?;
    game.move_to(parent, game.entities[setup.actor].position)?;
    advance_until(
        game,
        18_000,
        "original Scout detecting a real concealed Lotus",
        |game| game.entities[setup.actor].stealthed() && game.entities[setup.actor].detected(),
    )?;
    require(
        game.logically_visible(1, setup.actor),
        "Scout detection failed logical enemy visibility",
    )?;
    game.stop(parent)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_infiltrators_use_original_concealment_detection_and_finite_hacks() {
        let root = generals_assets::game_root().unwrap();
        super::run(&root).unwrap();
    }
}
