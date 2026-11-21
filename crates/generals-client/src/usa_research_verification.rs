//! Paid original base-USA Supply Lines and Chemical Suits consumers.
use crate::skirmish_verification::{
    advance_until, construct, construct_type, produce_type, require,
};
use generals_sim::skirmish::{Kind, Skirmish, particle_uplink::Stage};
use std::error::Error;

pub struct Setup {
    pub center: usize,
    pub infantry: Vec<usize>,
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    let setup = crate::strategy_verification::prepare(game)?;
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0)?;
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let ranger = crate::strategy_verification::template(game, "AmericaInfantryRanger")?;
    let defender = crate::strategy_verification::template(game, "AmericaInfantryMissileDefender")?;
    let burton = crate::strategy_verification::template(game, "AmericaInfantryColonelBurton")?;
    let existing = produce_type(game, barracks, ranger, 0)?;
    let colonel = produce_type(game, barracks, burton, 0)?;
    require(
        !game.entities[existing].chemical_suit() && !game.entities[colonel].chemical_suit(),
        "Ranger received unpaid Chemical Suits",
    )?;
    crate::strategy_verification::research(game, setup.center, "Upgrade_AmericaSupplyLines")?;
    crate::strategy_verification::research(game, setup.center, "Upgrade_AmericaChemicalSuits")?;
    let future = produce_type(game, barracks, ranger, 0)?;
    let missile = produce_type(game, barracks, defender, 0)?;
    let infantry = vec![existing, colonel, future, missile];
    for &id in &infantry {
        require(
            game.entities[id].chemical_suit(),
            "paid existing/future infantry lost Chemical Suits",
        )?;
    }
    for name in [
        "AmericaInfantryRanger",
        "AmericaInfantryMissileDefender",
        "AmericaInfantryPathfinder",
        "AmericaInfantryColonelBurton",
    ] {
        let index = crate::strategy_verification::template(game, name)?;
        let module = game.catalog[index]
            .maintenance
            .armor_upgrade
            .as_ref()
            .ok_or("original infantry chemical armor missing")?;
        require(
            module.trigger == "Upgrade_AmericaChemicalSuits",
            "chemical armor trigger differs from original",
        )?;
    }
    verify_delivery(game)?;
    Ok(Setup {
        center: setup.center,
        infantry,
    })
}

fn verify_delivery(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let collector = game
        .entities
        .iter()
        .position(|actor| {
            actor.owner == 0
                && actor.complete()
                && game.catalog[actor.type_index].kind == Kind::Harvester
        })
        .ok_or("paid supply center did not supply original Chinook")?;
    let center = crate::skirmish_verification::friendly_entity(game, Kind::SupplyCenter, 0)?;
    let capacity = game.catalog[game.entities[collector].type_index]
        .supply
        .capacity;
    require(capacity == 8, "original Chinook capacity changed")?;
    advance_until(
        game,
        18_000,
        "original Chinook collecting full cargo",
        |game| game.entities[collector].cargo == capacity,
    )?;
    game.stop(collector)?;
    let cash = game.players[0].cash;
    let earned = game.players[0].statistics.money_earned;
    game.deliver_supplies(collector, center)?;
    advance_until(game, 18_000, "paid Supply Lines delivery", |game| {
        game.entities[collector].cargo == 0
    })?;
    require(
        game.players[0].cash == cash + 660
            && game.players[0].statistics.money_earned == earned + 660,
        "full Chinook Supply Lines delivery differs from original $660",
    )?;
    game.stop(collector)?;
    println!(
        "Installed USA research: paid original queues, existing/future Chemical Suits and exact $660 Chinook cash/statistics passed"
    );
    Ok(())
}

pub fn verify(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let setup = prepare(game)?;
    require(
        game.entities[setup.center].complete() && setup.infantry.len() == 4,
        "USA research setup incomplete",
    )?;
    verify_pathfinders(game)
}

