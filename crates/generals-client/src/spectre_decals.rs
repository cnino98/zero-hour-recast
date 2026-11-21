//! Native persistent `RadiusDecal` overlays using original textures.
use crate::{playtest::Match, source_material::SourceMaterial, source_texture, terrain::Terrain};
use bevy::ecs::system::SystemParam;
use bevy::{
    camera::visibility::{NoFrustumCulling, RenderLayers},
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};
use generals_assets::virtual_assets::VirtualAssets;
use generals_sim::skirmish::{
    Skirmish, power_ocl,
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
    geometry: Geometry,
}

#[derive(PartialEq, Eq)]
struct Geometry {
    position: [u32; 2],
    radius: u32,
    color: [u32; 4],
}

#[derive(SystemParam)]
pub struct GridInspection<'w, 's> {
    decals: Res<'w, SpectreDecals>,
    visuals: Query<'w, 's, &'static ViewVisibility>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
}

impl GridInspection<'_, '_> {
    pub fn visible(&self, actor: usize, generation: u32) -> Result<bool, &'static str> {
        use bevy::mesh::VertexAttributeValues;

        if self.decals.generation != generation {
            return Ok(false);
        }
        let texture = self
            .decals
            .textures
            .get("EXGrid")
            .ok_or("EXGrid texture missing")?;
        let mut visible = 0;
        for (_, visual) in self.decals.visuals.range((actor, 3)..=(actor, usize::MAX)) {
            let Some(material) = self.materials.get(&visual.material) else {
                continue;
            };
            if material.texture.as_ref() != Some(texture)
                || !matches!(material.alpha_mode, AlphaMode::Add)
                || self.images.get(texture).is_none()
            {
                return Err("original grid decal lost its additive EXGrid material");
            }
            let Some(mesh) = self.meshes.get(&visual.mesh) else {
                continue;
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                return Err("original grid decal has no packed source colors");
            };
            if self
                .visuals
                .get(visual.entity)
                .is_ok_and(|visible| visible.get())
                && mesh.count_vertices() >= 3
                && colors.iter().any(|color| color[2] > 0.0)
            {
                visible += 1;
            }
        }
        Ok(visible == 30)
    }

    pub fn retired(&self, actor: usize, generation: u32) -> bool {
        self.decals.generation == generation
            && self
                .decals
                .visuals
                .range((actor, 3)..=(actor, usize::MAX))
                .next()
                .is_none()
    }
}

