//! SH 2bfaea60 `W3DModelDraw::reactToTransformChange` and `W3DTerrainTracks`.
use super::{Animations, Clock, Players};
use crate::{
    playtest::Match,
    playtest_view::UnitVisual,
    skirmish_data::{definition, field, scalar},
    source_material::SourceMaterial,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    ecs::system::SystemParam,
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::{Skirmish, visibility::Status};
use num_traits::ToPrimitive;
use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    path::Path,
};

const LENGTH: f32 = 10.0;
const GROUND_OFFSET: f32 = 2.0;
const BRIDGE_OFFSET: f32 = 0.25;

struct Limits {
    tracks: usize,
    edges: usize,
    opaque: usize,
    fade_ms: u32,
}

struct Edge {
    endpoints: [Vec3; 2],
    time: u32,
    opaque: bool,
    v: f32,
}

struct Track {
    visual: Option<(Entity, Handle<Mesh>)>,
    owner: Option<(usize, usize)>,
    texture: String,
    width: f32,
    anchor: Option<Vec3>,
    edges: VecDeque<Edge>,
    odd: bool,
    airborne: bool,
    capped: bool,
    last_position: Option<([f32; 3], [[f32; 3]; 3])>,
}

impl Track {
    fn new(owner: (usize, usize), texture: &str, width: f32) -> Self {
        Self {
            visual: None,
            owner: Some(owner),
            texture: texture.into(),
            width,
            anchor: None,
            edges: VecDeque::new(),
            odd: false,
            airborne: false,
            capped: true,
            last_position: None,
        }
    }

    fn update(&mut self, sample: &TrackSample, time: u32, edge_limit: usize) {
        let moved = self.last_position != Some(sample.transform);
        self.last_position = Some(sample.transform);
        if matches!(sample.visibility, TrackVisibility::Hidden)
            || moved && matches!(sample.visibility, TrackVisibility::Obscured)
        {
            self.cap(sample.ground, sample.normal, time, edge_limit);
        } else if moved {
            self.airborne |= sample.airborne;
            self.add_edge(sample.ground, sample.normal, time, edge_limit);
        }
    }

    fn add_edge(&mut self, position: Vec3, normal: Vec3, time: u32, limit: usize) {
        let Some(anchor) = self.anchor else {
            self.anchor = Some(position);
            self.airborne = true;
            self.capped = true;
            return;
        };
        self.capped = false;
        let delta = position - anchor;
        if delta.length_squared() >= LENGTH * LENGTH {
            self.push_edge(
                position,
                delta,
                normal,
                time,
                !self.airborne && self.edges.len() > 1,
                limit,
            );
            self.airborne = false;
        }
    }

    fn cap(&mut self, position: Vec3, normal: Vec3, time: u32, limit: usize) {
        if self.capped {
            return;
        }
        if self.edges.len() != 1
            && let Some(anchor) = self.anchor
        {
            let delta = position - anchor;
            if delta.length_squared() >= LENGTH * LENGTH {
                self.push_edge(position, delta, normal, time, false, limit);
            } else if let Some(edge) = self.edges.back_mut() {
                edge.opaque = false;
            }
        }
        self.capped = true;
        self.anchor = None;
    }

    fn push_edge(
        &mut self,
        position: Vec3,
        delta: Vec3,
        normal: Vec3,
        time: u32,
        opaque: bool,
        limit: usize,
    ) {
        if self.edges.len() >= limit {
            self.edges.pop_front();
        }
        let direction = Vec3::new(delta.x, delta.y, 0.0).normalize_or_zero();
        let offset = (self.width * 0.5) * direction.cross(normal);
        let lifted = position + Vec3::Z * GROUND_OFFSET;
        self.edges.push_back(Edge {
            endpoints: [lifted - offset, lifted + offset],
            time,
            opaque,
            v: if self.odd { 0.0 } else { 1.0 },
        });
        self.odd = !self.odd;
        self.anchor = Some(position);
    }

