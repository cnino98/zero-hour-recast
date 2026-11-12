//! Shared original transparent triangle pool, with reusable material draw runs.
mod draw;
mod geometry;
mod materials;
mod order;
#[cfg(test)]
mod storage_tests;
pub mod submission;
#[cfg(test)]
mod tests;
use crate::{
    model_render::RenderOrder, playtest_view::WorldCamera, source_material::SourceMaterial,
};
use bevy::{
    camera::{
        primitives::{Aabb, Frustum, MeshAabb},
        visibility::{NoFrustumCulling, RenderLayers, SetViewVisibility, VisibilitySystems},
    },
    ecs::system::SystemParam,
    prelude::*,
    transform::TransformSystems,
};
use std::{collections::HashMap, ops::Range, sync::Arc};

pub struct SortedPlugin;
impl Plugin for SortedPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Pool {
            vertices: Vec::new(),
            triangles: Vec::new(),
            runs: Vec::new(),
            mesh: None,
            variants: HashMap::new(),
            registry: submission::Registry::new(),
            sequence: 0,
            instances: HashMap::new(),
        });
        observe_sources(app);
        order::register(app);
        draw::register(app);
        app.add_systems(
            PostUpdate,
            publish_visibility
                .run_if(resource_exists::<crate::playtest::Match>)
                .in_set(VisibilitySystems::CheckVisibility)
                .after(bevy::camera::visibility::check_visibility_cpu_culling)
                .after(bevy::camera::visibility::check_visibility_gpu_culling),
        );
        app.add_systems(
            PostUpdate,
            update
                .run_if(resource_exists::<crate::playtest::Match>)
                .after(TransformSystems::Propagate)
                .after(VisibilitySystems::VisibilityPropagate)
                .after(VisibilitySystems::UpdateFrusta)
                .before(VisibilitySystems::CheckVisibility),
        );
    }
}

fn observe_sources(app: &mut App) {
    app.add_observer(
        |event: On<Add, MeshMaterial3d<SourceMaterial>>,
         commands: Commands,
         pool: ResMut<Pool>,
         instances: Query<&RenderInstance>| {
            register_source(event.entity, commands, pool, instances);
        },
    );
}

#[derive(Component)]
pub struct SourceScale(pub f32);
pub fn source_scale(scale: f32) -> f32 {
    if (scale - 1.0).abs() > 0.01 {
        scale
    } else {
        1.0
    }
}

#[derive(Component, Clone)]
pub struct RenderInstance(Arc<()>);
impl RenderInstance {
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}

#[derive(Component)]
struct SortedSource;
#[derive(Component)]
struct SubmissionSequence(u64);
fn register_source(
    entity: Entity,
    mut commands: Commands,
    mut pool: ResMut<Pool>,
    instances: Query<&RenderInstance>,
) {
    let sequence = pool.sequence;
    pool.sequence += 1;
    // W3DModelDraw::setModelState adds a new scene object when it replaces a
    // model. All parts of that one HLOD share membership; mere hide/show and
    // animated vertex updates retain it. Entity slot reuse is irrelevant.
    let sequence = instances.get(entity).map_or(sequence, |instance| {
        pool.instances
            .entry(Arc::as_ptr(&instance.0) as usize)
            .or_insert_with(|| (instance.0.clone(), sequence))
            .1
    });
    commands.entity(entity).insert(SubmissionSequence(sequence));
}
struct Node {
    mesh: Handle<Mesh>,
    material: Handle<SourceMaterial>,
    world: bevy::math::Affine3A,
    scale: f32,
    depth: f32,
    sequence: u64,
    submission: Option<submission::ModelSubmission>,
    particle: bool,
    unbounded: bool,
}

#[derive(Component)]
pub struct SortedBatch;

#[derive(Component)]
struct SortedRange(Range<u32>);
struct Run {
    entity: Entity,
    range: Range<u32>,
    material: Handle<SourceMaterial>,
}
impl Run {
    fn show(
        &mut self,
        commands: &mut Commands,
        range: Range<u32>,
        material: Handle<SourceMaterial>,
    ) {
        if self.range.is_empty() {
            commands.entity(self.entity).insert((
                Visibility::Visible,
                InheritedVisibility::VISIBLE,
                RenderLayers::layer(3),
            ));
        }
        if self.range != range {
            commands
                .entity(self.entity)
                .insert(SortedRange(range.clone()));
            self.range = range;
        }
        if self.material != material {
            commands
                .entity(self.entity)
                .insert(MeshMaterial3d(material.clone()));
            self.material = material;
        }
    }

