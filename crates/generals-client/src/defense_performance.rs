//! Paid dense-defense workload for the reported garrison/Patriot hitch.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind, PlayerSlot, Skirmish, diagnostics};
use std::{
    error::Error,
    hash::{DefaultHasher, Hasher},
    path::Path,
    time::{Duration, Instant},
};

pub struct Setup {
    pub bases: Vec<usize>,
    pub patriots: Vec<usize>,
    pub riders: Vec<usize>,
    pub enemies: Vec<usize>,
}

pub fn start(app: &mut bevy::prelude::App) -> Result<(), Box<dyn Error>> {
    let mut battle = app.world_mut().resource_mut::<crate::playtest::Match>();
    let setup = prepare(&mut battle.simulation)?;
    attack(&mut battle.simulation, &setup)?;
    advance_until(
        &mut battle.simulation,
        6000,
        "tank force approaching garrisoned defenses",
        |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| setup.riders.contains(&shot.source))
                && game.projectiles.len() >= 4
        },
    )?;
    battle.started = true;
    battle.paused = false;
    battle.select_objects(&[setup.bases[0]], true);
    let focus = battle.simulation.entities[setup.bases[0]].position;
    let generation = battle.generation;
    println!(
        "DEFENSE_LIVE frame={} records={} projectiles={}",
        battle.simulation.tick,
        battle.simulation.entities.len(),
        battle.simulation.projectiles.len()
    );
    let mut rig = app.world_mut().resource_mut::<crate::playtest::CameraRig>();
    rig.focus = bevy::prelude::Vec2::from_array(focus) * 0.1;
    // The ordinary first-frame camera reset otherwise replaces this battle
    // focus with the command center before any measured or captured frame.
    rig.generation = generation;
    app.world_mut()
        .resource_mut::<crate::playtest_view::Capture>()
        .wait_for_scenario();
    Ok(())
}

pub fn run(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut game = crate::opponent_verification::load_game(root, crate::playtest::FOUR_PLAYER_MAP)?;
    let setup = prepare(&mut game)?;
    let mut hash = DefaultHasher::new();
    measure(&mut game, &setup, "idle", &mut hash)?;
    let command = Instant::now();
    attack(&mut game, &setup)?;
    println!(
        "DEFENSE_COMMAND count={} elapsed_us={}",
        setup.enemies.len(),
        command.elapsed().as_micros()
    );
    measure(&mut game, &setup, "combat", &mut hash)?;
    println!("DEFENSE_TRACE value={:016x}", hash.finish());
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let slots = (0..game.slots.len())
        .map(|owner| PlayerSlot {
            faction: Some(if owner == 0 {
                Faction::Usa
            } else {
                Faction::China
            }),
            occupied: owner < 2,
            difficulty: None,
            team: None,
            start: Some(owner),
            color: Some(owner),
        })
        .collect();
    game.reset_slots(slots)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let mut setup = Setup {
        bases: Vec::new(),
        patriots: Vec::new(),
        riders: Vec::new(),
        enemies: Vec::new(),
    };
    for _ in 0..4 {
        let base = funded_build(game, worker, "AmericaFireBase", 0)?;
        setup.bases.push(base);
        for name in [
            "AmericaInfantryRanger",
            "AmericaInfantryMissileDefender",
            "AmericaInfantryRanger",
            "AmericaInfantryMissileDefender",
        ] {
            let kind = template(game, name)?;
            funds(game, kind, 0)?;
            let rider = produce_type(game, barracks, kind, 0)?;
            game.enter(rider, base)?;
            advance_until(game, 18000, "dense-defense garrison entry", |game| {
                game.entities[rider].contained_by() == Some(base)
            })?;
            setup.riders.push(rider);
        }
        funded_build(game, worker, "AmericaPowerPlant", 0)?;
        setup
            .patriots
            .push(funded_build(game, worker, "AmericaPatriotBattery", 0)?);
    }
    let worker = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, worker, Kind::PowerPlant, 1)?;
    construct(game, worker, Kind::SupplyCenter, 1)?;
    let factory = construct(game, worker, Kind::Factory, 1)?;
    for _ in 0..24 {
        let kind = template(game, "ChinaTankBattleMaster")?;
        funds(game, kind, 1)?;
        setup.enemies.push(produce_type(game, factory, kind, 1)?);
    }
    println!(
        "DEFENSE_SETUP frame={} fire_bases={} patriots={} stationed={} tanks={} records={} power={}",
        game.tick,
        setup.bases.len(),
        setup.patriots.len(),
        setup.riders.len(),
        setup.enemies.len(),
        game.entities.len(),
        game.players[0].power
    );
    require(
        game.players[0].power >= 0 && setup.riders.iter().all(|&id| game.entities[id].stationed()),
        "dense-defense setup lacks powered defenses or stationed infantry",
    )?;
    Ok(setup)
}

