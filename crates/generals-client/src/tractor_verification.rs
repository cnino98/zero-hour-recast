//! Paid original Toxin Tractor command, stream, contamination and Anthrax acceptance.
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
use generals_sim::skirmish::{Faction, Kind, Skirmish, hazards::Origin};
use std::{error::Error, path::Path};

pub struct Setup {
    pub actor: usize,
    pub target: usize,
    pub factory: usize,
    pub builder: usize,
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
    animations.verify_tractor_art(&data.models, &data.types)?;
    let options = root.join("UserData/Options.ini");
    drop(crate::control_bar::load(&assets, &data.types, &options)?);
    drop(crate::source_audio::load(&assets, &data.types, &options)?);
    drop(crate::attack_cursor::load(&assets)?);
    drop(crate::projectile_stream::load(&assets, &data.types)?);
    crate::particles::verify(&assets, &data.types)?;
    let movement = terrain.classify_movement(&map.triggers, &map.objects)?;
    let mut game = data.into_simulation(movement, layout)?;
    terrain.configure_projectiles(&mut game, projectiles.partition_cell_size)?;
    verify(&mut game)
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let actor = produce_type(game, factory, template(game, "GLAVehicleToxinTruck")?, 0)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let from = game.entities[actor].position;
    let to = game.entities[target].position;
    let delta = [from[0] - to[0], from[1] - to[1]];
    let distance = delta[0].hypot(delta[1]);
    let point = [
        (delta[0] / distance).mul_add(125.0, to[0]),
        (delta[1] / distance).mul_add(125.0, to[1]),
    ];
    game.move_to(actor, point)?;
    advance_until(
        game,
        18_000,
        "paid Tractor approaching ordinary stream range",
        |game| {
            let position = game.entities[actor].position;
            (position[0] - point[0]).hypot(position[1] - point[1]) < 15.0
        },
    )?;
    game.stop(actor)?;
    Ok(Setup {
        actor,
        target,
        factory,
        builder,
    })
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    stream_damage(game, &setup, false)?;
    contaminate(game, setup.actor, false)?;
    let palace = construct_type(game, setup.builder, template(game, "GLAPalace")?, 0)?;
    research(game, palace, "Upgrade_GLAAnthraxBeta")?;
    stream_damage(game, &setup, true)?;
    contaminate(game, setup.actor, true)?;
    let future = produce_type(
        game,
        setup.factory,
        template(game, "GLAVehicleToxinTruck")?,
        0,
    )?;
    let actor = &game.entities[future];
    require(
        actor.active_weapon(&game.catalog[actor.type_index]).name == "ToxinTruckGunUpgraded",
        "future Tractor did not inherit paid Anthrax Beta",
    )?;
    println!(
        "Installed Tractor: paid production, real toxin stream and damage, original secondary ground fire, post-cooldown field lifetime, Anthrax research and future-unit inheritance passed"
    );
    Ok(())
}

fn stream_damage(game: &mut Skirmish, setup: &Setup, beta: bool) -> Result<(), Box<dyn Error>> {
    let health = game.entities[setup.target].health;
    let expected = if beta {
        "ToxinTruckGunUpgraded"
    } else {
        "ToxinTruckGun"
    };
    let unit = &game.catalog[game.entities[setup.actor].type_index];
    let index = (0..unit.weapon_template_count())
        .find(|&index| {
            unit.weapon_template(index)
                .is_some_and(|weapon| weapon.name == expected)
        })
        .ok_or("Tractor expected original weapon template absent")?;
    game.attack(setup.actor, setup.target)?;
    let mut fired = false;
    let mut missile = false;
    let mut stream = false;
    for _ in 0..900 {
        game.step();
        fired |= game.presentation.shots.iter().any(|shot| {
            shot.source == setup.actor && shot.weapon_slot == 0 && shot.weapon_index == index
        });
        missile |= game
            .projectiles
            .iter()
            .any(|projectile| projectile.source == setup.actor && projectile.weapon_index == index);
        stream |= game
            .projectile_streams
            .iter()
            .any(|stream| stream.source == setup.actor && stream.weapon_index == index);
        if fired && missile && stream && game.entities[setup.target].health < health {
            let actor = &game.entities[setup.actor];
            let weapon = actor.active_weapon(&game.catalog[actor.type_index]);
            if weapon.name != expected {
                return Err(format!(
                    "Tractor fired wrong original primary weapon: expected={expected} actual={} slot={} upgraded={} player_beta={} salvage={} actor={} target={} frame={} health={health}->{} shots={:?}",
                    weapon.name,
                    actor.weapon_slot(),
                    actor.weapon_set_upgraded(),
                    game.has_upgrade(actor.owner, "Upgrade_GLAAnthraxBeta"),
                    actor.salvage_level(),
                    setup.actor,
                    setup.target,
                    game.tick,
                    game.entities[setup.target].health,
                    game.presentation.shots.iter().filter(|shot| shot.source == setup.actor)
                        .map(|shot| (shot.weapon_slot, shot.weapon_index)).collect::<Vec<_>>()
                ).into());
            }
            game.stop(setup.actor)?;
            return Ok(());
        }
    }
    Err(format!(
        "Tractor stream acceptance failed: expected={expected} fired={fired} missile={missile} stream={stream} health={health}->{} active={} slot={} upgraded={} salvage={}",
        game.entities[setup.target].health,
        game.entities[setup.actor].active_weapon(&game.catalog[game.entities[setup.actor].type_index]).name,
        game.entities[setup.actor].weapon_slot(),
        game.entities[setup.actor].weapon_set_upgraded(),
        game.entities[setup.actor].salvage_level(),
    )
    .into())
}

fn contaminate(game: &mut Skirmish, actor: usize, beta: bool) -> Result<(), Box<dyn Error>> {
    let point = game.entities[actor].position;
    let start = game.tick;
    game.fire_weapon_at(actor, 1, [point[0] + 4.0, point[1]])?;
    let mut shots = 0;
    for _ in 0..42 {
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == actor && shot.weapon_slot == 1)
            .count();
    }
    require(
        shots >= 4,
        "original contamination did not fire the minimum four sprayer shots",
    )?;
    game.stop(actor)?;
    advance_until(game, 30, "Tractor cooldown poison field", |game| {
        game.hazards.iter().any(|field| {
            field.source == actor
                && field.created >= start
                && matches!(field.origin, Origin::Cooldown(_))
        })
    })?;
    let field = game
        .hazards
        .iter()
        .find(|field| field.source == actor && field.created >= start)
        .ok_or("Tractor field vanished")?;
    let rules = game.catalog[game.entities[actor].type_index]
        .hazard_rules(field.origin)
        .ok_or("Tractor field metadata absent")?;
    require(
        matches!(
            rules.death_type,
            generals_sim::skirmish::DeathType::PoisonedBeta
        ) == beta
            && field.owner == 0
            && field.expires > field.created
            && field.expires - field.created < 900,
        "Tractor lost source contamination variant, owner or duration override",
    )?;
    let expires = field.expires;
    advance_until(game, 900, "Tractor source field lifetime", |game| {
        game.tick >= expires
            && !game
                .hazards
                .iter()
                .any(|field| field.source == actor && field.created >= start)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_tractor_uses_original_weapons_before_and_after_anthrax_research() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
