//! Native rigid/skin FVF containers and per-pass texture category submission.
use bevy::{camera::primitives::Aabb, math::Affine3A, prelude::*};
use std::sync::Arc;

pub struct MeshRegistration {
    pub name: String,
    pub house_color: bool,
    pub bounds: Aabb,
    pub scene_bounds: Aabb,
    pub sphere_radius: f32,
    pub sphere_center: Vec3,
    pub vertices: usize,
    pub indices: usize,
    pub skin: bool,
}

// The same original mesh registration is retained by all parts and instances.
#[derive(Component, Clone)]
pub struct ModelSubmission {
    pub mesh: Arc<MeshRegistration>,
    pub color: [u32; 3],
    pub bound_transform: Affine3A,
    pub mesh_order: usize,
    pub subobject_order: usize,
    pub pass: usize,
    pub polygon_order: usize,
    pub shader: [u8; 16],
    pub vertex_material: Vec<u32>,
}

impl ModelSubmission {
    pub fn set_pose(&mut self, pose: &generals_assets::w3d::MeshPose) {
        self.bound_transform = bound_transform(pose.bound_transform);
    }
    pub(super) fn sort_center(&self, world: Affine3A, scale: f32) -> Vec3 {
        let mesh_world = world * self.bound_transform;
        let center = mesh_world.transform_point3(self.mesh.sphere_center);
        // RenderObjClass caches a world-space sphere. Render_Sorted passes it
        // unchanged; Insert_Triangles applies the current world a second time.
        // DX8TextureCategoryClass sets the current world to identity for skins.
        if self.mesh.skin {
            center
        } else {
            // AssetManager scales geometry and HTree translations, not the
            // native world matrix. Bevy carries that same scale on the parent.
            let native_world = Affine3A {
                matrix3: mesh_world.matrix3 / scale,
                translation: mesh_world.translation,
            };
            native_world.transform_point3(center)
        }
    }
}
pub fn bound_transform(raw: [[f32; 3]; 4]) -> Affine3A {
    Affine3A::from_mat3_translation(
        Mat3::from_cols(
            Vec3::new(raw[0][0], raw[0][2], -raw[0][1]),
            Vec3::new(raw[2][0], raw[2][2], -raw[2][1]),
            Vec3::new(-raw[1][0], -raw[1][2], raw[1][1]),
        ),
        Vec3::new(raw[3][0], raw[3][2], -raw[3][1]) * 0.1,
    )
}

pub(super) struct Registry {
    models: Vec<(Arc<MeshRegistration>, [u32; 3], u32, usize)>,
    containers: Vec<Container>,
}
struct Container {
    skin: bool,
    vertices: usize,
    indices: usize,
    used_vertices: usize,
    used_indices: usize,
}
impl Registry {
    pub const fn new() -> Self {
        Self {
            models: Vec::new(),
            containers: Vec::new(),
        }
    }
    pub fn register(&mut self, mesh: &Arc<MeshRegistration>, color: [u32; 3], scale: f32) -> usize {
        let color = if mesh.house_color || scale.to_bits() != 1.0_f32.to_bits() {
            color
        } else {
            [0; 3]
        };
        let scale = scale.to_bits();
        if let Some((_, _, _, container)) =
            self.models
                .iter()
                .find(|(registered, registered_color, registered_scale, _)| {
                    registered.name == mesh.name
                        && *registered_color == color
                        && *registered_scale == scale
                })
        {
            return *container;
        }
        let container = self
            .containers
            .iter()
            .position(|container| {
                container.skin == mesh.skin
                    && (mesh.skin || mesh.vertices <= container.vertices - container.used_vertices)
                    && mesh.indices <= container.indices - container.used_indices
            })
            .unwrap_or_else(|| {
                self.containers.push(Container {
                    skin: mesh.skin,
                    vertices: mesh.vertices.max(4000),
                    indices: mesh.indices.max(12000),
                    used_vertices: 0,
                    used_indices: 0,
                });
                self.containers.len() - 1
            });
        self.containers[container].used_vertices += mesh.vertices;
        self.containers[container].used_indices += mesh.indices;
        self.models.push((mesh.clone(), color, scale, container));
        container
    }
    pub fn is_skin(&self, container: usize) -> bool {
        self.containers[container].skin
    }
}

