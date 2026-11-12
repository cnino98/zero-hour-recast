//! Paid original Angry Mob, its six authored child types, and Arm the Mob.
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

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    run_with(root, verify)
}

pub fn run_with(
    root: &Path,
    check: fn(&mut Skirmish) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
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
    check(&mut game)
}

pub struct Setup {
    pub nexus: usize,
    pub palace: usize,
    pub target: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Gla)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let mob = template(game, "GLAInfantryAngryMobNexus")?;
    let cash = game.players[0].cash;
    require(
        !game.prerequisites_met(0, mob)
            && game.train(barracks, mob).is_err()
            && game.players[0].cash == cash,
        "Mob bypassed Palace prerequisite or charged rejected production",
    )?;
    construct(game, worker, Kind::Factory, 0)?;
    let palace_type = template(game, "GLAPalace")?;
    let palace = construct_type(game, worker, palace_type, 0)?;
    let nexus = produce_type(game, barracks, mob, 0)?;
    advance_until(
        game,
        1200,
        "original ten-member Mob production exits",
        |game| game.spawned_children(nexus).len() == 10,
    )?;
    verify_children(game, nexus)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let goal = game.entities[target].position;
    game.move_to(worker, [goal[0], goal[1] - 180.0])?;
    advance_until(game, 7200, "real Worker scouting the Mob target", |game| {
        game.logically_visible(0, target)
    })?;
    Ok(Setup {
        nexus,
        palace,
        target,
    })
}

pub fn verify_children(game: &Skirmish, nexus: usize) -> Result<(), Box<dyn Error>> {
    let children = game.spawned_children(nexus);
    require(children.len() == 10, "original Mob has ten members")?;
    let sequence = [
        "GLAInfantryAngryMobPistol01",
        "GLAInfantryAngryMobRock02",
        "GLAInfantryAngryMobMolotov02",
        "GLAInfantryAngryMobPistol03",
        "GLAInfantryAngryMobRock04",
        "GLAInfantryAngryMobMolotov02",
        "GLAInfantryAngryMobPistol05",
    ];
    for (index, &child) in children.iter().enumerate() {
        let actor = &game.entities[child];
        require(
            actor.alive()
                && actor.spawn_master() == Some(nexus)
                && !actor.disabled_held()
                && game.catalog[actor.type_index].name == sequence[index % sequence.len()],
            "Mob lost an authored child type, spawn order, master or normal ground movement",
        )?;
    }
    Ok(())
}

pub fn weapon_names(game: &Skirmish, nexus: usize) -> Vec<String> {
    game.presentation
        .shots
        .iter()
        .filter(|shot| game.entities[shot.source].spawn_master() == Some(nexus))
        .filter_map(|shot| {
            game.catalog[game.entities[shot.source].type_index].weapon_template(shot.weapon_index)
        })
        .map(|weapon| weapon.name.clone())
        .collect()
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    game.attack(setup.nexus, setup.target)?;
    let mut seen = [false; 3];
    for _ in 0..7200 {
        game.step();
        for name in weapon_names(game, setup.nexus) {
            match name.as_str() {
                "GLAAngryMobPistolWeapon" => seen[0] = true,
                "GLAAngryMobRockProjectileWeapon" => seen[1] = true,
                "GLAAngryMobMolotovCocktailProjectileWeapon" => seen[2] = true,
                _ => {}
            }
        }
        if seen.into_iter().all(|value| value) {
            break;
        }
    }
    require(
        seen.into_iter().all(|value| value),
        "normal Mob did not actually fire pistol, rock and Molotov weapons",
    )?;
    game.stop(setup.nexus)?;
    research(game, setup.palace, "Upgrade_GLAArmTheMob")?;
    game.attack(setup.nexus, setup.target)?;
    let mut armed = [false; 2];
    for _ in 0..3600 {
        game.step();
        for name in weapon_names(game, setup.nexus) {
            match name.as_str() {
                "GLAAngryMobAK47Weapon" => armed[0] = true,
                "GLAAngryMobMolotovCocktailProjectileWeapon" => armed[1] = true,
                _ => {}
            }
        }
        if armed.into_iter().all(|value| value) {
            break;
        }
    }
    require(
        armed.into_iter().all(|value| value),
        "Arm the Mob did not fire AK47 while retaining Molotov children",
    )?;
    verify_replacement(game, setup.nexus)?;
    println!(
        "Paid original Angry Mob: Barracks/Palace gate, ten members/six types, normal group attack, researched AK47/Molotov weapons and genuine Scorpion casualty/replacement passed at {}",
        game.tick
    );
    Ok(())
}

