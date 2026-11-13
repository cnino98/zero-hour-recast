//! Original Killing Fields bridge and water contacts after paid fixed-wing and helicopter combat.
mod trial;

use crate::{
    capture_framing::{AircraftFrame, focus_on},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::error::Error;
use trial::{Fall, Site, Trial, WATER_DROP_FRAMES};

#[derive(Resource)]
struct Check {
    site: Site,
    trial: Trial,
    phase: Phase,
    began: Option<f64>,
    water_contact: Option<u64>,
}

#[derive(PartialEq, Eq)]
enum Phase {
    Waiting,
    Canopy,
    Outcome,
    BridgeRecorded,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let site = Site::Bridge;
    let trial = Trial::prepare(game, site)?;
    battle.select_objects(&[trial.aircraft], true);
    app.insert_resource(Check {
        site,
        trial,
        phase: Phase::Waiting,
        began: None,
        water_contact: None,
    });
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
    app.add_systems(FixedFirst, combat);
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
    let game = &battle.into_inner().simulation;
    let id = check
        .trial
        .fall
        .as_ref()
        .map_or(check.trial.aircraft, |fall| fall.pilot);
    let position = game.object_position(id);
    camera.distance = if game.height_above_ground(id) > 60.0 {
        camera.settings.max_height
    } else {
        camera.settings.min_height
    };
    focus_on(&mut camera, terrain.into_inner(), position);
}

fn combat(mut check: ResMut<Check>, mut battle: ResMut<Match>, mut exit: MessageWriter<AppExit>) {
    if matches!(check.phase, Phase::Canopy | Phase::Outcome)
        && let Err(error) = check.trial.observe(&mut battle.simulation)
    {
        check.phase = Phase::Done;
        error!("Native parachute terrain combat: {error}");
        exit.write(AppExit::error());
    }
}

#[derive(SystemParam)]
struct View<'w, 's> {
    clock: Res<'w, Time<Real>>,
    framing: AircraftFrame<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    view: View,
    mut battle: ResMut<Match>,
    mut capture: ResMut<Capture>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == Phase::Done || !capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = view.advance(&mut check, &mut battle, &mut capture, &mut time) {
        check.phase = Phase::Done;
        error!("Native parachute terrain: {error}");
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
        let began = *check.began.get_or_insert(now);
        require(
            now - began < 100.0,
            &format!(
                "{:?} trial timed out: {}",
                check.site,
                check.trial.details(&battle.simulation)
            ),
        )?;
        match check.phase {
            Phase::Waiting => {
                check.trial.begin(&mut battle.simulation)?;
                check.phase = Phase::Canopy;
                time.unpause();
            }
            Phase::Canopy => self.canopy(check, battle, capture, time)?,
            Phase::Outcome => self.outcome(check, battle, capture, time)?,
            Phase::BridgeRecorded => {
                check.site = Site::Water;
                check.trial = Trial::prepare(&mut battle.simulation, check.site)?;
                battle.generation = battle.generation.wrapping_add(1);
                battle.select_objects(&[check.trial.aircraft], true);
                check.began = None;
                check.phase = Phase::Waiting;
            }
            Phase::Done => {}
        }
        Ok(())
    }

    fn canopy(
        self,
        check: &mut Check,
        battle: &Match,
        capture: &mut Capture,
        time: &mut Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(fall) = &check.trial.fall else {
            return Ok(());
        };
        let game = &battle.simulation;
        let canopy = &game.entities[fall.canopy];
        if canopy.parachute_opened()
            && canopy.alive()
            && check.trial.over_surface(game, fall.canopy)
            && self.framing.visible(battle, fall.canopy)?
            && self.framing.visible(battle, fall.pilot)?
        {
            println!(
                "Native {:?} canopy: {}",
                check.site,
                check.trial.details(game)
            );
            capture.intermediate(match check.site {
                Site::Bridge => "parachute-bridge-canopy.png",
                Site::Water => "parachute-water-canopy.png",
            });
            time.pause();
            check.phase = Phase::Outcome;
        }
        Ok(())
    }

    fn outcome(
        self,
        check: &mut Check,
        battle: &mut Match,
        capture: &mut Capture,
        time: &mut Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(Fall { canopy, pilot }) = check.trial.fall else {
            return Ok(());
        };
        time.unpause();
        let game = &battle.simulation;
        if check.water_contact.is_none() && !check.trial.completed(game)? {
            return Ok(());
        }
        if matches!(check.site, Site::Water) {
            let contact = *check.water_contact.get_or_insert(game.tick);
            // Give the original PMparacht_DRP animation half a second before
            // framing the retained canopy after its initial water contact.
            if game.tick.saturating_sub(contact) < WATER_DROP_FRAMES {
                return Ok(());
            }
            require(
                check
                    .trial
                    .retains_water_outcome(game, Fall { canopy, pilot }),
                &format!(
                    "water death or canopy drawable lost: {}",
                    check.trial.details(game)
                ),
            )?;
        }
        let visible = match check.site {
            Site::Bridge => self.framing.visible(battle, pilot)?,
            Site::Water => self.framing.visible(battle, canopy)?,
        };
        if !visible {
            return Ok(());
        }
        println!(
            "Native {:?} contact: {}",
            check.site,
            check.trial.details(game)
        );
        time.pause();
        match check.site {
            Site::Bridge => {
                battle.select_objects(&[pilot], true);
                capture.intermediate("parachute-bridge-landed.png");
                check.phase = Phase::BridgeRecorded;
            }
            Site::Water => {
                capture.finish_scenario();
                check.phase = Phase::Done;
            }
        }
        Ok(())
    }
}
