//! Paid original Microwave tank disable, garrison-clear and emitter consumers.
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{DamageType, Faction, Kind, Skirmish};
use std::error::Error;

pub struct Setup {
    pub tank: usize,
    pub target: usize,
    pub garrison: usize,
    pub riders: Vec<usize>,
    pub barracks: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let factory = construct(game, builder, Kind::Factory, 0)?;
    let kind = template(game, "AmericaTankMicrowave")?;
    original_emitter_index(&game.catalog[kind], "MicrowaveTankEmitterWeapon")?;
    let cash = game.players[0].cash;
    require(
        game.train(factory, kind).is_err() && game.players[0].cash == cash,
        "Microwave bypassed Strategy Center prerequisite",
    )?;
    let strategy = template(game, "AmericaStrategyCenter")?;
    construct_type(game, builder, strategy, 0)?;
    let enemy_builder = friendly_entity(game, Kind::Builder, 1)?;
    construct(game, enemy_builder, Kind::PowerPlant, 1)?;
    construct(game, enemy_builder, Kind::SupplyCenter, 1)?;
    let barracks = construct(game, enemy_builder, Kind::Barracks, 1)?;
    let target = construct(game, enemy_builder, Kind::Factory, 1)?;
    let garrison = game
        .entities
        .iter()
        .position(|actor| game.catalog[actor.type_index].name == "WaterfrontWherehouse01")
        .ok_or("original Microwave diagnostic warehouse missing")?;
    let infantry = template(game, "ChinaInfantryRedguard")?;
    let mut riders = Vec::new();
    for _ in 0..2 {
        let rider = produce_type(game, barracks, infantry, 1)?;
        game.enter(rider, garrison)?;
        advance_until(
            game,
            18_000,
            "paid enemy infantry entering original map warehouse",
            |game| game.entities[rider].contained_by() == Some(garrison),
        )?;
        riders.push(rider);
    }
    let tank = produce_type(game, factory, kind, 0)?;
    let center = game.entities[target].position;
    let goal = [center[0], center[1] - 260.0];
    game.move_to(tank, goal)?;
    advance_until(
        game,
        12_000,
        "paid Microwave ordinary staging near enemy factory",
        |game| {
            let actor = &game.entities[tank];
            (actor.position[0] - goal[0]).hypot(actor.position[1] - goal[1]) < 25.0
        },
    )?;
    game.stop(tank)?;
    require(
        game.entities[target].alive()
            && !game.entities[target].subdued()
            && riders.iter().all(|&id| game.entities[id].alive()),
        "Microwave staging changed its acceptance targets",
    )?;
    Ok(Setup {
        tank,
        target,
        garrison,
        riders,
        barracks,
    })
}

// These are original-data capture prerequisites, not generic catalog admission.
pub fn original_emitter_index(
    unit: &generals_sim::skirmish::UnitType,
    expected: &str,
) -> Result<usize, Box<dyn Error>> {
    let [module] = unit.behavior.fire_weapons.as_slice() else {
        return Err("original capture requires its sole authored FireWeaponUpdate".into());
    };
    if module
        .weapon
        .as_ref()
        .is_none_or(|weapon| weapon.name != expected)
    {
        return Err("original capture requires its authored emitter Weapon".into());
    }
    unit.fire_weapon_index(0)
        .ok_or_else(|| "original capture emitter binding missing".into())
}

pub struct EmitterProduction {
    first_new: usize,
    producer: usize,
    type_index: usize,
}

pub fn queue_emitter(
    game: &mut Skirmish,
    setup: &Setup,
) -> Result<EmitterProduction, Box<dyn Error>> {
    game.stop(setup.tank)?;
    let type_index = template(game, "ChinaInfantryRedguard")?;
    let first_new = game.entities.len();
    let owner = game.entities[setup.barracks].owner;
    let cash = game.players[owner].cash;
    game.train(setup.barracks, type_index)?;
    require(
        cash.checked_sub(game.catalog[type_index].cost) == Some(game.players[owner].cash),
        "emitter infantry production did not deduct installed BuildCost",
    )?;
    Ok(EmitterProduction {
        first_new,
        producer: setup.barracks,
        type_index,
    })
}

pub fn emitter_ready(game: &Skirmish, production: &EmitterProduction) -> Option<usize> {
    game.entities[production.first_new..]
        .iter()
        .position(|actor| {
            actor.producer() == Some(production.producer)
                && actor.type_index == production.type_index
                && actor.complete()
                && !matches!(actor.ai.order, generals_sim::skirmish::Order::Exit(_))
        })
        .map(|index| production.first_new + index)
}

pub struct Emission {
    pub victim: usize,
    health: f32,
    damage: f32,
    last_actual: u64,
    pub damage_frame: Option<(u64, u8)>,
}

