//! Original CIA, Satellite and Spy Drone controls over the same enemy position.
use crate::{
    capture_framing::{AircraftFrame, focus_on},
    control_bar::UiData,
    input::{self, CommandMode, Mouse},
    playtest::{CameraRig, Match},
    playtest_ui::{dispatch_command, selected_command},
    playtest_view::Capture,
    skirmish_verification::{friendly_entity, require},
    source_audio::EventInspection,
    spectre_decals::GridInspection,
    strategy_verification::template,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::Kind;
use std::error::Error;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Cia,
    Satellite,
    DroneGrid,
    Drone,
    Cleared,
    Finished,
}

impl Phase {
    const fn power(self) -> &'static str {
        match self {
            Self::Cia => "SuperweaponCIAIntelligence",
            Self::Satellite => "SpecialPowerSpySatellite",
            Self::DroneGrid | Self::Drone => "SpecialPowerSpyDrone",
            Self::Cleared | Self::Finished => unreachable!(),
        }
    }

    const fn image(self) -> &'static str {
        match self {
            Self::Cia => "p03-recon-cia.png",
            Self::Satellite => "p03-recon-satellite.png",
            Self::DroneGrid => "p03-recon-drone-grid.png",
            Self::Drone => "p03-recon-drone.png",
            Self::Cleared | Self::Finished => unreachable!(),
        }
    }
}

#[derive(Resource)]
struct Check {
    phase: Phase,
    framed: Option<Phase>,
    strategy: usize,
    center: usize,
    target: usize,
    ping_type: usize,
    drone_type: usize,
    first: usize,
    heard: [bool; 3],
    grid_seen: [bool; 2],
    issued: bool,
    started: Option<f64>,
}

impl Check {
    const fn provider(&self) -> usize {
        if matches!(self.phase, Phase::Cia) {
            self.strategy
        } else {
            self.center
        }
    }
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let strategy = crate::strategy_verification::prepare(game)?.center;
    let center = friendly_entity(game, Kind::CommandCenter, 0)?;
    let target = friendly_entity(game, Kind::Builder, 1)?;
    game.stop(target)?;
    game.purchase_science(0, "SCIENCE_SpyDrone")?;
    require(
        game.can_use_power(strategy, Phase::Cia.power()),
        "paid Strategy Center did not finish with stock CIA readiness",
    )?;
    require(
        !game.visible_to(0, target),
        "recon target is already revealed",
    )?;
    let check = Check {
        phase: Phase::Cia,
        framed: None,
        strategy,
        center,
        target,
        ping_type: template(game, "SpySatellitePing")?,
        drone_type: template(game, "AmericaVehicleSpyDrone")?,
        first: game.entities.len(),
        heard: [false; 3],
        grid_seen: [false; 2],
        issued: false,
        started: None,
    };
    battle.select_objects(&[strategy], true);
    app.insert_resource(check);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(PreUpdate, hear);
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
    Ok(())
}

fn hear(mut check: ResMut<Check>, audio: EventInspection) {
    let sounds = audio.played([
        (
            "CIAIntelligenceActivate",
            Some(check.strategy),
            &["sciainte"],
        ),
        ("SpySatellite", None, &["sspysate"]),
        ("SpyDroneCreate", None, &["vdr2crea", "vdr2creb"]),
    ]);
    for (previous, played) in check.heard.iter_mut().zip(sounds) {
        *previous |= played;
    }
}

fn frame(
    check: ResMut<Check>,
    battle: Res<Match>,
    capture: Res<Capture>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    if !capture.into_inner().ready_for_scenario() {
        return;
    }
    let check = check.into_inner();
    let game = &battle.into_inner().simulation;
    let actor = match check.phase {
        Phase::Cia if !check.heard[0] => check.strategy,
        Phase::Drone | Phase::Cleared => {
            spawned(check, game, check.drone_type).unwrap_or(check.target)
        }
        _ => check.target,
    };
    camera.distance = if matches!(check.phase, Phase::Satellite | Phase::DroneGrid) {
        camera.settings.max_height
    } else {
        camera.settings.min_height
    };
    let position = game.object_position(actor);
    focus_on(&mut camera, terrain.into_inner(), position);
    check.framed = Some(check.phase);
}

fn spawned(check: &Check, game: &generals_sim::skirmish::Skirmish, kind: usize) -> Option<usize> {
    game.entities[check.first..]
        .iter()
        .position(|actor| actor.type_index == kind && actor.owner == 0)
        .map(|index| index + check.first)
}

