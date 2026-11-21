//! Client-owned drawables and free particle systems for the Particle Uplink.
use crate::{
    laser_beam::{self, Beam},
    model_animation::{Animations, Players},
    particles::{Particles, smudge::SceneMaterial},
    playtest::Match,
    playtest_view::{UnitVisual, WorldCamera},
    skirmish_data::{blocks, definition, field, number, scalar, unsigned},
};
use bevy::{
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::VertexAttributeValues,
    prelude::*,
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{
    Skirmish, UnitType,
    particle_uplink::{ORBITAL_BEAM_Z_OFFSET, Rules, Stage, Visual},
    visibility::Status,
};
use num_traits::ToPrimitive;
use std::{collections::BTreeMap, error::Error};

struct Laser {
    beams: Vec<Beam>,
    tile: Option<(f32, f32)>,
    scroll: f32,
    image: Option<Image>,
    material: Option<Handle<SceneMaterial>>,
}

struct Assembly {
    outer: Vec<[[f32; 4]; 3]>,
    connector: Option<[f32; 3]>,
    origin: Option<[f32; 3]>,
    emitters: Vec<u64>,
    stage: Stage,
    visible: bool,
    revealed: bool,
}

type Key = (usize, usize, usize);

#[derive(Resource)]
pub struct Presentation {
    lasers: BTreeMap<String, Laser>,
    assemblies: BTreeMap<(usize, usize), Assembly>,
    meshes: BTreeMap<Key, (Entity, Handle<Mesh>, String)>,
    generation: u32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Inspection<'w, 's> {
    presentation: Res<'w, Presentation>,
    visuals: Query<
        'w,
        's,
        (
            &'static ViewVisibility,
            &'static MeshMaterial3d<SceneMaterial>,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SceneMaterial>>,
    images: Res<'w, Assets<Image>>,
}

impl Inspection<'_, '_> {
    pub fn visible_impact(&self, battle: &Match, provider: usize) -> Result<bool, Box<dyn Error>> {
        let presentation = &self.presentation;
        if presentation.generation != battle.generation {
            return Ok(false);
        }
        for (ordinal, visual) in battle
            .simulation
            .particle_uplink_visuals(provider)
            .enumerate()
        {
            if !visual.target.is_some_and(|target| target.width > 0.0) {
                continue;
            }
            let Some((entity, handle, _)) = presentation.meshes.get(&(provider, ordinal, 17))
            else {
                continue;
            };
            let Ok((visible, material)) = self.visuals.get(*entity) else {
                continue;
            };
            if !visible.get() {
                continue;
            }
            let mesh = self.meshes.get(handle).ok_or("missing orbital beam mesh")?;
            let material = self
                .materials
                .get(&material.0)
                .ok_or("missing orbital beam material")?;
            let image = material
                .texture
                .as_ref()
                .and_then(|handle| self.images.get(handle))
                .ok_or("missing orbital beam texture")?;
            return Ok(
                mesh.indices().is_some_and(|indices| !indices.is_empty()) && image.width() > 0
            );
        }
        Ok(false)
    }
}

pub fn load(assets: &VirtualAssets, catalog: &[UnitType]) -> Result<Presentation, Box<dyn Error>> {
    let mut lasers = BTreeMap::new();
    if catalog
        .iter()
        .any(|unit| !unit.behavior.particle_uplinks.is_empty())
    {
        let files = [content::scan(
            assets.read("data/ini/object/weaponobjects.ini")?,
        )?];
        for rules in catalog
            .iter()
            .flat_map(|unit| &unit.behavior.particle_uplinks)
        {
            for name in rules
                .effects
                .connector_lasers
                .iter()
                .chain([&rules.effects.orbital_laser])
            {
                if !name.is_empty()
                    && !name.eq_ignore_ascii_case("NONE")
                    && !lasers.contains_key(name)
                {
                    let object = definition(&files, "Object", name)?;
                    let draw = blocks(&object.lines, 0, "Draw")?
                        .into_iter()
                        .find(|block| {
                            block[0]
                                .tokens
                                .get(1)
                                .is_some_and(|entry| entry == "W3DLaserDraw")
                        })
                        .ok_or("missing Particle Uplink W3DLaserDraw")?;
                    let count = u16::try_from(unsigned(draw, 1, "NumBeams")?)?;
                    if count == 0
                        || field(draw, 1, "Segments")
                            .map(|_| unsigned(draw, 1, "Segments"))
                            .transpose()?
                            .unwrap_or(1)
                            != 1
                        || field(draw, 1, "ArcHeight").is_some()
                    {
                        return Err(format!("unsupported Particle Uplink laser draw {name}").into());
                    }
                    let inner = laser_beam::color(
                        field(draw, 1, "InnerColor").ok_or("missing inner beam color")?,
                    )?;
                    let outer = laser_beam::color(
                        field(draw, 1, "OuterColor").ok_or("missing outer beam color")?,
                    )?;
                    let widths = [
                        number(draw, 1, "InnerBeamWidth")?,
                        number(draw, 1, "OuterBeamWidth")?,
                    ];
                    let texture = scalar(
                        field(draw, 1, "Texture").ok_or("missing Particle Uplink beam texture")?,
                    )?;
                    let mut image = crate::source_texture::load(assets, texture)?;
                    image.texture_descriptor.format =
                        image.texture_descriptor.format.remove_srgb_suffix();
                    let aspect = f32::from(u16::try_from(image.width())?)
                        / f32::from(u16::try_from(image.height())?);
                    let tile = field(draw, 1, "Tile").map(scalar).transpose()? == Some("Yes");
                    let tiling = field(draw, 1, "TilingScalar")
                        .map(scalar)
                        .transpose()?
                        .map(str::parse)
                        .transpose()?
                        .unwrap_or(1.0);
                    let scroll = field(draw, 1, "ScrollRate")
                        .map(scalar)
                        .transpose()?
                        .map(str::parse)
                        .transpose()?
                        .unwrap_or(0.0);
                    let beams = (0..count)
                        .rev()
                        .map(|index| laser_beam::beam(index, count, widths, inner, outer))
                        .collect();
                    lasers.insert(
                        name.clone(),
                        Laser {
                            beams,
                            tile: tile.then_some((aspect, tiling)),
                            scroll,
                            image: Some(image),
                            material: None,
                        },
                    );
                }
            }
        }
    }
    Ok(Presentation {
        lasers,
        assemblies: BTreeMap::new(),
        meshes: BTreeMap::new(),
        generation: 0,
    })
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Resources<'w, 's> {
    animations: ResMut<'w, Animations>,
    particles: ResMut<'w, Particles>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SceneMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    models: Query<'w, 's, (&'static UnitVisual, &'static Players, &'static Transform)>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut presentation: ResMut<Presentation>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut res: Resources,
) {
    let battle = battle.into_inner();
    let Presentation {
        lasers,
        assemblies,
        meshes,
        generation,
    } = &mut *presentation;
    if *generation != battle.generation {
        *generation = battle.generation;
        assemblies.clear();
        for (_, (entity, handle, _)) in std::mem::take(meshes) {
            commands.entity(entity).despawn();
            res.meshes.remove(&handle);
        }
    }
    let beams = collect_beams(battle, assemblies, &mut res);
    meshes.retain(|key, (entity, mesh, template)| {
        if beams
            .get(key)
            .is_some_and(|(name, _, _, _)| *name == template)
        {
            true
        } else {
            commands.entity(*entity).despawn();
            res.meshes.remove(&*mesh);
            false
        }
    });
    let camera = camera.into_inner();
    for (key, (name, points, width, elapsed)) in beams {
        let Some(laser) = lasers.get_mut(name) else {
            continue;
        };
        if let Some(image) = laser.image.take() {
            laser.material = Some(res.materials.add(SceneMaterial {
                smudge: 0,
                texture: Some(res.images.add(image)),
            }));
        }
        let mesh = match mesh(laser, points, width, elapsed, camera) {
            Ok(mesh) => mesh,
            Err(error) => {
                error!("Particle Uplink beam: {error}");
                continue;
            }
        };
        if let Some((_, handle, _)) = meshes.get(&key) {
            if let Some(mut destination) = res.meshes.get_mut(handle) {
                *destination = mesh;
            }
        } else {
            let handle = res.meshes.add(mesh);
            let entity = commands
                .spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(
                        laser
                            .material
                            .clone()
                            .expect("loaded Particle Uplink material"),
                    ),
                    Transform::IDENTITY,
                    NoFrustumCulling,
                    RenderLayers::layer(3),
                ))
                .id();
            meshes.insert(key, (entity, handle, name.clone()));
        }
    }
}

fn collect_beams<'a>(
    battle: &'a Match,
    assemblies: &mut BTreeMap<(usize, usize), Assembly>,
    res: &mut Resources,
) -> BTreeMap<(usize, usize, usize), BeamPose<'a>> {
    let game = &battle.simulation;
    let viewer = battle.observer_player.unwrap_or(0);
    let mut beams = BTreeMap::new();
    if battle.started {
        for (id, actor) in game
            .entities
            .iter()
            .enumerate()
            .filter(|(_, actor)| actor.exists())
        {
            let rules = &game.catalog[actor.type_index].behavior.particle_uplinks;
            if rules.is_empty() {
                continue;
            }
            let clear = matches!(game.visibility_status(viewer, id), Status::Clear);
            for (ordinal, (rules, visual)) in rules
                .iter()
                .zip(game.particle_uplink_visuals(id))
                .enumerate()
            {
                let active_key = (id, ordinal);
                let entry = assemblies.entry(active_key).or_insert_with(|| Assembly {
                    outer: Vec::new(),
                    connector: None,
                    origin: None,
                    emitters: Vec::new(),
                    stage: Stage::Idle,
                    visible: false,
                    revealed: false,
                });
                let transition = entry.stage != visual.stage;
                let newly_revealed = entry.visible != clear && clear;
                if transition || entry.visible != clear {
                    drain_emitters(&mut res.particles, &mut entry.emitters);
                    entry.stage = visual.stage;
                    entry.visible = clear;
                    entry.revealed = newly_revealed && !transition;
                    if clear {
                        start_assembly(entry, rules, id, battle, res);
                    }
                }
                let elapsed = visual.elapsed.to_f32().expect("native update frame");
                if clear {
                    building_beams(&mut beams, active_key, rules, &visual, entry, elapsed);
                }
                if let Some(target) = visual.target
                    && target.width > 0.0
                {
                    let elapsed = target.elapsed.to_f32().expect("native orbital frame");
                    beams.insert(
                        (id, ordinal, 17),
                        (
                            &rules.effects.orbital_laser,
                            [above(target.position), target.position],
                            target.width,
                            elapsed,
                        ),
                    );
                }
            }
        }
    }
    assemblies.retain(|(id, _), entry| {
        if battle.started
            && game
                .entities
                .get(*id)
                .is_some_and(generals_sim::skirmish::Entity::exists)
        {
            true
        } else {
            drain_emitters(&mut res.particles, &mut entry.emitters);
            false
        }
    });
    beams
}