pub fn load(
    assets: &VirtualAssets,
    simulation: &Skirmish,
) -> Result<SpectreDecals, Box<dyn Error>> {
    let mut names = BTreeSet::new();
    for unit in &simulation.catalog {
        if let Some(rules) = &unit.spectre {
            names.extend(rules.decals.iter().map(|decal| &decal.texture));
        }
        if let Some(rules) = &unit.behavior.neutron_missile {
            names.insert(&rules.decal.texture);
        }
        for rules in &unit.behavior.dynamic_shroud {
            if let Some(decal) = &rules.grid_decal {
                names.insert(&decal.texture);
            }
        }
        for rules in &unit.behavior.ocl_powers {
            let lists =
                std::iter::once(&rules.default).chain(rules.upgrades.iter().map(|(_, list)| list));
            for nugget in lists.flat_map(|list| &list.nuggets) {
                if let power_ocl::Nugget::Attack { decal, .. } = nugget {
                    names.insert(&decal.texture);
                }
            }
        }
    }
    names.extend(
        simulation
            .delivery_decal_templates()
            .map(|decal| &decal.texture),
    );
    let mut images = Vec::new();
    for name in names.into_iter().filter(|name| !name.is_empty()) {
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
    mut assets: DecalAssets,
) {
    let battle = battle.into_inner();
    let terrain = terrain.into_inner();
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
        let unit = &battle.simulation.catalog[actor.type_index];
        let mut update = |index, view: DecalView<'_>, opacity_override: Option<i32>| {
            // RadiusDecal tests the controlling local player, not observer visibility.
            if view.style.only_owner && actor.owner != 0
                || view.style.texture.is_empty()
                || view.radius <= 0.0
            {
                return;
            }
            let key = (id, index);
            active.insert(key);
            let color = if view.style.color == [0; 4] {
                let bytes = battle.simulation.players[actor.owner]
                    .color
                    .expect("radius decal owning player has a source color")
                    .to_be_bytes();
                [bytes[1], bytes[2], bytes[3]]
            } else {
                [
                    view.style.color[0],
                    view.style.color[1],
                    view.style.color[2],
                ]
            };
            let alpha = if view.enabled {
                opacity_override
                    .unwrap_or_else(|| view.frame.map_or(255, |frame| opacity(view.style, frame)))
            } else {
                0
            };
            decals.update_visual(
                &mut commands,
                key,
                &view,
                terrain,
                decal_color(view.style, color, alpha),
                &mut assets,
            );
        };
        if let Some(views) = unit
            .spectre
            .as_ref()
            .and_then(|rules| actor.spectre_decals(rules))
        {
            for (index, view) in views.into_iter().enumerate() {
                update(index, view, None);
            }
        }
        if let Some(mut view) = actor.radius_decal() {
            view.enabled = battle.simulation.draw_icon_ui;
            update(2, view, None);
        }
        for (ordinal, rules) in unit.behavior.dynamic_shroud.iter().enumerate() {
            if let Some(style) = &rules.grid_decal
                && let Some((positions, opacity)) =
                    battle.simulation.dynamic_shroud_grid_slots(id, ordinal)
            {
                for (index, position) in positions.into_iter().enumerate() {
                    update(
                        3 + ordinal * positions.len() + index,
                        DecalView {
                            style,
                            position,
                            radius: 100.0,
                            frame: None,
                            enabled: true,
                        },
                        Some(quantize_opacity(opacity)),
                    );
                }
            }
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

fn opacity(style: &Decal, frame: u32) -> i32 {
    let cycle = (frame % style.throb_frames)
        .to_f32()
        .expect("native frame conversion");
    let period = style
        .throb_frames
        .to_f32()
        .expect("native duration conversion");
    let theta = std::f32::consts::TAU * cycle / period;
    let percent = 0.5 * (theta.sin() + 1.0);
    quantize_opacity(style.opacity[0] + percent * (style.opacity[1] - style.opacity[0]))
}

fn quantize_opacity(alpha: f32) -> i32 {
    // The stock Spy Drone's zero clearing range produces 0/0 in the native
    // grid opacity. REAL_TO_INT keeps the x86 invalid-conversion bit pattern.
    (alpha * 255.0).to_i32().unwrap_or(i32::MIN)
}

fn decal_color(style: &Decal, color: [u8; 3], opacity: i32) -> [f32; 4] {
    let bytes = if style.additive {
        // Shadow stores the signed setter argument in an unsigned member
        // before converting it back to Real for additive shading.
        let alpha = u32::from_ne_bytes(opacity.to_ne_bytes())
            .to_f32()
            .expect("native opacity conversion")
            / 255.0;
        let [red, green, blue] =
            color.map(|channel| (f32::from(channel) * alpha).to_i32().unwrap_or(i32::MIN));
        // Shadow::setOpacity ORs the full signed results without masking or
        // shifting them. An invalid opacity can therefore affect every byte.
        (blue | green | red).to_be_bytes()
    } else {
        let [red, green, blue] = color;
        [opacity.to_be_bytes()[3], red, green, blue]
    };
    let [alpha, red, green, blue] = bytes.map(|channel| f32::from(channel) / 255.0);
    [red, green, blue, alpha]
}

impl SpectreDecals {
    fn update_visual(
        &mut self,
        commands: &mut Commands,
        key: (usize, usize),
        view: &DecalView<'_>,
        terrain: &Terrain,
        color: [f32; 4],
        assets: &mut DecalAssets<'_>,
    ) {
        let position = Vec2::new(view.position[0], view.position[1]);
        let mesh_color = if view.style.additive {
            color
        } else {
            [color[0], color[1], color[2], 1.0]
        };
        let material_opacity = if view.style.additive { 1.0 } else { color[3] };
        let geometry = Geometry {
            position: [position.x.to_bits(), position.y.to_bits()],
            radius: view.radius.to_bits(),
            color: mesh_color.map(f32::to_bits),
        };
        if let Some(visual) = self.visuals.get_mut(&key) {
            if visual.geometry != geometry {
                *assets
                    .meshes
                    .get_mut(&visual.mesh)
                    .expect("radius decal mesh") =
                    terrain.radius_decal(position, view.radius, mesh_color);
                visual.geometry = geometry;
            }
            let mut material = assets
                .materials
                .get_mut(&visual.material)
                .expect("radius decal material");
            material.parameters.opacity = material_opacity;
            material.texture = Some(self.textures[&view.style.texture].clone());
            material.alpha_mode = if view.style.additive {
                AlphaMode::Add
            } else {
                AlphaMode::Blend
            };
        } else {
            let mesh = assets
                .meshes
                .add(terrain.radius_decal(position, view.radius, mesh_color));
            let mut material = SourceMaterial::new(
                [0.0; 3],
                [0.0; 3],
                [0.0; 3],
                1.0,
                if view.style.additive {
                    AlphaMode::Add
                } else {
                    AlphaMode::Blend
                },
                false,
            );
            material.parameters.vertex_diffuse = 3;
            material.parameters.opacity = material_opacity;
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
                    geometry,
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
            additive: false,
        };
        assert_eq!(opacity(&style, 0), 127);
        assert_eq!(opacity(&style, 1), 255);
        assert_eq!(opacity(&style, 3), 0);
        assert_eq!(opacity(&style, 4), opacity(&style, 0));
    }

    #[test]
    fn dynamic_grid_preserves_full_signed_additive_packing() {
        let style = Decal {
            texture: "EXGrid".into(),
            opacity: [0.5, 1.0],
            throb_frames: 30,
            color: [32, 64, 128, 0],
            only_owner: true,
            additive: true,
        };
        let expected = [0.0, 0.0, 224.0 / 255.0, 0.0].map(f32::to_bits);
        assert_eq!(
            decal_color(&style, [32, 64, 128], 255).map(f32::to_bits),
            expected
        );
        let zero_range_opacity = quantize_opacity(f32::NAN);
        let packed = [112.0, 112.0, 224.0, 112.0].map(|byte| (byte / 255.0_f32).to_bits());
        assert_eq!(zero_range_opacity, i32::MIN);
        assert_eq!(
            decal_color(&style, [32, 64, 128], zero_range_opacity).map(f32::to_bits),
            packed
        );
    }
}
