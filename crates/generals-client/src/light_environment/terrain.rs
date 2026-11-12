//! `HeightMap`'s CPU vertex-buffer update, including its double attenuation factor.
use super::{Parameters, Resources, SourceMaterial, TerrainState, squared_length};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use num_traits::ToPrimitive;

pub(super) struct MeshState {
    revision: (u32, u64),
    original: Vec<[f32; 4]>,
    baseline: Vec<[u8; 4]>,
    parameters: Parameters,
    positions: [Vec4; 20],
    colors: [Vec4; 20],
    regions: [Option<Region>; 20],
    output: Vec<[f32; 4]>,
}

pub(super) fn apply(
    resources: &mut Resources,
    surfaces: &Query<
        (&Mesh3d, &MeshMaterial3d<SourceMaterial>),
        With<crate::terrain::changes::Surface>,
    >,
    state: &mut TerrainState,
    positions: &[Vec4; 20],
    colors: &[Vec4; 20],
    regions: &[Option<Region>; 20],
) {
    let selected = regions.iter().any(Option::is_some);
    let revision = (
        resources.battle.generation,
        resources.battle.simulation.terrain_height_revision(),
    );
    for (mesh_handle, material_handle) in surfaces {
        if !selected && !state.meshes.contains_key(&mesh_handle.0.id()) {
            continue;
        }
        let Some(material) = resources.materials.get(&material_handle.0) else {
            continue;
        };
        let mut parameters = material.parameters.clone();
        parameters.terrain_lights = 0;
        let Some(mesh) = resources.meshes.get(&mesh_handle.0) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x4(original)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("native terrain vertex colors");
        };
        let cached = state
            .meshes
            .entry(mesh_handle.0.id())
            .or_insert_with(|| MeshState {
                revision,
                original: original.clone(),
                baseline: Vec::new(),
                parameters: parameters.clone(),
                positions: [Vec4::ZERO; 20],
                colors: [Vec4::ZERO; 20],
                regions: [None; 20],
                output: Vec::new(),
            });
        let changed = cached.prepare(mesh, revision, parameters);
        let write = changed
            || (selected
                && (cached.positions != *positions
                    || cached.colors != *colors
                    || cached.regions != *regions));
        if write && cached.paint(mesh, positions, colors, regions) {
            let mut mesh = resources
                .meshes
                .get_mut(&mesh_handle.0)
                .expect("retained terrain mesh");
            let Some(VertexAttributeValues::Float32x4(output)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
            else {
                unreachable!();
            };
            output.copy_from_slice(&cached.output);
        }
        cached.positions = *positions;
        cached.colors = *colors;
        cached.regions = *regions;
        let flag = 1;
        if resources
            .materials
            .get(&material_handle.0)
            .is_some_and(|material| material.parameters.terrain_lights != flag)
        {
            resources
                .materials
                .get_mut(&material_handle.0)
                .unwrap()
                .parameters
                .terrain_lights = flag;
        }
    }
    state.meshes.retain(|id, _| resources.meshes.contains(*id));
}

impl MeshState {
    fn prepare(&mut self, mesh: &Mesh, revision: (u32, u64), parameters: Parameters) -> bool {
        if self.revision != revision {
            self.revision = revision;
            let Some(VertexAttributeValues::Float32x4(original)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("native terrain colors");
            };
            self.original.clone_from(original);
            self.baseline.clear();
        }
        let changed = self.parameters != parameters || self.baseline.is_empty();
        if changed {
            self.parameters = parameters;
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("native terrain normals");
            };
            self.baseline.clear();
            self.baseline.extend(
                normals
                    .iter()
                    .zip(&self.original)
                    .map(|(normal, color)| baseline(&self.parameters, *normal, *color)),
            );
        }
        if changed {
            self.output.clear();
            self.output.extend(
                self.baseline
                    .iter()
                    .map(|color| color.map(|channel| f32::from(channel) / 255.0)),
            );
        }
        changed
    }
    fn paint(
        &mut self,
        mesh: &Mesh,
        positions: &[Vec4; 20],
        colors: &[Vec4; 20],
        regions: &[Option<Region>; 20],
    ) -> bool {
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("native terrain positions");
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("native terrain normals");
        };
        for (quad_index, quad) in points.chunks_exact(4).enumerate() {
            let x = quad
                .iter()
                .map(|point| point[0])
                .fold(f32::INFINITY, f32::min);
            let y = quad
                .iter()
                .map(|point| -point[2])
                .fold(f32::INFINITY, f32::min);
            let x = x.to_i32().expect("native quad grid X");
            let y = y.to_i32().expect("native quad grid Y");
            if !regions
                .iter()
                .flatten()
                .any(|region| region.intersects(x, y))
            {
                continue;
            }
            for (corner, point) in quad.iter().enumerate() {
                let index = quad_index * 4 + corner;
                let point = Vec3::new(point[0] * 10.0, -point[2] * 10.0, point[1] * 10.0);
                let normal = normals[index];
                let normal = Vec3::new(normal[0], -normal[2], normal[1]);
                self.output[index] =
                    illuminate(self.baseline[index], point, normal, positions, colors)
                        .map(|channel| f32::from(channel) / 255.0);
            }
        }
        let Some(VertexAttributeValues::Float32x4(actual)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("native terrain colors");
        };
        // Compare the mesh, not only the cache: another writer may have changed it.
        actual
            .iter()
            .flatten()
            .map(|channel| channel.to_bits())
            .ne(self
                .output
                .iter()
                .flatten()
                .map(|channel| channel.to_bits()))
    }
}