fn start_assembly(
    entry: &mut Assembly,
    rules: &Rules,
    id: usize,
    battle: &Match,
    res: &mut Resources,
) {
    let game = &battle.simulation;
    let effects = &rules.effects;
    let (outer, connector, base) = match entry.stage {
        Stage::Charging => (0, None, None),
        Stage::Preparing => (1, None, None),
        Stage::AlmostReady => (1, Some(0), None),
        Stage::Ready => (1, Some(0), Some(0)),
        Stage::Firing => (2, Some(1), Some(2)),
        Stage::Postfire => (1, Some(0), Some(1)),
        Stage::Idle | Stage::Packing => return,
    };
    if entry.outer.is_empty() {
        entry.outer = (1..=effects.outer_bone_count)
            .filter_map(|index| {
                let bone = format!("{}{index:02}", effects.outer_bone);
                game.logical_bone_matrix(id, &bone)
                    .map(|matrix| world_matrix(game, id, matrix))
            })
            .collect();
    }
    if connector.is_some() && entry.connector.is_none() {
        entry.connector = animated_bone(id, &effects.connector_bone, game, battle.generation, res);
        entry.origin = animated_bone(id, &effects.fire_bone, game, battle.generation, res);
    }
    let mut emit = |name: &str, matrix| {
        if let Some(system) = res.particles.particle_uplink_emitter(
            name,
            matrix,
            &mut res.animations,
            battle.generation,
            game.tick,
        ) {
            entry.emitters.push(system);
        }
    };
    for &matrix in &entry.outer {
        emit(&effects.outer_flares[outer], matrix);
    }
    if let (Some(index), Some(position)) = (connector, entry.connector) {
        emit(&effects.connector_flares[index], at(position));
    }
    if let (Some(index), Some(position)) = (base, entry.origin) {
        emit(&effects.base_flares[index], at(position));
    }
}

