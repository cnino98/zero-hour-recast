use super::{
    data::Data,
    state::{Buffer, Topple},
};
use generals_sim::{
    collision_geometry::{Geometry, Shape},
    skirmish::map_scripts::Breeze,
};
use std::sync::Arc;

fn installed() -> generals_assets::virtual_assets::VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

pub(super) fn original() -> Arc<Data> {
    original_named("TreeFir01B")
}

fn original_named(name: &str) -> Arc<Data> {
    let assets = installed();
    let definitions = crate::world_data::object_definitions(&assets).unwrap();
    let object = definitions.get(name).unwrap();
    let draws = crate::skirmish_data::blocks(&object.lines, 0, "Draw").unwrap();
    let draw = draws
        .into_iter()
        .find(|draw| {
            draw[0]
                .tokens
                .get(1)
                .is_some_and(|family| family == "W3DTreeDraw")
        })
        .unwrap();
    let model = crate::skirmish_data::normal_draw_model(&assets, draw).unwrap();
    Arc::new(Data::load(&assets, draw, &model).unwrap())
}

fn movable() -> Geometry {
    Geometry {
        shape: Shape::Cylinder,
        is_small: true,
        major: 5.0,
        minor: 5.0,
        height: 10.0,
    }
}
pub(super) fn breeze() -> Breeze {
    Breeze {
        direction: [1.0, 0.0],
        intensity: 0.2,
        lean: 0.1,
        randomness: 0.5,
        period: 300,
        version: 0,
    }
}

#[ignore = "requires original game installation"]
#[test]
fn original_tree_uses_tga_mesh_colors_atlas_and_real_shader_vertex_payload() {
    let data = original();
    assert!(!data.vertices.is_empty());
    assert_eq!(data.vertices.len(), data.uv.len());
    assert_eq!(data.vertices.len(), data.colors.len());
    assert!(data.radius > 0.0);
    assert!(data.pixels.width > 0);
    let mut buffer = Buffer::new([1000.0; 2]);
    buffer.add(0, data, [500.0, 500.0, 0.0], 0.0, 1.0, 1);
    buffer.trees[0].visible = true;
    let atlas = super::atlas::Atlas::new(&buffer.types).unwrap();
    let light = super::Lighting {
        ambient: [1.0; 3],
        colors: [[0.0; 3]; 3],
        directions: [[0.0, 0.0, -1.0]; 3],
    };
    let mesh = buffer.mesh(&atlas, &light);
    assert_eq!(mesh.count_vertices(), buffer.types[0].vertices.len());
    let bevy::mesh::VertexAttributeValues::Float32x3(packed) = mesh
        .attribute(bevy::prelude::Mesh::ATTRIBUTE_NORMAL)
        .unwrap()
    else {
        panic!("native tree packed normal");
    };
    assert!(
        packed
            .iter()
            .all(|normal| normal.map(f32::to_bits) == [1.0_f32, 1.0, 0.0].map(f32::to_bits))
    );
}

#[ignore = "requires original game installation"]
#[test]
fn ordered_buckets_keep_strict_contact_height_and_crusher_topple_precedence() {
    let mut data = original();
    let rules = Arc::get_mut(&mut data).unwrap();
    rules.topple = true;
    rules.outward = 4;
    rules.inward = 2;
    let mut buffer = Buffer::new([1000.0; 2]);
    buffer.add(0, Arc::clone(&data), [500.0, 500.0, 0.0], 0.0, 1.0, 1);
    buffer.add(1, data, [500.0, 500.0, 0.0], 0.0, 1.0, 2);
    buffer.moved(3, 9, [488.0, 500.0, 0.0], [1.0, 0.0, 0.0], &movable(), 2);
    assert!(
        buffer
            .trees
            .iter()
            .all(|tree| matches!(tree.topple, Topple::Upright))
    );
    buffer.moved(4, 9, [495.0, 500.0, 12.0], [1.0, 0.0, 0.0], &movable(), 2);
    assert!(
        buffer
            .trees
            .iter()
            .all(|tree| matches!(tree.topple, Topple::Upright))
    );
    buffer.moved(5, 9, [495.0, 500.0, 0.0], [1.0, 0.0, 0.0], &movable(), 2);
    assert!(
        buffer
            .trees
            .iter()
            .all(|tree| matches!(tree.topple, Topple::Falling) && tree.push == 0.0)
    );
    assert_eq!(buffer.effects.len(), 2);
}

