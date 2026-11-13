//! Original map Pad, paid capture and full-duration cargo delivery observation.
mod supply_drop_zone;
use crate::{
    capture_framing::{self, AircraftFrame},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::{advance_until, construct, friendly_entity, produce_type, require},
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::{Faction, Kind, Order, Skirmish};
use std::error::Error;
#[cfg(test)]
pub use supply_drop_zone::build_paid_zone;
pub use supply_drop_zone::{
    prepare as prepare_supply_drop_zone, register as register_supply_drop_zone,
};

#[derive(Resource)]
struct Check {
    pad: usize,
    plane: usize,
    chute: usize,
    tank: usize,
    rally: [f32; 2],
    bounds: [[f32; 2]; 2],
    phase: u8,
    started: Option<f64>,
}

pub fn capture_original_pad(
    game: &mut Skirmish,
    faction: Faction,
) -> Result<usize, Box<dyn Error>> {
    game.reset_sandbox(faction)?;
    let pad = game
        .entities
        .iter()
        .position(|actor| {
            actor.alive() && game.catalog[actor.type_index].name == "TechReinforcementPad"
        })
        .ok_or("original map has no Reinforcement Pad")?;
    require(
        game.players[game.entities[pad].owner].faction == Faction::Civilian,
        "original Reinforcement Pad is not owned by its civilian map side",
    )?;
    let builder = friendly_entity(game, Kind::Builder, 0)?;
    if game.type_for(0, Kind::PowerPlant).is_some() {
        construct(game, builder, Kind::PowerPlant, 0)?;
    }
    let barracks = construct(game, builder, Kind::Barracks, 0)?;
    let name = match faction {
        Faction::Usa => "AmericaInfantryRanger",
        Faction::China => "ChinaInfantryRedguard",
        Faction::Gla => "GLAInfantryRebel",
        Faction::Civilian => return Err("Pad capture requires a base player faction".into()),
    };
    let capturer = game
        .catalog
        .iter()
        .position(|unit| unit.name == name)
        .ok_or("original faction capture infantry is missing")?;
    let infantry = produce_type(game, barracks, capturer, 0)?;
    let research = game.catalog[game.entities[barracks].type_index]
        .maintenance
        .research
        .iter()
        .position(|upgrade| upgrade.name == "Upgrade_InfantryCaptureBuilding")
        .ok_or("original Barracks has no capture research")?;
    game.queue_upgrade(barracks, research)?;
    advance_until(game, 3600, "original paid capture research", |game| {
        game.entities[barracks].queue.is_empty()
    })?;
    let point = game.entities[pad].position;
    game.move_to(infantry, [point[0] + 75.0, point[1]])?;
    advance_until(game, 18000, "original pad scouting", |game| {
        game.can_capture(infantry, pad)
    })?;
    game.capture(infantry, pad)?;
    advance_until(game, 18000, "original paid pad capture", |game| {
        game.entities[pad].owner == 0
    })?;
    game.stop(infantry)?;
    Ok(pad)
}

/// Original Pad `DefaultProductionExit` goal, including its native 20-unit extension.
pub fn original_delivery_goal(game: &Skirmish, pad: usize) -> Result<[f32; 3], Box<dyn Error>> {
    let actor = &game.entities[pad];
    let unit = &game.catalog[actor.type_index];
    let exit = unit
        .behavior
        .production_exit
        .as_ref()
        .ok_or("original Pad lacks its production exit")?;
    require(
        unit.name == "TechReinforcementPad"
            && exit.use_spawn_rally_point
            && exit.extend_rally
            && !exit.snap_rally
            && exit.queue.is_none()
            && actor.rally_point.is_none(),
        "Pad observation requires its original DefaultProductionExit and no user rally",
    )?;
    require(
        exit.rally.map(f32::to_bits) == [0.0_f32, -60.0, 0.0].map(f32::to_bits),
        "original Pad NaturalRallyPoint changed",
    )?;
    // DefaultProductionExitUpdate.cpp144: getNaturalRallyPoint defaults to
    // offset=true. W3DWaypointBuffer's displayed marker requests false instead.
    let squared = exit.rally.map(|coordinate| coordinate * coordinate);
    let inverse_length = 1.0 / ((squared[0] + squared[1]) + squared[2]).sqrt();
    let local = exit.rally.map(|coordinate| {
        let normalized = coordinate * inverse_length;
        let extension = normalized * 20.0;
        coordinate + extension
    });
    require(
        local.map(f32::to_bits) == [0.0_f32, -80.0, 0.0].map(f32::to_bits),
        "original Pad extended local goal changed",
    )?;
    let (position, basis) = game.object_transform(pad);
    Ok(std::array::from_fn(|axis| {
        let products = [
            basis[axis][0] * local[0],
            basis[axis][1] * local[1],
            basis[axis][2] * local[2],
        ];
        ((products[0] + products[1]) + products[2]) + position[axis]
    }))
}

pub fn prepare(app: &mut App, battle: &mut Match, faction: Faction) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let pad = capture_original_pad(game, faction)?;
    let (plane_name, tank_name) = match faction {
        Faction::Usa => ("AmericaJetCargoPlane", "AmericaTankCrusader"),
        Faction::China => ("ChinaJetCargoPlane", "ChinaTankBattleMaster"),
        Faction::Gla => ("GLAJetCargoPlane", "GLATankScorpion"),
        Faction::Civilian => return Err("Pad capture requires a base player faction".into()),
    };
    let first = game.entities.len();
    let captured = game.tick;
    let delay = game.catalog[game.entities[pad].type_index]
        .behavior
        .ocl_update
        .as_ref()
        .ok_or("original Pad has no OCLUpdate")?
        .min_delay;
    advance_until(
        game,
        delay + 2,
        "original full-duration Pad timer",
        |game| {
            game.entities[first..]
                .iter()
                .any(|actor| game.catalog[actor.type_index].name == plane_name)
        },
    )?;
    require(
        game.tick >= captured + u64::from(delay),
        "Pad delivery preceded its original full-duration timer",
    )?;
    let plane = (first..game.entities.len())
        .find(|&id| game.catalog[game.entities[id].type_index].name == plane_name)
        .ok_or("Pad cargo plane was not created")?;
    let chute = *game
        .passengers(plane)
        .first()
        .ok_or("cargo plane is empty")?;
    let tank = *game
        .passengers(chute)
        .first()
        .ok_or("cargo chute is empty")?;
    require(
        game.catalog[game.entities[chute].type_index].name == "LargeParachute"
            && game.catalog[game.entities[tank].type_index].name == tank_name,
        "original faction delivery identity changed",
    )?;
    let rally = original_delivery_goal(game, pad)?;
    battle.started = true;
    battle.select_objects(&[pad], true);
    app.insert_resource(Check {
        pad,
        plane,
        chute,
        tank,
        rally: [rally[0], rally[1]],
        bounds: app.world().resource::<Terrain>().draw_region(),
        phase: 0,
        started: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, advance);
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn advance(
    mut check: ResMut<Check>,
    capture: Res<Capture>,
    clock: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == 255 {
        return;
    }
    let capture = capture.into_inner();
    let now = clock.into_inner().elapsed_secs_f64();
    if now - *check.started.get_or_insert(now) > 180.0 {
        error!("Original Pad capture timed out in phase {}", check.phase);
        check.phase = 255;
        exit.write(AppExit::error());
    } else if capture.ready_for_scenario() && matches!(check.phase, 0 | 2 | 4) {
        check.phase += 1;
        time.unpause();
    }
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let id = match check.phase {
        0..=2 => check.plane,
        3..=4 => check.chute,
        _ => check.tank,
    };
    camera.distance = camera.settings.max_height;
    capture_framing::focus_on(&mut camera, terrain, battle.simulation.object_position(id));
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !matches!(check.phase, 1 | 3 | 5) || !view.capture.ready_for_scenario() {
        return;
    }
    match visible_stage(&check, &view.battle, &view.framing) {
        Ok(true) => {
            view.time.pause();
            match check.phase {
                1 => {
                    view.capture.intermediate("reinforcement-pad-plane.png");
                    check.phase = 2;
                }
                3 => {
                    view.capture.intermediate("reinforcement-pad-descent.png");
                    check.phase = 4;
                }
                _ => {
                    view.capture.finish_scenario();
                    check.phase = 255;
                    println!(
                        "Original Pad: paid capture/full timer, cargo plane, opened parachute, released faction vehicle at rally and peaceful plane retirement; retained pixels require review"
                    );
                }
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Original Pad capture phase {}: {error}", check.phase);
            check.phase = 255;
            view.exit.write(AppExit::error());
        }
    }
}

fn visible_stage(
    check: &Check,
    battle: &Match,
    framing: &AircraftFrame,
) -> Result<bool, Box<dyn Error>> {
    let game = &battle.simulation;
    require(
        [check.pad, check.plane, check.chute, check.tank]
            .into_iter()
            .all(|id| game.entities[id].owner == 0),
        "original Pad delivery changed ownership",
    )?;
    require(game.entities[check.tank].alive(), "delivered vehicle died")?;
    match check.phase {
        1 => {
            require(
                game.entities[check.chute].contained_by() == Some(check.plane),
                "cargo was released before the plane observation",
            )?;
            framing.visible(battle, check.plane)
        }
        3 => {
            require(
                game.entities[check.tank].parachute_rider(),
                "vehicle landed before an opened parachute observation",
            )?;
            Ok(game.entities[check.chute].contained_by().is_none()
                && game.entities[check.chute].parachute_opened()
                && framing.visible(battle, check.chute)?
                && framing.visible(battle, check.tank)?
                && framing.separated_actor(battle, check.plane, check.chute)?
                && framing.separated_actor(battle, check.plane, check.tank)?)
        }
        _ => {
            if game.entities[check.plane].exists() {
                return Ok(false);
            }
            require(
                game.entities[check.plane].health > 0.0,
                "peaceful cargo retirement became a kill",
            )?;
            let position = game.object_position(check.plane);
            require(
                (0..2).any(|axis| {
                    position[axis] < check.bounds[0][axis] || position[axis] > check.bounds[1][axis]
                }),
                "cargo plane retired before leaving the original map",
            )?;
            let tank = &game.entities[check.tank];
            Ok(!tank.parachute_rider()
                && matches!(tank.ai.order, Order::Idle)
                && (tank.position[0] - check.rally[0]).abs() <= 10.0
                && (tank.position[1] - check.rally[1]).abs() <= 10.0
                && game.height_above_ground(check.tank).abs() <= 0.1
                && framing.visible(battle, check.tank)?)
        }
    }
}

// Device-free regression invokes the same source/geometry readiness as inspect.
#[cfg(test)]
pub fn test_descent(world: &mut World) -> (bool, bool) {
    use bevy::ecs::system::RunSystemOnce;
    world.resource_mut::<Check>().phase = 3;
    world
        .run_system_once(
            |check: Res<Check>, battle: Res<Match>, framing: AircraftFrame| {
                let game = &battle.simulation;
                let original_ready = game.entities[check.chute].contained_by().is_none()
                    && game.entities[check.chute].parachute_opened()
                    && game.entities[check.tank].parachute_rider()
                    && framing.visible(&battle, check.chute).unwrap()
                    && framing.visible(&battle, check.tank).unwrap();
                (
                    original_ready,
                    visible_stage(&check, &battle, &framing).unwrap(),
                )
            },
        )
        .unwrap()
}

#[cfg(test)]
pub fn test_descent_frame(world: &mut World) {
    use bevy::ecs::system::RunSystemOnce;
    world.resource_mut::<Check>().phase = 3;
    world.run_system_once(frame).unwrap();
}
