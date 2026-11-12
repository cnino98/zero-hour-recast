//! Read actual model materials and CPU terrain vertices; never alter rendering.
use super::Impact;
use crate::{
    light_environment::{Bounds, DrawLights, DrawPose},
    particles::Particles,
    playtest::Match,
    playtest_view::{UnitVisual, WorldCamera},
    skirmish_verification::require,
    source_material::{Parameters, SourceMaterial},
};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

pub(super) struct Baseline {
    generation: u32,
    models: Vec<(Entity, Parameters)>,
}

struct Vertex {
    mesh: Handle<Mesh>,
    index: usize,
    color: [u32; 4],
}

pub(super) struct Evidence {
    camera: [u32; 16],
    light: usize,
    position: [u32; 3],
    vertices: Vec<Vertex>,
}

type ModelPart = (
    Entity,
    &'static ChildOf,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static ViewVisibility,
    Option<&'static Bounds>,
);

type TerrainPart = (
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static GlobalTransform,
);

#[derive(SystemParam)]
pub(super) struct Inspector<'w, 's> {
    particles: Res<'w, Particles>,
    lighting: DrawLights<'w>,
    materials: Res<'w, Assets<SourceMaterial>>,
    meshes: Res<'w, Assets<Mesh>>,
    camera: Single<'w, 's, &'static GlobalTransform, With<WorldCamera>>,
    roots: Query<'w, 's, (&'static UnitVisual, Option<&'static DrawPose>)>,
    parts: Query<'w, 's, ModelPart>,
    terrain: Query<'w, 's, TerrainPart, With<crate::terrain::changes::Surface>>,
}

impl Inspector<'_, '_> {
    pub(super) fn baseline(
        &self,
        battle: &Match,
        target: usize,
    ) -> Result<Baseline, Box<dyn Error>> {
        require(
            self.particles
                .pulses
                .lights()
                .iter()
                .all(|light| !light.enabled),
            "baseline has an unrelated active LightPulse",
        )?;
        let mut models = Vec::new();
        for (entity, parent, material, visible, _) in &self.parts {
            let Ok((actor, _)) = self.roots.get(parent.parent()) else {
                continue;
            };
            if actor.index == target && actor.generation == battle.generation && visible.get() {
                let material = self
                    .materials
                    .get(&material.0)
                    .ok_or("original model material missing")?;
                models.push((entity, material.parameters.clone()));
            }
        }
        require(!models.is_empty(), "no visible original target materials")?;
        Ok(Baseline {
            generation: battle.generation,
            models,
        })
    }

    pub(super) fn active(
        &self,
        battle: &Match,
        target: usize,
        baseline: &Baseline,
        impact: &Impact,
    ) -> Result<Evidence, Box<dyn Error>> {
        Self::same_generation(battle, baseline)?;
        let (index, light) = self
            .particles
            .pulses
            .lights()
            .iter()
            .enumerate()
            .find(|(_, light)| {
                light.enabled
                    && light.position.map(f32::to_bits) == impact.position.map(f32::to_bits)
            })
            .ok_or("actual DamageTankStruck pulse expired before active screenshot request")?;
        // Original FXList radius30, color255/255/128, zero increase/500ms decay.
        let factor = light.color[0];
        require(
            factor > 0.0 && factor < 1.0,
            "source pulse has not advanced its display fade",
        )?;
        require(
            light.color.map(f32::to_bits)
                == [factor, factor, (128.0 / 255.0) * factor].map(f32::to_bits)
                && light.outer_radius.to_bits() == (31.0 * factor).max(1.0).to_bits(),
            "active pulse differs from original authored color/range",
        )?;
        let mut contributions = 0;
        for (entity, original) in &baseline.models {
            let (_, parent, handle, visible, bounds) = self.parts.get(*entity)?;
            let (actor, pose) = self.roots.get(parent.parent())?;
            require(
                actor.index == target && visible.get(),
                "original target material lost its visible owner",
            )?;
            let actual = &self
                .materials
                .get(&handle.0)
                .ok_or("active material missing")?
                .parameters;
            let mut expected = original.clone();
            if self.lighting.affects(actor, pose, bounds) {
                require(
                    self.lighting.apply(actor, pose, bounds, &mut expected),
                    "actual model light environment missing",
                )?;
                require(
                    same_lighting(actual, &expected),
                    "rendered material lacks selected source light environment",
                )?;
                contributions += usize::from(!same_lighting(original, actual));
            }
        }
        require(
            contributions > 0,
            "LightPulse did not change any actual model material",
        )?;
        let vertices = self.terrain_vertices(impact.position)?;
        require(
            !vertices.is_empty(),
            "no actual CPU-lit terrain vertices near damage callback",
        )?;
        println!(
            "LightPulse source sample: slot={index} color={:?} outer={} affected_model_parts={contributions} terrain_vertices={}",
            light.color,
            light.outer_radius,
            vertices.len()
        );
        Ok(Evidence {
            camera: self.camera.to_matrix().to_cols_array().map(f32::to_bits),
            light: index,
            position: impact.position.map(f32::to_bits),
            vertices,
        })
    }

