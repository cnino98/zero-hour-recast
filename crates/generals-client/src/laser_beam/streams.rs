//! Original Patriot feedback and persistent Hacker binary streams.
use super::{Beam, RenderResources, geometry};
use crate::{
    playtest::Match,
    skirmish_data::{blocks, definition, field, scalar},
    source_material::SourceMaterial,
};
use bevy::{
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::VertexAttributeValues,
    prelude::*,
};
use generals_assets::virtual_assets::VirtualAssets;
use num_traits::ToPrimitive;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

mod cache;
#[cfg(test)]
mod equivalence;

pub(super) struct Streams {
    template: String,
    beams: Vec<Beam>,
    tile: bool,
    segments: u32,
    arc: f32,
    overlap: f32,
    tile_scale: f32,
    aspect: f32,
    scroll: f32,
    image: Option<Image>,
    material: Option<Handle<SourceMaterial>>,
    active: Vec<Stream>,
    pub(super) generation: u32,
    serial: u64,
}
struct Stream {
    object: usize,
    serial: u64,
    points: [[f32; 3]; 2],
    width_scale: f32,
    origin: [f32; 3],
    offset: f32,
    visible: bool,
}
#[derive(Component)]
pub struct StreamVisual {
    template: String,
    serial: u64,
    pub(super) generation: u32,
    geometry: cache::Geometry,
}

pub(super) fn load_feedback(
    assets: &VirtualAssets,
    catalog: &[generals_sim::skirmish::UnitType],
) -> Result<Vec<Streams>, Box<dyn Error>> {
    crate::skirmish_data::feedback_templates(catalog)
        .into_iter()
        .map(|name| load_template(assets, name))
        .collect()
}

fn load_template(assets: &VirtualAssets, name: &str) -> Result<Streams, Box<dyn Error>> {
    let (files, _) = crate::skirmish_data::source_files::effective(assets, &[name])?;
    let object = definition(&files, "Object", name)?;
    let draw = blocks(&object.lines, 0, "Draw")?
        .into_iter()
        .find(|block| {
            block[0]
                .tokens
                .get(1)
                .is_some_and(|name| name == "W3DLaserDraw")
        })
        .ok_or("assist stream draw missing")?;
    let tile = match field(draw, 1, "Tile").map(scalar).transpose()? {
        None => false,
        Some(value) if value.eq_ignore_ascii_case("Yes") => true,
        Some(value) if value.eq_ignore_ascii_case("No") => false,
        Some(_) => return Err("invalid native laser Tile boolean".into()),
    };
    // Native parses these fields but W3DLaserDraw never reads either value.
    for key in ["MaxIntensityLifetime", "FadeLifetime"] {
        if let Some(line) = field(draw, 1, key) {
            crate::skirmish_data::source_numbers::duration_frames(scalar(line)?)?;
        }
    }
    let texture = scalar(field(draw, 1, "Texture").ok_or("assist stream texture missing")?)?;
    let image = crate::source_texture::load(assets, texture)?;
    let aspect =
        f32::from(u16::try_from(image.width())?) / f32::from(u16::try_from(image.height())?);
    let beams = beam_layers(draw)?;
    let segments = field(draw, 1, "Segments")
        .map(scalar)
        .transpose()?
        .map_or(Ok(1), crate::skirmish_data::source_numbers::unsigned)?;
    let scroll = optional_number(draw, "ScrollRate", 0.0)?;
    if !scroll.is_finite() {
        return Err("invalid native laser geometry".into());
    }
    Ok(Streams {
        template: name.into(),
        beams,
        tile,
        segments,
        arc: optional_number(draw, "ArcHeight", 0.0)?,
        overlap: optional_number(draw, "SegmentOverlapRatio", 0.0)?,
        tile_scale: optional_number(draw, "TilingScalar", 1.0)?,
        aspect,
        scroll,
        image: Some(image),
        material: None,
        active: Vec::new(),
        generation: 0,
        serial: 0,
    })
}