    fn hide(&mut self, commands: &mut Commands) {
        self.range = 0..0;
        commands.entity(self.entity).insert((
            SortedRange(0..0),
            Visibility::Hidden,
            InheritedVisibility::HIDDEN,
            RenderLayers::none(),
        ));
    }
}

#[derive(Resource)]
struct Pool {
    vertices: Vec<geometry::Vertex>,
    triangles: Vec<geometry::Triangle>,
    runs: Vec<Run>,
    mesh: Option<Handle<Mesh>>,
    variants: HashMap<AssetId<SourceMaterial>, materials::Variant>,
    registry: submission::Registry,
    sequence: u64,
    instances: HashMap<usize, (Arc<()>, u64)>,
}
type Source = (
    Entity,
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static GlobalTransform,
    &'static InheritedVisibility,
    &'static mut RenderLayers,
    Option<&'static SortedSource>,
    Option<&'static RenderOrder>,
    Option<&'static crate::particles::ParticleVisual>,
    &'static SubmissionSequence,
    Option<&'static submission::ModelSubmission>,
    Option<&'static NoFrustumCulling>,
    Option<&'static ChildOf>,
);
#[derive(SystemParam)]
struct Resources<'w, 's> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    batches: Query<'w, 's, Entity, With<SortedBatch>>,
    scales: Query<'w, 's, &'static SourceScale>,
    sources: Query<'w, 's, Source, Without<SortedBatch>>,
}

fn update(
    mut commands: Commands,
    camera: Single<(&GlobalTransform, &Frustum), With<WorldCamera>>,
    mut pool: ResMut<Pool>,
    mut resources: Resources,
) {
    let pool = pool.as_mut();
    if pool
        .runs
        .iter()
        .any(|run| resources.batches.get(run.entity).is_err())
    {
        pool.registry = submission::Registry::new();
        pool.instances
            .retain(|_, (identity, _)| Arc::strong_count(identity) > 1);
        for run in pool.runs.drain(..) {
            if resources.batches.get(run.entity).is_ok() {
                commands.entity(run.entity).despawn();
            }
        }
        if let Some(mesh) = pool.mesh.take() {
            resources.meshes.remove(&mesh);
        }
        for (_, variant) in pool.variants.drain() {
            resources.materials.remove(&variant.handle);
        }
    }
    pool.vertices.clear();
    pool.triangles.clear();
    let (camera, frustum) = camera.into_inner();
    let view = camera.affine().inverse();
    let nodes = collect_nodes(&mut commands, &mut resources, view, frustum);
    let ordered = submission::order(&nodes, &mut pool.registry, &resources.materials);
    for index in ordered {
        let node = &nodes[index];
        let source = resources
            .meshes
            .get(&node.mesh)
            .expect("sorted source mesh exists");
        geometry::append(
            source,
            node.world,
            view,
            index,
            &mut pool.vertices,
            &mut pool.triangles,
        );
    }
    geometry::sort(&mut pool.triangles);
    upload_runs(pool, &nodes, &mut commands, &mut resources);
}

