//! Original horde and chemical-suit terrain decals, including Drawable's horde fade.
use crate::{
    model_animation::{Animations, Players},
    playtest::Match,
    playtest_view::UnitVisual,
    skirmish_data::{definition, optional_number},
    source_material::SourceMaterial,
    source_texture,
    terrain::Terrain,
};
use bevy::{
    camera::{
        primitives::{Aabb, MeshAabb},
        visibility::RenderLayers,
    },
    ecs::system::SystemParam,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{TerrainDecal, UnitType, visibility::Status};
use std::error::Error;

const TEXTURES: [&str; 6] = [
    "EXHorde",
    "EXHorde_UP",
    "EXHordeB",
    "EXHordeC_UP",
    "EXChemSuit",
    "fake_supply",
];

#[cfg(test)]
mod chem_tests;
pub mod chem_verification;
#[cfg(test)]
mod fake_tests;

struct Template {
    size: Vec2,
    offset: Vec2,
    chemical_suit: bool,
    fake: bool,
}

struct ObjectDecal {
    spawned: bool,
    opacity: f32,
    target: f32,
    rate: f32,
}

impl ObjectDecal {
    const fn new() -> Self {
        Self {
            spawned: false,
            opacity: 0.0,
            target: 0.0,
            rate: 0.0,
        }
    }

    fn advance(&mut self, present: bool, seconds: f32) {
        self.fade_target(
            if present { 1.0 } else { 0.0 },
            if present { 0.03 } else { -0.03 },
        );
        self.step(seconds);
    }

    const fn fade_target(&mut self, target: f32, rate: f32) {
        // Drawable.cpp851 retains the existing rate if the target is unchanged.
        if self.target.to_bits() != target.to_bits() {
            self.target = target;
            self.rate = rate;
        }
    }

    fn step(&mut self, seconds: f32) {
        let opacity_increment = self.rate * (seconds * 30.0);
        self.opacity += opacity_increment;
        if (self.rate < 0.0 && self.opacity <= 0.0) || (self.rate > 0.0 && self.opacity >= 1.0) {
            self.opacity = self.opacity.clamp(0.0, 1.0);
            self.rate = 0.0;
        }
    }
}

#[derive(Resource)]
pub struct HordeDecals {
    templates: Vec<Option<Template>>,
    images: Vec<Image>,
    handles: Vec<Handle<Image>>,
    objects: Vec<ObjectDecal>,
    generation: u32,
}

impl HordeDecals {
    fn advance(&mut self, battle: &Match, seconds: f32) {
        let game = &battle.simulation;
        self.objects
            .resize_with(game.entities.len(), ObjectDecal::new);
        // Drawable fades advance even while no visible model exists. First
        // visibility must retain that elapsed native opacity.
        for (id, actor) in game.entities.iter().enumerate() {
            if actor.exists()
                && let Some(template) = &self.templates[actor.type_index]
            {
                if let Some([target, rate]) = actor.terrain_decal_fade() {
                    self.objects[id].fade_target(target, rate);
                    self.objects[id].step(seconds);
                } else if template.fake {
                    self.objects[id].opacity = if fake_visible(battle, actor.owner) {
                        1.0
                    } else {
                        0.0
                    };
                } else if template.chemical_suit {
                    self.objects[id].opacity = if actor.chemical_suit() { 1.0 } else { 0.0 };
                } else {
                    self.objects[id].advance(actor.in_horde(), seconds);
                }
            }
        }
    }
}

#[derive(Component)]
pub struct Decal {
    object: usize,
    visual: Entity,
    generation: u32,
    mesh: Handle<Mesh>,
    material: Handle<SourceMaterial>,
    model: usize,
    texture: usize,
    size: Vec2,
    position: Vec2,
    angle: f32,
    last_tick: u64,
}

pub fn load(assets: &VirtualAssets, types: &[UnitType]) -> Result<HordeDecals, Box<dyn Error>> {
    let files = [
        content::scan(assets.read("data/ini/object/chinainfantry.ini")?)?,
        content::scan(assets.read("data/ini/object/chinavehicle.ini")?)?,
        content::scan(assets.read("data/ini/object/americainfantry.ini")?)?,
        content::scan(assets.read("data/ini/object/factionbuilding.ini")?)?,
    ];
    let mut templates = Vec::new();
    for unit in types {
        let chemical_suit = unit
            .maintenance
            .armor_upgrade
            .as_ref()
            .is_some_and(|upgrade| upgrade.trigger == "Upgrade_AmericaChemicalSuits");
        let fake = unit.kind_of.iter().any(|kind| kind == "FS_FAKE");
        if unit.behavior.horde.is_none() && !chemical_suit && !fake {
            templates.push(None);
            continue;
        }
        let lines = &definition(&files, "Object", &unit.name)?.lines;
        if fake
            && crate::skirmish_data::field(lines, 0, "ShadowTexture")
                .map(crate::skirmish_data::scalar)
                .transpose()?
                != Some(TEXTURES[5])
        {
            return Err(format!("unsupported fake building shadow texture: {}", unit.name).into());
        }
        templates.push(Some(Template {
            chemical_suit,
            fake,
            size: Vec2::new(
                optional_number(lines, "ShadowSizeX", 0.0)?,
                optional_number(lines, "ShadowSizeY", 0.0)?,
            ),
            offset: Vec2::new(
                optional_number(lines, "ShadowOffsetX", 0.0)?,
                optional_number(lines, "ShadowOffsetY", 0.0)?,
            ),
        }));
    }
    let mut images = Vec::new();
    for name in TEXTURES {
        let mut image = source_texture::load(assets, name)?;
        image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            ..default()
        });
        images.push(image);
    }
    Ok(HordeDecals {
        templates,
        images,
        handles: Vec::new(),
        objects: Vec::new(),
        generation: u32::MAX,
    })
}