type BeamPose<'a> = (&'a String, [[f32; 3]; 2], f32, f32);

fn building_beams<'a>(
    beams: &mut BTreeMap<Key, BeamPose<'a>>,
    (id, ordinal): (usize, usize),
    rules: &'a Rules,
    visual: &Visual,
    entry: &Assembly,
    elapsed: f32,
) {
    let connector = match visual.stage {
        Stage::AlmostReady | Stage::Ready | Stage::Postfire => 0,
        Stage::Firing => 1,
        _ => return,
    };
    if let Some(position) = entry.connector {
        for (slot, matrix) in entry.outer.iter().enumerate() {
            beams.insert(
                (id, ordinal, slot),
                (
                    &rules.effects.connector_lasers[connector],
                    [matrix.map(|row| row[3]), position],
                    1.0,
                    elapsed,
                ),
            );
        }
    }
    if let Some(position) = entry.origin
        && matches!(visual.stage, Stage::Firing | Stage::Postfire)
    {
        let growth = rules.growth_frames.to_f32().expect("native update frame");
        let progress = (elapsed / growth.max(1.0)).min(1.0);
        let width = if entry.revealed || growth == 0.0 {
            1.0
        } else if visual.stage == Stage::Firing {
            progress
        } else {
            1.0 - progress
        };
        if width > 0.0 {
            beams.insert(
                (id, ordinal, 16),
                (
                    &rules.effects.orbital_laser,
                    [position, above(position)],
                    width,
                    elapsed,
                ),
            );
        }
    }
}