    fn expire(&mut self, now: u32, fade_ms: u32) {
        // The pinned loop increments its index while decrementing active count.
        let mut scanned = 0;
        while scanned < self.edges.len()
            && self
                .edges
                .front()
                .is_some_and(|edge| now.wrapping_sub(edge.time) >= fade_ms)
        {
            self.edges.pop_front();
            scanned += 1;
        }
    }
}

struct TrackSample {
    transform: ([f32; 3], [[f32; 3]; 3]),
    ground: Vec3,
    normal: Vec3,
    visibility: TrackVisibility,
    airborne: bool,
}

enum TrackVisibility {
    Visible,
    Hidden,
    Obscured,
}

impl TrackSample {
    fn read(game: &Skirmish, id: usize, observer: usize) -> Self {
        let unit = &game.entities[id];
        let layer = unit.layer();
        let [x, y] = unit.position;
        let (height, normal) = game.track_height_and_normal(layer, unit.position);
        Self {
            transform: game.object_transform(id),
            ground: Vec3::new(x, y, height + if layer > 1 { BRIDGE_OFFSET } else { 0.0 }),
            normal: Vec3::from_array(normal),
            visibility: if unit.drawable_effectively_hidden() || unit.masked() {
                TrackVisibility::Hidden
            } else if unit.stealthed()
                || !matches!(
                    game.visibility_status(observer, id),
                    Status::Clear | Status::Partial
                )
            {
                TrackVisibility::Obscured
            } else {
                TrackVisibility::Visible
            },
            airborne: game.track_airborne(id),
        }
    }
}

struct TrackArt {
    image: Option<Image>,
    material: Option<Handle<SourceMaterial>>,
}

#[derive(Resource)]
pub struct TerrainTracks {
    limits: Limits,
    art: BTreeMap<String, TrackArt>,
    tracks: Vec<Track>,
    bound: BTreeMap<(usize, usize), usize>,
    attempted: BTreeMap<(usize, usize), (u64, usize, Option<usize>)>,
    generation: Option<u32>,
    frame: u32,
    previous_sync: u32,
    color: [f32; 3],
}

impl TerrainTracks {
    pub(crate) const fn set_color(&mut self, color: [f32; 3]) {
        self.color = color;
    }

    #[must_use]
    pub fn visible_edges(&self, object: usize) -> usize {
        self.tracks
            .iter()
            .filter(|track| track.owner.is_some_and(|(id, _)| id == object))
            .flat_map(|track| &track.edges)
            .filter(|edge| edge.opaque && self.frame.wrapping_sub(edge.time) < self.limits.fade_ms)
            .count()
    }

    /// Whether a strip using this object's authored texture is submitted.
    #[must_use]
    pub fn has_track_mesh(&self, object: usize) -> bool {
        self.tracks
            .iter()
            .filter(|track| track.owner.is_some_and(|(id, _)| id == object))
            .any(|track| track.visual.is_some())
    }

    fn unbind(&mut self, key: (usize, usize)) {
        if let Some(index) = self.bound.remove(&key) {
            self.tracks[index].owner = None;
        }
    }

    fn expire(&mut self, now: u32) {
        for track in &mut self.tracks {
            track.expire(now, self.limits.fade_ms);
        }
        self.tracks
            .retain(|track| track.owner.is_some() || !track.edges.is_empty());
        self.bound.clear();
        self.bound.extend(
            self.tracks
                .iter()
                .enumerate()
                .filter_map(|(index, track)| track.owner.map(|key| (key, index))),
        );
    }
}

