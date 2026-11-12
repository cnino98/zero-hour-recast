//! Native-capture inspection of the actual optimized tree batch and bound shroud.
use super::{Trees, state::Topple};
use crate::{
    playtest::Match, playtest_view::WorldCamera, tree_material::TreeMaterial, visibility_view::Fog,
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use generals_sim::{
    native_real,
    skirmish::{Skirmish, visibility::Status},
};
use num_traits::ToPrimitive;
use std::{error::Error, ops::Range};

#[derive(SystemParam)]
pub struct Frame<'w, 's> {
    trees: Res<'w, Trees>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<TreeMaterial>>,
    images: Res<'w, Assets<Image>>,
    fog: Res<'w, Fog>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    rendered: Query<
        'w,
        's,
        (
            &'static Mesh3d,
            &'static MeshMaterial3d<TreeMaterial>,
            &'static ViewVisibility,
        ),
    >,
}

pub fn intact(trees: &Trees, placement: usize) -> bool {
    trees.buffer.trees.iter().any(|tree| {
        tree.placement == placement && tree.kind.is_some() && matches!(tree.topple, Topple::Upright)
    })
}

fn cell_centers(game: &Skirmish, point: [f32; 3]) -> [[f32; 2]; 4] {
    let cell = game.visibility_cell_size();
    let inverse = 1.0 / cell;
    let half_cell = cell * 0.5;
    let base = std::array::from_fn::<_, 2, _>(|axis| {
        native_real::floor((point[axis] - half_cell) * inverse)
    });
    [[0, 0], [1, 0], [1, 1], [0, 1]].map(|offset| {
        std::array::from_fn(|axis| {
            ((base[axis] + offset[axis]).to_f32().expect("native cell") + 0.5) * cell
        })
    })
}

pub fn all_status(game: &Skirmish, viewer: usize, point: [f32; 3], expected: &Status) -> bool {
    let expected = std::mem::discriminant(expected);
    cell_centers(game, point)
        .into_iter()
        .all(|point| std::mem::discriminant(&game.visibility_cell(viewer, point)) == expected)
}

impl Frame<'_, '_> {
    pub fn camera_pose(&self) -> [u32; 16] {
        self.camera.1.to_matrix().to_cols_array().map(f32::to_bits)
    }

    pub fn sample(
        &self,
        battle: &Match,
        placement: usize,
        viewport: Rect,
    ) -> Result<Option<[[u8; 4]; 4]>, Box<dyn Error>> {
        if self.trees.generation != Some(battle.generation) {
            return Ok(None);
        }
        let tree = self
            .trees
            .buffer
            .trees
            .iter()
            .find(|tree| tree.placement == placement)
            .ok_or("selected original tree missing from real buffer")?;
        let kind = tree
            .kind
            .ok_or("selected original tree removed before paid clearing")?;
        if !matches!(tree.topple, Topple::Upright) {
            return Err("scouting toppled selected original tree before paid placement".into());
        }
        let Some(range) = self.vertex_range(placement) else {
            return Ok(None);
        };
        let Some(mesh_handle) = &self.trees.mesh else {
            return Ok(None);
        };
        let Some(material_handle) = &self.trees.material else {
            return Ok(None);
        };
        if !self.rendered.iter().any(|(mesh, material, visible)| {
            mesh.0 == *mesh_handle && material.0 == *material_handle && visible.get()
        }) {
            return Ok(None);
        }
        let mesh = self
            .meshes
            .get(mesh_handle)
            .ok_or("actual tree mesh missing")?;
        let material = self
            .materials
            .get(material_handle)
            .ok_or("actual tree material missing")?;
        let atlas = self
            .images
            .get(&material.atlas)
            .ok_or("bound tree atlas missing")?;
        if atlas.data.as_ref().is_none_or(Vec::is_empty) {
            return Err("bound original tree atlas has no uploaded pixels".into());
        }
        if !self.in_view(mesh, material, range, viewport)? {
            return Ok(None);
        }
        if super::prop_fogged(
            &battle.simulation,
            battle.observer_player.unwrap_or(0),
            tree.position,
        ) != all_status(
            &battle.simulation,
            battle.observer_player.unwrap_or(0),
            tree.position,
            &Status::Fogged,
        ) {
            return Err("tree runtime fog predicate differs from native four-cell sample".into());
        }
        let pixels = self.shroud_pixels(battle, tree.position, material)?;
        println!(
            "Original tree batch sample: native_record={placement} model={} texture={} pixels={pixels:?}",
            self.trees.buffer.types[kind].model, self.trees.buffer.types[kind].texture
        );
        Ok(Some(pixels))
    }

    pub fn cleared(&self, battle: &Match, placement: usize) -> Result<bool, Box<dyn Error>> {
        if self.trees.generation != Some(battle.generation) || self.trees.buffer.changed {
            return Ok(false);
        }
        let tree = self
            .trees
            .buffer
            .trees
            .iter()
            .find(|tree| tree.placement == placement)
            .ok_or("selected paid tree record disappeared")?;
        if tree.kind.is_some() || self.vertex_range(placement).is_some() {
            return Ok(false);
        }
        let Some(mesh) = self
            .trees
            .mesh
            .as_ref()
            .and_then(|handle| self.meshes.get(handle))
        else {
            return Ok(false);
        };
        let mut vertices = 0;
        let mut indices = 0;
        for tree in &self.trees.buffer.trees {
            let Some(kind) = tree.kind.filter(|_| tree.visible) else {
                continue;
            };
            let data = &self.trees.buffer.types[kind];
            if vertices + data.vertices.len() + 2 >= 30_000
                || indices + data.indices.len() + 6 >= 60_000
            {
                break;
            }
            vertices += data.vertices.len();
            indices += data.indices.len();
        }
        Ok(mesh.count_vertices() == vertices
            && mesh.indices().is_some_and(|actual| actual.len() == indices))
    }

    fn vertex_range(&self, placement: usize) -> Option<Range<usize>> {
        let mut vertices = 0;
        let mut indices = 0;
        for tree in &self.trees.buffer.trees {
            let Some(kind) = tree.kind.filter(|_| tree.visible) else {
                continue;
            };
            let data = &self.trees.buffer.types[kind];
            if vertices + data.vertices.len() + 2 >= 30_000
                || indices + data.indices.len() + 6 >= 60_000
            {
                return None;
            }
            let range = vertices..vertices + data.vertices.len();
            if tree.placement == placement {
                return Some(range);
            }
            vertices = range.end;
            indices += data.indices.len();
        }
        None
    }

    fn in_view(
        &self,
        mesh: &Mesh,
        material: &TreeMaterial,
        range: Range<usize>,
        viewport: Rect,
    ) -> Result<bool, Box<dyn Error>> {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return Err("tree batch lacks original positions".into());
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            return Err("tree batch lacks native sway data".into());
        };
        let points = positions
            .get(range.clone())
            .ok_or("selected tree not in actual uploaded mesh")?;
        let normals = normals
            .get(range)
            .ok_or("selected tree sway range missing")?;
        if points.is_empty() {
            return Err("original tree has no actual vertices".into());
        }
        let mut bounds = None::<Rect>;
        for (&point, normal) in points.iter().zip(normals) {
            let sway = normal[0].to_usize().ok_or("invalid tree sway index")?;
            let sway = material
                .parameters
                .sway
                .get(sway)
                .ok_or("tree sway index exceeds shader table")?;
            let point = Vec3::from_array(point) + (point[2] - normal[2]) * sway.truncate();
            let point = Vec3::new(point.x, point.z, -point.y) * 0.1;
            let Ok(point) = self.camera.0.world_to_viewport(self.camera.1, point) else {
                return Ok(false);
            };
            if !viewport.contains(point) {
                return Ok(false);
            }
            let rect = bounds.get_or_insert_with(|| Rect::from_corners(point, point));
            rect.min = rect.min.min(point);
            rect.max = rect.max.max(point);
        }
        Ok(bounds.is_some_and(|rect| rect.width() >= 3.0 && rect.height() >= 3.0))
    }

    fn shroud_pixels(
        &self,
        battle: &Match,
        point: [f32; 3],
        material: &TreeMaterial,
    ) -> Result<[[u8; 4]; 4], Box<dyn Error>> {
        if material.shroud.as_ref() != self.fog.texture() || material.shroud.is_none() {
            return Err("tree shader does not bind actual shroud texture".into());
        }
        let (offset, scale) = self.fog.tree_mapping();
        if scale.contains(&0.0)
            || material
                .parameters
                .shroud_offset
                .truncate()
                .truncate()
                .to_array()
                .map(f32::to_bits)
                != offset.map(f32::to_bits)
            || material
                .parameters
                .shroud_scale
                .truncate()
                .truncate()
                .to_array()
                .map(f32::to_bits)
                != scale.map(f32::to_bits)
        {
            return Err("tree shader shroud mapping differs from native texture".into());
        }
        let image = self
            .images
            .get(material.shroud.as_ref().unwrap())
            .ok_or("uploaded shroud texture missing")?;
        let width = image.texture_descriptor.size.width;
        let height = image.texture_descriptor.size.height;
        let data = image
            .data
            .as_ref()
            .ok_or("uploaded shroud pixels missing")?;
        let mut result = [[0; 4]; 4];
        for (color, position) in result
            .iter_mut()
            .zip(cell_centers(&battle.simulation, point))
        {
            let dimensions = [width, height];
            let texel: [u32; 2] = std::array::from_fn(|axis| {
                let coordinate = (position[axis] + offset[axis])
                    * scale[axis]
                    * dimensions[axis].to_f32().expect("native texture extent");
                u32::try_from(native_real::floor(coordinate)).expect("in-map tree texture sample")
            });
            if texel[0] >= width || texel[1] >= height {
                return Err("selected original tree outside shroud texture".into());
            }
            let start = usize::try_from((texel[1] * width + texel[0]) * 4)?;
            color.copy_from_slice(
                data.get(start..start + 4)
                    .ok_or("native shroud texel missing")?,
            );
        }
        Ok(result)
    }
}