pub fn attack(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    for (index, &enemy) in setup.enemies.iter().enumerate() {
        game.attack(enemy, setup.bases[index % setup.bases.len()])?;
    }
    Ok(())
}

fn funded_build(
    game: &mut Skirmish,
    worker: usize,
    name: &str,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let kind = template(game, name)?;
    funds(game, kind, owner)?;
    construct_type(game, worker, kind, owner)
}

fn funds(game: &mut Skirmish, kind: usize, owner: usize) -> Result<(), Box<dyn Error>> {
    advance_until(
        game,
        18000,
        "ordinary supply income funding defense workload",
        |game| game.players[owner].cash >= game.catalog[kind].cost,
    )
}

fn measure(
    game: &mut Skirmish,
    setup: &Setup,
    label: &str,
    hash: &mut DefaultHasher,
) -> Result<(), Box<dyn Error>> {
    let mut times = Vec::with_capacity(6000);
    let mut firing_times = Vec::new();
    let mut shots = [0; 4];
    let mut projectiles = 0;
    for _ in 0..6000 {
        let started = Instant::now();
        game.step();
        let elapsed = started.elapsed();
        times.push(elapsed);
        if !game.presentation.shots.is_empty() || !game.projectiles.is_empty() {
            firing_times.push(elapsed);
        }
        if elapsed >= Duration::from_millis(50) {
            println!(
                "DEFENSE_SLOW phase={label} frame={} elapsed_us={} records={} projectiles={} shots={}",
                game.tick,
                elapsed.as_micros(),
                game.entities.len(),
                game.projectiles.len(),
                game.presentation.shots.len()
            );
        }
        projectiles = projectiles.max(game.projectiles.len());
        for shot in &game.presentation.shots {
            let index = if setup.bases.contains(&shot.source) {
                0
            } else if setup.patriots.contains(&shot.source) {
                1
            } else if setup.riders.contains(&shot.source) {
                2
            } else {
                3
            };
            shots[index] += 1;
        }
        diagnostics::record(game, hash);
    }
    times.sort_unstable();
    let at = |percent: usize| times[(times.len() - 1) * percent / 100].as_micros();
    println!(
        "DEFENSE_PERF phase={label} frames={} p50_us={} p95_us={} max_us={} shots={shots:?} peak_projectiles={projectiles} trace={:016x}",
        times.len(),
        at(50),
        at(95),
        at(100),
        hash.finish()
    );
    if !firing_times.is_empty() {
        firing_times.sort_unstable();
        let at =
            |percent: usize| firing_times[(firing_times.len() - 1) * percent / 100].as_micros();
        println!(
            "DEFENSE_WEAPON_ACTIVITY phase={label} frames={} p50_us={} p95_us={} max_us={}",
            firing_times.len(),
            at(50),
            at(95),
            at(100)
        );
    }
    if label == "combat" {
        require(
            shots[..3].iter().all(|&shots| shots > 0),
            "defense workload did not exercise howitzers, Patriots and garrison weapons",
        )?;
    }
    Ok(())
}
