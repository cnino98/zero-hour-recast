//! Paid USA airfield and aircraft orders on the original Fallen Empire map.
pub mod crash;
use crate::{
    playtest::MAP_NAME,
    skirmish_data,
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type, require,
    },
    strategy_verification::template,
    terrain::Terrain,
    world_data,
};
use generals_assets::{map::MapData, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::{error::Error, path::Path};

pub struct Setup {
    pub airfield: usize,
    pub raptor: usize,
    pub comanche: usize,
    pub target: usize,
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
    let setup = prepare(&mut game)?;
    approach(&mut game, setup.raptor, setup.target)?;
    verify_sortie(&mut game, &setup)?;
    approach(&mut game, setup.raptor, setup.target)?;
    game.attack(setup.raptor, setup.target)?;
    advance_until(&mut game, 1800, "second paid Raptor sortie", |game| {
        game.presentation
            .shots
            .iter()
            .any(|shot| shot.source == setup.raptor)
    })?;
    game.stop(setup.raptor)?;
    approach(&mut game, setup.comanche, setup.target)?;
    game.attack(setup.comanche, setup.target)?;
    advance_until(
        &mut game,
        1800,
        "Comanche original missile attack",
        |game| {
            game.projectiles
                .iter()
                .any(|missile| missile.source == setup.comanche)
        },
    )
    .map_err(|error| format!("{error}; {}", details(&game, setup.comanche)))?;
    verify_rocket_pods(&mut game, &setup)?;
    crash::verify(&mut game)?;
    println!(
        "Paid aircraft: USA airfield, Raptor and Comanche production; real taxi/takeoff, four-shot sortie, native return/rearm and second sortie; Comanche missiles and researched twenty-round rocket-pod scatter passed"
    );
    Ok(())
}

fn verify_rocket_pods(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    research_rocket_pods(game, setup)?;
    let point = game.entities[setup.target].position;
    game.fire_weapon_at(setup.comanche, 2, point)?;
    let mut shots = 0;
    for _ in 0..3600 {
        game.step();
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == setup.comanche && shot.weapon_slot == 2)
            .count();
        if shots > 0
            && !matches!(
                game.entities[setup.comanche].ai.order,
                generals_sim::skirmish::Order::FireAt { .. }
            )
        {
            require(
                shots == 20,
                "Rocket pods did not finish exactly one original clip",
            )?;
            require(
                game.entities[setup.comanche].weapon_slot() == 0,
                "Rocket pod reload did not restore PRIMARY selection",
            )?;
            return Ok(());
        }
    }
    Err(format!(
        "Rocket pod clip timed out: shots={shots}; {}",
        details(game, setup.comanche)
    )
    .into())
}

pub fn research_rocket_pods(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    require(
        !game.location_weapon_ready(setup.comanche, 2),
        "Rocket pods available before research",
    )?;
    game.stop(setup.comanche)?;
    crate::strategy_verification::research(game, setup.airfield, "Upgrade_ComancheRocketPods")?;
    require(
        game.location_weapon_ready(setup.comanche, 2),
        "Paid rocket-pod research did not allocate TERTIARY",
    )?;
    Ok(())
}

pub fn prepare(game: &mut Skirmish) -> Result<Setup, Box<dyn Error>> {
    game.reset(Faction::Usa, Faction::China)?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    construct(game, builder, Kind::PowerPlant, 0)?;
    construct(game, builder, Kind::SupplyCenter, 0)?;
    let index = template(game, "AmericaAirfield")?;
    let airfield = construct_type(game, builder, index, 0)?;
    let index = template(game, "AmericaJetRaptor")?;
    let raptor = produce_type(game, airfield, index, 0)?;
    let index = template(game, "AmericaVehicleComanche")?;
    let comanche = produce_type(game, airfield, index, 0)?;
    advance_until(
        game,
        3600,
        "Raptor original hangar taxi and parking",
        |game| {
            game.entities[raptor].aircraft_phase() == Some("idle")
                && game.entities[raptor].weapon_rounds_remaining(0) == 4
        },
    )
    .map_err(|error| format!("{error}; {}", details(game, raptor)))?;
    let target = friendly_entity(game, Kind::CommandCenter, 1)?;
    Ok(Setup {
        airfield,
        raptor,
        comanche,
        target,
    })
}

