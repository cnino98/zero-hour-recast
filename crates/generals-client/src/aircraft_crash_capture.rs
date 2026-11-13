//! Paid anti-air kill, original falling jet/debris and the actual ground-crash FX.
pub mod pilot;
pub mod terrain;
#[cfg(test)]
mod tests;
mod views;

use crate::{
    aircraft_verification::{self, crash},
    capture_framing::{self, AircraftFrame},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    source_audio::FiringAudioInspection,
    terrain::Terrain,
};
use bevy::prelude::*;
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: crash::Setup,
    stage: Stage,
    death: Option<u64>,
    ground: Option<GroundImpact>,
    hulk: Option<usize>,
    sounds: [bool; 3],
    began: Option<f64>,
    seen: [bool; 4],
    snapshot: Option<views::Snapshot>,
    view_index: u8,
}

struct GroundImpact {
    frame: u64,
    position: [f32; 3],
}

enum Stage {
    Waiting,
    Attack,
    FallingCaptured,
    FallingView,
    Ground,
    GroundCaptured,
    Settling,
    HulkCaptured,
    HulkView,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = crash::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.aircraft.raptor], true);
    app.insert_resource(Check {
        setup,
        stage: Stage::Waiting,
        death: None,
        ground: None,
        hulk: None,
        sounds: [false; 3],
        began: None,
        seen: [false; 4],
        snapshot: None,
        view_index: 0,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, (control, focus, audio).chain());
    app.add_systems(FixedFirst, target_raptor);
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn focus(
    check: Res<Check>,
    battle: Res<Match>,
    mut camera: ResMut<CameraRig>,
    terrain: Res<Terrain>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    if matches!(check.stage, Stage::Settling | Stage::Done)
        && let (Some(id), Some(impact)) = (check.hulk, &check.ground)
    {
        camera.settings.yaw =
            impact_view_yaw(battle.simulation.object_transform(id).0, impact.position);
    }
    if check.hulk.is_some() {
        camera.distance = camera.settings.min_height;
    }
    if let Some(snapshot) = &check.snapshot {
        camera.settings.yaw =
            f32::from(check.view_index).mul_add(std::f32::consts::FRAC_PI_2, snapshot.yaw);
    }
    capture_framing::focus_on(
        &mut camera,
        terrain.into_inner(),
        battle
            .simulation
            .object_position(check.hulk.unwrap_or(check.setup.aircraft.raptor)),
    );
}

fn control(mut check: ResMut<Check>, capture: Res<Capture>, mut time: ResMut<Time<Virtual>>) {
    let capture = capture.into_inner();
    if matches!(check.stage, Stage::Done) {
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    if matches!(check.stage, Stage::FallingCaptured | Stage::HulkCaptured) {
        check.next_view(&mut time);
    } else if matches!(check.stage, Stage::GroundCaptured) {
        check.stage = Stage::Settling;
        time.unpause();
    }
}

// Resolve the defending player's target before each logic step, even when
// several steps run between rendered frames.
fn target_raptor(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut exit: MessageWriter<AppExit>,
) {
    if matches!(check.stage, Stage::Attack | Stage::Ground)
        && let Err(error) = crash::attack_when_visible(&mut battle.simulation, &check.setup)
    {
        error!("Aircraft crash command: {error}");
        check.stage = Stage::Done;
        exit.write(AppExit::error());
    }
}

fn audio(mut check: ResMut<Check>, audio: FiringAudioInspection) {
    let events = ["RaptorDie", "RaptorVoiceFalling", "RaptorCrash"];
    if let Some(index) = check.sounds.iter().position(|heard| !heard) {
        check.sounds[index] |= audio.object_fx(events[index]);
    }
}

fn record(mut check: ResMut<Check>, battle: Res<Match>, mut exit: MessageWriter<AppExit>) {
    let battle = battle.into_inner();
    if matches!(check.stage, Stage::Done | Stage::Waiting) {
        return;
    }
    let game = &battle.simulation;
    if check.death.is_none() && !game.entities[check.setup.aircraft.raptor].alive() {
        match crash::verify_death(game, &check.setup) {
            Ok(frame) => check.death = Some(frame),
            Err(error) => {
                error!("Aircraft crash death: {error}");
                check.stage = Stage::Done;
                exit.write(AppExit::error());
            }
        }
    }
    for event in &game.presentation.object_fx_events {
        if event.object == check.setup.aircraft.raptor && event.effect == "FX_JetDeathHitGround" {
            check.ground = Some(GroundImpact {
                frame: event.frame,
                position: event.position,
            });
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
    animation_clock: Res<'w, crate::model_animation::Clock>,
    camera: Res<'w, CameraRig>,
}

fn physical(
    battle: &Match,
    producer: usize,
    model: Option<&str>,
    template: &str,
    framing: &AircraftFrame,
) -> Result<bool, Box<dyn Error>> {
    for (id, actor) in battle.simulation.entities.iter().enumerate() {
        if actor.producer() == Some(producer)
            && actor.physical_present()
            && actor.physical_model() == model
            && battle.simulation.catalog[actor.type_index].name == template
            && framing.visible(battle, id)?
            && framing.physical_pose(battle, id)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn inspect(
    check: ResMut<Check>,
    view: View,
    battle: ResMut<Match>,
    time: ResMut<Time<Virtual>>,
    capture: ResMut<Capture>,
    exit: MessageWriter<AppExit>,
) {
    view.inspect(check, battle, time, capture, exit);
}

impl View<'_, '_> {
    fn inspect(
        self,
        mut check: ResMut<Check>,
        mut battle: ResMut<Match>,
        mut time: ResMut<Time<Virtual>>,
        mut capture: ResMut<Capture>,
        mut exit: MessageWriter<AppExit>,
    ) {
        let view = self;

        let framing = &view.framing;
        let particles = &view.particles;
        let now = view.clock.elapsed_secs_f64();
        if matches!(check.stage, Stage::Done) {
            return;
        }
        let elapsed = now - *check.began.get_or_insert(now);
        let result = (|| {
            require(
                elapsed < 120.0,
                "native aircraft crash exceeded 120 real seconds",
            )?;
            if !capture.ready_for_scenario() {
                return Ok(());
            }
            let id = check.setup.aircraft.raptor;
            if matches!(check.stage, Stage::Waiting) {
                if framing.visible(&battle, id)? {
                    crash::begin(&mut battle.simulation, &check.setup)?;
                    check.stage = Stage::Attack;
                    time.unpause();
                }
            } else if matches!(check.stage, Stage::Attack)
                && let Some(frame) = check.death
            {
                check.seen = [
                    framing.visible(&battle, id)? && framing.physical_pose(&battle, id)?,
                    physical(&battle, id, Some("AVRaptor_D4"), "GenericDebris", framing)?,
                    particles.actor_fx("FX_RaptorDeathInitial", frame, battle.generation)?,
                    particles.incendiary("JetBurning", battle.generation)?,
                ];
                if check.seen.iter().all(|seen| *seen)
                    && check.sounds[..2].iter().all(|heard| *heard)
                {
                    check.stage = Stage::FallingCaptured;
                    time.pause();
                    check.freeze(&battle, &view, &time, id);
                    check.log_view(&battle, &view, &time)?;
                    capture.intermediate("aircraft-crash-falling.png");
                    println!(
                        "Original airborne crash captured: real Gattling kill, retained Raptor/full physical basis, original D4 debris, ALPHA_TEST JetDebris, attached JetBurning and both advancing death voices"
                    );
                }
            } else if matches!(check.stage, Stage::Ground) {
                check.capture_ground(&battle, framing, particles, &mut time, &mut capture)?;
            } else if matches!(check.stage, Stage::Settling) {
                check.capture_hulk(&battle, &view, &mut time, &mut capture)?;
            } else if matches!(check.stage, Stage::FallingView | Stage::HulkView) {
                check.capture_view(&battle, &view, &time, &mut capture)?;
            }
            Ok::<(), Box<dyn Error>>(())
        })();
        if let Err(error) = result {
            error!(
                "Native aircraft crash: {error}; death={:?} ground={:?} visible={:?} audio={:?}; {}; gun={}; weapon={}",
                check.death,
                check.ground.as_ref().map(|impact| impact.frame),
                check.seen,
                check.sounds,
                aircraft_verification::details(&battle.simulation, check.setup.aircraft.raptor),
                generals_sim::skirmish::diagnostics::movement_details(
                    &battle.simulation,
                    check.setup.attacker
                ),
                generals_sim::skirmish::diagnostics::weapon_details(
                    &battle.simulation,
                    check.setup.attacker,
                    check.setup.aircraft.raptor
                )
            );
            check.stage = Stage::Done;
            exit.write(AppExit::error());
        }
    }
}

impl Check {
    fn capture_ground(
        &mut self,
        battle: &Match,
        framing: &AircraftFrame,
        particles: &ParticleInspection,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let Some(impact) = &self.ground else {
            return Ok(());
        };
        let frame = impact.frame;
        let id = self.setup.aircraft.raptor;
        self.seen[0] = physical(battle, id, None, "AmericaJetRaptorHulk", framing)?;
        self.seen[2] = particles.actor_fx("FX_JetDeathHitGround", frame, battle.generation)?;
        if self.seen[0] && self.seen[2] && self.sounds[2] {
            self.hulk = battle.simulation.entities.iter().position(|actor| {
                actor.producer() == Some(id)
                    && actor.physical_present()
                    && battle.simulation.catalog[actor.type_index].name == "AmericaJetRaptorHulk"
            });
            self.stage = Stage::GroundCaptured;
            time.pause();
            capture.intermediate("aircraft-crash.png");
            println!(
                "Original ground crash captured: native Raptor hulk, original ground explosion/dust/debris and advancing RaptorCrash audio"
            );
        }
        Ok(())
    }

    fn capture_hulk(
        &mut self,
        battle: &Match,
        view: &View,
        time: &mut Time<Virtual>,
        capture: &mut Capture,
    ) -> Result<(), Box<dyn Error>> {
        let framing = &view.framing;
        let particles = &view.particles;
        let id = self
            .hulk
            .ok_or("captured crash has no original hulk identity")?;
        let actor = &battle.simulation.entities[id];
        require(
            actor.physical_present(),
            "original hulk retired before its after-flash capture",
        )?;
        // The paid source check places the flash retirement near frame 67,
        // before the hulk starts to sink. Frame 100 is already visibly buried.
        // Keep the earlier live, partially exposed wreck at the original zoom limit.
        let impact = self
            .ground
            .as_ref()
            .ok_or("missing original jet ground impact")?;
        let age = battle.simulation.tick.saturating_sub(impact.frame);
        if age >= 75
            && particles.actor_flash_finished("FX_JetDeathHitGround", battle.generation)?
            && framing.physical_pose(battle, id)?
            && framing.visible(battle, id)?
        {
            self.stage = Stage::HulkCaptured;
            time.pause();
            self.freeze(battle, view, time, id);
            self.log_view(battle, view, time)?;
            capture.intermediate("aircraft-crash-hulk-view-0.png");
            let position = battle.simulation.object_position(id);
            println!(
                "Original Raptor hulk captured {age} frames after impact: actor={id} position={position:?} terrain={} basis={:?}; unchanged physical lifetime and exact rendered matrix",
                battle.simulation.ground_height([position[0], position[1]]),
                actor.physical_basis(),
            );
        }
        Ok(())
    }
}

// Camera source coordinates are [sin(yaw), -cos(yaw)]. The hulk can move
// beyond its original ground-impact position before the flash finishes.
fn impact_view_yaw(hulk: [f32; 3], impact: [f32; 3]) -> f32 {
    (hulk[0] - impact[0]).atan2(impact[1] - hulk[1])
}

#[cfg(test)]
pub fn test_impact_view_yaw(hulk: [f32; 3], impact: [f32; 3]) -> f32 {
    impact_view_yaw(hulk, impact)
}
