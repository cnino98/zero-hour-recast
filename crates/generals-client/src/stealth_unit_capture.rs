//! Paid Scout detection and combat-earned Pathfinder native acceptance.
use crate::{
    input::{self, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;
mod pathfinder;
mod scout;
#[cfg(test)]
mod tests;
mod view;
use view::View;

#[derive(Resource)]
struct Check {
    actor: usize,
    target: usize,
    scout: bool,
    phase: u8,
    sounds: [bool; 3],
    speech_finished: bool,
    animation: Animation,
    shots: u32,
    credit: [u32; 2],
    experience: i32,
}

struct Animation {
    previous: Option<f32>,
    advanced: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match, scout: bool) -> Result<(), Box<dyn Error>> {
    let (actor, target) = if scout {
        crate::infiltrator_verification::prepare_scout(&mut battle.simulation)?
    } else {
        crate::progression_verification::pathfinder::prepare_capture(&mut battle.simulation)?
    };
    let game = &battle.simulation;
    let credit = [
        game.players[0].statistics.units_destroyed,
        game.players[1].statistics.units_lost,
    ];
    let experience = game.entities[actor].experience;
    battle.select_objects(&[actor], true);
    app.insert_resource(Check {
        actor,
        target,
        scout,
        phase: 0,
        sounds: [false; 3],
        speech_finished: false,
        animation: Animation {
            previous: None,
            advanced: false,
        },
        shots: 0,
        credit,
        experience,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let focus = Vec2::from_array(game.entities[check.actor].position) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    register_systems(app);
}

fn register_systems(app: &mut App) {
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(FixedUpdate, record_shots.after(crate::laser_beam::record));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if check.phase == 255 {
        return;
    }
    let names = if check.scout {
        ["ScoutDroneVoiceCreate", "HumveeVoiceMove", ""]
    } else {
        [
            "PathfinderVoiceMove",
            "PathfinderVoiceAttack",
            "PathfinderWeapon",
        ]
    };
    check.speech_finished = check.sounds[0] && audio.finished(check.actor, names[0]);
    let played = audio.played(names.map(|name| (check.actor, name)));
    for (heard, now) in check.sounds.iter_mut().zip(played) {
        *heard |= now;
    }
}

fn record_shots(mut check: ResMut<Check>, battle: Res<Match>) {
    if check.scout || check.phase != 3 {
        return;
    }
    let battle = battle.into_inner();
    check.shots += u32::try_from(
        battle
            .simulation
            .presentation
            .shots
            .iter()
            .filter(|shot| {
                shot.source == check.actor
                    && shot.target == Some(check.target)
                    && shot.weapon_slot == 0
            })
            .count(),
    )
    .expect("frame shot count fits u32");
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    camera: ResMut<'w, CameraRig>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    if control.real.elapsed_secs() > 180.0 {
        if check.scout
            && let Some(child) = control.battle.simulation.produced_drone(check.actor)
        {
            error!("{}", view.scout_state(&control.battle, child, check.target));
        }
        error!(
            "Stealth unit capture timeout: scout={} phase={} audio={:?} animation={} shots={} actor={} target={}",
            check.scout,
            check.phase,
            check.sounds,
            check.animation.advanced,
            check.shots,
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.actor
            ),
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.target
            )
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    let result = view.progress(&mut check, &mut control);
    if let Err(error) = result {
        error!("Stealth unit native phase {}: {error}", check.phase);
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}

fn move_order(control: &mut Control, point: [f32; 2]) -> Result<(), Box<dyn Error>> {
    require(
        input::context_order(
            &mut control.battle,
            Some(Vec2::from_array(point)),
            None,
            &mut control.mouse,
        ),
        "original context Move was rejected",
    )?;
    require(
        control
            .mouse
            .voices
            .last()
            .is_some_and(|request| matches!(request.voice(), Voice::Move)),
        "original Move did not request its authored voice",
    )?;
    control.clock.unpause();
    Ok(())
}

fn attack_order(control: &mut Control, target: usize) -> Result<(), Box<dyn Error>> {
    let point = Vec2::from_array(control.battle.simulation.entities[target].position);
    require(
        input::context_order(
            &mut control.battle,
            Some(point),
            Some(PickTarget::Unit(target)),
            &mut control.mouse,
        ),
        "original context Attack was rejected",
    )?;
    require(
        control
            .mouse
            .voices
            .last()
            .is_some_and(|request| matches!(request.voice(), Voice::Attack)),
        "original Attack did not request its authored voice",
    )?;
    control.clock.unpause();
    Ok(())
}

fn finish(check: &mut Check, control: &mut Control) {
    check.phase = 255;
    control.clock.pause();
    control.capture.finish_scenario();
}

#[cfg(test)]
mod schedule_tests {
    #[test]
    fn actual_capture_graph_initializes_after_current_tick_effects() {
        crate::playtest::schedule_tests::assert_capture_graph(
            super::register_systems,
            "stealth_unit_capture::record_shots",
        );
    }
}
