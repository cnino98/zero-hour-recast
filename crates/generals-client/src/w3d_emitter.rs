//! Original standalone WW3D emitter draw and persistent particle buffers.
mod buffer;
mod data;
mod pose;
mod random;
mod render;
mod runtime;
#[cfg(test)]
mod tests;
use bevy::prelude::*;
pub use data::validate;
pub use data::{load, validate_draw, validate_object};
use generals_assets::w3d::emitter::Definition;
pub use render::{update, verification};
use std::sync::Arc;

struct Binding {
    name: String,
    definition: Arc<Definition>,
    texture: usize,
}

pub struct Definitions {
    bindings: Vec<Option<Binding>>,
    images: Vec<Image>,
}

struct Texture {
    image: Option<Image>,
    material: Option<Handle<crate::source_material::SourceMaterial>>,
}

#[derive(Resource)]
pub struct Emitters {
    definitions: Definitions,
    scene: runtime::Scene,
    textures: Vec<Texture>,
    generation: Option<u32>,
}

impl Emitters {
    fn new() -> Self {
        Self {
            definitions: Definitions {
                bindings: Vec::new(),
                images: Vec::new(),
            },
            scene: runtime::Scene::new(),
            textures: Vec::new(),
            generation: None,
        }
    }

    fn configure(&mut self, mut definitions: Definitions) {
        self.scene.reset();
        self.generation = None;
        self.textures = definitions
            .images
            .drain(..)
            .map(|image| Texture {
                image: Some(image),
                material: None,
            })
            .collect();
        self.definitions = definitions;
    }

    pub fn created(
        &mut self,
        game: &generals_sim::skirmish::Skirmish,
        generation: u32,
        object: usize,
        type_index: usize,
        validate_logic: bool,
        now: u32,
    ) {
        if self.generation != Some(generation) {
            self.scene.reset();
            self.generation = Some(generation);
        }
        let Some(binding) = self
            .definitions
            .bindings
            .get(type_index)
            .and_then(Option::as_ref)
        else {
            return;
        };
        let (position, basis) = game.object_transform(object);
        self.scene.create(
            object,
            type_index,
            binding,
            pose::Pose::new(position, basis),
            now,
            validate_logic,
        );
    }

    pub fn destroyed(&mut self, object: usize) {
        self.scene.destroy(object);
    }
}

/// Replace map scene bindings while retaining native process random streams.
pub fn replace_scene(world: &mut World, definitions: Definitions) {
    let mut emitters = world
        .remove_resource::<Emitters>()
        .unwrap_or_else(Emitters::new);
    emitters.configure(definitions);
    world.insert_resource(emitters);
}