#[derive(SystemParam)]
pub struct View<'w, 's> {
    battle: Res<'w, Match>,
    terrain: Res<'w, Terrain>,
    animations: Res<'w, Animations>,
    time: Res<'w, Time<Virtual>>,
    models: Query<'w, 's, (Entity, &'static UnitVisual, &'static Players)>,
}

#[derive(SystemParam)]
pub struct Art<'w> {
    images: ResMut<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
}

pub fn update(
    mut commands: Commands,
    mut data: ResMut<HordeDecals>,
    view: View,
    mut art: Art,
    mut decals: Query<(Entity, &mut Decal, &mut Visibility, &mut Aabb)>,
) {
    let View {
        battle,
        terrain,
        animations,
        time,
        models,
    } = view;
    if data.handles.is_empty() {
        let images = std::mem::take(&mut data.images);
        data.handles = images
            .into_iter()
            .map(|image| art.images.add(image))
            .collect();
    }
    if data.generation != battle.generation {
        for (entity, decal, _, _) in &decals {
            art.meshes.remove(decal.mesh.id());
            art.materials.remove(decal.material.id());
            commands.entity(entity).despawn();
        }
        data.objects.clear();
        data.generation = battle.generation;
    }
    data.advance(&battle, time.delta_secs());
    for (visual_entity, visual, players) in &models {
        if visual.generation != battle.generation {
            continue;
        }
        let actor = &battle.simulation.entities[visual.index];
        if data.templates[actor.type_index].is_none() {
            continue;
        }
        let Some((model, bounds)) = animations.decal_model(players, actor.type_index) else {
            continue;
        };
        if !data.objects[visual.index].spawned && actor.alive() {
            spawn_decal(
                &mut commands,
                &mut art,
                &battle,
                &terrain,
                &data,
                (visual_entity, visual.index),
                (model, bounds),
            );
            data.objects[visual.index].spawned = true;
        }
    }
    for (entity, mut decal, mut visibility, mut aabb) in &mut decals {
        if decal.generation != battle.generation {
            continue;
        }
        let actor = &battle.simulation.entities[decal.object];
        if !actor.exists() {
            art.meshes.remove(decal.mesh.id());
            art.materials.remove(decal.material.id());
            commands.entity(entity).despawn();
            continue;
        }
        let Ok((_, _, players)) = models.get(decal.visual) else {
            continue;
        };
        let Some((model, bounds)) = animations.decal_model(players, actor.type_index) else {
            continue;
        };
        update_decal(
            (&mut decal, &mut aabb),
            &data,
            &battle,
            &terrain,
            &mut art,
            (model, bounds),
        );
        visibility.set_if_neq(
            if battle.started
                && !actor.masked()
                && (data.templates[actor.type_index]
                    .as_ref()
                    .is_some_and(|template| template.fake)
                    || !actor.terrain_decal_none())
                // Drawable::setStealthLook -> updateHiddenStatus -> setHidden
                // suppresses the terrain decal together with an unseen actor.
                && battle.simulation.logically_visible(0, decal.object)
                && data.objects[decal.object].opacity > 0.0
                && !matches!(
                    battle.simulation.visibility_status(0, decal.object),
                    Status::Fogged | Status::Shrouded
                )
            {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        );
    }
}

fn spawn_decal(
    commands: &mut Commands,
    art: &mut Art,
    battle: &Match,
    terrain: &Terrain,
    data: &HordeDecals,
    object: (Entity, usize),
    model: (usize, [f32; 2]),
) {
    let (visual, object) = object;
    let (model, bounds) = model;
    let actor = &battle.simulation.entities[object];
    let template = data.templates[actor.type_index]
        .as_ref()
        .expect("horde template");
    let size = initial_size(template.size, bounds);
    let position = Vec2::from_array(actor.position);
    let mut material =
        SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Blend, false);
    material.parameters.vertex_diffuse = 3;
    material.parameters.opacity = (data.objects[object].opacity * 255.0).trunc() / 255.0;
    material.texture = Some(data.handles[0].clone());
    let material = art.materials.add(material);
    let mesh = terrain.object_decal(position, size, template.offset, actor.facing, [1.0; 4]);
    let bounds = mesh.compute_aabb().expect("terrain decal positions");
    let mesh = art.meshes.add(mesh);
    commands.spawn((
        Decal {
            object,
            visual,
            generation: battle.generation,
            mesh: mesh.clone(),
            material: material.clone(),
            model,
            texture: usize::MAX,
            size,
            position,
            angle: actor.facing,
            last_tick: battle.simulation.tick,
        },
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::IDENTITY,
        bounds,
        Visibility::Hidden,
        RenderLayers::layer(2),
    ));
}