pub fn prepare_emitter(
    game: &mut Skirmish,
    setup: &Setup,
    victim: usize,
    last_actual: u64,
) -> Result<Emission, Box<dyn Error>> {
    game.stop(setup.tank)?;
    let victim_id = victim;
    let victim = &game.entities[victim_id];
    if !victim.alive() || victim.veterancy != 0 {
        let attacker = victim.last_attacker().map(|id| {
            let actor = &game.entities[id];
            (
                id,
                game.catalog[actor.type_index].name.as_str(),
                actor.owner,
            )
        });
        return Err(format!(
            "paid emitter victim must be a live ordinary infantry: tick={} id={} template={} owner={} health={} veterancy={} position={:?} idle={} death={} last_attacker={attacker:?}",
            game.tick, victim_id, game.catalog[victim.type_index].name,
            victim.owner, victim.health, victim.veterancy, victim.position,
            matches!(victim.ai.order, generals_sim::skirmish::Order::Idle), victim.death_type() as u8,
        ).into());
    }
    let unit = &game.catalog[game.entities[setup.tank].type_index];
    let weapon = unit
        .weapon_template(unit.fire_weapon_index(0).ok_or("missing indexed emitter")?)
        .ok_or("missing emitter weapon")?;
    let damage =
        weapon.damage * game.catalog[victim.type_index].armor[DamageType::Microwave as usize];
    require(
        damage > 0.0,
        "paid emitter victim has no Microwave damage response",
    )?;
    let emission = Emission {
        victim: victim_id,
        health: victim.health,
        damage,
        last_actual,
        damage_frame: None,
    };
    let tank_start = game.entities[setup.tank].position;
    let victim_start = game.entities[victim_id].position;
    // Move into the emitter radius without driving through the infantry.
    // The original tank would otherwise crush the target during the capture.
    let delta = [
        victim_start[0] - tank_start[0],
        victim_start[1] - tank_start[1],
    ];
    let distance = delta[0].hypot(delta[1]);
    let separation = weapon.range * 0.65;
    if distance > separation {
        let fraction = 0.5 * (1.0 - separation / distance);
        let offset = delta.map(|axis| axis * fraction);
        game.move_to(
            victim_id,
            [victim_start[0] - offset[0], victim_start[1] - offset[1]],
        )?;
        game.move_to(
            setup.tank,
            [tank_start[0] + offset[0], tank_start[1] + offset[1]],
        )?;
    }
    Ok(emission)
}

