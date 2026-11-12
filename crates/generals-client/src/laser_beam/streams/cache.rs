//! `W3DLaserDraw` retains segments until their endpoints or viewing transform change.
use super::{RenderResources, Stream, Streams};
use crate::terrain::Terrain;
use bevy::{mesh::VertexAttributeValues, prelude::*};

pub(super) struct Geometry {
    points: [[u32; 3]; 2],
    camera: [u32; 12],
    base_v: Vec<f32>,
    offset: u32,
    width_scale: u32,
    origin: [u32; 3],
}

impl Geometry {
    pub(super) fn build(
        source: &Streams,
        stream: &Stream,
        terrain: &Terrain,
        camera: &Transform,
    ) -> Result<(Mesh, Self), &'static str> {
        let mut mesh = source.mesh(
            stream.points,
            stream.width_scale,
            stream.origin,
            terrain,
            camera,
        )?;
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return Err("assist arc has no original UV coordinates");
        };
        let geometry = Self {
            points: stream.points.map(|point| point.map(f32::to_bits)),
            camera: camera.compute_affine().to_cols_array().map(f32::to_bits),
            base_v: uv.iter().map(|point| point[1]).collect(),
            offset: stream.offset.to_bits(),
            width_scale: stream.width_scale.to_bits(),
            origin: stream.origin.map(f32::to_bits),
        };
        geometry.scroll(&mut mesh, stream.offset)?;
        Ok((mesh, geometry))
    }

    pub(super) fn refresh(
        &mut self,
        source: &Streams,
        stream: &Stream,
        camera: &Transform,
        resources: &mut RenderResources,
        handle: &Handle<Mesh>,
    ) -> Result<(), &'static str> {
        if self.origin != stream.origin.map(f32::to_bits)
            || self.width_scale != stream.width_scale.to_bits()
            || self.points != stream.points.map(|point| point.map(f32::to_bits))
            || self.camera != camera.compute_affine().to_cols_array().map(f32::to_bits)
            || !resources.meshes.contains(handle)
        {
            let (mesh, geometry) = Self::build(source, stream, &resources.terrain, camera)?;
            resources
                .meshes
                .insert(handle.id(), mesh)
                .map_err(|_| "assist arc asset generation changed")?;
            *self = geometry;
        } else if self.offset != stream.offset.to_bits() {
            let mut mesh = resources
                .meshes
                .get_mut(handle)
                .ok_or("assist arc asset disappeared")?;
            self.scroll(&mut mesh, stream.offset)?;
            self.offset = stream.offset.to_bits();
        }
        Ok(())
    }

    fn scroll(&self, mesh: &mut Mesh, offset: f32) -> Result<(), &'static str> {
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
        else {
            return Err("assist arc has no original UV coordinates");
        };
        if uv.len() != self.base_v.len() {
            return Err("assist arc UV count changed");
        }
        for (point, &base) in uv.iter_mut().zip(&self.base_v) {
            // Preserve the native tiled-V + offset operation. Subtracting the
            // previous offset would accumulate rounded UV errors every frame.
            point[1] = base + offset;
        }
        Ok(())
    }
}
