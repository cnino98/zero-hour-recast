//! Original `SnowManager` weather lattice and map-script screen fades.
pub mod data;
mod geometry;
mod material;
#[cfg(test)]
mod tests;
use crate::{playtest::Match, playtest_view::WorldCamera};
use bevy::{
    asset::embedded_asset,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    ecs::system::SystemParam,
    prelude::*,
};
use generals_assets::virtual_assets::VirtualAssets;
use material::{Parameters, WeatherMaterial};
use std::error::Error;

pub struct WeatherPlugin;
impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "weather_view.wgsl");
        app.add_plugins(MaterialPlugin::<WeatherMaterial>::default());
        app.add_systems(
            Update,
            update
                .after(crate::playtest::update_camera)
                .run_if(resource_exists::<Match>),
        );
    }
}

#[derive(Resource)]
pub struct WeatherView {
    settings: data::Settings,
    noise: Vec<f32>,
    random: u32,
    overridden: bool,
    logic_generation: Option<u32>,
    image: Option<Image>,
    particle_material: Option<Handle<WeatherMaterial>>,
    fade_material: Option<Handle<WeatherMaterial>>,
    time: f32,
    generation: Option<u32>,
}
impl WeatherView {
    pub(crate) fn capture_state(&self) -> (u32, f32, u64) {
        let hash = self
            .noise
            .iter()
            .fold(14_695_981_039_346_656_037_u64, |hash, value| {
                (hash ^ u64::from(value.to_bits())).wrapping_mul(1_099_511_628_211)
            });
        (self.random, self.time, hash)
    }

    pub fn load(
        assets: &VirtualAssets,
        baseline: data::Settings,
        overridden: bool,
        previous: Option<&Self>,
    ) -> Result<Self, Box<dyn Error>> {
        let override_settings = overridden.then(|| data::load(assets)).transpose()?;
        let mut result = Self::configure(baseline, override_settings, previous);
        result.image = if result.settings.enabled {
            let mut image = crate::source_texture::load(assets, &result.settings.texture)?;
            image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
            Some(image)
        } else {
            None
        };
        Ok(result)
    }

    fn configure(
        baseline: data::Settings,
        override_settings: Option<data::Settings>,
        previous: Option<&Self>,
    ) -> Self {
        let overridden = override_settings.is_some();
        let mut random = previous.map_or(1, |previous| previous.random);
        let mut noise = previous.map_or_else(
            || geometry::noise(baseline.dimension, &mut random),
            |previous| previous.noise.clone(),
        );
        let settings = if let Some(settings) = override_settings {
            noise = geometry::noise(settings.dimension, &mut random);
            settings
        } else if let Some(previous) = previous {
            // Reset deletes the INI override but SnowManager::reset does not
            // regenerate noise or overwrite the manager's cached geometry.
            // Render/ReAcquireResources read these three global fields directly.
            let mut settings = previous.settings.clone();
            settings.enabled = baseline.enabled;
            settings.points = baseline.points;
            settings.texture = baseline.texture;
            settings
        } else {
            baseline
        };
        Self {
            settings,
            noise,
            random,
            overridden,
            logic_generation: None,
            image: None,
            particle_material: None,
            fade_material: None,
            time: previous.map_or(0.0, |previous| previous.time),
            generation: None,
        }
    }

    fn begin_generation(&mut self, generation: u32) {
        if self
            .logic_generation
            .is_some_and(|previous| previous != generation)
            && self.overridden
        {
            self.noise = geometry::noise(self.settings.dimension, &mut self.random);
        }
        self.logic_generation = Some(generation);
    }
}

#[derive(Component)]
struct WeatherVisual {
    screen: bool,
}
#[derive(SystemParam)]
struct Resources<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<WeatherMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

fn update(
    mut commands: Commands,
    mut weather: ResMut<WeatherView>,
    battle: Res<Match>,
    time: Res<Time<Virtual>>,
    camera: Single<(&Camera, &Transform, &Projection), With<WorldCamera>>,
    mut resources: Resources,
    visuals: Query<(Entity, &WeatherVisual, &Mesh3d)>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    let (camera, transform, projection) = camera.into_inner();
    if battle.started {
        weather.begin_generation(battle.generation);
    }
    let rebuild = weather.generation != Some(battle.generation);
    if !battle.started || rebuild {
        for (entity, _, mesh) in &visuals {
            commands.entity(entity).despawn();
            resources.meshes.remove(&mesh.0);
        }
        weather.generation = None;
        if !battle.started {
            return;
        }
    }
    weather.generation = Some(battle.generation);
    weather.time = (weather.time + time.delta_secs())
        % (weather.settings.dimension / weather.settings.velocity);
    if let Some(image) = weather.image.take() {
        let texture = resources.images.add(image);
        weather.particle_material = Some(resources.materials.add(WeatherMaterial {
            parameters: Parameters {
                screen: 0,
                amount: 0.0,
            },
            texture: Some(texture),
            additive: false,
        }));
    }
    if weather.settings.enabled
        && let Some(viewport) = camera.physical_viewport_size()
    {
        let mesh = geometry::mesh(
            &weather.settings,
            &weather.noise,
            weather.time,
            transform,
            projection.get_clip_from_view(),
            viewport.as_vec2(),
        );
        if !rebuild
            && let Some((_, _, handle)) = visuals.iter().find(|(_, visual, _)| !visual.screen)
        {
            if let Some(mut current) = resources.meshes.get_mut(&handle.0) {
                *current = mesh;
            }
        } else {
            commands.spawn((
                WeatherVisual { screen: false },
                Mesh3d(resources.meshes.add(mesh)),
                MeshMaterial3d(
                    weather
                        .particle_material
                        .as_ref()
                        .expect("weather material loaded")
                        .clone(),
                ),
                Transform::IDENTITY,
                RenderLayers::layer(9),
                NoFrustumCulling,
            ));
        }
    }
    let fade = battle.simulation.map_script_fade();
    if let Some(fade) = fade {
        // W3DStatusCircle::Render packs the channel as Int(255*intensity).
        let amount = (fade.amount * 255.0).trunc() / 255.0;
        let handle = weather.fade_material.get_or_insert_with(|| {
            resources.materials.add(WeatherMaterial {
                parameters: Parameters { screen: 1, amount },
                texture: None,
                additive: fade.additive,
            })
        });
        if let Some(mut material) = resources.materials.get_mut(&*handle) {
            material.parameters.amount = amount;
            material.additive = fade.additive;
        }
        if rebuild || !visuals.iter().any(|(_, visual, _)| visual.screen) {
            commands.spawn((
                WeatherVisual { screen: true },
                Mesh3d(resources.meshes.add(geometry::screen())),
                MeshMaterial3d(handle.clone()),
                Transform::IDENTITY,
                RenderLayers::layer(10),
                NoFrustumCulling,
            ));
        }
    } else {
        for (entity, visual, mesh) in &visuals {
            if !rebuild && visual.screen {
                commands.entity(entity).despawn();
                resources.meshes.remove(&mesh.0);
            }
        }
    }
}
