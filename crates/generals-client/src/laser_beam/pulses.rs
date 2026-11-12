//! `Weapon::createLaser`: independent short-lived beams with fixed end positions.
mod data;
mod inspection;
pub use inspection::PulseInspection;

use super::{Beam, RenderResources, geometry};
use crate::{
    model_animation::{Animations, Players},
    particles::smudge::SceneMaterial,
    playtest::Match,
    playtest_view::UnitVisual,
};
use bevy::{
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::VertexAttributeValues,
    prelude::*,
};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::visibility::Status;
use std::{collections::BTreeMap, error::Error};

struct Template {
    name: String,
    beams: Vec<Beam>,
    particles: [String; 2],
    lifetime: [u32; 2],
    tiling: f32,
    scroll: f32,
    aspect: f32,
    image: Option<Image>,
    material: Option<Handle<SceneMaterial>>,
}
struct Binding {
    template: usize,
    bone: String,
}
struct Pulse {
    serial: u64,
    source: usize,
    weapon: String,
    points: [[f32; 3]; 2],
    origin: [f32; 2],
    expires: u64,
    offset: f32,
    last_used: std::time::Duration,
    emitters: [Option<u64>; 2],
}
pub(super) struct Pulses {
    templates: Vec<Template>,
    bindings: BTreeMap<String, Binding>,
    active: Vec<Pulse>,
    generation: u32,
    serial: u64,
}

#[derive(Component)]
pub struct PulseVisual {
    serial: u64,
    generation: u32,
}

type ModelData<'a> = (&'a UnitVisual, &'a Players, &'a Transform);

#[derive(bevy::ecs::system::SystemParam)]
pub struct Presentation<'w, 's> {
    animations: Res<'w, Animations>,
    models: Query<'w, 's, ModelData<'static>>,
    visuals: Query<'w, 's, (Entity, &'static PulseVisual, &'static Mesh3d)>,
}

pub(super) fn load(assets: &VirtualAssets) -> Result<Pulses, Box<dyn Error>> {
    let templates = data::load(assets)?;
    let bindings = data::bindings(assets, &templates)?;
    Ok(Pulses {
        templates,
        bindings,
        active: Vec::new(),
        generation: 0,
        serial: 0,
    })
}

impl Pulses {
    pub(super) fn maintain(&mut self, battle: &Match, particles: &mut crate::particles::Particles) {
        if self.generation != battle.generation {
            self.active.clear();
            self.generation = battle.generation;
        }
        self.active.retain(|pulse| {
            let keep = battle.started && battle.simulation.tick < pulse.expires;
            if !keep {
                particles.retire_optional_laser_emitters(pulse.emitters);
            }
            keep
        });
    }

    pub(super) fn record(
        &mut self,
        battle: &Match,
        shot: &generals_sim::skirmish::point_defense::Shot,
        animations: &mut Animations,
        particles: &mut crate::particles::Particles,
        models: &Query<(&UnitVisual, &Players, &Transform)>,
        now: std::time::Duration,
    ) {
        self.maintain(battle, particles);
        let game = &battle.simulation;
        let actor = &game.entities[shot.source];
        let Some(weapon) = game.catalog[shot.weapon_type].weapon_template(shot.weapon_index) else {
            return;
        };
        let binding = self
            .bindings
            .get(&weapon.name)
            .expect("validated weapon laser template");
        let template = &self.templates[binding.template];
        let lifetime = shot.lifetime.expect("simulation laser lifetime");
        assert!(lifetime >= template.lifetime[0].max(1) && lifetime <= template.lifetime[1].max(1));
        let source = shot.source_position;
        let start =
            source_bone(battle, animations, models, shot.source, &binding.bone).unwrap_or(source);
        let points = [start, shot.target_position];
        // initLaser admits endpoint emitters once, using the parent's shroud
        // state. clientUpdate subsequently moves only the beam's start.
        let emitters = if actor.contained_by().is_some()
            || matches!(
                game.visibility_status(0, shot.source),
                Status::Clear | Status::Partial
            ) {
            particles.optional_laser_emitters(
                &template.particles,
                points,
                animations,
                battle.generation,
                shot.frame,
            )
        } else {
            [None; 2]
        };
        self.serial = self.serial.wrapping_add(1);
        self.active.push(Pulse {
            serial: self.serial,
            source: shot.source,
            weapon: weapon.name.clone(),
            points,
            origin: [source[0], source[1]],
            expires: shot.frame + u64::from(lifetime),
            offset: 0.0,
            last_used: now,
            emitters,
        });
    }