#[derive(SystemParam)]
struct View<'w, 's> {
    frame: AircraftFrame<'w, 's>,
    grid: GridInspection<'w, 's>,
    ui: Res<'w, UiData>,
    clock: Res<'w, Time<Real>>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    mut time: ResMut<Time<Virtual>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == Phase::Finished
        || check.framed != Some(check.phase)
        || !capture.ready_for_scenario()
    {
        return;
    }
    let now = view.clock.elapsed_secs_f64();
    let started = *check.started.get_or_insert(now);
    time.unpause();
    let outcome = if now - started > 120.0 {
        Err(format!(
            "recon timed out during {:?}, audio={:?}, grids={:?}",
            check.phase, check.heard, check.grid_seen
        )
        .into())
    } else {
        observe(&mut check, view, &mut battle, &mut mouse)
    };
    match outcome {
        Ok(true) => {
            time.pause();
            let phase = check.phase;
            check.started = None;
            check.issued = false;
            check.phase = match phase {
                Phase::Cia => Phase::Satellite,
                Phase::Satellite => Phase::DroneGrid,
                Phase::DroneGrid => Phase::Drone,
                Phase::Drone => Phase::Cleared,
                Phase::Cleared | Phase::Finished => Phase::Finished,
            };
            println!(
                "Native P03 USA reconnaissance {phase:?} at frame {}",
                battle.simulation.tick
            );
            if check.phase == Phase::Finished {
                capture.finish_scenario();
            } else {
                capture.intermediate(phase.image());
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Native P03 reconnaissance: {error}");
            check.phase = Phase::Finished;
            exit.write(AppExit::error());
        }
    }
}

fn observe(
    check: &mut Check,
    view: View,
    battle: &mut Match,
    mouse: &mut Mouse,
) -> Result<bool, Box<dyn Error>> {
    let View {
        frame, grid, ui, ..
    } = view;
    let game = &battle.simulation;
    let generation = battle.generation;
    let target = check.target;
    let ping = spawned(check, game, check.ping_type);
    let drone = spawned(check, game, check.drone_type);
    if !check.issued
        && matches!(
            check.phase,
            Phase::Cia | Phase::Satellite | Phase::DroneGrid
        )
    {
        if check.phase == Phase::Satellite && game.visible_to(0, target)
            || check.phase == Phase::DroneGrid
                && !ping
                    .is_some_and(|id| !game.entities[id].exists() && grid.retired(id, generation))
        {
            return Ok(false);
        }
        issue(check, ui.into_inner(), battle, mouse)?;
        return Ok(false);
    }
    if matches!(
        check.phase,
        Phase::Cia | Phase::Satellite | Phase::DroneGrid | Phase::Drone
    ) && battle
        .simulation
        .power_cooldown_remaining(check.provider(), check.phase.power())
        == 0
    {
        return Ok(false);
    }
    let game = &battle.simulation;
    match check.phase {
        Phase::Cia => {
            Ok(check.heard[0] && game.visible_to(0, target) && frame.visible(battle, target)?)
        }
        Phase::Satellite => {
            let Some(id) = ping else { return Ok(false) };
            let visible = grid.visible(id, generation)?;
            check.grid_seen[0] |= visible;
            Ok(check.heard[1] && visible)
        }
        Phase::DroneGrid => {
            let Some(id) = drone else { return Ok(false) };
            let visible = grid.visible(id, generation)?;
            check.grid_seen[1] |= visible;
            Ok(check.heard[2] && visible && game.height_above_ground(id) > 0.0)
        }
        Phase::Drone => {
            let Some(id) = drone else { return Ok(false) };
            require(
                game.entities[check.first..]
                    .iter()
                    .filter(|actor| actor.type_index == check.drone_type && actor.owner == 0)
                    .count()
                    == 1,
                "recon sequence created more than one original Spy Drone",
            )?;
            Ok(check.heard[2]
                && check.grid_seen[1]
                // The stock spawn descends from 300 units while its grid retires.
                // Keep the complete-body proof on that same ordinary timeline.
                && game.height_above_ground(id) > 0.0
                && frame.visible(battle, id)?)
        }
        Phase::Cleared => {
            let Some(id) = drone else { return Ok(false) };
            Ok(grid.retired(id, generation)
                && (game.height_above_ground(id) - 90.0).abs() < 2.0
                && game.visible_to(0, target)
                && frame.visible(battle, id)?)
        }
        Phase::Finished => Ok(false),
    }
}

fn issue(
    check: &mut Check,
    ui: &UiData,
    battle: &mut Match,
    mouse: &mut Mouse,
) -> Result<(), Box<dyn Error>> {
    let phase = check.phase;
    let provider = check.provider();
    battle.select_objects(&[provider], true);
    let power = phase.power();
    let action = (1..=14)
        .filter_map(|slot| selected_command(battle, ui, slot))
        .find(|command| command.action.power() == Some(power))
        .ok_or("original USA reconnaissance button missing")?
        .action;
    dispatch_command(battle, mouse, action);
    if phase != Phase::Cia {
        require(
            matches!(mouse.pending_command, CommandMode::Power { caster, name, .. }
                if caster == provider && name == power),
            "original reconnaissance button did not enter targeting mode",
        )?;
        let position = battle.simulation.entities[check.target].position;
        input::targeted_order(battle, mouse, Vec2::from_array(position), None);
        require(
            matches!(mouse.pending_command, CommandMode::Normal),
            "original reconnaissance target was rejected",
        )?;
    }
    check.issued = true;
    Ok(())
}
