//! Original cleanup button/radius/voices followed by paid SCUD-field removal.
use super::{RadiusCursor, RadiusVisual};
use crate::{
    cleanup_verification::{self, Progress, Setup},
    control_bar::{CommandAction, UiData},
    input::{CommandMode, Mouse},
    particles::ParticleInspection,
    playtest::{CameraRig, Match},
    playtest_ui::{Action, PromotionIcon},
    playtest_view::{Capture, WorldCamera},
    projectile_stream::ProjectileStreamInspection,
    source_audio::{Voice, cleanup_verification::Inspection},
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    ui_render::ui_material::MaterialNode,
    window::PrimaryWindow,
};
use std::error::Error;

#[derive(Resource)]
struct Check {
    setup: Setup,
    phase: u8,
    point: Option<Vec2>,
    audio: [bool; 2],
    visuals: [bool; 3],
    snapshots: [bool; 2],
    health: f32,
    mode_requested: bool,
    progress: Progress,
    trail_details: Option<String>,
    trail_systems: std::collections::BTreeSet<u64>,
}

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let setup = cleanup_verification::prepare(&mut battle.simulation)?;
    battle.select_objects(&[setup.actor], true);
    let health = setup.health;
    app.insert_resource(Check {
        setup,
        phase: 0,
        point: None,
        audio: [false; 2],
        visuals: [false; 3],
        snapshots: [false; 2],
        health,
        mode_requested: false,
        progress: Progress::new(health),
        trail_details: None,
        trail_systems: std::collections::BTreeSet::new(),
    });
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    Ok(())
}

