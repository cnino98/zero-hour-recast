//! All three base Overlord attachments through paid installed commands.
use super::{advance_until, construct, friendly_entity, produce_type, require};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;

pub struct Capture {
    pub tanks: [usize; 3],
    pub rider: usize,
}

pub fn prepare(game: &mut Skirmish) -> Result<Capture, Box<dyn Error>> {
    game.reset(Faction::China, Faction::Usa)?;
    let worker = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, worker, Kind::PowerPlant, 0)?;
    construct(game, worker, Kind::SupplyCenter, 0)?;
    let barracks = construct(game, worker, Kind::Barracks, 0)?;
    let factory = construct(game, worker, Kind::Factory, 0)?;
    construct(game, worker, Kind::TechBuilding, 0)?;
    let index = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaTankOverlord")
        .ok_or("missing Overlord")?;
    let center = friendly_entity(game, Kind::CommandCenter, 0)?;
    let base = game.entities[center].position;
    let mut tanks = [0; 3];
    for (slot, suffix) in ["GattlingCannon", "PropagandaTower", "BattleBunker"]
        .into_iter()
        .enumerate()
    {
        tanks[slot] = purchase_attachment(game, factory, index, base, slot, suffix)?;
    }
    let infantry = game
        .catalog
        .iter()
        .position(|unit| unit.name == "ChinaInfantryTankHunter")
        .ok_or("missing Tank Hunter")?;
    let cost = game.catalog[infantry].cost;
    advance_until(game, 9000, "supplies funding bunker passenger", |game| {
        game.players[0].cash >= cost
    })?;
    let rider = produce_type(game, barracks, infantry, 0)?;
    game.enter(rider, tanks[2])?;
    advance_until(
        game,
        3000,
        "entering the upgraded Overlord Bunker",
        |game| game.entities[rider].contained_by() == game.entities[tanks[2]].attachment_child(),
    )?;
    require(
        game.passengers(tanks[2]) == [rider] && game.passenger_can_fire(rider),
        "Bunker did not redirect inventory/firing",
    )?;
    println!(
        "Installed Overlord: all three paid exclusive upgrades, native child templates and bunker boarding passed at frame {}",
        game.tick
    );
    Ok(Capture { tanks, rider })
}

fn purchase_attachment(
    game: &mut Skirmish,
    factory: usize,
    index: usize,
    base: [f32; 2],
    slot: usize,
    suffix: &str,
) -> Result<usize, Box<dyn Error>> {
    let name = format!("Upgrade_ChinaOverlord{suffix}");
    let upgrade = game.catalog[index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == name)
        .ok_or("missing attachment research")?;
    let cost = game.catalog[index].cost + game.catalog[index].maintenance.research[upgrade].cost;
    advance_until(
        game,
        18_000,
        "supplies funding an Overlord and its attachment",
        |game| game.players[0].cash >= cost,
    )?;
    let tank = produce_type(game, factory, index, 0)?;
    let offset = f32::from(u16::try_from(slot)?) * 100.0;
    let destination = [base[0] - 350.0 + offset, base[1] - 100.0];
    game.move_to(tank, destination)?;
    game.queue_upgrade(tank, upgrade)?;
    require(
        game.catalog[index]
            .maintenance
            .research
            .iter()
            .enumerate()
            .all(|(other, _)| !game.can_research(tank, other)),
        "Overlord allows conflicting simultaneous research",
    )?;
    advance_until(game, 3000, "paid Overlord attachment creation", |game| {
        game.entities[tank].attachment_child().is_some()
    })?;
    advance_until(game, 3000, "upgraded Overlord leaving factory", |game| {
        matches!(game.entities[tank].ai.order, Order::Idle)
    })?;
    let child = game.entities[tank]
        .attachment_child()
        .ok_or("missing portable child")?;
    require(
        game.catalog[game.entities[child].type_index].name == format!("ChinaTankOverlord{suffix}"),
        "OCL created the wrong attachment",
    )?;
    require(
        game.entities[child].attachment_parent() == Some(tank)
            && !game.entities[child].masked()
            && !game.entities[child].selectable(&game.catalog[game.entities[child].type_index]),
        "portable attachment visibility/selection diverged",
    )?;
    require(
        game.catalog[index]
            .maintenance
            .research
            .iter()
            .enumerate()
            .all(|(other, _)| !game.can_research(tank, other)),
        "completed attachment did not lock conflicting upgrades",
    )?;
    Ok(tank)
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let capture = prepare(game)?;
    engage(game, capture.tanks[0])?;
    let bunker = capture.tanks[2];
    game.evacuate(bunker)?;
    advance_until(game, 300, "Overlord bunker evacuation", |game| {
        game.entities[capture.rider].contained_by().is_none()
    })?;
    require(
        game.entities[bunker].attachment_child().is_some(),
        "Evacuate removed the portable Bunker",
    )?;
    game.enter(capture.rider, bunker)?;
    advance_until(game, 3000, "Overlord bunker re-entry", |game| {
        game.entities[capture.rider].contained_by() == game.entities[bunker].attachment_child()
    })?;
    let cannon = game.entities[capture.tanks[0]]
        .attachment_child()
        .ok_or("missing cannon")?;
    let source = &game.catalog[game.entities[cannon].type_index];
    require(
        source.weapon.name == "GattlingBuildingGun"
            && source
                .secondary_weapon
                .as_ref()
                .is_some_and(|weapon| weapon.name == "GattlingBuildingGunAir"),
        "portable cannon lacks original ground/air weapon sets",
    )?;
    let speaker = game.entities[capture.tanks[1]]
        .attachment_child()
        .ok_or("missing speaker")?;
    require(
        game.catalog[game.entities[speaker].type_index]
            .behavior
            .propaganda
            .as_ref()
            .is_some_and(|rules| {
                rules.delay == 60 && rules.radius.to_bits() == 150.0_f32.to_bits()
            }),
        "speaker lacks original radius/timer",
    )?;
    println!(
        "Installed Overlord: original cannon weapon sets, speaker rules and bunker individual inventory/evacuation passed"
    );
    Ok(())
}