fn verify_pathfinders(game: &mut Skirmish) -> Result<(), Box<dyn Error>> {
    let (existing, _) = crate::progression_verification::pathfinder::prepare_capture(game)?;
    require(
        !game.entities[existing].chemical_suit(),
        "Pathfinder received unpaid Chemical Suits",
    )?;
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0)?;
    let center_type = crate::strategy_verification::template(game, "AmericaStrategyCenter")?;
    let cost = game.catalog[center_type].cost;
    advance_until(
        game,
        18_000,
        "supplies funding Strategy Center for promoted Pathfinder",
        |game| game.players[0].cash >= cost,
    )?;
    let center = construct_type(game, builder, center_type, 0)?;
    crate::strategy_verification::research(game, center, "Upgrade_AmericaChemicalSuits")?;
    let barracks = crate::skirmish_verification::friendly_entity(game, Kind::Barracks, 0)?;
    let pathfinder = crate::strategy_verification::template(game, "AmericaInfantryPathfinder")?;
    let cost = game.catalog[pathfinder].cost;
    advance_until(
        game,
        18_000,
        "supplies funding future promoted Pathfinder",
        |game| game.players[0].cash >= cost,
    )?;
    let future = produce_type(game, barracks, pathfinder, 0)?;
    require(
        game.entities[existing].chemical_suit() && game.entities[future].chemical_suit(),
        "paid existing/future promoted Pathfinder lost Chemical Suits",
    )?;
    println!(
        "Installed Chemical Suits: combat-earned Pathfinder science and paid existing/future consumers passed"
    );
    Ok(())
}

pub const PARTICLE_POWER: &str = "SuperweaponParticleUplinkCannon";

pub fn prepare_particle_uplink(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    crate::strategy_verification::prepare(game)?;
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0)?;
    let plant = crate::skirmish_verification::friendly_entity(game, Kind::PowerPlant, 0)?;
    crate::strategy_verification::research(game, plant, "Upgrade_AmericaAdvancedControlRods")?;
    let plant_type = game.entities[plant].type_index;
    let cost = game.build_cost(0, plant_type);
    advance_until(
        game,
        18_000,
        "supplies paying for the second Cold Fusion Reactor",
        |game| game.players[0].cash >= cost,
    )?;
    construct_type(game, builder, plant_type, 0)?;
    let kind = crate::strategy_verification::template(game, "AmericaParticleCannonUplink")?;
    let cost = game.build_cost(0, kind);
    advance_until(
        game,
        18_000,
        "supplies paying for original Particle Uplink",
        |game| game.players[0].cash >= cost,
    )?;
    let cannon = construct_type(game, builder, kind, 0)?;
    require(
        game.has_operational_power(cannon),
        "paid reactors cannot power Particle Uplink",
    )?;
    require(
        (6900..=7200).contains(&game.power_cooldown_remaining(cannon, PARTICLE_POWER)),
        "completed Particle Uplink did not begin original local recharge",
    )?;
    advance_until(game, 7300, "Particle Uplink fire readiness", |game| {
        game.particle_uplink_visuals(cannon)
            .next()
            .is_some_and(|visual| visual.stage == Stage::Ready)
    })?;
    Ok(cannon)
}

pub fn scout_open_target(
    game: &mut Skirmish,
    provider: usize,
    scout: usize,
) -> Result<[f32; 2], Box<dyn Error>> {
    use generals_sim::skirmish::visibility::Status;

    let [x, y] = game.entities[provider].position;
    for [dx, dy] in [
        [360.0, 0.0],
        [0.0, 360.0],
        [-360.0, 0.0],
        [0.0, -360.0],
        [360.0, 360.0],
        [-360.0, 360.0],
        [360.0, -360.0],
        [-360.0, -360.0],
    ] {
        let point = [x + dx, y + dy];
        if game.entities.iter().any(|actor| {
            actor.owner == 0
                && actor.alive()
                && game.catalog[actor.type_index].kind.is_building()
                && (point[0] - actor.position[0]).hypot(point[1] - actor.position[1]) < 250.0
        }) {
            continue;
        }
        if matches!(game.visibility_cell(0, point), Status::Clear) {
            return Ok(point);
        }
        if game.move_to(scout, point).is_ok()
            && advance_until(
                game,
                1200,
                "paid Crusader scouting the power target",
                |game| matches!(game.visibility_cell(0, point), Status::Clear),
            )
            .is_ok()
        {
            game.stop(scout)?;
            return Ok(point);
        }
    }
    Err("No scouted power target is clear of the USA base".into())
}