fn collect_nodes(
    commands: &mut Commands,
    resources: &mut Resources,
    view: bevy::math::Affine3A,
    frustum: &Frustum,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (
        entity,
        mesh,
        material,
        world,
        visible,
        mut layers,
        marked,
        order,
        particle,
        sequence,
        submission,
        no_cull,
        parent,
    ) in &mut resources.sources
    {
        let requested = order.map_or_else(
            || {
                layers.intersects(&RenderLayers::layer(3))
                    || marked.is_some() && *layers == RenderLayers::none()
            },
            |order| {
                resources
                    .materials
                    .get(&material.0)
                    .is_some_and(|material| order.layer(material.parameters.opacity) == 3)
            },
        );
        if !requested {
            if marked.is_some() {
                commands.entity(entity).remove::<SortedSource>();
                if let Some(order) = order {
                    let opacity = resources
                        .materials
                        .get(&material.0)
                        .expect("source material exists")
                        .parameters
                        .opacity;
                    *layers = RenderLayers::layer(order.layer(opacity));
                }
            }
            continue;
        }
        if marked.is_none() {
            commands.entity(entity).insert(SortedSource);
        }
        *layers = RenderLayers::none();
        let bound = source_bounds(resources.meshes.get(&mesh.0), submission, world.affine());
        if visible.get()
            && (no_cull.is_some() || source_in_frustum(bound, submission, world.affine(), frustum))
        {
            let scale = parent
                .and_then(|parent| resources.scales.get(parent.parent()).ok())
                .map_or(1.0, |scale| scale.0);
            // PointGroup has no bound. Multipoint StreakLine uses its sphere.
            let unbounded = particle.is_some_and(|visual| !visual.bounded)
                || submission.is_some_and(|submission| submission.mesh.sphere_radius <= 0.0);
            let depth = if unbounded {
                0.0
            } else {
                submission.map_or_else(
                    || bound.map_or(0.0, |bounds| view.transform_point3(bounds.center.into()).z),
                    |submission| {
                        view.transform_point3(submission.sort_center(world.affine(), scale))
                            .z
                    },
                )
            };
            nodes.push(Node {
                mesh: mesh.0.clone(),
                material: material.0.clone(),
                world: world.affine(),
                scale,
                depth,
                sequence: particle.map_or(sequence.0, |visual| visual.system),
                submission: submission.cloned(),
                particle: particle.is_some(),
                unbounded,
            });
        }
    }
    nodes
}

// Sorted outputs use identity transforms and override both phase and sort bias.
// Compare every remaining uniform/pipeline/texture bit before sharing a draw.
fn interchangeable_materials(left: &SourceMaterial, right: &SourceMaterial) -> bool {
    let SourceMaterial {
        parameters,
        texture,
        shroud,
        alpha_mode,
        double_sided,
        depth_write,
        depth_compare,
        force_sorted_phase: _,
        depth_bias: _,
    } = left;
    let crate::source_material::Parameters {
        ambient,
        diffuse,
        emissive,
        scene_ambient,
        light_color,
        light_direction,
        point_position,
        point_ambient,
        terrain_lights,
        alpha_test,
        vertex_diffuse,
        opacity,
        primary_gradient,
        shroud_extent,
    } = parameters;
    let bits = |value: Vec4| value.to_array().map(f32::to_bits);
    let b = &right.parameters;
    let alpha = match (*alpha_mode, right.alpha_mode) {
        (AlphaMode::Mask(a), AlphaMode::Mask(b)) => a.to_bits() == b.to_bits(),
        (a, b) => a == b,
    };
    alpha
        && texture == &right.texture
        && shroud == &right.shroud
        && *double_sided == right.double_sided
        && *depth_write == right.depth_write
        && *depth_compare == right.depth_compare
        && bits(*ambient) == bits(b.ambient)
        && bits(*diffuse) == bits(b.diffuse)
        && bits(*emissive) == bits(b.emissive)
        && bits(*scene_ambient) == bits(b.scene_ambient)
        && light_color.map(bits) == b.light_color.map(bits)
        && light_direction.map(bits) == b.light_direction.map(bits)
        && point_position.map(bits) == b.point_position.map(bits)
        && point_ambient.map(bits) == b.point_ambient.map(bits)
        && *terrain_lights == b.terrain_lights
        && alpha_test.to_bits() == b.alpha_test.to_bits()
        && *vertex_diffuse == b.vertex_diffuse
        && opacity.to_bits() == b.opacity.to_bits()
        && *primary_gradient == b.primary_gradient
        && bits(*shroud_extent) == bits(b.shroud_extent)
}

fn sorted_ranges(
    triangles: &[geometry::Triangle],
    nodes: &[Node],
    materials: &Assets<SourceMaterial>,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut first = 0;
    while first < triangles.len() {
        let node = triangles[first].node;
        let mut end = first + 1;
        while end < triangles.len() {
            let next = triangles[end].node;
            if nodes[node].material != nodes[next].material
                && !interchangeable_materials(
                    materials
                        .get(&nodes[node].material)
                        .expect("sorted source material exists"),
                    materials
                        .get(&nodes[next].material)
                        .expect("sorted source material exists"),
                )
            {
                break;
            }
            end += 1;
        }
        ranges.push(first..end);
        first = end;
    }
    ranges
}