fn baseline(parameters: &Parameters, normal: [f32; 3], color: [f32; 4]) -> [u8; 4] {
    let normal = Vec3::from_array(normal).normalize();
    let mut diffuse = Vec3::ZERO;
    for (light, direction) in parameters
        .light_color
        .iter()
        .zip(&parameters.light_direction)
    {
        diffuse += light.truncate() * normal.dot(-direction.truncate()).max(0.0);
    }
    let ambient = parameters.ambient.truncate() * parameters.scene_ambient.truncate();
    let diffuse = parameters.diffuse.truncate() * diffuse;
    let lit = ambient + diffuse + parameters.emissive.truncate();
    let rgb = (lit.clamp(Vec3::ZERO, Vec3::ONE) * Vec3::from_array([color[0], color[1], color[2]]))
        .to_array();
    [pack(rgb[0]), pack(rgb[1]), pack(rgb[2]), pack(color[3])]
}

fn illuminate(
    base: [u8; 4],
    point: Vec3,
    normal: Vec3,
    positions: &[Vec4; 20],
    colors: &[Vec4; 20],
) -> [u8; 4] {
    let mut rgb = [
        f32::from(base[0]) * (1.0_f32 / 255.0),
        f32::from(base[1]) * (1.0_f32 / 255.0),
        f32::from(base[2]) * (1.0_f32 / 255.0),
    ];
    for (position, color) in positions.iter().zip(colors) {
        if position.w <= 0.0 {
            continue;
        }
        let delta = point - position.truncate();
        let distance = squared_length(delta).sqrt();
        if distance >= position.w {
            continue;
        }
        let factor = factor(distance, position.w);
        // Vector3::operator/= computes one reciprocal, then multiplies.
        let reciprocal = 1.0 / distance;
        let normalized = if distance == 0.0 {
            // Selected x86 invalid-operation result of each zero*infinity.
            // Negation below is native HeightMap's separate lightRay operation.
            Vec3::splat(f32::from_bits(0xffc0_0000))
        } else {
            delta * reciprocal
        };
        let ray = -normalized;
        let products = (ray * normal).to_array();
        let shade = (((products[0] + products[1]) + products[2]) * factor).clamp(0.0, 1.0);
        for channel in 0..3 {
            let diffuse = shade * color[channel];
            rgb[channel] += diffuse;
            let ambient = factor * color[channel];
            rgb[channel] += ambient;
        }
    }
    pack_rgb(rgb, base[3])
}

fn factor(distance: f32, outer: f32) -> f32 {
    (1.0_f64 - (f64::from(distance) - 1.0) / (f64::from(outer) - 1.0))
        .to_f32()
        .expect("finite native terrain attenuation")
        .clamp(0.0, 1.0)
}
fn pack(value: f32) -> u8 {
    let scaled = value.clamp(0.0, 1.0) * 255.0;
    // WWMath::Float_To_Int_Chop; x86 masks variable shift counts to five bits.
    // This also retains the source machine's result for nonfinite inputs.
    chop(scaled).to_le_bytes()[0]
}

const fn chop(value: f32) -> i32 {
    let bits = i32::from_ne_bytes(value.to_ne_bytes());
    let sign = bits >> 31;
    let mantissa = (bits & 0x007f_ffff) | 0x0080_0000;
    let exponent = ((bits & 0x7fff_ffff) >> 23) - 127;
    let shift = u32::from_ne_bytes((31 - exponent).to_ne_bytes());
    let shifted = u32::from_ne_bytes(mantissa.to_ne_bytes())
        .wrapping_shl(8)
        .wrapping_shr(shift);
    let shifted = i32::from_ne_bytes(shifted.to_ne_bytes());
    ((shifted ^ sign).wrapping_sub(sign)) & !(exponent >> 31)
}

