#[cfg(test)]
pub mod animation_fixture;
pub mod crate_verification;
#[cfg(test)]
mod lifetime_tests;
#[cfg(test)]
mod placed_height_tests;
mod placement_preview;
mod prepared_materials;
pub mod prepared_meshes;
pub use placement_preview::{Placement, update as placement};

use crate::{
    model_render,
    playtest::{CameraRig, Match},
    source_material::SourceMaterial,
    source_texture,
    terrain::Terrain,
};
use bevy::{
    app::AppExit,
    camera::{Camera3dDepthLoadOp, CameraOutputMode, visibility::RenderLayers},
    core_pipeline::tonemapping::Tonemapping,
    image::{ImageAddressMode, ImageSampler},
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems,
        render_resource::{BlendState, CachedPipelineState, PipelineCache},
        view::{
            NoIndirectDrawing,
            screenshot::{Screenshot, ScreenshotCaptured},
            window::ExtractedWindows,
        },
    },
    shader::ShaderCacheError,
};
use generals_assets::{content, virtual_assets::VirtualAssets, w3d::Model};
use std::{
    collections::BTreeMap,
    error::Error,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Resource)]
pub struct Models {
    parts: Vec<Vec<model_render::Part>>,
    terrain: Vec<model_render::Part>,
    roads: Vec<model_render::Part>,
    tree_lighting: Lighting,
    textures: BTreeMap<(String, u16), Image>,
    placement_opacity: f32,
    colors: Vec<[f32; 3]>,
    owner_textures: BTreeMap<(String, u16), Vec<Image>>,
}

pub struct WorldObject {
    pub terrain_bridge: bool,
    pub animation: Option<Arc<crate::model_animation::WorldDraw>>,
    pub model_index: usize,
    pub position: [f32; 3],
    pub angle: f32,
    pub scale: f32,
    pub tree: bool,
    pub tree_data: Option<(usize, Arc<crate::tree_buffer::data::Data>)>,
    pub prop_radius: Option<f32>,
    pub supply: Option<usize>,
    pub supply_draw: Option<crate::supply_presentation::SupplyDraw>,
}

#[derive(Component)]
pub struct SupplyVisual(pub usize);

#[derive(Resource)]
pub struct WorldObjects(pub Vec<WorldObject>);

struct Lighting {
    ambient: [f32; 3],
    lights: Vec<([f32; 3], [f32; 3])>,
}

impl Models {
    pub(crate) fn load_roads(
        &mut self,
        assets: &VirtualAssets,
        records: &[generals_assets::map::MapObject],
        terrain: &Terrain,
    ) -> Result<(), Box<dyn Error>> {
        let lighting = &self
            .terrain
            .first()
            .ok_or("Missing terrain lighting")?
            .material
            .parameters;
        let mut roads = crate::road_view::load(assets, records, terrain)?;
        for part in &mut roads {
            part.material.parameters.shroud_mode = 1;
            part.material.parameters.scene_ambient = lighting.scene_ambient;
            part.material.parameters.light_color = lighting.light_color;
            part.material.parameters.light_direction = lighting.light_direction;
            if let Some(key) = &part.texture
                && !self.textures.contains_key(key)
            {
                let mut image = source_texture::load(assets, &key.0)?;
                image.texture_descriptor.format =
                    image.texture_descriptor.format.remove_srgb_suffix();
                // RoadType requests MIP_LEVELS_3. The DDS chain remains shared
                // storage; the sampler limits the effective original levels.
                if let ImageSampler::Descriptor(sampler) = &mut image.sampler {
                    sampler.lod_max_clamp = 2.0;
                }
                self.textures.insert(key.clone(), image);
            }
        }
        self.roads = roads;
        Ok(())
    }

    pub(crate) fn water_lighting(&self) -> [f32; 3] {
        let material = &self
            .terrain
            .first()
            .expect("loaded terrain material")
            .material
            .parameters;
        let mut color = material.scene_ambient.truncate();
        for (direction, light) in material.light_direction.iter().zip(&material.light_color) {
            color += light.truncate() * (-direction.y).max(0.0);
        }
        color.to_array()
    }

    pub(crate) fn track_color(&self) -> [f32; 3] {
        let terrain = &self
            .terrain
            .first()
            .expect("loaded terrain material")
            .material
            .parameters;
        let color = terrain.scene_ambient.truncate() + terrain.light_color[0].truncate() * 0.5;
        color
            .to_array()
            .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).trunc() / 255.0)
    }

    pub(crate) fn scorch_color(&self) -> [f32; 4] {
        let material = &self
            .terrain
            .first()
            .expect("loaded terrain material")
            .material
            .parameters;
        let color = (material.scene_ambient.truncate() + material.light_color[0].truncate()) / 2.0;
        let color = color
            .to_array()
            .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).trunc() / 255.0);
        [color[0], color[1], color[2], 1.0]
    }

    pub fn load(
        assets: &VirtualAssets,
        models: &[Model],
        time_of_day: &str,
        terrain: &Terrain,
        map_lighting: Option<&generals_assets::map::MapLighting>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut models = models
            .iter()
            .map(model_render::parts)
            .collect::<Result<Vec<_>, _>>()?;
        let mut terrain = terrain_parts(assets, terrain)?;
        let object_lighting = lighting(assets, time_of_day, "TerrainObjectsLighting")?;
        let terrain_lighting = lighting(assets, time_of_day, "TerrainLighting")?;
        for part in models.iter_mut().flatten() {
            apply_lighting(&mut part.material, &object_lighting);
        }
        for part in &mut terrain {
            apply_lighting(&mut part.material, &terrain_lighting);
        }
        let mut textures = BTreeMap::new();
        let mut colors = player_colors(assets)?;
        // Player's neutral constructor color is also passed to W3DDebrisDraw.
        if !colors
            .iter()
            .any(|color| color.map(f32::to_bits) == [1.0_f32.to_bits(); 3])
        {
            colors.push([1.0; 3]);
        }
        let mut owner_textures = BTreeMap::new();
        for part in models.iter().flatten().chain(&terrain) {
            if let Some(key) = &part.texture
                && !textures.contains_key(key)
            {
                let mut image = if let Some(extent) = part.texture_extent {
                    source_texture::load_terrain(assets, &key.0, extent)?
                } else {
                    source_texture::load_mesh(assets, &key.0)?
                };
                image.texture_descriptor.format =
                    image.texture_descriptor.format.remove_srgb_suffix();
                if let ImageSampler::Descriptor(sampler) = &mut image.sampler {
                    if key.1 & 8 != 0 {
                        sampler.address_mode_u = ImageAddressMode::ClampToEdge;
                    }
                    if key.1 & 16 != 0 {
                        sampler.address_mode_v = ImageAddressMode::ClampToEdge;
                    }
                }
                let name = key
                    .0
                    .rsplit('/')
                    .next()
                    .unwrap_or(&key.0)
                    .to_ascii_lowercase();
                if name.starts_with("zhca") || name.starts_with("zhcd") {
                    let recolored = colors
                        .iter()
                        .map(|&color| {
                            source_texture::recolor(&image, color, name.starts_with("zhcd"))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    owner_textures.insert(key.clone(), recolored);
                }
                textures.insert(key.clone(), image);
            }
        }
        let mut models = Self {
            parts: models,
            terrain,
            roads: Vec::new(),
            tree_lighting: object_lighting,
            textures,
            placement_opacity: placement_opacity(assets)?,
            colors,
            owner_textures,
        };
        if let Some(lighting) = map_lighting {
            models.set_lighting(lighting)?;
        }
        Ok(models)
    }
    fn set_lighting(
        &mut self,
        map: &generals_assets::map::MapLighting,
    ) -> Result<(), Box<dyn Error>> {
        let active = map.active().ok_or("invalid map TimeOfDay")?;
        let convert = |lights: &[generals_assets::map::MapLight; 3]| Lighting {
            ambient: lights[0].ambient,
            lights: lights
                .iter()
                .map(|light| (light.diffuse, light.position))
                .collect(),
        };
        let objects = convert(&active.objects);
        let terrain = convert(&active.terrain);
        for part in self.parts.iter_mut().flatten() {
            apply_lighting(&mut part.material, &objects);
        }
        for part in &mut self.terrain {
            apply_lighting(&mut part.material, &terrain);
        }
        self.tree_lighting = objects;
        Ok(())
    }
}

pub fn player_colors(assets: &VirtualAssets) -> Result<Vec<[f32; 3]>, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/multiplayer.ini")?)?;
    let colors = file
        .declarations
        .iter()
        .filter(|item| item.kind == "MultiplayerColor")
        .map(|item| {
            let line = item
                .lines
                .iter()
                .rev()
                .find(|line| line.tokens.first().is_some_and(|field| field == "RGBColor"))
                .ok_or("player color lacks RGBColor")?;
            let mut rgb = [0.0; 3];
            for (index, prefix) in ["R:", "G:", "B:"].iter().enumerate() {
                let value = line
                    .tokens
                    .iter()
                    .find_map(|token| token.strip_prefix(prefix))
                    .ok_or("player color channel missing")?
                    .parse::<u8>()?;
                rgb[index] = f32::from(value) / 255.0;
            }
            Ok(rgb)
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    if colors.len() != 8 {
        return Err("Expected eight original player colors".into());
    }
    Ok(colors)
}

fn placement_opacity(assets: &VirtualAssets) -> Result<f32, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let value = file
        .declarations
        .iter()
        .filter(|item| item.kind == "GameData")
        .flat_map(|item| &item.lines)
        .rev()
        .find(|line| {
            line.tokens
                .first()
                .is_some_and(|name| name == "ObjectPlacementOpacity")
        })
        .map(|line| {
            line.tokens
                .get(1)
                .ok_or("ObjectPlacementOpacity missing value")
        })
        .transpose()?;
    // GlobalData::GlobalData initializes this field to 0.45 when INI omits it.
    let opacity = value.map_or(Ok(0.45), |value| value.parse::<f32>())?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err("invalid ObjectPlacementOpacity".into());
    }
    Ok(opacity)
}

