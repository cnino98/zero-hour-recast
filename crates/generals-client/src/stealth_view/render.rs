use super::{Cloaks, Look};
use crate::{
    model_animation::AnimatedPart, playtest_view::UnitVisual, source_material::SourceMaterial,
};
use bevy::{
    camera::visibility::RenderLayers, ecs::system::SystemParam, prelude::*,
    render::render_resource::CompareFunction,
};
use std::collections::BTreeSet;

#[derive(Component)]
pub struct HeatPart {
    pub(super) source: Entity,
}

type SourcePart = (
    Entity,
    &'static ChildOf,
    &'static Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static Transform,
    &'static Visibility,
    &'static AnimatedPart,
);
type Overlay = (
    Entity,
    &'static HeatPart,
    &'static mut Mesh3d,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static mut Transform,
    &'static mut Visibility,
);

#[derive(SystemParam)]
pub struct Parts<'w, 's> {
    parents: Query<'w, 's, &'static UnitVisual>,
    source: Query<'w, 's, SourcePart, Without<HeatPart>>,
    overlays: Query<'w, 's, Overlay, With<HeatPart>>,
}

pub fn render(
    mut commands: Commands,
    cloaks: Res<Cloaks>,
    mut materials: ResMut<Assets<SourceMaterial>>,
    parts: Parts,
) {
    let cloaks = cloaks.into_inner();
    let Parts {
        parents,
        source,
        mut overlays,
    } = parts;
    let mut existing = BTreeSet::new();
    for (entity, overlay, mut mesh, material, mut transform, mut visibility) in &mut overlays {
        let Ok((_, parent, source_mesh, source_material, source_transform, source_visibility, _)) =
            source.get(overlay.source)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        let state = parents
            .get(parent.parent())
            .ok()
            .and_then(|unit| cloaks.states.get(&unit.index));
        let Some(state) = state.filter(|state| state.heat > 0.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(overlay.source);
        if mesh.0 != source_mesh.0 {
            mesh.0.clone_from(&source_mesh.0);
        }
        if *transform != *source_transform {
            *transform = *source_transform;
        }
        if *visibility != *source_visibility {
            *visibility = *source_visibility;
        }
        if let Some(source) = materials.get(&source_material.0).cloned() {
            let updated = heat_material(&source, state.heat, matches!(state.look, Look::Detected));
            if materials
                .get(&material.0)
                .is_some_and(|current| *current != updated)
                && let Some(mut material) = materials.get_mut(&material.0)
            {
                *material = updated;
            }
        }
    }
    for (entity, parent, mesh, material, transform, visibility, part) in &source {
        // A material overlay draws each original triangle once. Later authored
        // material passes may cover those same triangles and must not duplicate it.
        if part.binding.pass != 0 || existing.contains(&entity) {
            continue;
        }
        let Ok(unit) = parents.get(parent.parent()) else {
            continue;
        };
        let Some(state) = cloaks
            .states
            .get(&unit.index)
            .filter(|state| state.heat > 0.0)
        else {
            continue;
        };
        let Some(source) = materials.get(&material.0) else {
            continue;
        };
        let material = heat_material(source, state.heat, matches!(state.look, Look::Detected));
        let material = materials.add(material);
        commands.spawn((
            HeatPart { source: entity },
            Mesh3d(mesh.0.clone()),
            MeshMaterial3d(material),
            *transform,
            *visibility,
            ChildOf(parent.parent()),
            RenderLayers::layer(2),
        ));
    }
}

fn heat_material(source: &SourceMaterial, opacity: f32, detected_only: bool) -> SourceMaterial {
    let mut result = SourceMaterial::new(
        [0.0; 3],
        [0.02, 0.01, 0.0],
        [0.5, 0.2, 0.0],
        1.0,
        AlphaMode::Add,
        source.double_sided,
    );
    result.parameters.scene_ambient = source.parameters.scene_ambient;
    result.parameters.light_color = source.parameters.light_color;
    result.parameters.light_direction = source.parameters.light_direction;
    result.parameters.point_position = source.parameters.point_position;
    result.parameters.point_ambient = source.parameters.point_ambient;
    // materialPassEmissiveOverride scales emissive RGB, not the additive pass alpha.
    result.parameters.emissive *= opacity;
    result.parameters.shroud_extent = source.parameters.shroud_extent;
    result.shroud.clone_from(&source.shroud);
    result.depth_compare = (!detected_only).then_some(CompareFunction::Equal);
    result.depth_write = false;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use std::sync::Arc;

    fn scene() -> (App, Entity) {
        let mut app = App::new();
        let mut cloaks = Cloaks::new();
        let mut state = crate::stealth_view::State::new(0.0);
        state.set_look(Look::FriendlyDetected, 0.3, false);
        cloaks.states.insert(0, state);
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<SourceMaterial>()
            .insert_resource(cloaks)
            .add_systems(Update, render);
        let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(
            Mesh::new(
                bevy::render::render_resource::PrimitiveTopology::TriangleList,
                bevy::asset::RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3]),
        );
        let material = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(SourceMaterial::new(
                [1.0; 3],
                [1.0; 3],
                [0.0; 3],
                1.0,
                AlphaMode::Opaque,
                false,
            ));
        let parent = app
            .world_mut()
            .spawn(UnitVisual {
                index: 0,
                generation: 0,
            })
            .id();
        let source = app
            .world_mut()
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(parent),
                AnimatedPart {
                    model: 0,
                    binding: Arc::new(crate::model_render::PartBinding {
                        linear_mapper: None,
                        mesh: 0,
                        pass: 0,
                        vertices: vec![0, 1, 2],
                    }),
                },
            ))
            .id();
        (app, source)
    }

    #[test]
    fn unchanged_cloak_frames_do_not_modify_assets_but_fading_and_motion_still_update() {
        let (mut app, source) = scene();
        app.update();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<SourceMaterial>>>()
            .clear();
        app.update();
        assert!(
            !app.world_mut()
                .resource_mut::<Messages<AssetEvent<SourceMaterial>>>()
                .drain()
                .any(|event| matches!(event, AssetEvent::Modified { .. }))
        );
        app.world_mut()
            .get_mut::<Transform>(source)
            .unwrap()
            .translation
            .x = 10.0;
        app.world_mut()
            .resource_mut::<Cloaks>()
            .states
            .get_mut(&0)
            .unwrap()
            .heat = 0.5;
        app.update();
        let mut overlays = app
            .world_mut()
            .query_filtered::<(&Transform, &MeshMaterial3d<SourceMaterial>), With<HeatPart>>();
        let (transform, handle) = overlays.single(app.world()).unwrap();
        assert_eq!(transform.translation.x.to_bits(), 10.0_f32.to_bits());
        let material = app
            .world()
            .resource::<Assets<SourceMaterial>>()
            .get(&handle.0)
            .unwrap();
        assert_eq!(material.parameters.emissive.x.to_bits(), 0.25_f32.to_bits());
        assert!(
            app.world_mut()
                .resource_mut::<Messages<AssetEvent<SourceMaterial>>>()
                .drain()
                .any(|event| matches!(event, AssetEvent::Modified { .. }))
        );
    }
    #[test]
    fn native_heat_pass_uses_original_lighting_emissive_and_two_depth_policies() {
        let mut source =
            SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
        source.parameters.light_color[0] = Vec4::ONE;
        for detected_only in [false, true] {
            let material = heat_material(&source, 0.5, detected_only);
            assert!(matches!(material.alpha_mode, AlphaMode::Add));
            assert_eq!(
                material.parameters.emissive.to_array().map(f32::to_bits),
                [0.25_f32, 0.1, 0.0, 0.0].map(f32::to_bits)
            );
            assert_eq!(
                material.parameters.diffuse.to_array().map(f32::to_bits),
                [0.02_f32, 0.01, 0.0, 1.0].map(f32::to_bits)
            );
            assert_eq!(
                material.parameters.light_color,
                source.parameters.light_color
            );
            assert!(!material.depth_write);
            assert_eq!(
                material.depth_compare,
                (!detected_only).then_some(CompareFunction::Equal)
            );
            assert!(material.texture.is_none());
        }
    }
}

#[cfg(test)]
mod light_tests {
    use super::*;
    #[test]
    fn additional_heat_pass_retains_actual_selected_point_light_parameters() {
        let mut source =
            SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
        source.parameters.point_position[0] = Vec4::new(10.0, 20.0, -30.0, 100.0);
        source.parameters.point_ambient[0] = Vec4::new(0.5, 0.25, 0.125, 0.0008);
        let heat = heat_material(&source, 0.5, false);
        assert_eq!(
            heat.parameters.point_position,
            source.parameters.point_position
        );
        assert_eq!(
            heat.parameters.point_ambient,
            source.parameters.point_ambient
        );
    }
}
