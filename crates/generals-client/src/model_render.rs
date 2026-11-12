#[cfg(test)]
mod deformation_tests;
#[cfg(test)]
mod original_material_tests;
use crate::source_material::SourceMaterial;
use bevy::{
    asset::RenderAssetUsages, mesh::VertexAttributeValues, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use generals_assets::w3d::{
    Mesh as SourceMesh, Model,
    material::{Materials, Pass},
};
use std::{collections::BTreeMap, error::Error};

pub struct Part {
    pub submission: Option<crate::sorted_view::submission::ModelSubmission>,
    pub light_bounds: Option<crate::light_environment::Bounds>,
    pub order: RenderOrder,
    pub mesh: Mesh,
    pub material: SourceMaterial,
    pub texture: Option<(String, u16)>,
    pub texture_extent: Option<u32>,
    pub house_color: bool,
    pub pickable: bool,
    pub binding: Option<PartBinding>,
}

/// `MeshModelClass::read_shader_ids` and authored `W3dMeshHeader3::SortLevel`.
#[derive(Component, Clone, Copy)]
pub struct RenderOrder {
    static_level: u8,
    sorted: bool,
}
impl RenderOrder {
    pub const fn ordinary() -> Self {
        Self {
            static_level: 0,
            sorted: false,
        }
    }
    pub fn static_level(self) -> Option<u8> {
        (self.static_level != 0).then_some(self.static_level)
    }
    pub fn layer(self, opacity: f32) -> usize {
        if self.sorted {
            3
        } else if opacity.to_bits() != 1.0_f32.to_bits() {
            8
        } else if self.static_level != 0 {
            16 + usize::from(self.static_level)
        } else {
            2
        }
    }
}
fn render_order(source: &SourceMesh, materials: &Materials) -> Result<RenderOrder, Box<dyn Error>> {
    let static_level = u8::try_from(source.sort_level)
        .ok()
        .filter(|level| *level <= 32)
        .ok_or("invalid original mesh static sort level")?;
    let sorted = static_level == 0
        && materials.passes[0].shader_ids.iter().any(|&id| {
            let shader = materials.shaders[usize::try_from(id).expect("validated shader")];
            shader[3] != 0 && shader[12] == 0
        });
    Ok(RenderOrder {
        static_level,
        sorted,
    })
}

pub struct PartBinding {
    pub mesh: usize,
    pub pass: usize,
    pub vertices: Vec<usize>,
}

#[derive(Component)]
pub struct Pickable;

pub fn parts(model: &Model) -> Result<Vec<Part>, Box<dyn Error>> {
    let mut result = Vec::new();
    let scene_bounds = convert_bounds(&model.scene_bounds);
    for (mesh_index, mesh) in model.meshes.iter().enumerate() {
        if mesh.attributes & 0x1000 != 0 {
            continue;
        }
        let materials = mesh
            .materials
            .as_ref()
            .ok_or_else(|| format!("{} lacks original material records", mesh.name))?;
        if mesh.attributes & 0x00ff_0000 == 0x0002_0000 && mesh.positions.len() > 65535 {
            return Err(format!("{} exceeds the original skin vertex buffer", mesh.name).into());
        }
        let registration = registration(mesh, materials, scene_bounds);
        for (pass_index, pass) in materials.passes.iter().enumerate() {
            // W3dUtilityClass::Convert_Shader maps DetailColor/AlphaFunc to the native
            // post-detail combiner. Disabled operations never consume stage 1.
            if pass.stages.len() > 1
                && pass.shader_ids.iter().any(|&id| {
                    let shader = materials.shaders[usize::try_from(id).expect("validated shader")];
                    shader[9] != 0 || shader[10] != 0
                })
            {
                return Err(
                    format!("{} uses an unsupported second texture stage", mesh.name).into(),
                );
            }
            let mut groups = BTreeMap::<(u32, u32, u32), Vec<usize>>::new();
            for (face, triangle) in mesh.indices.chunks_exact(3).enumerate() {
                let material = id(&pass.vertex_material_ids, usize::try_from(triangle[0])?);
                if triangle.iter().any(|&vertex| {
                    usize::try_from(vertex).map_or(true, |vertex| {
                        id(&pass.vertex_material_ids, vertex) != material
                    })
                }) {
                    return Err(
                        format!("{} has a triangle crossing vertex materials", mesh.name).into(),
                    );
                }
                let shader = id(&pass.shader_ids, face);
                let texture = pass
                    .stages
                    .first()
                    .map_or(u32::MAX, |stage| id(&stage.texture_ids, face));
                groups
                    .entry((material, shader, texture))
                    .or_default()
                    .push(face);
            }
            for ((material, shader, texture), faces) in groups {
                let mut part = part(
                    (mesh_index, mesh),
                    materials,
                    pass,
                    &faces,
                    material,
                    shader,
                    texture,
                )?;
                let vertex = &materials.vertex_materials[usize::try_from(material)?];
                let mut vertex_material = vec![vertex.attributes, u32::from(pass.colors.is_some())];
                for color in [
                    vertex.ambient,
                    vertex.diffuse,
                    vertex.specular,
                    vertex.emissive,
                ] {
                    vertex_material.extend(color.map(f32::to_bits));
                }
                vertex_material.extend([vertex.opacity.to_bits(), vertex.shininess.to_bits()]);
                part.submission = Some(crate::sorted_view::submission::ModelSubmission {
                    mesh: registration.clone(),
                    color: [0; 3],
                    bound_transform: crate::sorted_view::submission::bound_transform(
                        mesh.bound_transform,
                    ),
                    mesh_order: mesh_index,
                    subobject_order: mesh.subobject_order,
                    pass: pass_index,
                    polygon_order: faces[0],
                    shader: materials.shaders[usize::try_from(shader)?],
                    vertex_material,
                });
                part.light_bounds =
                    Some(crate::light_environment::Bounds::from_model(model, mesh)?);
                if let Some(binding) = &mut part.binding {
                    binding.pass = pass_index;
                }
                result.push(part);
            }
        }
    }
    Ok(result)
}

fn registration(
    mesh: &SourceMesh,
    materials: &Materials,
    scene_bounds: bevy::camera::primitives::Aabb,
) -> std::sync::Arc<crate::sorted_view::submission::MeshRegistration> {
    std::sync::Arc::new(crate::sorted_view::submission::MeshRegistration {
        name: mesh.name.to_ascii_lowercase(),
        house_color: mesh
            .name
            .rsplit('.')
            .next()
            .is_some_and(|name| name.to_ascii_uppercase().starts_with("HOUSECOLOR"))
            || materials
                .textures
                .iter()
                .any(|texture| texture.name.to_ascii_uppercase().starts_with("ZHC")),
        bounds: original_bounds(mesh),
        scene_bounds,
        sphere_radius: mesh.sphere_radius * 0.1,
        sphere_center: Vec3::new(
            mesh.sphere_center[0],
            mesh.sphere_center[2],
            -mesh.sphere_center[1],
        ) * 0.1,
        vertices: mesh.positions.len(),
        indices: mesh.indices.len() * materials.passes.len(),
        skin: mesh.attributes & 0x00ff_0000 == 0x0002_0000,
    })
}

fn original_bounds(mesh: &SourceMesh) -> bevy::camera::primitives::Aabb {
    convert_bounds(&mesh.bounds)
}
fn convert_bounds(bounds: &generals_assets::w3d::Bounds) -> bevy::camera::primitives::Aabb {
    bevy::camera::primitives::Aabb::from_min_max(
        Vec3::new(bounds.min[0], bounds.min[2], -bounds.max[1]) * 0.1,
        Vec3::new(bounds.max[0], bounds.max[2], -bounds.min[1]) * 0.1,
    )
}

fn id(ids: &[u32], element: usize) -> u32 {
    ids[if ids.len() == 1 { 0 } else { element }]
}

fn part(
    source: (usize, &SourceMesh),
    materials: &Materials,
    pass: &Pass,
    faces: &[usize],
    material_id: u32,
    shader_id: u32,
    texture_id: u32,
) -> Result<Part, Box<dyn Error>> {
    let (mesh_index, source) = source;
    let vertex_material = &materials.vertex_materials[usize::try_from(material_id)?];
    let shader = materials.shaders[usize::try_from(shader_id)?];
    let alpha_mode = alpha_mode(shader, &source.name)?;
    let texture = if shader[8] != 0 && texture_id != u32::MAX {
        let texture = &materials.textures[usize::try_from(texture_id)?];
        Some((texture.name.to_ascii_lowercase(), texture.attributes & 0x18))
    } else {
        None
    };
    let mut vertices = Vec::with_capacity(faces.len() * 3);
    let mut positions = Vec::with_capacity(faces.len() * 3);
    let mut normals = Vec::with_capacity(faces.len() * 3);
    let mut coordinates = Vec::with_capacity(faces.len() * 3);
    let mut colors = Vec::with_capacity(faces.len() * 3);
    for &face in faces {
        for corner in 0..3 {
            let vertex = usize::try_from(source.indices[face * 3 + corner])?;
            vertices.push(vertex);
            let point = source.positions[vertex];
            // A proper rotation: source Z-up becomes Bevy Y-up without reflecting winding.
            positions.push([point[0] * 0.1, point[2] * 0.1, -point[1] * 0.1]);
            if let Some(source_normals) = &source.normals {
                let normal = source_normals[vertex];
                normals.push([normal[0], normal[2], -normal[1]]);
            }
            if let Some(stage) = pass
                .stages
                .first()
                .filter(|stage| !stage.coordinates.is_empty())
            {
                let coordinate = stage
                    .face_coordinate_ids
                    .as_ref()
                    .map_or(Ok(vertex), |ids| usize::try_from(ids[face][corner]))?;
                coordinates.push(stage.coordinates[coordinate]);
            }
            colors.push(pass.colors.as_ref().map_or([1.0; 4], |colors| {
                colors[vertex].map(|value| f32::from(value) / 255.0)
            }));
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    if coordinates.is_empty() {
        // DX8MeshRenderer emits no texture fields for zero UV arrays; its
        // dynamic/skinned path explicitly writes zero for absent coordinates.
        // Bevy requires this attribute even when the source has no UV channel.
        coordinates.resize(faces.len() * 3, [0.0; 2]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, coordinates);
    if normals.is_empty() {
        mesh.compute_flat_normals();
    } else {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    let mut material = SourceMaterial::new(
        vertex_material.ambient,
        vertex_material.diffuse,
        vertex_material.emissive,
        vertex_material.opacity,
        alpha_mode,
        source.attributes & 0x2000 != 0,
    );

    material.parameters.vertex_diffuse = u32::from(pass.colors.is_some());
    material.parameters.primary_gradient = u32::from(shader[5]);
    material.depth_write = shader[1] != 0;
    material.depth_compare =
        (shader[0] == 7).then_some(bevy::render::render_resource::CompareFunction::Always);
    material.parameters.alpha_test = if shader[12] != 0 { 96.0 / 255.0 } else { -1.0 };
    Ok(Part {
        light_bounds: None,
        submission: None,
        order: render_order(source, materials)?,
        mesh,
        material,
        texture,
        texture_extent: None,
        binding: Some(PartBinding {
            mesh: mesh_index,
            pass: 0,
            vertices,
        }),
        // MeshClass::Load/Cast_Ray ignores an entire mesh if pass zero has any
        // blended shader without alpha testing, including opaque subparts.
        pickable: !materials.passes[0].shader_ids.iter().any(|&id| {
            let shader = materials.shaders[usize::try_from(id).expect("validated shader index")];
            (shader[7] != 1 || shader[3] != 0) && shader[12] == 0
        }),
        house_color: source
            .name
            .rsplit('.')
            .next()
            .unwrap_or(&source.name)
            .to_ascii_uppercase()
            .starts_with("HOUSECOLOR"),
    })
}

/// Updates only positions/normals, retaining original per-face materials and UVs.
pub fn deform(
    mesh: &mut Mesh,
    binding: &PartBinding,
    poses: &[generals_assets::w3d::MeshPose],
) -> Result<bool, Box<dyn Error>> {
    let pose = validate_deformation(mesh, binding, poses)?;
    write_attribute(
        mesh,
        Mesh::ATTRIBUTE_POSITION,
        binding,
        &pose.positions,
        0.1,
    );
    if let Some(normals) = &pose.normals {
        write_attribute(mesh, Mesh::ATTRIBUTE_NORMAL, binding, normals, 1.0);
    } else {
        // Prepared parts are non-indexed triangles. Use the same Bevy function
        // as compute_flat_normals, retaining the existing normal allocation.
        for triangle in (0..binding.vertices.len()).step_by(3) {
            let normal = {
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    unreachable!("validated positions")
                };
                bevy::mesh::triangle_normal(
                    points[triangle],
                    points[triangle + 1],
                    points[triangle + 2],
                )
            };
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
            else {
                unreachable!("validated normals")
            };
            normals[triangle..triangle + 3].fill(normal);
        }
    }
    Ok(pose.visible)
}

/// Compares final vertex bits before requesting tracked mutable asset access.
/// Visibility is independent: a hidden part may still deform, and an unchanged
/// muzzle mesh may become visible without changing its vertex buffers.
pub fn deformation_matches(
    mesh: &Mesh,
    binding: &PartBinding,
    poses: &[generals_assets::w3d::MeshPose],
) -> Result<bool, Box<dyn Error>> {
    let pose = validate_deformation(mesh, binding, poses)?;
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!("validated positions")
    };
    if points.iter().zip(&binding.vertices).any(|(point, &index)| {
        point.map(f32::to_bits) != render_point(pose.positions[index], 0.1).map(f32::to_bits)
    }) {
        return Ok(false);
    }
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        unreachable!("validated normals")
    };
    Ok(pose.normals.as_ref().map_or_else(
        || {
            // Position bits already match the values deform would write. Reuse
            // those positions for the identical flat-normal calculation.
            points
                .chunks_exact(3)
                .zip(normals.chunks_exact(3))
                .all(|(points, normals)| {
                    let expected = bevy::mesh::triangle_normal(points[0], points[1], points[2]);
                    normals
                        .iter()
                        .all(|normal| normal.map(f32::to_bits) == expected.map(f32::to_bits))
                })
        },
        |source| {
            normals
                .iter()
                .zip(&binding.vertices)
                .all(|(normal, &index)| {
                    normal.map(f32::to_bits) == render_point(source[index], 1.0).map(f32::to_bits)
                })
        },
    ))
}

fn validate_deformation<'a>(
    mesh: &Mesh,
    binding: &PartBinding,
    poses: &'a [generals_assets::w3d::MeshPose],
) -> Result<&'a generals_assets::w3d::MeshPose, Box<dyn Error>> {
    let pose = poses
        .get(binding.mesh)
        .ok_or("animation mesh binding missing")?;
    // Validate every source index before touching a live asset. The prepared
    // expanded triangle topology and its UV/color buffers remain unchanged.
    if binding.vertices.iter().any(|&index| {
        index >= pose.positions.len()
            || pose
                .normals
                .as_ref()
                .is_some_and(|normals| index >= normals.len())
    }) {
        return Err("animation vertex binding missing".into());
    }
    for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
        if !matches!(mesh.attribute(attribute), Some(VertexAttributeValues::Float32x3(values)) if values.len() == binding.vertices.len())
        {
            return Err("animation attribute binding mismatch".into());
        }
    }
    Ok(pose)
}