impl Streams {
    pub(super) fn record_native(&mut self, battle: &Match, native: &super::native::Lasers) {
        if self.generation != battle.generation {
            self.active.clear();
            self.generation = battle.generation;
        }
        let game = &battle.simulation;
        self.active
            .retain(|stream| native.active.contains_key(&stream.object));
        for (&object, state) in &native.active {
            let actor = &game.entities[object];
            if game.catalog[actor.type_index].name != self.template {
                continue;
            }
            let visible = state.drawable_present
                && game.visible_to(battle.observer_player.unwrap_or(0), object);
            if let Some(stream) = self
                .active
                .iter_mut()
                .find(|stream| stream.object == object)
            {
                stream.points = state.points;
                stream.width_scale = state.radius.width;
                stream.origin = state.position;
                stream.visible = visible;
            } else {
                self.serial = self.serial.wrapping_add(1);
                self.active.push(Stream {
                    object,
                    serial: self.serial,
                    points: state.points,
                    width_scale: state.radius.width,
                    origin: state.position,
                    offset: 0.0,
                    visible,
                });
            }
        }
    }

    pub fn render(
        &mut self,
        commands: &mut Commands,
        battle: &Match,
        camera: &Transform,
        resources: &mut RenderResources,
        visuals: &mut Query<(Entity, &mut StreamVisual, &Mesh3d)>,
        delta: f32,
    ) {
        if let Some(mut image) = self.image.take() {
            image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
            let mut material =
                SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Add, true);
            material.parameters.vertex_diffuse = 3;
            material.texture = Some(resources.images.add(image));
            self.material = Some(resources.materials.add(material));
        }
        let visible: BTreeSet<_> = self
            .active
            .iter()
            .filter(|stream| stream.visible && self.segments > 0 && !self.beams.is_empty())
            .map(|stream| stream.serial)
            .collect();
        let mut existing = BTreeMap::new();
        for (entity, visual, mesh) in visuals.iter_mut() {
            if visual.template != self.template {
                continue;
            }
            if !battle.started
                || visual.generation != self.generation
                || !visible.contains(&visual.serial)
            {
                commands.entity(entity).despawn();
                resources.meshes.remove(&mesh.0);
            } else {
                existing.insert(visual.serial, (entity, mesh.0.clone(), visual));
            }
        }
        for index in 0..self.active.len() {
            if !self.active[index].visible || self.segments == 0 || self.beams.is_empty() {
                continue;
            }
            let scroll_distance = self.scroll * delta;
            let offset = self.active[index].offset + scroll_distance;
            self.active[index].offset = offset - offset.floor();
            let stream = &self.active[index];
            let position = Vec3::new(stream.origin[0], stream.origin[2], -stream.origin[1]) * 0.1;
            if let Some((entity, handle, visual)) = existing.get_mut(&stream.serial) {
                commands
                    .entity(*entity)
                    .insert(Transform::from_translation(position));
                if let Err(error) = visual
                    .geometry
                    .refresh(self, stream, camera, resources, handle)
                {
                    bevy::log::error_once!("Original assistance stream update failed: {error}");
                }
            } else {
                let Ok((mesh, geometry)) =
                    cache::Geometry::build(self, stream, &resources.terrain, camera)
                else {
                    continue;
                };
                commands.spawn((
                    StreamVisual {
                        template: self.template.clone(),
                        serial: stream.serial,
                        generation: self.generation,
                        geometry,
                    },
                    Mesh3d(resources.meshes.add(mesh)),
                    MeshMaterial3d(
                        self.material
                            .as_ref()
                            .expect("stream material loaded")
                            .clone(),
                    ),
                    Transform::from_translation(position),
                    RenderLayers::layer(3),
                    NoFrustumCulling,
                ));
            }
        }
    }

    pub(super) fn endpoints(&self, object: usize) -> Option<[[f32; 3]; 2]> {
        self.active
            .iter()
            .find(|stream| stream.object == object)
            .map(|stream| stream.points)
    }

    pub(super) fn inspect(
        &self,
        object: usize,
        visual: &StreamVisual,
        mesh: &Mesh,
        material: &SourceMaterial,
    ) -> Result<bool, &'static str> {
        if visual.template != self.template || visual.generation != self.generation {
            return Ok(false);
        }
        let Some(stream) = self
            .active
            .iter()
            .find(|stream| stream.serial == visual.serial && stream.object == object)
        else {
            return Ok(false);
        };
        self.inspect_geometry(stream, mesh, material)
    }

    fn inspect_geometry(
        &self,
        stream: &Stream,
        mesh: &Mesh,
        material: &SourceMaterial,
    ) -> Result<bool, &'static str> {
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return Err("Hacker stream has no rendered positions");
        };
        if points.len()
            != usize::try_from(self.segments).expect("native segment count") * self.beams.len() * 4
            || mesh.indices().is_none_or(|indices| {
                indices.len()
                    != usize::try_from(self.segments).expect("native segment count")
                        * self.beams.len()
                        * 6
            })
            || points.iter().flatten().any(|value| !value.is_finite())
            || material.texture.is_none()
            || !matches!(material.alpha_mode, AlphaMode::Add)
            || Vec3::from_array(stream.points[0]).distance(Vec3::from_array(stream.points[1]))
                <= 1.0
        {
            return Err("Hacker stream lost original additive arc geometry");
        }
        Ok(true)
    }

    fn mesh(
        &self,
        endpoints: [[f32; 3]; 2],
        width_scale: f32,
        origin: [f32; 3],
        terrain: &crate::terrain::Terrain,
        camera: &Transform,
    ) -> Result<Mesh, &'static str> {
        let mut combined = None;
        for index in 0..self.segments {
            let points = if self.arc > 0.0 && self.segments > 1 {
                arc_segment(
                    endpoints,
                    index,
                    self.segments,
                    self.arc,
                    self.overlap,
                    |point| terrain.sample(Vec2::new(point[0], point[1]) / 10.0) * 10.0,
                )?
            } else {
                endpoints
            };
            let length = vector_length(std::array::from_fn(|axis| {
                points[1][axis] - points[0][axis]
            }));
            for beam in &self.beams {
                let beam = Beam {
                    width: beam.width * width_scale,
                    color: beam.color,
                };
                let tile = if self.tile {
                    length / beam.width * self.aspect * self.tile_scale
                } else {
                    1.0
                };
                let mut segment = geometry(std::slice::from_ref(&beam), points, camera)?;
                let origin = Vec3::new(origin[0], origin[2], -origin[1]) * 0.1;
                if let Some(VertexAttributeValues::Float32x3(positions)) =
                    segment.attribute_mut(Mesh::ATTRIBUTE_POSITION)
                {
                    for point in positions {
                        *point = (Vec3::from_array(*point) - origin).to_array();
                    }
                }
                if let Some(VertexAttributeValues::Float32x2(uv)) =
                    segment.attribute_mut(Mesh::ATTRIBUTE_UV_0)
                {
                    for point in uv {
                        point[1] *= tile;
                    }
                }
                if let Some(mesh) = &mut combined {
                    Mesh::merge(mesh, &segment).map_err(|_| "native laser segment merge failed")?;
                } else {
                    combined = Some(segment);
                }
            }
        }
        combined.ok_or("assist stream has no segments")
    }
}