struct Category {
    container: usize,
    pass: usize,
    shader: [u8; 16],
    vertex_material: Vec<u32>,
    texture: Option<AssetId<Image>>,
    tasks: Vec<usize>,
}

pub(super) fn order(
    nodes: &[super::Node],
    registry: &mut Registry,
    materials: &Assets<crate::source_material::SourceMaterial>,
) -> Vec<usize> {
    // SimpleScene adds each drawable at the RenderList head. HLOD meshes and
    // each mesh's PolygonRendererList retain source model/pass/polygon order.
    let mut models: Vec<_> = (0..nodes.len())
        .filter(|&index| nodes[index].submission.is_some())
        .collect();
    models.sort_by_key(|&index| {
        let node = &nodes[index];
        let submission = node.submission.as_ref().expect("model submission");
        (
            std::cmp::Reverse(node.sequence),
            submission.subobject_order,
            submission.pass,
            submission.polygon_order,
        )
    });
    let models: Vec<_> = models
        .into_iter()
        .map(|index| {
            let submission = nodes[index].submission.as_ref().expect("model submission");
            (
                index,
                registry.register(&submission.mesh, submission.color, nodes[index].scale),
            )
        })
        .collect();
    let chunks = skin_chunks(&models, nodes, registry.containers.len());
    let categories = categories(&models, nodes, registry, materials);
    let mut result: Vec<_> = (0..nodes.len())
        .filter(|&index| nodes[index].submission.is_none() && !nodes[index].particle)
        .collect();
    result.sort_by_key(|&index| std::cmp::Reverse(nodes[index].sequence));
    // SegmentedLine submits while the scene is traversed; mesh categories flush
    // afterwards, and PointGroup systems are submitted by DoParticles last.
    let mut first = 0;
    while first < categories.len() {
        let container = categories[first].container;
        let end = first
            + categories[first..]
                .iter()
                .take_while(|category| category.container == container)
                .count();
        let group = &categories[first..end];
        let last_chunk = group
            .iter()
            .flat_map(|category| &category.tasks)
            .map(|&index| chunks[index])
            .max()
            .unwrap_or(0);
        for chunk in 0..=last_chunk {
            for category in group {
                result.extend(
                    category
                        .tasks
                        .iter()
                        .copied()
                        .filter(|&index| chunks[index] == chunk),
                );
            }
        }
        first = end;
    }
    let mut particles: Vec<_> = (0..nodes.len())
        .filter(|&index| nodes[index].particle)
        .collect();
    particles.sort_by_key(|&index| nodes[index].sequence);
    result.extend(particles);
    // Insert_To_Sorted_List uses strict greater-than, preserving this native
    // encounter order for equal centers; unbounded PointGroups have center0.
    result.sort_by(|&a, &b| {
        nodes[b]
            .depth
            .partial_cmp(&nodes[a].depth)
            .expect("finite source sort center")
            .then_with(|| nodes[b].unbounded.cmp(&nodes[a].unbounded))
    });
    // Insert_To_Sorting_Pool discards nodes beyond its fixed 4096-entry array.
    // Admission follows the center ordering and zero-center splice, before
    // the common triangle sort (and independent of the index upload chunks).
    result.truncate(4096);
    result
}