fn initial_size(authored: Vec2, bounds: [f32; 2]) -> Vec2 {
    Vec2::new(
        if authored.x == 0.0 {
            bounds[0]
        } else {
            authored.x
        },
        if authored.y == 0.0 {
            bounds[1]
        } else {
            authored.y
        },
    )
}

fn fake_visible(battle: &Match, owner: usize) -> bool {
    // Drawable::changedTeam and GameClient::updateFakeDrawables expose the
    // authored shadow texture to allies/neutral observers, never enemies.
    battle
        .resource_owner()
        .is_none_or(|viewer| !battle.simulation.enemies(viewer, owner))
}

fn update_decal(
    object: (&mut Decal, &mut Aabb),
    data: &HordeDecals,
    battle: &Match,
    terrain: &Terrain,
    art: &mut Art,
    model: (usize, [f32; 2]),
) {
    let (decal, aabb) = object;
    let (model, bounds) = model;
    let actor = &battle.simulation.entities[decal.object];
    let unit = &battle.simulation.catalog[actor.type_index];
    let template = data.templates[actor.type_index]
        .as_ref()
        .expect("Horde decal template");
    let infantry = unit.kind_of.iter().any(|kind| kind == "INFANTRY");
    let texture = if template.fake {
        Some(5)
    } else {
        match actor.terrain_decal() {
            TerrainDecal::None => None,
            TerrainDecal::Horde => Some(0),
            TerrainDecal::HordeNationalism => Some(1),
            TerrainDecal::HordeVehicle => Some(2),
            TerrainDecal::HordeFanaticism => Some(3),
            TerrainDecal::ChemicalSuit => Some(4),
        }
    };
    let present = if template.fake {
        fake_visible(battle, actor.owner)
    } else if template.chemical_suit {
        actor.chemical_suit()
    } else {
        actor.in_horde()
    };
    if texture.is_none() {
        decal.texture = usize::MAX;
    }
    let changed = model != decal.model || texture.is_some_and(|texture| texture != decal.texture);
    let mut size = if !infantry
        && battle.simulation.tick != decal.last_tick
        && actor.in_horde()
        && !actor.hacked()
        && !actor.paralyzed()
        && actor.contained_by().is_none()
    {
        Vec2::splat(3.5 * unit.geometry.major)
    } else {
        decal.size
    };
    if changed {
        // Native sets vehicle size before setTerrainDecal recreates the original
        // shadow dimensions. Its next logic update restores the vehicle size.
        size = initial_size(template.size, bounds);
        decal.model = model;
        if let Some(texture) = texture
            && let Some(mut material) = art.materials.get_mut(&decal.material)
        {
            decal.texture = texture;
            material.texture = Some(data.handles[texture].clone());
        }
    }
    decal.last_tick = battle.simulation.tick;
    let alpha = data.objects[decal.object].opacity;
    let position = Vec2::from_array(actor.position);
    if alpha > 0.0
        && (changed
            || position != decal.position
            || size != decal.size
            || actor.facing.to_bits() != decal.angle.to_bits())
    {
        replace_geometry(
            &mut art.meshes,
            &decal.mesh,
            aabb,
            terrain.object_decal(position, size, template.offset, actor.facing, [1.0; 4]),
        );
        decal.position = position;
        decal.size = size;
        decal.angle = actor.facing;
    }
    if alpha == 0.0 && !present {
        decal.texture = usize::MAX;
    }
    if let Some(material) = art.materials.get(&decal.material) {
        let value = (alpha * 255.0).trunc() / 255.0;
        if material.parameters.opacity.to_bits() != value.to_bits()
            && let Some(mut material) = art.materials.get_mut(&decal.material)
        {
            material.parameters.opacity = value;
        }
    }
}