    pub(super) fn render(
        &mut self,
        commands: &mut Commands,
        battle: &Match,
        camera: &Transform,
        resources: &mut RenderResources,
        presentation: &Presentation,
        now: std::time::Duration,
    ) {
        self.load_materials(resources);
        let mut existing = BTreeMap::new();
        for (entity, visual, mesh) in &presentation.visuals {
            if visual.generation != battle.generation
                || !battle.started
                || !self
                    .active
                    .iter()
                    .any(|pulse| pulse.serial == visual.serial && visible(battle, pulse))
            {
                commands.entity(entity).despawn();
                resources.meshes.remove(&mesh.0);
            } else {
                existing.insert(visual.serial, mesh.0.clone());
            }
        }
        if !battle.started {
            return;
        }
        for pulse in &mut self.active {
            let binding = &self.bindings[&pulse.weapon];
            let template = &self.templates[binding.template];
            if let Some(start) = source_bone(
                battle,
                &presentation.animations,
                &presentation.models,
                pulse.source,
                &binding.bone,
            ) {
                pulse.points[0] = start;
            }
            if !visible(battle, pulse) {
                continue;
            }
            let milliseconds = now.as_millis() - pulse.last_used.as_millis();
            let bytes = milliseconds.to_le_bytes();
            let upper = f32::from(u16::from_le_bytes([bytes[2], bytes[3]])) * 65_536.0;
            let elapsed = upper + f32::from(u16::from_le_bytes([bytes[0], bytes[1]]));
            pulse.offset = scroll(pulse.offset, template.scroll, elapsed);
            pulse.last_used = now;
            let mesh = match pulse_mesh(template, pulse.points, pulse.offset, camera) {
                Ok(mesh) => mesh,
                Err(error) => {
                    error!("weapon laser: {error}");
                    continue;
                }
            };
            if let Some(handle) = existing.get(&pulse.serial) {
                if let Some(mut destination) = resources.meshes.get_mut(handle) {
                    *destination = mesh;
                }
            } else {
                commands.spawn((
                    PulseVisual {
                        serial: pulse.serial,
                        generation: battle.generation,
                    },
                    Mesh3d(resources.meshes.add(mesh)),
                    MeshMaterial3d(
                        template
                            .material
                            .as_ref()
                            .expect("laser pulse material loaded")
                            .clone(),
                    ),
                    Transform::IDENTITY,
                    RenderLayers::layer(3),
                    NoFrustumCulling,
                ));
            }
        }
    }

    fn load_materials(&mut self, resources: &mut RenderResources) {
        for template in &mut self.templates {
            if let Some(mut image) = template.image.take() {
                image.texture_descriptor.format =
                    image.texture_descriptor.format.remove_srgb_suffix();
                template.material = Some(resources.scene_materials.add(SceneMaterial {
                    smudge: 0,
                    texture: Some(resources.images.add(image)),
                }));
            }
        }
    }
}

pub(super) fn source_bone(
    battle: &Match,
    animations: &Animations,
    models: &Query<(&UnitVisual, &Players, &Transform)>,
    source: usize,
    bone: &str,
) -> Option<[f32; 3]> {
    let (_, player, transform) = models
        .iter()
        .find(|(visual, _, _)| visual.index == source && visual.generation == battle.generation)?;
    let actor = &battle.simulation.entities[source];
    let offset = if bone.is_empty() {
        None
    } else {
        animations
            .named_bone_position(actor, player, bone)
            .expect("validated source laser bone")
    };
    // The child drawable already has its original container bone transform.
    let point = offset.map_or(transform.translation, |point| {
        transform.transform_point(Vec3::new(point[0], point[2], -point[1]) * 0.1)
    });
    Some([point.x * 10.0, -point.z * 10.0, point.y * 10.0])
}