pub fn construct_strategy_center(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    let builder = crate::skirmish_verification::friendly_entity(game, Kind::Builder, 0)?;
    let strategy = crate::strategy_verification::template(game, "AmericaStrategyCenter")?;
    let cost = game.build_cost(0, strategy);
    advance_until(
        game,
        18_000,
        "supplies paying for Strategy Center",
        |game| game.players[0].cash >= cost,
    )?;
    construct_type(game, builder, strategy, 0)
}

#[cfg(test)]
mod p03 {
    use super::*;
    use crate::{
        skirmish_verification::friendly_entity,
        strategy_verification::{research, template},
    };
    use generals_sim::skirmish::{Faction, PowerTarget};

    mod spectre;

    fn setup() -> Skirmish {
        let root = generals_assets::game_root().unwrap();
        crate::infiltrator_verification::load_game(&root).unwrap()
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_particle_uplink_charges_and_drives_original_orbital_beam() -> Result<(), Box<dyn Error>>
    {
        use bevy::prelude::Vec2;

        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let root = generals_assets::game_root()?;
        let mut mouse = crate::input::load(
            &assets,
            &root.join("UserData/Options.ini"),
            &battle.simulation.catalog,
        )?;
        let game = &mut battle.simulation;
        let cannon = prepare_particle_uplink(game)?;
        let power = PARTICLE_POWER;
        let point = game.entities[cannon].position;
        let target = [point[0] + 50.0, point[1]];
        game.cast_power(cannon, power, PowerTarget::Position(target))?;
        advance_until(game, 90, "original orbital beam reaching target", |game| {
            game.particle_uplink_visuals(cannon)
                .next()
                .is_some_and(|visual| visual.target.is_some())
        })?;
        let retarget = Vec2::new(target[0] + 40.0, target[1]);
        let destination = retarget.extend(game.ground_height(retarget.to_array()));
        let cooldown = game.power_cooldown_remaining(cannon, power);
        mouse.world_position = Some(destination);
        battle.select_objects(&[cannon], true);
        require(
            crate::attack_cursor::context(&mut battle, &mouse) == Some(14),
            "original Particle Uplink steering cursor was unavailable",
        )?;
        require(
            crate::input::context_order(&mut battle, Some(retarget), None, &mut mouse),
            "normal context order did not steer the active beam",
        )?;
        crate::input::commands::step(&mut battle, &mut mouse);
        let game = &mut battle.simulation;
        assert_eq!(game.power_cooldown_remaining(cannon, power), cooldown - 1);
        advance_until(game, 30, "controlled beam crossing terrain", |game| {
            game.particle_uplink_visuals(cannon)
                .next()
                .and_then(|visual| visual.target)
                .is_some_and(|target_beam| target_beam.position[0] >= target[0] + 5.0)
        })?;
        let remnant = template(game, "ParticleUplinkCannonTrailRemnant")?;
        require(
            game.entities
                .iter()
                .any(|object| object.type_index == remnant && object.owner == 0),
            "Particle Uplink never created its authored damage remnant",
        )?;
        advance_until(game, 450, "original orbital beam retiring", |game| {
            game.particle_uplink_visuals(cannon)
                .next()
                .is_some_and(|visual| visual.stage == Stage::Idle)
        })?;
        Ok(())
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn purchased_spy_drone_and_satellite_create_original_visible_objects()
    -> Result<(), Box<dyn Error>> {
        let mut game = setup();
        let strategy = crate::strategy_verification::prepare(&mut game)?.center;
        let center = friendly_entity(&game, Kind::CommandCenter, 0)?;
        let enemy = friendly_entity(&game, Kind::CommandCenter, 1)?;
        let position = game.entities[enemy].position;
        game.purchase_science(0, "SCIENCE_SpyDrone")?;
        game.cast_power(
            center,
            "SpecialPowerSpySatellite",
            PowerTarget::Position(position),
        )?;
        let ping = template(&game, "SpySatellitePing")?;
        let drone = template(&game, "AmericaVehicleSpyDrone")?;
        advance_until(
            &mut game,
            35,
            "Satellite revealing an original enemy structure",
            |game| {
                game.entities
                    .iter()
                    .any(|actor| actor.type_index == ping && actor.exists())
                    && game.visible_to(0, enemy)
            },
        )?;
        game.cast_power(
            center,
            "SpecialPowerSpyDrone",
            PowerTarget::Position(position),
        )?;
        let scout = game
            .entities
            .iter()
            .position(|actor| actor.type_index == drone && actor.owner == 0 && actor.exists())
            .ok_or("paid Spy Drone did not create its original scouting object")?;
        require(
            game.catalog[drone].locomotor.legal_surfaces == 3,
            "Spy Drone lost its GROUND WATER mask",
        )?;
        require(
            game.power_cooldown_remaining(center, "SpecialPowerSpyDrone") == 2700,
            "Spy Drone did not start original ninety-second shared recharge",
        )?;
        advance_until(
            &mut game,
            30,
            "Spy Drone lifting during its original EXGrid",
            |game| {
                game.height_above_ground(scout) > 0.0
                    && game.dynamic_shroud_grid_slots(scout, 0).is_some()
                    && game.visible_to(0, scout)
            },
        )?;
        advance_until(
            &mut game,
            600,
            "Spy Drone settling at original stationary hover height",
            |game| (game.height_above_ground(scout) - 90.0).abs() < 2.0,
        )?;
        require(
            game.entities[scout].position.map(f32::to_bits) == position.map(f32::to_bits)
                && game.dynamic_shroud_grid_slots(scout, 0).is_none(),
            "settled Spy Drone drifted or retained its transient EXGrid",
        )?;
        verify_spy_drone_armor(&mut game, strategy, center, scout, position)?;
        Ok(())
    }

    fn verify_spy_drone_armor(
        game: &mut Skirmish,
        strategy: usize,
        center: usize,
        existing: usize,
        target: [f32; 2],
    ) -> Result<(), Box<dyn Error>> {
        let kind = game.entities[existing].type_index;
        require(
            game.entities[existing].health.to_bits() == 200.0_f32.to_bits(),
            "unresearched original Spy Drone did not have 200 HP",
        )?;
        research(game, strategy, "Upgrade_AmericaDroneArmor")?;
        advance_until(
            game,
            2701,
            "paid Drone Armor and Spy Drone recharge",
            |game| {
                game.entities[existing].health.to_bits() == 250.0_f32.to_bits()
                    && game.entities[existing].max_health.to_bits() == 250.0_f32.to_bits()
                    && game.can_use_power(center, "SpecialPowerSpyDrone")
            },
        )?;
        let first = game.entities.len();
        game.cast_power(
            center,
            "SpecialPowerSpyDrone",
            PowerTarget::Position(target),
        )?;
        require(
            game.entities[first..].iter().any(|actor| {
                actor.type_index == kind
                    && actor.owner == 0
                    && actor.health.to_bits() == 250.0_f32.to_bits()
                    && actor.max_health.to_bits() == 250.0_f32.to_bits()
            }),
            "future original Spy Drone did not inherit paid Drone Armor",
        )?;
        Ok(())
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn cia_intelligence_reveals_then_releases_original_enemy_objects() -> Result<(), Box<dyn Error>>
    {
        let mut game = setup();
        let strategy = crate::strategy_verification::prepare(&mut game)?.center;
        let enemy = friendly_entity(&game, Kind::Builder, 1)?;
        require(
            !game.visible_to(0, enemy),
            "enemy builder was already visible",
        )?;
        let power = "SuperweaponCIAIntelligence";
        require(
            game.power_cooldown_remaining(strategy, power) == 0
                && game.can_use_power(strategy, power),
            "paid Strategy Center did not finish with stock CIA readiness",
        )?;
        game.cast_power(strategy, power, PowerTarget::None)?;
        require(
            game.power_cooldown_remaining(strategy, power) == 9_000,
            "CIA did not start its original local recharge",
        )?;
        advance_until(
            &mut game,
            2,
            "CIA revealing an original enemy object",
            |game| game.visible_to(0, enemy),
        )?;
        advance_until(
            &mut game,
            1_100,
            "CIA reveal and native unlook persistence expiring",
            |game| !game.visible_to(0, enemy),
        )?;
        Ok(())
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_laser_missiles_increase_real_damage_from_the_existing_raptor()
    -> Result<(), Box<dyn Error>> {
        let mut game = setup();
        let aircraft = crate::aircraft_verification::prepare(&mut game)?;
        let power = "Upgrade_AmericaLaserMissiles";
        require(!game.has_upgrade(0, power), "unpaid Laser Missiles")?;
        crate::aircraft_verification::approach(&mut game, aircraft.raptor, aircraft.target)?;
        let original = first_aircraft_hit(&mut game, aircraft.raptor, aircraft.target)?;
        rearm_aircraft(&mut game, aircraft.raptor, aircraft.airfield, 4)?;
        research(&mut game, aircraft.airfield, power)?;
        crate::aircraft_verification::approach(&mut game, aircraft.raptor, aircraft.target)?;
        let researched = first_aircraft_hit(&mut game, aircraft.raptor, aircraft.target)?;
        require(
            [original, researched].map(f32::to_bits) == [100.0_f32, 125.0_f32].map(f32::to_bits),
            &format!(
                "Raptor missile damage before / after paid research was {original} / {researched}"
            ),
        )?;
        Ok(())
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_laser_missiles_increase_damage_from_the_existing_stealth_fighter()
    -> Result<(), Box<dyn Error>> {
        use crate::stealth_fighter_verification::{approach, prepare};
        let mut game = setup();
        let sortie = prepare(&mut game)?;
        let upgrade = "Upgrade_AmericaLaserMissiles";
        require(!game.has_upgrade(0, upgrade), "unpaid Laser Missiles")?;
        approach(&mut game, sortie.aircraft, sortie.target)?;
        let original = first_aircraft_hit(&mut game, sortie.aircraft, sortie.target)?;
        rearm_aircraft(&mut game, sortie.aircraft, sortie.airfield, 2)?;
        research(&mut game, sortie.airfield, upgrade)?;
        approach(&mut game, sortie.aircraft, sortie.target)?;
        let researched = first_aircraft_hit(&mut game, sortie.aircraft, sortie.target)?;
        require(
            [original, researched].map(f32::to_bits) == [100.0_f32, 125.0_f32].map(f32::to_bits),
            &format!(
                "Stealth Fighter missile damage before / after paid research was {original} / {researched}"
            ),
        )
    }

    fn first_aircraft_hit(
        game: &mut Skirmish,
        aircraft: usize,
        target: usize,
    ) -> Result<f32, Box<dyn Error>> {
        let health = game.entities[target].health;
        game.attack(aircraft, target)?;
        advance_until(game, 1800, "original aircraft missile damage", |game| {
            let victim = &game.entities[target];
            victim.health < health && victim.last_attacker() == Some(aircraft)
        })?;
        Ok(health - game.entities[target].health)
    }

    fn rearm_aircraft(
        game: &mut Skirmish,
        aircraft: usize,
        airfield: usize,
        rounds: u32,
    ) -> Result<(), Box<dyn Error>> {
        game.land_aircraft(aircraft, airfield)?;
        advance_until(
            game,
            7200,
            "original aircraft landing and rearming",
            |game| {
                let actor = &game.entities[aircraft];
                actor.aircraft_phase() == Some("idle")
                    && actor.weapon_rounds_remaining(0) == rounds
                    && game.height_above_ground(aircraft) < 10.0
                    && game.projectiles.iter().all(|shot| shot.source != aircraft)
            },
        )
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_drone_armor_upgrades_existing_and_future_scout_drones() -> Result<(), Box<dyn Error>> {
        let mut game = setup();
        let base = crate::strategy_verification::prepare(&mut game)?;
        let humvee = template(&game, "AmericaVehicleHumvee")?;
        let scout = template(&game, "AmericaVehicleScoutDrone")?;
        let purchase = |game: &mut Skirmish| -> Result<usize, Box<dyn Error>> {
            let cost = game.catalog[humvee].cost;
            advance_until(
                game,
                18_000,
                "ordinary supplies funding Scout Drone carrier",
                |game| game.players[0].cash >= cost,
            )?;
            let vehicle = produce_type(game, base.factory, humvee, 0)?;
            research(game, vehicle, "Upgrade_AmericaScoutDrone")?;
            let id = game
                .produced_drone(vehicle)
                .ok_or("paid Scout Drone missing")?;
            require(
                game.entities[id].type_index == scout
                    && game.entities[id].drone_parent() == Some(vehicle),
                "Scout Drone purchase changed its original payload or parent",
            )?;
            Ok(id)
        };
        let existing = purchase(&mut game)?;
        require(
            [
                game.entities[existing].health,
                game.entities[existing].max_health,
            ]
            .map(f32::to_bits)
                == [100.0_f32; 2].map(f32::to_bits),
            "unresearched Scout Drone health differs from the original 100",
        )?;
        research(&mut game, base.center, "Upgrade_AmericaDroneArmor")?;
        let future = purchase(&mut game)?;
        require(
            [existing, future].into_iter().all(|id| {
                let actor = &game.entities[id];
                [actor.health, actor.max_health].map(f32::to_bits)
                    == [125.0_f32; 2].map(f32::to_bits)
            }),
            "Drone Armor did not add 25 current and maximum HP to both paid Scout Drones",
        )
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn earned_a10_and_paradrop_use_original_ranked_flights() -> Result<(), Box<dyn Error>> {
        let (assets, battle, mut animations) =
            crate::cleanup_verification::tests::installed_with_animations();
        let mut game = battle.simulation;
        let mut particles = crate::particles::load(&assets, &game.catalog)?;
        require(
            crate::particles::weapon_fx_sounds(&assets, &game.catalog)?
                .iter()
                .any(|name| name == "A10ThunderboltMissileWeaponSound"),
            "the original detached missile ignition audio dependency is missing",
        )?;
        game.reset(Faction::Usa, Faction::China)?;
        crate::progression_verification::earn_combat_rank(&mut game, 3)?;
        let caster = friendly_entity(&game, Kind::CommandCenter, 0)?;
        let target = game.entities[caster].position;
        for rank in 1..=3 {
            game.purchase_science(0, &format!("SCIENCE_A10ThunderboltMissileStrike{rank}"))?;
        }
        let launch_actors = game.entities.len();
        game.cast_power(
            caster,
            "SuperweaponA10ThunderboltMissileStrike",
            PowerTarget::Position(target),
        )?;
        let a10 = template(&game, "AmericaJetA10Thunderbolt")?;
        let missile = template(&game, "A10ThunderboltMissile")?;
        require(
            game.entities[launch_actors..]
                .iter()
                .filter(|actor| actor.type_index == a10)
                .count()
                == 3,
            "A-10 III did not start an original three-aircraft formation",
        )?;
        advance_until(
            &mut game,
            1800,
            "original A-10 wing-bone missile ignition audio",
            |game| {
                crate::particles::detached_ignition_sound(
                    game,
                    &mut particles,
                    &mut animations,
                    missile,
                )
            },
        )?;
        game.reset(Faction::Usa, Faction::China)?;
        crate::progression_verification::earn_combat_rank(&mut game, 3)?;
        let caster = friendly_entity(&game, Kind::CommandCenter, 0)?;
        let target = game.entities[caster].position;
        for rank in 1..=3 {
            game.purchase_science(0, &format!("SCIENCE_Paradrop{rank}"))?;
        }
        let launch_actors = game.entities.len();
        game.cast_power(
            caster,
            "SuperweaponParadropAmerica",
            PowerTarget::Position(target),
        )?;
        let plane = template(&game, "AmericaJetCargoPlane")?;
        let ranger = template(&game, "AmericaInfantryRanger")?;
        let spawned = &game.entities[launch_actors..];
        require(
            spawned
                .iter()
                .filter(|actor| actor.type_index == plane)
                .count()
                == 2
                && spawned
                    .iter()
                    .filter(|actor| actor.type_index == ranger)
                    .count()
                    == 20,
            "Paradrop III lost its two independently authored ten-Ranger deliveries",
        )?;
        advance_until(
            &mut game,
            3600,
            "Paradrop Rangers landing from original parachutes",
            |game| {
                game.entities[launch_actors..].iter().any(|actor| {
                    actor.type_index == ranger && actor.alive() && actor.contained_by().is_none()
                })
            },
        )?;
        Ok(())
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn earned_daisy_cutter_science_and_paid_moab_keep_cooldown_and_payload_identity()
    -> Result<(), Box<dyn Error>> {
        let mut game = setup();
        game.set_match_options(50_000, 1);
        game.reset(Faction::Usa, Faction::China)?;
        crate::progression_verification::earn_combat_rank(&mut game, 5)?;
        let center = friendly_entity(&game, Kind::CommandCenter, 0)?;
        let strategy = construct_strategy_center(&mut game)?;
        game.purchase_science(0, "SCIENCE_DaisyCutter")?;
        let power = "SuperweaponDaisyCutter";
        let target = game.entities[center].position;
        game.cast_power(center, power, PowerTarget::Position(target))?;
        let daisy = template(&game, "DaisyCutterBomb")?;
        let moab = template(&game, "MOAB")?;
        require(
            game.entities
                .iter()
                .any(|actor| actor.type_index == daisy && actor.exists())
                && !game
                    .entities
                    .iter()
                    .any(|actor| actor.type_index == moab && actor.exists()),
            "unresearched Daisy Cutter selected the wrong payload",
        )?;
        let reload_end = game.tick + u64::from(game.power_cooldown_remaining(center, power));
        research(&mut game, strategy, "Upgrade_AmericaMOAB")?;
        require(
            game.players[0].has_science("SCIENCE_MOAB"),
            "paid research did not grant SCIENCE_MOAB",
        )?;
        require(
            game.tick + u64::from(game.power_cooldown_remaining(center, power)) == reload_end,
            "MOAB research altered the in-progress Daisy Cutter recharge",
        )?;
        advance_until(
            &mut game,
            10_801,
            "original Daisy Cutter recharge",
            |game| game.tick >= reload_end,
        )?;
        game.cast_power(center, power, PowerTarget::Position(target))?;
        require(
            game.entities
                .iter()
                .any(|actor| actor.type_index == moab && actor.exists()),
            "researched Daisy Cutter did not select the original MOAB",
        )?;
        Ok(())
    }
}
