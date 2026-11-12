//! Native point-pulse selection, drawable light environments and terrain lighting.
use crate::{
    model_animation::Animations,
    particles::Particles,
    playtest_view::UnitVisual,
    source_material::{Parameters, SourceMaterial},
};
use bevy::{ecs::system::SystemParam, prelude::*};
use generals_assets::w3d::{Mesh as SourceMesh, Model};
use num_traits::ToPrimitive;
use std::error::Error;

#[derive(Component, Clone, Copy)]
pub struct Bounds {
    center: Vec3,
    radius: f32,
    pub native_height: f32,
}
impl Bounds {
    pub fn from_model(model: &Model, mesh: &SourceMesh) -> Result<Self, Box<dyn Error>> {
        let (center, radius) = if let Some(sphere) = &model.scene_sphere {
            (sphere.center, sphere.radius)
        } else if !model.hlod {
            (mesh.sphere_center, mesh.sphere_radius)
        } else {
            return Err(format!(
                "dynamic lighting needs resolved HLOD prototype bounds for {}",
                mesh.name
            )
            .into());
        };
        Ok(Self {
            center: Vec3::from_array(center),
            radius,
            native_height: 0.0,
        })
    }
}

#[derive(Component, Clone, Copy)]
pub struct DrawPose {
    pub position: [f32; 3],
    pub basis: [[f32; 3]; 3],
}
impl DrawPose {
    #[must_use]
    pub fn from_object(game: &generals_sim::skirmish::Skirmish, id: usize) -> Self {
        let (position, basis) = game.object_transform(id);
        let mut result = Self { position, basis };
        if let Some(instance) = game.entities[id].drawable_instance_matrix() {
            result = result.compose(Self {
                position: instance.map(|row| row[3]),
                basis: instance.map(|row| [row[0], row[1], row[2]]),
            });
        }
        result
    }
    #[must_use]
    pub fn point(&self, local: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|axis| {
            let x = self.basis[axis][0] * local[0];
            let y = self.basis[axis][1] * local[1];
            let z = self.basis[axis][2] * local[2];
            ((x + y) + z) + self.position[axis]
        })
    }
    #[must_use]
    pub fn compose(self, local: Self) -> Self {
        Self {
            position: self.point(local.position),
            basis: std::array::from_fn(|row| {
                std::array::from_fn(|column| {
                    let x = self.basis[row][0] * local.basis[0][column];
                    let y = self.basis[row][1] * local.basis[1][column];
                    let z = self.basis[row][2] * local.basis[2][column];
                    (x + y) + z
                })
            }),
        }
    }
}

struct Sample {
    position: [f32; 3],
    color: [f32; 3],
    outer_radius: f32,
}

#[derive(Resource, Default)]
pub struct Frame {
    active: Vec<Sample>,
}
impl Frame {
    fn capture(&mut self, pool: &crate::light_pulse::Pool) {
        self.active.clear();
        self.active.extend(
            pool.lights()
                .iter()
                .filter(|light| light.enabled)
                .map(|light| Sample {
                    position: light.position,
                    color: light.color,
                    outer_radius: light.outer_radius,
                }),
        );
    }
}

