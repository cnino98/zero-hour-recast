//! Paid original `DamageTankStruck` model and terrain `LightPulse` observation.
mod inspection;
mod setup;
#[cfg(test)]
mod tests;

use crate::{
    capture_framing::{self, AircraftFrame},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

enum Phase {
    Baseline,
    BaselineSaved,
    Attack,
    Impact,
    ActiveSaved,
    Done,
}

struct Impact {
    tick: u64,
    position: [f32; 3],
}

#[derive(Resource)]
struct Check {
    setup: setup::Setup,
    phase: Phase,
    began: Option<f64>,
    baseline: Option<inspection::Baseline>,
    impact: Option<Impact>,
    active: Option<inspection::Evidence>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = setup::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.attacker], true);
    app.insert_resource(Check {
        setup,
        phase: Phase::Baseline,
        began: None,
        baseline: None,
        impact: None,
        active: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, control);
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

fn frame(check: Res<Check>, terrain: Res<Terrain>, mut rig: ResMut<CameraRig>) {
    let check = check.into_inner();
    let terrain = terrain.into_inner();
    rig.distance = rig.settings.min_height;
    capture_framing::focus_on(&mut rig, terrain, check.setup.focus);
}

fn control(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    capture: Res<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let capture = capture.into_inner();
    if matches!(check.phase, Phase::BaselineSaved) && capture.ready_for_scenario() {
        match battle
            .simulation
            .force_attack(check.setup.attacker, check.setup.target)
        {
            Ok(()) => {
                check.phase = Phase::Attack;
                time.unpause();
            }
            Err(error) => {
                error!("LightPulse ordinary force attack: {error}");
                check.phase = Phase::Done;
                exit.write(AppExit::error());
            }
        }
    }
}

fn record(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !matches!(check.phase, Phase::Attack) {
        return;
    }
    let target = check.setup.target;
    let Some(event) = battle
        .simulation
        .presentation
        .object_fx_events
        .iter()
        .find(|event| event.object == target && event.effect == "FX_DamageTankStruck")
    else {
        return;
    };
    check.impact = Some(Impact {
        tick: event.frame,
        position: event.position,
    });
    let game = &mut battle.simulation;
    let result = require(
        game.entities[target].alive() && game.entities[target].health < check.setup.health,
        "original tank damage callback lacks real nonfatal damage",
    )
    .and_then(|()| {
        game.stop(check.setup.attacker)?;
        game.stop(target)?;
        Ok(())
    });
    if let Err(error) = result {
        error!("LightPulse real damage: {error}");
        check.phase = Phase::Done;
        exit.write(AppExit::error());
        return;
    }
    // Virtual pause stops further combat, not W3DDynamicLight display-frame fade.
    time.pause();
    check.phase = Phase::Impact;
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
    lights: inspection::Inspector<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    if matches!(check.phase, Phase::Done) {
        return;
    }
    let result = view.observe(&mut check, &mut capture);
    if let Err(error) = result {
        error!("Original LightPulse capture: {error}");
        check.phase = Phase::Done;
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn observe(self, check: &mut Check, capture: &mut Capture) -> Result<(), Box<dyn Error>> {
        let now = self.clock.elapsed_secs_f64();
        require(
            now - *check.began.get_or_insert(now) < 90.0,
            "LightPulse observation timed out",
        )?;
        if !capture.ready_for_scenario() {
            return Ok(());
        }
        let target = check.setup.target;
        if !self.framing.visible(&self.battle, target)? {
            return Ok(());
        }
        match check.phase {
            Phase::Baseline => {
                check.baseline = Some(self.lights.baseline(&self.battle, target)?);
                capture.intermediate("lightpulse-before.png");
                check.phase = Phase::BaselineSaved;
            }
            Phase::Impact => {
                let impact = check
                    .impact
                    .as_ref()
                    .ok_or("missing actual damage callback")?;
                let baseline = check
                    .baseline
                    .as_ref()
                    .ok_or("missing original model baseline")?;
                check.active = Some(self.lights.active(&self.battle, target, baseline, impact)?);
                println!(
                    "Original LightPulse active: target={target} damage_tick={} position={:?}; actual model material and CPU terrain captured",
                    impact.tick, impact.position
                );
                capture.intermediate("lightpulse-active.png");
                check.phase = Phase::ActiveSaved;
            }
            Phase::ActiveSaved => {
                let baseline = check
                    .baseline
                    .as_ref()
                    .ok_or("missing original model baseline")?;
                let active = check
                    .active
                    .as_ref()
                    .ok_or("missing actual active light evidence")?;
                if self
                    .lights
                    .retired(&self.battle, target, baseline, active)?
                {
                    capture.finish_scenario();
                    check.phase = Phase::Done;
                    println!(
                        "Original LightPulse retired: model restored and actual CPU terrain contribution changed back; inspect fixed-camera images"
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}
