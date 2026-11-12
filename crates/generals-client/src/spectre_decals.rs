//! Native Spectre attack-area/reticle `RadiusDecal` using original texture assets.
use crate::{
    control_bar::UiData, playtest::Match, source_material::SourceMaterial, source_texture,
    terrain::Terrain,
};
use bevy::ecs::system::SystemParam;
use bevy::{
    camera::visibility::{NoFrustumCulling, RenderLayers},
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{
    UnitType,
    spectre::{Decal, DecalView},
};
use num_traits::ToPrimitive;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

#[derive(SystemParam)]
pub struct DecalAssets<'w> {
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
}

#[derive(Resource)]
pub struct SpectreDecals {
    images: Vec<(String, Image)>,
    textures: BTreeMap<String, Handle<Image>>,
    visuals: BTreeMap<(usize, usize), Visual>,
    generation: u32,
}

struct Visual {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<SourceMaterial>,
    position: [u32; 2],
}

pub fn load(assets: &VirtualAssets, catalog: &[UnitType]) -> Result<SpectreDecals, Box<dyn Error>> {
    let names: BTreeSet<_> = catalog
        .iter()
        .filter_map(|unit| unit.spectre.as_ref())
        .flat_map(|rules| &rules.decals)
        .filter(|decal| !decal.texture.is_empty())
        .map(|decal| &decal.texture)
        .collect();
    let mut images = Vec::new();
    for name in names {
        let mut image = source_texture::load(assets, name)?;
        image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            ..default()
        });
        images.push((name.clone(), image));
    }
    Ok(SpectreDecals {
        images,
        textures: BTreeMap::new(),
        visuals: BTreeMap::new(),
        generation: u32::MAX,
    })
}

pub fn update(
    mut commands: Commands,
    mut decals: ResMut<SpectreDecals>,
    battle: Res<Match>,
    terrain: Res<Terrain>,
    ui: Res<UiData>,
    mut assets: DecalAssets,
) {
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
    let ui = ui.into_inner();
    for (name, image) in std::mem::take(&mut decals.images) {
        decals.textures.insert(name, assets.images.add(image));
    }
    if decals.generation != battle.generation {
        for (_, visual) in std::mem::take(&mut decals.visuals) {
            commands.entity(visual.entity).despawn();
            assets.meshes.remove(visual.mesh.id());
            assets.materials.remove(visual.material.id());
        }
        decals.generation = battle.generation;
    }
    let mut active = BTreeSet::new();
    for (id, actor) in battle.simulation.entities.iter().enumerate() {
        if !actor.exists() {
            continue;
        }
        let Some(rules) = &battle.simulation.catalog[actor.type_index].spectre else {
            continue;
        };
        let Some(views) = actor.spectre_decals(rules) else {
            continue;
        };
        for (index, view) in views.into_iter().enumerate() {
            // RadiusDecal tests the controlling local player, not observer visibility.
            if view.style.only_owner && actor.owner != 0
                || view.style.texture.is_empty()
                || view.radius <= 0.0
            {
                continue;
            }
            let key = (id, index);
            active.insert(key);
            let color = if view.style.color == [0; 4] {
                ui.results.colors[battle.simulation.player_colors[actor.owner]
                    .expect("Spectre controlling player color")]
            } else {
                [
                    view.style.color[0],
                    view.style.color[1],
                    view.style.color[2],
                ]
                .map(|value| f32::from(value) / 255.0)
            };
            decals.update_visual(&mut commands, key, &view, terrain, color, &mut assets);
        }
    }
    decals.visuals.retain(|key, visual| {
        if active.contains(key) {
            return true;
        }
        commands.entity(visual.entity).despawn();
        assets.meshes.remove(visual.mesh.id());
        assets.materials.remove(visual.material.id());
        false
    });
}

fn opacity(style: &Decal, frame: u32) -> f32 {
    let cycle = (frame % style.throb_frames)
        .to_f32()
        .expect("native frame conversion");
    let period = style
        .throb_frames
        .to_f32()
        .expect("native duration conversion");
    let theta = std::f32::consts::TAU * cycle / period;
    let percent = 0.5 * (theta.sin() + 1.0);
    ((style.opacity[0] + percent * (style.opacity[1] - style.opacity[0])) * 255.0).trunc() / 255.0
}

impl SpectreDecals {
    fn update_visual(
        &mut self,
        commands: &mut Commands,
        key: (usize, usize),
        view: &DecalView<'_>,
        terrain: &Terrain,
        color: [f32; 3],
        assets: &mut DecalAssets<'_>,
    ) {
        let position = Vec2::new(view.position[0], view.position[1]);
        let position_bits = [position.x.to_bits(), position.y.to_bits()];
        let alpha = if view.enabled {
            view.frame.map_or(1.0, |frame| opacity(view.style, frame))
        } else {
            0.0
        };
        if let Some(visual) = self.visuals.get_mut(&key) {
            if visual.position != position_bits {
                *assets
                    .meshes
                    .get_mut(&visual.mesh)
                    .expect("Spectre radius mesh") = terrain.radius_decal(
                    position,
                    view.radius,
                    [color[0], color[1], color[2], 1.0],
                );
                visual.position = position_bits;
            }
            assets
                .materials
                .get_mut(&visual.material)
                .expect("Spectre decal material")
                .parameters
                .opacity = alpha;
        } else {
            let mesh = assets.meshes.add(terrain.radius_decal(
                position,
                view.radius,
                [color[0], color[1], color[2], 1.0],
            ));
            let mut material =
                SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Blend, false);
            material.parameters.vertex_diffuse = 3;
            material.parameters.opacity = alpha;
            material.texture = Some(self.textures[&view.style.texture].clone());
            let material = assets.materials.add(material);
            let entity = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    NoFrustumCulling,
                    RenderLayers::layer(2),
                ))
                .id();
            self.visuals.insert(
                key,
                Visual {
                    entity,
                    mesh,
                    material,
                    position: position_bits,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radius_opacity_uses_global_frame_and_truncated_native_byte() {
        let style = Decal {
            texture: "radius".into(),
            opacity: [0.0, 1.0],
            throb_frames: 4,
            color: [0; 4],
            only_owner: true,
        };
        assert_eq!(opacity(&style, 0).to_bits(), (127.0_f32 / 255.0).to_bits());
        assert_eq!(opacity(&style, 1).to_bits(), 1.0_f32.to_bits());
        assert_eq!(opacity(&style, 3).to_bits(), 0.0_f32.to_bits());
        assert_eq!(opacity(&style, 4).to_bits(), opacity(&style, 0).to_bits());
    }
}