#[derive(SystemParam)]
pub struct DrawLights<'w> {
    frame: Option<Res<'w, Frame>>,
    animations: Option<Res<'w, Animations>>,
    battle: Option<Res<'w, crate::playtest::Match>>,
}
impl DrawLights<'_> {
    /// # Panics
    /// Active native pulses require their match and original draw definitions.
    #[must_use]
    pub fn affects(
        &self,
        actor: &UnitVisual,
        pose: Option<&DrawPose>,
        bounds: Option<&Bounds>,
    ) -> bool {
        let Some(frame) = self.frame.as_ref().filter(|frame| !frame.active.is_empty()) else {
            return false;
        };
        let (Some(pose), Some(bounds)) = (pose, bounds) else {
            return false;
        };
        let center = Vec3::from_array(pose.point([
            bounds.center.x,
            bounds.center.y,
            bounds.center.z + bounds.native_height,
        ]));
        let battle = self
            .battle
            .as_ref()
            .expect("active light frame requires its match");
        let animations = self
            .animations
            .as_ref()
            .expect("active light frame requires original draw modules");
        actor.generation == battle.generation
            && animations
                .receives_dynamic_lights(battle.simulation.entities[actor.index].type_index)
            && matches!(
                battle
                    .simulation
                    .visibility_status(battle.observer_player.unwrap_or(0), actor.index),
                generals_sim::skirmish::visibility::Status::Clear
                    | generals_sim::skirmish::visibility::Status::Partial
            )
            && frame.active.iter().any(|light| {
                light.color.iter().any(|channel| *channel >= 0.05)
                    && intersects(
                        center,
                        bounds.radius,
                        Vec3::from_array(light.position),
                        light.outer_radius,
                    )
            })
    }
    /// # Panics
    /// Active native pulses require their match and original draw definitions.
    pub fn apply(
        &self,
        actor: &UnitVisual,
        pose: Option<&DrawPose>,
        bounds: Option<&Bounds>,
        parameters: &mut Parameters,
    ) -> bool {
        let Some(frame) = self.frame.as_ref().filter(|frame| !frame.active.is_empty()) else {
            return false;
        };
        let (Some(pose), Some(bounds)) = (pose, bounds) else {
            return false;
        };
        let battle = self
            .battle
            .as_ref()
            .expect("active light frame requires its match");
        let animations = self
            .animations
            .as_ref()
            .expect("active light frame requires original draw modules");
        if actor.generation != battle.generation {
            return false;
        }
        let unit = &battle.simulation.entities[actor.index];
        if !animations.receives_dynamic_lights(unit.type_index)
            || !matches!(
                battle
                    .simulation
                    .visibility_status(battle.observer_player.unwrap_or(0), actor.index),
                generals_sim::skirmish::visibility::Status::Clear
                    | generals_sim::skirmish::visibility::Status::Partial
            )
        {
            return false;
        }
        let center = Vec3::from_array(pose.point([
            bounds.center.x,
            bounds.center.y,
            bounds.center.z + bounds.native_height,
        ]));
        environment(parameters, center, bounds.radius, &frame.active);
        true
    }
}

#[derive(SystemParam)]
pub struct Resources<'w> {
    particles: ResMut<'w, Particles>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    frame: ResMut<'w, Frame>,
    terrain: Res<'w, crate::terrain::Terrain>,
    meshes: ResMut<'w, Assets<Mesh>>,
    battle: Res<'w, crate::playtest::Match>,
}

// W3DScene::draw updates dynamic lights once before rendering terrain and Drawables.
pub fn update(
    mut resources: Resources,
    surfaces: Query<
        (&Mesh3d, &MeshMaterial3d<SourceMaterial>),
        With<crate::terrain::changes::Surface>,
    >,
    mut terrain_state: Local<TerrainState>,
) {
    resources.particles.pulses.advance_frame();
    update_terrain(&mut resources, &surfaces, &mut terrain_state);
    let Resources {
        particles, frame, ..
    } = &mut resources;
    frame.capture(&particles.pulses);
}

struct Input {
    contribution: f32,
    color: Vec4,
    direction: Vec4,
    point: Vec4,
    ambient: Vec4,
}

fn insert(lights: &mut [Option<Input>; 4], light: Input) {
    let Some(index) = lights.iter().position(|other| {
        other
            .as_ref()
            .is_none_or(|other| light.contribution > other.contribution)
    }) else {
        return;
    };
    for slot in (index + 1..4).rev() {
        lights[slot] = lights[slot - 1].take();
    }
    lights[index] = Some(light);
}

