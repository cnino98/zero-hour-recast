//! Paid ground roster from native producer commands, separate from spawned children.
use super::{
    Faction, Kind, Skirmish, advance_until, construct_type, friendly_entity, produce_type, require,
    verify_movement,
};
use std::error::Error;

pub(super) fn verify(
    game: &mut Skirmish,
    barracks: usize,
    factory: usize,
    owner: usize,
    paid_types: [usize; 2],
) -> Result<(), Box<dyn Error>> {
    let faction = game.players[owner].faction;
    // Internal destruction children also use TechBuilding; only these native
    // builder products unlock the base-faction factory/Barracks tech units.
    let tech_name = match faction {
        Faction::Usa => "AmericaStrategyCenter",
        Faction::China => "ChinaPropagandaCenter",
        Faction::Gla => "GLAPalace",
        Faction::Civilian => return Err("Neutral objects have no faction production roster".into()),
    };
    let tech = game
        .catalog
        .iter()
        .position(|unit| unit.faction == faction && unit.name == tech_name)
        .ok_or("Missing native tech building")?;
    fund(game, tech, owner)?;
    let builder = friendly_entity(game, Kind::Builder, owner)?;
    construct_type(game, builder, tech, owner)?;
    if faction == Faction::Gla {
        game.purchase_science(owner, "SCIENCE_MarauderTank")?;
        println!("  Roster unlock: paid SCIENCE_MarauderTank");
    }
    report_separate_objects(game, [barracks, factory], faction)?;
    for (producer, wrong) in [(barracks, factory), (factory, barracks)] {
        let products = game.catalog[game.entities[producer].type_index]
            .buildable_objects
            .clone();
        for name in products {
            let Some(index) = game.catalog.iter().position(|unit| unit.name == name) else {
                println!("  Roster omitted: {name}; original menu product is not implemented");
                continue;
            };
            if paid_types.contains(&index) {
                println!("  Roster covered: {name}; paid production/movement in main loop");
                continue;
            }
            let unit = &game.catalog[index];
            if !unit
                .build_rules
                .sciences
                .iter()
                .all(|name| game.players[owner].has_science(name))
            {
                let check = science_check(&name).ok_or_else(|| {
                    format!(
                        "Roster science product {name} lacks an explicit paid acceptance scenario"
                    )
                })?;
                let cash = game.players[owner].cash;
                require(
                    game.train(producer, index).is_err() && game.players[owner].cash == cash,
                    "locked science roster product was produced or charged money",
                )?;
                println!("  Roster separate: {name}; locked original science, {check}");
                continue;
            }
            require(
                game.prerequisites_met(owner, index),
                &format!(
                    "Roster product {name} is missing original requirements {:?}",
                    game.catalog[index].build_rules.objects,
                ),
            )?;
            let cash = game.players[owner].cash;
            require(
                game.train(wrong, index).is_err() && game.players[owner].cash == cash,
                "wrong producer accepted expanded unit",
            )?;
            fund(game, index, owner)?;
            let actor = produce_type(game, producer, index, owner)
                .map_err(|error| format!("Roster product {name}: {error}"))?;
            if name == "GLAInfantryAngryMobNexus" {
                advance_until(game, 1200, "paid Mob spawning its ten members", |game| {
                    game.spawned_children(actor).len() == 10
                })?;
                crate::angry_mob_verification::verify_children(game, actor)?;
            }
            verify_rules(game, index)?;
            verify_movement(game, actor)
                .map_err(|error| format!("Roster movement {name}: {error}"))?;
            println!(
                "  Roster admitted: {name}; original {} menu, paid production, exit and movement",
                game.catalog[game.entities[producer].type_index].name,
            );
        }
    }
    Ok(())
}

fn fund(game: &mut Skirmish, index: usize, owner: usize) -> Result<(), Box<dyn Error>> {
    let cost = game.catalog[index].cost;
    advance_until(
        game,
        36_000,
        "ordinary supply income funding roster",
        |game| game.players[owner].cash >= cost,
    )?;
    Ok(())
}

fn science_check(name: &str) -> Option<&str> {
    match name {
        "AmericaTankPaladin" => Some("paid --verify-point-defense unlock/production/combat"),
        "GLAInfantryHijacker" => Some(
            "combat-earned rank-three SCIENCE_Hijacker, paid Barracks production and driver transfer in --verify-hijacker",
        ),
        "GLAVehicleScudLauncher" => Some("paid --verify-artillery unlock/production/combat"),
        "AmericaInfantryPathfinder" => {
            Some("combat-earned rank science and paid sniper scenario in this entrypoint")
        }
        _ => None,
    }
}

