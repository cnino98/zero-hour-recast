//! Visible original loaded bus, first-death throw and held second-life hulk.
use crate::{
    battle_bus_verification::{self, Setup},
    capture_framing::{self, AircraftFrame},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    attacker_retreat_started: bool,
    camera: Option<(Vec2, f32)>,
    held_since: Option<f64>,
    diagnostic: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = battle_bus_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.bus], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        attacker_retreat_started: false,
        camera: None,
        held_since: None,
        diagnostic: false,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, begin);
    app.add_systems(
        Update,
        hold_camera
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

fn hold_camera(check: Res<Check>, mut camera: ResMut<CameraRig>) {
    if let Some((focus, distance)) = check.into_inner().camera {
        camera.focus = focus;
        camera.distance = distance;
    }
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    camera: ResMut<'w, CameraRig>,
    terrain: Res<'w, Terrain>,
    time: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: Res<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    if control.real.elapsed_secs() > 120.0 {
        error!(
            "Battle Bus native capture timed out phase={}: {}",
            check.phase,
            context(&control.battle.simulation, &check.setup)
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    let result = (|| {
        if check.phase == 2
            && !check.attacker_retreat_started
            && control.battle.simulation.entities[check.setup.bus].battle_bus_second_life()
        {
            // Stop returns the human tank to idle enemy acquisition. Retreat
            // on the simulation transition, independently of draw readiness.
            let game = &mut control.battle.simulation;
            let base = game.entities[check.setup.target].position;
            game.move_to(check.setup.attacker, [base[0] + 700.0, base[1]])?;
            check.attacker_retreat_started = true;
            println!(
                "Battle Bus native second-life retreat: {}",
                context(game, &check.setup)
            );
        }
        if !control.capture.ready_for_scenario() {
            return Ok(());
        }
        check.camera = None;
        if check.phase == 1 || check.phase == 5 {
            control
                .battle
                .simulation
                .attack(check.setup.attacker, check.setup.bus)?;
            control.time.unpause();
            check.phase = if check.phase == 1 { 2 } else { 6 };
        }
        let game = &control.battle.simulation;
        if check.phase < 5 && !game.entities[check.setup.bus].alive() {
            return Err(format!(
                "Battle Bus died before native second-life capture: {}",
                context(game, &check.setup)
            )
            .into());
        }
        let focus = if check.phase >= 5 {
            game.entities
                .iter()
                .position(|actor| game.catalog[actor.type_index].name == "DeadBattleBusHulk")
                .unwrap_or(check.setup.bus)
        } else {
            check.setup.bus
        };
        let position = game.object_position(focus);
        capture_framing::focus_on(&mut control.camera, &control.terrain, position);
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Battle Bus native phase {}: {error}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: ResMut<'w, Match>,
    framing: AircraftFrame<'w, 's>,
    camera: Res<'w, CameraRig>,
    time: ResMut<'w, Time<Virtual>>,
    clock: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if check.phase == 255 || !view.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let bus = check.setup.bus;
        let visible = if check.phase == 6 {
            let game = &view.battle.simulation;
            let Some(hulk) = game.entities.iter().position(|actor| {
                game.catalog[actor.type_index].name == "DeadBattleBusHulk"
                    && actor.physical_height().is_some()
            }) else {
                return Ok(());
            };
            hulk
        } else {
            bus
        };
        if !view.framing.visible(&view.battle, visible)? {
            held_visibility_failure(&mut check, &mut view)?;
            return Ok(());
        }
        if check.phase == 0 {
            check.camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("battle-bus-loaded.png");
            check.phase = 1;
        } else if check.phase == 2
            && check.attacker_retreat_started
            && view.battle.simulation.entities[bus].battle_bus_second_life()
            && view.battle.simulation.height_above_ground(bus) > 1.0
        {
            println!(
                "Battle Bus native phase 2 -> 3 visible=true: {}",
                context(&view.battle.simulation, &check.setup)
            );
            view.time.pause();
            check.camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("battle-bus-thrown.png");
            check.phase = 3;
        } else if check.phase == 3 {
            println!(
                "Battle Bus native phase 3 -> 4 visible=true: {}",
                context(&view.battle.simulation, &check.setup)
            );
            view.time.unpause();
            check.phase = 4;
        } else if check.phase == 4 && view.battle.simulation.entities[bus].battle_bus_grounded() {
            let game = &view.battle.simulation;
            println!(
                "Battle Bus native phase 4 -> 5 visible=true: {}",
                context(game, &check.setup)
            );
            require(
                check.setup.riders.iter().all(|&id| {
                    game.entities[id].alive() && game.entities[id].contained_by() == Some(bus)
                }),
                "native Battle Bus fake death lost surviving infantry",
            )?;
            view.time.pause();
            check.camera = Some((view.camera.focus, view.camera.distance));
            view.capture.intermediate("battle-bus-held.png");
            check.phase = 5;
        } else if check.phase == 6 && !view.battle.simulation.entities[bus].alive() {
            view.time.pause();
            check.camera = Some((view.camera.focus, view.camera.distance));
            view.capture.finish_scenario();
            check.phase = 255;
            println!(
                "Battle Bus native capture: paid load, ordinary damage, source throw, held passengers and original final-death physical hulk passed"
            );
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!("Battle Bus native render phase {}: {error}", check.phase);
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}

fn context(game: &generals_sim::skirmish::Skirmish, setup: &Setup) -> String {
    let bus = &game.entities[setup.bus];
    let position = game.object_position(setup.bus);
    let above_ground = game.height_above_ground(setup.bus);
    format!(
        "frame={} bus={} health={} alive={} second_life={} held={} last_attacker={:?} position={position:?} ground_height={} above_ground={above_ground}; bus {}; attacker={} health={} position={:?} {}",
        game.tick,
        setup.bus,
        bus.health,
        bus.alive(),
        bus.battle_bus_second_life(),
        bus.battle_bus_grounded(),
        bus.last_attacker(),
        position[2] - above_ground,
        generals_sim::skirmish::diagnostics::weapon_details(game, setup.bus, setup.attacker),
        setup.attacker,
        game.entities[setup.attacker].health,
        game.entities[setup.attacker].position,
        generals_sim::skirmish::diagnostics::weapon_details(game, setup.attacker, setup.bus),
    )
}

fn held_visibility_failure(
    check: &mut Check,
    view: &mut View<'_, '_>,
) -> Result<(), Box<dyn Error>> {
    let game = &view.battle.simulation;
    let id = check.setup.bus;
    let actor = &game.entities[id];
    if check.phase != 4 || !actor.battle_bus_grounded() || check.diagnostic {
        return Ok(());
    }
    let began = check
        .held_since
        .get_or_insert_with(|| view.clock.elapsed_secs_f64());
    if view.clock.elapsed_secs_f64() - *began < 5.0 {
        return Ok(());
    }
    check.diagnostic = true;
    println!(
        "Battle Bus held D1 visibility diagnostic: phase={} frame={} id={id} template={} expected_model=UVBattBus_D1 health={} alive={} held={} masked={} world={:?} flags={:?}; {}",
        check.phase,
        game.tick,
        game.catalog[actor.type_index].name,
        actor.health,
        actor.alive(),
        actor.battle_bus_grounded(),
        actor.masked(),
        game.object_position(id),
        game.model_flags(actor, [0.5, 0.1]),
        view.framing.visibility_diagnostic(&view.battle, id)?
    );
    // Freeze the current failed pose for the actual framebuffer capture. The
    // PreUpdate deadline uses Time<Real>, so this cannot suspend its timeout.
    view.time.pause();
    check.camera = Some((view.camera.focus, view.camera.distance));
    view.capture
        .intermediate("battle-bus-held-visibility-diagnostic.png");
    Ok(())
}
