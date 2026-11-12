//! Original Avenger pulse geometry and sound output during ordinary paid combat.
use crate::{
    input::{self, Mouse, PickTarget},
    laser_beam::PulseInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    point_defense_verification,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
};
use bevy::prelude::*;
use std::error::Error;

#[derive(Resource)]
struct Check {
    actor: usize,
    turret: usize,
    target: usize,
    air: bool,
    began: u64,
    started: bool,
    done: bool,
    beams: [bool; 3],
    sounds: [bool; 2],
}

pub fn prepare(app: &mut App, battle: &mut Match, air: bool) -> Result<(), Box<dyn Error>> {
    let (setup, target) = if air {
        point_defense_verification::prepare_air(&mut battle.simulation)?
    } else {
        point_defense_verification::prepare_missile(&mut battle.simulation)?
    };
    battle.select_objects(&[setup.avenger], true);
    app.insert_resource(Check {
        actor: setup.avenger,
        turret: setup.turret,
        target,
        air,
        began: battle.simulation.tick,
        started: false,
        done: false,
        beams: [false; 3],
        sounds: [false; 2],
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let actor = Vec2::from_array(game.entities[check.actor].position);
    let target = Vec2::from_array(game.entities[check.target].position);
    let air_frame = check.air.then(|| {
        (
            (target - actor).to_angle(),
            game.height_above_ground(check.target) * 0.05,
        )
    });
    let mut camera = app.world_mut().resource_mut::<CameraRig>();
    camera.focus = (actor + target) * 0.05;
    if let Some((yaw, height)) = air_frame {
        // Frame ground separation horizontally so both altitude and hull clear the HUD.
        camera.settings.yaw = yaw;
        let (sine, cosine) = yaw.sin_cos();
        let offset = height / camera.settings.pitch.tan();
        camera.focus -= Vec2::new(sine, -cosine) * offset;
    }
    app.add_systems(PreUpdate, (inspect_audio, begin).chain());
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn inspect_audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    if !check.started || check.done {
        return;
    }
    let played = if check.air {
        audio.played([
            (check.turret, "AvengerAirLaserWeapon"),
            (check.actor, "AvengerVoiceAttack"),
        ])
    } else {
        // The point-defense pulse is a Sound nugget in FireFX, played at the firing position.
        audio.played_events([
            (None, "PaladinPointDefenseLaserPulse"),
            (Some(check.actor), "AvengerPaintWeaponLoop"),
        ])
    };
    for (heard, played) in check.sounds.iter_mut().zip(played) {
        *heard |= played;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    time: ResMut<'w, Time<Virtual>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    pulses: PulseInspection<'w, 's>,
    battle: Res<'w, Match>,
}

// PulseInspection reads Match, so command mutation is a separate system phase.
fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut capture: ResMut<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done || !capture.ready_for_scenario() {
        return;
    }
    if !check.started {
        return;
    }
    let result = inspect_beams(&mut check, &view.pulses, view.battle.into_inner());
    match result {
        Ok(true) => {
            check.done = true;
            time.pause();
            capture.finish_scenario();
            println!(
                "Original Avenger {}: visible layered EXLaser4 pulses and advancing original sound output passed",
                if check.air {
                    "anti-air"
                } else {
                    "point defense and target painting"
                }
            );
        }
        Ok(false) => {}
        Err(error) => {
            check.done = true;
            error!("Avenger native acceptance: {error}");
            exit.write(AppExit::error());
        }
    }
}

fn inspect_beams(
    check: &mut Check,
    pulses: &PulseInspection,
    battle: &Match,
) -> Result<bool, Box<dyn Error>> {
    let weapons = if check.air {
        ["AvengerAirLaserOne", "AvengerAirLaserTwo", ""]
    } else {
        [
            "AvengerPointDefenseLaserOne",
            "AvengerPointDefenseLaserTwo",
            "AvengerTargetDesignator",
        ]
    };
    if let Some(index) = check.beams.iter().position(|seen| !seen) {
        let weapon = weapons[index];
        check.beams[index] = weapon.is_empty()
            || pulses.visible(if check.air { check.turret } else { check.actor }, weapon)?;
    }
    if check.beams.into_iter().all(|seen| seen) && check.sounds.into_iter().all(|heard| heard) {
        return Ok(true);
    }
    if battle.simulation.tick - check.began >= 1800 {
        return Err(format!(
            "timed out: beams={:?} sounds={:?}",
            check.beams, check.sounds
        )
        .into());
    }
    Ok(false)
}

fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.started || !control.capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        require(
            input::context_order(
                &mut control.battle,
                None,
                Some(PickTarget::Unit(check.target)),
                &mut control.mouse,
            ),
            "original Avenger context attack rejected",
        )?;
        if check.air {
            require(
                control
                    .mouse
                    .voices
                    .last()
                    .is_some_and(|request| matches!(request.voice(), Voice::AttackAir)),
                "airborne locomotor did not select original VoiceAttackAir",
            )?;
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        check.done = true;
        error!("Avenger native command: {error}");
        control.exit.write(AppExit::error());
        return;
    }
    check.started = true;
    check.began = control.battle.simulation.tick;
    control.time.unpause();
}
