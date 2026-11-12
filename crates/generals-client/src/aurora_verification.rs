//! Paid Aurora production, original supersonic sorties, rearm and physical crash.
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

pub struct Setup {
    pub airfield: usize,
    pub center: usize,
    pub aircraft: usize,
    pub target: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let airfield_type = template(game, "AmericaAirfield")?;
    let airfield = construct_type(game, builder, airfield_type, 0)?;
    let kind = template(game, "AmericaJetAurora")?;
    let cash = game.players[0].cash;
    require(
        !game.prerequisites_met(0, kind)
            && game.train(airfield, kind).is_err()
            && game.players[0].cash == cash,
        "Aurora bypassed its Strategy Center prerequisite or charged a rejected queue",
    )?;
    let center_type = template(game, "AmericaStrategyCenter")?;
    let center = construct_type(game, builder, center_type, 0)?;
    let id = produce_type(game, airfield, kind, 0)?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let point = game.entities[target].position;
    game.move_to(builder, [point[0], point[1] - 190.0])?;
    advance_until(
        game,
        7200,
        "real dozer scouting the Aurora target",
        |game| game.logically_visible(0, target),
    )?;
    Ok(Setup {
        airfield,
        center,
        aircraft: id,
        target,
    })
}

fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let Setup {
        airfield,
        center,
        aircraft: id,
        target,
    } = setup;
    sortie(game, id, target)?;
    println!("Paid Aurora first sortie/rearm passed at {}", game.tick);
    research(game, center, "Upgrade_AmericaAdvancedTraining")?;
    sortie(game, id, target)?;
    println!(
        "Paid Aurora researched second sortie/rearm passed at {}",
        game.tick
    );
    crash(game, id, airfield)?;
    println!(
        "Paid Aurora: Strategy Center prerequisite, one-bomb supersonic sorties, sluggish return/rearm, Advanced Training and physical crash/replacement passed at {}",
        game.tick
    );
    Ok(())
}

pub fn approach(game: &mut Skirmish, id: usize, target: usize) -> Result<(), Box<dyn Error>> {
    let point = game.entities[target].position;
    game.move_to(id, [point[0], point[1] - 800.0])?;
    advance_until(
        game,
        7200,
        "Aurora taxi and ordinary airborne staging",
        |game| {
            let actor = &game.entities[id];
            actor.aircraft_phase() == Some("idle")
                && game.height_above_ground(id) > 30.0
                && (actor.position[0] - point[0]).hypot(actor.position[1] - point[1] + 800.0)
                    < 200.0
        },
    )?;
    require(
        game.entities[id].weapon_rounds_remaining(0) == 1,
        "Aurora staging consumed its bomb",
    )
}

fn sortie(game: &mut Skirmish, id: usize, target: usize) -> Result<(), Box<dyn Error>> {
    approach(game, id, target)?;
    let health = game.entities[target].health;
    let mut minimum = health;
    let mut shots = 0;
    let mut supersonic = false;
    let mut sluggish = false;
    let mut projectile = false;
    let mut landing = false;
    game.attack(id, target)?;
    for _ in 0..9000 {
        game.step();
        let actor = &game.entities[id];
        let unit = &game.catalog[actor.type_index];
        let motion = actor.aircraft_draw_motion(unit, game.tick);
        let rules = unit
            .aircraft
            .as_ref()
            .ok_or("Aurora lacks original aircraft rules")?;
        if let Some(motion) = motion {
            let attack = rules
                .attacking
                .as_ref()
                .and_then(|set| set.locomotor.flight.as_ref())
                .ok_or("Aurora lacks supersonic locomotor")?;
            let returning = rules
                .returning
                .as_ref()
                .and_then(|set| set.locomotor.flight.as_ref())
                .ok_or("Aurora lacks sluggish locomotor")?;
            supersonic |= std::ptr::eq(motion.rules, &raw const attack.draw);
            sluggish |= std::ptr::eq(motion.rules, &raw const returning.draw);
        }
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == id)
            .count();
        projectile |= game
            .projectiles
            .iter()
            .any(|projectile| projectile.source == id);
        landing |= matches!(
            actor.aircraft_phase(),
            Some("landing" | "reload ammunition")
        );
        minimum = minimum.min(game.entities[target].health);
        if shots > 0
            && actor.aircraft_phase() == Some("idle")
            && actor.weapon_rounds_remaining(0) == 1
            && game.height_above_ground(id) < 30.0
        {
            require(shots == 1, "Aurora fired more than its one-bomb clip")?;
            require(
                supersonic && sluggish && projectile && landing,
                "Aurora omitted authored supersonic, sluggish return, projectile or landing state",
            )?;
            require(
                minimum < health,
                "Aurora bomb did not damage its real target",
            )?;
            return Ok(());
        }
    }
    Err(format!("Aurora sortie timed out shots={shots} supersonic={supersonic} sluggish={sluggish} projectile={projectile} landing={landing}; {}", crate::aircraft_verification::details(game, id)).into())
}

