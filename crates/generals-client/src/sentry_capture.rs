//! Paid Sentry research, original deployment, concealment and actual gun effects.
mod commands;
#[cfg(test)]
mod tests;
mod view;

use crate::{
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    sentry_verification,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;
use view::View;

#[derive(Resource)]
struct Check {
    setup: sentry_verification::Setup,
    stage: Stage,
    began: Option<f64>,
    research: u64,
    transitions: [Option<u64>; 4],
    next: [f32; 2],
    moved: bool,
    animation: Option<f32>,
    animated: bool,
    sounds: [bool; 2],
    shots: Vec<Shot>,
    target_health: f32,
}

enum Stage {
    Cloak,
    FactorySelection,
    ResearchSelection,
    Research,
    DroneSelection,
    Deploy,
    DeployedCaptured,
    Move,
    Approach,
    Fire,
    Done,
}

struct Shot {
    frame: u64,
    weapon: usize,
    veterancy: u8,
    position: [f32; 3],
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = sentry_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.drone], true);
    app.insert_resource(Check {
        target_health: battle.simulation.entities[setup.target].health,
        setup,
        stage: Stage::Cloak,
        began: None,
        research: 0,
        transitions: [None; 4],
        next: [0.0; 2],
        moved: false,
        animation: None,
        animated: false,
        sounds: [false; 2],
        shots: Vec::new(),
    });
    // Allow native StealthUpdate events to reach the client cloak renderer.
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let point = Vec2::from_array(game.entities[check.setup.drone].position) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = point;
    register_systems(app);
}

fn register_systems(app: &mut App) {
    app.add_systems(PreUpdate, audio);
    app.add_systems(FixedUpdate, record.after(crate::laser_beam::record));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    let id = check.setup.drone;
    for (heard, playing) in check
        .sounds
        .iter_mut()
        .zip(audio.played([(id, "SentryDroneVoiceMove"), (id, "SentryDroneWeapon")]))
    {
        *heard |= playing;
    }
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let battle = battle.into_inner();
    let game = &battle.simulation;
    let id = check.setup.drone;
    let condition = game.entities[id].deployment_condition();
    let transition = match (&check.stage, condition) {
        (Stage::Deploy, Some("UNPACKING")) => Some(0),
        (Stage::Deploy, Some("DEPLOYED")) => Some(1),
        (Stage::Move, Some("PACKING")) => Some(2),
        (Stage::Move, None) if check.transitions[2].is_some() => Some(3),
        _ => None,
    };
    if let Some(index) = transition {
        check.transitions[index].get_or_insert(game.tick);
    }
    if matches!(check.stage, Stage::Fire) {
        let target = check.setup.target;
        check
            .shots
            .extend(game.presentation.shots.iter().filter_map(|shot| {
                (shot.source == id && shot.target == Some(target) && shot.weapon_slot == 0)
                    .then_some(Shot {
                        frame: game.tick,
                        weapon: shot.weapon_index,
                        veterancy: shot.veterancy,
                        position: shot.source_position,
                    })
            }));
    }
}

#[derive(SystemParam)]
struct Control<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    camera: ResMut<'w, CameraRig>,
    time: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    commands: Commands<'w, 's>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if matches!(check.stage, Stage::Done) {
        return;
    }
    let now = control.real.elapsed_secs_f64();
    let began = *check.began.get_or_insert(now);
    let result = require(
        now - began < 180.0,
        "native Sentry acceptance exceeded 180 seconds",
    )
    .and_then(|()| {
        if control.capture.ready_for_scenario() {
            view.progress(&mut check, &mut control)?;
        }
        Ok(())
    });
    if let Err(error) = result {
        let game = &control.battle.simulation;
        error!(
            "Native Sentry: {error}; research={} transitions={:?} moved={} animated={} audio={:?} shots={}; drone={}; target={}",
            game.has_upgrade(0, "Upgrade_AmericaSentryDroneGun"),
            check.transitions,
            check.moved,
            check.animated,
            check.sounds,
            check.shots.len(),
            generals_sim::skirmish::diagnostics::movement_details(game, check.setup.drone),
            generals_sim::skirmish::diagnostics::movement_details(game, check.setup.target)
        );
        check.stage = Stage::Done;
        control.exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod schedule_tests {
    #[test]
    fn actual_capture_graph_initializes_after_current_tick_effects() {
        crate::playtest::schedule_tests::assert_capture_graph(
            super::register_systems,
            "sentry_capture::record",
        );
    }
}
