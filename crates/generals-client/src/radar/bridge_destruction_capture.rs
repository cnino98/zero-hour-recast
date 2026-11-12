//! Original Killing Fields landmark, paid force-ground damage and delayed radar.
mod camera;
mod setup;
#[cfg(test)]
mod tests;
use super::{Display, Radar};
use crate::{
    capture_framing::{self, AircraftFrame},
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    skirmish_verification::require,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_sim::skirmish::Skirmish;
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: setup::Setup,
    phase: u8,
    started: Option<f64>,
    water: Option<(usize, u64)>,
    destroyed: Option<u64>,
    radar: Vec<(usize, [u8; 4])>,
    tail_seen: bool,
    emitter_id: Option<u64>,
    waiting: Option<(u8, &'static str)>,
}

pub const fn map_name() -> &'static str {
    "Maps\\Killing Fields\\Killing Fields.map"
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = setup::prepare(&mut battle.simulation)?;
    battle.started = true;
    battle.select_objects(&[setup.hydrant_tank], true);
    app.insert_resource(Check {
        setup,
        phase: 0,
        started: None,
        water: None,
        destroyed: None,
        radar: Vec::new(),
        tail_seen: false,
        emitter_id: None,
        waiting: None,
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, advance);
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
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
    geometry: camera::Geometry,
    mut exit: MessageWriter<AppExit>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    let focus = if (2..=6).contains(&check.phase) || check.phase == 14 {
        battle.simulation.object_position(check.setup.hydrant)
    } else {
        check.setup.target
    };
    camera.distance = camera.settings.max_height;
    let terrain = terrain.into_inner();
    capture_framing::focus_on(&mut camera, terrain, focus);
    if !((2..=6).contains(&check.phase) || check.phase == 14)
        && let Err(error) = geometry.fit(battle, check.setup.bridge, terrain, &mut camera)
    {
        error!("Original bridge observation camera: {error}");
        exit.write(AppExit::error());
    }
}

fn advance(
    mut check: ResMut<Check>,
    mut battle: ResMut<Match>,
    capture: Res<Capture>,
    clock: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase == 255 {
        return;
    }
    let now = clock.into_inner().elapsed_secs_f64();
    if now - *check.started.get_or_insert(now) > 420.0 {
        error!(
            "Original bridge destruction timed out in phase {}",
            check.phase
        );
        check.phase = 255;
        exit.write(AppExit::error());
        return;
    }
    if !capture.into_inner().ready_for_scenario() {
        return;
    }
    if let Err(error) = advance_scenario(&mut check, &mut battle.simulation, &mut time) {
        error!("Original bridge destruction phase {}: {error}", check.phase);
        check.phase = 255;
        exit.write(AppExit::error());
    }
}

fn advance_scenario(
    check: &mut Check,
    game: &mut Skirmish,
    time: &mut Time<Virtual>,
) -> Result<(), Box<dyn Error>> {
    if check.phase <= 5 || check.phase == 14 {
        advance_water(check, game, time)
    } else {
        advance_bridge(check, game, time)
    }
}

fn advance_water(
    check: &mut Check,
    game: &mut Skirmish,
    time: &mut Time<Virtual>,
) -> Result<(), Box<dyn Error>> {
    let site = &check.setup;
    match check.phase {
        1 => {
            let point = setup::floor(game, game.entities[site.hydrant].position);
            game.materialize_selection_group(0, &[site.hydrant_tank]);
            require(
                game.visible_to(0, site.hydrant)
                    && matches!(
                        game.force_attack_result(site.hydrant_tank, None, point),
                        generals_sim::skirmish::CanAttackResult::Possible
                    ),
                "actual hydrant ground point does not admit the paid Crusader shot",
            )?;
            game.group_force_attack_position(&[site.hydrant_tank], point)?;
            time.unpause();
            check.phase = 2;
        }
        2 => {
            if let Some(water) = game
                .entities
                .iter()
                .enumerate()
                .find(|(_, actor)| {
                    actor.exists()
                        && actor.producer() == Some(site.hydrant)
                        && game.catalog[actor.type_index].name == "WaterSpray"
                })
                .map(|(id, _)| id)
            {
                require(
                    game.catalog[game.entities[water].type_index]
                        .live_physics
                        .is_none(),
                    "WaterSpray acquired invented Physics",
                )?;
                game.stop(site.hydrant_tank)?;
                check.water = Some((water, game.tick));
                check.phase = 3;
            }
        }
        4 | 14 => {
            time.unpause();
            check.phase = 5;
        }
        _ => {}
    }
    Ok(())
}

fn advance_bridge(
    check: &mut Check,
    game: &mut Skirmish,
    time: &mut Time<Virtual>,
) -> Result<(), Box<dyn Error>> {
    let site = &check.setup;
    match check.phase {
        6 => {
            game.materialize_selection_group(game.entities[site.tomahawk].owner, &[site.tomahawk]);
            game.move_to_world(site.tomahawk, site.bridge_fire)?;
            require(
                setup::exact_goal(game, site.tomahawk, site.bridge_fire),
                "bridge firing goal adjusted",
            )?;
            time.unpause();
            check.phase = 7;
        }
        7 if setup::squared(
            game.entities[site.tomahawk].position,
            [site.bridge_fire[0], site.bridge_fire[1]],
        ) < 25.0 * 25.0 =>
        {
            game.stop(site.tomahawk)?;
            game.group_force_attack_position(&[site.tomahawk], site.target)?;
            check.phase = 8;
        }
        8 if !game.entities[site.bridge].alive() => {
            require(
                !game.radar_bridge_working(site.layer),
                "real source bridge destruction missing",
            )?;
            check.destroyed = Some(game.tick);
            game.stop(site.tomahawk)?;
            time.pause();
            check.phase = 11;
        }
        12 => {
            time.unpause();
            check.phase = 13;
        }
        _ => {}
    }
    Ok(())
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    radar: Res<'w, Radar>,
    display: Single<'w, 's, (&'static ImageNode, &'static InheritedVisibility), With<Display>>,
    images: Res<'w, Assets<Image>>,
    framing: AircraftFrame<'w, 's>,
    animations: Res<'w, crate::model_animation::Animations>,
    players: Query<
        'w,
        's,
        (
            &'static crate::playtest_view::UnitVisual,
            &'static crate::model_animation::Players,
            Option<&'static Children>,
            Option<&'static GlobalTransform>,
        ),
    >,
    emitter: crate::w3d_emitter::verification::Frame<'w, 's>,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}

fn inspect(mut check: ResMut<Check>, mut view: View) {
    if !matches!(check.phase, 0 | 3 | 5 | 11 | 13) {
        return;
    }
    if !view.capture.ready_for_scenario() {
        let stage = if view.capture.pipelines_ready() {
            "capture handshake (pipelines ready)"
        } else {
            "capture pipeline readiness"
        };
        note_wait(&mut check, stage, view.battle.simulation.tick);
        return;
    }
    if let Err(error) = inspect_scenario(&mut check, &mut view) {
        error!("Original bridge frame phase {}: {error}", check.phase);
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}

fn inspect_scenario(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    let game = &view.battle.simulation;
    require(
        game.entities[check.setup.tomahawk].alive(),
        "paid source Tomahawk died",
    )?;
    let (image, visible) = *view.display;
    if !visible.get() {
        note_wait(check, "radar display visibility", game.tick);
        return Ok(());
    }
    let Some(pixels) = view
        .images
        .get(&image.image)
        .and_then(|image| image.data.as_ref())
    else {
        note_wait(check, "radar CPU image", game.tick);
        return Ok(());
    };
    require(
        view.radar.image.as_ref() == Some(&image.image) && pixels == &view.radar.pixels,
        "displayed radar image differs from actual uploaded radar pixels",
    )?;
    if matches!(check.phase, 3 | 5) {
        inspect_water(check, view)
    } else {
        inspect_bridge(check, view)
    }
}

fn inspect_water(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    let game = &view.battle.simulation;
    match check.phase {
        3 => {
            let (water, _) = check.water.ok_or("missing actual WaterSpray birth")?;
            require(
                game.entities[water].exists(),
                "WaterSpray retired before genuine visible mesh",
            )?;
            if !view.emitter.visible(
                water,
                view.battle.generation,
                view.framing.uncovered_viewport(),
            )? {
                return Ok(());
            }
            check.emitter_id = view.emitter.buffer_id(water, view.battle.generation);
            require(
                check.emitter_id.is_some(),
                "visible WaterSpray has no real buffer identity",
            )?;
            view.time.pause();
            view.capture.intermediate("bridge-hydrant-water.png");
            check.phase = 4;
        }
        5 => {
            let (water, birth) = check.water.ok_or("missing actual WaterSpray birth")?;
            if game.entities[water].exists() {
                return Ok(());
            }
            if !check.tail_seen {
                if !view.emitter.retiring(water, view.battle.generation)? {
                    return Ok(());
                }
                if !view.emitter.visible(
                    water,
                    view.battle.generation,
                    view.framing.uncovered_viewport(),
                )? {
                    return Ok(());
                }
                check.tail_seen = true;
                view.time.pause();
                view.capture.intermediate("bridge-hydrant-tail.png");
                check.phase = 14;
                return Ok(());
            }
            if !view.emitter.retired(
                check
                    .emitter_id
                    .ok_or("missing observed emitter identity")?,
                view.battle.generation,
            ) {
                return Ok(());
            }
            let rules = &game.catalog[game.entities[water].type_index]
                .behavior
                .lifetimes;
            require(
                rules.as_slice() == [[150, 150]] && game.tick >= birth + 150,
                "original five-second WaterSpray lifecycle changed",
            )?;
            view.time.pause();
            view.capture.intermediate("bridge-hydrant-retired.png");
            check.phase = 6;
        }
        _ => {}
    }
    Ok(())
}

fn inspect_bridge(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    let game = &view.battle.simulation;
    match check.phase {
        0 => {
            if !view.framing.visible(&view.battle, check.setup.bridge)? {
                bridge_framing_wait(check, view);
                return Ok(());
            }
            check.radar = radar_samples(&view.radar, game, check.setup.layer);
            require(
                !check.radar.is_empty(),
                "original bridge has no visible radar footprint",
            )?;
            view.time.pause();
            view.capture.intermediate("bridge-intact-radar.png");
            check.phase = 1;
        }
        11 => {
            require(
                game.tick - check.destroyed.ok_or("missing bridge death frame")? <= 90,
                "missed retained native radar interval",
            )?;
            require(
                radar_unchanged(check, &view.radar),
                "radar bridge vanished before native >90-frame refresh",
            )?;
            if !view.framing.visible(&view.battle, check.setup.bridge)? {
                bridge_framing_wait(check, view);
                return Ok(());
            }
            view.capture
                .intermediate("bridge-rubble-radar-retained.png");
            check.phase = 12;
        }
        13 => {
            if game.tick - check.destroyed.ok_or("missing bridge death frame")? <= 90 {
                return Ok(());
            }
            require(
                check
                    .radar
                    .iter()
                    .all(|(index, _)| view.radar.bridges.pixels[index + 3] == 0),
                "native delayed bridge radar footprint remains",
            )?;
            if !view.framing.visible(&view.battle, check.setup.bridge)? {
                bridge_framing_wait(check, view);
                return Ok(());
            }
            if !bridge_animation(view, check.setup.bridge)? {
                return Ok(());
            }
            view.time.pause();
            view.capture.finish_scenario();
            check.phase = 255;
            println!(
                "Original Killing Fields paid force-ground bridge destruction, native delayed radar and visible WaterSpray lifecycle passed; inspect saved pixels."
            );
        }
        _ => {}
    }
    Ok(())
}

fn bridge_animation(view: &View, bridge: usize) -> Result<bool, Box<dyn Error>> {
    let Some((_, players, _, _)) = view
        .players
        .iter()
        .find(|(visual, ..)| visual.index == bridge && visual.generation == view.battle.generation)
    else {
        return Ok(false);
    };
    let actor = &view.battle.simulation.entities[bridge];
    let draws = view.animations.inspect(players, actor.type_index)?;
    let draw = draws
        .iter()
        .find(|draw| draw.module == 0)
        .ok_or("destroyed original bridge has no active collapse animation")?;
    require(
        draw.clip.eq_ignore_ascii_case("CMTBridge_A1.CMTBridge_A1") && draw.mode == "ONCE",
        "bridge did not select its authored rubble animation",
    )?;
    Ok(draw.frame > 0.0 && draw.bounds.is_some() && view.framing.visible(&view.battle, bridge)?)
}

fn radar_samples(radar: &Radar, game: &Skirmish, layer: u8) -> Vec<(usize, [u8; 4])> {
    let mut result = Vec::new();
    for y in 0..128_u16 {
        for x in 0..128_u16 {
            let center = [
                f32::from(x) * radar.world.x / 128.0,
                f32::from(y) * radar.world.y / 128.0,
            ];
            if game
                .radar_bridge_at(center)
                .is_none_or(|(found, _, _)| found != layer)
            {
                continue;
            }
            let index = (usize::from(127 - y) * 128 + usize::from(x)) * 4;
            if radar.bridges.pixels[index + 3] != 0 {
                result.push((
                    index,
                    radar.bridges.pixels[index..index + 4]
                        .try_into()
                        .expect("radar pixel"),
                ));
            }
        }
    }
    result
}

fn radar_unchanged(check: &Check, radar: &Radar) -> bool {
    check
        .radar
        .iter()
        .all(|(index, color)| radar.bridges.pixels[*index..*index + 4] == *color)
}

// Report only transitions between observed blockers; keep original wait paths.
fn note_wait(check: &mut Check, stage: &'static str, tick: u64) -> bool {
    if check.waiting == Some((check.phase, stage)) {
        return false;
    }
    check.waiting = Some((check.phase, stage));
    eprintln!(
        "Original bridge capture waiting: phase={} tick={tick} stage={stage}",
        check.phase
    );
    true
}

fn bridge_framing_wait(check: &mut Check, view: &View) {
    if !note_wait(
        check,
        "bridge geometry/full viewport",
        view.battle.simulation.tick,
    ) {
        return;
    }
    let game = &view.battle.simulation;
    let root = view
        .players
        .iter()
        .find(|(visual, ..)| {
            visual.index == check.setup.bridge && visual.generation == view.battle.generation
        })
        .map(|(_, players, children, world)| {
            (
                players.0.len(),
                children.map(RelationshipTarget::len),
                world.is_some(),
            )
        });
    let shroud = match game.visibility_status(0, check.setup.bridge) {
        generals_sim::skirmish::visibility::Status::Clear => "Clear",
        generals_sim::skirmish::visibility::Status::Partial => "Partial",
        generals_sim::skirmish::visibility::Status::Fogged => "Fogged",
        generals_sim::skirmish::visibility::Status::Shrouded => "Shrouded",
    };
    eprintln!(
        "Original bridge framing wait: shroud={shroud} root[players,children,global]={root:?} framing={:?}",
        view.framing
            .visibility_diagnostic(&view.battle, check.setup.bridge)
    );
}