fn upload_runs(
    pool: &mut Pool,
    nodes: &[Node],
    commands: &mut Commands,
    resources: &mut Resources,
) {
    let ranges = sorted_ranges(&pool.triangles, nodes, &resources.materials);
    materials::begin_frame(&mut pool.variants, &resources.materials);
    if !ranges.is_empty() {
        let mesh = pool
            .mesh
            .get_or_insert_with(|| resources.meshes.add(geometry::empty_mesh()));
        geometry::fill_pool(
            &mut resources
                .meshes
                .get_mut(&*mesh)
                .expect("shared sorted mesh exists"),
            &pool.vertices,
            &pool.triangles,
        );
    }
    for (run_index, range) in ranges.iter().enumerate() {
        let node = pool.triangles[range.start].node;
        let material = materials::bind(
            &mut pool.variants,
            &nodes[node].material,
            &mut resources.materials,
        );
        let vertices = u32::try_from(range.start * 3).expect("sorted vertex offset")
            ..u32::try_from(range.end * 3).expect("sorted vertex end");
        if run_index == pool.runs.len() {
            let entity = commands
                .spawn((
                    SortedBatch,
                    SortedRange(vertices.clone()),
                    bevy::render::batching::NoAutomaticBatching,
                    Mesh3d(pool.mesh.as_ref().expect("nonempty sorted pool").clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    GlobalTransform::IDENTITY,
                    Visibility::Visible,
                    InheritedVisibility::VISIBLE,
                    ViewVisibility::default(),
                    RenderLayers::layer(3),
                    NoFrustumCulling,
                ))
                .id();
            pool.runs.push(Run {
                entity,
                material,
                range: vertices,
            });
        } else {
            pool.runs[run_index].show(commands, vertices, material);
        }
    }
    // Live draws form a prefix. The first empty region begins the previously hidden tail.
    for run in &mut pool.runs[ranges.len()..] {
        if run.range.is_empty() {
            break;
        }
        run.hide(commands);
    }
}

type VisibleSource = (
    &'static Mesh3d,
    &'static GlobalTransform,
    &'static InheritedVisibility,
    &'static mut ViewVisibility,
    Option<&'static NoFrustumCulling>,
    Option<&'static submission::ModelSubmission>,
);

// Source entities remain the pick/inspection owners while their triangles draw
// through pooled batches. Setting this after visibility extraction does not add
// the originals to a camera render list or duplicate their draw.
fn publish_visibility(
    camera: Single<&Frustum, With<WorldCamera>>,
    meshes: Res<Assets<Mesh>>,
    mut sources: Query<VisibleSource, With<SortedSource>>,
) {
    let camera = camera.into_inner();
    let meshes = meshes.into_inner();
    for (mesh, world, inherited, mut visible, no_cull, submission) in &mut sources {
        if inherited.get()
            && (no_cull.is_some()
                || source_in_frustum(
                    source_bounds(meshes.get(&mesh.0), submission, world.affine()),
                    submission,
                    world.affine(),
                    camera,
                ))
        {
            visible.set_visible();
        }
    }
}

// MeshClass culls its world axis-aligned box, shared by all material parts.
fn source_bounds(
    mesh: Option<&Mesh>,
    submission: Option<&submission::ModelSubmission>,
    world: bevy::math::Affine3A,
) -> Option<Aabb> {
    let (bounds, transform) = if let Some(submission) = submission {
        (submission.mesh.bounds, world * submission.bound_transform)
    } else {
        (mesh.and_then(Mesh::compute_aabb)?, world)
    };
    Some(transform_bounds(bounds, transform))
}

fn transform_bounds(bounds: Aabb, transform: bevy::math::Affine3A) -> Aabb {
    Aabb {
        center: transform.transform_point3a(bounds.center),
        half_extents: transform.matrix3.abs() * bounds.half_extents,
    }
}
fn source_in_frustum(
    bounds: Option<Aabb>,
    submission: Option<&submission::ModelSubmission>,
    world: bevy::math::Affine3A,
    frustum: &Frustum,
) -> bool {
    let intersects =
        |bounds: Aabb| frustum.intersects_obb(&bounds, &bevy::math::Affine3A::IDENTITY, true, true);
    if let Some(submission) = submission {
        if !intersects(transform_bounds(submission.mesh.scene_bounds, world)) {
            return false;
        }
        if submission.mesh.skin {
            return true;
        }
    }
    bounds.is_some_and(intersects)
}