fn report_separate_objects(
    game: &Skirmish,
    producers: [usize; 2],
    faction: Faction,
) -> Result<(), Box<dyn Error>> {
    for unit in game.catalog.iter().filter(|unit| {
        unit.faction == faction && matches!(unit.kind, Kind::Infantry | Kind::Tank | Kind::Vehicle)
    }) {
        if producers.iter().any(|&id| {
            game.catalog[game.entities[id].type_index]
                .buildable_objects
                .contains(&unit.name)
        }) {
            continue;
        }
        let check = match unit.name.as_str() {
            "AmericaJetRaptor" | "AmericaVehicleComanche" =>
                "original Airfield menu; paid --verify-aircraft",
            "AmericaJetStealthFighter" => "original Airfield menu; paid --verify-stealth-fighter",
            "AmericaJetAurora" => "original Airfield menu; paid --verify-aurora",
            "ChinaJetMIG" =>
                "original China Airfield menu; paid --verify-china-aircraft",
            "ChinaVehicleHelix" => "original China Airfield menu; paid --verify-helix",
            "ChinaHelixGattlingCannon" | "ChinaHelixPropagandaTower" | "ChinaHelixBattleBunker" =>
                "original Helix upgrade attachments; paid --verify-helix",
            _ if !unit.behavior.mobs.is_empty() && game.catalog.iter().any(|parent| {
                parent.name == "GLAInfantryAngryMobNexus"
                    && parent.behavior.spawns.iter().any(|spawn| spawn.templates.contains(&unit.name))
            }) => "ordered children of the paid Angry Mob nexus; verified during this Barracks production run",
            "AmericaVehicleBattleDrone" | "AmericaVehicleHellfireDrone" =>
                "original object-upgrade children; paid --verify-drones",
            "AmericaVehicleScoutDrone" =>
                "original object-upgrade child; paid --verify-infiltrators",
            "AmericaTankAvengerLaserTurret" =>
                "original Avenger attached AA turret; paid --verify-point-defense",
            "ChinaTankOverlordGattlingCannon" | "ChinaTankOverlordPropagandaTower"
                | "ChinaTankOverlordBattleBunker" =>
                "original Overlord upgrade attachments; paid Overlord scenario in this entrypoint",
            "GLAInfantryStingerSoldier" =>
                "original Stinger spawn; paid --verify-stinger",
            "ToxicInfantry_TerroristOnly" | "ToxicInfantryBeta_TerroristOnly" =>
                "original Terrorist poison-death children; paid --verify-terrorist",
            "GLAVehicleTechnicalChassisOne" | "GLAVehicleTechnicalChassisTwo"
                | "GLAVehicleTechnicalChassisThree" =>
                "original Technical build variations; paid Technical scenario in this entrypoint",
            _ => return Err(format!(
                "Roster object {} has no Barracks/War Factory command or explicit separate acceptance",
                unit.name,
            ).into()),
        };
        println!("  Roster separate: {}; {check}", unit.name);
    }
    Ok(())
}

fn verify_rules(game: &Skirmish, index: usize) -> Result<(), Box<dyn Error>> {
    let unit = &game.catalog[index];
    if unit.name == "AmericaVehicleMedic" {
        require(
            unit.weapon.name == "AmbulanceCleanHazardWeapon"
                && matches!(
                    unit.weapon.damage_type,
                    generals_sim::skirmish::DamageType::HazardCleanup
                )
                && unit.weapon.damage.to_bits() == 100.0_f32.to_bits()
                && unit.weapon.auto_choose == 0
                && !unit.behavior.cleanup.is_empty()
                && unit.maintenance.auto_heal.len() == 2,
            "Ambulance lacks its two healing modules or original non-auto-chosen hazard-cleanup weapon",
        )?;
    } else if unit.name == "ChinaTankOverlord" {
        require(
            unit.weapon.clip_size == 2
                && unit
                    .weapon
                    .area
                    .as_ref()
                    .is_some_and(|area| area.secondary_damage > 0.0)
                && unit
                    .weapon
                    .motion
                    .as_ref()
                    .is_some_and(|motion| motion.launches[0].len() == 2),
            "Overlord lacks native two-barrel clip or secondary damage ring",
        )?;
    }
    Ok(())
}
