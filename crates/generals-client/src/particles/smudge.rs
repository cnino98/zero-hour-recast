//! `ParticleSys` update snapshots and `W3DSmudge` five-vertex scene distortion.
use super::Particles;
use crate::{playtest::Match, playtest_view::WorldCamera};
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::visibility::{NoFrustumCulling, RenderLayers},
    ecs::system::SystemParam,
    mesh::{Indices, MeshVertexBufferLayoutRef},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, PrimitiveTopology,
        RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

pub struct SmudgePlugin;
impl Plugin for SmudgePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "smudge.wgsl");
        app.add_plugins(MaterialPlugin::<SceneMaterial>::default())
            .add_systems(
                Update,
                update.after(super::update).run_if(resource_exists::<Match>),
            );
    }
}

// Material assets require Asset, TypePath, AsBindGroup and Clone.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(SmudgeKey)]
pub struct SceneMaterial {
    #[uniform(0)]
    pub smudge: u32,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
}

// Pipeline cache keys require equality, hashing and cloning.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SmudgeKey(bool);
impl From<&SceneMaterial> for SmudgeKey {
    fn from(material: &SceneMaterial) -> Self {
        Self(material.smudge != 0)
    }
}

impl Material for SceneMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://generals_client/particles/smudge.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn reads_view_transmission_texture(&self) -> bool {
        self.smudge != 0
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.smudge != 0 {
            AlphaMode::Opaque
        } else {
            AlphaMode::Add
        }
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(false);
        }
        // Classification stays transmissive so Bevy snapshots the scene once.
        // The fragment shader computes the original encoded-color equation.
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(blend(key.bind_group_data.0));
            }
        }
        Ok(())
    }
}

const fn blend(smudge: bool) -> BlendState {
    if smudge {
        BlendState::ALPHA_BLENDING
    } else {
        BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        }
    }
}

pub(super) struct Smudge {
    pub system: u64,
    pub position: [f32; 3],
    pub size: f32,
    pub opacity: f32,
    pub offset: [f32; 2],
}

#[derive(Component)]
pub(super) struct SmudgeVisual {
    pub generation: u32,
    pub ranges: Vec<(u64, std::ops::Range<usize>)>,
}

#[derive(SystemParam)]
struct Resources<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SceneMaterial>>,
}

fn update(
    mut commands: Commands,
    battle: Res<Match>,
    particles: Res<Particles>,
    camera: Single<(&Transform, &Projection), With<WorldCamera>>,
    resources: Resources,
    visuals: Query<(Entity, &Mesh3d), With<SmudgeVisual>>,
    mut material: Local<Option<Handle<SceneMaterial>>>,
) {
    let battle = battle.into_inner();
    let particles = particles.into_inner();
    let (transform, projection) = camera.into_inner();
    let Resources {
        mut meshes,
        mut materials,
    } = resources;
    let geometry = if battle.started {
        mesh(
            &particles.smudges,
            transform,
            projection.get_clip_from_view(),
        )
    } else {
        None
    };
    let Some((mesh, ranges)) = geometry else {
        for (entity, mesh) in &visuals {
            commands.entity(entity).despawn();
            meshes.remove(&mesh.0);
        }
        return;
    };
    let visual = SmudgeVisual {
        generation: battle.generation,
        ranges,
    };
    if let Some((entity, handle)) = visuals.iter().next() {
        commands.entity(entity).insert(visual);
        if let Some(mut current) = meshes.get_mut(&handle.0) {
            *current = mesh;
        }
    } else {
        let material = material.get_or_insert_with(|| {
            materials.add(SceneMaterial {
                smudge: 1,
                texture: None,
            })
        });
        commands.spawn((
            visual,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::IDENTITY,
            RenderLayers::layer(4),
            NoFrustumCulling,
        ));
    }
}

type SmudgeRanges = Vec<(u64, std::ops::Range<usize>)>;

fn mesh(smudges: &[Smudge], camera: &Transform, projection: Mat4) -> Option<(Mesh, SmudgeRanges)> {
    let world = camera.compute_affine();
    let inverse = world.inverse();
    let mut positions = Vec::new();
    let mut uv = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let mut ranges: SmudgeRanges = Vec::new();
    for smudge in smudges {
        let point = smudge.position;
        let center = inverse.transform_point3(Vec3::new(point[0], point[2], -point[1]) * 0.1);
        let corners = [
            Vec3::new(-0.5, 0.5, 0.0),
            Vec3::new(-0.5, -0.5, 0.0),
            Vec3::new(0.5, -0.5, 0.0),
            Vec3::new(0.5, 0.5, 0.0),
        ];
        let corners = corners.map(|offset| center + offset * (smudge.size * 0.1));
        let clip = corners.map(|point| projection * point.extend(1.0));
        if clip.iter().any(|point| point.w <= 0.0) {
            continue;
        }
        let corners_uv = clip.map(|point| {
            [
                (point.x / point.w + 1.0) * 0.5,
                (1.0 - point.y / point.w) * 0.5,
            ]
        });
        if (0..2).any(|axis| {
            corners_uv.iter().all(|uv| uv[axis] < 0.0) || corners_uv.iter().all(|uv| uv[axis] > 1.0)
        }) {
            continue;
        }
        let center_uv = center_uv(corners_uv, smudge.offset);
        let start = positions.len();
        if let Some((system, range)) = ranges.last_mut()
            && *system == smudge.system
        {
            range.end += 5;
        } else {
            ranges.push((smudge.system, start..start + 5));
        }
        let base = u32::try_from(positions.len()).expect("smudge vertex count fits u32");
        positions.extend(
            corners
                .into_iter()
                .chain([center])
                .map(|point| world.transform_point3(point).to_array()),
        );
        uv.extend(corners_uv.into_iter().chain([center_uv]));
        colors.extend([[1.0, 238.0 / 255.0, 221.0 / 255.0, 0.0]; 4]);
        colors.push([
            1.0,
            238.0 / 255.0,
            221.0 / 255.0,
            (smudge.opacity.clamp(0.0, 1.0) * 255.0).trunc() / 255.0,
        ]);
        indices.extend([0, 4, 3, 3, 4, 2, 2, 4, 1, 1, 4, 0].map(|index| base + index));
    }
    // W3DSmudge renders only admitted screen fans. Do not submit an empty
    // indexed mesh: Bevy skips its allocation but still attempts both uploads.
    if positions.is_empty() {
        return None;
    }
    let count = positions.len();
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    Some((mesh, ranges))
}

fn center_uv(corners: [[f32; 2]; 4], mut offset: [f32; 2]) -> [f32; 2] {
    for axis in 0..2 {
        if corners.iter().any(|uv| !(0.0..=1.0).contains(&uv[axis])) {
            offset[axis] = 0.0;
        }
    }
    let x = (corners[3][0] - corners[0][0]) * (0.5 + offset[0]);
    let y = (corners[1][1] - corners[0][1]) * (0.5 + offset[1]);
    [corners[0][0] + x, corners[0][1] + y]
}

#[cfg(test)]
mod tests;