#[ignore = "requires original game installation"]
#[test]
fn push_progress_is_per_terrain_draw_while_sway_and_topple_follow_logic_ratio() {
    let mut data = original();
    let rules = Arc::get_mut(&mut data).unwrap();
    rules.topple = true;
    rules.outward = 4;
    rules.inward = 2;
    let mut buffer = Buffer::new([1000.0; 2]);
    buffer.add(0, data, [500.0, 500.0, 0.0], 0.0, 1.0, 1);
    buffer.moved(3, 9, [495.0, 501.0, 0.0], [1.0, 0.0, 0.0], &movable(), 1);
    let mut draws = 0;
    let wave = buffer.draw(
        &breeze(),
        0.0,
        |low, high| {
            draws += 1;
            (low + high) * 0.5
        },
        |_| false,
    );
    assert_eq!(draws, 20);
    assert_eq!(buffer.trees[0].push.to_bits(), 0.25_f32.to_bits());
    assert_eq!(wave[0].map(f32::to_bits), [0.0_f32; 3].map(f32::to_bits));
    assert!(wave[1][0] > 0.0);
    buffer.draw(
        &breeze(),
        0.25,
        |_, _| panic!("unchanged breeze draws no RNG"),
        |_| false,
    );
    assert_eq!(buffer.trees[0].push.to_bits(), 0.5_f32.to_bits());
    buffer.moved(7, 9, [495.0, 501.0, 0.0], [1.0, 0.0, 0.0], &movable(), 2);
    let basis = buffer.trees[0].basis;
    buffer.draw(&breeze(), 0.0, |_, _| panic!("unchanged breeze"), |_| false);
    assert_eq!(
        basis.map(|row| row.map(f32::to_bits)),
        buffer.trees[0].basis.map(|row| row.map(f32::to_bits))
    );
    buffer.draw(&breeze(), 0.5, |_, _| panic!("unchanged breeze"), |_| false);
    assert_ne!(
        basis.map(|row| row.map(f32::to_bits)),
        buffer.trees[0].basis.map(|row| row.map(f32::to_bits))
    );
}

#[ignore = "requires original game installation"]
#[test]
fn fogged_topple_pauses_then_reveal_snaps_down_before_expiring_sink() {
    let mut data = original();
    let rules = Arc::get_mut(&mut data).unwrap();
    rules.topple = true;
    rules.kill = true;
    let mut buffer = Buffer::new([1000.0; 2]);
    buffer.add(0, data, [500.0, 500.0, 0.0], 0.0, 1.0, 1);
    buffer.moved(3, 9, [495.0, 500.0, 0.0], [1.0, 0.0, 0.0], &movable(), 2);
    buffer.draw(&breeze(), 1.0, |low, high| (low + high) * 0.5, |_| true);
    assert!(matches!(buffer.trees[0].topple, Topple::Fogged));
    assert!(buffer.trees[0].kind.is_some());
    buffer.draw(&breeze(), 1.0, |_, _| panic!("unchanged breeze"), |_| false);
    assert!(matches!(buffer.trees[0].topple, Topple::Down));
    assert!(buffer.trees[0].kind.is_some());
    buffer.draw(&breeze(), 1.0, |_, _| panic!("unchanged breeze"), |_| false);
    assert!(buffer.trees[0].kind.is_none());
}