fn pack_rgb(rgb: [f32; 3], alpha: u8) -> [u8; 4] {
    let channels = rgb.map(|value| chop(value.clamp(0.0, 1.0) * 255.0));
    let signed = channels[2] | channels[1].wrapping_shl(8) | channels[0].wrapping_shl(16);
    let packed = u32::from_ne_bytes(signed.to_ne_bytes()) | u32::from(alpha).wrapping_shl(24);
    let [blue, green, red, alpha] = packed.to_le_bytes();
    [red, green, blue, alpha]
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Region {
    pub current: [i32; 4],
    pub previous: [i32; 4],
}
impl Region {
    fn intersects(self, x: i32, y: i32) -> bool {
        [self.current, self.previous].iter().any(|bounds| {
            bounds[0] <= x + 1 && bounds[2] >= x && bounds[1] <= y + 1 && bounds[3] >= y
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn double_factor_preserves_original_packed_boundary_and_ordered_additions() {
        let distance = 20.738_562_f32;
        let mut positions = [Vec4::ZERO; 20];
        let mut colors = [Vec4::ZERO; 20];
        positions[0] = Vec4::new(distance, 0.0, 0.0, 21.0);
        colors[0] = Vec4::splat(0.25);
        let actual = illuminate(
            [0, 0, 0, 255],
            Vec3::ZERO,
            Vec3::new(0.2, 0.0, 0.0),
            &positions,
            &colors,
        );
        assert_eq!(actual, [1, 1, 1, 255]);
        let approximate = (1.0 - (distance - 1.0) / 20.0).clamp(0.0, 1.0);
        let diffuse = 0.2 * approximate * 0.25;
        let ambient = approximate * 0.25;
        assert_eq!(pack(diffuse + ambient), 0);
    }
    #[test]
    fn source_packed_color_preserves_x86_shift_and_nonfinite_sign_bits() {
        assert_eq!(
            [pack(0.0), pack(1.0 / 255.0), pack(0.5), pack(1.0)],
            [0, 1, 127, 255]
        );
        assert_eq!(pack(f32::from_bits(0x7fc0_0000)), 1);
        assert_eq!(pack(f32::from_bits(0xffc0_0000)), 255);
        assert_eq!(pack(f32::INFINITY), 255);
        assert_eq!(pack(f32::NEG_INFINITY), 0);
        assert_eq!(chop(f32::from_bits(0x7fc0_0000)), 1);
        assert_eq!(chop(f32::from_bits(0xffc0_0000)), -1);
        assert_eq!(chop(f32::INFINITY), 1);
        assert_eq!(chop(f32::NEG_INFINITY), -1);
        assert_eq!(
            pack_rgb([0.0, 0.0, f32::from_bits(0xffc0_0000)], 32),
            [255; 4]
        );
        let channels = [pack(f32::from_bits(0xffc0_0000)); 3];
        let rgb =
            u32::from(channels[0]) << 16 | u32::from(channels[1]) << 8 | u32::from(channels[2]);
        assert_eq!(rgb, 0x00ff_ffff);
    }
    #[test]
    fn coincident_vertex_preserves_selected_x86_invalid_then_light_ray_negation() {
        let mut positions = [Vec4::ZERO; 20];
        let mut colors = [Vec4::ZERO; 20];
        positions[0] = Vec4::new(0.0, 0.0, 0.0, 21.0);
        colors[0] = Vec4::splat(0.25);
        assert_eq!(
            illuminate([0, 0, 0, 255], Vec3::ZERO, Vec3::Y, &positions, &colors),
            [1, 1, 1, 255]
        );
    }
    #[test]
    fn selective_quad_refresh_retains_unprocessed_region_and_authored_blend_alpha() {
        use bevy::{asset::RenderAssetUsages, render::render_resource::PrimitiveTopology};
        let points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [100.0, 0.0, 0.0],
            [101.0, 0.0, 0.0],
            [101.0, 0.0, -1.0],
            [100.0, 0.0, -1.0],
        ];
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 8])
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            vec![[1.0, 1.0, 1.0, 127.0 / 255.0]; 8],
        );
        let parameters =
            SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false)
                .parameters;
        let mut state = MeshState {
            revision: (0, 0),
            original: vec![[1.0, 1.0, 1.0, 127.0 / 255.0]; 8],
            baseline: Vec::new(),
            parameters: parameters.clone(),
            positions: [Vec4::ZERO; 20],
            colors: [Vec4::ZERO; 20],
            regions: [None; 20],
            output: Vec::new(),
        };
        state.prepare(&mesh, (0, 0), parameters);
        let mut positions = [Vec4::ZERO; 20];
        let mut colors = [Vec4::ZERO; 20];
        let mut regions = [None; 20];
        positions[0] = Vec4::new(0.0, 0.0, 5.0, 21.0);
        colors[0] = Vec4::splat(0.25);
        regions[0] = Some(Region {
            current: [-2, -2, 3, 3],
            previous: [-2, -2, 3, 3],
        });
        state.paint(&mesh, &positions, &colors, &regions);
        let retained = state.output[..4].to_vec();
        assert!(retained.iter().any(|color| color[0] > 0.0));
        positions[0] = Vec4::new(1000.0, 0.0, 5.0, 21.0);
        regions[0] = Some(Region {
            current: [98, -2, 103, 3],
            previous: [98, -2, 103, 3],
        });
        state.paint(&mesh, &positions, &colors, &regions);
        assert_eq!(&state.output[..4], retained);
        assert!(
            state
                .output
                .iter()
                .all(|color| color[3].to_bits() == (127.0_f32 / 255.0).to_bits())
        );
    }
}

#[cfg(test)]
mod dirty_tests;