pub fn observe_emitter(
    game: &Skirmish,
    setup: &Setup,
    emission: &mut Emission,
) -> Result<(), Box<dyn Error>> {
    let index = game.catalog[game.entities[setup.tank].type_index]
        .fire_weapon_index(0)
        .ok_or("missing indexed emitter")?;
    let shot = game
        .presentation
        .shots
        .iter()
        .find(|shot| shot.source == setup.tank && shot.weapon_index == index);
    if shot.is_some() {
        require(
            game.tick >= emission.last_actual + 30,
            "emitter fired during actual-weapon exclusion",
        )?;
    }
    let health = game.entities[emission.victim].health;
    if health.to_bits() != emission.health.to_bits() {
        let shot = shot.ok_or_else(|| {
            let victim = &game.entities[emission.victim];
            let attacker = victim.last_attacker().map(|id| {
                let actor = &game.entities[id];
                (id, game.catalog[actor.type_index].name.as_str(), actor.owner)
            });
            let tank_shots = game.presentation.shots.iter()
                .filter(|shot| shot.source == setup.tank)
                .map(|shot| (shot.weapon_index, shot.weapon_slot, shot.target, shot.veterancy))
                .collect::<Vec<_>>();
            let victim_shots = game.presentation.shots.iter()
                .filter(|shot| shot.target == Some(emission.victim))
                .map(|shot| (shot.source, shot.weapon_index, shot.weapon_slot))
                .collect::<Vec<_>>();
            format!(
                "paid emitter victim damaged without actual emitter shot: tick={} victim={} health_previous={} health_current={} expected_damage={} position={:?} veterancy={} death={} last_attacker={attacker:?} emitter_index={index} tank_shots={tank_shots:?} victim_shots={victim_shots:?}",
                game.tick, emission.victim, emission.health, health, emission.damage,
                victim.position, victim.veterancy, victim.death_type() as u8,
            )
        })?;
        require(
            health.to_bits() == (emission.health - emission.damage).to_bits(),
            "paid emitter damage differs from original weapon and infantry armor",
        )?;
        emission.damage_frame = Some((game.tick, shot.veterancy));
    }
    emission.health = health;
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    let health = game.entities[setup.target].health;
    game.attack(setup.tank, setup.target)?;
    advance_until(game, 9000, "original Microwave building subdual", |game| {
        game.entities[setup.target].subdued()
    })?;
    require(
        game.entities[setup.target].health.to_bits() == health.to_bits(),
        "Microwave subdual changed ordinary building health",
    )?;
    require(
        setup.riders.iter().all(|&id| game.entities[id].alive()),
        "building disable phase killed unrelated garrison",
    )?;
    game.stop(setup.tank)?;
    advance_until(
        game,
        9000,
        "original subdual helper recovery after firing stops",
        |game| !game.entities[setup.target].subdued(),
    )?;
    let health = game.entities[setup.garrison].health;
    game.attack(setup.tank, setup.garrison)?;
    advance_until(
        game,
        18_000,
        "original Microwave direct garrison clearance",
        |game| setup.riders.iter().all(|&id| !game.entities[id].alive()),
    )?;
    require(
        game.entities[setup.garrison].health.to_bits() == health.to_bits(),
        "Microwave garrison-clear damaged the building body",
    )?;
    let mut last_actual = game.tick;
    let emitter_index = game.catalog[game.entities[setup.tank].type_index]
        .fire_weapon_index(0)
        .ok_or("missing emitter weapon")?;
    let production = queue_emitter(game, &setup)?;
    advance_until(
        game,
        9000,
        "late paid emitter infantry leaving barracks",
        |game| {
            if game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == setup.tank && shot.weapon_index != emitter_index)
            {
                last_actual = game.tick;
            }
            emitter_ready(game, &production).is_some()
        },
    )?;
    let victim = emitter_ready(game, &production).ok_or("paid emitter infantry missing")?;
    let mut emission = prepare_emitter(game, &setup, victim, last_actual)?;
    let approach_start = game.tick;
    let approach_positions = [setup.tank, victim].map(|id| game.entities[id].position);
    for _ in 0..600 {
        game.step();
        observe_emitter(game, &setup, &mut emission)?;
        if emission.damage_frame.is_some() {
            break;
        }
    }
    if emission.damage_frame.is_none() {
        return Err(format!(
            "late paid infantry never received original emitter damage within {} normal approach ticks: start={approach_positions:?}; tank {}; victim {}",
            game.tick - approach_start,
            generals_sim::skirmish::diagnostics::movement_details(game, setup.tank),
            generals_sim::skirmish::diagnostics::movement_details(game, victim),
        ).into());
    }
    println!(
        "Paid original Microwave: Strategy Center prerequisite, building subdual/recovery, direct garrison clearance and actual armor-correct enemy emitter damage passed; approach_start={approach_positions:?} ticks={} final={:?}",
        game.tick - approach_start,
        [setup.tank, victim].map(|id| game.entities[id].position),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_original_microwave_disables_recovers_and_clears_without_building_health_damage() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        super::verify(&mut battle.simulation).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn late_emitter_payment_keeps_current_effects_and_waits_for_real_production_exit() {
        use crate::skirmish_verification::advance_until;
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let game = &mut battle.simulation;
        let setup = super::prepare(game).unwrap();
        game.attack(setup.tank, setup.target).unwrap();
        advance_until(game, 9000, "actual Microwave disabler shot", |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == setup.tank)
        })
        .unwrap();
        let tick = game.tick;
        let count = game.entities.len();
        let shots = game.presentation.shots.len();
        let effects = game.presentation.weapon_fx.len();
        assert!(shots > 0);
        let production = super::queue_emitter(game, &setup).unwrap();
        assert_eq!(game.tick, tick);
        assert_eq!(game.entities.len(), count);
        assert_eq!(game.presentation.shots.len(), shots);
        assert_eq!(game.presentation.weapon_fx.len(), effects);
        assert!(super::emitter_ready(game, &production).is_none());
        advance_until(game, 9000, "actual paid emitter infantry exit", |game| {
            super::emitter_ready(game, &production).is_some()
        })
        .unwrap();
        let id = super::emitter_ready(game, &production).unwrap();
        assert!(id >= count);
        let actor = &game.entities[id];
        assert!(actor.alive());
        assert_eq!(actor.veterancy, 0);
        assert_eq!(actor.producer(), Some(setup.barracks));
        assert_eq!(game.catalog[actor.type_index].name, "ChinaInfantryRedguard");
        assert!(game.tick > tick);
        let ready_tick = game.tick;
        let health = actor.health;
        let positions = [setup.tank, id].map(|id| game.entities[id].position);
        super::prepare_emitter(game, &setup, id, tick).unwrap();
        assert_eq!(game.tick, ready_tick);
        assert_eq!(game.entities[id].health.to_bits(), health.to_bits());
        assert_eq!(game.entities[id].veterancy, 0);
        for actor in [setup.tank, id] {
            assert!(matches!(
                game.entities[actor].ai.order,
                generals_sim::skirmish::Order::Move(_)
            ));
        }
        assert_eq!(
            [setup.tank, id].map(|id| game.entities[id].position.map(f32::to_bits)),
            positions.map(|position| position.map(f32::to_bits)),
        );
    }
}