fn recording_world() -> bevy::prelude::World {
    use bevy::prelude::*;
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    battle.started = false;
    battle.simulation.presentation.weapon_fx.clear();
    battle
        .simulation
        .presentation
        .terrain_movement_events
        .clear();
    let catalog = &battle.simulation.catalog;
    let root = &generals_assets::game_root().unwrap();
    let mut world = World::new();
    world.insert_resource(crate::weapon_fx::load(&assets, catalog).unwrap());
    world.insert_resource(crate::laser_beam::load(&assets, catalog).unwrap());
    world.insert_resource(crate::particles::load(&assets, catalog).unwrap());
    world.insert_resource(
        crate::source_audio::load(&assets, catalog, &root.join("UserData/Options.ini")).unwrap(),
    );
    world.insert_resource(crate::selection_feedback::load(&assets).unwrap());
    world.insert_resource(crate::structure_view::Structures::default());
    world.insert_resource(crate::model_animation::Animations::effect_fixture(19));
    world.insert_resource(Time::<()>::default());
    world.insert_resource(super::Trees::new(
        [1000.0; 2],
        super::Lighting {
            ambient: [1.0; 3],
            colors: [[0.0; 3]; 3],
            directions: [[0.0, 0.0, -1.0]; 3],
        },
    ));
    let data = original();
    world.insert_resource(crate::playtest_view::WorldObjects(
        (0..2)
            .map(|index| crate::playtest_view::WorldObject {
                animation: None,
                model_index: 0,
                position: [500.0, 500.0, 0.0],
                angle: 0.0,
                scale: 1.0,
                tree: true,
                tree_data: Some((index, Arc::clone(&data))),
                prop_radius: None,
                supply: None,
                supply_draw: None,
            })
            .collect(),
    ));
    world.insert_resource(battle);
    world
}

#[ignore = "requires original game installation"]
#[test]
fn ordered_client_record_keeps_startup_and_same_tick_appends_once_across_restart() {
    use bevy::ecs::system::RunSystemOnce;
    use generals_sim::skirmish::{WeaponFxEvent, terrain_movement::Event};
    let mut world = recording_world();
    for index in 0..2 {
        {
            let mut battle = world.resource_mut::<crate::playtest::Match>();
            battle
                .simulation
                .presentation
                .terrain_movement_events
                .push(Event::TreeAdded(index));
            battle
                .simulation
                .presentation
                .weapon_fx
                .push(WeaponFxEvent::TerrainMovement(index));
        }
        world.run_system_once(crate::weapon_fx::record).unwrap();
        world.run_system_once(crate::weapon_fx::record).unwrap();
        assert_eq!(
            world.resource::<super::Trees>().buffer.trees.len(),
            index + 1
        );
    }
    {
        let mut battle = world.resource_mut::<crate::playtest::Match>();
        battle.simulation.tick += 1;
        battle.simulation.presentation.weapon_fx.clear();
        battle
            .simulation
            .presentation
            .terrain_movement_events
            .clear();
    }
    world.run_system_once(crate::weapon_fx::record).unwrap();
    assert_eq!(world.resource::<super::Trees>().buffer.trees.len(), 2);
    {
        let mut battle = world.resource_mut::<crate::playtest::Match>();
        battle.generation += 1;
        battle.simulation.tick = 0;
        battle
            .simulation
            .presentation
            .terrain_movement_events
            .push(Event::TreeAdded(0));
        battle
            .simulation
            .presentation
            .weapon_fx
            .push(WeaponFxEvent::TerrainMovement(0));
    }
    world.run_system_once(crate::weapon_fx::record).unwrap();
    assert_eq!(world.resource::<super::Trees>().buffer.trees.len(), 1);
}