fn terrain_parts(
    assets: &VirtualAssets,
    terrain: &Terrain,
) -> Result<Vec<model_render::Part>, Box<dyn Error>> {
    let definitions = content::scan(assets.read("data/ini/terrain.ini")?)?;
    terrain
        .batches()
        .into_iter()
        .map(|batch| {
            let definition = definitions
                .declarations
                .iter()
                .find(|entry| entry.kind == "Terrain" && entry.name == batch.texture_class)
                .ok_or_else(|| format!("missing Terrain {}", batch.texture_class))?;
            let texture = definition
                .lines
                .iter()
                .find(|line| line.tokens.first().is_some_and(|token| token == "Texture"))
                .and_then(|line| line.tokens.get(1))
                .ok_or("terrain class lacks Texture")?;
            Ok(model_render::Part {
                light_bounds: None,
                submission: None,
                order: model_render::RenderOrder::ordinary(),
                mesh: batch.mesh,
                texture_extent: Some(batch.pixel_extent),
                house_color: false,
                pickable: false,
                binding: None,
                texture: Some((format!("art/terrain/{texture}").to_ascii_lowercase(), 0)),
                material: {
                    let mut material = SourceMaterial::new(
                        [1.0; 3],
                        [1.0; 3],
                        [0.0; 3],
                        1.0,
                        if batch.layer == 0 {
                            AlphaMode::Opaque
                        } else {
                            AlphaMode::Blend
                        },
                        false,
                    );
                    material.depth_bias = f32::from(batch.layer);
                    material.parameters.shroud_mode = 1;
                    material
                },
            })
        })
        .collect()
}

fn lighting(
    assets: &VirtualAssets,
    time_of_day: &str,
    kind: &str,
) -> Result<Lighting, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let definition = file
        .declarations
        .iter()
        .find(|entry| entry.kind == "GameData")
        .ok_or("missing GameData lighting")?;
    let name = match time_of_day.to_ascii_uppercase().as_str() {
        "MORNING" => "Morning",
        "AFTERNOON" => "Afternoon",
        "EVENING" => "Evening",
        "NIGHT" => "Night",
        _ => return Err("unsupported source TimeOfDay".into()),
    };
    let vector =
        |field: &str, labels: [&str; 3], divisor: f32| -> Result<[f32; 3], Box<dyn Error>> {
            let line = definition
                .lines
                .iter()
                .find(|line| line.tokens.first().is_some_and(|key| key == field))
                .ok_or_else(|| format!("missing {field}"))?;
            let mut result = [0.0; 3];
            for (index, label) in labels.iter().enumerate() {
                let token = line
                    .tokens
                    .iter()
                    .find_map(|token| token.strip_prefix(label))
                    .ok_or("missing lighting component")?;
                result[index] = token.parse::<f32>()? / divisor;
                if !result[index].is_finite() {
                    return Err("nonfinite lighting component".into());
                }
            }
            Ok(result)
        };
    let mut ambient = [0.0; 3];
    let mut lights = Vec::new();
    for suffix in ["", "2", "3"] {
        let prefix = format!("{kind}{name}");
        let local = vector(
            &format!("{prefix}Ambient{suffix}"),
            ["R:", "G:", "B:"],
            255.0,
        )?;
        if suffix.is_empty() {
            ambient = local;
        }
        lights.push((
            vector(
                &format!("{prefix}Diffuse{suffix}"),
                ["R:", "G:", "B:"],
                255.0,
            )?,
            vector(
                &format!("{prefix}LightPos{suffix}"),
                ["X:", "Y:", "Z:"],
                1.0,
            )?,
        ));
    }
    Ok(Lighting { ambient, lights })
}

#[derive(Resource)]
pub struct Capture {
    stage: CaptureStage,
    frames: u16,
    output: PathBuf,
    intermediate: Option<&'static str>,
    pipelines_ready: bool,
    pipeline_error: Option<String>,
    started_at: Option<f32>,
}

enum CaptureStage {
    Disabled,
    Waiting,
    Rendering,
    Verified,
    Pending,
}

impl Capture {
    fn inspect_pipelines<'a>(
        &mut self,
        waiting: bool,
        mut states: impl Iterator<Item = &'a CachedPipelineState>,
    ) {
        if self.pipeline_error.is_none() {
            self.pipeline_error = states.find_map(pipeline_error);
        }
        // Failed pipelines leave Bevy's waiting queue. An empty queue alone
        // cannot establish that the geometry was actually drawable.
        self.pipelines_ready = !waiting && self.pipeline_error.is_none();
    }

    pub const fn is_pending(&self) -> bool {
        !matches!(self.stage, CaptureStage::Disabled)
    }

    pub const fn pipelines_ready(&self) -> bool {
        self.pipelines_ready
    }

    pub const fn wait_for_scenario(&mut self) {
        self.stage = CaptureStage::Waiting;
        self.started_at = None;
    }

    pub const fn finish_scenario(&mut self) {
        // The scenario already checked its live geometry/visibility. It needs
        // a compiled drawable frame, not an unrelated consecutive-frame delay.
        self.stage = CaptureStage::Verified;
    }

    pub const fn intermediate(&mut self, filename: &'static str) {
        self.intermediate = Some(filename);
        self.started_at = None;
        self.stage = CaptureStage::Verified;
    }

    pub const fn ready_for_feedback(&self) -> bool {
        matches!(self.stage, CaptureStage::Rendering) && self.frames >= 80
    }

    pub const fn ready_for_scenario(&self) -> bool {
        matches!(self.stage, CaptureStage::Waiting) && self.pipelines_ready
    }

    fn request_ready(&mut self, surface: &mut SurfaceFrame) -> bool {
        if !matches!(self.stage, CaptureStage::Rendering | CaptureStage::Verified) {
            return false;
        }
        if !self.pipelines_ready || !surface.available {
            if matches!(self.stage, CaptureStage::Rendering) {
                self.frames = 0;
            }
            return false;
        }
        if matches!(self.stage, CaptureStage::Rendering) {
            // Existing feedback/supply scenarios issue commands at frame 80.
            // Preserve their handshake until they report explicit completion.
            self.frames = self.frames.saturating_add(1);
            if self.frames < 90 {
                return false;
            }
        }
        self.stage = CaptureStage::Pending;
        surface.captured = None;
        surface.pipeline_error = None;
        surface.requested = false;
        true
    }

    fn complete_frame(&mut self, surface: &mut SurfaceFrame) -> bool {
        if self.pipeline_error.is_none() {
            self.pipeline_error = surface.pipeline_error.take();
        }
        self.pipelines_ready &= self.pipeline_error.is_none();
        let accepted = surface.captured.take() == Some(true) && self.pipeline_error.is_none();
        surface.requested = false;
        self.stage = if accepted && self.intermediate.is_some() {
            CaptureStage::Waiting
        } else if accepted {
            CaptureStage::Disabled
        } else {
            CaptureStage::Verified
        };
        // Keep started_at: retries share the original bounded deadline.
        accepted
    }
}

