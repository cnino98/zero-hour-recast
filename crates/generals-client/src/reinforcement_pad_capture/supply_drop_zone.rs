//! Paid USA Supply Drop Zone, original countdown and physical cargo delivery.
use crate::{
    capture_framing::{self, AircraftFrame},
    control_bar::UiData,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::{advance_until, construct, construct_type, friendly_entity, require},
    strategy_verification::template,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::{Faction, Kind, Skirmish};
use std::{error::Error, time::Duration};

pub fn build_paid_zone(game: &mut Skirmish) -> Result<usize, Box<dyn Error>> {
    game.reset_sandbox(Faction::Usa)?;
    let dozer = friendly_entity(game, Kind::Builder, 0)?;
    let zone_type = template(game, "AmericaSupplyDropZone")?;
    require(
        game.catalog[zone_type].cost == 2500,
        "original Drop Zone cost differs",
    )?;
    require(
        game.construction_admission(dozer, zone_type).is_err(),
        "Drop Zone is available without its original prerequisites",
    )?;
    construct(game, dozer, Kind::PowerPlant, 0)?;
    construct(game, dozer, Kind::SupplyCenter, 0)?;
    construct(game, dozer, Kind::Factory, 0)?;
    let center_type = template(game, "AmericaStrategyCenter")?;
    construct_type(game, dozer, center_type, 0)?;
    advance_until(
        game,
        18_000,
        "ordinary supplies funding Drop Zone",
        |game| game.players[0].cash >= game.catalog[zone_type].cost,
    )?;
    construct_type(game, dozer, zone_type, 0)
}

#[derive(Resource)]
struct Check {
    zone: usize,
    chute_type: usize,
    crate_type: usize,
    first_payload: usize,
    phase: Phase,
    started: Option<Duration>,
}

#[derive(Clone, Copy)]
enum Phase {
    Countdown,
    WaitForCapture,
    Delivery,
    Done,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let game = &mut battle.simulation;
    let zone = build_paid_zone(game)?;
    let chute_type = template(game, "AmericaCrateParachute")?;
    let crate_type = template(game, "SupplyDropZoneCrate")?;
    let first_payload = game.entities.len();
    advance_until(
        game,
        3_601,
        "Drop Zone nearing its first native delivery",
        |game| {
            game.delivery_countdown(zone)
                .is_some_and(|(seconds, progress)| seconds == 3 && progress > 90)
        },
    )?;
    battle.started = true;
    battle.select_objects(&[zone], true);
    app.insert_resource(Check {
        zone,
        chute_type,
        crate_type,
        first_payload,
        phase: Phase::Countdown,
        started: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, advance);
    app.add_systems(
        Update,
        focus
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

fn advance(
    mut check: ResMut<Check>,
    capture: Res<Capture>,
    clock: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let clock = clock.into_inner();
    let capture = capture.into_inner();
    if matches!(check.phase, Phase::Done) {
        return;
    }
    let now = clock.elapsed();
    if now.saturating_sub(*check.started.get_or_insert(now)) > Duration::from_secs(90) {
        let event = match check.phase {
            Phase::Countdown => "localized countdown and building",
            Phase::WaitForCapture => "countdown frame write",
            Phase::Delivery => "opened parachute and its crate",
            Phase::Done => unreachable!(),
        };
        error!("Original Supply Drop Zone capture timed out waiting for {event}");
        check.phase = Phase::Done;
        exit.write(AppExit::error());
    } else if matches!(check.phase, Phase::WaitForCapture) && capture.ready_for_scenario() {
        check.phase = Phase::Delivery;
        time.unpause();
    }
}

fn opened_payload(game: &Skirmish, check: &Check) -> Option<(usize, usize)> {
    game.entities[check.first_payload..]
        .iter()
        .enumerate()
        .find_map(|(offset, actor)| {
            if actor.type_index != check.chute_type || !actor.alive() || !actor.parachute_opened() {
                return None;
            }
            let chute = check.first_payload + offset;
            let [item] = game.passengers(chute) else {
                return None;
            };
            (game.entities[*item].type_index == check.crate_type).then_some((chute, *item))
        })
}

fn focus(
    check: Res<Check>,
    battle: Res<Match>,
    mut camera: ResMut<CameraRig>,
    terrain: Res<Terrain>,
) {
    let check = check.into_inner();
    let game = &battle.into_inner().simulation;
    let delivery = matches!(check.phase, Phase::Delivery | Phase::Done);
    let id = if delivery {
        opened_payload(game, check).map_or(check.zone, |(chute, _)| chute)
    } else {
        check.zone
    };
    let mut center = game.object_position(id);
    center[2] += game.catalog[game.entities[id].type_index]
        .geometry_bounds
        .center_height;
    camera.distance = if delivery {
        camera.settings.min_height
    } else {
        camera.settings.max_height
    };
    capture_framing::focus_on(&mut camera, terrain.into_inner(), center);
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    data: Res<'w, UiData>,
    frame: AircraftFrame<'w, 's>,
    labels: Query<
        'w,
        's,
        (
            &'static Text,
            &'static ComputedNode,
            &'static InheritedVisibility,
        ),
    >,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !view.capture.ready_for_scenario() {
        return;
    }
    let result = match check.phase {
        Phase::Countdown => {
            let expected = view
                .data
                .ocl_timer()
                .padded_description
                .replacen("%d", "0", 1)
                .replacen("%d", "3", 1);
            let text_visible = view.labels.iter().any(|(label, node, visible)| {
                label.0 == expected && visible.get() && node.size().cmpgt(Vec2::ZERO).all()
            });
            view.frame
                .visible(&view.battle, check.zone)
                .map(|zone| text_visible && zone)
        }
        Phase::Delivery => {
            let Some((chute, item)) = opened_payload(&view.battle.simulation, &check) else {
                return;
            };
            view.frame.visible(&view.battle, chute).and_then(|chute| {
                view.frame
                    .visible(&view.battle, item)
                    .map(|item| chute && item)
            })
        }
        Phase::WaitForCapture | Phase::Done => return,
    };
    match result {
        Ok(true) if matches!(check.phase, Phase::Countdown) => {
            view.capture.intermediate("supply-drop-zone-countdown.png");
            check.phase = Phase::WaitForCapture;
        }
        Ok(true) => {
            view.time.pause();
            view.capture.finish_scenario();
            check.phase = Phase::Done;
            println!(
                "Original Supply Drop Zone: paid construction, localized countdown, opened crate parachute and separate crate mesh visible"
            );
        }
        Ok(false) => {}
        Err(error) => {
            error!("Supply Drop Zone native verification: {error}");
            check.phase = Phase::Done;
            view.exit.write(AppExit::error());
        }
    }
}
