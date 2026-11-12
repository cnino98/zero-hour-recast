use super::{Lasers, StreamVisual};
use crate::{playtest::Match, playtest_view::Capture, source_material::SourceMaterial};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

pub fn prepare(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (defender, target) = crate::weapon_verification::prepare_laser(&mut battle.simulation)?;
    let data = app.world().resource::<crate::control_bar::UiData>();
    if !data.units[battle.simulation.entities[defender].type_index]
        .commands
        .iter()
        .any(|command| matches!(command.action, crate::control_bar::CommandAction::LaserLock))
    {
        return Err("Missile Defender lacks the original Laser Lock command button".into());
    }
    battle.select_objects(&[defender], true);
    verify_cursor_ownership(
        &mut app.world_mut().resource_mut::<crate::input::Mouse>(),
        battle,
        defender,
        target,
    )?;
    app.add_systems(
        PostUpdate,
        verify.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

fn verify_cursor_ownership(
    mouse: &mut crate::input::Mouse,
    battle: &mut Match,
    defender: usize,
    target: usize,
) -> Result<(), Box<dyn Error>> {
    use crate::{input::CommandMode, skirmish_verification::require};
    let hovered = mouse.hovered;
    mouse.hovered = Some(target);
    mouse.pending_command = CommandMode::LaserLock;
    require(
        crate::attack_cursor::context(battle, mouse) == Some(2),
        "local Laser Lock cursor did not accept the prepared target",
    )?;
    // Preserve admission/visibility while changing only who controls this pair.
    battle.simulation.entities[defender].owner = 1;
    battle.simulation.entities[target].owner = 0;
    require(
        battle.simulation.can_laser_lock(defender, target),
        "opposing Laser Lock fixture was not otherwise eligible",
    )?;
    let cursor = crate::attack_cursor::context(battle, mouse);
    battle.simulation.entities[defender].owner = 0;
    battle.simulation.entities[target].owner = 1;
    require(
        cursor == Some(3),
        "enemy selection exposed a valid Laser Lock cursor",
    )?;
    mouse.hovered = hovered;
    mouse.pending_command = CommandMode::Normal;
    println!("Laser Lock cursor: valid local candidate and invalid enemy selection verified");
    Ok(())
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Completion<'w> {
    capture: ResMut<'w, Capture>,
    exit: MessageWriter<'w, AppExit>,
    clock: ResMut<'w, Time<Virtual>>,
    time: Res<'w, Time<Real>>,
}

fn verify(
    battle: Res<Match>,
    lasers: Res<Lasers>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<SourceMaterial>>,
    visuals: Query<(
        &StreamVisual,
        &Mesh3d,
        &MeshMaterial3d<SourceMaterial>,
        &ViewVisibility,
    )>,
    mut completion: Completion,
    mut started: Local<Option<f32>>,
) {
    let battle = battle.into_inner();
    let lasers = lasers.into_inner();
    let meshes = meshes.into_inner();
    let materials = materials.into_inner();
    if completion.clock.is_paused() {
        return;
    }
    let now = completion.time.elapsed_secs();
    let start = started.get_or_insert(now);
    if now - *start > 60.0 {
        eprintln!("Laser Lock render diagnostic timed out");
        completion.exit.write(AppExit::error());
        return;
    }
    for (visual, mesh, material, visible) in &visuals {
        if visual.generation != battle.generation || !visible.get() {
            continue;
        }
        let Some(&actor) = battle.selected.first() else {
            continue;
        };
        if battle.simulation.entities[actor].laser_target().is_none() {
            continue;
        }
        let Some(mesh) = meshes.get(&mesh.0) else {
            continue;
        };
        let Some(material) = materials.get(&material.0) else {
            continue;
        };
        let Some(points_source) = lasers.native.active.iter().find_map(|(&object, state)| {
            if state.parent != Some(actor) {
                return None;
            }
            lasers.streams.iter().find_map(|stream| {
                match stream.inspect(object, visual, mesh, material) {
                    Ok(true) => stream.endpoints(object),
                    _ => None,
                }
            })
        }) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        if points.len() != 12
            || mesh.indices().is_none_or(|indices| indices.len() != 18)
            || material.texture.is_none()
            || !matches!(material.alpha_mode, AlphaMode::Add)
            || points.iter().flatten().any(|value| !value.is_finite())
            || Vec3::from_array(points_source[0]).distance(Vec3::from_array(points_source[1]))
                <= 1.0
        {
            eprintln!("Laser Lock render geometry differs from authored three-beam source");
            completion.exit.write(AppExit::error());
            return;
        }
        println!(
            "Laser Lock rendered: original EXLaser texture, 3 additive beam layers and visible source endpoints verified"
        );
        completion.clock.pause();
        completion.capture.finish_scenario();
        break;
    }
}

pub fn prepare_defenses(app: &mut App, battle: &mut Match) -> Result<(), Box<dyn Error>> {
    let (batteries, _, _) = crate::weapon_verification::defenses::prepare(&mut battle.simulation)?;
    battle.select_objects(&batteries, true);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.add_systems(
        PostUpdate,
        verify_defenses.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
    Ok(())
}

fn verify_defenses(
    lasers: Res<Lasers>,
    meshes: Res<Assets<Mesh>>,
    visuals: Query<(&super::streams::StreamVisual, &Mesh3d, &ViewVisibility)>,
    mut completion: Completion,
    mut complete: Local<bool>,
) {
    let lasers = lasers.into_inner();
    let meshes = meshes.into_inner();
    if *complete {
        return;
    }
    for (visual, handle, visible) in &visuals {
        if !visible.get()
            || !lasers
                .streams
                .iter()
                .any(|stream| visual.generation == stream.generation)
        {
            continue;
        }
        let Some(mesh) = meshes.get(&handle.0) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        if points.len() != 80
            || mesh.indices().is_none_or(|indices| indices.len() != 120)
            || points.iter().flatten().any(|point| !point.is_finite())
        {
            eprintln!("Patriot stream lost original twenty-segment geometry");
            completion.exit.write(AppExit::error());
            return;
        }
        println!(
            "Patriot assistance rendered: original EXBinaryStream32 texture, twenty additive arc segments and finite source endpoints"
        );
        completion.clock.pause();
        completion.capture.finish_scenario();
        *complete = true;
        return;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct HackerBeamInspection<'w, 's> {
    camera: Single<
        'w,
        's,
        (&'static Camera, &'static GlobalTransform),
        With<crate::playtest_view::WorldCamera>,
    >,
    lasers: Res<'w, Lasers>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    visuals: Query<
        'w,
        's,
        (
            &'static super::streams::StreamVisual,
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
    >,
}

impl HackerBeamInspection<'_, '_> {
    pub fn visible(&self, actor: usize) -> Result<bool, Box<dyn Error>> {
        let (camera, transform) = *self.camera;
        let Some(viewport) = camera.logical_viewport_rect() else {
            return Ok(false);
        };
        for (&object, state) in &self.lasers.native.active {
            if state.parent != Some(actor) || !state.drawable_present {
                continue;
            }
            if state.points.into_iter().any(|point| {
                let point = Vec3::new(point[0], point[2], -point[1]) * 0.1;
                !camera
                    .world_to_viewport(transform, point)
                    .is_ok_and(|position| viewport.contains(position))
            }) {
                continue;
            }
            for (visual, mesh, material, visible) in &self.visuals {
                if !visible.get() {
                    continue;
                }
                let (Some(mesh), Some(material)) =
                    (self.meshes.get(&mesh.0), self.materials.get(&material.0))
                else {
                    continue;
                };
                for stream in &self.lasers.streams {
                    if stream.inspect(object, visual, mesh, material)? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}
