//! Optimized `W3DTreeBuffer`, SH 2bfaea60. Its instances are not simulator Objects.
mod atlas;
pub mod data;
mod mesh;
pub mod shadows;
mod sphere;
mod state;
#[cfg(test)]
mod tests;
pub mod verification;
use crate::{
    model_animation::Animations,
    particles::Particles,
    playtest::Match,
    playtest_view::{WorldCamera, WorldObjects},
    tree_material::{Parameters, TreeMaterial},
    visibility_view::Fog,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::{
        primitives::{Frustum, Sphere},
        visibility::{NoFrustumCulling, RenderLayers},
    },
    ecs::system::SystemParam,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    math::Vec3A,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use generals_sim::skirmish::{terrain_movement::Event, visibility::Status};
pub use mesh::Lighting;
use num_traits::ToPrimitive;
use std::sync::Arc;

#[derive(Resource)]
pub struct Trees {
    generation: Option<u32>,
    buffer: state::Buffer,
    bounds: [f32; 2],
    lighting: Lighting,
    clear_cursor: usize,
    atlas: Option<atlas::Atlas>,
    atlas_types: usize,
    mesh: Option<Handle<Mesh>>,
    mesh_entity: Option<Entity>,
    material: Option<Handle<TreeMaterial>>,
}

impl Trees {
    pub fn new(bounds: [f32; 2], lighting: Lighting) -> Self {
        Self {
            generation: None,
            buffer: state::Buffer::new(bounds),
            bounds,
            lighting,
            clear_cursor: 0,
            atlas: None,
            atlas_types: 0,
            mesh: None,
            mesh_entity: None,
            material: None,
        }
    }
}

impl Trees {
    fn reset(&mut self, generation: u32) {
        if self.generation != Some(generation) {
            self.buffer = state::Buffer::new(self.bounds);
            self.clear_cursor = 0;
            self.atlas_types = usize::MAX;
            self.generation = Some(generation);
        }
    }

    pub fn record_event(
        &mut self,
        battle: &Match,
        objects: &WorldObjects,
        index: usize,
        animations: &mut Animations,
        particles: &mut Particles,
    ) {
        self.reset(battle.generation);
        let trees = self;
        let event = &battle.simulation.presentation.terrain_movement_events[index];
        let clear_region = match event {
            Event::TreeAdded(index) => {
                let object = objects
                    .0
                    .iter()
                    .find(|object| object.tree_data.as_ref().is_some_and(|(id, _)| id == index))
                    .expect("original ordered tree placement");
                let (_, data) = object.tree_data.as_ref().expect("tree placement data");
                // Native bootstrap forces scale randomness to zero. Equal-bound
                // client real calls retain their native no-draw contract.
                if trees.buffer.trees.len() >= 4000 {
                    return;
                }
                animations.effect_real(battle.generation, 1.0, 1.0);
                let sway = usize::try_from(animations.effect_integer(battle.generation, [1, 10]))
                    .expect("ten native sway types");
                let mut angle = object.angle;
                let turn = 2.0 * std::f32::consts::PI;
                while angle.partial_cmp(&std::f32::consts::PI) == Some(std::cmp::Ordering::Greater)
                {
                    angle -= turn;
                }
                while matches!(
                    angle.partial_cmp(&-std::f32::consts::PI),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                ) {
                    angle += turn;
                }
                trees.buffer.add(
                    *index,
                    Arc::clone(data),
                    object.position,
                    angle,
                    object.scale,
                    sway,
                );
                None
            }
            Event::Moved {
                frame,
                object,
                position,
                direction,
                geometry,
                crusher_level,
                clear_region,
            } => {
                trees.buffer.moved(
                    *frame,
                    *object,
                    *position,
                    *direction,
                    geometry,
                    *crusher_level,
                );
                Some(clear_region)
            }
        };
        flush_effects(
            &mut trees.buffer,
            battle,
            particles,
            animations,
            clear_region,
        );
    }
}

pub fn clear(battle: Res<Match>, objects: Res<WorldObjects>, mut trees: ResMut<Trees>) {
    let battle = battle.into_inner();
    let objects = objects.into_inner();
    let trees = trees.as_mut();
    trees.reset(battle.generation);
    for clear in &battle.simulation.construction_clears[trees.clear_cursor..] {
        for tree in &mut trees.buffer.trees {
            if tree.kind.is_none() {
                continue;
            }
            let object = objects
                .0
                .iter()
                .find(|object| {
                    object
                        .tree_data
                        .as_ref()
                        .is_some_and(|(id, _)| *id == tree.placement)
                })
                .expect("tree placement");
            if crate::scenery_clear::collides(clear, object) {
                tree.kind = None;
                trees.buffer.changed = true;
            }
        }
    }
    trees.clear_cursor = battle.simulation.construction_clears.len();
}

fn flush_effects(
    buffer: &mut state::Buffer,
    battle: &Match,
    particles: &mut Particles,
    animations: &mut Animations,
    region: Option<&generals_sim::skirmish::visibility::ClearRegion>,
) {
    for effect in buffer.effects.drain(..) {
        let viewer = battle.observer_player.unwrap_or(0);
        let point = [effect.position[0], effect.position[1]];
        let clear = region.map_or_else(
            || {
                matches!(
                    battle.simulation.visibility_cell(viewer, point),
                    Status::Clear
                )
            },
            |region| region.is_clear(viewer, point),
        );
        particles.tree_position_event(
            &battle.simulation,
            battle.generation,
            &effect.name,
            effect.position,
            clear,
            animations,
        );
    }
}

#[derive(SystemParam)]
pub struct Draw<'w, 's> {
    commands: Commands<'w, 's>,
    battle: Res<'w, Match>,
    trees: ResMut<'w, Trees>,
    animations: ResMut<'w, Animations>,
    particles: ResMut<'w, Particles>,
    audio: ResMut<'w, crate::source_audio::SourceAudio>,
    fixed: Res<'w, Time<Fixed>>,
    time: Res<'w, Time<Virtual>>,
    camera: Query<'w, 's, &'static Frustum, With<WorldCamera>>,
    fog: Res<'w, Fog>,
    terrain: Res<'w, crate::terrain::Terrain>,
    shadows: ResMut<'w, shadows::Shadows>,
    source_materials: ResMut<'w, Assets<crate::source_material::SourceMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<TreeMaterial>>,
    images: ResMut<'w, Assets<Image>>,
}

pub fn draw(input: Draw) {
    let Draw {
        mut commands,
        battle,
        mut trees,
        mut animations,
        mut particles,
        mut audio,
        fixed,
        time,
        camera,
        fog,
        terrain,
        mut shadows,
        mut source_materials,
        mut meshes,
        mut materials,
        mut images,
    } = input;
    let Ok(frustum) = camera.single() else {
        return;
    };
    let trees = trees.as_mut();
    trees.cull(frustum);
    // W3DTreeBuffer queues/flushes decals after culling, before updating topples.
    shadows.draw(
        &trees.buffer,
        &terrain,
        &mut commands,
        &mut meshes,
        &mut source_materials,
        &mut images,
    );
    let ratio = if !battle.started || battle.paused || time.is_paused() {
        0.0
    } else {
        (time.delta_secs() / fixed.timestep().as_secs_f32()).min(1.0)
    };
    let sway = trees.buffer.draw(
        &battle.simulation.breeze(),
        ratio,
        |low, high| animations.effect_real(battle.generation, low, high),
        |point| {
            prop_fogged(
                &battle.simulation,
                battle.observer_player.unwrap_or(0),
                point,
            )
        },
    );
    flush_effects(
        &mut trees.buffer,
        &battle,
        &mut particles,
        &mut animations,
        None,
    );
    particles.flush_effect_sounds(&mut audio);
    trees.update_atlas(&mut commands, &mut meshes, &mut materials, &mut images);
    let Some(atlas) = &trees.atlas else {
        return;
    };
    if trees.buffer.needs_vertex_update() {
        let mesh = trees.buffer.mesh(atlas, &trees.lighting);
        trees.update_mesh(&mut commands, &mut meshes, mesh);
        trees.buffer.changed = false;
        trees.buffer.pushed = false;
    }
    let Some(handle) = &trees.material else {
        return;
    };
    let sway = sway.map(|value| Vec3::from_array(value).extend(0.0));
    let shroud = fog.texture().cloned();
    let (offset, scale) = fog.tree_mapping();
    update_material(&mut materials, handle, sway, shroud, offset, scale);
}

fn update_material(
    materials: &mut Assets<TreeMaterial>,
    handle: &Handle<TreeMaterial>,
    sway: [Vec4; 11],
    shroud: Option<Handle<Image>>,
    offset: [f32; 2],
    scale: [f32; 2],
) {
    let offset = Vec4::new(offset[0], offset[1], 0.0, 0.0);
    let scale = Vec4::new(scale[0], scale[1], 0.0, 0.0);
    let current = materials.get(handle).expect("tree material");
    // Preserve the shader payload, including signed zero, without re-preparing
    // an unchanged material during a paused or unchanged terrain draw.
    if current
        .parameters
        .sway
        .map(|value| value.to_array().map(f32::to_bits))
        == sway.map(|value| value.to_array().map(f32::to_bits))
        && current.shroud == shroud
        && current
            .parameters
            .shroud_offset
            .to_array()
            .map(f32::to_bits)
            == offset.to_array().map(f32::to_bits)
        && current.parameters.shroud_scale.to_array().map(f32::to_bits)
            == scale.to_array().map(f32::to_bits)
    {
        return;
    }
    let mut material = materials.get_mut(handle).expect("tree material");
    material.parameters.sway = sway;
    material.shroud = shroud;
    material.parameters.shroud_offset = offset;
    material.parameters.shroud_scale = scale;
}

impl Trees {
    fn cull(&mut self, frustum: &Frustum) {
        for tree in &mut self.buffer.trees {
            let visible = tree.kind.is_some()
                && frustum.intersects_sphere(
                    &Sphere {
                        center: Vec3A::new(tree.center[0], tree.center[2], -tree.center[1]) * 0.1,
                        radius: tree.radius * 0.1,
                    },
                    true,
                );
            if visible != tree.visible {
                self.buffer.changed = true;
                tree.visible = visible;
            }
        }
    }

    // Native drawTrees retains buffers but issues no draw for an empty batch.
    fn update_mesh(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>, mesh: Mesh) {
        let entity = self.mesh_entity.expect("tree aggregate entity");
        if mesh.count_vertices() == 0 || mesh.indices().is_none_or(bevy::mesh::Indices::is_empty) {
            commands.entity(entity).remove::<Mesh3d>();
            return;
        }
        let handle = if let Some(handle) = &self.mesh {
            *meshes.get_mut(handle).expect("tree mesh") = mesh;
            handle.clone()
        } else {
            let handle = meshes.add(mesh);
            self.mesh = Some(handle.clone());
            handle
        };
        commands.entity(entity).insert(Mesh3d(handle));
    }

    fn update_atlas(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<TreeMaterial>,
        images: &mut Assets<Image>,
    ) {
        if self.atlas_types != self.buffer.types.len() {
            let atlas = atlas::Atlas::new(&self.buffer.types).expect("admitted native tree atlas");
            let image = atlas_image(&atlas);
            let handle = images.add(image);
            if let Some(material) = &self.material {
                materials.get_mut(material).expect("tree material").atlas = handle;
            } else {
                let material = materials.add(TreeMaterial {
                    parameters: Parameters {
                        sway: [Vec4::ZERO; 11],
                        shroud_offset: Vec4::ZERO,
                        shroud_scale: Vec4::ZERO,
                    },
                    atlas: handle,
                    shroud: None,
                });
                let entity = commands
                    .spawn((
                        MeshMaterial3d(material.clone()),
                        Transform::IDENTITY,
                        Visibility::Visible,
                        NoFrustumCulling,
                        RenderLayers::layer(13),
                    ))
                    .id();
                self.material = Some(material);
                self.mesh_entity = Some(entity);
                self.update_mesh(commands, meshes, self.buffer.mesh(&atlas, &self.lighting));
            }
            self.atlas_types = self.buffer.types.len();
            self.atlas = Some(atlas);
        }
    }
}

fn atlas_image(atlas: &atlas::Atlas) -> Image {
    let extent = u32::try_from(atlas.extent).expect("2048 tree texture");
    let mut image = Image::new_uninit(
        Extent3d {
            width: extent,
            height: extent,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = atlas.extent.ilog2() + 1;
    image.data = Some(atlas_mips(atlas));
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..Default::default()
    });
    image
}

// PartitionManager::getPropShroudStatusForPlayer samples the four neighboring
// cells centered around the prop. Mixed statuses are PARTIAL_CLEAR, not FOGGED.
fn prop_fogged(game: &generals_sim::skirmish::Skirmish, viewer: usize, point: [f32; 3]) -> bool {
    let cell = game.visibility_cell_size();
    let inverse = 1.0 / cell;
    let base = std::array::from_fn::<_, 2, _>(|axis| {
        let half_cell = cell * 0.5;
        generals_sim::native_real::floor((point[axis] - half_cell) * inverse)
    });
    [[0, 0], [1, 0], [1, 1], [0, 1]].into_iter().all(|offset| {
        let point = std::array::from_fn(|axis| {
            ((base[axis] + offset[axis])
                .to_f32()
                .expect("native shroud cell")
                + 0.5)
                * cell
        });
        matches!(game.visibility_cell(viewer, point), Status::Fogged)
    })
}

pub fn effect_names(objects: &WorldObjects) -> std::collections::BTreeSet<String> {
    objects
        .0
        .iter()
        .filter_map(|object| object.tree_data.as_ref())
        .flat_map(|(_, data)| [&data.topple_fx, &data.bounce_fx])
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("NONE"))
        .cloned()
        .collect()
}

// D3DX_FILTER_BOX recursively averages each 2x2 source block. Quantize each
// resulting RGBA8 level before filtering it again; do not average from level0.
fn atlas_mips(atlas: &atlas::Atlas) -> Vec<u8> {
    let mut output = atlas.rgba.clone();
    let mut previous = atlas.rgba.clone();
    let mut width = atlas.extent;
    while width > 1 {
        let half = width / 2;
        let mut next = vec![0; half * half * 4];
        for y in 0..half {
            for x in 0..half {
                for channel in 0..4 {
                    let base = ((2 * y) * width + 2 * x) * 4 + channel;
                    let sum = u16::from(previous[base])
                        + u16::from(previous[base + 4])
                        + u16::from(previous[base + width * 4])
                        + u16::from(previous[base + width * 4 + 4]);
                    next[(y * half + x) * 4 + channel] =
                        u8::try_from((sum + 2) / 4).expect("RGBA8 box filter");
                }
            }
        }
        output.extend_from_slice(&next);
        previous = next;
        width = half;
    }
    output
}
