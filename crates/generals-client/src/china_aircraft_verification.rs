//! Paid China hangars, `MiG` sorties and the original shared eight-impact firestorm.
mod crash;
mod sortie;
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
    pub airfield: usize,
    pub migs: [usize; 4],
    pub target: usize,
    pub assembly: [f32; 2],
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
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let defenders = crash::prepare(game, &setup)?;
    approach(game, &setup)?;
    sortie::verify(game, &setup, false)?;
    upgrade(game, &setup)?;
    approach(game, &setup)?;
    sortie::verify(game, &setup, true)?;
    crash::verify(game, &setup, defenders)?;
    println!(
        "Paid China aircraft: four original hangars, capacity rejection, two-missile normal/Black Napalm sorties, eight-impact shared firestorms, return/rearm and Aircraft Armor passed at {}",
        game.tick
    );
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let index = template(game, "ChinaAirfield")?;
    let cash = game.players[0].cash;
    require(
        !game.prerequisites_met(0, index),
        "China Airfield bypassed its Supply Center prerequisite",
    )?;
    require(
        game.players[0].cash == cash,
        "prerequisite query changed cash",
    )?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let airfield = construct_type(game, builder, index, 0)?;
    let index = template(game, "ChinaJetMIG")?;
    let mut migs = [0; 4];
    for actor in &mut migs {
        *actor = produce_type(game, airfield, index, 0)?;
    }
    advance_until(
        game,
        3600,
        "four MiGs parked in their original hangars",
        |game| {
            migs.iter().all(|&id| {
                game.entities[id].aircraft_phase() == Some("idle")
                    && game.entities[id].weapon_rounds_remaining(0) == 2
            })
        },
    )?;
    for (index, &id) in migs.iter().enumerate() {
        require(
            migs[..index].iter().all(|&other| {
                game.entities[id].position.map(f32::to_bits)
                    != game.entities[other].position.map(f32::to_bits)
            }),
            "parked MiGs did not occupy four distinct model hangars",
        )?;
    }
    let cash = game.players[0].cash;
    require(
        !game.parking_available(airfield, index)
            && game.train(airfield, index).is_err()
            && game.players[0].cash == cash,
        "fifth MiG bypassed four-hangar capacity or charged cash",
    )?;
    require(
        migs.iter()
            .all(|&id| game.entities[id].producer() == Some(airfield)),
        "MiG lost its paid producer/parking owner",
    )?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let position = game.entities[target].position;
    Ok(Setup {
        airfield,
        migs,
        target,
        assembly: [position[0], position[1] - 800.0],
    })
}

pub fn upgrade(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    require(
        setup
            .migs
            .iter()
            .all(|&id| game.entities[id].max_health.to_bits() == 160.0_f32.to_bits()),
        "unresearched MiG did not use original 160 health",
    )?;
    research(game, setup.airfield, "Upgrade_ChinaAircraftArmor")?;
    require(
        setup.migs.iter().all(|&id| {
            game.entities[id].max_health.to_bits() == 200.0_f32.to_bits()
                && game.entities[id].health.to_bits() == 200.0_f32.to_bits()
        }),
        "Aircraft Armor did not add 40 maximum and current health to parked MiGs",
    )?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let factory_type = template(game, "ChinaWarFactory")?;
    let cost = game.catalog[factory_type].cost;
    advance_until(
        game,
        18000,
        "ordinary supply income funding China War Factory",
        |game| game.players[0].cash >= cost,
    )?;
    let factory = construct_type(game, builder, factory_type, 0)?;
    research(game, factory, "Upgrade_ChinaBlackNapalm")?;
    require(
        setup.migs.iter().all(|&id| {
            let actor = &game.entities[id];
            actor.active_weapon(&game.catalog[actor.type_index]).name == "BlackNapalmMissileWeapon"
        }),
        "Black Napalm did not replace every existing MiG primary weapon",
    )
}

pub fn approach(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    scout(game, setup)?;
    for (&id, offset) in setup.migs.iter().zip([-45.0, -15.0, 15.0, 45.0]) {
        game.move_to(id, [setup.assembly[0] + offset, setup.assembly[1]])?;
    }
    let mut ready = [false; 4];
    for _ in 0..7200 {
        game.step();
        for (index, &id) in setup.migs.iter().enumerate() {
            let actor = &game.entities[id];
            ready[index] = actor.aircraft_phase() == Some("idle")
                && game.height_above_ground(id) > 10.0
                && (actor.position[0] - setup.assembly[0])
                    .hypot(actor.position[1] - setup.assembly[1])
                    < 200.0;
        }
        if ready.into_iter().all(|value| value) && game.logically_visible(0, setup.target) {
            require(
                setup
                    .migs
                    .iter()
                    .all(|&id| game.entities[id].weapon_rounds_remaining(0) == 2),
                "MiG staging consumed missiles before the ordered volley",
            )?;
            return Ok(());
        }
    }
    Err(format!("MiG formation approach timed out: {}", details(game, setup)).into())
}

pub fn scout(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    let target = game.entities[setup.target].position;
    game.move_to(builder, [target[0], target[1] - 190.0])?;
    advance_until(game, 7200, "real dozer scouting the MiG target", |game| {
        game.logically_visible(0, setup.target)
    })
}

pub fn details(game: &Skirmish, setup: &Setup) -> String {
    setup
        .migs
        .iter()
        .map(|&id| crate::aircraft_verification::details(game, id))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_china_hangars_migs_research_and_shared_firestorms() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
