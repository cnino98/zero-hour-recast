//! Original context DISARM, paid Scout revelation, digging and clearing effects.
mod area;
mod stages;
#[cfg(test)]
mod tests;
mod view;

use crate::{
    disarm_verification::{self, Setup, Trace},
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use std::error::Error;
use view::View;

#[derive(Resource)]
struct Check {
    setup: Setup,
    trace: Trace,
    stage: Stage,
    began: Option<f64>,
    pointer: Option<Vec2>,
    animation: Option<f32>,
    animated: bool,
    audio: [bool; 2],
    prior_voice_retired: bool,
    failure: Option<String>,
    area: area::Progress,
}

enum Stage {
    Hover,
    ContextPending,
    Approach,
    Prime,
    Dig,
    DigCaptured,
    Clearing,
    ClearedCaptured,
    Reload,
    AreaMode,
    AreaPress,
    AreaRelease,
    AreaOrder,
    AreaPending,
    AreaRun,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = disarm_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.builder], true);
    app.insert_resource(Check {
        setup,
        trace: Trace::new(),
        stage: Stage::Hover,
        began: None,
        pointer: None,
        animation: None,
        animated: false,
        audio: [false; 2],
        prior_voice_retired: false,
        failure: None,
        area: area::Progress::new(),
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let actor = Vec2::from_array(game.entities[check.setup.builder].position);
    let target = Vec2::new(check.setup.position[0], check.setup.position[1]);
    app.world_mut().resource_mut::<CameraRig>().focus = (actor + target) * 0.05;
    app.add_systems(PreUpdate, (pointer, voice, clearing_sound));
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn pointer(check: Res<Check>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    if let Some(point) = check.into_inner().pointer {
        window.set_cursor_position(Some(point));
    }
}

fn voice(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    let event = "DozerUSAVoiceClearMine";
    let present = audio.present(check.setup.builder, event);
    let played = audio.played([(check.setup.builder, event)])[0];
    check.observe_voice(present, played);
}

impl Check {
    const fn observe_voice(&mut self, present: bool, played: bool) {
        match self.stage {
            Stage::AreaMode => self.prior_voice_retired = !present,
            Stage::AreaPress | Stage::AreaRelease => {}
            _ => self.audio[0] |= played,
        }
    }
}

fn clearing_sound(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    check.audio[1] |= audio.object_fx("MineClearedByDozer");
}

fn record(mut check: ResMut<Check>, battle: ResMut<Match>, mut time: ResMut<Time<Virtual>>) {
    if matches!(check.stage, Stage::Hover | Stage::Done) {
        return;
    }
    let game = &mut battle.into_inner().simulation;
    let check = &mut *check;
    let admitted = match check.stage {
        Stage::ContextPending => require(
            matches!(game.entities[check.setup.builder].ai.order,
                generals_sim::skirmish::Order::Attack(target) if target == check.setup.trap),
            "context attack omitted original disarm order",
        ),
        Stage::AreaPending => require(
            matches!(
                game.entities[check.setup.builder].ai.order,
                generals_sim::skirmish::Order::FireAt { .. }
            ),
            "real ground click did not execute the original area command",
        ),
        _ => Ok(()),
    };
    if let Err(error) = admitted {
        check.failure = Some(error.to_string());
        time.pause();
        return;
    }
    match check.stage {
        Stage::ContextPending => check.stage = Stage::Approach,
        Stage::AreaPending => check.stage = Stage::AreaRun,
        _ => {}
    }
    if matches!(
        check.stage,
        Stage::AreaMode | Stage::AreaPress | Stage::AreaRelease | Stage::AreaOrder | Stage::AreaRun
    ) {
        if let Err(error) = check.area.record(game, check.setup.builder) {
            check.failure = Some(error.to_string());
            time.pause();
        } else if check.area.finished {
            time.pause();
        }
        return;
    }
    if let Err(error) = check.trace.record(game, &check.setup) {
        check.failure = Some(error.to_string());
        time.pause();
        return;
    }
    match check.stage {
        Stage::Approach if check.trace.preattack.is_some() => {
            // Let the actual first digging draw initialize its source emitters
            // before advancing their original thirty-frame initial delay.
            check.stage = Stage::Prime;
            time.pause();
        }
        Stage::Dig
            if check
                .trace
                .preattack
                .is_some_and(|start| game.tick >= start + 32) =>
        {
            time.pause();
        }
        Stage::Clearing
            if check
                .trace
                .shot
                .as_ref()
                .is_some_and(|shot| game.tick >= shot.frame + 2) =>
        {
            time.pause();
        }
        Stage::Reload
            if check.trace.reloaded && game.mine_clearing_ready(check.setup.builder, 0) =>
        {
            time.pause();
        }
        _ => {}
    }
}

#[derive(SystemParam)]
struct Control<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    time: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    events: MessageWriter<'w, bevy::input::mouse::MouseButtonInput>,
    window: Single<'w, 's, Entity, With<PrimaryWindow>>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if matches!(check.stage, Stage::Done) {
        return;
    }
    let now = control.real.elapsed_secs_f64();
    let began = *check.began.get_or_insert(now);
    let result = require(
        now - began < 180.0,
        "native DISARM acceptance exceeded 180 seconds",
    )
    .and_then(|()| {
        if let Some(error) = check.failure.take() {
            return Err(error.into());
        }
        if control.capture.ready_for_scenario() {
            view.progress(&mut check, &mut control)?;
        }
        Ok(())
    });
    if let Err(error) = result {
        error!(
            "Native DISARM: {error}; pre={:?} shot={:?} reload={} animated={} audio={:?}; {}",
            check.trace.preattack,
            check.trace.shot.as_ref().map(|shot| shot.frame),
            check.trace.reloaded,
            check.animated,
            check.audio,
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.setup.builder
            )
        );
        check.stage = Stage::Done;
        control.exit.write(AppExit::error());
    }
}
