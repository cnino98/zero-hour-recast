//! Original paid Jarmen controls, pilot damage, infantry takeover and rifle.
use crate::{
    control_bar::CommandAction,
    input::{self, CommandMode, Mouse, PickTarget},
    jarmen_verification::{self, Setup},
    playtest::{CameraRig, Match},
    playtest_ui::dispatch_command,
    playtest_view::Capture,
    skirmish_verification::{advance_until, require},
    source_audio::FiringAudioInspection,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;
mod animation;
mod stages;
#[cfg(test)]
mod tests;
mod view;
use view::View;

struct Heard {
    sounds: [bool; 6],
    mode_finished: bool,
    pilot_finished: bool,
}

#[derive(Resource)]
struct Check {
    setup: Setup,
    stage: u8,
    audio: Heard,
    shots: [u32; 2],
    pilot_event: bool,
    vehicle_health: u32,
    losses: [u32; 2],
    kills: u32,
    animation: animation::Proof,
    held_overstep: Option<std::time::Duration>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = prepare_targets(&mut battle.simulation)?;
    let game = &battle.simulation;
    let vehicle_health = game.entities[setup.vehicle].health.to_bits();
    let losses = [0, 1].map(|owner| game.players[owner].statistics.units_lost);
    let kills = game.players[0].statistics.units_destroyed;
    battle.select_objects(&[setup.sniper], true);
    app.insert_resource(Check {
        setup,
        stage: 0,
        audio: Heard {
            sounds: [false; 6],
            mode_finished: false,
            pilot_finished: false,
        },
        shots: [0; 2],
        pilot_event: false,
        vehicle_health,
        losses,
        kills,
        animation: animation::Proof::new(),
        held_overstep: None,
    });
    // Native cloak opacity is delivered by the next simulation updates.
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    Ok(())
}

fn prepare_targets(game: &mut generals_sim::skirmish::Skirmish) -> Result<Setup, Box<dyn Error>> {
    let setup = jarmen_verification::prepare(game)?;
    let point = game.entities[setup.sniper].position;
    game.move_to(setup.infantry, point)?;
    advance_until(
        game,
        18_000,
        "paid infantry approaching Jarmen's view",
        |game| {
            let target = game.entities[setup.infantry].position;
            (target[0] - point[0]).hypot(target[1] - point[1]) < 275.0
        },
    )?;
    game.stop(setup.infantry)?;
    require(
        game.entities[setup.infantry].alive() && game.entities[setup.sniper].stealthed(),
        "native Jarmen preparation lost its live target or stationary concealment",
    )?;
    Ok(setup)
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let focus = Vec2::from_array(game.entities[check.setup.sniper].position) * 0.1;
    app.world_mut().resource_mut::<CameraRig>().focus = focus;
    register_systems(app);
}

fn register_systems(app: &mut App) {
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(FixedUpdate, record.after(crate::laser_beam::record));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(
    mut check: ResMut<Check>,
    audio: FiringAudioInspection,
    world_audio: FiringAudioInspection,
) {
    let actor = check.setup.sniper;
    check.audio.mode_finished =
        check.audio.sounds[0] && audio.finished(actor, "JarmenKellVoiceModeSnipe");
    check.audio.pilot_finished =
        check.audio.sounds[1] && audio.finished(actor, "JarmenKellVoiceSnipe");
    let heard = audio.played([
        (actor, "JarmenKellVoiceModeSnipe"),
        (actor, "JarmenKellVoiceSnipe"),
        (actor, "JarmenKellWeaponSnipe"),
        (actor, "JarmenKellVoiceAttack"),
        (actor, "JarmenKellWeapon"),
    ]);
    for (before, now) in check.audio.sounds[..5].iter_mut().zip(heard) {
        *before |= now;
    }
    check.audio.sounds[5] |= world_audio.object_fx("VehicleSnipeVictim");
}

fn record(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    mut clock: ResMut<Time<Virtual>>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    let game = &battle.into_inner().simulation;
    for shot in &game.presentation.shots {
        if shot.source == check.setup.sniper {
            if shot.weapon_slot == 1 && shot.target == Some(check.setup.vehicle) {
                check.shots[1] += 1;
            } else if shot.weapon_slot == 0 && shot.target == Some(check.setup.infantry) {
                check.shots[0] += 1;
                if check.stage == 7 && check.held_overstep.is_none() {
                    // A slow rendered frame may otherwise run past this genuine
                    // one-tick firing state. Retain all unconsumed time for resume.
                    clock.pause();
                    let remaining = fixed.overstep();
                    fixed.discard_overstep(remaining);
                    check.held_overstep = Some(remaining);
                }
            }
        }
    }
    check.pilot_event |= game
        .presentation
        .pilot_events
        .iter()
        .any(|event| event.object == check.setup.vehicle);
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    fixed: ResMut<'w, Time<Fixed>>,
    real: Res<'w, Time<Real>>,
    camera: ResMut<'w, CameraRig>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if check.stage == 255 {
        return;
    }
    if control.real.elapsed_secs() > 180.0 {
        error!(
            "Jarmen capture timeout: stage={} audio={:?} shots={:?} pilot={} moving_geometry={} rifle_pose={} sniper={} vehicle={} driver={}",
            check.stage,
            check.audio.sounds,
            check.shots,
            check.pilot_event,
            check.animation.movement,
            check.animation.shot,
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.setup.sniper
            ),
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.setup.vehicle
            ),
            generals_sim::skirmish::diagnostics::movement_details(
                &control.battle.simulation,
                check.setup.driver
            ),
        );
        check.stage = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.progress(&mut check, &mut control) {
        error!("Jarmen native stage {}: {error}", check.stage);
        check.stage = 255;
        control.exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod schedule_tests {
    #[test]
    fn actual_capture_graph_initializes_after_current_tick_effects() {
        crate::playtest::schedule_tests::assert_capture_graph(
            super::register_systems,
            "jarmen_capture::record",
        );
    }
}