pub fn register(app: &mut App) {
    let check = app.world().resource::<Check>();
    let game = &app.world().resource::<Match>().simulation;
    let actor = Vec2::from_array(game.entities[check.setup.actor].position);
    let target = Vec2::new(check.setup.position[0], check.setup.position[1]);
    app.world_mut().resource_mut::<CameraRig>().focus = (actor + target) * 0.05;
    app.add_systems(PreUpdate, inspect_audio);
    app.add_systems(FixedUpdate, record.after(crate::particles::step));
    app.add_systems(Update, update.after(super::update));
    app.add_systems(
        PostUpdate,
        inspect.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

fn record(mut check: ResMut<Check>, battle: Res<Match>) {
    let battle = battle.into_inner();
    if check.phase != 7 {
        return;
    }
    let Check {
        progress, setup, ..
    } = check.as_mut();
    progress.observe(&battle.simulation, setup);
}

fn inspect_audio(mut check: ResMut<Check>, battle: Res<Match>, audio: Inspection) {
    let battle = battle.into_inner();
    let actor = check.setup.actor;
    let unit = battle.simulation.entities[actor].type_index;
    for (heard, played) in check.audio.iter_mut().zip(audio.played(actor, unit)) {
        *heard |= played;
    }
}

#[derive(SystemParam)]
struct Scene<'w, 's> {
    battle: ResMut<'w, Match>,
    mouse: Res<'w, Mouse>,
    data: Res<'w, UiData>,
    terrain: Res<'w, Terrain>,
    cursor: Res<'w, RadiusCursor>,
    pointer: Single<'w, 's, &'static Node, With<crate::attack_cursor::AttackPointer>>,
    buttons: Query<
        'w,
        's,
        (
            &'static Action,
            &'static Node,
            &'static UiGlobalTransform,
            &'static Interaction,
            &'static MaterialNode<PromotionIcon>,
        ),
    >,
    window: Single<'w, 's, (Entity, &'static mut Window), With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    visual: Single<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static Visibility,
        ),
        With<RadiusVisual>,
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
    icons: Res<'w, Assets<PromotionIcon>>,
    events: MessageWriter<'w, MouseButtonInput>,
    audio: Inspection<'w, 's>,
}

fn update(
    mut check: ResMut<Check>,
    mut scene: Scene,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let time = time.into_inner();
    if check.phase == 255 {
        return;
    }
    if time.elapsed_secs() > 90.0 {
        error!(
            "Cleanup verification timeout: {}; ready={} pipelines={}",
            details(&check, &scene),
            capture.ready_for_scenario(),
            capture.pipelines_ready()
        );
        exit.write(AppExit::error());
        check.phase = 255;
        return;
    }
    if !capture.ready_for_scenario() {
        return;
    }
    if check.phase >= 7 {
        clock.unpause();
        return;
    }
    clock.pause();
    match step(&mut check, &mut scene, &mut capture) {
        Ok(true) => {
            check.phase += 1;
            eprintln!("Cleanup native input: {}", details(&check, &scene));
            if check.phase == 7 {
                clock.unpause();
            }
        }
        Ok(false) => {}
        Err(error) => {
            error!("Cleanup input verification phase {}: {error}", check.phase);
            exit.write(AppExit::error());
            check.phase = 255;
        }
    }
}

fn step(check: &mut Check, scene: &mut Scene, capture: &mut Capture) -> Result<bool, &'static str> {
    match check.phase {
        0 | 1 => {
            if !aim_button(scene, check.setup.actor)? {
                return Ok(false);
            }
            if check.phase == 1 {
                button(scene, ButtonState::Pressed);
            }
        }
        2 => {
            let unit = scene.battle.simulation.entities[check.setup.actor].type_index;
            scene.audio.verify_silent_mode(unit)?;
            if !scene
                .mouse
                .voices
                .iter()
                .any(|request| matches!(request.voice(), Voice::CleanupMode))
            {
                return Err(
                    "original cleanup button did not request its authored silent mode voice",
                );
            }
            check.mode_requested = true;
            button(scene, ButtonState::Released);
        }
        5 => button(scene, ButtonState::Released),
        3 => {
            if !matches!(scene.mouse.pending_command, CommandMode::Cleanup) {
                return Err("original Ambulance button did not enter cleanup targeting");
            }
            let position = check.setup.position;
            let point = Vec2::new(position[0], position[1]) * 0.1;
            let world = Vec3::new(point.x, scene.terrain.sample(point), -point.y);
            let screen = scene
                .camera
                .0
                .world_to_viewport(scene.camera.1, world)
                .map_err(|_| "cleanup field is outside camera")?;
            let ray = scene
                .camera
                .0
                .viewport_to_world(scene.camera.1, screen)
                .map_err(|_| "cleanup pointer ray missing")?;
            check.point = scene.terrain.ray_point(ray).map(|point| point * 10.0);
            scene.window.1.set_cursor_position(Some(screen));
        }
        4 => {
            visible(
                scene,
                check.point.ok_or("cleanup ground projection missing")?,
            )?;
            if !check.mode_requested {
                return Err("cleanup targeting lost its original silent mode request");
            }
            if !check.snapshots[0] {
                capture.intermediate("cleanup-radius.png");
                check.snapshots[0] = true;
                return Ok(false);
            }
            button(scene, ButtonState::Pressed);
        }
        6 => {
            let expected = check.point.ok_or("cleanup command point missing")?;
            let actor = &scene.battle.simulation.entities[check.setup.actor];
            if !matches!(scene.mouse.pending_command, CommandMode::Normal)
                || actor
                    .cleaning_area()
                    .is_none_or(|point| Vec2::from_array(point).distance(expected) > 0.001)
            {
                return Err("ground click failed to issue the original cleanup area order");
            }
        }
        _ => unreachable!("bounded cleanup input phases"),
    }
    Ok(true)
}

fn details(check: &Check, scene: &Scene) -> String {
    let game = &scene.battle.simulation;
    format!(
        "phase={} frame={} cleanup_targeting={} normal={} mode_request={} mode_audio={} order_audio={} visuals={:?} snapshots={:?} cursor={:?} target={:?} radius_style={} radius={:?} radius_position={:?} radius_hidden={} health={}->{}; {}",
        check.phase,
        game.tick,
        matches!(scene.mouse.pending_command, CommandMode::Cleanup),
        matches!(scene.mouse.pending_command, CommandMode::Normal),
        check.mode_requested,
        check.audio[0],
        check.audio[1],
        check.visuals,
        check.snapshots,
        scene.window.1.cursor_position(),
        check.point,
        scene.cursor.current,
        scene.cursor.radius,
        scene.cursor.position,
        *scene.visual.2 == Visibility::Hidden,
        check.setup.health,
        check.health,
        check.progress.details(game, &check.setup)
    )
}

fn aim_button(scene: &mut Scene, actor: usize) -> Result<bool, &'static str> {
    let command = scene
        .data
        .commands(&scene.battle.simulation, actor)
        .iter()
        .find(|command| matches!(command.action, CommandAction::Cleanup))
        .ok_or("Ambulance cleanup command missing")?;
    let (_, node, transform, interaction, material) = scene
        .buttons
        .iter()
        .find(|(action, _, _, _, _)| matches!(action, Action::Slot(slot) if *slot == command.slot))
        .ok_or("original cleanup button missing")?;
    let expected = scene.data.icon(&command.image);
    if node.display == Display::None
        || scene
            .icons
            .get(&material.0)
            .is_none_or(|icon| icon.image != expected.image || icon.uv_rect != expected.uv_rect)
    {
        return Err("original Ambulance cleanup command artwork is absent");
    }
    let point = transform.translation / scene.window.1.scale_factor();
    let authored =
        scene.data.windows[&format!("ControlBar.wnd:ButtonCommand{:02}", command.slot)].center();
    if point.distance(authored * (scene.window.1.size() / Vec2::new(800.0, 600.0))) > 1.0 {
        return Err("cleanup command differs from the original WND rectangle");
    }
    let ready = *interaction == Interaction::Hovered
        && scene
            .window
            .1
            .cursor_position()
            .is_some_and(|position| position.distance(point) < 1.0);
    scene.window.1.set_cursor_position(Some(point));
    Ok(ready)
}

