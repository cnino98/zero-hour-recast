//! Paid Outpost inventory, cloak, detector, revealed route and passenger combat.
use crate::{
    input::{self, Mouse, PickTarget},
    playtest::{CameraRig, Match},
    playtest_ui::dispatch_command,
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::{FiringAudioInspection, Voice},
};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use generals_sim::skirmish::stealth;
use std::error::Error;
mod fixture;
mod stages;
#[cfg(test)]
mod tests;
mod view;
use fixture::Setup;
use view::View;

#[derive(Resource)]
struct Check {
    setup: Setup,
    stage: u8,
    began: Option<f64>,
    pointer: Option<Vec2>,
    scan: bool,
    audio: [bool; 2],
    shots: u32,
    death: Option<u64>,
    final_death: Option<u64>,
    animation: Option<f32>,
    animated: bool,
    move_goal: Option<[f32; 2]>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = fixture::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.outpost], true);
    app.insert_resource(Check {
        setup,
        stage: 0,
        began: None,
        pointer: None,
        scan: false,
        audio: [false; 2],
        shots: 0,
        death: None,
        final_death: None,
        animation: None,
        animated: false,
        move_goal: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let host = Vec2::from_array(game.entities[check.setup.outpost].position);
    let target = Vec2::from_array(game.entities[check.setup.enemies[0]].position);
    app.world_mut().resource_mut::<CameraRig>().focus = (host + target) * 0.05;
    app.add_systems(PreUpdate, (pointer, audio));
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::ui::UiSystems::Layout)
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn pointer(check: Res<Check>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let check = check.into_inner();
    if let Some(point) = check.pointer {
        window.set_cursor_position(Some(point));
    }
}

fn audio(mut check: ResMut<Check>, inspection: FiringAudioInspection) {
    let host = check.setup.outpost;
    let heard = inspection.played([
        (host, "ListeningOutpostVoiceMove"),
        (check.setup.riders[0], "TankHunterWeapon"),
        (check.setup.riders[1], "TankHunterWeapon"),
    ]);
    check.audio[0] |= heard[0];
    check.audio[1] |= heard[1] || heard[2];
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let game = &battle.into_inner().simulation;
    let host = check.setup.outpost;
    check.scan |= game.presentation.stealth_events.iter().any(|event| {
        matches!(event, stealth::Event::Scan { detector, feedback: Some(_), .. } if *detector == host)
    });
    let shots = u32::try_from(
        game.presentation
            .shots
            .iter()
            .filter(|shot| {
                check.setup.riders.contains(&shot.source)
                    && shot
                        .target
                        .is_some_and(|target| check.setup.enemies.contains(&target))
            })
            .count(),
    )
    .expect("frame shot count");
    check.shots += shots;
    if !game.entities[host].alive() && check.death.is_none() {
        check.death = Some(game.tick);
    }
    if check.death.is_some() && !game.entities[host].corpse_present() {
        check.final_death.get_or_insert(game.tick);
    }
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    mouse: ResMut<'w, Mouse>,
    clock: ResMut<'w, Time<Virtual>>,
    real: Res<'w, Time<Real>>,
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, view: View, mut control: Control) {
    if check.stage == 255 || !control.capture.ready_for_scenario() {
        return;
    }
    let began = *check
        .began
        .get_or_insert_with(|| control.real.elapsed_secs_f64());
    let result = if control.real.elapsed_secs_f64() - began > 180.0 {
        let game = &control.battle.simulation;
        let enemy = check.setup.enemies[0];
        Err(format!(
            "Outpost native timeout: stage={} scan={} audio={:?} shots={} death={:?}/{:?}; selection={:?}, pointer={:?}, hovered={:?}, route={}, enemy={} at {:?}, goal={:?}; {}",
            check.stage,
            check.scan,
            check.audio,
            check.shots,
            check.death,
            check.final_death,
            control.battle.selected,
            check.pointer,
            control.mouse.hovered,
            view.route(),
            enemy,
            game.object_position(enemy),
            game.entities[enemy].ai_goal_position(),
            generals_sim::skirmish::diagnostics::movement_details(game, check.setup.outpost)
        )
        .into())
    } else {
        view.progress(&mut check, &mut control)
    };
    if let Err(error) = result {
        error!("Outpost native acceptance: {error}");
        check.stage = 255;
        control.exit.write(AppExit::error());
    }
}

fn enter(control: &mut Control, riders: &[usize], host: usize) -> Result<(), Box<dyn Error>> {
    control.battle.select_objects(riders, true);
    require(
        input::context_order(
            &mut control.battle,
            None,
            Some(PickTarget::Unit(host)),
            &mut control.mouse,
        ),
        "Outpost original context Enter rejected",
    )?;
    control.battle.select_objects(&[host], true);
    control.clock.unpause();
    Ok(())
}

fn attack(control: &mut Control, target: usize) -> Result<(), Box<dyn Error>> {
    let point = Vec2::from_array(control.battle.simulation.entities[target].position);
    require(
        input::context_order(
            &mut control.battle,
            Some(point),
            Some(PickTarget::Unit(target)),
            &mut control.mouse,
        ),
        "Outpost original context Attack rejected",
    )?;
    require(
        control
            .mouse
            .voices
            .last()
            .is_some_and(|request| matches!(request.voice(), Voice::Attack)),
        "Outpost Attack did not request original speech",
    )?;
    control.clock.unpause();
    Ok(())
}
