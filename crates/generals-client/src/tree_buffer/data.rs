//! SH `W3DTreeDrawModuleData` and the original first mesh, independent of Object modules.
use crate::skirmish_data::{field, scalar, source_real, text};
use generals_assets::{content::ContentLine, virtual_assets::VirtualAssets, w3d::Model};
use num_traits::ToPrimitive;
use std::error::Error;

pub struct Data {
    pub model: String,
    pub texture: String,
    pub outward: u32,
    pub inward: u32,
    pub distance: f32,
    pub darkening: f32,
    pub topple: bool,
    pub shadow: bool,
    pub kill: bool,
    pub velocity: f32,
    pub acceleration: f32,
    pub bounce: f32,
    pub minimum_speed: f32,
    pub sink_frames: f32,
    pub sink_distance: f32,
    pub topple_fx: String,
    pub bounce_fx: String,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub uv: Vec<[f32; 2]>,
    pub colors: Vec<[u8; 4]>,
    pub emissive: [f32; 3],
    pub offset: [f32; 3],
    pub center: [f32; 3],
    pub radius: f32,
    pub shadow_size: f32,
    pub pixels: super::atlas::Texture,
}

impl Data {
    pub fn load(
        assets: &VirtualAssets,
        draw: &[ContentLine],
        model: &Model,
    ) -> Result<Self, Box<dyn Error>> {
        let mesh = model
            .meshes
            .iter()
            .min_by_key(|mesh| mesh.subobject_order)
            .ok_or("W3DTreeDraw first subobject is not a mesh")?;
        let materials = mesh
            .materials
            .as_ref()
            .ok_or("W3DTreeDraw mesh lacks materials")?;
        let pass = materials
            .passes
            .first()
            .ok_or("W3DTreeDraw mesh lacks first pass")?;
        let stage = pass.stages.first().ok_or("W3DTreeDraw mesh lacks UV0")?;
        if stage.coordinates.len() != mesh.positions.len() || stage.face_coordinate_ids.is_some() {
            return Err("W3DTreeBuffer needs native per-vertex UV0".into());
        }
        // The asset reader has applied the rigid HLOD base pose. The native buffer
        // reads mesh vertices before that pose and adds only subobject translation.
        let offset = mesh.bound_transform[3];
        let axes = &mesh.bound_transform[..3];
        let inverse = |value: [f32; 3]| {
            std::array::from_fn(|axis| {
                let x = value[0] * axes[axis][0];
                let y = value[1] * axes[axis][1];
                let z = value[2] * axes[axis][2];
                (x + y) + z
            })
        };
        let vertices: Vec<_> = mesh
            .positions
            .iter()
            .map(|point| inverse(std::array::from_fn(|axis| point[axis] - offset[axis])))
            .collect();
        let normals = mesh.normals.as_ref().map_or_else(
            || vec![[0.0, 0.0, 1.0]; vertices.len()],
            |normals| normals.iter().copied().map(inverse).collect(),
        );
        let (mut center, radius) = super::sphere::from_points(&vertices);
        for (center, offset) in center.iter_mut().zip(offset) {
            *center += offset;
        }
        let texture = text(draw, 1, "TextureName")?;
        Ok(Self {
            model: text(draw, 1, "ModelName")?.to_owned(),
            texture: texture.to_owned(),
            outward: frames(draw, "MoveOutwardTime", 1)?,
            inward: frames(draw, "MoveInwardTime", 1)?,
            distance: real(draw, "MoveOutwardDistanceFactor", 1.0)?,
            darkening: real(draw, "DarkeningFactor", 0.0)?,
            topple: boolean(draw, "DoTopple", false)?,
            shadow: boolean(draw, "DoShadow", false)?,
            kill: boolean(draw, "KillWhenFinishedToppling", true)?,
            velocity: percent(draw, "InitialVelocityPercent", 0.2)?,
            acceleration: percent(draw, "InitialAccelPercent", 0.01)?,
            bounce: percent(draw, "BounceVelocityPercent", 0.3)?,
            minimum_speed: positive(draw, "MinimumToppleSpeed", 0.5)?,
            sink_frames: frames(draw, "SinkTime", 300)?
                .to_f32()
                .expect("source duration"),
            sink_distance: positive(draw, "SinkDistance", 20.0)?,
            topple_fx: effect(draw, "ToppleFX")?,
            bounce_fx: effect(draw, "BounceFX")?,
            vertices,
            normals,
            indices: mesh.indices.clone(),
            uv: stage.coordinates.clone(),
            colors: pass
                .colors
                .clone()
                .unwrap_or_else(|| vec![[255; 4]; mesh.positions.len()]),
            emissive: materials
                .vertex_materials
                .first()
                .map_or([0.0; 3], |material| material.emissive),
            offset,
            center,
            radius,
            shadow_size: (model.scene_bounds.max[0] - model.scene_bounds.min[0]
                + model.scene_bounds.max[1]
                - model.scene_bounds.min[1])
                * 0.5,
            pixels: super::atlas::Texture::load(assets, texture)?,
        })
    }
}

fn real(draw: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(draw, 1, key).map_or(Ok(default), |line| source_real(scalar(line)?))
}
fn percent(draw: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    field(draw, 1, key).map_or(Ok(default), |line| {
        Ok(source_real(scalar(line)?.trim_end_matches('%'))? / 100.0)
    })
}
fn boolean(draw: &[ContentLine], key: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    field(draw, 1, key).map_or(Ok(default), |line| {
        match scalar(line)?.to_ascii_lowercase().as_str() {
            "yes" => Ok(true),
            "no" => Ok(false),
            _ => Err(format!("invalid tree {key}").into()),
        }
    })
}
fn frames(draw: &[ContentLine], key: &str, default: u32) -> Result<u32, Box<dyn Error>> {
    field(draw, 1, key).map_or(Ok(default), |line| {
        let milliseconds: u32 = scalar(line)?.parse()?;
        (milliseconds.to_f32().expect("native unsigned duration") * (30.0 / 1000.0))
            .ceil()
            .to_u32()
            .ok_or_else(|| "tree duration overflow".into())
    })
}
fn effect(draw: &[ContentLine], key: &str) -> Result<String, Box<dyn Error>> {
    field(draw, 1, key).map_or(Ok(String::new()), |line| scalar(line).map(str::to_owned))
}
fn positive(draw: &[ContentLine], key: &str, default: f32) -> Result<f32, Box<dyn Error>> {
    let value = real(draw, key, default)?;
    if value > 0.0 {
        Ok(value)
    } else {
        Err(format!("tree {key} must be positive").into())
    }
}
