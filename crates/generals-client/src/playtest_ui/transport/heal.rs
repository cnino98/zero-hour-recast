//! Original barracks entry, enclosed healing and automatic authored exit.
use crate::{
    input::{self, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_view::{Capture, UnitVisual},
    skirmish_verification::require,
};
use bevy::{app::AppExit, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    host: usize,
    rider: usize,
    stage: Stage,
    entered: Option<u64>,
}

enum Stage {
    Entry,
    Command,
    Approach,
    Held,
    Exit,
    Outside,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (host, rider) = crate::transport_verification::heal::prepare(&mut battle.simulation)?;
    require(
        battle.simulation.can_enter(rider, host),
        "rifle-injured Ranger cannot enter healing barracks",
    )?;
    battle.select_objects(&[rider], true);
    app.insert_resource(Check {
        host,
        rider,
        stage: Stage::Entry,
        entered: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
    Ok(())
}

pub fn frame_camera(app: &mut App) {
    let check = app.world().resource::<Check>();
    let battle = app.world().resource::<Match>();
    let host = Vec2::from_array(battle.simulation.entities[check.host].position);
    let rider = Vec2::from_array(battle.simulation.entities[check.rider].position);
    app.world_mut().resource_mut::<CameraRig>().focus = (host + rider) * 0.05;
}

#[derive(bevy::ecs::system::SystemParam)]
struct Control<'w> {
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    real_time: Res<'w, Time<Real>>,
    mouse: ResMut<'w, Mouse>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(
    mut battle: ResMut<Match>,
    visuals: Query<(&UnitVisual, &Visibility)>,
    mut check: ResMut<Check>,
    mut control: Control,
) {
    if matches!(check.stage, Stage::Done) {
        return;
    }
    if control.real_time.elapsed_secs() > 60.0 {
        check.stage = Stage::Done;
        error!("Barracks healing rendered acceptance timed out");
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    let present = |id| {
        visuals
            .iter()
            .find(|(visual, _)| visual.index == id && visual.generation == battle.generation)
            .map(|(_, visibility)| *visibility)
    };
    let host = present(check.host);
    let rider = present(check.rider);
    if host.is_none() || rider.is_none() {
        return;
    }
    let result = require(
        host == Some(Visibility::Visible),
        "paid original barracks drawable is hidden",
    )
    .and_then(|()| advance_stage(&mut battle, &mut check, rider, &mut control));
    if let Err(error) = result {
        check.stage = Stage::Done;
        error!("Barracks healing rendered acceptance: {error}");
        control.exit.write(AppExit::error());
    }
}

fn advance_stage(
    battle: &mut Match,
    check: &mut Check,
    rider: Option<Visibility>,
    control: &mut Control,
) -> Result<(), Box<dyn Error>> {
    match check.stage {
        Stage::Entry => {
            require(
                rider == Some(Visibility::Visible),
                "injured Ranger entry drawable is hidden",
            )?;
            control.time.pause();
            control.capture.intermediate("barracks-healing-entry.png");
            check.stage = Stage::Command;
        }
        Stage::Command => {
            control.mouse.hovered = Some(check.host);
            require(
                crate::attack_cursor::context(battle, &control.mouse) == Some(1),
                "injured Ranger lacks original Enter cursor",
            )?;
            require(
                input::context_order(
                    battle,
                    None,
                    Some(PickTarget::Unit(check.host)),
                    &mut control.mouse,
                ),
                "original context command did not enter healing barracks",
            )?;
            control.time.unpause();
            check.stage = Stage::Approach;
        }
        Stage::Approach => {
            if battle.simulation.entities[check.rider].contained_by() == Some(check.host)
                && rider == Some(Visibility::Hidden)
            {
                require(
                    !battle.simulation.entities[check.rider].selectable(
                        &battle.simulation.catalog
                            [battle.simulation.entities[check.rider].type_index],
                    ),
                    "healing Ranger remains selectable",
                )?;
                check.entered = Some(battle.simulation.tick);
                control.time.pause();
                control
                    .capture
                    .intermediate("barracks-healing-contained.png");
                check.stage = Stage::Held;
            }
        }
        Stage::Held => {
            control.time.unpause();
            check.stage = Stage::Exit;
        }
        Stage::Exit => {
            if battle.simulation.entities[check.rider]
                .contained_by()
                .is_none()
                && rider == Some(Visibility::Visible)
            {
                let actor = &battle.simulation.entities[check.rider];
                require(
                    actor.alive() && actor.health.to_bits() == actor.max_health.to_bits(),
                    "automatically exited Ranger lacks full health",
                )?;
                require(
                    battle.simulation.tick - check.entered.ok_or("healing entry frame missing")?
                        <= 60,
                    "barracks automatic exit exceeded two-second source interval",
                )?;
                check.stage = Stage::Outside;
            }
        }
        Stage::Outside => {
            let actor = &battle.simulation.entities[check.rider];
            if rider == Some(Visibility::Visible)
                && !actor.moving()
                && matches!(actor.ai.order, generals_sim::skirmish::Order::Idle)
            {
                control.time.pause();
                control.capture.finish_scenario();
                check.stage = Stage::Done;
                println!(
                    "Rendered barracks healing: paid original Barracks/Ranger, injured Enter cursor/context command, hidden unselectable healing interval and visible full-health automatic exit passed"
                );
            }
        }
        Stage::Done => {}
    }
    Ok(())
}