fn beam_layers(
    draw: &[generals_assets::content::ContentLine],
) -> Result<Vec<Beam>, Box<dyn Error>> {
    let count = field(draw, 1, "NumBeams")
        .map(scalar)
        .transpose()?
        .map_or(Ok(1), crate::skirmish_data::source_numbers::unsigned)?;
    let inner = super::color(field(draw, 1, "InnerColor").ok_or("stream inner color missing")?)?;
    let outer = field(draw, 1, "OuterColor")
        .map(super::color)
        .transpose()?
        .unwrap_or([0.0; 4]);
    let width = optional_number(draw, "InnerBeamWidth", 0.0)?;
    let outer_width = optional_number(draw, "OuterBeamWidth", 1.0)?;
    let mut beams = Vec::new();
    for index in (0..count).rev() {
        let (width, color) = if count == 1 {
            (width, std::array::from_fn(|axis| inner[axis] * inner[3]))
        } else {
            let fraction = index.to_f32().expect("native beam index")
                / (count - 1).to_f32().expect("native beam count");
            let width_delta = fraction * (outer_width - width);
            let width = width + width_delta;
            let alpha_delta = fraction * (outer[3] - inner[3]);
            let alpha = inner[3] + alpha_delta;
            let color = std::array::from_fn(|axis| {
                let delta = fraction * (outer[axis] - inner[axis]);
                let component = inner[axis] + delta;
                component * alpha
            });
            (width, color)
        };
        beams.push(Beam { width, color });
    }
    Ok(beams)
}