fn visible(battle: &Match, pulse: &Pulse) -> bool {
    matches!(
        battle.simulation.visibility_cell(0, pulse.origin),
        Status::Clear | Status::Partial
    )
}

fn scroll(offset: f32, rate: f32, elapsed: f32) -> f32 {
    let per_millisecond = rate * 0.001;
    let movement = per_millisecond * elapsed;
    let offset = offset + movement;
    offset - offset.floor()
}

fn tile_factor(length: f32, width: f32, aspect: f32, tiling: f32) -> f32 {
    (length / width * aspect * tiling).clamp(0.0, 50.0)
}

fn pulse_mesh(
    template: &Template,
    points: [[f32; 3]; 2],
    offset: f32,
    camera: &Transform,
) -> Result<Mesh, &'static str> {
    let mut mesh = geometry(&template.beams, points, camera)?;
    let squared = std::array::from_fn::<_, 3, _>(|axis| {
        let delta = points[1][axis] - points[0][axis];
        delta * delta
    });
    let length = (squared[0] + squared[1] + squared[2]).sqrt();
    if let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for (beam, vertices) in template.beams.iter().zip(uv.chunks_exact_mut(4)) {
            let tile = tile_factor(length, beam.width, template.aspect, template.tiling);
            for uv in vertices {
                let tiled = uv[1] * tile;
                uv[1] = tiled + offset;
            }
        }
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::{Template, pulse_mesh, scroll, tile_factor};
    use bevy::{mesh::VertexAttributeValues, prelude::*};

    #[test]
    fn original_segment_renderer_clamps_negative_and_excessive_tiling_then_wraps_scroll() {
        assert_eq!(
            tile_factor(200.0, 0.005, 1.0, -3.0).to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            tile_factor(200.0, 0.005, 1.0, 3.0).to_bits(),
            50.0_f32.to_bits()
        );
        assert_eq!(
            tile_factor(40.0, 10.0, 0.5, 2.0).to_bits(),
            4.0_f32.to_bits()
        );
        assert_eq!(scroll(0.5, -2500.0, 1.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(scroll(0.25, -2500.0, 2.0).to_bits(), 0.25_f32.to_bits());
    }

    #[test]
    fn four_native_pulse_layers_keep_authored_widths_colors_and_clamped_scrolling_uvs() {
        let template = Template {
            name: "PointDefenseLaserBeam".into(),
            beams: (0..4)
                .rev()
                .map(|index| {
                    super::super::beam(
                        index,
                        4,
                        [0.005, 10.0],
                        [1.0, 0.0, 180.0 / 255.0, 120.0 / 255.0],
                        [100.0 / 255.0, 0.0, 100.0 / 255.0, 100.0 / 255.0],
                    )
                })
                .collect(),
            particles: [String::new(), String::new()],
            lifetime: [3, 3],
            tiling: -3.0,
            scroll: -2500.0,
            aspect: 1.0,
            image: None,
            material: None,
        };
        let camera = Transform::from_xyz(0.0, 100.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        let points = [[10.0, 20.0, 30.0], [100.0, 200.0, 60.0]];
        let mesh = pulse_mesh(&template, points, 0.375, &camera).unwrap();
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("pulse positions missing");
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("pulse colors missing");
        };
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("pulse UVs missing");
        };
        assert_eq!(positions.len(), 16);
        assert_eq!(mesh.indices().unwrap().len(), 24);
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(
            uv.iter()
                .all(|point| point[1].to_bits() == 0.375_f32.to_bits())
        );
        for (beam, vertices) in template.beams.iter().zip(colors.chunks_exact(4)) {
            assert!(vertices.iter().all(|color| {
                color[..3]
                    .iter()
                    .zip(beam.color)
                    .all(|(actual, expected)| actual.to_bits() == expected.to_bits())
            }));
        }
    }
}
