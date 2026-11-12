//! Paid source `BuildVariations` production, combat crate drops and native pickups.
use super::{advance_until, construct, friendly_entity, produce_type, require};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub struct Capture {
    pub technicals: [usize; 3],
    pub marauder: usize,
    pub rider: usize,
    pub enemy_center: usize,
    pub stage: [f32; 2],
}
pub fn prepare(game: &mut Skirmish) -> Result<Capture, Box<dyn Error>> {
    game.reset(Faction::Gla, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    let base = game.entities[factory].position;
    let stage = [base[0] - 350.0, base[1] - 200.0];
    let wrapper = template(game, "GLAVehicleTechnical")?;
    let mut technicals = [0; 3];
    for (offset, id) in technicals.iter_mut().enumerate() {
        *id = funded_unit(game, factory, wrapper, 0)?;
        let displacement = f32::from(u16::try_from(offset)?) * 95.0;
        park(game, *id, [stage[0] + displacement, stage[1]])?;
        require(
            game.catalog[game.entities[*id].type_index]
                .name
                .starts_with("GLAVehicleTechnicalChassis"),
            "Technical wrapper spawned instead of its native build variation",
        )?;
    }
    game.purchase_science(0, "SCIENCE_MarauderTank")?;
    let index = template(game, "GLATankMarauder")?;
    let marauder = funded_unit(game, factory, index, 0)?;
    park(game, marauder, [stage[0] + 285.0, stage[1]])?;
    let enemy_center = friendly_entity(game, Kind::CommandCenter, 1)?;
    let enemy_worker = template(game, "AmericaVehicleDozer")?;
    for (collector, count) in [(technicals[1], 1), (technicals[2], 2), (marauder, 2)] {
        for _ in 0..count {
            salvage_one(game, collector, enemy_center, enemy_worker, stage)?;
        }
        let slot: f32 = if collector == technicals[1] {
            1.0
        } else if collector == technicals[2] {
            2.0
        } else {
            3.0
        };
        park(game, collector, [slot.mul_add(95.0, stage[0]), stage[1]])?;
    }
    let rebel = template(game, "GLAInfantryRebel")?;
    let rider = funded_unit(game, barracks, rebel, 0)?;
    game.enter(rider, technicals[0])?;
    advance_until(game, 6000, "Technical infantry boarding", |game| {
        game.entities[rider].contained_by() == Some(technicals[0])
    })?;
    require(
        game.container_rules(technicals[0])
            .is_some_and(|rules| rules.capacity == 5)
            && game.passengers(technicals[0]) == [rider],
        "Technical original five-slot transport missing",
    )?;
    for (id, level) in [
        (technicals[0], 0),
        (technicals[1], 1),
        (technicals[2], 2),
        (marauder, 2),
    ] {
        require(
            game.entities[id].salvage_level() == level,
            "paid combat crate progression differs from source",
        )?;
    }
    println!(
        "Installed Technical: paid wrapper chose native chassis; infantry transport and actual enemy deaths/pickups upgraded Technical MG/cannon/RPG and Marauder twice at frame {}",
        game.tick
    );
    Ok(Capture {
        technicals,
        marauder,
        rider,
        enemy_center,
        stage,
    })
}
fn template(game: &Skirmish, name: &str) -> Result<usize, Box<dyn Error>> {
    game.catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or_else(|| format!("missing {name}").into())
}
fn funded_unit(
    game: &mut Skirmish,
    producer: usize,
    index: usize,
    owner: usize,
) -> Result<usize, Box<dyn Error>> {
    let cost = game.catalog[index].cost;
    advance_until(game, 18000, "funding original Technical scenario", |game| {
        game.players[owner].cash >= cost
    })?;
    produce_type(game, producer, index, owner).map_err(|error| {
        format!(
            "Technical scenario production {} at producer {producer}, frame {}: {error}",
            game.catalog[index].name, game.tick
        )
        .into()
    })
}
fn park(game: &mut Skirmish, id: usize, position: [f32; 2]) -> Result<(), Box<dyn Error>> {
    game.move_to(id, position)?;
    advance_until(game, 9000, "source vehicle movement", |game| {
        matches!(game.entities[id].ai.order, Order::Idle)
    })
}
pub fn salvage_one(
    game: &mut Skirmish,
    collector: usize,
    producer: usize,
    victim: usize,
    stage: [f32; 2],
) -> Result<(), Box<dyn Error>> {
    let first = game.entities.len();
    let target = funded_unit(game, producer, victim, 1)?;
    let level = game.entities[collector].salvage_level();
    // Attack while the enemy approaches; native auto-acquisition stays enabled.
    game.attack(collector, target)
        .map_err(|error| format!("salvage collector {collector} target {target}: {error}"))?;
    let point = [stage[0] + 150.0, stage[1] - 200.0];
    game.move_to(target, point)?;
    advance_until(game, 9000, "actual Technical/Marauder enemy kill", |game| {
        !game.entities[target].alive()
    })?;
    let crate_id = game.entities[first..]
        .iter()
        .position(generals_sim::skirmish::Entity::crate_present)
        .map(|offset| first + offset)
        .ok_or_else(|| {
            let attacker = game.entities[target]
                .last_attacker()
                .map(|id| (id, &game.catalog[game.entities[id].type_index].name));
            let crates: Vec<_> = game.entities[first..]
                .iter()
                .filter(|actor| game.catalog[actor.type_index].salvage.pickup.is_some())
                .map(|actor| &game.catalog[actor.type_index].name)
                .collect();
            format!("eligible enemy death did not create an active salvage crate at frame {}; target {target} at {:?}, attacker {attacker:?}, collector {collector} level {}, created crates {crates:?}", game.tick, game.entities[target].position, game.entities[collector].salvage_level())
        })?;
    require(
        game.can_salvage(collector, crate_id),
        "source collector cannot pick up its salvage",
    )?;
    let position = game.entities[crate_id].position;
    game.move_to(collector, position)?;
    advance_until(game, 850, "native crate contact pickup", |game| {
        game.entities[collector].salvage_level() > level
    })?;
    require(
        !game.entities[crate_id].crate_present(),
        "picked up crate retained active contacts",
    )?;
    park(game, collector, stage)?;
    Ok(())
}
pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    game.evacuate(setup.technicals[0])?;
    advance_until(game, 300, "Technical infantry exit", |game| {
        game.entities[setup.rider].contained_by().is_none()
    })?;
    let expected = [
        "TechnicalMachineGunWeapon",
        "TechnicalCannonWeapon",
        "TechnicalRPGWeapon",
    ];
    for (id, name) in setup.technicals.into_iter().zip(expected) {
        let actor = &game.entities[id];
        require(
            actor.active_weapon(&game.catalog[actor.type_index]).name == name,
            "Technical selected the wrong original weapon set",
        )?;
    }
    let actor = &game.entities[setup.marauder];
    require(
        actor.active_weapon(&game.catalog[actor.type_index]).name == "MarauderTankGunUpgradeTwo",
        "Marauder second native upgrade absent",
    )?;
    Ok(())
}