fn button(scene: &mut Scene, state: ButtonState) {
    scene.events.write(MouseButtonInput {
        button: MouseButton::Left,
        state,
        window: scene.window.0,
    });
}

fn visible(scene: &mut Scene, expected: Vec2) -> Result<(), &'static str> {
    let cursor = &scene.cursor;
    let style = &cursor.styles[1];
    let material = scene
        .materials
        .get(&style.material)
        .ok_or("cleanup radius material missing")?;
    let image = material
        .texture
        .as_ref()
        .and_then(|image| scene.images.get(image))
        .ok_or("cleanup radius image missing")?;
    let mesh = scene
        .meshes
        .get(&cursor.mesh)
        .ok_or("cleanup radius mesh missing")?;
    if cursor.current != 1
        || style.template.texture != "SCCAmbulance"
        || cursor.radius.map(f32::to_bits) != Some(110.0_f32.to_bits())
        || cursor.position.distance(expected) > 0.001
        || *scene.visual.2 == Visibility::Hidden
        || scene.visual.0.0 != cursor.mesh
        || scene.visual.1.0 != style.material
        || scene.pointer.display != Display::Flex
        || crate::attack_cursor::context(&mut scene.battle, &scene.mouse) != Some(5)
        || image.width() == 0
        || material.depth_write
        || material.parameters.vertex_diffuse != 3
        || mesh.indices().is_none_or(|indices| indices.len() < 6)
        || (material.parameters.opacity - style.template.opacity(scene.battle.simulation.tick))
            .abs()
            > f32::EPSILON
    {
        return Err(
            "cleanup radius differs from original SCCAmbulance, 110 range, pulse, target cursor or geometry",
        );
    }
    Ok(())
}

#[derive(SystemParam)]
struct Proof<'w, 's> {
    battle: Res<'w, Match>,
    stream: ProjectileStreamInspection<'w, 's>,
    particles: ParticleInspection<'w, 's>,
}

fn inspect(
    mut check: ResMut<Check>,
    proof: Proof,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    if check.phase != 7 {
        return;
    }
    let Proof {
        battle,
        stream,
        particles,
    } = proof;
    let game = &battle.simulation;
    let systems = particles.cleanup_systems(game, check.progress.projectile_ids());
    check
        .trail_systems
        .extend(systems.into_iter().map(|(system, _)| system));
    if let Some(details) = particles.cleanup_details(&check.trail_systems, battle.generation) {
        check.trail_details = Some(details);
    }
    if !capture.ready_for_scenario() {
        return;
    }
    let result = (|| {
        let actor = check.setup.actor;
        let stream_visible = stream.visible(actor, game.entities[actor].weapon_index(0))?;
        check.visuals[1] |= stream_visible;
        let ids = check.progress.projectile_ids();
        let trail_visible = particles.cleanup_trail(&check.trail_systems, battle.generation)?;
        if !ids.is_empty() {
            check.visuals[0] = true;
            check.visuals[2] |= trail_visible;
        }
        if stream_visible && trail_visible && !check.snapshots[1] {
            check.snapshots[1] = true;
            clock.pause();
            capture.intermediate("cleanup-spray.png");
            return Ok(());
        }
        let field = game
            .hazards
            .iter()
            .find(|field| field.id == check.setup.field);
        check.health = field.map_or(0.0, |field| field.health(&game.entities));
        if game.tick >= check.setup.expires {
            return Err("cleanup field reached natural expiry before verification".into());
        }
        if field.is_none()
            && check.progress.removed_by_damage(&check.setup)
            && check.visuals.into_iter().all(|seen| seen)
            && check.snapshots.into_iter().all(|saved| saved)
            && check.mode_requested
            && check.audio[1]
        {
            check.phase = 255;
            clock.pause();
            capture.finish_scenario();
            println!(
                "Original Ambulance cleanup: paid production/SCUD poison, original button and 110-unit SCCAmbulance target radius, authored silent mode request and advancing object order voice, visible native stream/trail and actual missile field-health removal before expiry passed"
            );
        }
        Ok::<(), Box<dyn Error>>(())
    })();
    if let Err(error) = result {
        error!(
            "Cleanup rendered verification: {error}; mode_request={} audio={:?} visuals={:?} snapshots={:?} last_trail={:?}; {}",
            check.mode_requested,
            check.audio,
            check.visuals,
            check.snapshots,
            check.trail_details,
            check.progress.details(game, &check.setup)
        );
        check.phase = 255;
        exit.write(AppExit::error());
    }
}
