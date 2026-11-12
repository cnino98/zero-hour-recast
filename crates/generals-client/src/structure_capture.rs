//! Paid attacks against original map structures; no fixture health or pose writes.
use crate::{
    capture_framing,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;
#[cfg(test)]
mod camera_tests;
mod setup;
mod view;

#[derive(Resource)]
struct Check {
    setup: setup::Setup,
    phase: u8,
    started: Option<f64>,
    dead_frame: Option<u64>,
    attacker: Option<usize>,
    audio: bool,
    particles: bool,
    bone: bool,
    effect: Option<(String, u64)>,
    falling_frames: u64,
    visible_falling_frames: u64,
    effect_falling_frames: u64,
}

pub const fn map_name(topple: bool) -> &'static str {
    if topple {
        "Maps\\Cairo Commandos\\Cairo Commandos.map"
    } else {
        "Maps\\Golden Oasis\\Golden Oasis.map"
    }
}

pub fn prepare(app: &mut App, battle: &mut Match, topple: bool) -> Result<(), Box<dyn Error>> {
    let setup = setup::prepare(&mut battle.simulation, topple)?;
    battle.started = true;
    battle.selected.clone_from(&setup.tanks);
    app.insert_resource(Check {
        setup,
        phase: 0,
        started: None,
        dead_frame: None,
        attacker: None,
        audio: false,
        particles: false,
        bone: false,
        effect: None,
        falling_frames: 0,
        visible_falling_frames: 0,
        effect_falling_frames: 0,
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
        view::inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
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
    if now - *check.started.get_or_insert(now) > 180.0 {
        error!(
            "Structure capture timeout phase={} dead={:?} audio={} particles={} bone={}",
            check.phase, check.dead_frame, check.audio, check.particles, check.bone
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if !control.capture.ready_for_scenario() || !matches!(check.phase, 0 | 2) {
        return;
    }
    if check.phase == 0 {
        // Ordinary player force attack permits these unflagged neutral buildings.
        for &tank in &check.setup.tanks {
            if let Err(error) = control
                .battle
                .simulation
                .force_attack(tank, check.setup.target)
            {
                error!("Paid structure force-attack rejected: {error}");
                check.phase = 255;
                control.exit.write(AppExit::error());
                return;
            }
        }
    }
    check.phase += 1;
    control.time.unpause();
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let battle = battle.into_inner();
    if !matches!(check.phase, 1 | 3) {
        return;
    }
    let game = &battle.simulation;
    if check.dead_frame.is_none() && !game.entities[check.setup.target].alive() {
        check.attacker = game.entities[check.setup.target].last_attacker();
        check.dead_frame = Some(game.tick);
    }
    for event in &game.presentation.structure_events {
        if event.object == check.setup.target
            && matches!(
                event.action,
                generals_sim::skirmish::structure_death::VisualAction::PositionFx
            )
            && let Some(effect) = &event.effect
        {
            check.effect = Some((effect.effect.clone(), event.frame));
        }
    }
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    structures: Res<crate::structure_view::Structures>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let structures = structures.into_inner();
    camera.distance = camera.settings.max_height;
    let mut center = battle.simulation.object_position(check.setup.target);
    let height_offset = check.setup.height * 0.3;
    center[2] += height_offset;
    if !check.setup.topple {
        // Collapse translates only Drawable's instance; its collision position stays fixed.
        center[2] += structures.offset(battle.generation, check.setup.target)[2];
    }
    capture_framing::focus_on(&mut camera, terrain.into_inner(), center);
}
