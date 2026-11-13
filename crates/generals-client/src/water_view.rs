//! `SuperHackers` `W3DWater` polygon surfaces, river strips and terrain shoreline mask.
pub mod data;
pub mod geometry;
mod material;
mod shore;
#[cfg(test)]
mod tests;

use crate::{playtest::Match, terrain::Terrain, visibility_view::Fog};
use bevy::{
    asset::embedded_asset,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    ecs::system::SystemParam,
    prelude::*,
};
use generals_assets::{map::PolygonTrigger, virtual_assets::VirtualAssets};
use material::{Parameters, WaterMaterial};
use num_traits::ToPrimitive;
use std::error::Error;

pub struct WaterPlugin;
impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "water_view.wgsl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default());
        app.add_systems(
            Update,
            update
                .after(crate::visibility_view::update)
                .run_if(resource_exists::<Match>),
        );
    }
}

#[derive(Resource)]
pub struct WaterView {
    settings: data::Settings,
    polygons: Vec<geometry::Polygon>,
    shoreline: Mesh,
    images: Option<[Image; 4]>,
    materials: Vec<Handle<WaterMaterial>>,
    phase: f32,
    generation: Option<u32>,
}
impl WaterView {
    pub fn load(
        assets: &VirtualAssets,
        triggers: &[PolygonTrigger],
        terrain: &Terrain,
        light: [f32; 3],
        time_of_day: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let settings = data::load(assets, time_of_day, light)?;
        let polygons = geometry::polygons(triggers)?;
        let shoreline = shore::mesh(triggers, terrain, &settings)?;
        let mut images = Vec::new();
        for name in [
            settings.texture.as_str(),
            "WaterSurfaceBubbles.tga",
            "Noise0000.tga",
            "TWAlphaEdge.tga",
        ] {
            let mut image = crate::source_texture::load(assets, name)?;
            image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
            images.push(image);
        }
        let images: [Image; 4] = images
            .try_into()
            .map_err(|_| "four original water textures")?;
        Ok(Self {
            settings,
            polygons,
            shoreline,
            images: Some(images),
            materials: Vec::new(),
            phase: 0.0,
            generation: None,
        })
    }
}

#[derive(Component)]
pub struct WaterVisual {
    index: usize,
}

#[derive(SystemParam)]
struct Resources<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<WaterMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    fog: Res<'w, Fog>,
}

fn update(
    mut commands: Commands,
    mut water: ResMut<WaterView>,
    battle: Res<Match>,
    time: Res<Time<Virtual>>,
    mut resources: Resources,
    visuals: Query<(Entity, &WaterVisual, &Mesh3d)>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    if !battle.started {
        for (entity, _, mesh) in &visuals {
            commands.entity(entity).despawn();
            resources.meshes.remove(&mesh.0);
        }
        water.generation = None;
        return;
    }
    if water.polygons.is_empty() {
        return;
    }
    let Some(shroud) = resources.fog.texture().cloned() else {
        return;
    };
    prepare_materials(&mut water, &mut resources, &shroud);
    let step = 0.002 * (time.delta_secs() * 30.0);
    water.phase += step;
    let game = &battle.simulation;
    let dimensions = game.visibility_dimensions();
    let extent = Vec4::new(
        f32::from(dimensions[0]) * game.visibility_cell_size() * 0.1,
        f32::from(dimensions[1]) * game.visibility_cell_size() * 0.1,
        0.0,
        0.0,
    );
    for handle in &water.materials {
        let mut material = resources
            .materials
            .get_mut(handle)
            .expect("water material loaded");
        material.parameters.phase = water.phase;
        material.parameters.extent = extent;
        material.shroud = Some(shroud.clone());
    }
    if water.generation == Some(battle.generation) {
        for (_, visual, handle) in &visuals {
            if visual.index < 2 {
                continue;
            }
            let mut mesh = geometry::mesh(
                &water.polygons[visual.index - 2],
                &water.settings,
                water.phase,
                |point| resources.fog.level(&game.visibility_cell(0, point)),
            );
            mesh.final_aabb = Some(bevy::math::bounding::Aabb3d::new(Vec3::ZERO, Vec3::ZERO));
            if let Some(mut current) = resources.meshes.get_mut(&handle.0) {
                *current = mesh;
            }
        }
    } else {
        for (entity, _, mesh) in &visuals {
            commands.entity(entity).despawn();
            resources.meshes.remove(&mesh.0);
        }
        for index in 0..water.materials.len() {
            // BaseHeightMapRenderObjClass::renderShoreLinesSorted skips zero shoreline tiles.
            if index == 1 && water.shoreline.count_vertices() == 0 {
                continue;
            }
            let mut mesh = match index {
                0 => shore::screen(),
                1 => water.shoreline.clone(),
                _ => geometry::mesh(
                    &water.polygons[index - 2],
                    &water.settings,
                    water.phase,
                    |point| resources.fog.level(&game.visibility_cell(0, point)),
                ),
            };
            mesh.final_aabb = Some(bevy::math::bounding::Aabb3d::new(Vec3::ZERO, Vec3::ZERO));
            commands.spawn((
                WaterVisual { index },
                Mesh3d(resources.meshes.add(mesh)),
                MeshMaterial3d(water.materials[index].clone()),
                Transform::IDENTITY,
                RenderLayers::layer(if index < 2 { 5 } else { 6 }),
                NoFrustumCulling,
            ));
        }
        water.generation = Some(battle.generation);
    }
}

fn prepare_materials(water: &mut WaterView, resources: &mut Resources, shroud: &Handle<Image>) {
    if let Some(images) = water.images.take() {
        let textures = images.map(|image| resources.images.add(image));
        let lookup = resources.images.add(shore::lookup(water.settings.opacity));
        for index in 0..water.polygons.len() + 2 {
            let mode = if index < 2 {
                u32::try_from(index).expect("shore pass")
            } else if water.polygons[index - 2].river.is_some() {
                3
            } else {
                2
            };
            let material = WaterMaterial {
                parameters: Parameters {
                    mode,
                    phase: water.phase,
                    opacity: water.settings.opacity,
                    padding: 0.0,
                    extent: Vec4::ZERO,
                },
                water: Some(textures[0].clone()),
                sparkles: Some(textures[1].clone()),
                noise: Some(textures[2].clone()),
                edge: Some(textures[3].clone()),
                shroud: Some(shroud.clone()),
                lookup: Some(lookup.clone()),
                additive: water.settings.additive,
                soft: water.settings.soft && water.settings.depth != 0.0,
                // All transforms have the same origin. Bevy's transparent sort
                // bias therefore retains original PolygonTrigger list order.
                order: index.to_f32().expect("water polygon ordinal"),
            };
            water.materials.push(resources.materials.add(material));
        }
    }
}