#[ignore = "requires original game installation"]
#[test]
fn original_vines_missing_tga_keeps_mesh_and_samples_actual_atlas_origin() {
    use bevy::{mesh::VertexAttributeValues, prelude::Mesh};
    let assets = installed();
    for directory in ["art/terrain", "art/textures"] {
        assert!(matches!(
            assets.read(&format!("{directory}/pmvines01.tga")),
            Err(generals_assets::AssetError::Missing(_))
        ));
    }
    assert!(assets.read("art/textures/pmvines01.dds").is_ok());
    let vines = original_named("VinesGreen");
    assert_eq!(vines.pixels.width, 0);
    assert!(!vines.pixels.half);
    assert!(!vines.vertices.is_empty());
    let vine_vertices = vines.vertices.len();
    let neighbor = original();
    assert!(neighbor.pixels.width > 0);
    let expected = super::atlas::Atlas::new(&[Arc::clone(&neighbor)]).unwrap();
    let mut buffer = Buffer::new([1000.0; 2]);
    buffer.add(0, vines, [500.0, 500.0, 0.0], 0.0, 1.0, 1);
    buffer.add(1, neighbor, [600.0, 500.0, 0.0], 0.0, 1.0, 2);
    for tree in &mut buffer.trees {
        tree.visible = true;
    }
    let atlas = super::atlas::Atlas::new(&buffer.types).unwrap();
    assert_eq!(atlas.extent, expected.extent);
    assert_eq!(atlas.rgba, expected.rgba);
    assert_eq!(atlas.origins, [[0, 0], [0, 0]]);
    let light = super::Lighting {
        ambient: [1.0; 3],
        colors: [[0.0; 3]; 3],
        directions: [[0.0, 0.0, -1.0]; 3],
    };
    let mesh = buffer.mesh(&atlas, &light);
    assert_eq!(
        mesh.count_vertices(),
        vine_vertices + buffer.types[1].vertices.len()
    );
    let VertexAttributeValues::Float32x2(uv) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap() else {
        panic!("native tree UV0");
    };
    assert!(
        uv[..vine_vertices]
            .iter()
            .all(|point| point.map(f32::to_bits) == [0_u32; 2])
    );
    assert!(
        uv[vine_vertices..]
            .iter()
            .any(|point| point.map(f32::to_bits) != [0_u32; 2])
    );
}

