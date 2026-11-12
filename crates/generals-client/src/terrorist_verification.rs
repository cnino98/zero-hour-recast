//! Paid original Terrorist production and ordinary contact attack on Fallen Empire.
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type, require},
    strategy_verification::template,
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{DeathType, Faction, Kind, Skirmish, WeaponFxEvent};
use std::{error::Error, path::Path};

mod poison;

pub struct Setup {
    pub actor: usize,
    pub target: usize,
    pub reserve: usize,
    pub health: f32,
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
    let setup = prepare(&mut game)?;
    game.attack(setup.actor, setup.target)?;
    advance_until(
        &mut game,
        3000,
        "ordinary Terrorist contact attack",
        |game| !game.entities[setup.actor].alive(),
    )?;
    verify_explosion(&game, &setup)?;
    advance_until(
        &mut game,
        600,
        "original Terrorist fling/sink/destruction",
        |game| game.entities[setup.actor].masked(),
    )?;
    require(
        game.entities[setup.reserve].alive(),
        "uncommanded reserve Terrorist died",
    )?;
    verify_accounting(&mut game)?;
    poison::verify(&mut game)?;
    println!(
        "Original Terrorist: paid barracks/two units, normal move/contact attack, SUICIDED callback/ordered INITIAL FX, native dynamite damage and timed corpse deletion passed"
    );
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    prepare_target(game, Faction::Usa, Kind::CommandCenter)
}

fn prepare_target(
    game: &mut Skirmish,
    enemy: Faction,
    kind: Kind,
) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, enemy)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let terrorist = template(game, "GLAInfantryTerrorist")?;
    let actor = produce_type(game, barracks, terrorist, 0)?;
    let reserve = produce_type(game, barracks, terrorist, 0)?;
    let target = if kind == Kind::Infantry {
        let worker = friendly_entity(game, Kind::Builder, 1)?;
        let barracks = construct(game, worker, Kind::Barracks, 1)?;
        produce_type(game, barracks, template(game, "GLAInfantryRebel")?, 1)?
    } else {
        friendly_entity(game, kind, 1)?
    };
    approach_target(game, actor, target)?;
    Ok(Setup {
        actor,
        target,
        reserve,
        health: game.entities[target].health,
    })
}

pub fn approach_target(
    game: &mut Skirmish,
    actor: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let origin = game.entities[actor].position;
    let goal = game.entities[target].position;
    let offset = [origin[0] - goal[0], origin[1] - goal[1]];
    let length = offset[0].hypot(offset[1]);
    let radius = game.catalog[game.entities[target].type_index]
        .geometry_bounds
        .sphere_radius
        + 70.0;
    let point = [
        (offset[0] / length).mul_add(radius, goal[0]),
        (offset[1] / length).mul_add(radius, goal[1]),
    ];
    game.move_to(actor, point)?;
    advance_until(
        game,
        18_000,
        "paid Terrorist approaching enemy target",
        |game| {
            let position = game.entities[actor].position;
            (position[0] - point[0]).hypot(position[1] - point[1]) < 15.0
        },
    )?;
    game.stop(actor)?;
    require(
        game.entities[actor].alive(),
        "Terrorist detonated without an attack",
    )?;
    Ok(())
}

fn verify_accounting(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare_target(game, Faction::Gla, Kind::Infantry)?;
    let victim = &game.catalog[game.entities[setup.target].type_index];
    require(
        victim.name == "GLAInfantryRebel",
        "accounting fixture lost original paid Rebel victim",
    )?;
    let expected_points = victim.progression.skill_point_value[0];
    require(
        expected_points > 0,
        "Rebel fixture must award native player XP",
    )?;
    let points = game.players[0].skill_points;
    let own_losses = game.players[0].statistics.units_lost;
    let enemy_losses = game.players[1].statistics.units_lost;
    let kills = game.players[0].statistics.units_destroyed;
    game.attack(setup.actor, setup.target)?;
    advance_until(
        game,
        3000,
        "paid Terrorist killing the paid enemy Rebel",
        |game| !game.entities[setup.actor].alive(),
    )?;
    verify_explosion(game, &setup)?;
    require(
        !game.entities[setup.target].alive(),
        "native dynamite failed to kill Rebel",
    )?;
    require(
        game.players[0].statistics.units_lost == own_losses + 1
            && game.players[1].statistics.units_lost == enemy_losses + 1
            && game.players[0].statistics.units_destroyed == kills + 1,
        "suicide or victim loss/kill accounting differs",
    )?;
    require(
        game.players[0].skill_points == points + expected_points
            && game.entities[setup.actor].experience == 0,
        "native Rebel player XP or untrainable Terrorist XP differs",
    )?;
    println!(
        "Original Terrorist accounting: paid Rebel killed by dynamite, one own loss, enemy loss/kill credit, positive authored player XP and untrainable unit XP passed"
    );
    Ok(())
}

pub fn verify_explosion(game: &Skirmish, setup: &Setup) -> Result<u64, Box<dyn Error>> {
    let actor = &game.entities[setup.actor];
    require(
        !actor.alive() && matches!(actor.death_type(), DeathType::Suicided),
        "contact weapon did not cause original SUICIDED death",
    )?;
    require(
        game.entities[setup.target].health < setup.health,
        "dynamite did not damage the contacted target",
    )?;
    let (event_index, event) = game
        .presentation
        .object_fx_events
        .iter()
        .enumerate()
        .find(|(_, event)| event.object == setup.actor && event.effect == "FX_TerroristExplode")
        .ok_or("missing native selected INITIAL explosion FX")?;
    let initial = game
        .presentation
        .weapon_fx
        .iter()
        .position(|event| matches!(event, WeaponFxEvent::ObjectFx(index) if *index == event_index))
        .ok_or("INITIAL effect lost callback ordering")?;
    let dynamite = game
        .presentation
        .weapon_fx
        .iter()
        .position(|event| match event {
            WeaponFxEvent::Shot(index) => {
                game.presentation.shots[*index].source == setup.actor
                    && game.presentation.shots[*index].weapon_index >= 7
            }
            _ => false,
        })
        .ok_or("death weapon did not fire through ordinary weapon callback")?;
    require(
        initial < dynamite,
        "death weapon FX preceded SlowDeath INITIAL",
    )?;
    Ok(event.frame)
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_terrorist_suicide_poison_and_accounting_match_original_commands() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
