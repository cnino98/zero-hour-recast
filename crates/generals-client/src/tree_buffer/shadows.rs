//! `W3DTreeBuffer::drawTrees` / `W3DProjectedShadowManager::queueDecal`, SH 2bfaea60.
use super::state::{Buffer, Topple};
use crate::{source_material::SourceMaterial, terrain::Terrain};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    image::{ImageAddressMode, ImageSampler},
    mesh::Indices,
    prelude::*,
    render::render_resource::{CompareFunction, PrimitiveTopology},
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{collections::BTreeMap, error::Error, path::Path};

// Z, tree scale, heading, sway and push are deliberately absent: the native
// standalone shadow keeps angle zero and projects the authored size onto terrain.
#[derive(PartialEq)]
struct Decal {
    center: [f32; 2],
    size: f32,
}

#[derive(Resource)]
pub struct Shadows {
    image: Option<Image>,
    mesh: Option<Handle<Mesh>>,
    mesh_entity: Option<Entity>,
    current: Vec<Decal>,
    next: Vec<Decal>,
}

pub fn load(
    assets: &VirtualAssets,
    root: &Path,
    has_trees: bool,
) -> Result<Shadows, Box<dyn Error>> {
    let text = match std::fs::read_to_string(root.join("UserData/Options.ini")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let options: BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect();
    let globals = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let levels = content::scan(assets.read("data/ini/gamelod.ini")?)?;
    let enabled = enabled(&options, &globals, &levels)?;
    Ok(Shadows {
        image: if enabled && has_trees {
            Some(texture(assets)?)
        } else {
            None
        },
        mesh: None,
        mesh_entity: None,
        current: Vec::new(),
        next: Vec::new(),
    })
}

fn enabled(
    options: &BTreeMap<&str, &str>,
    globals: &generals_assets::content::ContentFile,
    levels: &generals_assets::content::ContentFile,
) -> Result<bool, Box<dyn Error>> {
    let level = options.get("StaticGameLOD").copied().ok_or(
        "StaticGameLOD is Unknown: native hardware recommendation is an unresolved P07 dependency",
    )?;
    if level.eq_ignore_ascii_case("Custom") {
        let global = globals
            .declarations
            .iter()
            .find(|item| item.kind == "GameData")
            .ok_or("GameData missing")?;
        let fallback = decal_setting(global)?.ok_or("GameData UseShadowDecals missing")?;
        return Ok(options
            .get("UseShadowDecals")
            .copied()
            .unwrap_or(fallback)
            .eq_ignore_ascii_case("Yes"));
    }
    let name = ["Low", "Medium", "High", "VeryHigh"]
        .into_iter()
        .find(|name| name.eq_ignore_ascii_case(level))
        .ok_or("Unknown StaticGameLOD")?;
    // StaticGameLODInfo starts with true, then INI declarations overwrite fields.
    let mut result = true;
    for item in levels
        .declarations
        .iter()
        .filter(|item| item.kind == "StaticGameLOD" && item.name.eq_ignore_ascii_case(name))
    {
        if let Some(value) = decal_setting(item)? {
            result = value.eq_ignore_ascii_case("Yes");
        }
    }
    Ok(result)
}

fn decal_setting(
    item: &generals_assets::content::Declaration,
) -> Result<Option<&str>, Box<dyn Error>> {
    crate::skirmish_data::field(&item.lines, 0, "UseShadowDecals")
        .map(crate::skirmish_data::scalar)
        .transpose()
}

fn texture(assets: &VirtualAssets) -> Result<Image, Box<dyn Error>> {
    let mut image = crate::source_texture::load(assets, "shadow.tga")?;
    image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
    if let ImageSampler::Descriptor(sampler) = &mut image.sampler {
        sampler.address_mode_u = ImageAddressMode::ClampToEdge;
        sampler.address_mode_v = ImageAddressMode::ClampToEdge;
        // Set_Mip_Mapping(FILTER_TYPE_NONE), retaining original min/mag defaults.
        sampler.lod_min_clamp = 0.0;
        sampler.lod_max_clamp = 0.0;
    }
    Ok(image)
}

impl Shadows {
    // Reuses both selection allocations and keeps the aggregate GPU mesh untouched
    // while only tree sway/push changes. Rebuild only when decal geometry changes.
    fn select(&mut self, buffer: &Buffer) -> bool {
        self.next.clear();
        for tree in &buffer.trees {
            let Some(kind) = tree.kind else {
                continue;
            };
            let data = &buffer.types[kind];
            if tree.visible && data.shadow && !matches!(tree.topple, Topple::Falling | Topple::Down)
            {
                self.next.push(Decal {
                    center: [tree.position[0], tree.position[1]],
                    size: data.shadow_size,
                });
            }
        }
        if self.next == self.current {
            return false;
        }
        std::mem::swap(&mut self.next, &mut self.current);
        true
    }

    pub(super) fn draw(
        &mut self,
        buffer: &Buffer,
        terrain: &Terrain,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<SourceMaterial>,
        images: &mut Assets<Image>,
    ) {
        if self.image.is_none() && self.mesh.is_none() {
            return;
        }
        if !self.select(buffer) {
            return;
        }
        if self.current.is_empty() {
            if let Some(entity) = self.mesh_entity {
                commands.entity(entity).remove::<Mesh3d>();
            }
            return;
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new())
        .with_inserted_indices(Indices::U32(Vec::new()));
        for decal in &self.current {
            let part = terrain.object_decal(
                Vec2::from_array(decal.center),
                Vec2::splat(decal.size),
                Vec2::ZERO,
                0.0,
                [1.0; 4],
            );
            mesh.merge(&part)
                .expect("matching projected shadow attributes");
        }
        if mesh.count_vertices() == 0 || mesh.indices().is_none_or(Indices::is_empty) {
            if let Some(entity) = self.mesh_entity {
                commands.entity(entity).remove::<Mesh3d>();
            }
            return;
        }
        if let Some(handle) = &self.mesh {
            *meshes.get_mut(handle).expect("tree shadow aggregate mesh") = mesh;
            commands
                .entity(self.mesh_entity.expect("tree shadow aggregate entity"))
                .insert(Mesh3d(handle.clone()));
        } else {
            let handle = meshes.add(mesh);
            let mut material = SourceMaterial::new(
                [0.0; 3],
                [0.0; 3],
                [0.0; 3],
                1.0,
                AlphaMode::Multiply,
                false,
            );
            material.parameters.vertex_diffuse = 3;
            material.depth_compare = Some(CompareFunction::GreaterEqual);
            material.texture =
                Some(images.add(self.image.take().expect("original shadow texture")));
            let entity = commands
                .spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(materials.add(material)),
                    Transform::IDENTITY,
                    Visibility::Visible,
                    NoFrustumCulling,
                    RenderLayers::layer(12),
                ))
                .id();
            self.mesh_entity = Some(entity);
            self.mesh = Some(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> generals_assets::content::ContentFile {
        content::scan(generals_assets::virtual_assets::Asset {
            path: "tree-shadows.ini".into(),
            origin: "native GameLOD fixture".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap()
    }

    #[test]
    fn selected_static_lod_overrides_user_shadow_option_but_custom_uses_it() {
        let globals = fixture("GameData\n UseShadowDecals = No\nEnd\n");
        let levels = fixture(
            "StaticGameLOD Low\n UseShadowDecals = No\nEnd\nStaticGameLOD High\n UseShadowDecals = Yes\nEnd\n",
        );
        let mut options = BTreeMap::from([("StaticGameLOD", "High"), ("UseShadowDecals", "No")]);
        assert!(enabled(&options, &globals, &levels).unwrap());
        options.insert("StaticGameLOD", "Low");
        options.insert("UseShadowDecals", "Yes");
        assert!(!enabled(&options, &globals, &levels).unwrap());
        options.insert("StaticGameLOD", "Custom");
        assert!(enabled(&options, &globals, &levels).unwrap());
        options.remove("UseShadowDecals");
        assert!(!enabled(&options, &globals, &levels).unwrap());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn shadow_selection_precedes_topple_update_and_ignores_visual_deformation() {
        let mut data = super::super::tests::original();
        let rules = std::sync::Arc::get_mut(&mut data).unwrap();
        rules.shadow = true;
        rules.shadow_size = 24.0;
        let mut buffer = Buffer::new([1000.0; 2]);
        buffer.add(0, data, [500.0, 400.0, 50.0], 1.2, 3.0, 1);
        let mut shadows = Shadows {
            image: None,
            mesh: None,
            mesh_entity: None,
            current: Vec::new(),
            next: Vec::new(),
        };
        assert!(!shadows.select(&buffer)); // Cull result still hidden.
        buffer.trees[0].visible = true;
        assert!(shadows.select(&buffer));
        assert_eq!(
            shadows.current[0].center.map(f32::to_bits),
            [500.0_f32, 400.0].map(f32::to_bits)
        );
        assert_eq!(shadows.current[0].size.to_bits(), 24.0_f32.to_bits()); // Authored width, not 3x scale.
        buffer.trees[0].push = 0.5;
        buffer.trees[0].position[2] = 20.0;
        assert!(!shadows.select(&buffer)); // Sway/push/Z never reuploads a decal.
        buffer.trees[0].topple = Topple::Falling;
        assert!(shadows.select(&buffer));
        assert!(shadows.current.is_empty());
        // Fogged topples are admitted by native pre-update selection. Revealing
        // may change them to DOWN this draw; the queued shadow lasts that draw.
        buffer.trees[0].topple = Topple::Fogged;
        assert!(shadows.select(&buffer));
        assert_eq!(shadows.current.len(), 1);
        buffer.draw(
            &super::super::tests::breeze(),
            0.0,
            |low, high| (low + high) * 0.5,
            |_| false,
        );
        assert!(matches!(buffer.trees[0].topple, Topple::Down));
        assert_eq!(shadows.current.len(), 1);
        assert!(shadows.select(&buffer));
        assert!(shadows.current.is_empty());
        buffer.trees[0].topple = Topple::Upright;
        assert!(shadows.select(&buffer));
        buffer.trees[0].kind = None;
        assert!(shadows.select(&buffer));
        assert!(shadows.current.is_empty());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn original_shadow_texture_retains_encoded_pixels_and_disables_wrapping_and_mips() {
        let root = &generals_assets::game_root().unwrap();
        let assets = VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let original = crate::source_texture::load(&assets, "shadow.tga").unwrap();
        let image = texture(&assets).unwrap();
        assert_eq!(image.data, original.data);
        assert_eq!(
            image.texture_descriptor.format,
            original.texture_descriptor.format.remove_srgb_suffix()
        );
        assert!(image.width() > 1 && image.height() > 1);
        let ImageSampler::Descriptor(sampler) = image.sampler else {
            panic!("native shadow sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.lod_max_clamp.to_bits(), 0.0_f32.to_bits());
    }

    fn flat_terrain() -> Terrain {
        use generals_assets::{
            HeightMap,
            map::{TerrainTiles, TextureClass},
        };
        let map = HeightMap {
            width: 8,
            height: 8,
            border: 2,
            boundaries: vec![[4, 4]],
            heights: vec![0; 64],
        };
        let tiles = TerrainTiles {
            width: 8,
            height: 8,
            base_tiles: vec![0; 64],
            blend_tiles: vec![0; 64],
            extra_blend_tiles: vec![0; 64],
            cliff_tiles: vec![0; 64],
            cliff_flags: vec![0; 8],
            texture_classes: vec![TextureClass {
                name: "SourceSand".to_owned(),
                first_tile: 0,
                tile_count: 1,
                width: 1,
            }],
            edge_classes: Vec::new(),
            blends: vec![None],
            cliffs: vec![None],
        };
        Terrain::new(map, tiles, true, true).unwrap()
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn aggregate_shadow_detaches_when_empty_and_reuses_current_decal_when_visible() {
        use bevy::ecs::world::CommandQueue;
        let mut data = super::super::tests::original();
        let rules = std::sync::Arc::get_mut(&mut data).unwrap();
        rules.shadow = true;
        rules.shadow_size = 12.0;
        let mut buffer = Buffer::new([40.0; 2]);
        buffer.add(0, data, [10.0, 10.0, 0.0], 0.0, 1.0, 1);
        let mut shadows = Shadows {
            image: Some(Image::default()),
            mesh: None,
            mesh_entity: None,
            current: Vec::new(),
            next: Vec::new(),
        };
        let terrain = flat_terrain();
        let mut world = World::new();
        let mut commands = CommandQueue::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<SourceMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut retained = None;
        let mut first_uv = None;
        for visible in [false, true, false, true] {
            buffer.trees[0].visible = visible;
            if visible && retained.is_some() {
                buffer.trees[0].position[0] += 5.0;
            }
            shadows.draw(
                &buffer,
                &terrain,
                &mut Commands::new(&mut commands, &world),
                &mut meshes,
                &mut materials,
                &mut images,
            );
            commands.apply(&mut world);
            let Some(entity) = shadows.mesh_entity else {
                assert!(!visible);
                assert!(meshes.is_empty());
                continue;
            };
            let handle = shadows.mesh.as_ref().unwrap();
            assert_eq!(meshes.len(), 1);
            assert_eq!(materials.len(), 1);
            assert_eq!(images.len(), 1);
            let mut drawables =
                world.query_filtered::<Entity, With<MeshMaterial3d<SourceMaterial>>>();
            assert_eq!(drawables.iter(&world).collect::<Vec<_>>(), [entity]);
            assert_eq!(world.get::<Mesh3d>(entity).is_some(), visible);
            let mesh = meshes.get(handle).unwrap();
            assert!(mesh.count_vertices() > 0);
            assert!(!mesh.indices().unwrap().is_empty());
            let bevy::mesh::VertexAttributeValues::Float32x2(uv) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
            else {
                panic!("projected shadow UVs");
            };
            let uv = uv
                .iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>();
            if let Some((id, original_entity)) = retained {
                assert_eq!(handle.id(), id);
                assert_eq!(entity, original_entity);
                if visible {
                    assert_ne!(first_uv.as_ref().unwrap(), &uv);
                } else {
                    assert_eq!(first_uv.as_ref().unwrap(), &uv);
                }
            } else {
                retained = Some((handle.id(), entity));
                first_uv = Some(uv);
            }
        }
    }
}