fn environment(parameters: &mut Parameters, center: Vec3, radius: f32, pulses: &[Sample]) {
    let mut lights: [Option<Input>; 4] = [None, None, None, None];
    for index in 0..4 {
        let color = parameters.light_color[index];
        if color.x < 0.05 && color.y < 0.05 && color.z < 0.05 {
            continue;
        }
        insert(
            &mut lights,
            Input {
                contribution: color.truncate().length_squared(),
                color,
                direction: parameters.light_direction[index],
                point: Vec4::ZERO,
                ambient: Vec4::ZERO,
            },
        );
    }
    let mut ambient = parameters.scene_ambient.truncate();
    for light in pulses {
        let color = Vec3::from_array(light.color);
        if color.x < 0.05 && color.y < 0.05 && color.z < 0.05 {
            continue;
        }
        let position = Vec3::from_array(light.position);
        if !intersects(center, radius, position, light.outer_radius) {
            continue;
        }
        let delta = position - center;
        let distance = squared_length(delta).sqrt();
        let attenuation = center_attenuation(distance, light.outer_radius);
        ambient += color * attenuation;
        insert(
            &mut lights,
            Input {
                contribution: (color * attenuation).length_squared(),
                color: color.extend(0.0),
                direction: Vec4::new(
                    0.0,
                    0.0,
                    0.0,
                    if (f64::from(light.outer_radius) - 1.0).abs() < 1e-5 {
                        0.0
                    } else {
                        0.1
                    },
                ),
                point: (Vec3::new(position.x, position.z, -position.y) * 0.1)
                    .extend(light.outer_radius),
                ambient: color.extend(
                    (8.0_f64 / (f64::from(light.outer_radius) * f64::from(light.outer_radius)))
                        .to_f32()
                        .expect("native attenuation"),
                ),
            },
        );
    }
    parameters.scene_ambient = Vec3::from_array(ambient.to_array().map(|value| {
        f32::from(
            (value.clamp(0.0, 1.0) * 255.0)
                .to_u8()
                .expect("clamped ambient"),
        ) / 255.0
    }))
    .extend(0.0);
    parameters.light_color = [Vec4::ZERO; 4];
    parameters.light_direction = [Vec4::ZERO; 4];
    parameters.point_position = [Vec4::ZERO; 4];
    parameters.point_ambient = [Vec4::ZERO; 4];
    for (index, light) in lights.into_iter().flatten().enumerate() {
        parameters.light_color[index] = light.color;
        parameters.light_direction[index] = light.direction;
        parameters.point_position[index] = light.point;
        parameters.point_ambient[index] = light.ambient;
    }
}

fn center_attenuation(distance: f32, outer: f32) -> f32 {
    let (distance, outer) = (f64::from(distance), f64::from(outer));
    if (outer - 1.0).abs() < f64::from(0.0001_f32) {
        if distance > 1.0 { 0.0 } else { 1.0 }
    } else {
        (1.0 - (distance - 1.0) / (outer - 1.0))
            .to_f32()
            .expect("native attenuation")
            .clamp(0.0, 1.0)
    }
}

fn update_terrain(
    resources: &mut Resources,
    surfaces: &Query<
        (&Mesh3d, &MeshMaterial3d<SourceMaterial>),
        With<crate::terrain::changes::Surface>,
    >,
    state: &mut TerrainState,
) {
    let region = resources.terrain.draw_region();
    let mut positions = [Vec4::ZERO; 20];
    let mut colors = [Vec4::ZERO; 20];
    let mut count = 0;
    let mut regions = [None; 20];
    for light in resources.particles.pulses.lights_mut() {
        if light.enabled || light.prior_enabled {
            if light.prior_enabled {
                light.previous_bounds = light.current_bounds;
            }
            let [x, y, _] = light.position;
            let range = light.outer_radius;
            light.current_bounds = [
                (x - range) / 10.0,
                (y - range) / 10.0,
                (x + range) / 10.0 + 1.0,
                (y + range) / 10.0 + 1.0,
            ]
            .map(|value| value.to_i32().expect("native terrain light grid"));
            if !light.prior_enabled {
                light.previous_bounds = light.current_bounds;
            }
            let intersects = |bounds: [i32; 4]| {
                let [min_x, min_y, max_x, max_y] =
                    bounds.map(|value| value.to_f32().expect("native terrain grid"));
                min_x < region[1][0] / 10.0 + 1.0
                    && min_y < region[1][1] / 10.0 + 1.0
                    && max_x > region[0][0] / 10.0
                    && max_y > region[0][1] / 10.0
            };
            if intersects(light.current_bounds) || intersects(light.previous_bounds) {
                regions[count] = Some(terrain::Region {
                    current: light.current_bounds,
                    previous: light.previous_bounds,
                });
                if light.enabled {
                    positions[count] = Vec3::from_array(light.position).extend(range);
                    colors[count] = Vec3::from_array(light.color).extend(0.0);
                }
                count += 1;
                if count == 20 {
                    break;
                }
            }
        }
        light.prior_enabled = light.enabled;
    }
    terrain::apply(resources, surfaces, state, &positions, &colors, &regions);
}

fn intersects(center: Vec3, radius: f32, other: Vec3, other_radius: f32) -> bool {
    let delta = center - other;
    let sum = radius + other_radius;
    squared_length(delta) < sum * sum
}

#[cfg(test)]
mod tests;

fn squared_length(delta: Vec3) -> f32 {
    let x = delta.x * delta.x;
    let y = delta.y * delta.y;
    let z = delta.z * delta.z;
    (x + y) + z
}

mod terrain;

#[derive(Default)]
pub struct TerrainState {
    meshes: std::collections::HashMap<AssetId<Mesh>, terrain::MeshState>,
}