fn mesh(
    laser: &Laser,
    points: [[f32; 3]; 2],
    width: f32,
    elapsed: f32,
    camera: &Transform,
) -> Result<Mesh, &'static str> {
    let beams: Vec<_> = laser
        .beams
        .iter()
        .map(|beam| Beam {
            width: beam.width * width,
            color: beam.color,
        })
        .collect();
    let mut mesh = laser_beam::geometry(&beams, points, camera)?;
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
    else {
        return Err("Particle Uplink beam UVs missing");
    };
    let delta = std::array::from_fn::<_, 3, _>(|axis| points[1][axis] - points[0][axis]);
    let [x, y, z] = delta.map(|component| component * component);
    let length = (x + y + z).sqrt();
    let scroll = (elapsed * laser.scroll / 30.0).rem_euclid(1.0);
    for (beam, vertices) in beams.iter().zip(uv.chunks_exact_mut(4)) {
        let tile = laser.tile.map_or(1.0, |(aspect, tiling)| {
            (length / beam.width * aspect * tiling).clamp(0.0, 50.0)
        });
        for uv in vertices {
            let tiled = uv[1] * tile;
            uv[1] = tiled + scroll;
        }
    }
    Ok(mesh)
}

fn animated_bone(
    id: usize,
    bone: &str,
    game: &Skirmish,
    generation: u32,
    res: &Resources,
) -> Option<[f32; 3]> {
    let (_, player, matrix) = res
        .models
        .iter()
        .find(|(visual, _, _)| visual.index == id && visual.generation == generation)?;
    let p = res
        .animations
        .named_bone_position(&game.entities[id], player, bone)
        .expect("validated Particle Uplink bone animation")?;
    let point = matrix.transform_point(Vec3::new(p[0], p[2], -p[1]) * 0.1);
    Some([point.x * 10.0, -point.z * 10.0, point.y * 10.0])
}

fn world_matrix(game: &Skirmish, id: usize, matrix: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let (position, basis) = game.object_transform(id);
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let x = basis[row][0] * matrix[0][column];
            let y = basis[row][1] * matrix[1][column];
            let z = basis[row][2] * matrix[2][column];
            x + y + z + if column == 3 { position[row] } else { 0.0 }
        })
    })
}

const fn at(p: [f32; 3]) -> [[f32; 4]; 3] {
    [
        [1.0, 0.0, 0.0, p[0]],
        [0.0, 1.0, 0.0, p[1]],
        [0.0, 0.0, 1.0, p[2]],
    ]
}

fn above(mut position: [f32; 3]) -> [f32; 3] {
    position[2] += ORBITAL_BEAM_Z_OFFSET;
    position
}

fn drain_emitters(particles: &mut Particles, ids: &mut Vec<u64>) {
    for id in ids.drain(..) {
        particles.retire_optional_laser_emitters([Some(id), None]);
    }
}
