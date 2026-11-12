use super::{Streams, render::Visual};
use crate::{
    control_bar::UiData,
    playtest::Match,
    playtest_ui::{PromotionTransition, captures_pointer},
    playtest_view::WorldCamera,
    source_material::SourceMaterial,
};
use bevy::{mesh::VertexAttributeValues, prelude::*, window::PrimaryWindow};
use std::error::Error;

#[derive(bevy::ecs::system::SystemParam)]
pub struct ProjectileStreamInspection<'w, 's> {
    battle: Res<'w, Match>,
    streams: Res<'w, Streams>,
    ui: Res<'w, UiData>,
    promotions: Res<'w, PromotionTransition>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    visuals: Query<
        'w,
        's,
        (
            &'static Visual,
            &'static Mesh3d,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
    >,
}

impl ProjectileStreamInspection<'_, '_> {
    pub fn visible(self, source: usize, weapon_index: usize) -> Result<bool, Box<dyn Error>> {
        let Self {
            battle,
            streams,
            ui,
            promotions,
            window,
            camera,
            meshes,
            materials,
            visuals,
        } = self;
        let (camera, transform) = *camera;
        let Some(viewport) = camera.logical_viewport_rect() else {
            return Ok(false);
        };
        let Some(name) = battle.simulation.catalog[battle.simulation.entities[source].type_index]
            .weapon_template(weapon_index)
            .and_then(|weapon| weapon.projectile.stream.as_deref())
        else {
            return Err("requested weapon has no original projectile stream".into());
        };
        let template = &streams.templates[name];
        for (visual, mesh, material, visible) in &visuals {
            if visual.source != source
                || visual.weapon != weapon_index
                || visual.generation != battle.generation
                || !visible.get()
            {
                continue;
            }
            let Some(mesh) = meshes.get(&mesh.0) else {
                continue;
            };
            let Some(material_data) = materials.get(&material.0) else {
                continue;
            };
            let positions = mesh_positions(mesh)?;
            if !matches!(material_data.alpha_mode, AlphaMode::Add)
                || material_data.parameters.vertex_diffuse != 3
                || template.material.as_ref() != Some(&material.0)
                || material_data.texture.is_none()
            {
                return Err(
                    "native projectile stream lost original geometry or additive texture".into(),
                );
            }
            if positions.iter().all(|position| {
                camera
                    .world_to_viewport(transform, Vec3::from_array(*position))
                    .is_ok_and(|point| {
                        viewport.contains(point)
                            && !captures_pointer(&battle, &window, point, &ui, &promotions)
                    })
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub(super) fn mesh_positions(mesh: &Mesh) -> Result<&[[f32; 3]], &'static str> {
    let (
        Some(VertexAttributeValues::Float32x3(positions)),
        Some(VertexAttributeValues::Float32x3(normals)),
        Some(VertexAttributeValues::Float32x2(uv)),
        Some(VertexAttributeValues::Float32x4(colors)),
    ) = (
        mesh.attribute(Mesh::ATTRIBUTE_POSITION),
        mesh.attribute(Mesh::ATTRIBUTE_NORMAL),
        mesh.attribute(Mesh::ATTRIBUTE_UV_0),
        mesh.attribute(Mesh::ATTRIBUTE_COLOR),
    )
    else {
        return Err("native projectile stream lacks required material vertex attributes");
    };
    if positions.len() < 4
        || normals.len() != positions.len()
        || uv.len() != positions.len()
        || colors.len() != positions.len()
        || mesh.indices().is_none_or(|indices| indices.len() < 6)
        || positions
            .iter()
            .flatten()
            .chain(normals.iter().flatten())
            .chain(uv.iter().flatten())
            .chain(colors.iter().flatten())
            .any(|value| !value.is_finite())
    {
        return Err("native projectile stream has inconsistent material vertex attributes");
    }
    Ok(positions)
}