pub fn capture_combat(game: &mut Skirmish, setup: &Capture) -> Result<usize, Box<dyn Error>> {
    let [cannon_point, missile_point] = capture_positions(game, setup);
    let middle = [setup.stage[0] + 142.5, setup.stage[1]];
    let advance: [f32; 2] = std::array::from_fn(|axis| {
        (cannon_point[axis] + missile_point[axis]).mul_add(0.5, -middle[axis]) * 0.25
    });
    // Bring the idle display subjects closer through real movement before any
    // fresh crate lifetime starts. Keep their natural AI and source camera limits.
    for id in [setup.technicals[0], setup.marauder] {
        let position = game.entities[id].position;
        park(
            game,
            id,
            [position[0] + advance[0], position[1] + advance[1]],
        )?;
    }
    let victim = template(game, "AmericaVehicleDozer")?;
    let first = game.entities.len();
    let target = funded_unit(game, setup.enemy_center, victim, 1)?;
    let missile_target = funded_unit(game, setup.enemy_center, victim, 1)?;
    // Keep the two paid victims apart and on the enemy side of the army. Moving
    // both through the army lets normal acquisition kill the second too early.
    park(game, target, cannon_point)
        .map_err(|error| format!("capture cannon victim {target} staging: {error}"))?;
    park(game, missile_target, missile_point)
        .map_err(|error| format!("capture RPG victim {missile_target} staging: {error}"))?;
    game.attack(setup.technicals[1], target)
        .map_err(|error| format!("capture cannon target {target}: {error}"))?;
    advance_until(game, 9000, "source capture crate creation", |game| {
        !game.entities[target].alive()
    })?;
    let crate_id = game.entities[first..]
        .iter()
        .position(generals_sim::skirmish::Entity::crate_present)
        .map(|offset| offset + first)
        .ok_or("capture combat dropped no source crate")?;
    game.attack(setup.technicals[2], missile_target)
        .map_err(|error| format!("capture RPG target {missile_target}: {error}"))?;
    advance_until(game, 9000, "original Technical RPG launch", |game| {
        game.projectiles.iter().any(|projectile| {
            projectile.source == setup.technicals[2]
                && projectile.weapon_index == 4
                && projectile.render_transform(&game.entities).is_some()
        })
    })?;
    require(
        game.entities[crate_id].crate_present(),
        "capture crate expired before RPG launch",
    )?;
    Ok(crate_id)
}

fn capture_positions(game: &Skirmish, setup: &Capture) -> [[f32; 2]; 2] {
    let center = [setup.stage[0] + 142.5, setup.stage[1]];
    let enemy = game.entities[setup.enemy_center].position;
    let offset = [enemy[0] - center[0], enemy[1] - center[1]];
    let length = offset[0].hypot(offset[1]);
    let direction = [offset[0] / length, offset[1] / length];
    let forward = [
        direction[0].mul_add(400.0, center[0]),
        direction[1].mul_add(400.0, center[1]),
    ];
    [250.0, -250.0].map(|side| {
        [
            (-direction[1]).mul_add(side, forward[0]),
            direction[0].mul_add(side, forward[1]),
        ]
    })
}