fn replace_geometry(
    meshes: &mut Assets<Mesh>,
    handle: &Handle<Mesh>,
    bounds: &mut Aabb,
    mesh: Mesh,
) {
    // Vertices use world coordinates; Bevy's initial entity bounds do not follow
    // later mesh asset edits. Preserve frustum culling with the new bounds.
    *bounds = mesh.compute_aabb().expect("terrain decal positions");
    if let Some(mut current) = meshes.get_mut(handle) {
        *current = mesh;
    }
}

#[derive(SystemParam)]
pub struct Inspection<'w, 's> {
    data: Res<'w, HordeDecals>,
    decals: Query<'w, 's, (&'static Decal, &'static ViewVisibility)>,
    materials: Res<'w, Assets<SourceMaterial>>,
    meshes: Res<'w, Assets<Mesh>>,
}

impl Inspection<'_, '_> {
    pub fn inspect(&self, battle: &Match, id: usize) -> Result<Option<f32>, Box<dyn Error>> {
        let Some((decal, visible)) = self
            .decals
            .iter()
            .find(|(decal, _)| decal.object == id && decal.generation == battle.generation)
        else {
            return Ok(None);
        };
        let alpha = self.data.objects[id].opacity;
        if !visible.get() || alpha == 0.0 {
            return Ok(None);
        }
        let actor = &battle.simulation.entities[id];
        let template = self.data.templates[actor.type_index]
            .as_ref()
            .ok_or("missing original horde decal template")?;
        let material = self
            .materials
            .get(&decal.material)
            .ok_or("missing horde material")?;
        let expected = usize::from(actor.horde_bonuses() & 2 != 0);
        if material.texture.as_ref() != self.data.handles.get(expected)
            || decal.size != template.size
            || self.meshes.get(&decal.mesh).is_none()
        {
            return Err(
                "horde decal differs from original infantry texture/dimensions/terrain geometry"
                    .into(),
            );
        }
        Ok(Some(alpha))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_and_rotating_world_decal_replaces_its_entity_culling_bounds() {
        let mut meshes = Assets::<Mesh>::default();
        let initial = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[-1.0, 0.01, -0.5], [1.0, 0.01, 0.5]],
        );
        let mut bounds = initial.compute_aabb().unwrap();
        let handle = meshes.add(initial);
        let previous = bounds.center;
        // A translated/quarter-turned rectangular terrain patch, with the new
        // slope height. Position/UV projection itself is covered in terrain::decal.
        let changed = Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[2.5, 0.01, -3.0], [3.5, 0.11, -1.0]],
        );
        replace_geometry(&mut meshes, &handle, &mut bounds, changed);
        assert!(bounds.center.x > previous.x + 2.0);
        assert!(bounds.center.z < previous.z - 1.0);
        let actual = meshes.get(&handle).unwrap().compute_aabb().unwrap();
        assert_eq!(bounds.center, actual.center);
        assert_eq!(bounds.half_extents, actual.half_extents);
    }

    #[test]
    fn hidden_first_visible_decal_keeps_elapsed_fade_and_reverse_progress() {
        let mut state = ObjectDecal::new();
        // No model/decal exists while its drawable is first hidden by shroud.
        state.advance(true, 2.0);
        assert!(!state.spawned);
        assert_eq!(state.opacity.to_bits(), 1.0_f32.to_bits());
        state.advance(false, 0.25);
        assert!((state.opacity - 0.775).abs() < f32::EPSILON);
        // Presentation creates the decal later without resetting source fade.
        state.spawned = true;
        state.advance(true, 0.1);
        assert!((state.opacity - 0.865).abs() < f32::EPSILON);
    }

    #[test]
    fn sinking_infantry_fades_existing_decal_and_preserves_unchanged_target_rate() {
        let mut decal = ObjectDecal::new();
        decal.advance(true, 2.0);
        decal.fade_target(0.0, -0.2);
        decal.step(1.0 / 30.0);
        assert_eq!(decal.opacity.to_bits(), 0.8_f32.to_bits());
        decal.step(1.0);
        assert_eq!(decal.opacity.to_bits(), 0.0_f32.to_bits());
        let mut already_fading = ObjectDecal::new();
        already_fading.advance(true, 2.0);
        already_fading.advance(false, 0.1);
        already_fading.fade_target(0.0, -0.2);
        assert_eq!(already_fading.rate.to_bits(), (-0.03_f32).to_bits());
    }
}