// Both Bevy worlds share the captured frame's surface status. Reading the next
// frame's status would race the asynchronous GPU readback.
#[derive(Resource, Clone)]
pub struct CaptureSurface(Arc<Mutex<SurfaceFrame>>);

impl CaptureSurface {
    pub fn available(&self) -> bool {
        self.0.lock().expect("capture surface lock").available
    }

    pub fn saved(&self) -> bool {
        self.0.lock().expect("capture surface lock").saved
    }
}

struct SurfaceFrame {
    saved: bool,
    available: bool,
    requested: bool,
    captured: Option<bool>,
    pipeline_error: Option<String>,
}

fn pipeline_error(state: &CachedPipelineState) -> Option<String> {
    match state {
        CachedPipelineState::Err(
            error @ (ShaderCacheError::ProcessShaderError(_)
            | ShaderCacheError::CreateShaderModule(_)),
        ) => Some(error.to_string()),
        _ => None,
    }
}

#[derive(Resource)]
pub struct Geometry {
    colors: Vec<[u32; 3]>,
    models: Vec<Vec<PreparedPart>>,
    generation: u32,
    spawned: usize,
}

struct PreparedPart {
    light_bounds: Option<crate::light_environment::Bounds>,
    submission: Option<crate::sorted_view::submission::ModelSubmission>,
    order: model_render::RenderOrder,
    binding: Option<Arc<model_render::PartBinding>>,
    pickable: bool,
    mesh: Handle<Mesh>,
    // Unowned drawable followed by each original house color.
    materials: Vec<Handle<SourceMaterial>>,
}

impl Geometry {
    pub(crate) fn packed_color_index(&self, color: u32) -> Option<usize> {
        let rgb = [16, 8, 0].map(|shift| {
            f32::from(u8::try_from((color >> shift) & 255).expect("packed channel")) / 255.0
        });
        self.colors
            .iter()
            .position(|entry| *entry == rgb.map(f32::to_bits))
    }

    pub fn spawn_unit_parts(
        &self,
        commands: &mut Commands,
        entity: Entity,
        model: usize,
        owner: Option<usize>,
        meshes: &mut Assets<Mesh>,
        frame: &crate::model_animation::Frame,
    ) {
        let instance = crate::sorted_view::RenderInstance::new();
        commands.entity(entity).with_children(|parent| {
            for part in &self.models[model] {
                let mut visible = true;
                let mesh = if let Some(pose) = &frame.meshes {
                    let Some(mut mesh) = meshes.get(&part.mesh).cloned() else {
                        continue;
                    };
                    if let Some(binding) = &part.binding {
                        match model_render::deform(&mut mesh, binding, pose) {
                            Ok(value) => visible = value,
                            Err(error) => {
                                error!("Original animation binding: {error}");
                                continue;
                            }
                        }
                    }
                    meshes.add(mesh)
                } else {
                    part.mesh.clone()
                };
                if let Some((_, boxes, draw)) = &frame.supply
                    && let Some(binding) = &part.binding
                {
                    visible &= draw.visible(binding.mesh, *boxes);
                }
                let mut child = parent.spawn((
                    instance.clone(),
                    Mesh3d(mesh),
                    Transform::from_translation(Vec3::Y * (frame.height_offset * 0.1)),
                    if visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    MeshMaterial3d(part.materials[owner.map_or(0, |owner| owner + 1)].clone()),
                    part.order,
                    RenderLayers::layer(part.order.layer(1.0)),
                ));
                if let Some(mut bounds) = part.light_bounds {
                    bounds.native_height = frame.height_offset;
                    child.insert(bounds);
                }
                if part.pickable {
                    child.insert(model_render::Pickable);
                }
                if let Some(submission) = &part.submission {
                    let mut submission = submission.clone();
                    submission.color = owner.map_or([0; 3], |owner| self.colors[owner]);
                    if let Some(pose) = &frame.meshes {
                        submission.set_pose(&pose[submission.mesh_order]);
                    }
                    child.insert(submission);
                }
                if let Some((source, _, draw)) = &frame.supply
                    && let Some(binding) = &part.binding
                {
                    crate::supply_presentation::attach(&mut child, *source, draw, binding.mesh);
                }
                if let Some(binding) = &part.binding {
                    child.insert(crate::model_animation::AnimatedPart {
                        model,
                        binding: Arc::clone(binding),
                    });
                }
            }
        });
    }
}

#[derive(Component)]
pub struct UnitVisual {
    pub index: usize,
    pub generation: u32,
}

#[derive(Component)]
pub struct WorldCamera;