fn categories(
    models: &[(usize, usize)],
    nodes: &[super::Node],
    registry: &Registry,
    materials: &Assets<crate::source_material::SourceMaterial>,
) -> Vec<Category> {
    let mut categories: Vec<Category> = Vec::new();
    for &(index, container) in models {
        let node = &nodes[index];
        let submission = node.submission.as_ref().expect("model submission");
        let material = materials
            .get(&node.material)
            .expect("registered source material");
        let texture = material.texture.as_ref().map(Handle::id);
        // Native Equal_Material compares the original vertex material CRC.
        // Runtime lighting environment and shroud state are per-mesh and do not
        // change category membership. House-color material changes do.
        let mut vertex_material = submission.vertex_material.clone();
        for color in [
            material.parameters.ambient,
            material.parameters.diffuse,
            material.parameters.emissive,
        ] {
            vertex_material.extend(color.to_array().map(f32::to_bits));
        }
        let category = categories.iter().position(|category| {
            category.container == container
                && category.pass == submission.pass
                && category.shader == submission.shader
                && category.vertex_material == vertex_material
                && category.texture == texture
        });
        let category = category.unwrap_or_else(|| {
            // The visible category list uses Add(head, onlyonce=true).
            categories.insert(
                0,
                Category {
                    container,
                    pass: submission.pass,
                    shader: submission.shader,
                    vertex_material,
                    texture,
                    tasks: Vec::new(),
                },
            );
            0
        });
        // DX8TextureCategory::Add_Render_Task links at the head independently
        // of the visible category list's once-only head insertion.
        categories[category].tasks.insert(0, index);
    }
    categories.sort_by_key(|category| {
        (
            registry.is_skin(category.container),
            category.container,
            category.pass,
        )
    });
    categories
}