fn prepare_defenders(game: &mut Skirmish, center: [f32; 2]) -> Result<[usize; 2], Box<dyn Error>> {
    let builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, builder, Kind::PowerPlant, 1)?;
    construct(game, builder, Kind::SupplyCenter, 1)?;
    let factory = construct(game, builder, Kind::Factory, 1)?;
    let attacker_type = template(game, "ChinaTankGattling")?;
    let mut defenders = [0; 2];
    for (index, attacker) in defenders.iter_mut().enumerate() {
        *attacker = produce_type(game, factory, attacker_type, 1)?;
        let point = [center[0] + 600.0, center[1] - 350.0 + [0.0, 150.0][index]];
        game.move_to(*attacker, point).map_err(|error| {
            format!(
                "paid Aurora AA staging: {error}; {}",
                generals_sim::skirmish::diagnostics::movement_details(game, *attacker)
            )
        })?;
        advance_until(
            game,
            7200,
            "paid Gattling staging outside Aurora approach",
            |game| {
                let actor = &game.entities[*attacker];
                actor.alive()
                    && !actor.moving()
                    && matches!(actor.ai.order, Order::Idle)
                    && (actor.position[0] - center[0]).hypot(actor.position[1] - center[1]) > 500.0
            },
        )?;
    }
    Ok(defenders)
}

fn crash(game: &mut Skirmish, id: usize, airfield: usize) -> Result<(), Box<dyn Error>> {
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    let center = game.entities[target].position;
    let defenders = prepare_defenders(game, center)?;
    let point = [center[0] + 100.0, center[1] - 500.0];
    game.move_to(id, point).map_err(|error| {
        format!(
            "paid Aurora takeoff for crash: {error}; {}",
            crate::aircraft_verification::details(game, id)
        )
    })?;
    advance_until(game, 7200, "Aurora airborne crash staging", |game| {
        let actor = &game.entities[id];
        actor.aircraft_phase() == Some("idle")
            && game.height_above_ground(id) > 30.0
            && (actor.position[0] - point[0]).hypot(actor.position[1] - point[1]) < 150.0
    })?;
    game.guard(id, point).map_err(|error| {
        format!(
            "paid Aurora airborne Guard before AA approach: {error}; {}",
            crate::aircraft_verification::details(game, id)
        )
    })?;
    // Aurora's one real bomb can kill a Gattling outright. Two separated paid
    // attackers let ordinary Guard counterfire while the other closes and fires.
    for (&attacker, offset) in defenders.iter().zip([-70.0, 70.0]) {
        game.move_to(attacker, [point[0] + offset, point[1]])?;
    }
    println!(
        "Paid Aurora crash staged: aircraft={id}, defenders={defenders:?} at {}",
        game.tick
    );
    let mut seen = [false; 4];
    let mut shots = 0;
    for _ in 0..9000 {
        for attacker in defenders {
            if game.entities[id].alive()
                && game.entities[attacker].alive()
                && game.logically_visible(1, id)
                && !matches!(game.entities[attacker].ai.order, Order::Attack(target) if target == id)
            {
                game.attack(attacker, id).map_err(|error| {
                    format!(
                        "paid Aurora AA Attack: {error}; defender {}; aircraft {}",
                        generals_sim::skirmish::diagnostics::movement_details(game, attacker),
                        crate::aircraft_verification::details(game, id)
                    )
                })?;
            }
        }
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| defenders.contains(&shot.source))
            .count();
        seen[0] |= game.entities[id].jet_corpse()
            && game.entities[id]
                .last_attacker()
                .is_some_and(|attacker| defenders.contains(&attacker));
        seen[1] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_AuroraDeathInitial");
        seen[2] |= game
            .presentation
            .object_fx_events
            .iter()
            .any(|event| event.object == id && event.effect == "FX_JetDeathHitGround");
        seen[3] |= game.entities.iter().any(|actor| {
            actor.producer() == Some(id)
                && actor.physical_present()
                && game.catalog[actor.type_index].name == "AmericaJetAuroraHulk"
        });
        if seen.into_iter().all(|value| value)
            && game.entities[id].masked()
            && game
                .entities
                .iter()
                .all(|actor| actor.producer() != Some(id) || !actor.physical_present())
        {
            require(shots > 0, "Aurora crash had no real AA shots")?;
            let kind = game.entities[id].type_index;
            require(
                game.parking_available(airfield, kind),
                "Aurora death failed to release its paid hangar",
            )?;
            let replacement = produce_type(game, airfield, kind, 0)?;
            require(
                game.entities[replacement].producer() == Some(airfield),
                "replacement Aurora lost its paid airfield",
            )?;
            return Ok(());
        }
    }
    Err(format!(
        "Aurora crash timed out shots={shots} corpse/initial/ground/hulk={seen:?}; {}",
        crate::aircraft_verification::details(game, id)
    )
    .into())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_aurora_supersonic_bomb_rearm_research_crash_and_replacement() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }
}