/// Commands an existing opposing Dozer into the mounted gun's real firing range.
pub fn engage(game: &mut Skirmish, parent: usize) -> Result<(), Box<dyn Error>> {
    let child = game.entities[parent]
        .attachment_child()
        .ok_or("missing cannon attachment")?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    stage_gun_target(game, parent, target)?;
    let point = game.entities[parent].position;
    game.move_to(target, point)?;
    await_mounted_shot(game, parent, child, target)
    .map_err(|error| {
        format!(
            "{error}; target {}; parent {}; child {}; child_reachable={} targetable={} slot={} weapon={}",
            generals_sim::skirmish::diagnostics::movement_details(game, target),
            generals_sim::skirmish::diagnostics::movement_details(game, parent),
            generals_sim::skirmish::diagnostics::movement_details(game, child),
            game.weapon_target_reachable(child, target),
            game.weapon_can_target(child, target),
            game.entities[child].weapon_slot(),
            game.entities[child].active_weapon(&game.catalog[game.entities[child].type_index]).name,
        )
    })?;
    require(
        game.entities[target].alive(),
        "mounted cannon firing scenario lost its target",
    )?;
    println!(
        "Installed Overlord: actual mounted Gattling shot at frame {}",
        game.tick
    );
    Ok(())
}

fn stage_gun_target(
    game: &mut Skirmish,
    parent: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let point = game.entities[parent].position;
    // Approach the isolated side of the cannon rather than crossing the other
    // paid Overlords and their Tank Hunter's original firing ranges.
    for destination in [
        [point[0] - 300.0, point[1] + 400.0],
        [point[0] - 300.0, point[1]],
    ] {
        game.move_to(target, destination)?;
        advance_until(
            game,
            12_000,
            "Dozer staging outside the other Overlord guns",
            |game| {
                let actor = &game.entities[target];
                actor.alive()
                    && matches!(actor.ai.order, Order::Idle)
                    && !actor.moving()
                    && (actor.position[0] - destination[0])
                        .hypot(actor.position[1] - destination[1])
                        < 40.0
            },
        )
        .map_err(|error| {
            format!(
                "{error}; {}",
                generals_sim::skirmish::diagnostics::movement_details(game, target)
            )
        })?;
    }
    Ok(())
}

fn await_mounted_shot(
    game: &mut Skirmish,
    parent: usize,
    child: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let mut shots = Vec::new();
    let mut stopped = false;
    let mut command_state = None;
    for _ in 0..12_000 {
        game.step();
        if game
            .presentation
            .shots
            .iter()
            .any(|shot| shot.source == child)
        {
            game.stop(target)?;
            return Ok(());
        }
        if !stopped && game.weapon_target_reachable(child, target) {
            game.stop(target)?;
            // The gun's authored VisionRange is 200, below its AttackRange 225.
            // An ordinary parent attack forwards the selected target to the
            // portable child instead of relying on an out-of-sight idle scan.
            game.attack(parent, target)?;
            command_state = Some(mounted_details(game, parent, child, target));
            stopped = true;
        }
        for shot in game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.target == Some(target))
        {
            if shots.len() < 24 {
                let unit = &game.catalog[game.entities[shot.source].type_index];
                shots.push(format!(
                    "frame={} source={}:{} weapon={} from={:?} target_health={}",
                    game.tick,
                    shot.source,
                    unit.name,
                    unit.weapon_template(shot.weapon_index)
                        .map_or("missing", |weapon| weapon.name.as_str()),
                    shot.source_position,
                    game.entities[target].health
                ));
            }
        }
        if !game.entities[target].alive() {
            break;
        }
    }
    let killer = game.entities[target]
        .last_attacker()
        .map(|id| (id, game.catalog[game.entities[id].type_index].name.as_str()));
    let final_state = mounted_details(game, parent, child, target);
    Err(format!("mounted Gattling produced no shot by tick {}; target_last_attacker={killer:?}; prior_target_shots={shots:?}; command={command_state:?}; final={final_state}", game.tick).into())
}

fn mounted_details(game: &Skirmish, parent: usize, child: usize, target: usize) -> String {
    let actor = &game.entities[child];
    let unit = &game.catalog[actor.type_index];
    let weapon = actor.active_weapon(unit);
    let victim = &game.catalog[game.entities[target].type_index];
    format!(
        "frame={} child={} at={:?} parent_at={:?} allowed={} passengers={:?} acquisition={} damage={} target_armor={} auto_choose={} {}",
        game.tick,
        child,
        actor.position,
        game.entities[parent].position,
        game.passenger_can_fire(child),
        game.passengers(parent),
        unit.ai.acquisition,
        weapon.damage,
        victim.armor[weapon.damage_type as usize],
        weapon.auto_choose,
        generals_sim::skirmish::diagnostics::weapon_details(game, child, target),
    )
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn paid_overlord_mount_fires_while_original_dozer_approaches() {
        let root = generals_assets::game_root().unwrap();
        let mut game = crate::infiltrator_verification::load_game(&root).unwrap();
        let capture = super::prepare(&mut game).unwrap();
        super::engage(&mut game, capture.tanks[0]).unwrap();
    }
}