// VisibleSkinHead is another head-inserted list. Its 65535-vertex upload windows
// are consumed before iterating the same visible category list for each window.
fn skin_chunks(models: &[(usize, usize)], nodes: &[super::Node], containers: usize) -> Vec<usize> {
    let mut chunks = vec![0; nodes.len()];
    let mut state: Vec<_> = (0..containers).map(|_| (None, 0, 0)).collect();
    for &(index, container) in models.iter().rev() {
        let node = &nodes[index];
        let submission = node.submission.as_ref().expect("model submission");
        if !submission.mesh.skin {
            continue;
        }
        let key = (node.sequence, submission.subobject_order);
        let (previous, vertices, chunk) = &mut state[container];
        if *previous != Some(key) {
            if *vertices + submission.mesh.vertices > 65535 {
                *vertices = 0;
                *chunk += 1;
            }
            *vertices += submission.mesh.vertices;
            *previous = Some(key);
        }
        chunks[index] = *chunk;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_first_fit_retains_shared_mesh_registration_and_separates_skin_buffers() {
        let mut registry = Registry::new();
        let first = Arc::new(MeshRegistration {
            name: "FIRST".into(),
            house_color: false,
            bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            scene_bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            sphere_radius: 1.0,
            sphere_center: Vec3::splat(0.5),
            vertices: 3000,
            indices: 9000,
            skin: false,
        });
        let second = Arc::new(MeshRegistration {
            name: "SECOND".into(),
            house_color: false,
            bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            scene_bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            sphere_radius: 1.0,
            sphere_center: Vec3::splat(0.5),
            vertices: 1500,
            indices: 3000,
            skin: false,
        });
        let third = Arc::new(MeshRegistration {
            name: "THIRD".into(),
            house_color: false,
            bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            scene_bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            sphere_radius: 1.0,
            sphere_center: Vec3::splat(0.5),
            vertices: 1000,
            indices: 3000,
            skin: false,
        });
        let skin = Arc::new(MeshRegistration {
            name: "SKIN".into(),
            house_color: false,
            bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            scene_bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            sphere_radius: 1.0,
            sphere_center: Vec3::splat(0.5),
            vertices: 5000,
            indices: 3000,
            skin: true,
        });
        assert_eq!(registry.register(&first, [0; 3], 1.0), 0);
        assert_eq!(registry.register(&first, [0; 3], 1.0), 0);
        assert_eq!(registry.register(&second, [0; 3], 1.0), 1);
        assert_eq!(registry.register(&third, [0; 3], 1.0), 0);
        assert_eq!(registry.register(&skin, [0; 3], 1.0), 2);
        assert!(registry.is_skin(2));
    }
    fn registration(skin: bool) -> Arc<MeshRegistration> {
        Arc::new(MeshRegistration {
            name: if skin { "SKIN" } else { "RIGID" }.into(),
            house_color: false,
            bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            scene_bounds: Aabb::from_min_max(Vec3::ZERO, Vec3::ONE),
            sphere_radius: 1.0,
            sphere_center: Vec3::splat(0.5),
            vertices: 3,
            indices: 6,
            skin,
        })
    }
    fn model(
        mesh: Arc<MeshRegistration>,
        sequence: u64,
        mesh_order: usize,
        pass: usize,
        shader: u8,
        material: Handle<crate::source_material::SourceMaterial>,
    ) -> super::super::Node {
        super::super::Node {
            mesh: Handle::default(),
            material,
            clear: false,
            world: Affine3A::IDENTITY,
            scale: 1.0,
            depth: -1.0,
            sequence,
            particle: false,
            unbounded: false,
            submission: Some(ModelSubmission {
                mesh,
                color: [0; 3],
                bound_transform: Affine3A::IDENTITY,
                subobject_order: mesh_order,
                mesh_order,
                pass,
                polygon_order: 0,
                shader: [shader; 16],
                vertex_material: vec![0],
            }),
        }
    }
    #[test]
    fn category_heads_and_task_heads_preserve_native_equal_depth_overlap_order() {
        use crate::source_material::SourceMaterial;
        let mut materials = Assets::<SourceMaterial>::default();
        let material = materials.add(SourceMaterial::new(
            [0.0; 3],
            [1.0; 3],
            [0.0; 3],
            0.5,
            AlphaMode::Blend,
            false,
        ));
        let rigid = registration(false);
        let skin = registration(true);
        // Scene head traverses the newest drawable first. Within that drawable,
        // mesh0/A precedes mesh1/B. The older A task then goes at category A's
        // head, while category B remains ahead of A. Pass1 flushes after pass0;
        // skin containers flush after every rigid container despite registering
        // first. Whole-entity or creation-index sorting gets these ties wrong.
        let nodes = [
            model(rigid.clone(), 10, 0, 0, 1, material.clone()),
            model(rigid.clone(), 10, 1, 0, 2, material.clone()),
            model(rigid.clone(), 1, 0, 0, 1, material.clone()),
            model(rigid, 10, 0, 1, 1, material.clone()),
            model(skin, 20, 0, 0, 1, material),
        ];
        assert_eq!(
            order(&nodes, &mut Registry::new(), &materials),
            [1, 2, 0, 3, 4]
        );
    }
    #[test]
    fn unbounded_particles_splice_before_bounded_zero_centers_and_keep_system_order() {
        let materials = Assets::<crate::source_material::SourceMaterial>::default();
        let mut nodes: Vec<_> = [9, 5, 2]
            .into_iter()
            .map(|sequence| super::super::Node {
                mesh: Handle::default(),
                material: Handle::default(),
                clear: false,
                world: Affine3A::IDENTITY,
                scale: 1.0,
                depth: 0.0,
                sequence,
                submission: None,
                particle: sequence != 5,
                unbounded: sequence != 5,
            })
            .collect();
        assert_eq!(order(&nodes, &mut Registry::new(), &materials), [2, 0, 1]);
        nodes[1].depth = -0.0;
        assert_eq!(order(&nodes, &mut Registry::new(), &materials), [2, 0, 1]);
        nodes[1].depth = 1.0;
        assert_eq!(order(&nodes, &mut Registry::new(), &materials), [1, 2, 0]);
        for (node, depth) in nodes.iter_mut().zip([-0.0, 0.0, -0.0]) {
            node.depth = depth;
            node.particle = false;
            node.unbounded = false;
        }
        assert_eq!(order(&nodes, &mut Registry::new(), &materials), [0, 1, 2]);
    }

    #[test]
    fn pool_limit_admits_native_center_order_before_global_triangle_sort() {
        let materials = Assets::<crate::source_material::SourceMaterial>::default();
        let mut nodes: Vec<_> = (0..4097)
            .map(|sequence| super::super::Node {
                mesh: Handle::default(),
                material: Handle::default(),
                clear: false,
                world: Affine3A::IDENTITY,
                scale: 1.0,
                depth: 0.0,
                sequence,
                submission: None,
                particle: true,
                unbounded: true,
            })
            .collect();
        // The final scene node is in front of the unbounded particle splice,
        // so it consumes a slot despite being submitted after all particles.
        nodes[4096].particle = false;
        nodes[4096].unbounded = false;
        nodes[4096].depth = 1.0;
        let admitted = order(&nodes, &mut Registry::new(), &materials);
        assert_eq!(admitted.len(), 4096);
        assert_eq!(admitted[0], 4096);
        assert_eq!(&admitted[1..], &(0..4095).collect::<Vec<_>>());
    }

    #[test]
    fn owner_colors_share_ordinary_geometry_but_recolors_and_scale_create_native_variants() {
        let mut mesh = registration(false);
        Arc::get_mut(&mut mesh).unwrap().vertices = 4000;
        let red = [1.0_f32, 0.0, 0.0].map(f32::to_bits);
        let blue = [0.0_f32, 0.0, 1.0].map(f32::to_bits);
        let mut registry = Registry::new();
        assert_eq!(registry.register(&mesh, red, 1.0), 0);
        assert_eq!(registry.register(&mesh, blue, 1.0), 0);
        let mut reload = registration(false);
        Arc::get_mut(&mut reload).unwrap().vertices = 4000;
        assert_eq!(registry.register(&reload, red, 1.0), 0);
        assert_eq!(registry.register(&mesh, red, 2.0), 1);
        assert_eq!(registry.register(&mesh, blue, 2.0), 2);
        let mut colored = registration(false);
        let registration = Arc::get_mut(&mut colored).unwrap();
        registration.name = "body.housecolor".into();
        registration.house_color = true;
        registration.vertices = 4000;
        assert_eq!(registry.register(&colored, red, 1.0), 3);
        assert_eq!(registry.register(&colored, blue, 1.0), 4);
        assert_eq!(registry.register(&colored, red, 1.0), 3);
    }
    #[test]
    fn rigid_bounds_keep_native_second_world_transform_while_skin_uses_identity() {
        let mut submission = ModelSubmission {
            mesh: registration(false),
            color: [0; 3],
            bound_transform: Affine3A::from_translation(Vec3::X * 2.0),
            mesh_order: 0,
            subobject_order: 0,
            pass: 0,
            polygon_order: 0,
            shader: [0; 16],
            vertex_material: Vec::new(),
        };
        let world = Affine3A::from_translation(Vec3::X * 10.0);
        assert_eq!(
            submission
                .sort_center(world, 1.0)
                .to_array()
                .map(f32::to_bits),
            [24.5_f32, 0.5, 0.5].map(f32::to_bits)
        );
        submission.mesh = registration(true);
        assert_eq!(
            submission
                .sort_center(world, 1.0)
                .to_array()
                .map(f32::to_bits),
            [12.5_f32, 0.5, 0.5].map(f32::to_bits)
        );
        submission.mesh = registration(false);
        let scaled = Affine3A::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::IDENTITY,
            Vec3::X * 10.0,
        );
        assert_eq!(
            submission
                .sort_center(scaled, 2.0)
                .to_array()
                .map(f32::to_bits),
            [29.0_f32, 1.0, 1.0].map(f32::to_bits)
        );
    }

    #[test]
    fn skin_upload_windows_precede_texture_categories_at_the_native_vertex_limit() {
        use crate::source_material::SourceMaterial;
        let mut materials = Assets::<SourceMaterial>::default();
        let material = materials.add(SourceMaterial::new(
            [0.0; 3],
            [1.0; 3],
            [0.0; 3],
            0.5,
            AlphaMode::Blend,
            false,
        ));
        let mut mesh = registration(true);
        Arc::get_mut(&mut mesh).unwrap().vertices = 35000;
        let nodes = [
            model(mesh.clone(), 10, 0, 0, 1, material.clone()),
            model(mesh.clone(), 1, 0, 0, 2, material.clone()),
            model(mesh, 0, 0, 0, 1, material),
        ];
        assert_eq!(order(&nodes, &mut Registry::new(), &materials), [2, 1, 0]);
    }
}
