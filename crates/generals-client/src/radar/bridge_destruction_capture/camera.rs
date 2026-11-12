//! Full original bridge observation, using its currently retained rendered parts.
use crate::{
    capture_framing::AircraftFrame,
    playtest::{CameraRig, Match},
    playtest_view::{UnitVisual, WorldCamera},
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

#[derive(SystemParam)]
pub(super) struct Geometry<'w, 's> {
    particles: Res<'w, crate::particles::Particles>,
    framing: AircraftFrame<'w, 's>,
    camera: Single<'w, 's, &'static Camera, With<WorldCamera>>,
    roots: Query<'w, 's, (&'static UnitVisual, &'static Children)>,
    parts: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static GlobalTransform,
            &'static InheritedVisibility,
            &'static MeshMaterial3d<SourceMaterial>,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
}

impl Geometry<'_, '_> {
    pub(super) fn fit(
        self,
        battle: &Match,
        bridge: usize,
        terrain: &Terrain,
        rig: &mut CameraRig,
    ) -> Result<(), Box<dyn Error>> {
        let (Some(viewport), Some(uncovered)) = (
            self.camera.logical_viewport_rect(),
            self.framing.uncovered_viewport(),
        ) else {
            return Ok(());
        };
        let Some((_, children)) = self
            .roots
            .iter()
            .find(|(visual, _)| visual.index == bridge && visual.generation == battle.generation)
        else {
            return Ok(());
        };
        let mut points = Vec::new();
        for child in children {
            let Ok((mesh, world, inherited, material)) = self.parts.get(*child) else {
                continue;
            };
            if !inherited.get() {
                continue;
            }
            let material = self
                .materials
                .get(&material.0)
                .ok_or("original bridge material missing")?;
            if !material.parameters.opacity.is_finite() {
                return Err("original bridge opacity is nonfinite".into());
            }
            if material.parameters.opacity <= 0.0 {
                continue;
            }
            let mesh = self
                .meshes
                .get(&mesh.0)
                .ok_or("original bridge mesh missing")?;
            let Some(VertexAttributeValues::Float32x3(vertices)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("original bridge positions missing".into());
            };
            let indices: Vec<_> = mesh.indices().map_or_else(
                || (0..vertices.len()).collect(),
                |indices| indices.iter().collect(),
            );
            let solid = indices.chunks_exact(3).any(|triangle| {
                let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                    .map(|index| Vec3::from_array(vertices[index]));
                (b - a).cross(c - a).length_squared() > 0.0
            });
            if !solid {
                continue;
            }
            for vertex in vertices {
                let point = world.transform_point(Vec3::from_array(*vertex));
                if !point.is_finite() {
                    return Err("original bridge world geometry is nonfinite".into());
                }
                points.push(point);
            }
        }
        if points.is_empty() {
            return Ok(());
        }
        let shaken = rig.focus + self.particles.camera_shake();
        let focus = Vec3::new(shaken.x, terrain.sample(rig.focus), -shaken.y);
        let distance = minimum_distance(&points, focus, rig, viewport, uncovered, &self.camera)?;
        rig.distance = distance;
        Ok(())
    }
}

fn minimum_distance(
    points: &[Vec3],
    focus: Vec3,
    rig: &CameraRig,
    viewport: Rect,
    uncovered: Rect,
    camera: &Camera,
) -> Result<f32, Box<dyn Error>> {
    let projection = camera.computed.clip_from_view;
    if !projection.is_finite()
        || projection.x_axis.x <= 0.0
        || projection.y_axis.y <= 0.0
        || projection.z_axis.w.to_bits() != (-1.0_f32).to_bits()
        || projection.w_axis.w.to_bits() != 0.0_f32.to_bits()
    {
        return Err("original bridge observation needs a finite perspective".into());
    }
    let centre = viewport.center();
    let left = centre.x - uncovered.min.x - 12.0;
    let right = uncovered.max.x - 12.0 - centre.x;
    let top = centre.y - uncovered.min.y - 12.0;
    let bottom = uncovered.max.y - 12.0 - centre.y;
    if [left, right, top, bottom]
        .into_iter()
        .any(|room| !room.is_finite() || room <= 0.0)
    {
        return Err("original bridge observation has no finite viewport room".into());
    }
    let focal_x = viewport.width() * 0.5 * projection.x_axis.x;
    let focal_y = viewport.height() * 0.5 * projection.y_axis.y;
    let (sin_pitch, cos_pitch) = rig.settings.pitch.sin_cos();
    let (sin_yaw, cos_yaw) = rig.settings.yaw.sin_cos();
    let mut distance = rig.distance;
    for point in points {
        let local = *point - focus;
        let x = cos_yaw.mul_add(local.x, -sin_yaw * local.z);
        let back = sin_yaw.mul_add(local.x, cos_yaw * local.z);
        let base = (-sin_pitch).mul_add(local.y, -cos_pitch * back);
        let up = cos_pitch.mul_add(local.y, -sin_pitch * back);
        let width = x.abs() * focal_x / if x < 0.0 { left } else { right };
        let height = up.abs() * focal_y / if up < 0.0 { bottom } else { top };
        distance = distance.max(sin_pitch * (width.max(height) - base));
    }
    // Use the same affine inverse, separate projection and viewport conversion
    // as AircraftFrame. A precomposed Mat4 rounds differently at the border.
    // Retain the bounded outward rounding and original viewport guards.
    for _ in 0..64 {
        if !distance.is_finite() {
            return Err("original bridge cannot fit a finite observation distance".into());
        }
        let horizontal = distance / rig.settings.pitch.tan();
        let transform = Transform::from_translation(
            focus + Vec3::new(horizontal * sin_yaw, distance, horizontal * cos_yaw),
        )
        .looking_at(focus, Vec3::Y);
        let world = GlobalTransform::from(transform);
        let mut inside = true;
        for point in points {
            let Ok(pixel) = camera.world_to_viewport(&world, *point) else {
                inside = false;
                break;
            };
            if !pixel.is_finite()
                || pixel.cmplt(uncovered.min + Vec2::splat(12.0)).any()
                || pixel.cmpgt(uncovered.max - Vec2::splat(12.0)).any()
            {
                inside = false;
                break;
            }
        }
        if inside {
            return Ok(distance);
        }
        distance = distance.next_up();
    }
    Err("original bridge f32 projection did not converge within 64 outward ULPs".into())
}