fn verify_replacement(game: &mut Skirmish, nexus: usize) -> Result<(), Box<dyn Error>> {
    game.stop(nexus)?;
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    let dealer = construct(game, worker, Kind::Factory, 1)?;
    let tank_type = template(game, "GLATankScorpion")?;
    let tank = produce_type(game, dealer, tank_type, 1)?;
    let child = *game
        .spawned_children(nexus)
        .first()
        .ok_or("Mob has no surviving member for real casualty test")?;
    game.attack(tank, child)?;
    advance_until(
        game,
        7200,
        "paid enemy Scorpion causing an original Mob casualty",
        |game| !game.entities[child].alive(),
    )?;
    let died = game.tick;
    game.stop(tank)?;
    let center = friendly_entity(game, Kind::CommandCenter, 0)?;
    let home = game.entities[center].position;
    game.move_to(nexus, [home[0] + 100.0, home[1] + 100.0])?;
    let original = game.spawned_children(nexus).to_vec();
    advance_until(
        game,
        2400,
        "original thirty-second Mob replacement",
        |game| {
            game.spawned_children(nexus)
                .iter()
                .any(|id| !original.contains(id))
        },
    )?;
    require(
        game.tick > died + 900 && !game.spawned_children(nexus).contains(&child),
        "Mob replacement bypassed strict native due frame or reused its dead child",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_paid_mob_masked_nexus_leaves_barracks_and_drives_its_real_children() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = super::prepare(game).unwrap();
        let parent = &game.entities[setup.nexus];
        assert!(parent.masked());
        assert!(game.catalog[parent.type_index].live_physics.is_some());
        let producer = parent.producer().unwrap();
        assert_eq!(
            game.catalog[game.entities[producer].type_index].name,
            "GLABarracks"
        );
        let origin = parent.position;
        let birth = game.entities[producer].position;
        assert!(
            (origin[0] - birth[0]).hypot(origin[1] - birth[1]) > 30.0,
            "paid masked Nexus must finish its original producer exit"
        );
        super::verify_children(game, setup.nexus).unwrap();
        let children = game.spawned_children(setup.nexus).to_vec();
        let positions: Vec<_> = children
            .iter()
            .map(|&child| game.entities[child].position)
            .collect();
        let goal = game.entities[setup.target].position;
        game.move_to(setup.nexus, goal).unwrap();
        for _ in 0..600 {
            game.step();
            if (game.entities[setup.nexus].position[0] - origin[0])
                .hypot(game.entities[setup.nexus].position[1] - origin[1])
                > 100.0
            {
                break;
            }
        }
        let parent = &game.entities[setup.nexus];
        assert!(parent.masked());
        assert!(
            (parent.position[0] - origin[0]).hypot(parent.position[1] - origin[1]) > 100.0,
            "masked Nexus must continue through its real locomotor/Physics callbacks"
        );
        super::verify_children(game, setup.nexus).unwrap();
        assert!(
            children.iter().zip(positions).all(|(&child, original)| {
                let actor = &game.entities[child];
                actor.spawn_master() == Some(setup.nexus)
                    && (actor.position[0] - original[0]).hypot(actor.position[1] - original[1])
                        > 10.0
            }),
            "all ten actual Mob children must follow their moving masked parent"
        );
    }
}