const fn render_point(point: [f32; 3], scale: f32) -> [f32; 3] {
    [point[0] * scale, point[2] * scale, -point[1] * scale]
}

fn write_attribute(
    mesh: &mut Mesh,
    attribute: bevy::mesh::MeshVertexAttribute,
    binding: &PartBinding,
    source: &[[f32; 3]],
    scale: f32,
) {
    let Some(VertexAttributeValues::Float32x3(values)) = mesh.attribute_mut(attribute) else {
        unreachable!("validated animation attribute")
    };
    for (value, &index) in values.iter_mut().zip(&binding.vertices) {
        *value = render_point(source[index], scale);
    }
}

fn alpha_mode(shader: [u8; 16], name: &str) -> Result<AlphaMode, Box<dyn Error>> {
    // W3dShaderStruct's native LESS_EQUAL=3 and ALWAYS=7. Original Locater01/02
    // use ALWAYS with depth writes disabled and ordinary source-alpha blending.
    if !matches!(shader[0], 3 | 7)
        || shader[5] > 1
        || (shader[5] == 0 && shader[8] == 0)
        || shader[9] != 0
        || shader[10] != 0
    {
        return Err(format!("{name} uses an unsupported fixed-function shader").into());
    }
    if shader[1] > 1 || shader[12] > 1 {
        return Err(format!("{name} invalid depth-write/alpha-test flag").into());
    }
    match (shader[7], shader[3]) {
        (1, 0) => Ok(if shader[12] != 0 {
            AlphaMode::Mask(96.0 / 255.0)
        } else {
            AlphaMode::Opaque
        }),
        (2, 5) => Ok(AlphaMode::Blend),
        (1, 1) => Ok(AlphaMode::Add),
        (0, 2) => Ok(AlphaMode::Multiply),
        pair => Err(format!("{name} uses unsupported source/destination blend {pair:?}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AlphaMode, Materials, Mesh, Pass, SourceMesh, VertexAttributeValues, alpha_mode, part,
    };
    use generals_assets::w3d::material::{Texture, VertexMaterial};

    #[test]
    fn original_glow_passes_and_locator_preserve_independent_depth_and_blend_settings() {
        let mut mesh = SourceMesh {
            name: "ABPWRPLANT_A1.POWER GLOW01".into(),
            bone_name: None,
            attributes: 0,
            sort_level: 0,
            subobject_order: 0,
            bounds: generals_assets::w3d::Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            sphere_center: [0.0; 3],
            sphere_radius: 1.0,
            bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2],
            normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
            materials: None,
            deformation: None,
        };
        let mut materials = Materials {
            vertex_materials: vec![VertexMaterial {
                name: "Glowong windows".into(),
                attributes: 0,
                ambient: [1.0; 3],
                diffuse: [1.0; 3],
                specular: [0.0; 3],
                emissive: [0.0; 3],
                shininess: 1.0,
                opacity: 1.0,
            }],
            // Original W3D.big records: the unlit base writes depth; its lit glow adds.
            shaders: vec![
                [3, 1, 0, 0, 2, 0, 0, 1, 1, 0, 0, 2, 0, 0, 0, 2],
                [3, 0, 0, 1, 2, 1, 0, 1, 1, 0, 0, 2, 0, 0, 0, 2],
                // Both installed Locater W3Ds use this identical shader record.
                [7, 0, 0, 5, 2, 1, 0, 2, 1, 0, 0, 2, 0, 0, 0, 2],
            ],
            textures: vec![Texture {
                name: "glow.tga".into(),
                attributes: 0,
            }],
            passes: vec![Pass {
                vertex_material_ids: vec![0],
                shader_ids: vec![0],
                colors: None,
                stages: Vec::new(),
            }],
        };
        materials.passes[0]
            .stages
            .push(generals_assets::w3d::material::TextureStage {
                texture_ids: vec![0],
                coordinates: Vec::new(),
                face_coordinate_ids: None,
            });
        let base = part((0, &mesh), &materials, &materials.passes[0], &[0], 0, 0, 0).unwrap();
        let glow = part((0, &mesh), &materials, &materials.passes[0], &[0], 0, 1, 0).unwrap();
        assert_eq!(base.material.parameters.primary_gradient, 0);
        assert!(base.material.depth_write);
        assert!(matches!(base.material.alpha_mode, AlphaMode::Opaque));
        assert_eq!(glow.material.parameters.primary_gradient, 1);
        assert!(!glow.material.depth_write);
        assert!(matches!(glow.material.alpha_mode, AlphaMode::Add));
        assert_eq!(base.texture, glow.texture);
        assert_eq!(
            base.texture.as_ref().map(|(name, _)| name.as_str()),
            Some("glow.tga")
        );
        let Some(VertexAttributeValues::Float32x2(coordinates)) =
            base.mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("required Bevy UV attribute absent")
        };
        assert_eq!(coordinates, &[[0.0, 0.0]; 3]);
        let locator = part((0, &mesh), &materials, &materials.passes[0], &[0], 0, 2, 0).unwrap();
        assert!(matches!(
            locator.material.depth_compare,
            Some(bevy::render::render_resource::CompareFunction::Always)
        ));
        assert!(!locator.material.depth_write);
        assert!(matches!(locator.material.alpha_mode, AlphaMode::Blend));
        assert!(base.material.depth_compare.is_none());
        assert!(glow.material.depth_compare.is_none());
        // A later additive pass does not make an opaque pass-zero mesh SORT.
        assert_eq!(glow.order.layer(1.0), 2);
        assert_eq!(glow.order.layer(0.5), 8);
        materials.passes[0].shader_ids = vec![1];
        let sorted = super::render_order(&mesh, &materials).unwrap();
        assert_eq!(sorted.layer(1.0), 3);
        assert_eq!(sorted.layer(0.5), 3);
        materials.shaders[1][12] = 1;
        assert_eq!(
            super::render_order(&mesh, &materials).unwrap().layer(1.0),
            2
        );
        materials.shaders[1][12] = 0;
        mesh.sort_level = 2;
        let authored = super::render_order(&mesh, &materials).unwrap();
        assert_eq!(authored.static_level(), Some(2));
        assert_eq!(authored.layer(1.0), 18);
        assert_eq!(authored.layer(0.5), 8);
    }

    #[test]
    fn cement_rubble_alpha_test_retains_source_alpha_blend() {
        // Installed PMcemblk2.DAMAGE BITS03 uses all three independently.
        let mut shader = [0; 16];
        shader[0] = 3;
        shader[5] = 1;
        shader[1] = 1;
        shader[3] = 5;
        shader[7] = 2;
        shader[12] = 1;
        assert!(matches!(
            alpha_mode(shader, "PMcemblk2.DAMAGE BITS03"),
            Ok(AlphaMode::Blend)
        ));
    }
}