    pub(super) fn retired(
        &self,
        battle: &Match,
        target: usize,
        baseline: &Baseline,
        evidence: &Evidence,
    ) -> Result<bool, Box<dyn Error>> {
        Self::same_generation(battle, baseline)?;
        require(
            self.camera.to_matrix().to_cols_array().map(f32::to_bits) == evidence.camera,
            "actual camera changed between active and retired pulse images",
        )?;
        let light = self
            .particles
            .pulses
            .lights()
            .get(evidence.light)
            .ok_or("observed pulse pool entry disappeared")?;
        require(
            light.position.map(f32::to_bits) == evidence.position,
            "observed light slot was reused",
        )?;
        if light.enabled {
            return Ok(false);
        }
        for (entity, original) in &baseline.models {
            let (_, parent, handle, visible, _) = self.parts.get(*entity)?;
            let (actor, _) = self.roots.get(parent.parent())?;
            require(
                actor.index == target && visible.get(),
                "retired pulse target body missing",
            )?;
            let actual = &self
                .materials
                .get(&handle.0)
                .ok_or("restored material missing")?
                .parameters;
            require(
                same_lighting(original, actual),
                "original model lighting did not restore after pulse",
            )?;
        }
        let mut changed = 0;
        for vertex in &evidence.vertices {
            let mesh = self
                .meshes
                .get(&vertex.mesh)
                .ok_or("observed terrain mesh disappeared")?;
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                return Err("observed terrain vertex colors disappeared".into());
            };
            changed += usize::from(colors[vertex.index].map(f32::to_bits) != vertex.color);
        }
        require(
            changed > 0,
            "active and retired CPU terrain colors are identical",
        )?;
        println!(
            "LightPulse CPU terrain retirement: changed_vertices={changed} observed_vertices={}",
            evidence.vertices.len()
        );
        Ok(true)
    }

    fn same_generation(battle: &Match, baseline: &Baseline) -> Result<(), Box<dyn Error>> {
        require(
            battle.generation == baseline.generation,
            "LightPulse generation changed between observations",
        )
    }

    fn terrain_vertices(&self, position: [f32; 3]) -> Result<Vec<Vertex>, Box<dyn Error>> {
        let mut result = Vec::new();
        for (handle, material, world) in &self.terrain {
            let material = self
                .materials
                .get(&material.0)
                .ok_or("terrain material missing")?;
            let mesh = self.meshes.get(&handle.0).ok_or("terrain mesh missing")?;
            let (
                Some(VertexAttributeValues::Float32x3(points)),
                Some(VertexAttributeValues::Float32x4(colors)),
            ) = (
                mesh.attribute(Mesh::ATTRIBUTE_POSITION),
                mesh.attribute(Mesh::ATTRIBUTE_COLOR),
            )
            else {
                return Err("terrain position/color attributes missing".into());
            };
            for (index, point) in points.iter().enumerate() {
                let world = world.transform_point(Vec3::from_array(*point));
                let source = Vec3::new(world.x, -world.z, world.y) * 10.0;
                if source.distance_squared(Vec3::from_array(position)) < 31.0 * 31.0 {
                    require(
                        material.parameters.terrain_lights == 1,
                        "terrain does not draw actual CPU prelit colors",
                    )?;
                    result.push(Vertex {
                        mesh: handle.0.clone(),
                        index,
                        color: colors[index].map(f32::to_bits),
                    });
                }
            }
        }
        Ok(result)
    }
}

fn same_lighting(a: &Parameters, b: &Parameters) -> bool {
    a.scene_ambient == b.scene_ambient
        && a.light_color == b.light_color
        && a.light_direction == b.light_direction
        && a.point_position == b.point_position
        && a.point_ambient == b.point_ambient
}
