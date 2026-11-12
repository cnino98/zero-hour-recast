//! Fixed original map features, paid scouting/crossing and live weather lifecycle.
mod clearing;
#[cfg(test)]
pub use clearing::test_tree_travel;
mod scene;
mod setup;
use crate::{
    capture_framing,
    map_selection::MapSelection,
    playtest::{CameraRig, Match},
    playtest_view::Capture,
    terrain::Terrain,
    weather_view::WeatherView,
};
use bevy::{camera::visibility::RenderLayers, ecs::system::SystemParam, prelude::*};
pub use scene::Scene;
use std::error::Error;

#[derive(Resource)]
struct Check {
    bridge: bool,
    setup: Option<setup::Setup>,
    preparation: Option<setup::Preparation>,
    preparation_started: Option<f64>,
    elapsed: f64,
    phase: u8,
    started: Option<f64>,
    frame: u64,
    generation: u32,
    weather: (u32, f32, u64),
    site: Option<clearing::Site>,
}

pub const fn map_name(bridge: bool) -> &'static str {
    if bridge {
        "Maps\\Golden Oasis\\Golden Oasis.map"
    } else {
        "Maps\\Eastern Everglades\\Eastern Everglades.map"
    }
}

pub fn prepare(app: &mut App, battle: &mut Match, bridge: bool) -> Result<(), Box<dyn Error>> {
    let preparation = setup::Preparation::begin(battle)?;
    let check = Check {
        bridge,
        setup: None,
        preparation: Some(preparation),
        preparation_started: None,
        elapsed: 0.0,
        site: None,
        phase: 0,
        started: None,
        frame: battle.simulation.tick,
        generation: battle.generation,
        weather: app.world().resource::<WeatherView>().capture_state(),
    };
    app.insert_resource(check);
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    Ok(())
}

pub fn register(app: &mut App) {
    app.add_systems(PreUpdate, begin);
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

#[derive(SystemParam)]
struct Control<'w> {
    battle: ResMut<'w, Match>,
    scene: Res<'w, Scene>,
    objects: Res<'w, crate::playtest_view::WorldObjects>,
    trees: Res<'w, crate::tree_buffer::Trees>,
    clock: Res<'w, Time<Real>>,
    time: ResMut<'w, Time<Virtual>>,
    capture: Res<'w, Capture>,
    maps: ResMut<'w, MapSelection>,
    weather: Res<'w, WeatherView>,
    exit: MessageWriter<'w, AppExit>,
}
fn begin(mut check: ResMut<Check>, mut control: Control) {
    if check.phase == 255 {
        return;
    }
    let now = control.clock.elapsed_secs_f64();
    let previous = check.started.replace(now).unwrap_or(now);
    if let Some(preparation) = &check.preparation {
        let state = preparation.description();
        if now - *check.preparation_started.get_or_insert(now) > 300.0 {
            error!(
                "World paid preparation wall timeout: bridge={} state={state}",
                check.bridge
            );
            check.phase = 255;
            control.exit.write(AppExit::error());
            return;
        }
    } else {
        check.elapsed += now - previous;
    }
    if check.elapsed > 300.0 {
        error!(
            "World native capture timed out bridge={} phase={}",
            check.bridge, check.phase
        );
        check.phase = 255;
        control.exit.write(AppExit::error());
        return;
    }
    if check.preparation.is_none() && !control.capture.ready_for_scenario() {
        return;
    }
    if let Err(error) = advance(&mut check, &mut control) {
        error!("World native capture failed: {error}");
        check.phase = 255;
        control.exit.write(AppExit::error());
    }
}
fn advance(check: &mut Check, control: &mut Control) -> Result<(), Box<dyn Error>> {
    if prepare_frame(check, &mut control.battle, &control.scene)? {
        control.time.unpause();
        return Ok(());
    }
    if check.phase >= 9 {
        return clearing::advance(check, control);
    }
    let setup = check.setup.as_ref().ok_or("world capture not prepared")?;
    if check.phase == 0 || check.phase == 2 {
        if check.bridge && check.phase == 0 {
            control
                .battle
                .simulation
                .move_to_world(setup.tank, setup.destination)?;
            let goal = control.battle.simulation.entities[setup.tank].ai_goal_position();
            let requested = setup.destination;
            if [goal[0], goal[1]].map(f32::to_bits)
                != [requested[0], requested[1]].map(f32::to_bits)
            {
                return Err("bridge crossing staging target was adjusted".into());
            }
        }
        check.phase += 1;
        control.time.unpause();
    } else if !check.bridge && check.phase == 4 {
        check.weather = control.weather.capture_state();
        control.battle.restart();
        check.setup = None;
        check.preparation = Some(setup::Preparation::begin(&mut control.battle)?);
        check.preparation_started = None;
        check.frame = control.battle.simulation.tick;
        check.phase = 5;
        control.time.unpause();
    } else if !check.bridge && check.phase == 6 {
        check.weather = control.weather.capture_state();
        check.generation = control.battle.generation;
        let index = control.maps.current;
        if let Some(reason) = control.maps.maps[index].admission.rejection() {
            return Err(reason.into());
        }
        control.maps.requested = Some(index);
        check.phase = 7;
    } else if check.phase == 7 && control.battle.generation != check.generation {
        check.setup = None;
        check.preparation = Some(setup::Preparation::begin(&mut control.battle)?);
        check.preparation_started = None;
        check.frame = control.battle.simulation.tick;
        check.phase = 8;
        control.time.unpause();
    }
    Ok(())
}
fn prepare_frame(
    check: &mut Check,
    battle: &mut Match,
    scene: &Scene,
) -> Result<bool, Box<dyn Error>> {
    let Some(preparation) = &mut check.preparation else {
        return Ok(false);
    };
    if let Some(setup) = preparation.advance(battle, scene, check.bridge)? {
        check.setup = Some(setup);
        check.preparation = None;
        check.preparation_started = None;
        check.frame = battle.simulation.tick;
    }
    Ok(true)
}