fn optional_number(
    lines: &[generals_assets::content::ContentLine],
    name: &str,
    default: f32,
) -> Result<f32, Box<dyn Error>> {
    field(lines, 1, name)
        .map(scalar)
        .transpose()?
        .map_or(Ok(default), crate::skirmish_data::source_real)
}

fn arc_segment(
    points: [[f32; 3]; 2],
    index: u32,
    count: u32,
    height: f32,
    overlap: f32,
    terrain: impl Fn([f32; 3]) -> f32,
) -> Result<[[f32; 3]; 2], &'static str> {
    let [start, end] = points;
    let vector: [f32; 3] = std::array::from_fn(|axis| end[axis] - start[axis]);
    let half = vector_length(vector) * 0.5;
    if half == 0.0 {
        return Err("coincident native assist endpoints have no drawable arc");
    }
    let middle: [f32; 3] = std::array::from_fn(|axis| (start[axis] + end[axis]) * 0.5);
    let ratios = [
        index.to_f32().expect("native segment index")
            / count.to_f32().expect("native segment count")
            - if index > 0 { overlap } else { 0.0 },
        (index.to_f32().expect("native segment index") + 1.0)
            / count.to_f32().expect("native segment count")
            + if index + 1 < count { overlap } else { 0.0 },
    ];
    Ok(ratios.map(|ratio| {
        let mut point: [f32; 3] = std::array::from_fn(|axis| {
            let distance = vector[axis] * ratio;
            start[axis] + distance
        });
        let to_middle = std::array::from_fn(|axis| middle[axis] - point[axis]);
        let radians = vector_length(to_middle) / half * std::f32::consts::PI * 0.5;
        let arc_height = radians.cos() * height;
        point[2] += arc_height;
        point[2] = point[2].max(2.0 + terrain(point));
        point
    }))
}

fn vector_length(vector: [f32; 3]) -> f32 {
    let squared = vector.map(|value| value * value);
    (squared[0] + squared[1] + squared[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::arc_segment;

    #[test]
    fn arc_has_original_midpoint_height_and_terrain_clearance_at_both_ends() {
        let points = [[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]];
        let beginning = arc_segment(points, 0, 20, 30.0, 0.0, |_| 0.0).unwrap();
        let middle = arc_segment(points, 9, 20, 30.0, 0.0, |_| 0.0).unwrap();
        assert_eq!(
            beginning[0].map(f32::to_bits),
            [0.0, 0.0, 2.0_f32].map(f32::to_bits)
        );
        assert_eq!(
            middle[1].map(f32::to_bits),
            [100.0, 0.0, 30.0_f32].map(f32::to_bits)
        );
        let raised = arc_segment(points, 9, 20, 30.0, 0.0, |_| 35.0).unwrap();
        assert!(
            raised
                .iter()
                .all(|point| point[2].to_bits() == 37.0_f32.to_bits())
        );
        for index in 0..19 {
            let left = arc_segment(points, index, 20, 30.0, 0.0, |_| 0.0).unwrap();
            let right = arc_segment(points, index + 1, 20, 30.0, 0.0, |_| 0.0).unwrap();
            assert_eq!(left[1].map(f32::to_bits), right[0].map(f32::to_bits));
        }
    }

    #[test]
    fn overlap_only_extends_interior_endpoints_and_degenerate_feedback_is_not_uploaded() {
        let points = [[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]];
        let first = arc_segment(points, 0, 20, 30.0, 0.01, |_| 0.0).unwrap();
        let last = arc_segment(points, 19, 20, 30.0, 0.01, |_| 0.0).unwrap();
        assert_eq!(first[0][0].to_bits(), 0.0_f32.to_bits());
        assert!(first[1][0] > 10.0);
        assert!(last[0][0] < 190.0);
        assert_eq!(last[1][0].to_bits(), 200.0_f32.to_bits());
        assert!(arc_segment([points[0]; 2], 0, 20, 30.0, 0.0, |_| 0.0).is_err());
    }
}