pub fn approach(game: &mut Skirmish, actor: usize, target: usize) -> Result<(), Box<dyn Error>> {
    let point = game.entities[target].position;
    let goal = [point[0] + 100.0, point[1] - 160.0];
    game.move_to(actor, goal)?;
    advance_until(
        game,
        5400,
        "aircraft ordinary takeoff and approach",
        |game| {
            let aircraft = &game.entities[actor];
            aircraft.aircraft_phase() == Some("idle")
                && game.height_above_ground(actor)
                    > game.catalog[aircraft.type_index]
                        .locomotor
                        .airborne_targeting_height
                && (aircraft.position[0] - goal[0]).hypot(aircraft.position[1] - goal[1]) < 120.0
                && game.logically_visible(0, target)
        },
    )
    .map_err(|error| format!("{error}; {}", details(game, actor)).into())
}

fn verify_sortie(game: &mut Skirmish, setup: &Setup) -> Result<(), Box<dyn Error>> {
    require(
        game.entities[setup.airfield].complete(),
        "Raptor rearm requires its paid, complete original airfield",
    )?;
    let health = game.entities[setup.target].health;
    game.attack(setup.raptor, setup.target)?;
    let mut shots = 0;
    let mut missiles = false;
    let mut reload = false;
    let mut minimum_health = health;
    let mut impacts = Vec::new();
    for _ in 0..7200 {
        game.step();
        minimum_health = minimum_health.min(game.entities[setup.target].health);
        for event in &game.presentation.weapon_fx {
            if let generals_sim::skirmish::WeaponFxEvent::Projectile(event) = event
                && event.source == setup.raptor
                && matches!(
                    event.stage,
                    generals_sim::skirmish::ProjectileFxStage::Detonation
                )
            {
                impacts.push(event.position);
            }
        }
        shots += game
            .presentation
            .shots
            .iter()
            .filter(|shot| shot.source == setup.raptor)
            .count();
        missiles |= game
            .projectiles
            .iter()
            .any(|missile| missile.source == setup.raptor);
        reload |= game.entities[setup.raptor].aircraft_phase() == Some("reload ammunition");
        if reload
            && game.entities[setup.raptor].aircraft_phase() == Some("idle")
            && game.entities[setup.raptor].weapon_rounds_remaining(0) == 4
        {
            require(
                shots == 4,
                &format!(
                    "Raptor rearmed after launching {shots} of its four original missiles; impacts={impacts:?}; {}",
                    details(game, setup.raptor)
                ),
            )?;
            require(missiles, "Raptor fired no native missile objects")?;
            require(minimum_health < health, "Raptor missiles failed to damage their real target")
                .map_err(|error| format!("{error}; target={:?}; initial={health}; minimum={minimum_health}; final={}; impacts={impacts:?}; {}", game.object_position(setup.target), game.entities[setup.target].health, details(game, setup.raptor)))?;
            println!(
                "  Raptor four-missile sortie damage: initial={health} minimum={minimum_health} after-rearm={}",
                game.entities[setup.target].health
            );
            return Ok(());
        }
        require(
            game.entities[setup.raptor].alive(),
            "Raptor died during its first sortie",
        )?;
    }
    Err(format!(
        "Raptor return/rearm timed out: shots={shots} missiles={missiles} reload={reload}; {}",
        details(game, setup.raptor)
    )
    .into())
}

pub fn details(game: &Skirmish, id: usize) -> String {
    let actor = &game.entities[id];
    format!(
        "aircraft={id} tick={} phase={:?} position={:?} height={:?} health={} ammo={} moving={}",
        game.tick,
        actor.aircraft_phase(),
        actor.position,
        actor.flight_height(),
        actor.health,
        actor.weapon_rounds_remaining(0),
        actor.moving()
    )
}