pub fn load_tracks(
    assets: &VirtualAssets,
    root: &Path,
    animations: &Animations,
) -> Result<TerrainTracks, Box<dyn Error>> {
    let globals = [content::scan(assets.read("data/ini/gamedata.ini")?)?];
    let lines = &definition(&globals, "GameData", "")?.lines;
    let enabled = field(lines, 0, "MakeTrackMarks")
        .map(scalar)
        .transpose()?
        .is_some_and(|value| value.eq_ignore_ascii_case("Yes"));
    let max_tracks = field(lines, 0, "MaxTerrainTracks")
        .map(scalar)
        .transpose()?
        .map_or(Ok(0), str::parse)?;
    let mut limits = Limits {
        tracks: if enabled { max_tracks } else { 0 },
        edges: 100,
        opaque: 25,
        fade_ms: 300_000,
    };
    let options = match std::fs::read_to_string(root.join("UserData/Options.ini")) {
        Ok(options) => options,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let level = options
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| key.trim() == "StaticGameLOD")
        .map(|(_, level)| level.trim())
        .next_back();
    if let Some(level) = level.filter(|level| !level.eq_ignore_ascii_case("Custom")) {
        let settings = content::scan(assets.read("data/ini/gamelod.ini")?)?;
        for lod in settings
            .declarations
            .iter()
            .filter(|lod| lod.kind == "StaticGameLOD" && lod.name.eq_ignore_ascii_case(level))
        {
            if let Some(value) = field(&lod.lines, 0, "MaxTankTrackEdges") {
                limits.edges = scalar(value)?.parse()?;
            }
            if let Some(value) = field(&lod.lines, 0, "MaxTankTrackOpaqueEdges") {
                limits.opaque = scalar(value)?.parse()?;
            }
            if let Some(value) = field(&lod.lines, 0, "MaxTankTrackFadeDelay") {
                limits.fade_ms = scalar(value)?.parse()?;
            }
        }
    }
    if limits.edges < 2
        || limits.opaque > limits.edges
        || limits.fade_ms == 0
        || limits.tracks.saturating_mul(limits.edges).saturating_mul(2) >= 65_535
    {
        return Err("invalid original terrain track buffer limits".into());
    }
    let mut art = BTreeMap::new();
    if enabled {
        for draw in animations.draws.iter().flatten() {
            if let Some(name) = &draw.track
                && !art.contains_key(name)
            {
                let mut image = crate::source_texture::load(assets, name)?;
                image.texture_descriptor.format =
                    image.texture_descriptor.format.remove_srgb_suffix();
                art.insert(
                    name.clone(),
                    TrackArt {
                        image: Some(image),
                        material: None,
                    },
                );
            }
        }
    }
    Ok(TerrainTracks {
        limits,
        art,
        tracks: Vec::new(),
        bound: BTreeMap::new(),
        attempted: BTreeMap::new(),
        generation: None,
        frame: 0,
        previous_sync: 0,
        color: [1.0; 3],
    })
}

type TrackVisual = (&'static UnitVisual, &'static Players);

#[derive(Component)]
struct TrackVisualMesh;

