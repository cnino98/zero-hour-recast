//! Paid Comanche Countermeasures versus original `MiG` missiles.
use crate::{
    aircraft_verification::{
        self,
        comanche_trial::{self, Exposure},
    },
    capture_framing::{AircraftFrame, focus_on},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    strategy_verification::research,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;

#[derive(Resource)]
struct Check {
    aircraft: usize,
    defender: usize,
    airfield: usize,
    first: usize,
    began: Option<f64>,
    trial: Option<Exposure>,
    done: bool,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let setup = aircraft_verification::prepare(game)?;
    research(game, setup.airfield, "Upgrade_AmericaCountermeasures")?;
    let defender = comanche_trial::produce_mig(game)?;
    app.insert_resource(Check {
        aircraft: setup.comanche,
        defender,
        airfield: setup.airfield,
        first: game.entities.len(),
        began: None,
        trial: None,
        done: false,
    });
    battle.select_objects(&[setup.comanche], true);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        frame
            .after(crate::playtest::camera_controls)
            .before(crate::playtest::update_camera),
    );
    app.add_systems(FixedFirst, observe_trial);
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let game = &battle.simulation;
    camera.distance = camera.settings.max_height;
    focus_on(
        &mut camera,
        terrain.into_inner(),
        game.object_position(check.aircraft),
    );
}

fn observe_trial(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    mut exit: MessageWriter<AppExit>,
) {
    if !check.done
        && let Some(trial) = &mut check.trial
        && let Err(error) = trial.observe(&mut battle.simulation)
    {
        check.done = true;
        error!("Native Comanche Countermeasures: {error}");
        exit.write(AppExit::error());
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
    particles: ParticleInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut capture: ResMut<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.done || !capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.advance(&mut check, &mut battle, &mut capture, &mut time) {
        check.done = true;
        error!("Native Comanche Countermeasures: {error}");
        exit.write(AppExit::error());
    }
}

impl View<'_, '_> {
    fn advance(
        self,
        check: &mut Check,
        battle: &mut Match,
        capture: &mut Capture,
        time: &mut Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let now = self.clock.elapsed_secs_f64();
        let start = *check.began.get_or_insert(now);
        let game = &mut battle.simulation;
        require(
            now - start < 240.0,
            &format!(
                "no rendered Comanche volley, diversion={} at {}",
                check.trial.as_ref().is_some_and(|trial| trial.diverted),
                aircraft_verification::details(game, check.aircraft)
            ),
        )?;
        let Some(trial) = &check.trial else {
            check.trial = Some(Exposure::new(
                check.aircraft,
                check.defender,
                check.airfield,
            ));
            time.unpause();
            return Ok(());
        };
        require(
            game.entities[check.aircraft].alive(),
            "paid Comanche died before original flare capture",
        )?;
        if !trial.diverted || !self.framing.visible(battle, check.aircraft)? {
            return Ok(());
        }
        let game = &battle.simulation;
        let comanche = Vec3::from_array(game.object_position(check.aircraft));
        for (id, flare) in game.entities.iter().enumerate().skip(check.first) {
            if flare.exists()
                && game.catalog[flare.type_index].name == "CountermeasureFlare"
                && Vec3::from_array(game.object_position(id)).distance(comanche) > 12.0
                && self.particles.drawable_state(
                    id,
                    "CountermeasureFlareParticleSys",
                    battle.generation,
                )?
                && self
                    .particles
                    .drawable_state(id, "CounterMeasureSmoke", battle.generation)?
            {
                println!(
                    "Native Comanche Countermeasures: paid missile diverted, visible flare and original smoke separated from live Comanche"
                );
                time.pause();
                check.done = true;
                capture.finish_scenario();
                break;
            }
        }
        Ok(())
    }
}