#[cfg(test)]
mod tests {
    use crate::input::{self, PickTarget};
    use bevy::math::Vec2;
    use generals_sim::skirmish::{Order, diagnostics};

    #[ignore = "requires original game installation"]
    #[test]
    fn paid_aircraft_taxi_attack_return_rearm_and_helicopter_fire() {
        let root = &generals_assets::game_root().unwrap();
        super::run(root).unwrap();
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn researched_comanche_interrupts_offset_travel_to_aim_and_fire_normal_missiles() {
        comanche_offset_attack_after_raptor_capture(0);
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn researched_comanche_fires_after_three_tick_raptor_capture_delay() {
        comanche_offset_attack_after_raptor_capture(3);
    }

    fn comanche_offset_attack_after_raptor_capture(capture_ticks: u32) {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let root = &generals_assets::game_root().unwrap();
        let setup = super::prepare(&mut battle.simulation).unwrap();
        super::research_rocket_pods(&mut battle.simulation, &setup).unwrap();
        super::approach(&mut battle.simulation, setup.raptor, setup.target).unwrap();
        battle
            .simulation
            .attack(setup.raptor, setup.target)
            .unwrap();
        super::advance_until(&mut battle.simulation, 1800, "first Raptor shot", |game| {
            game.presentation
                .shots
                .iter()
                .any(|shot| shot.source == setup.raptor)
        })
        .unwrap();
        // Native R19 captured the first shot at5815 and issued Stop at5818.
        // Advance real logic; preserve the ordinary move/context-attack inputs.
        for _ in 0..capture_ticks {
            battle.simulation.step();
        }
        battle.simulation.stop(setup.raptor).unwrap();
        let target = Vec2::from_array(battle.simulation.entities[setup.target].position);
        battle
            .simulation
            .move_to(setup.comanche, [target.x + 100.0, target.y - 160.0])
            .unwrap();
        super::advance_until(
            &mut battle.simulation,
            5400,
            "offset Comanche approach",
            |game| {
                Vec2::from_array(game.entities[setup.comanche].position).distance(target) < 220.0
                    && game.logically_visible(0, setup.target)
            },
        )
        .unwrap();
        let actor = &battle.simulation.entities[setup.comanche];
        assert!(matches!(actor.ai.order, Order::Move(_)));
        let direction = target - Vec2::from_array(actor.position);
        assert!(Vec2::from_angle(actor.facing).angle_to(direction).abs() > 0.1);
        let mut mouse = input::load(
            &assets,
            &root.join("UserData/Options.ini"),
            &battle.simulation.catalog,
        )
        .unwrap();
        battle.select_objects(&[setup.comanche], true);
        assert!(input::context_order(
            &mut battle,
            Some(target),
            Some(PickTarget::Unit(setup.target)),
            &mut mouse,
        ));
        assert!(
            matches!(battle.simulation.entities[setup.comanche].ai.order, Order::Attack(id) if id == setup.target)
        );
        let game = &mut battle.simulation;
        let health = game.entities[setup.target].health;
        let mut missile = false;
        let mut damage = false;
        for _ in 0..1800 {
            game.step();
            missile |= game
                .presentation
                .shots
                .iter()
                .any(|shot| shot.source == setup.comanche && shot.weapon_slot == 1);
            assert!(
                !game
                    .presentation
                    .shots
                    .iter()
                    .any(|shot| shot.source == setup.comanche && shot.weapon_slot == 2)
            );
            damage |= game.entities[setup.target].health < health;
            if missile && damage {
                break;
            }
        }
        assert!(
            missile && damage,
            "missile={missile} damage={damage}; {}; {}",
            super::details(game, setup.comanche),
            diagnostics::weapon_details(game, setup.comanche, setup.target)
        );
    }
}