#[derive(Component)]
pub struct ScenePassCamera;

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut source_materials: ResMut<Assets<SourceMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut models: ResMut<Models>,
    objects: Res<WorldObjects>,
    terrain: Res<Terrain>,
) {
    let objects = objects.into_inner();
    let terrain = terrain.into_inner();
    let textures: BTreeMap<_, _> = std::mem::take(&mut models.textures)
        .into_iter()
        .map(|(key, image)| (key, images.add(image)))
        .collect();
    prepare_tree_materials(&mut models, objects);
    let lighting = &models.tree_lighting;
    let tree_lighting = crate::tree_buffer::Lighting {
        ambient: lighting.ambient,
        colors: std::array::from_fn(|index| lighting.lights[index].0),
        directions: std::array::from_fn(|index| lighting.lights[index].1),
    };
    commands.insert_resource(crate::tree_buffer::Trees::new(
        terrain.tree_bounds(),
        tree_lighting,
    ));
    let model_parts = std::mem::take(&mut models.parts);
    let static_levels: std::collections::BTreeSet<_> = model_parts
        .iter()
        .flatten()
        .filter_map(|part| part.order.static_level())
        .collect();
    let terrain_parts = std::mem::take(&mut models.terrain);
    let road_parts = std::mem::take(&mut models.roads);
    let owner_textures: BTreeMap<_, _> = std::mem::take(&mut models.owner_textures)
        .into_iter()
        .map(|(key, pair)| {
            (
                key,
                pair.into_iter()
                    .map(|image| images.add(image))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let colors = &models.colors;
    let mut prepared_materials = prepared_materials::Cache::new();
    let mut prepare = |part| {
        prepare_part(
            part,
            &textures,
            &owner_textures,
            colors,
            &mut meshes,
            &mut source_materials,
            &mut prepared_materials,
        )
    };

    let mut shroud_meshes = Vec::new();
    for (index, part) in terrain_parts.into_iter().enumerate() {
        let base = matches!(part.material.alpha_mode, AlphaMode::Opaque);
        let part = prepare(part);
        if base {
            shroud_meshes.push(part.mesh.clone());
        }
        commands.spawn((
            Mesh3d(part.mesh),
            MeshMaterial3d(part.materials[0].clone()),
            crate::terrain::changes::Surface(index),
        ));
    }
    for part in road_parts {
        let part = prepare(part);
        commands.spawn((
            crate::terrain::changes::Road,
            Mesh3d(part.mesh),
            MeshMaterial3d(part.materials[0].clone()),
            RenderLayers::layer(11),
            bevy::camera::visibility::NoFrustumCulling,
        ));
    }
    let handles: Vec<Vec<_>> = model_parts
        .into_iter()
        .map(|parts| parts.into_iter().map(&mut prepare).collect())
        .collect();
    let shroud = source_materials.add(crate::source_material::terrain::shroud_material());
    for mesh in shroud_meshes {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(shroud.clone()),
            RenderLayers::layer(crate::source_material::terrain::SHROUD),
        ));
    }
    spawn_world_objects(
        &mut commands,
        objects,
        &handles,
        &mut source_materials,
        &shroud,
    );
    commands.insert_resource(placement_preview::Previews::new(models.placement_opacity));
    commands.insert_resource(Geometry {
        colors: colors.iter().map(|color| color.map(f32::to_bits)).collect(),
        models: handles,
        generation: u32::MAX,
        spawned: 0,
    });
    commands.remove_resource::<Models>();
    cameras(&mut commands, &static_levels);
}

fn prepare_tree_materials(models: &mut Models, objects: &WorldObjects) {
    for object in &objects.0 {
        if object.tree {
            for part in &mut models.parts[object.model_index] {
                // W3DTreeBuffer::doLighting uses full ambient/diffuse and post-light DCG.
                part.material.parameters.ambient = Vec4::ONE;
                part.material.parameters.diffuse = Vec4::ONE;
                part.material.parameters.vertex_diffuse = 2;
                part.material.parameters.alpha_test = 96.0 / 255.0;
                part.material.alpha_mode = AlphaMode::Mask(96.0 / 255.0);
                part.material.double_sided = true;
                part.material.depth_write = true;
            }
        }
    }
}

fn spawn_world_objects(
    commands: &mut Commands,
    objects: &WorldObjects,
    handles: &[Vec<PreparedPart>],
    materials: &mut Assets<SourceMaterial>,
    shroud: &Handle<SourceMaterial>,
) {
    let mut bridge_materials = BTreeMap::new();
    for (index, object) in objects.0.iter().enumerate() {
        if object.tree {
            continue;
        }
        let mut entity = commands.spawn((
            Transform::from_translation(source_position(object.position))
                .with_rotation(Quat::from_rotation_y(object.angle))
                .with_scale(Vec3::splat(crate::sorted_view::source_scale(object.scale))),
            crate::sorted_view::SourceScale(crate::sorted_view::source_scale(object.scale)),
            Visibility::Visible,
        ));
        if object.tree || object.prop_radius.is_some() {
            entity.insert(crate::scenery_clear::Scenery::new(index));
        }
        if let Some(supply) = object.supply {
            entity.insert(SupplyVisual(supply));
        }
        if object.animation.is_some() {
            entity.insert(crate::model_animation::WorldPlayer::new(
                object.model_index,
                object.supply,
            ));
            continue;
        }
        let instance = crate::sorted_view::RenderInstance::new();
        entity.with_children(|parent| {
            for part in &handles[object.model_index] {
                let material = if object.terrain_bridge {
                    bridge_materials
                        .entry(part.materials[0].id())
                        .or_insert_with(|| {
                            let mut material = materials
                                .get(&part.materials[0])
                                .expect("prepared bridge material")
                                .clone();
                            material.parameters.shroud_mode = 1;
                            materials.add(material)
                        })
                        .clone()
                } else {
                    part.materials[0].clone()
                };
                if object.terrain_bridge {
                    parent.spawn((
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(shroud.clone()),
                        RenderLayers::from_layers(&[
                            crate::source_material::terrain::BRIDGE_SHROUD,
                        ]),
                    ));
                }
                let mut child = parent.spawn((
                    instance.clone(),
                    Mesh3d(part.mesh.clone()),
                    MeshMaterial3d(material),
                    RenderLayers::layer(if object.terrain_bridge {
                        crate::source_material::terrain::BRIDGE
                    } else {
                        part.order.layer(1.0)
                    }),
                ));
                if !object.terrain_bridge {
                    child.insert(part.order);
                }
                if object.supply.is_some() && part.pickable {
                    child.insert(model_render::Pickable);
                }
                if let Some(submission) = &part.submission {
                    child.insert(submission.clone());
                }
                if let (Some(source), Some(draw), Some(binding)) =
                    (object.supply, &object.supply_draw, &part.binding)
                {
                    crate::supply_presentation::attach(&mut child, source, draw, binding.mesh);
                }
            }
        });
    }
}

fn cameras(commands: &mut Commands, static_levels: &std::collections::BTreeSet<u8>) {
    // Select Bevy's direct draw path at spawn; CPU visibility and GPU mesh
    // uniform preprocessing remain enabled for every original 3D scene pass.
    commands.spawn((
        Camera3d::default(),
        // OptionPreferences::getAntiAliasing defaults to NONE when absent.
        Msaa::Off,
        NoIndirectDrawing,
        bevy::camera::CompositingSpace::Srgb,
        WorldCamera,
        Tonemapping::None,
        Transform::default(),
        RenderLayers::layer(0),
    ));
    // W3DScene renders waypoint feedback after terrain and before drawables.
    // Bevy shares the target's depth texture; later passes preserve its contents.
    // RTS3DScene::Flush: opaque/occluded objects, tree decals, trees, static lists.
    let mut layers = vec![11];
    layers.extend(crate::source_material::terrain::PASSES);
    layers.extend([1, 2, 12, 13]);
    layers.extend(
        static_levels
            .iter()
            .rev()
            .filter(|&&level| level >= 2)
            .map(|&level| 16 + usize::from(level)),
    );
    layers.extend([5, 6]);
    if static_levels.contains(&1) {
        layers.push(17);
    }
    layers.extend([8, 7, 9, 4, 3, 10]);
    let final_pass = layers.len() - 1;
    for (index, layer) in layers.into_iter().enumerate() {
        let order = isize::try_from(index + 1).expect("bounded original scene passes");
        commands.spawn((
            Camera3d {
                depth_load_op: Camera3dDepthLoadOp::Load,
                ..default()
            },
            // All scene passes must share the world's color/depth sample count.
            Msaa::Off,
            Camera {
                order,
                clear_color: ClearColorConfig::None,
                // All 3D passes share the intermediate color/depth textures.
                // WorldCamera retains the initial window clear; only the final
                // scene pass needs to copy the accumulated framebuffer again.
                output_mode: if index == final_pass {
                    // Native water uses framebuffer alpha as scratch data;
                    // replace the output rather than compositing by that alpha.
                    CameraOutputMode::Write {
                        blend_state: Some(BlendState::REPLACE),
                        clear_color: ClearColorConfig::None,
                    }
                } else {
                    CameraOutputMode::Skip
                },
                ..default()
            },
            bevy::camera::CompositingSpace::Srgb,
            ScenePassCamera,
            NoIndirectDrawing,
            Tonemapping::None,
            Transform::default(),
            RenderLayers::from_layers(&[layer]),
        ));
    }
}

type ScenePassFilter = (With<ScenePassCamera>, Without<WorldCamera>);

pub fn sync_pass_cameras(
    main: Single<(&Transform, &Projection, &Camera), With<WorldCamera>>,
    mut passes: Query<(&mut Transform, &mut Projection, &mut Camera), ScenePassFilter>,
) {
    let (main_transform, main_projection, main_camera) = main.into_inner();
    for (mut transform, mut projection, mut camera) in &mut passes {
        transform.set_if_neq(*main_transform);
        projection.clone_from(main_projection);
        camera.viewport.clone_from(&main_camera.viewport);
    }
}

fn prepare_part(
    part: model_render::Part,
    textures: &BTreeMap<(String, u16), Handle<Image>>,
    owner_textures: &BTreeMap<(String, u16), Vec<Handle<Image>>>,
    colors: &[[f32; 3]],
    meshes: &mut Assets<Mesh>,
    source_materials: &mut Assets<SourceMaterial>,
    prepared_materials: &mut prepared_materials::Cache,
) -> PreparedPart {
    let mut material = part.material;
    material.texture = part
        .texture
        .as_ref()
        .and_then(|key| textures.get(key).cloned());
    let neutral = prepared_materials.add(material.clone(), source_materials);
    let mut materials = vec![neutral; colors.len() + 1];
    for owner in 0..colors.len() {
        let texture = part
            .texture
            .as_ref()
            .and_then(|key| owner_textures.get(key));
        if part.house_color || texture.is_some() {
            let mut owned = material.clone();
            if part.house_color {
                owned.parameters.ambient = Vec3::from_array(colors[owner]).extend(0.0);
                let opacity = owned.parameters.diffuse.w;
                owned.parameters.diffuse = Vec3::from_array(colors[owner]).extend(opacity);
            }
            if let Some(texture) = texture {
                owned.texture = Some(texture[owner].clone());
            }
            materials[owner + 1] = prepared_materials.add(owned, source_materials);
        }
    }
    // Keep animation-state prototypes available for pose/binding work without
    // allocating GPU buffers for models that have never been instantiated.
    let mut mesh = part.mesh;
    mesh.asset_usage = bevy::asset::RenderAssetUsages::MAIN_WORLD;
    PreparedPart {
        light_bounds: part.light_bounds,
        submission: part.submission,
        order: part.order,
        binding: part.binding.map(Arc::new),
        pickable: part.pickable,
        mesh: meshes.add(mesh),
        materials,
    }
}

fn apply_lighting(material: &mut SourceMaterial, lighting: &Lighting) {
    material.parameters.scene_ambient = Vec3::from_array(lighting.ambient).extend(0.0);
    for (index, &(color, direction)) in lighting.lights.iter().enumerate() {
        material.parameters.light_color[index] = Vec3::from_array(color).extend(0.0);
        material.parameters.light_direction[index] =
            Vec3::new(direction[0], direction[2], -direction[1])
                .normalize_or_zero()
                .extend(0.0);
    }
}

fn source_position(position: [f32; 3]) -> Vec3 {
    Vec3::new(position[0] * 0.1, position[2] * 0.1, -position[1] * 0.1)
}

pub fn render(
    mut commands: Commands,
    mut geometry: ResMut<Geometry>,
    battle: Res<Match>,
    mut visuals: Query<(
        Entity,
        &UnitVisual,
        &mut Transform,
        &mut Visibility,
        Option<&mut crate::light_environment::DrawPose>,
    )>,
    timings: Option<Res<crate::render_performance::Timings>>,
    draw_visibility: Option<Res<crate::model_animation::draw_visibility::DrawVisibility>>,
) {
    let timings = timings.map(Res::into_inner);
    let draw_visibility = draw_visibility.map(Res::into_inner);
    let _span = timings.map(|timings| timings.span("unit_visuals"));

    let battle = battle.into_inner();
    let observer = battle.observer_player.unwrap_or(0);
    if geometry.generation != battle.generation {
        for (entity, _, _, _, _) in &visuals {
            commands.entity(entity).despawn();
        }
        geometry.generation = battle.generation;
        geometry.spawned = 0;
    }
    spawn_units(&mut commands, &mut geometry, battle);
    for (entity, visual, mut transform, mut visibility, draw_pose) in &mut visuals {
        if visual.generation != battle.generation {
            continue;
        }
        if !drawable_present(&battle.simulation, visual.index) {
            // GameLogic::destroyObject calls GameClient::destroyDrawable. Bevy
            // recursively drops the private animated meshes with their children.
            commands.entity(entity).despawn();
            continue;
        }
        let unit = &battle.simulation.entities[visual.index];
        if unit.drawable_effectively_hidden() {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        if battle.simulation.catalog[unit.type_index].kind == generals_sim::skirmish::Kind::Crate {
            if !unit.crate_present() {
                // CrateCollide and DeletionUpdate destroy the original drawable.
                // The simulation retains only its stable object index.
                commands.entity(entity).despawn();
                continue;
            }
            visibility.set_if_neq(
                if battle.simulation.crate_visible_to(observer, visual.index) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
            );
            continue;
        }
        if unit.masked() && unit.alive() {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let mut shroud = battle.simulation.visibility_status(observer, visual.index);
        if matches!(
            shroud,
            generals_sim::skirmish::visibility::Status::Fogged
                | generals_sim::skirmish::visibility::Status::Shrouded
        ) && draw_visibility.is_some_and(|cache| !cache.obscured(battle, visual.index))
        {
            shroud = &generals_sim::skirmish::visibility::Status::Partial;
        }
        if matches!(shroud, generals_sim::skirmish::visibility::Status::Fogged) {
            // PartitionData's ghost retains the last visible drawable pose.
            visibility.set_if_neq(Visibility::Visible);
            continue;
        }
        if unit.masked() {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(
            if (unit.alive()
                || unit.has_drawable()
                    && matches!(
                        battle.simulation.catalog[unit.type_index].body_kind,
                        generals_sim::skirmish::BodyKind::Inactive
                    )
                || unit.corpse_present()
                || unit.physical_present()
                || battle.simulation.catalog[unit.type_index]
                    .maintenance
                    .rubble_height
                    .is_some()
                || battle.simulation.catalog[unit.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "LANDMARK_BRIDGE"))
                && (!unit.alive() || battle.simulation.logically_visible(observer, visual.index))
                && !matches!(shroud, generals_sim::skirmish::visibility::Status::Shrouded)
            {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        );
        transform.set_if_neq(unit_transform(&battle.simulation, visual.index));
        if let Some(mut pose) = draw_pose {
            *pose =
                crate::light_environment::DrawPose::from_object(&battle.simulation, visual.index);
            pose.position[2] += unit.aircraft_draw_height();
        }
    }
}

fn spawn_units(commands: &mut Commands, geometry: &mut Geometry, battle: &Match) {
    for (index, unit) in battle
        .simulation
        .entities
        .iter()
        .enumerate()
        .skip(geometry.spawned)
    {
        if !drawable_present(&battle.simulation, index) {
            continue;
        }
        let stats = &battle.simulation.catalog[unit.type_index];
        let crate_object = stats.kind == generals_sim::skirmish::Kind::Crate;
        if crate_object && !unit.crate_present() {
            continue;
        }
        let mut draw_pose =
            crate::light_environment::DrawPose::from_object(&battle.simulation, index);
        draw_pose.position[2] += unit.aircraft_draw_height();
        let mut visual = commands.spawn((
            UnitVisual {
                index,
                generation: battle.generation,
            },
            draw_pose,
            unit_transform(&battle.simulation, index),
            if unit.drawable_effectively_hidden()
                || unit.masked()
                || (crate_object && !battle.simulation.crate_visible_to(0, index))
            {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
            crate::model_animation::Players(Vec::new()),
        ));
        if let Some(source) = battle.simulation.supply_index(index) {
            visual.insert(SupplyVisual(source));
        }
    }
    geometry.spawned = battle.simulation.entities.len();
}

pub fn drawable_present(game: &generals_sim::skirmish::Skirmish, id: usize) -> bool {
    let actor = &game.entities[id];
    if actor.projectile_id().is_some() {
        return false;
    }
    actor.alive()
        || actor.has_drawable() && matches!(game.catalog[actor.type_index].body_kind, generals_sim::skirmish::BodyKind::Inactive)
        || actor.crate_present()
        || actor.corpse_present()
        || actor.physical_present()
        || game.retained_rubble(id)
        || game.catalog[actor.type_index].kind_of.iter().any(|kind| kind == "LANDMARK_BRIDGE")
        // The current presentation keeps W3DGhostObject's last-seen snapshot
        // on this root. Its orphan survives until the location is revealed.
        || matches!(
            game.visibility_status(0, id),
            generals_sim::skirmish::visibility::Status::Fogged
        )
}

fn unit_transform(game: &generals_sim::skirmish::Skirmish, id: usize) -> Transform {
    let unit = &game.entities[id];
    let base = Transform::from_translation(unit_position(game, id))
        .with_rotation(Quat::from_rotation_y(unit.facing));
    let Some(matrix) = unit.drawable_instance_matrix() else {
        return base;
    };
    // Native X,Y,Z -> view X,Z,-Y; Drawable postmultiplies its instance matrix.
    let matrix = Mat4::from_cols(
        Vec4::new(matrix[0][0], matrix[2][0], -matrix[1][0], 0.0),
        Vec4::new(matrix[0][2], matrix[2][2], -matrix[1][2], 0.0),
        Vec4::new(-matrix[0][1], -matrix[2][1], matrix[1][1], 0.0),
        Vec4::new(
            matrix[0][3] * 0.1,
            matrix[2][3] * 0.1,
            -matrix[1][3] * 0.1,
            1.0,
        ),
    );
    base.mul_transform(Transform::from_matrix(matrix))
}

pub fn unit_position(game: &generals_sim::skirmish::Skirmish, id: usize) -> Vec3 {
    let [x, y, z] = game.object_position(id);
    let draw_offset = game.entities[id].aircraft_draw_height() * 0.1;
    let world_height = z * 0.1;
    Vec3::new(x * 0.1, world_height + draw_offset, -y * 0.1)
}

pub fn register_capture(app: &mut App, enabled: bool, output: PathBuf) {
    app.insert_resource(Capture {
        stage: if enabled {
            CaptureStage::Rendering
        } else {
            CaptureStage::Disabled
        },
        frames: 0,
        output,
        intermediate: None,
        pipelines_ready: false,
        pipeline_error: None,
        started_at: None,
    });
    let surface = CaptureSurface(Arc::new(Mutex::new(SurfaceFrame {
        saved: false,
        available: false,
        requested: false,
        captured: None,
        pipeline_error: None,
    })));
    app.insert_resource(surface.clone());
    if enabled
        || app
            .world()
            .contains_resource::<crate::render_performance::Timings>()
    {
        app.add_systems(Update, capture_window_events);
        app.sub_app_mut(RenderApp)
            .insert_resource(surface)
            .add_systems(ExtractSchedule, capture_readiness)
            .add_systems(
                Render,
                (
                    capture_surface
                        .after(RenderSystems::PrepareViews)
                        .before(RenderSystems::Render),
                    capture_pipeline_result
                        .after(RenderSystems::Render)
                        .before(RenderSystems::Cleanup),
                ),
            );
    }
}

fn capture_window_events(
    mut occlusion: MessageReader<bevy::window::WindowOccluded>,
    mut focus: MessageReader<bevy::window::WindowFocused>,
) {
    for event in occlusion.read() {
        eprintln!(
            "Diagnostic window {:?}: occluded={}",
            event.window, event.occluded
        );
    }
    for event in focus.read() {
        eprintln!(
            "Diagnostic window {:?}: focused={}",
            event.window, event.focused
        );
    }
}

fn capture_surface(
    windows: Res<ExtractedWindows>,
    cameras: Res<bevy::render::camera::SortedCameras>,
    surface: Res<CaptureSurface>,
) {
    let windows = windows.into_inner();
    let cameras = cameras.into_inner();
    let surface = surface.into_inner();
    let available = windows
        .primary
        .and_then(|entity| windows.get(&entity))
        .is_some_and(|window| window.swap_chain_texture_view.is_some());
    let mut frame = surface.0.lock().expect("capture surface lock");
    if frame.available != available {
        eprintln!(
            "Diagnostic drawable available={available} primary={:?} windows={} cameras={}",
            windows.primary,
            windows.len(),
            cameras.0.len()
        );
    }
    frame.available = available;
    if frame.requested {
        frame.captured = Some(available);
    }
}

fn capture_pipeline_result(pipelines: Res<PipelineCache>, surface: Res<CaptureSurface>) {
    let pipelines = pipelines.into_inner();
    let surface = surface.into_inner();
    let mut frame = surface.0.lock().expect("capture surface lock");
    if frame.requested {
        frame.pipeline_error = pipelines
            .pipelines()
            .find_map(|pipeline| pipeline_error(&pipeline.state));
        frame.captured = frame.captured.map(|available| {
            available
                && pipelines.waiting_pipelines().next().is_none()
                && frame.pipeline_error.is_none()
        });
        frame.requested = false;
    }
}

fn capture_readiness(
    mut main_world: ResMut<MainWorld>,
    pipelines: Res<PipelineCache>,
    surface: Res<CaptureSurface>,
) {
    let pipelines = pipelines.into_inner();
    let surface = surface.into_inner();
    // Follow Bevy's loading_screen example: pipeline compilation belongs to the
    // render world, while screenshot requests are issued by the main world.
    if let Some(mut capture) = main_world.get_resource_mut::<Capture>() {
        capture.inspect_pipelines(
            pipelines.waiting_pipelines().next().is_some(),
            pipelines.pipelines().map(|pipeline| &pipeline.state),
        );
        let mut frame = surface.0.lock().expect("capture surface lock");
        if matches!(capture.stage, CaptureStage::Pending) && frame.captured.is_none() {
            frame.requested = true;
        }
    }
}

pub fn capture(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    rig: Option<Res<CameraRig>>,
    shell: Option<Res<crate::playtest::startup::Shell>>,
    surface: Res<CaptureSurface>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let shell = shell.map(Res::into_inner);
    let surface = surface.into_inner();
    let time = time.into_inner();

    if !matches!(capture.stage, CaptureStage::Disabled)
        && let Some(error) = capture.pipeline_error.take()
    {
        capture.stage = CaptureStage::Disabled;
        eprintln!("Smoke capture rejected a failed render pipeline: {error}");
        exit.write(AppExit::error());
        return;
    }

    if matches!(
        capture.stage,
        CaptureStage::Disabled | CaptureStage::Waiting
    ) || rig.is_some_and(|rig| rig.generation == u32::MAX)
    {
        return;
    }
    let started_at = *capture
        .started_at
        .get_or_insert_with(|| time.elapsed_secs());
    if time.elapsed_secs() - started_at >= 60.0 {
        let pending = matches!(capture.stage, CaptureStage::Pending);
        capture.stage = CaptureStage::Disabled;
        eprintln!(
            "Smoke capture timed out: stable frames={}, pipelines ready={}, drawable available={}, pending={}",
            capture.frames,
            capture.pipelines_ready,
            surface.0.lock().expect("capture surface lock").available,
            pending
        );
        exit.write(AppExit::error());
        return;
    }
    if matches!(capture.stage, CaptureStage::Pending) {
        return;
    }
    let mut frame = surface.0.lock().expect("capture surface lock");
    if matches!(capture.stage, CaptureStage::Rendering)
        && capture.frames == 0
        && capture.pipelines_ready
        && frame.available
    {
        eprintln!(
            "Smoke render became ready after {:.2} seconds; waiting for stable frames",
            time.elapsed_secs() - started_at
        );
        if let Some(shell) = &shell {
            eprintln!(
                "Startup shell render ready: elapsed_ms={:.3}",
                shell.startup_elapsed_ms()
            );
        }
    }
    if !capture.request_ready(&mut frame) {
        return;
    }
    drop(frame);
    commands
        .spawn(Screenshot::primary_window())
        .observe(finish_capture);
}

fn finish_capture(
    mut event: On<ScreenshotCaptured>,
    mut capture: ResMut<Capture>,
    surface: Res<CaptureSurface>,
    mut exit: MessageWriter<AppExit>,
    timings: Option<Res<crate::render_performance::Timings>>,
) {
    let profiling = timings.map(Res::into_inner).is_some();
    let surface = surface.into_inner();
    if !matches!(capture.stage, CaptureStage::Pending) {
        return;
    }
    let accepted = capture.complete_frame(&mut surface.0.lock().expect("capture surface lock"));
    if accepted {
        let output = capture.intermediate.map_or_else(
            || capture.output.clone(),
            |filename| capture.output.with_file_name(filename),
        );
        let result = std::mem::take(&mut event.image)
            .try_into_dynamic()
            .map_err(|error| error.to_string())
            .and_then(|image| {
                image
                    .to_rgb8()
                    .save(&output)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => {
                let final_image = capture.intermediate.take().is_none();
                capture.started_at = None;
                surface.0.lock().expect("capture surface lock").saved = final_image;
                eprintln!("Verified screenshot saved: {}", output.display());
                if final_image && !profiling {
                    exit.write(AppExit::Success);
                }
            }
            Err(error) => {
                eprintln!("Unable to save verified screenshot: {error}");
                exit.write(AppExit::error());
            }
        }
    } else {
        // Bevy can emit an untouched zero buffer when macOS supplies no drawable.
        // Reject that frame using its surface status, without inspecting pixels.
        // Bevy marks the completed request Captured and removes it next First.
        // Request another real frame within the existing deadline; never save
        // this readback or reuse another frame's drawable result.
        eprintln!(
            "Smoke capture discarded: requested frame lacked a drawable or ready shaders; retrying."
        );
    }
}

#[cfg(test)]
mod capture_tests {
    use super::{Capture, CaptureStage, SurfaceFrame};
    use std::path::PathBuf;

    fn capture() -> Capture {
        Capture {
            stage: CaptureStage::Rendering,
            frames: 0,
            output: PathBuf::new(),
            intermediate: None,
            pipelines_ready: false,
            pipeline_error: None,
            started_at: Some(12.0),
        }
    }

    const fn surface() -> SurfaceFrame {
        SurfaceFrame {
            saved: false,
            available: false,
            requested: false,
            captured: None,
            pipeline_error: None,
        }
    }

    #[test]
    fn verified_scene_waits_for_pipelines_and_a_drawable_without_warmup() {
        let mut capture = capture();
        let mut frame = surface();
        capture.finish_scenario();
        frame.available = true;
        assert!(!capture.request_ready(&mut frame));
        capture.pipelines_ready = true;
        frame.available = false;
        assert!(!capture.request_ready(&mut frame));
        frame.available = true;
        assert!(capture.request_ready(&mut frame));
        assert!(matches!(capture.stage, CaptureStage::Pending));
        assert_eq!(capture.frames, 0);
    }

    #[test]
    fn scenario_wait_preserves_the_final_readback_window_and_its_timeout() {
        use bevy::prelude::*;
        use std::{
            sync::{Arc, Mutex},
            time::Duration,
        };

        let mut capture = capture();
        capture.pipelines_ready = true;
        capture.wait_for_scenario();
        let mut app = App::new();
        app.add_message::<AppExit>()
            .insert_resource(capture)
            .insert_resource(Time::<Real>::default())
            .insert_resource(super::CaptureSurface(Arc::new(Mutex::new(surface()))))
            .add_systems(Update, super::capture);
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(100));
        app.update();
        assert!(app.world().resource::<Capture>().ready_for_scenario());
        assert!(app.world().resource::<Capture>().started_at.is_none());
        app.world_mut().resource_mut::<Capture>().finish_scenario();
        app.update();
        let capture = app.world().resource::<Capture>();
        assert!(matches!(capture.stage, CaptureStage::Verified));
        assert_eq!(
            capture.started_at.map(f32::to_bits),
            Some(100.0_f32.to_bits())
        );
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(59));
        app.update();
        assert!(matches!(
            app.world().resource::<Capture>().stage,
            CaptureStage::Verified
        ));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert!(matches!(
            app.world().resource::<Capture>().stage,
            CaptureStage::Disabled
        ));
    }

    #[test]
    fn missed_requested_drawable_retries_without_accepting_later_surface_or_resetting_deadline() {
        let mut capture = capture();
        let mut frame = surface();
        capture.pipelines_ready = true;
        capture.finish_scenario();
        frame.available = true;
        assert!(capture.request_ready(&mut frame));
        // A later available surface cannot validate the failed requested one.
        frame.captured = Some(false);
        assert!(!capture.complete_frame(&mut frame));
        assert!(capture.is_pending());
        assert!(matches!(capture.stage, CaptureStage::Verified));
        assert_eq!(
            capture.started_at.map(f32::to_bits),
            Some(12.0_f32.to_bits())
        );
        assert!(capture.request_ready(&mut frame));
        assert!(frame.captured.is_none());
        frame.captured = Some(true);
        assert!(capture.complete_frame(&mut frame));
        assert!(!capture.is_pending());
        assert!(matches!(capture.stage, CaptureStage::Disabled));
    }

    #[test]
    fn legacy_feedback_still_gets_its_command_window_before_capture() {
        let mut capture = capture();
        let mut frame = surface();
        capture.pipelines_ready = true;
        frame.available = true;
        for _ in 0..79 {
            assert!(!capture.request_ready(&mut frame));
        }
        assert!(!capture.ready_for_feedback());
        assert!(!capture.request_ready(&mut frame));
        assert!(capture.ready_for_feedback());
        for _ in 80..89 {
            assert!(!capture.request_ready(&mut frame));
        }
        assert!(capture.request_ready(&mut frame));
    }

    #[test]
    fn intermediate_retries_missed_drawable_then_returns_to_the_scenario() {
        let mut capture = capture();
        let mut frame = surface();
        capture.pipelines_ready = true;
        frame.available = true;
        capture.intermediate("cleanup-radius.png");
        assert!(capture.request_ready(&mut frame));
        frame.captured = Some(false);
        assert!(!capture.complete_frame(&mut frame));
        assert!(!capture.ready_for_scenario());
        assert!(capture.request_ready(&mut frame));
        frame.captured = Some(true);
        assert!(capture.complete_frame(&mut frame));
        assert!(capture.ready_for_scenario());
        assert!(capture.is_pending());
        assert_eq!(capture.intermediate, Some("cleanup-radius.png"));
    }

    #[test]
    fn failed_pipeline_rejects_verified_scene_and_pending_readback() {
        use bevy::{render::render_resource::CachedPipelineState, shader::ShaderCacheError};

        let mut capture = capture();
        let mut frame = surface();
        frame.available = true;
        capture.inspect_pipelines(false, std::iter::empty());
        capture.finish_scenario();
        assert!(capture.request_ready(&mut frame));
        let failed = CachedPipelineState::Err(ShaderCacheError::CreateShaderModule(
            "missing stream vertex attribute".to_owned(),
        ));
        capture.inspect_pipelines(false, std::iter::once(&failed));
        frame.captured = Some(true);
        assert!(!capture.complete_frame(&mut frame));
        assert!(!capture.request_ready(&mut frame));
        capture.wait_for_scenario();
        assert!(!capture.ready_for_scenario());
        assert!(capture.pipeline_error.is_some());
        // A later extraction cannot erase an error before the main world exits.
        capture.inspect_pipelines(false, std::iter::empty());
        assert!(!capture.pipelines_ready());
    }

    #[test]
    fn shader_load_retry_keeps_capture_waiting_without_a_fatal_error() {
        use bevy::{render::render_resource::CachedPipelineState, shader::ShaderCacheError};

        let mut capture = capture();
        capture.wait_for_scenario();
        let retry = CachedPipelineState::Err(ShaderCacheError::ShaderImportNotYetAvailable);
        capture.inspect_pipelines(true, std::iter::once(&retry));
        assert!(!capture.ready_for_scenario());
        assert!(capture.pipeline_error.is_none());
        capture.inspect_pipelines(false, std::iter::empty());
        assert!(capture.ready_for_scenario());
    }

    #[test]
    fn shader_failure_on_the_requested_frame_rejects_preflight_ready_capture() {
        let mut capture = capture();
        let mut frame = surface();
        capture.inspect_pipelines(false, std::iter::empty());
        capture.finish_scenario();
        frame.available = true;
        assert!(capture.request_ready(&mut frame));
        // Compilation after extraction can fail before the screenshot renders.
        // A later main-world callback must use that frame's result.
        frame.captured = Some(false);
        frame.pipeline_error = Some("stream shader failed during render".to_owned());
        assert!(!capture.complete_frame(&mut frame));
        assert_eq!(
            capture.pipeline_error.as_deref(),
            Some("stream shader failed during render")
        );
        assert!(matches!(capture.stage, CaptureStage::Verified));
        assert!(!capture.request_ready(&mut frame));
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn scene_passes_present_accumulated_color_without_water_alpha_recomposition() {
        use bevy::ecs::world::CommandQueue;
        use std::collections::BTreeSet;

        for levels in [BTreeSet::new(), BTreeSet::from([1, 4])] {
            let mut world = World::new();
            let mut queue = CommandQueue::default();
            cameras(&mut Commands::new(&mut queue, &world), &levels);
            queue.apply(&mut world);
            assert!(
                world
                    .query_filtered::<(
                        Has<NoIndirectDrawing>,
                        Has<bevy::camera::visibility::NoCpuCulling>,
                        Option<&Msaa>
                    ), With<Camera3d>>()
                    .iter(&world)
                    .all(|(direct, no_cpu_culling, msaa)| {
                        direct && !no_cpu_culling && msaa == Some(&Msaa::Off)
                    })
            );
            let (main, depth) = world
                .query_filtered::<(&Camera, &Camera3d), With<WorldCamera>>()
                .single(&world)
                .expect("one initial world camera");
            assert_eq!(main.order, 0);
            assert!(matches!(main.clear_color, ClearColorConfig::Default));
            assert!(matches!(
                depth.depth_load_op,
                Camera3dDepthLoadOp::Clear(0.0)
            ));
            assert!(matches!(
                main.output_mode,
                CameraOutputMode::Write {
                    blend_state: None,
                    clear_color: ClearColorConfig::Default
                }
            ));
            let mut passes: Vec<_> = world
                .query_filtered::<(&Camera, &Camera3d, &RenderLayers), With<ScenePassCamera>>()
                .iter(&world)
                .collect();
            passes.sort_by_key(|(camera, _, _)| camera.order);
            let mut expected = vec![11, 60, 61, 64, 62, 63, 1, 2, 12, 13];
            if !levels.is_empty() {
                expected.push(20);
            }
            expected.extend([5, 6]);
            if !levels.is_empty() {
                expected.push(17);
            }
            expected.extend([8, 7, 9, 4, 3, 10]);
            assert_eq!(passes.len(), expected.len());
            for (index, ((camera, depth, layers), layer)) in
                passes.into_iter().zip(expected).enumerate()
            {
                assert_eq!(camera.order, isize::try_from(index + 1).unwrap());
                assert!(camera.is_active);
                assert_eq!(layers, &RenderLayers::from_layers(&[layer]));
                assert!(matches!(depth.depth_load_op, Camera3dDepthLoadOp::Load));
                assert!(matches!(camera.clear_color, ClearColorConfig::None));
                if layer != 10 {
                    assert!(matches!(camera.output_mode, CameraOutputMode::Skip));
                    continue;
                }
                let CameraOutputMode::Write {
                    blend_state,
                    clear_color,
                } = camera.output_mode
                else {
                    panic!("final native scene pass must present its accumulated framebuffer");
                };
                // Explicit REPLACE is required: Bevy 0.19 interprets None as
                // ALPHA_BLENDING for every camera after the first output camera.
                assert_eq!(blend_state, Some(BlendState::REPLACE));
                assert!(matches!(clear_color, ClearColorConfig::None));
            }
        }
    }

    #[test]
    fn buffered_bridge_overlay_spawns_on_its_own_high_layer_and_camera() {
        use crate::source_material::terrain::{BRIDGE, BRIDGE_SHROUD};
        use bevy::ecs::world::CommandQueue;
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut materials = Assets::<SourceMaterial>::default();
        let original = materials.add(SourceMaterial::new(
            [1.0; 3],
            [1.0; 3],
            [0.0; 3],
            1.0,
            AlphaMode::Opaque,
            false,
        ));
        let shroud = materials.add(crate::source_material::terrain::shroud_material());
        let parts = vec![vec![PreparedPart {
            light_bounds: None,
            submission: None,
            order: model_render::RenderOrder::ordinary(),
            binding: None,
            pickable: false,
            mesh: Handle::default(),
            materials: vec![original],
        }]];
        let objects = WorldObjects(vec![WorldObject {
            terrain_bridge: true,
            animation: None,
            model_index: 0,
            position: [0.0; 3],
            angle: 0.0,
            scale: 1.0,
            tree: false,
            tree_data: None,
            prop_radius: None,
            supply: None,
            supply_draw: None,
        }]);
        let mut commands = Commands::new(&mut queue, &world);
        spawn_world_objects(&mut commands, &objects, &parts, &mut materials, &shroud);
        cameras(&mut commands, &std::collections::BTreeSet::new());
        queue.apply(&mut world);
        let mut meshes: Vec<_> = world
            .query_filtered::<(
                &RenderLayers,
                &MeshMaterial3d<SourceMaterial>,
                Has<model_render::RenderOrder>,
            ), With<Mesh3d>>()
            .iter(&world)
            .collect();
        meshes.sort_by_key(|(layers, _, _)| layers.iter().next().unwrap());
        assert_eq!(meshes.len(), 2);
        for ((layers, material, sorted), layer) in meshes.into_iter().zip([BRIDGE, BRIDGE_SHROUD]) {
            assert_eq!(layers.iter().collect::<Vec<_>>(), [layer]);
            assert!(!sorted);
            assert_eq!(
                materials.get(&material.0).unwrap().parameters.shroud_mode,
                if layer == BRIDGE { 1 } else { 2 }
            );
        }
        let overlay = RenderLayers::from_layers(&[BRIDGE_SHROUD]);
        let cameras: Vec<_> = world
            .query_filtered::<(&Camera, &RenderLayers), With<ScenePassCamera>>()
            .iter(&world)
            .filter(|(_, layers)| layers.intersects(&overlay))
            .collect();
        assert_eq!(cameras.len(), 1);
        // Road, scorch and bridge base precede the bridge overlay; tracks follow it.
        assert_eq!(cameras[0].0.order, 4);
    }

    #[test]
    fn direct_draw_cameras_keep_cpu_frustum_layer_and_hidden_filters() {
        use bevy::{
            camera::{
                primitives::{Aabb, Frustum},
                visibility::{
                    NoFrustumCulling, VisibilityClass, VisibleEntities,
                    check_visibility_cpu_culling,
                },
            },
            ecs::world::CommandQueue,
        };
        use std::{any::TypeId, collections::BTreeSet};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, check_visibility_cpu_culling);
        let world = app.world_mut();
        let mut queue = CommandQueue::default();
        cameras(
            &mut Commands::new(&mut queue, world),
            &BTreeSet::from([1, 4]),
        );
        queue.apply(world);
        for mut frustum in world
            .query_filtered::<&mut Frustum, With<Camera3d>>()
            .iter_mut(world)
        {
            *frustum = Frustum(bevy::math::primitives::ViewFrustum::from_clip_from_world(
                &Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0),
            ));
        }
        let mut spawn = |layer, x, inherited| {
            let mut class = VisibilityClass::default();
            class.push(TypeId::of::<Mesh3d>());
            world
                .spawn((
                    Mesh3d::default(),
                    class,
                    Aabb::from_min_max(Vec3::splat(-0.5), Vec3::splat(0.5)),
                    GlobalTransform::from_translation(Vec3::new(x, 0.0, -10.0)),
                    inherited,
                    ViewVisibility::default(),
                    RenderLayers::from_layers(&[layer]),
                ))
                .id()
        };
        let terrain = spawn(0, 0.0, InheritedVisibility::VISIBLE);
        let particle = spawn(3, 0.0, InheritedVisibility::VISIBLE);
        let offscreen = spawn(0, 1000.0, InheritedVisibility::VISIBLE);
        let hidden = spawn(0, 0.0, InheritedVisibility::HIDDEN);
        let wrong_layer = spawn(31, 0.0, InheritedVisibility::VISIBLE);
        let unbounded = spawn(3, 1000.0, InheritedVisibility::VISIBLE);
        world.entity_mut(unbounded).insert(NoFrustumCulling);
        app.update();
        let world = app.world_mut();
        for entity in [terrain, particle, unbounded] {
            assert!(world.get::<ViewVisibility>(entity).unwrap().get());
        }
        for entity in [offscreen, hidden, wrong_layer] {
            assert!(!world.get::<ViewVisibility>(entity).unwrap().get());
        }
        let visible = world
            .query_filtered::<&VisibleEntities, With<WorldCamera>>()
            .single(world)
            .unwrap();
        assert_eq!(visible.get(TypeId::of::<Mesh3d>()), &[terrain]);
        for (layers, visible) in world
            .query_filtered::<(&RenderLayers, &VisibleEntities), With<ScenePassCamera>>()
            .iter(world)
        {
            let expected = if *layers == RenderLayers::layer(3) {
                BTreeSet::from([particle, unbounded])
            } else {
                BTreeSet::new()
            };
            let actual: BTreeSet<_> = visible
                .get(TypeId::of::<Mesh3d>())
                .iter()
                .copied()
                .collect();
            assert_eq!(actual, expected);
        }
    }

    #[derive(Resource)]
    struct Changes(Vec<Entity>);

    #[test]
    fn idle_pass_roots_stop_reporting_transform_changes_and_camera_movement_propagates() {
        let mut app = App::new();
        app.insert_resource(Changes(Vec::new()))
            .add_systems(Update, sync_pass_cameras)
            .add_systems(
                Update,
                (|changed: Query<Entity, Changed<Transform>>, mut result: ResMut<Changes>| {
                    result.0 = changed.iter().collect();
                })
                .after(sync_pass_cameras),
            );
        let camera = app
            .world_mut()
            .spawn((
                WorldCamera,
                Transform::IDENTITY,
                Projection::default(),
                Camera::default(),
            ))
            .id();
        let pass = app
            .world_mut()
            .spawn((
                ScenePassCamera,
                Transform::IDENTITY,
                Projection::default(),
                Camera::default(),
            ))
            .id();
        app.update();
        app.update();
        assert!(!app.world().resource::<Changes>().0.contains(&pass));
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation = Vec3::new(10.0, 20.0, 30.0);
        app.update();
        assert!(app.world().resource::<Changes>().0.contains(&pass));
        assert_eq!(
            app.world().get::<Transform>(camera),
            app.world().get::<Transform>(pass)
        );
        app.update();
        assert!(!app.world().resource::<Changes>().0.contains(&pass));
    }
}
