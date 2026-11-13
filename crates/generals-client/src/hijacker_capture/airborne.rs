//! An ordinary missile strike releases the same stored driver from airborne cargo.
use crate::{
    capture_framing::{AircraftFrame, focus_on},
    hijacker_verification::{self, Airborne},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use std::{error::Error, f32::consts::FRAC_PI_2};

#[derive(Resource)]
struct Check {
    setup: Airborne,
    phase: Phase,
    began: Option<f64>,
    yaw: Option<f32>,
    canopy: Option<usize>,
    hull: Option<usize>,
    departure: Option<Vec2>,
}

#[derive(PartialEq, Eq)]
enum Phase {
    Waiting,
    Falling,
    Canopy,
    Landing,
    OnFoot,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = hijacker_verification::prepare_airborne(&mut battle.simulation)?;
    battle.select_objects(&[setup.chinook], true);
    app.insert_resource(Check {
        setup,
        phase: Phase::Waiting,
        began: None,
        yaw: None,
        canopy: None,
        hull: None,
        departure: None,
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
    app.add_systems(
        PostUpdate,
        inspect
            .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
            .after(bevy::ui::UiSystems::PostLayout),
    );
}

fn frame(
    mut check: ResMut<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let battle = battle.into_inner();
    let game = &battle.simulation;
    let id = match check.phase {
        Phase::Waiting => check.setup.chinook,
        _ => check.setup.driver,
    };
    let yaw = *check.yaw.get_or_insert(camera.settings.yaw);
    if matches!(check.phase, Phase::Falling | Phase::Canopy)
        && game.entities[check.setup.driver].contained_by().is_some()
    {
        check.hull = check.hull.or_else(|| humvee_hull(game, check.setup.host));
        camera.settings.yaw = fall_yaw(game, &check, yaw);
    } else {
        camera.settings.yaw = yaw;
    }
    camera.distance = if game.height_above_ground(id) > 10.0 {
        camera.settings.max_height
    } else {
        camera.settings.min_height
    };
    focus_on(&mut camera, terrain.into_inner(), game.object_position(id));
}

fn humvee_hull(game: &generals_sim::skirmish::Skirmish, host: usize) -> Option<usize> {
    game.entities.iter().position(|actor| {
        actor.producer() == Some(host)
            && actor.physical_present()
            && game.catalog[actor.type_index].name == "AmericaVehicleHumveeDeadHull"
    })
}

fn fall_yaw(game: &generals_sim::skirmish::Skirmish, check: &Check, yaw: f32) -> f32 {
    let rider = Vec2::from_array(game.entities[check.setup.driver].position);
    let from_chinook = rider - Vec2::from_array(game.entities[check.setup.chinook].position);
    let from_hull = check
        .hull
        .map(|id| rider - Vec2::from_array(game.entities[id].position));
    let anchor = from_hull
        .filter(|away| away.length_squared() > 1.0)
        .unwrap_or(from_chinook);
    if anchor.length_squared() <= 1.0 {
        return yaw + FRAC_PI_2;
    }
    // View from the rider's side of the wreck. Overlapping screen rectangles
    // are acceptable only when the real rider geometry is entirely in front.
    anchor.x.atan2(-anchor.y)
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
        error!("Native airborne Hijacker: {error}");
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
        let Airborne {
            driver,
            chinook,
            host,
            defender,
            losses,
        } = check.setup;
        let game = &battle.simulation;
        let actor = &game.entities[driver];
        require(
            now - start < 90.0,
            &format!(
                "airborne driver timed out: tick={} cargo={:?} driver={:?} contained={:?}",
                game.tick,
                game.entities[host].contained_by(),
                actor.driving_vehicle(),
                actor.contained_by()
            ),
        )?;
        match check.phase {
            Phase::Waiting if self.framing.visible(battle, chinook)? => {
                require(
                    actor.alive() && actor.masked() && actor.driving_vehicle() == Some(host),
                    "paid cargo lost its stored driver before AA fire",
                )?;
                battle.simulation.attack(defender, chinook)?;
                check.phase = Phase::Falling;
                time.unpause();
            }
            Phase::Falling => self.falling(check, battle, capture, time)?,
            Phase::Canopy => {
                check.phase = Phase::Landing;
                time.unpause();
            }
            Phase::Landing
                if actor.contained_by().is_none() && self.framing.visible(battle, driver)? =>
            {
                require(
                    actor.alive()
                        && !actor.masked()
                        && !actor.parachute_rider()
                        && actor.selectable(&game.catalog[actor.type_index])
                        && game.height_above_ground(driver).abs() < 0.1
                        && game.players[0].statistics.units_lost == losses + 2,
                    "landing did not restore the original selectable driver or count both vehicle losses",
                )?;
                let departure = Vec2::from_array(actor.position);
                let wreck = Vec2::from_array(game.entities[chinook].position);
                let away = (departure - wreck).normalize_or(Vec2::Y);
                let target = departure + away * 55.0;
                battle.select_objects(&[driver], true);
                battle.simulation.move_to(driver, target.to_array())?;
                check.departure = Some(departure);
                check.phase = Phase::OnFoot;
            }
            Phase::OnFoot => {
                let departure = check.departure.ok_or("driver has no landing position")?;
                require(
                    actor.alive(),
                    "restored driver died after parachute release",
                )?;
                if Vec2::from_array(actor.position).distance(departure) >= 30.0
                    && check
                        .canopy
                        .is_some_and(|chute| !game.entities[chute].has_drawable())
                    && self.framing.visible(battle, driver)?
                {
                    check.phase = Phase::Done;
                    time.pause();
                    capture.finish_scenario();
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn falling(
        self,
        check: &mut Check,
        battle: &Match,
        capture: &mut Capture,
        time: &mut Time<Virtual>,
    ) -> Result<(), Box<dyn Error>> {
        let Airborne {
            driver,
            chinook,
            host,
            defender,
            ..
        } = check.setup;
        let game = &battle.simulation;
        let actor = &game.entities[driver];
        require(actor.alive(), "stored driver died with the airborne cargo")?;
        let Some(chute) = actor
            .contained_by()
            .filter(|&id| game.entities[id].parachute_opened())
        else {
            return Ok(());
        };
        require(
            !game.entities[host].exists()
                && !game.entities[chinook].alive()
                && game.entities[chinook].last_attacker() == Some(defender)
                && game.catalog[game.entities[chute].type_index].name == "AmericaParachute"
                && game.entities[chute].owner == 0
                && actor.driving_vehicle().is_none()
                && actor.parachute_rider(),
            "cargo death did not release the original driver under its owned canopy",
        )?;
        let Some(hull) = check.hull else {
            return Ok(());
        };
        if game.height_above_ground(chute) > 15.0
            && self.framing.visible(battle, chute)?
            && self.framing.visible(battle, driver)?
            && self.framing.unobscured_actor(battle, chinook, driver)?
            && self.framing.unobscured_actor(battle, chinook, chute)?
            && self.framing.unobscured_actor(battle, hull, driver)?
            && self.framing.unobscured_actor(battle, hull, chute)?
        {
            time.pause();
            check.canopy = Some(chute);
            check.phase = Phase::Canopy;
            capture.intermediate("hijacker-airborne-canopy.png");
            println!(
                "Native airborne Hijacker: open canopy {chute}, separate stored driver {driver}, paid AA kill {defender}, original Humvee hull {hull}"
            );
        }
        Ok(())
    }
}