#[derive(SystemParam)]
pub struct TrackArtResources<'w, 's> {
    meshes: ResMut<'w, Assets<Mesh>>,
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    visuals: Query<'w, 's, (Entity, &'static Mesh3d), With<TrackVisualMesh>>,
}

pub fn update_tracks(
    mut commands: Commands,
    battle: Res<Match>,
    clock: Res<Clock>,
    animations: Res<Animations>,
    tracks: Option<ResMut<TerrainTracks>>,
    visuals: Query<TrackVisual>,
    mut art: TrackArtResources,
) {
    let Some(mut tracks) = tracks else { return };
    let battle = battle.into_inner();
    let clock = clock.into_inner();
    let animations = animations.into_inner();
    if tracks.generation != Some(battle.generation) {
        tracks.generation = Some(battle.generation);
        tracks.tracks.clear();
        tracks.bound.clear();
        tracks.attempted.clear();
    }
    tracks.frame = clock.committed_milliseconds();
    tracks.previous_sync = clock.previous_sync_milliseconds();
    tracks.expire(clock.previous_sync_milliseconds());
    tracks.sync_drawables(battle, animations, &visuals);
    clear_replaced_meshes(&tracks, &mut commands, &art.visuals, &mut art.meshes);
    tracks.render(
        &mut commands,
        &mut art.meshes,
        &mut art.images,
        &mut art.materials,
    );
}

impl TerrainTracks {
    fn sync_drawables(
        &mut self,
        battle: &Match,
        animations: &Animations,
        visuals: &Query<TrackVisual>,
    ) {
        let tracks = self;
        let game = &battle.simulation;
        let mut live = BTreeMap::new();
        if battle.started && tracks.limits.tracks > 0 {
            for (visual, players) in visuals {
                if visual.generation != battle.generation {
                    continue;
                }
                let id = visual.index;
                let unit = &game.entities[id];
                if !unit.has_drawable() {
                    continue;
                }
                let draw_type = players.draw_type(unit.type_index);
                for (module, player) in players.0.iter().enumerate() {
                    let Some(draw) = animations.draws[draw_type].get(module) else {
                        continue;
                    };
                    let key = (id, module);
                    if let Some(texture) = draw
                        .track
                        .as_deref()
                        .filter(|texture| tracks.art.contains_key(*texture))
                    {
                        let model = draw
                            .states
                            .get(player.state)
                            .and_then(|state| state.render_model);
                        live.insert(key, (unit.drawable_birth(), draw_type, texture, model));
                    }
                }
            }
        }
        let removed: Vec<_> = tracks
            .bound
            .keys()
            .copied()
            .filter(|key| {
                live.get(key).is_none_or(|(birth, kind, _, model)| {
                    tracks.tracks[tracks.bound[key]].owner != Some(*key)
                        || tracks
                            .attempted
                            .get(key)
                            .is_some_and(|previous| previous != &(*birth, *kind, *model))
                })
            })
            .collect();
        for key in removed {
            tracks.unbind(key);
            tracks.attempted.remove(&key);
        }
        tracks.attempted.retain(|key, _| live.contains_key(key));
        let frame = tracks.frame;
        let limit = tracks.limits.edges;
        let mut live: Vec<_> = live.into_iter().collect();
        live.sort_unstable_by_key(|((_, module), (birth, _, _, _))| {
            (std::cmp::Reverse(*birth), *module)
        });
        for (key, (birth, kind, texture, model)) in live {
            let prior = tracks.attempted.insert(key, (birth, kind, model));
            let index = if let Some(&index) = tracks.bound.get(&key) {
                index
            } else {
                if prior == Some((birth, kind, model)) {
                    continue;
                }
                let Some(model) = model.filter(|_| tracks.tracks.len() < tracks.limits.tracks)
                else {
                    continue;
                };
                let model = &animations.models[model];
                let width = model
                    .bone_position("TREADFX01")
                    .zip(model.bone_position("TREADFX02"))
                    .map_or(14.0, |(left, right)| {
                        Vec3::from_array(left).distance(Vec3::from_array(right)) + 4.0
                    });
                let index = tracks.tracks.len();
                tracks.tracks.push(Track::new(key, texture, width));
                tracks.bound.insert(key, index);
                index
            };
            tracks.tracks[index].update(
                &TrackSample::read(game, key.0, battle.observer_player.unwrap_or(0)),
                frame,
                limit,
            );
        }
    }
}

fn clear_replaced_meshes(
    tracks: &TerrainTracks,
    commands: &mut Commands,
    visuals: &Query<(Entity, &Mesh3d), With<TrackVisualMesh>>,
    meshes: &mut Assets<Mesh>,
) {
    for (entity, mesh) in visuals {
        if !tracks.owns_mesh(&mesh.0) {
            commands.entity(entity).despawn();
            meshes.remove(&mesh.0);
        }
    }
}

impl TerrainTracks {
    fn owns_mesh(&self, mesh: &Handle<Mesh>) -> bool {
        self.tracks.iter().any(|track| {
            track
                .visual
                .as_ref()
                .is_some_and(|(_, handle)| handle == mesh)
        })
    }