#[test]
fn tree_tga_missing_lookup_does_not_hide_corrupt_or_unreadable_assets() {
    use generals_assets::virtual_assets::{Layer, VirtualAssets};
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir()
        .join("generals-client-tests")
        .join(format!("tree-tga-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(root.join("art/terrain")).unwrap();
    std::fs::create_dir_all(root.join("art/textures")).unwrap();
    let terrain = root.join("art/terrain/check.tga");
    std::fs::write(&terrain, [0_u8; 3]).unwrap();
    let mut tga = vec![0_u8; 18 + 64 * 64 * 3];
    tga[2] = 2;
    tga[12] = 64;
    tga[14] = 64;
    tga[16] = 24;
    std::fs::write(root.join("art/textures/check.tga"), tga).unwrap();
    let assets = VirtualAssets::from_layers(vec![Layer::Loose(root.clone())]);
    let corrupt = super::atlas::Texture::load(&assets, "check.tga");
    std::fs::remove_file(terrain).unwrap();
    let fallback = super::atlas::Texture::load(&assets, "check.tga");
    std::fs::remove_dir_all(&root).unwrap();
    let unreadable = super::atlas::Texture::load(&assets, "check.tga");
    assert!(corrupt.is_err());
    assert_eq!(fallback.unwrap().width, 1);
    assert!(unreadable.is_err());
}

#[test]
fn atlas_image_preserves_full_mip_payload_at_native_512_and_2048_extents() {
    use bevy::{
        asset::RenderAssetUsages,
        image::{ImageAddressMode, ImageFilterMode, ImageSampler},
        render::render_resource::{TextureDimension, TextureFormat},
    };
    // Constant quadrants remain unchanged down to 2x2; the last pixel is their
    // rounded RGBA box average [91,102,113,124], independently calculated.
    let quadrants = [
        [1, 3, 5, 7],
        [20, 40, 60, 80],
        [100, 120, 140, 160],
        [241, 243, 245, 247],
    ];
    for extent in [512_usize, 2048] {
        let mut rgba = Vec::with_capacity(extent * extent * 4);
        for y in 0..extent {
            for x in 0..extent {
                rgba.extend_from_slice(
                    &quadrants[usize::from(x >= extent / 2) + 2 * usize::from(y >= extent / 2)],
                );
            }
        }
        let atlas = super::atlas::Atlas {
            extent,
            origins: vec![[0, 0]],
            rgba,
        };
        let image = super::atlas_image(&atlas);
        assert_eq!(image.width(), u32::try_from(extent).unwrap());
        assert_eq!(image.height(), u32::try_from(extent).unwrap());
        assert_eq!(image.texture_descriptor.dimension, TextureDimension::D2);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(image.texture_descriptor.mip_level_count, extent.ilog2() + 1);
        assert_eq!(
            image.asset_usage,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD
        );
        let bytes = image.data.as_ref().unwrap();
        assert_eq!(&bytes[..atlas.rgba.len()], atlas.rgba);
        let mut offset = 0;
        let mut width = extent;
        loop {
            let length = width * width * 4;
            let level = &bytes[offset..offset + length];
            for (index, pixel) in level.chunks_exact(4).enumerate() {
                let expected = if width == 1 {
                    [91, 102, 113, 124]
                } else {
                    quadrants[usize::from(index % width >= width / 2)
                        + 2 * usize::from(index / width >= width / 2)]
                };
                assert_eq!(
                    pixel, expected,
                    "atlas{extent} mip width{width} pixel{index}"
                );
            }
            offset += length;
            if width == 1 {
                break;
            }
            width /= 2;
        }
        assert_eq!(offset, bytes.len());
        let ImageSampler::Descriptor(sampler) = image.sampler else {
            panic!("native atlas sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
        assert_eq!(sampler.mag_filter, ImageFilterMode::Linear);
        assert_eq!(sampler.min_filter, ImageFilterMode::Linear);
        assert_eq!(sampler.mipmap_filter, ImageFilterMode::Linear);
    }
}

#[ignore = "requires original game installation"]
#[test]
fn aggregate_tree_mesh_detaches_when_empty_and_reuses_current_geometry_when_visible() {
    use bevy::{ecs::world::CommandQueue, prelude::*};

    let lighting = super::Lighting {
        ambient: [1.0; 3],
        colors: [[0.0; 3]; 3],
        directions: [[0.0, 0.0, -1.0]; 3],
    };
    let mut trees = super::Trees::new([1000.0; 2], lighting);
    trees
        .buffer
        .add(0, original(), [500.0, 400.0, 0.0], 0.0, 1.0, 1);
    let mut world = World::new();
    let mut commands = CommandQueue::default();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<crate::tree_material::TreeMaterial>::default();
    let mut images = Assets::<Image>::default();
    trees.update_atlas(
        &mut Commands::new(&mut commands, &world),
        &mut meshes,
        &mut materials,
        &mut images,
    );
    commands.apply(&mut world);
    let entity = trees.mesh_entity.unwrap();
    assert!(world.get::<Mesh3d>(entity).is_none());
    assert!(meshes.is_empty());
    let mut retained = None;
    let mut first_positions = None;
    for visible in [true, false, true] {
        trees.buffer.trees[0].visible = visible;
        if visible && retained.is_some() {
            trees.buffer.trees[0].position[0] += 100.0;
        }
        let mesh = trees
            .buffer
            .mesh(trees.atlas.as_ref().unwrap(), &trees.lighting);
        trees.update_mesh(&mut Commands::new(&mut commands, &world), &mut meshes, mesh);
        commands.apply(&mut world);
        let handle = trees.mesh.as_ref().unwrap();
        assert_eq!(meshes.len(), 1);
        assert_eq!(trees.mesh_entity, Some(entity));
        let positions = meshes
            .get(handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()
            .iter()
            .map(|point| point.map(f32::to_bits))
            .collect::<Vec<_>>();
        assert!(!positions.is_empty());
        if let Some(id) = retained {
            assert_eq!(handle.id(), id);
            if visible {
                assert_ne!(first_positions.as_ref().unwrap(), &positions);
            } else {
                assert_eq!(first_positions.as_ref().unwrap(), &positions);
            }
        } else {
            retained = Some(handle.id());
            first_positions = Some(positions);
        }
        assert_eq!(world.get::<Mesh3d>(entity).is_some(), visible);
        if visible {
            assert_eq!(&world.get::<Mesh3d>(entity).unwrap().0, handle);
            assert_eq!(
                meshes.get(handle).unwrap().count_vertices(),
                trees.buffer.types[0].vertices.len()
            );
            assert_eq!(
                meshes.get(handle).unwrap().indices().unwrap().len(),
                trees.buffer.types[0].indices.len()
            );
        }
    }
}

#[test]
fn unchanged_tree_draw_avoids_material_events_without_stopping_sway() {
    use crate::tree_material::{Parameters, TreeMaterial};
    use bevy::{asset::AssetPlugin, prelude::*};

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<TreeMaterial>();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<TreeMaterial>>()
        .add(TreeMaterial {
            parameters: Parameters {
                sway: [Vec4::ZERO; 11],
                shroud_offset: Vec4::ZERO,
                shroud_scale: Vec4::ONE,
            },
            atlas: Handle::default(),
            shroud: None,
        });
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<TreeMaterial>>>()
        .clear();
    let mut buffer = Buffer::new([1000.0; 2]);
    let mut wind = breeze();
    let mut random_calls = 0;
    // A paused draw still initializes a changed breeze. An unchanged paused
    // draw emits nothing, while the following fractional step must advance.
    for (ratio, version, changed, calls) in [
        (0.0, 0, true, 20),
        (0.0, 0, false, 20),
        (0.25, 0, true, 20),
        (0.0, 1, true, 40),
        (0.0, 1, false, 40),
    ] {
        wind.version = version;
        let sway = buffer
            .draw(
                &wind,
                ratio,
                |low, high| {
                    random_calls += 1;
                    (low + high) * 0.5
                },
                |_| false,
            )
            .map(|value| Vec3::from_array(value).extend(0.0));
        super::update_material(
            &mut app.world_mut().resource_mut::<Assets<TreeMaterial>>(),
            &handle,
            sway,
            None,
            [0.0; 2],
            [1.0; 2],
        );
        app.update();
        let events: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<TreeMaterial>>>()
            .drain()
            .filter_map(|event| match event {
                AssetEvent::Modified { id } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(
            events,
            if changed {
                vec![handle.id()]
            } else {
                Vec::new()
            }
        );
        assert_eq!(random_calls, calls);
        let material = app
            .world()
            .resource::<Assets<TreeMaterial>>()
            .get(&handle)
            .unwrap();
        let bits = material
            .parameters
            .sway
            .map(|value| value.to_array().map(f32::to_bits));
        assert_eq!(bits, sway.map(|value| value.to_array().map(f32::to_bits)));
    }
}

#[test]
fn tree_material_keeps_fog_mapping_and_signed_zero_updates() {
    use crate::tree_material::{Parameters, TreeMaterial};
    use bevy::{asset::AssetPlugin, prelude::*};

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<TreeMaterial>();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<TreeMaterial>>()
        .add(TreeMaterial {
            parameters: Parameters {
                sway: [Vec4::ZERO; 11],
                shroud_offset: Vec4::ZERO,
                shroud_scale: Vec4::ONE,
            },
            atlas: Handle::default(),
            shroud: None,
        });
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<TreeMaterial>>>()
        .clear();
    // Fog identity/mapping changes must also reach the renderer, even when
    // sway is stationary; preserve signed-zero shader bytes as well.
    for (shroud, offset, scale) in [
        (Some(Handle::<Image>::default()), [0.0; 2], [1.0; 2]),
        (None, [0.5, 0.0], [1.0; 2]),
        (None, [0.5, 0.0], [2.0, 1.0]),
        (None, [0.5, -0.0], [2.0, 1.0]),
    ] {
        let sway = app
            .world()
            .resource::<Assets<TreeMaterial>>()
            .get(&handle)
            .unwrap()
            .parameters
            .sway;
        super::update_material(
            &mut app.world_mut().resource_mut::<Assets<TreeMaterial>>(),
            &handle,
            sway,
            shroud.clone(),
            offset,
            scale,
        );
        app.update();
        let modified: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<TreeMaterial>>>()
            .drain()
            .filter_map(|event| match event {
                AssetEvent::Modified { id } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(modified, [handle.id()]);
        let material = app
            .world()
            .resource::<Assets<TreeMaterial>>()
            .get(&handle)
            .unwrap();
        assert_eq!(material.shroud, shroud);
        assert_eq!(
            material
                .parameters
                .shroud_offset
                .to_array()
                .map(f32::to_bits),
            [offset[0], offset[1], 0.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(
            material
                .parameters
                .shroud_scale
                .to_array()
                .map(f32::to_bits),
            [scale[0], scale[1], 0.0, 0.0].map(f32::to_bits)
        );
    }
}