#[cfg(test)]
pub fn test_preparation(world: &mut World) -> Result<bool, Box<dyn Error>> {
    world.resource_scope(|world, mut check: Mut<Check>| {
        let scene = world.remove_resource::<Scene>().expect("actual map Scene");
        let result = prepare_frame(&mut check, &mut world.resource_mut::<Match>(), &scene);
        world.insert_resource(scene);
        result.map(|_| check.preparation.is_none())
    })
}

#[cfg(test)]
pub fn test_camera_frame(world: &mut World, phase: u8) {
    use bevy::ecs::system::RunSystemOnce;
    world.resource_mut::<Check>().phase = phase;
    world.run_system_once(frame).unwrap();
}

fn frame(
    check: Res<Check>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    mut camera: ResMut<CameraRig>,
) {
    let check = check.into_inner();
    let battle = battle.into_inner();
    camera.distance = camera.settings.max_height;
    let focus = if let Some(setup) = &check.setup {
        // Screenshot readback continues after inspection enters the terminal phase.
        if check.bridge && matches!(check.phase, 3 | 255) {
            battle.simulation.object_position(setup.tank)
        } else {
            setup.focus
        }
    } else if let Some(preparation) = &check.preparation {
        preparation.focus(&battle.simulation)
    } else {
        return;
    };
    capture_framing::focus_on(&mut camera, terrain.into_inner(), focus);
}

#[derive(SystemParam)]
struct View<'w, 's> {
    battle: Res<'w, Match>,
    weather: Res<'w, WeatherView>,
    framing: capture_framing::AircraftFrame<'w, 's>,
    meshes: Res<'w, Assets<Mesh>>,
    trees: crate::tree_buffer::verification::Frame<'w, 's>,
    visuals: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static RenderLayers,
            &'static ViewVisibility,
        ),
    >,
    capture: ResMut<'w, Capture>,
    time: ResMut<'w, Time<Virtual>>,
    exit: MessageWriter<'w, AppExit>,
}
fn inspect(mut check: ResMut<Check>, mut view: View) {
    if check.preparation.is_some()
        || !matches!(check.phase, 1 | 3 | 5 | 8 | 10 | 12 | 14 | 16 | 18)
        || !view.capture.ready_for_scenario()
    {
        return;
    }
    if let Err(error) = inspect_frame(&mut check, &mut view) {
        error!("World frame verification failed: {error}");
        check.phase = 255;
        view.exit.write(AppExit::error());
    }
}
fn inspect_frame(check: &mut Check, view: &mut View) -> Result<(), Box<dyn Error>> {
    if check.phase >= 9 {
        return clearing::inspect(check, view);
    }
    let game = &view.battle.simulation;
    let setup = check.setup.as_ref().ok_or("world frame not prepared")?;
    if !game.entities[setup.tank].alive() {
        return Err("paid world scouting tank died".into());
    }
    if !view.framing.visible(&view.battle, setup.tank)? {
        return Ok(());
    }
    let has_layer = |layer| {
        view.visuals.iter().any(|(mesh, layers, visible)| {
            visible.get()
                && layers.intersects(&RenderLayers::layer(layer))
                && view
                    .meshes
                    .get(&mesh.0)
                    .is_some_and(|mesh| mesh.count_vertices() > 0)
        })
    };
    if !has_layer(6) {
        return Ok(());
    }
    if check.bridge {
        let actor = &game.entities[setup.tank];
        let middle = Vec2::new(setup.focus[0], setup.focus[1]);
        let on_middle = Vec2::from_array(actor.position).distance_squared(middle) < 30.0 * 30.0;
        if check.phase == 1 && actor.layer == setup.layer && on_middle {
            view.time.pause();
            view.capture.intermediate("world-bridge-crossing.png");
            check.phase = 2;
        } else if check.phase == 3 && actor.layer == 1 {
            let delta = Vec2::from_array(actor.position)
                - Vec2::new(setup.destination[0], setup.destination[1]);
            if delta.length_squared() < 30.0 * 30.0 {
                view.time.pause();
                view.capture.finish_scenario();
                check.phase = 255;
                println!(
                    "Original bridge paid crossing PASSED; inspect saved original bridge, road and water pixels."
                );
            }
        }
        return Ok(());
    }
    if !has_layer(9) || game.tick < check.frame + 30 {
        return Ok(());
    }
    let state = view.weather.capture_state();
    let name = match check.phase {
        1 => "world-rain-initial.png",
        3 => {
            if game.map_script_fade().is_none_or(|fade| fade.amount < 0.1) {
                return Ok(());
            }
            "world-rain-lightning.png"
        }
        5 | 8 => {
            if state.0 == check.weather.0 || state.2 == check.weather.2 {
                return Err(
                    "original weather noise stream did not advance on restart/reload".into(),
                );
            }
            if check.phase == 5 {
                "world-rain-restarted.png"
            } else {
                "world-rain.png"
            }
        }
        _ => return Ok(()),
    };
    println!(
        "Original rain capture phase={} tick={} generation={} weather={state:?}",
        check.phase, game.tick, view.battle.generation
    );
    view.time.pause();
    view.capture.intermediate(name);
    check.phase += 1;
    Ok(())
}