    fn render(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
        materials: &mut Assets<SourceMaterial>,
    ) {
        for (order, track) in self.tracks.iter_mut().rev().enumerate() {
            let mesh = strip_mesh(
                std::iter::once(&*track),
                &self.limits,
                self.previous_sync,
                self.color,
            );
            let material = if mesh.indices().is_none_or(Indices::is_empty) {
                None
            } else {
                Some(
                    self.art
                        .get_mut(&track.texture)
                        .expect("loaded track art")
                        .material(images, materials),
                )
            };
            track.render(mesh, order, material, commands, meshes);
        }
    }
}

impl TrackArt {
    fn material(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<SourceMaterial>,
    ) -> Handle<SourceMaterial> {
        self.material
            .get_or_insert_with(|| {
                let image = images.add(self.image.take().expect("unuploaded track texture"));
                let mut material =
                    SourceMaterial::new([0.0; 3], [0.0; 3], [0.0; 3], 1.0, AlphaMode::Blend, false);
                material.parameters.vertex_diffuse = 4;
                material.parameters.shroud_mode = 1;
                material.texture = Some(image);
                materials.add(material)
            })
            .clone()
    }
}

impl Track {
    fn render(
        &mut self,
        mesh: Mesh,
        order: usize,
        material: Option<Handle<SourceMaterial>>,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
    ) {
        // Native fewer-than-two-edge strips submit no geometry. Empty uploads
        // are invalid for Bevy's allocator; retirement also permits later resume.
        let Some(material) = material else {
            if let Some((entity, handle)) = self.visual.take() {
                commands.entity(entity).despawn();
                meshes.remove(&handle);
            }
            return;
        };
        let order = crate::source_material::terrain::TrackOrder(order);
        if let Some((entity, handle)) = &self.visual {
            if let Some(mut current) = meshes.get_mut(handle) {
                *current = mesh;
            }
            commands.entity(*entity).insert(order);
            return;
        }
        let mesh = meshes.add(mesh);
        let entity = commands
            .spawn((
                TrackVisualMesh,
                order,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                NoFrustumCulling,
                RenderLayers::layer(crate::source_material::terrain::TRACK),
            ))
            .id();
        self.visual = Some((entity, mesh));
    }
}

fn strip_mesh<'a>(
    tracks: impl Iterator<Item = &'a Track>,
    limits: &Limits,
    frame: u32,
    color: [f32; 3],
) -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for track in tracks.filter(|track| track.edges.len() >= 2) {
        for (index, edge) in track.edges.iter().enumerate() {
            let age = i32::from_ne_bytes(frame.wrapping_sub(edge.time).to_ne_bytes())
                .max(0)
                .to_f32()
                .expect("WW3D elapsed time");
            let age_fade =
                (1.0 - age / limits.fade_ms.to_f32().expect("track fade milliseconds")).max(0.0);
            let from_end = track.edges.len() - index;
            let distance_fade = if from_end > limits.opaque {
                1.0 - (from_end - limits.opaque)
                    .to_f32()
                    .expect("track edge distance")
                    / (limits.edges - limits.opaque)
                        .to_f32()
                        .expect("track fade edges")
            } else {
                1.0
            };
            let alpha = if edge.opaque {
                (age_fade * distance_fade * 255.0).trunc() / 255.0
            } else {
                0.0
            };
            let base =
                u16::try_from(positions.len()).expect("validated native track index capacity");
            for point in edge.endpoints {
                positions.push([point.x * 0.1, point.z * 0.1, -point.y * 0.1]);
                colors.push([color[0], color[1], color[2], alpha]);
            }
            uvs.extend([[0.0, edge.v], [1.0, edge.v]]);
            if index > 0 {
                indices.extend([base - 2, base - 1, base + 1, base - 2, base + 1, base]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; colors.len()])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U16(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(x: f32, z: f32) -> TrackSample {
        TrackSample {
            transform: (
                [x, 0.0, z],
                [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            ),
            ground: Vec3::new(x, 0.0, z),
            normal: Vec3::Z,
            visibility: TrackVisibility::Visible,
            airborne: false,
        }
    }

    #[test]
    fn faded_track_mesh_retires_then_resumes_with_original_material() {
        let mut app = App::new();
        let name = "extiretrack03.tga";
        let mut track = Track::new((1, 0), name, 14.0);
        for (time, x) in [(0, 0.0), (20, 10.0), (40, 20.0), (60, 30.0)] {
            track.update(&sample(x, 0.0), time, 100);
        }
        app.insert_resource(TerrainTracks {
            limits: Limits {
                tracks: 100,
                edges: 100,
                opaque: 25,
                fade_ms: 60_000,
            },
            art: BTreeMap::from([(
                name.into(),
                TrackArt {
                    image: Some(Image::default()),
                    material: None,
                },
            )]),
            tracks: vec![track],
            bound: BTreeMap::from([((1, 0), 0)]),
            attempted: BTreeMap::new(),
            generation: Some(1),
            frame: 60,
            previous_sync: 60,
            color: [1.0; 3],
        })
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<SourceMaterial>>()
        .add_systems(
            Update,
            |mut tracks: ResMut<TerrainTracks>,
             mut commands: Commands,
             mut art: TrackArtResources| {
                let now = tracks.previous_sync;
                tracks.expire(now);
                tracks.render(
                    &mut commands,
                    &mut art.meshes,
                    &mut art.images,
                    &mut art.materials,
                );
            },
        );
        app.update();
        let (old_entity, old_mesh) = app.world().resource::<TerrainTracks>().tracks[0]
            .visual
            .clone()
            .unwrap();
        let original_material = app.world().resource::<TerrainTracks>().art[name]
            .material
            .clone()
            .unwrap();
        assert!(app.world().resource::<TerrainTracks>().has_track_mesh(1));
        app.world_mut()
            .resource_mut::<TerrainTracks>()
            .previous_sync = 60_100;
        app.update();
        assert!(!app.world().resource::<TerrainTracks>().has_track_mesh(1));
        assert!(app.world().get_entity(old_entity).is_err());
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&old_mesh)
                .is_none()
        );

        {
            let mut tracks = app.world_mut().resource_mut::<TerrainTracks>();
            tracks.previous_sync = 60_160;
            for (time, x) in [(60_120, 40.0), (60_140, 50.0), (60_160, 60.0)] {
                tracks.tracks[0].update(&sample(x, 0.0), time, 100);
            }
        }
        app.update();
        let world = app.world();
        let tracks = world.resource::<TerrainTracks>();
        let (new_entity, new_mesh) = tracks.tracks[0].visual.as_ref().unwrap();
        assert!(tracks.has_track_mesh(1));
        assert_ne!(*new_entity, old_entity);
        assert_ne!(new_mesh, &old_mesh);
        assert_eq!(
            world
                .get::<MeshMaterial3d<SourceMaterial>>(*new_entity)
                .unwrap()
                .0,
            original_material
        );
        assert_eq!(world.resource::<Assets<SourceMaterial>>().len(), 1);
        assert_eq!(world.resource::<Assets<Image>>().len(), 1);
    }

    #[test]
    fn replaced_map_retires_stale_track_meshes_and_preserves_current_ones() {
        let mut app = App::new();
        let mut meshes = Assets::<Mesh>::default();
        let limits = Limits {
            tracks: 1,
            edges: 5,
            opaque: 3,
            fade_ms: 200,
        };
        let old = meshes.add(strip_mesh(std::iter::empty(), &limits, 0, [1.0; 3]));
        let current = meshes.add(strip_mesh(std::iter::empty(), &limits, 0, [1.0; 3]));
        let old_entity = app
            .world_mut()
            .spawn((TrackVisualMesh, Mesh3d(old.clone())))
            .id();
        let current_entity = app
            .world_mut()
            .spawn((TrackVisualMesh, Mesh3d(current.clone())))
            .id();
        let tracks = TerrainTracks {
            limits,
            art: BTreeMap::from([(
                "extiretrack.tga".into(),
                TrackArt {
                    image: None,
                    material: None,
                },
            )]),
            tracks: {
                let mut track = Track::new((1, 0), "extiretrack.tga", 14.0);
                track.visual = Some((current_entity, current.clone()));
                vec![track]
            },
            bound: BTreeMap::new(),
            attempted: BTreeMap::new(),
            generation: Some(1),
            frame: 0,
            previous_sync: 0,
            color: [1.0; 3],
        };
        app.insert_resource(tracks)
            .insert_resource(meshes)
            .add_systems(
                Update,
                |tracks: Res<TerrainTracks>,
                 mut commands: Commands,
                 visuals: Query<(Entity, &Mesh3d), With<TrackVisualMesh>>,
                 mut meshes: ResMut<Assets<Mesh>>| {
                    clear_replaced_meshes(
                        tracks.into_inner(),
                        &mut commands,
                        &visuals,
                        &mut meshes,
                    );
                },
            );
        app.update();
        let world = app.world();
        assert!(world.get_entity(old_entity).is_err());
        assert!(world.get_entity(current_entity).is_ok());
        assert!(world.resource::<Assets<Mesh>>().get(&old).is_none());
        assert!(world.resource::<Assets<Mesh>>().get(&current).is_some());
    }

    #[test]
    fn track_edges_use_native_spacing_width_uv_and_caps() {
        let mut track = Track::new((4, 0), "extiretrack03.tga", 14.0);
        for (time, x) in [(0, 0.0), (33, 8.0), (66, 10.0), (99, 20.0), (133, 30.0)] {
            track.update(&sample(x, 0.0), time, 6);
        }
        assert_eq!(track.edges.len(), 3);
        let first = &track.edges[0];
        assert_eq!(
            first.endpoints,
            [Vec3::new(10.0, 7.0, 2.0), Vec3::new(10.0, -7.0, 2.0)]
        );
        assert_eq!(
            track
                .edges
                .iter()
                .map(|edge| edge.v.to_bits())
                .collect::<Vec<_>>(),
            [1.0_f32.to_bits(), 0.0_f32.to_bits(), 1.0_f32.to_bits()]
        );
        assert!(track.edges[2].opaque);
        let mut shrouded = sample(35.0, 0.0);
        shrouded.visibility = TrackVisibility::Obscured;
        track.update(&shrouded, 166, 6);
        assert!(track.anchor.is_none());
        assert!(!track.edges[2].opaque);
        track.update(&sample(150.0, 0.0), 199, 6);
        track.update(&sample(160.0, 0.0), 232, 6);
        assert!(!track.edges[3].opaque);
        assert_eq!(track.edges[3].endpoints[0].x.to_bits(), 160.0_f32.to_bits());
    }

    #[test]
    fn airborne_edge_fades_on_bridge_and_unbound_track_returns_to_pool() {
        let mut track = Track::new((1, 0), "extiretrack.tga", 14.0);
        let deck = 25.0 + BRIDGE_OFFSET;
        for (time, x) in [(0, 0.0), (20, 10.0), (40, 20.0), (60, 30.0)] {
            track.update(&sample(x, deck), time, 5);
        }
        let mut airborne = sample(40.0, deck);
        airborne.airborne = true;
        track.update(&airborne, 80, 5);
        assert!(track.edges[2].opaque);
        assert!(!track.edges[3].opaque);
        assert_eq!(
            track.edges[3].endpoints[0].z.to_bits(),
            (deck + GROUND_OFFSET).to_bits()
        );
        let mut tracks = TerrainTracks {
            limits: Limits {
                tracks: 1,
                edges: 5,
                opaque: 3,
                fade_ms: 200,
            },
            art: BTreeMap::new(),
            tracks: vec![track],
            bound: BTreeMap::from([((1, 0), 0)]),
            attempted: BTreeMap::new(),
            generation: Some(1),
            frame: 100,
            previous_sync: 100,
            color: [1.0; 3],
        };
        assert_eq!(tracks.visible_edges(1), 1);
        assert_eq!(
            strip_mesh(tracks.tracks.iter(), &tracks.limits, 100, tracks.color)
                .indices()
                .unwrap()
                .len(),
            18
        );
        tracks.unbind((1, 0));
        tracks.expire(300);
        tracks.expire(301);
        tracks.expire(302);
        assert!(tracks.tracks.is_empty());
    }
}
