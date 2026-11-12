//! Original paid Microwave Tank, building disable beam and authored garrison clearing.
use crate::{
    capture_framing::{self, AircraftFrame},
    input::{self, Mouse, PickTarget},
    laser_beam::PulseInspection,
    microwave_verification::{self, Emission, Setup},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

mod emitter;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    began: Option<f64>,
    health: [f32; 2],
    diagnostic_tick: u64,
    diagnostic_saved: bool,
    last_actual: u64,
    emission: Option<Emission>,
    production: Option<microwave_verification::EmitterProduction>,
    evidence: Option<emitter::Evidence>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = microwave_verification::prepare(&mut battle.simulation)?;
    let health = [setup.target, setup.garrison].map(|id| battle.simulation.entities[id].health);
    battle.select_objects(&[setup.tank], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        began: None,
        health,
        diagnostic_tick: 0,
        diagnostic_saved: false,
        last_actual: battle.simulation.tick,
        emission: None,
        production: None,
        evidence: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, begin);
    app.add_systems(FixedUpdate, record.after(crate::laser_beam::record));
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

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    time: ResMut<'w, Time<Virtual>>,
    clock: Res<'w, Time<Real>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    let now = control.clock.elapsed_secs_f64();
    if now - *check.began.get_or_insert(now) > 90.0 {
        error!("Microwave native capture timed out phase={}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() || !matches!(check.phase, 0 | 2 | 4 | 6 | 8) {
        return;
    }
    if check.phase == 6 {
        check.phase = 7;
        control.time.unpause();
        return;
    }
    if matches!(check.phase, 4 | 8) {
        if let Err(error) = emitter::begin(&mut check, &mut control) {
            error!("Microwave native emitter setup: {error}");
            check.phase = 255;
            control.exit.write(AppExit::error());
        }
        return;
    }
    let target = if check.phase == 0 {
        check.setup.target
    } else {
        check.setup.garrison
    };
    let point = Vec2::from_array(control.battle.simulation.entities[target].position);
    if !input::context_order(
        &mut control.battle,
        Some(point),
        Some(PickTarget::Unit(target)),
        &mut control.mouse,
    ) {
        error!(
            "Microwave native ordinary Attack rejected phase={}",
            check.phase
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    check.phase += 1;
    control.time.unpause();
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
    if let Some(evidence) = &check.evidence {
        evidence.restore_camera(&mut camera);
        return;
    }
    let game = &battle.simulation;
    let tank = game.object_position(check.setup.tank);
    let target = game.object_position(if check.phase <= 2 {
        check.setup.target
    } else if check.phase <= 4 {
        check.setup.garrison
    } else {
        check
            .emission
            .as_ref()
            .map_or(check.setup.garrison, |emission| emission.victim)
    });
    let focus = std::array::from_fn(|axis| (tank[axis] + target[axis]) * 0.5);
    // Frame the source weapon range horizontally, above the original control bar.
    camera.settings.yaw = (target[1] - tank[1]).atan2(target[0] - tank[0]);
    camera.distance = if check.phase >= 5 && check.emission.is_some() {
        // The final pair frames the infantry and the visible emitter distortion.
        camera.settings.min_height
    } else {
        camera.settings.max_height
    };
    capture_framing::focus_on(&mut camera, terrain, focus);
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    pulses: PulseInspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    rig: Res<'w, CameraRig>,
    camera: Single<
        'w,
        's,
        (&'static Camera, &'static GlobalTransform),
        With<crate::playtest_view::WorldCamera>,
    >,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !matches!(check.phase, 1 | 3 | 5 | 7) || !view.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let game = &view.battle.simulation;
        let tank = check.setup.tank;
        let target = if check.phase == 1 {
            check.setup.target
        } else if check.phase == 3 {
            check.setup.garrison
        } else {
            check
                .emission
                .as_ref()
                .map_or(check.setup.garrison, |emission| emission.victim)
        };
        diagnose(&mut check, &view.battle, &view.framing, target)?;
        if !check.diagnostic_saved {
            view.capture.intermediate("microwave-diagnostic.png");
            check.diagnostic_saved = true;
            return Ok(());
        }
        require(game.entities[tank].alive(), "native Microwave Tank died")?;
        require(
            game.entities[target].alive(),
            "native microwave target died",
        )?;
        require(
            [check.setup.target, check.setup.garrison].map(|id| game.entities[id].health.to_bits())
                == check.health.map(f32::to_bits),
            "native microwave beam changed building health",
        )?;
        if !view.framing.visible(&view.battle, tank)?
            || !view.framing.visible(&view.battle, target)?
        {
            return Ok(());
        }
        if check.phase == 1 {
            if !game.entities[target].subdued() {
                return Ok(());
            }
            require(
                check
                    .setup
                    .riders
                    .iter()
                    .all(|&id| game.entities[id].alive()),
                "garrison was cleared before the separate building disable capture",
            )?;
            if !view.pulses.visible(tank, "MicrowaveTankBuildingDisabler")? {
                return Ok(());
            }
            view.time.pause();
            view.capture.intermediate("microwave-disabled-building.png");
            check.phase = 2;
        } else if check.phase == 3 {
            if !game.passengers(target).is_empty()
                || check
                    .setup
                    .riders
                    .iter()
                    .any(|&id| game.entities[id].alive())
            {
                return Ok(());
            }
            if !view.pulses.visible(tank, "MicrowaveTankBuildingClearer")? {
                return Ok(());
            }
            view.time.pause();
            view.capture.intermediate("microwave-garrison-cleared.png");
            check.phase = 4;
        } else {
            emitter::inspect(&mut check, &mut view)?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Microwave native capture phase {}: {error}", check.phase);
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}

fn emitter_visible(
    check: &Check,
    battle: &Match,
    framing: &AircraftFrame,
    particles: &ParticleInspection,
    camera: (&Camera, &GlobalTransform),
) -> Result<bool, Box<dyn Error>> {
    let Some((frame, veterancy)) = check
        .emission
        .as_ref()
        .and_then(|emission| emission.damage_frame)
    else {
        return Ok(false);
    };
    let game = &battle.simulation;
    let tank = check.setup.tank;
    let unit = &game.catalog[game.entities[tank].type_index];
    let index = unit
        .fire_weapon_index(0)
        .ok_or("missing native emitter weapon")?;
    let Some(viewport) = framing.uncovered_viewport() else {
        return Ok(false);
    };
    particles.weapon_fire_above_hud(
        (game.entities[tank].type_index, index, veterancy),
        frame,
        battle.generation,
        (camera, viewport),
    )
}

fn record(mut check: ResMut<Check>, battle: Res<Match>, mut exit: MessageWriter<AppExit>) {
    let battle = battle.into_inner();
    if check.phase == 255 {
        return;
    }
    let game = &battle.simulation;
    let index = game.catalog[game.entities[check.setup.tank].type_index].fire_weapon_index(0);
    if game
        .presentation
        .shots
        .iter()
        .any(|shot| shot.source == check.setup.tank && Some(shot.weapon_index) != index)
    {
        check.last_actual = game.tick;
    }
    if matches!(check.phase, 5 | 7) {
        let check = &mut *check;
        if let Some(emission) = &mut check.emission
            && let Err(error) =
                microwave_verification::observe_emitter(game, &check.setup, emission)
        {
            error!("Microwave native emitter damage: {error}");
            check.phase = 255;
            exit.write(AppExit::error());
        }
    }
}

fn diagnose(
    check: &mut Check,
    battle: &Match,
    framing: &AircraftFrame,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    let game = &battle.simulation;
    if game.tick < check.diagnostic_tick {
        return Ok(());
    }
    println!(
        "Microwave capture phase={} tick={} subdued={} subdual={} tank_idle={} tank={} target={}",
        check.phase,
        game.tick,
        game.entities[target].subdued(),
        game.entities[target].subdual_damage(),
        matches!(
            game.entities[check.setup.tank].ai.order,
            generals_sim::skirmish::Order::Idle
        ),
        framing.visibility_diagnostic(battle, check.setup.tank)?,
        framing.visibility_diagnostic(battle, target)?,
    );
    check.diagnostic_tick = game.tick + 300;
    Ok(())
}
