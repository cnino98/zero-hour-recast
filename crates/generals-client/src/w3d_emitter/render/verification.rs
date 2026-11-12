//! Native capture reads the actual standalone emitter mesh and material.
use super::{EmitterVisual, Emitters, SourceMaterial};
use crate::playtest_view::WorldCamera;
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

type Visual = (
    &'static EmitterVisual,
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static GlobalTransform,
    &'static InheritedVisibility,
    &'static ViewVisibility,
);

#[derive(SystemParam)]
pub struct Frame<'w, 's> {
    emitters: Res<'w, Emitters>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    visuals: Query<'w, 's, Visual>,
}

impl Frame<'_, '_> {
    pub fn buffer_id(&self, object: usize, generation: u32) -> Option<u64> {
        if self.emitters.generation != Some(generation) {
            return None;
        }
        self.emitters
            .scene
            .emitters
            .iter()
            .find(|emitter| emitter.object == object)
            .map(|emitter| emitter.id)
    }

    pub fn retired(&self, id: u64, generation: u32) -> bool {
        self.emitters.generation == Some(generation)
            && !self
                .emitters
                .scene
                .emitters
                .iter()
                .any(|emitter| emitter.id == id)
            && !self
                .visuals
                .iter()
                .any(|(visual, ..)| visual.id == id && visual.generation == generation)
    }

    pub fn retiring(&self, object: usize, generation: u32) -> Result<bool, Box<dyn Error>> {
        if self.emitters.generation != Some(generation) {
            return Ok(false);
        }
        let emitter = self
            .emitters
            .scene
            .emitters
            .iter()
            .find(|emitter| emitter.object == object)
            .ok_or("WaterSpray buffer retired before its real particle tail was observed")?;
        Ok(emitter.buffer.dead && !emitter.buffer.particles.is_empty())
    }

    pub fn visible(
        &self,
        object: usize,
        generation: u32,
        viewport: Option<Rect>,
    ) -> Result<bool, Box<dyn Error>> {
        let Some(viewport) = viewport else {
            return Ok(false);
        };
        let Some(id) = self.buffer_id(object, generation) else {
            return Ok(false);
        };
        let emitter = self
            .emitters
            .scene
            .emitters
            .iter()
            .find(|emitter| emitter.id == id)
            .ok_or("observed emitter lost its buffer")?;
        let Some((_, mesh, material, transform, inherited, visible)) = self
            .visuals
            .iter()
            .find(|(visual, ..)| visual.id == id && visual.generation == generation)
        else {
            return Ok(false);
        };
        if !inherited.get() || !visible.get() {
            return Ok(false);
        }
        let binding = self.emitters.definitions.bindings[emitter.template]
            .as_ref()
            .ok_or("observed emitter lost its original binding")?;
        if self.emitters.textures[binding.texture].material.as_ref() != Some(&material.0)
            || !emitter
                .buffer
                .definition
                .texture
                .eq_ignore_ascii_case("exhydrant.tga")
        {
            return Err("WaterSpray does not bind original EXHydrant texture material".into());
        }
        let material = self
            .materials
            .get(&material.0)
            .ok_or("emitter material unavailable")?;
        if material.parameters.primary_gradient != 0 || material.parameters.vertex_diffuse != 3 {
            return Err(
                "standalone emitter material lost original texture-only unshrouded pass".into(),
            );
        }
        let image = material
            .texture
            .as_ref()
            .and_then(|handle| self.images.get(handle))
            .ok_or("original emitter texture unavailable")?;
        if image.data.as_ref().is_none_or(Vec::is_empty) {
            return Err("original emitter texture has no uploaded pixels".into());
        }
        let mesh = self.meshes.get(&mesh.0).ok_or("emitter mesh unavailable")?;
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return Err("emitter mesh lacks quad positions".into());
        };
        if points.is_empty() {
            return Ok(false);
        }
        if points.len() != emitter.buffer.particles.len() * 4
            || mesh
                .indices()
                .is_none_or(|indices| indices.len() != emitter.buffer.particles.len() * 6)
        {
            return Err("rendered emitter mesh differs from actual particle buffer".into());
        }
        let mut bounds = None::<Rect>;
        for point in points {
            let point = transform.transform_point(Vec3::from_array(*point));
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
        Ok(bounds.is_some_and(|bounds| bounds.width() >= 2.0 && bounds.height() >= 2.0))
    }
}
