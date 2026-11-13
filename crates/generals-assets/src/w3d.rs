//! Bounded W3D geometry, materials, hierarchy and raw/timecoded-animation loading.
//!
//! Mesh positions and normals are returned in the model's original Z-up frame.
//! Single-bone skin vertices initially use the hierarchy's base pose.
//! `Model::pose` evaluates raw translation/quaternion and visibility channels.
//! The first HLOD array supplies the mesh bindings.
//!
//! Skinning follows `MeshGeometryClass::read_vertex_influences` and
//! `get_deformed_vertices` in `SuperHackers` 2bfaea60, GeneralsMD/Code/Libraries/
//! Source/WWVegas/WW3D2/meshgeometry.cpp. Source copyright Electronic Arts
//! Inc. 2025, GPL-3.0-or-later. Adaptive-delta animation remains unsupported.

use std::fmt;

pub mod animation;
mod compressed;
pub mod emitter;
mod load;
pub mod material;
mod pose;
mod public_bones;
mod scene_bounds;
pub use load::read_asset;
pub use pose::{BoneControl, Deformation, Hierarchy, MeshPose};

const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_MESHES: usize = 2048;
const MAX_VERTICES: usize = 1_000_000;
const MAX_TRIANGLES: usize = 2_000_000;
const MAX_PIVOTS: usize = 4096;
const MAX_PARENT_DEPTH: usize = 128;
const CHUNK_HEADER_BYTES: usize = 8;
const CONTAINER_FLAG: u32 = 0x8000_0000;
const GEOMETRY_TYPE_MASK: u32 = 0x00ff_0000;
const GEOMETRY_TYPE_SKIN: u32 = 0x0002_0000;

/// A model's axis-aligned bounds in its original coordinate frame.
#[derive(Clone)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// A static mesh after applying its base-pose pivot chain.
pub struct Mesh {
    /// Qualified W3D name, such as `AVCRUSADER.CHASSIS`.
    pub name: String,
    /// Named hierarchy pivot to which this HLOD subobject is attached.
    pub bone_name: Option<String>,
    /// Original W3D attributes, including hidden and two-sided flags.
    pub attributes: u32,
    pub sort_level: i32,
    pub subobject_order: usize,
    /// Authored object-space volumes, before the HLOD attachment transform.
    pub bounds: Bounds,
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
    /// HLOD attachment axes and translation in the original Z-up frame.
    pub bound_transform: [[f32; 3]; 4],
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub materials: Option<material::Materials>,
    /// Original model-space geometry and its rigid/skin bone bindings.
    pub deformation: Option<Deformation>,
}

/// Native object-space sphere before the placed model's authored scale.
pub struct Sphere {
    pub center: [f32; 3],
    pub radius: f32,
}

/// Static geometry from a W3D file.
pub struct Model {
    /// Actual HLOD prototype (`W3D_CHUNK_HLOD`), not merely an `HTree`.
    pub hlod: bool,
    /// A real standalone WW3D emitter prototype, distinct from empty/NULL geometry.
    pub emitter: Option<emitter::Definition>,
    /// Exact prototype sphere when every scene subobject's metadata is present.
    /// External/nested object metadata is required by the actual PROP consumer.
    pub scene_sphere: Option<Sphere>,
    /// Original HLOD base-pose header/box union used for scene visibility.
    pub scene_bounds: Bounds,
    pub meshes: Vec<Mesh>,
    /// Source hierarchy order; index zero is the root, not a weapon attachment.
    pub bone_names: Vec<String>,
    pub bounds: Bounds,
    pub hierarchy: Option<Hierarchy>,
}

/// A malformed or unsupported W3D input.
#[derive(Debug)]
pub enum Error {
    Truncated,
    InvalidChunk(&'static str),
    LimitExceeded(&'static str),
    Unsupported(&'static str),
    NonFinite,
    IndexOutOfBounds { index: u32, vertices: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("truncated W3D data"),
            Self::InvalidChunk(reason) => write!(formatter, "invalid W3D chunk: {reason}"),
            Self::LimitExceeded(kind) => write!(formatter, "W3D {kind} exceeds the parser limit"),
            Self::Unsupported(kind) => write!(formatter, "unsupported W3D {kind}"),
            Self::NonFinite => formatter.write_str("non-finite W3D geometry"),
            Self::IndexOutOfBounds { index, vertices } => {
                write!(
                    formatter,
                    "W3D triangle vertex {index} exceeds {vertices} vertices"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

impl Model {
    /// Native built-in `NULL` prototype: no draw geometry, with its own bounds.
    /// `Null3DObjClass` keeps a radius0.1 sphere and box half extents0.1.
    #[must_use]
    pub const fn null() -> Self {
        Self {
            hlod: false,
            emitter: None,
            scene_sphere: Some(Sphere {
                center: [0.0; 3],
                radius: 0.1,
            }),
            scene_bounds: Bounds {
                min: [-0.1; 3],
                max: [0.1; 3],
            },
            meshes: Vec::new(),
            bone_names: Vec::new(),
            bounds: Bounds {
                min: [-0.1; 3],
                max: [0.1; 3],
            },
            hierarchy: None,
        }
    }

    /// Reads static meshes and the first HLOD's base-pose hierarchy bindings.
    ///
    /// # Errors
    /// Rejects truncated chunks, invalid geometry counts or indices, non-finite
    /// values, malformed or missing hierarchies, and excessive allocations.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::parse_with_hierarchy(bytes, None)
    }

    /// Returns the HLOD hierarchy name when it is not embedded in this file.
    ///
    /// # Errors
    /// Rejects oversized input and malformed or duplicate hierarchy/HLOD chunks.
    pub fn hierarchy_name(bytes: &[u8]) -> Result<Option<String>, Error> {
        let embedded = top_level_chunk(bytes, 0x100)?;
        let lod = top_level_chunk(bytes, 0x700)?;
        let name = lod.map(hlod_hierarchy_name).transpose()?;
        Ok(name.filter(|name| embedded.is_none() && !name.is_empty()))
    }

    fn parse_emitter(data: &[u8]) -> Result<Self, Error> {
        // ParticleEmitterClass::Update_Cached_Bounding_Volumes uses a point;
        // its separate live ParticleBuffer owns the dynamic particle bounds.
        Ok(Self {
            hlod: false,
            emitter: Some(emitter::parse(data)?),
            scene_sphere: Some(Sphere {
                center: [0.0; 3],
                radius: 0.0,
            }),
            scene_bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            meshes: Vec::new(),
            bone_names: Vec::new(),
            bounds: Bounds {
                min: [0.0; 3],
                max: [0.0; 3],
            },
            hierarchy: None,
        })
    }

    /// Reads geometry in its base pose, using an external W3D hierarchy if needed.
    ///
    /// # Errors
    /// Rejects malformed geometry or hierarchies, mismatched hierarchy names,
    /// missing or invalid skin bone indices, and excessive allocations.
    pub fn parse_with_hierarchy(
        bytes: &[u8],
        external_hierarchy: Option<&[u8]>,
    ) -> Result<Self, Error> {
        Self::parse_with_absent(bytes, external_hierarchy, &[])
    }

    fn parse_with_absent(
        bytes: &[u8],
        external_hierarchy: Option<&[u8]>,
        absent: &[String],
    ) -> Result<Self, Error> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(Error::LimitExceeded("file size"));
        }
        let mut emitter_data = None;
        let mut emitter_companion = false;
        let mut mesh_data = Vec::new();
        let mut boxes = Vec::new();
        let mut hierarchy = None;
        let mut lod = None;
        for chunk in Chunks::new(bytes) {
            let chunk = chunk?;
            match chunk.id {
                0 => {
                    chunk.require_container()?;
                    if mesh_data.len() == MAX_MESHES {
                        return Err(Error::LimitExceeded("mesh count"));
                    }
                    mesh_data.push(chunk.body);
                }
                0x500 => {
                    chunk.require_container()?;
                    set_once(&mut emitter_data, chunk.body)?;
                }
                0x740 => boxes.push(chunk.body),
                0x100 => {
                    chunk.require_container()?;
                    set_once(&mut hierarchy, chunk.body)?;
                }
                0x700 => {
                    chunk.require_container()?;
                    set_once(&mut lod, chunk.body)?;
                }
                _ => emitter_companion = true,
            }
        }
        if let Some(data) = emitter_data {
            if !mesh_data.is_empty()
                || !boxes.is_empty()
                || hierarchy.is_some()
                || lod.is_some()
                || external_hierarchy.is_some()
                || emitter_companion
            {
                return Err(Error::Unsupported("emitter combined with scene subobjects"));
            }
            return Self::parse_emitter(data);
        }
        if hierarchy.is_none() {
            hierarchy = external_hierarchy
                .map(|data| top_level_chunk(data, 0x100))
                .transpose()?
                .flatten();
        }
        if let Some(lod) = lod {
            let expected = hlod_hierarchy_name(lod)?;
            if !expected.is_empty() {
                let data = hierarchy.ok_or(Error::InvalidChunk("missing model hierarchy"))?;
                let header = hierarchy_header(data)?;
                if !read_name(&header[4..20])?.eq_ignore_ascii_case(&expected) {
                    return Err(Error::InvalidChunk("hierarchy name does not match HLOD"));
                }
            }
        }
        let hierarchy_name = hierarchy
            .map(hierarchy_header)
            .transpose()?
            .map(|header| read_name(&header[4..20]))
            .transpose()?;
        let pivots = hierarchy
            .map(parse_hierarchy)
            .transpose()?
            .unwrap_or_default();
        let mut meshes = mesh_data
            .into_iter()
            .map(|data| parse_mesh(data, &pivots))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, mesh) in meshes.iter_mut().enumerate() {
            mesh.subobject_order = index;
        }
        let bindings = lod.map(parse_bindings).transpose()?;
        if let Some(bindings) = &bindings {
            apply_bindings(&mut meshes, bindings, &pivots)?;
        }
        let scene_bounds = scene_bounds::load(&meshes, &boxes, bindings.as_deref(), &pivots)?;
        let scene_sphere = scene_bounds::sphere(&meshes, &boxes, lod, &pivots, absent);
        // A valid HLOD can consist entirely of a hierarchy and non-rendering
        // subobjects, as in original mine groups. Its scene volumes still exist.
        let bounds = if meshes.is_empty() && lod.is_some() {
            scene_bounds.clone()
        } else {
            bounds(&meshes)?
        };
        Ok(Self {
            hlod: lod.is_some(),
            emitter: None,
            scene_sphere,
            scene_bounds,
            meshes,
            bone_names: pivots.iter().map(|pivot| pivot.name.clone()).collect(),
            bounds,
            hierarchy: hierarchy_name.map(|name| Hierarchy::new(name, pivots)),
        })
    }
}

fn top_level_chunk(bytes: &[u8], id: u32) -> Result<Option<&[u8]>, Error> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::LimitExceeded("file size"));
    }
    let mut found = None;
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id == id {
            chunk.require_container()?;
            set_once(&mut found, chunk.body)?;
        }
    }
    Ok(found)
}

fn hlod_hierarchy_name(bytes: &[u8]) -> Result<String, Error> {
    let mut header = None;
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id == 0x701 {
            set_once(&mut header, chunk.body)?;
        }
    }
    let header = header.ok_or(Error::InvalidChunk("missing HLOD header"))?;
    if header.len() != 40 {
        return Err(Error::Unsupported("HLOD header size"));
    }
    read_name(&header[24..40])
}

struct Chunk<'a> {
    id: u32,
    body: &'a [u8],
    container: bool,
}

impl Chunk<'_> {
    const fn require_container(&self) -> Result<(), Error> {
        if self.container {
            Ok(())
        } else {
            Err(Error::InvalidChunk("missing container flag"))
        }
    }
}

struct Chunks<'a> {
    remaining: &'a [u8],
}

impl<'a> Chunks<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let input = std::mem::take(&mut self.remaining);
        let result = (|| {
            let id = read_u32(input, 0)?;
            let size_flags = read_u32(input, 4)?;
            let size = usize::try_from(size_flags & !CONTAINER_FLAG)
                .map_err(|_| Error::LimitExceeded("chunk size"))?;
            let end = CHUNK_HEADER_BYTES
                .checked_add(size)
                .ok_or(Error::Truncated)?;
            let body = input.get(CHUNK_HEADER_BYTES..end).ok_or(Error::Truncated)?;
            self.remaining = input.get(end..).ok_or(Error::Truncated)?;
            Ok(Chunk {
                id,
                body,
                container: size_flags & CONTAINER_FLAG != 0,
            })
        })();
        Some(result)
    }
}

const fn set_once<'a>(slot: &mut Option<&'a [u8]>, bytes: &'a [u8]) -> Result<(), Error> {
    if slot.replace(bytes).is_some() {
        Err(Error::InvalidChunk("duplicate required chunk"))
    } else {
        Ok(())
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset.checked_add(4).ok_or(Error::Truncated)?;
    let value: [u8; 4] = bytes
        .get(offset..end)
        .ok_or(Error::Truncated)?
        .try_into()
        .map_err(|_| Error::Truncated)?;
    Ok(u32::from_le_bytes(value))
}

fn read_float(bytes: &[u8], offset: usize) -> Result<f32, Error> {
    let value = f32::from_bits(read_u32(bytes, offset)?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::NonFinite)
    }
}

fn read_vector(bytes: &[u8], offset: usize) -> Result<[f32; 3], Error> {
    Ok([
        read_float(bytes, offset)?,
        read_float(bytes, offset + 4)?,
        read_float(bytes, offset + 8)?,
    ])
}

fn read_name(bytes: &[u8]) -> Result<String, Error> {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    let name =
        std::str::from_utf8(&bytes[..end]).map_err(|_| Error::Unsupported("non-UTF-8 name"))?;
    Ok(name.to_owned())
}

fn read_count(
    bytes: &[u8],
    offset: usize,
    limit: usize,
    kind: &'static str,
) -> Result<usize, Error> {
    let count =
        usize::try_from(read_u32(bytes, offset)?).map_err(|_| Error::LimitExceeded(kind))?;
    if count > limit {
        Err(Error::LimitExceeded(kind))
    } else {
        Ok(count)
    }
}

fn vectors(bytes: &[u8], count: usize) -> Result<Vec<[f32; 3]>, Error> {
    if bytes.len() != count * 12 {
        return Err(Error::InvalidChunk("vertex count does not match data"));
    }
    bytes
        .chunks_exact(12)
        .map(|item| read_vector(item, 0))
        .collect()
}

fn parse_mesh(bytes: &[u8], pivots: &[Pivot]) -> Result<Mesh, Error> {
    let (mut header, mut positions, mut normals, mut triangles) = (None, None, None, None);
    let mut influences = None;
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0x1f => set_once(&mut header, chunk.body)?,
            2 => set_once(&mut positions, chunk.body)?,
            3 => set_once(&mut normals, chunk.body)?,
            0x0e => set_once(&mut influences, chunk.body)?,
            0x20 => set_once(&mut triangles, chunk.body)?,
            _ => {}
        }
    }
    let header = header.ok_or(Error::InvalidChunk("missing mesh header"))?;
    if header.len() != 116 {
        return Err(Error::Unsupported("mesh header size"));
    }
    let attributes = read_u32(header, 4)?;
    let geometry_type = attributes & GEOMETRY_TYPE_MASK;
    if !matches!(geometry_type, 0 | GEOMETRY_TYPE_SKIN) || attributes & 2 != 0 {
        return Err(Error::Unsupported("mesh geometry type"));
    }
    let short_name = read_name(&header[8..24])?;
    let container = read_name(&header[24..40])?;
    let name = if container.is_empty() {
        short_name
    } else {
        format!("{container}.{short_name}")
    };
    let triangle_count = read_count(header, 40, MAX_TRIANGLES, "triangle count")?;
    let vertex_count = read_count(header, 44, MAX_VERTICES, "vertex count")?;
    let positions = vectors(
        positions.ok_or(Error::InvalidChunk("missing vertices"))?,
        vertex_count,
    )?;
    let normals = normals
        .map(|data| vectors(data, vertex_count))
        .transpose()?;
    let triangles = triangles.ok_or(Error::InvalidChunk("missing triangles"))?;
    if triangles.len() != triangle_count * 32 {
        return Err(Error::InvalidChunk("triangle count does not match data"));
    }
    let mut indices = Vec::with_capacity(triangle_count * 3);
    for triangle in triangles.chunks_exact(32) {
        for offset in [0, 4, 8] {
            let index = read_u32(triangle, offset)?;
            if usize::try_from(index).map_or(true, |value| value >= vertex_count) {
                return Err(Error::IndexOutOfBounds {
                    index,
                    vertices: vertex_count,
                });
            }
            indices.push(index);
        }
    }
    let mut mesh = Mesh {
        name,
        bone_name: None,
        attributes,
        subobject_order: 0,
        sort_level: i32::from_le_bytes(read_u32(header, 56)?.to_le_bytes()),
        bounds: Bounds {
            min: read_vector(header, 76)?,
            max: read_vector(header, 88)?,
        },
        sphere_center: read_vector(header, 100)?,
        sphere_radius: read_float(header, 112)?,
        bound_transform: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
        positions,
        indices,
        normals,
        materials: material::parse(bytes, vertex_count, triangle_count)?,
        deformation: None,
    };
    if geometry_type == GEOMETRY_TYPE_SKIN {
        apply_skin(
            &mut mesh,
            influences.ok_or(Error::InvalidChunk("missing vertex influences"))?,
            pivots,
        )?;
    } else if influences.is_some() {
        return Err(Error::InvalidChunk(
            "vertex influences on non-skin geometry",
        ));
    }
    Ok(mesh)
}

struct Pivot {
    name: String,
    parent: Option<usize>,
    translation: [f32; 3],
    rotation: [f32; 4],
}

fn hierarchy_header(bytes: &[u8]) -> Result<&[u8], Error> {
    let mut header = None;
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id == 0x101 {
            set_once(&mut header, chunk.body)?;
        }
    }
    let header = header.ok_or(Error::InvalidChunk("missing hierarchy header"))?;
    if header.len() != 36 {
        return Err(Error::Unsupported("hierarchy header size"));
    }
    Ok(header)
}

fn parse_hierarchy(bytes: &[u8]) -> Result<Vec<Pivot>, Error> {
    let header = hierarchy_header(bytes)?;
    let mut pivots = None;
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id == 0x102 {
            set_once(&mut pivots, chunk.body)?;
        }
    }
    let count = read_count(header, 20, MAX_PIVOTS, "pivot count")?;
    let data = pivots.ok_or(Error::InvalidChunk("missing pivots"))?;
    if data.len() != count * 60 {
        return Err(Error::InvalidChunk("pivot count does not match data"));
    }
    let mut result = Vec::with_capacity(count);
    for (pivot_index, entry) in data.chunks_exact(60).enumerate() {
        let parent = read_u32(entry, 16)?;
        let parent = if parent == u32::MAX {
            None
        } else {
            let index =
                usize::try_from(parent).map_err(|_| Error::InvalidChunk("invalid parent pivot"))?;
            if index >= count {
                return Err(Error::InvalidChunk("invalid parent pivot"));
            }
            Some(index)
        };
        if (pivot_index == 0) != parent.is_none() {
            return Err(Error::InvalidChunk("invalid hierarchy root"));
        }
        let translation = read_vector(entry, 20)?;
        let mut rotation = [
            read_float(entry, 44)?,
            read_float(entry, 48)?,
            read_float(entry, 52)?,
            read_float(entry, 56)?,
        ];
        let length = rotation
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !length.is_finite() || length <= f32::EPSILON {
            return Err(Error::InvalidChunk("invalid pivot rotation"));
        }
        for value in &mut rotation {
            *value /= length;
        }
        result.push(Pivot {
            name: read_name(&entry[..16])?,
            parent,
            translation,
            rotation,
        });
    }
    for index in 0..result.len() {
        pivot_chain(index, &result)?;
    }
    Ok(result)
}

fn parse_bindings(bytes: &[u8]) -> Result<Vec<(String, usize)>, Error> {
    let mut first_lod = None;
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id == 0x702 && first_lod.is_none() {
            chunk.require_container()?;
            first_lod = Some(chunk.body);
        }
    }
    let data = first_lod.ok_or(Error::InvalidChunk("missing HLOD array"))?;
    let mut bindings = Vec::new();
    let mut expected = None;
    for chunk in Chunks::new(data) {
        let chunk = chunk?;
        match chunk.id {
            0x703 => {
                if expected.is_some() {
                    return Err(Error::InvalidChunk("duplicate HLOD count"));
                }
                expected = Some(read_count(chunk.body, 0, MAX_MESHES, "HLOD count")?);
            }
            0x704 => {
                if chunk.body.len() != 36 {
                    return Err(Error::InvalidChunk("invalid HLOD binding"));
                }
                if bindings.len() == MAX_MESHES {
                    return Err(Error::LimitExceeded("HLOD count"));
                }
                let pivot = usize::try_from(read_u32(chunk.body, 0)?)
                    .map_err(|_| Error::InvalidChunk("invalid HLOD pivot"))?;
                bindings.push((read_name(&chunk.body[4..])?, pivot));
            }
            _ => {}
        }
    }
    if expected != Some(bindings.len()) {
        return Err(Error::InvalidChunk("HLOD count does not match data"));
    }
    Ok(bindings)
}

fn pivot_chain(index: usize, pivots: &[Pivot]) -> Result<Vec<&Pivot>, Error> {
    let mut chain = Vec::new();
    let mut cursor = Some(index);
    while let Some(index) = cursor {
        if chain.len() == MAX_PARENT_DEPTH {
            return Err(Error::InvalidChunk("cyclic or excessively deep hierarchy"));
        }
        let pivot = pivots
            .get(index)
            .ok_or(Error::InvalidChunk("missing bound pivot"))?;
        // HTreeClass::Base_Update replaces pivot zero with the model transform.
        // Vertices returned here remain in model space, so that transform is identity.
        if index == 0 {
            break;
        }
        chain.push(pivot);
        cursor = pivot.parent;
    }
    Ok(chain)
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1].mul_add(right[2], -left[2] * right[1]),
        left[2].mul_add(right[0], -left[0] * right[2]),
        left[0].mul_add(right[1], -left[1] * right[0]),
    ]
}

fn rotate(vector: [f32; 3], quaternion: [f32; 4]) -> [f32; 3] {
    let axis = [quaternion[0], quaternion[1], quaternion[2]];
    let twice_cross = cross(axis, vector).map(|value| 2.0 * value);
    let correction = cross(axis, twice_cross);
    std::array::from_fn(|index| {
        quaternion[3].mul_add(twice_cross[index], vector[index]) + correction[index]
    })
}

fn apply_skin(mesh: &mut Mesh, influences: &[u8], pivots: &[Pivot]) -> Result<(), Error> {
    if influences.len() != mesh.positions.len() * 8 {
        return Err(Error::InvalidChunk(
            "vertex influence count does not match data",
        ));
    }
    mesh.deformation = Some(Deformation::skin(mesh, influences));
    for (index, influence) in influences.chunks_exact(8).enumerate() {
        let bone = usize::from(u16::from_le_bytes([influence[0], influence[1]]));
        for pivot in pivot_chain(bone, pivots)? {
            let vertex = &mut mesh.positions[index];
            *vertex = rotate(*vertex, pivot.rotation);
            for (component, translation) in vertex.iter_mut().zip(pivot.translation) {
                *component += translation;
            }
            if let Some(normals) = &mut mesh.normals {
                normals[index] = rotate(normals[index], pivot.rotation);
                if normals[index].iter().any(|value| !value.is_finite()) {
                    return Err(Error::NonFinite);
                }
            }
        }
    }
    Ok(())
}

fn base_bound_transform(index: usize, pivots: &[Pivot]) -> Result<[[f32; 3]; 4], Error> {
    let mut transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]];
    for pivot in pivot_chain(index, pivots)? {
        for axis in &mut transform {
            *axis = rotate(*axis, pivot.rotation);
        }
        for (component, translation) in transform[3].iter_mut().zip(pivot.translation) {
            *component += translation;
        }
    }
    Ok(transform)
}

fn apply_bindings(
    meshes: &mut [Mesh],
    bindings: &[(String, usize)],
    pivots: &[Pivot],
) -> Result<(), Error> {
    for mesh in meshes {
        let mut matching = bindings
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(&mesh.name));
        let Some((_, index)) = matching.next() else {
            return Err(Error::InvalidChunk("mesh has no HLOD binding"));
        };
        if matching.next().is_some() {
            return Err(Error::InvalidChunk("duplicate mesh binding"));
        }
        mesh.subobject_order = bindings
            .iter()
            .position(|(name, _)| name.eq_ignore_ascii_case(&mesh.name))
            .expect("validated HLOD binding");
        mesh.bone_name = Some(
            pivots
                .get(*index)
                .ok_or(Error::InvalidChunk("missing bound pivot"))?
                .name
                .clone(),
        );
        mesh.bound_transform = base_bound_transform(*index, pivots)?;
        if mesh.attributes & GEOMETRY_TYPE_MASK == GEOMETRY_TYPE_SKIN {
            if *index >= pivots.len() {
                return Err(Error::InvalidChunk("missing bound pivot"));
            }
            continue;
        }
        mesh.deformation = Some(Deformation::rigid(mesh, *index));
        for pivot in pivot_chain(*index, pivots)? {
            for vertex in &mut mesh.positions {
                *vertex = rotate(*vertex, pivot.rotation);
                for (component, translation) in vertex.iter_mut().zip(pivot.translation) {
                    *component += translation;
                }
            }
            if let Some(normals) = &mut mesh.normals {
                for normal in normals {
                    *normal = rotate(*normal, pivot.rotation);
                    if normal.iter().any(|value| !value.is_finite()) {
                        return Err(Error::NonFinite);
                    }
                }
            }
        }
    }
    Ok(())
}

fn bounds(meshes: &[Mesh]) -> Result<Bounds, Error> {
    let mut vertices = meshes.iter().flat_map(|mesh| &mesh.positions);
    let first = *vertices
        .next()
        .ok_or(Error::InvalidChunk("model has no vertices"))?;
    let mut bounds = Bounds {
        min: first,
        max: first,
    };
    for vertex in std::iter::once(&first).chain(vertices) {
        for (index, value) in vertex.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(Error::NonFinite);
            }
            bounds.min[index] = bounds.min[index].min(value);
            bounds.max[index] = bounds.max[index].max(value);
        }
    }
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::{Error, Model};

    fn chunk(id: u32, body: &[u8], container: bool) -> Vec<u8> {
        let mut data = id.to_le_bytes().to_vec();
        let size = u32::try_from(body.len()).expect("fixture fits u32");
        data.extend_from_slice(&(size | if container { 0x8000_0000 } else { 0 }).to_le_bytes());
        data.extend_from_slice(body);
        data
    }

    fn triangle_model(indices: [u32; 3], triangle_bytes: usize) -> Vec<u8> {
        let mut header = vec![0; 116];
        header[40..44].copy_from_slice(&1_u32.to_le_bytes());
        header[44..48].copy_from_slice(&3_u32.to_le_bytes());
        let mut body = chunk(0x1f, &header, false);
        body.extend(chunk(2, &[0; 36], false));
        let mut triangle = [0; 32];
        for (offset, index) in indices.into_iter().enumerate() {
            triangle[offset * 4..offset * 4 + 4].copy_from_slice(&index.to_le_bytes());
        }
        body.extend(chunk(0x20, &triangle[..triangle_bytes], false));
        chunk(0, &body, true)
    }

    #[test]
    fn mesh_header_retains_signed_sort_level_and_original_whole_mesh_bounds() {
        let mut bytes = triangle_model([0, 1, 2], 32);
        let header = 16;
        bytes[header + 56..header + 60].copy_from_slice(&(-2_i32).to_le_bytes());
        for (offset, point) in [
            (76, [-4.0_f32, -3.0, -2.0]),
            (88, [6.0, 7.0, 8.0]),
            (100, [1.0, 2.0, 3.0]),
        ] {
            for (axis, value) in point.into_iter().enumerate() {
                let start = header + offset + axis * 4;
                bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        bytes[header + 112..header + 116].copy_from_slice(&9.0_f32.to_le_bytes());
        let model = Model::parse(&bytes).unwrap();
        let sphere = model.scene_sphere.as_ref().unwrap();
        assert_eq!(
            sphere.center.map(f32::to_bits),
            [1.0_f32, 2.0, 3.0].map(f32::to_bits)
        );
        assert_eq!(sphere.radius.to_bits(), 9.0_f32.to_bits());
        let mesh = &model.meshes[0];
        assert_eq!(mesh.sort_level, -2);
        assert_eq!(
            mesh.bounds.min.map(f32::to_bits),
            [-4.0_f32, -3.0, -2.0].map(f32::to_bits)
        );
        assert_eq!(
            mesh.bounds.max.map(f32::to_bits),
            [6.0_f32, 7.0, 8.0].map(f32::to_bits)
        );
        assert_eq!(
            mesh.sphere_center.map(f32::to_bits),
            [1.0_f32, 2.0, 3.0].map(f32::to_bits)
        );
        bytes[header + 100..header + 104].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(matches!(Model::parse(&bytes), Err(Error::NonFinite)));
    }

    #[test]
    fn declared_chunk_size_cannot_escape_input() {
        let bytes = [0, 0, 0, 0, 255, 255, 255, 255];
        assert!(matches!(Model::parse(&bytes), Err(Error::Truncated)));
    }

    #[test]
    fn nonfinite_positions_normals_and_bounds_remain_rejected() {
        let original = triangle_model([0, 1, 2], 32);
        let mut positions = original[8..].to_vec();
        let first_position = super::CHUNK_HEADER_BYTES + 116 + super::CHUNK_HEADER_BYTES;
        positions[first_position..first_position + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            Model::parse(&chunk(0, &positions, true)),
            Err(Error::NonFinite)
        ));
        let mut normals = [0; 36];
        normals[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut body = original[8..].to_vec();
        body.extend(chunk(3, &normals, false));
        assert!(matches!(
            Model::parse(&chunk(0, &body, true)),
            Err(Error::NonFinite)
        ));
        let mut model = Model::parse(&original).unwrap();
        model.meshes[0].positions[0][0] = f32::NAN;
        assert!(matches!(
            super::bounds(&model.meshes),
            Err(Error::NonFinite)
        ));
    }

    #[test]
    fn truncated_triangle_record_is_rejected() {
        let bytes = triangle_model([0, 1, 2], 31);
        assert!(matches!(Model::parse(&bytes), Err(Error::InvalidChunk(_))));
    }

    #[test]
    fn triangle_cannot_reference_vertex_outside_mesh() {
        let bytes = triangle_model([0, 1, 3], 32);
        assert!(matches!(
            Model::parse(&bytes),
            Err(Error::IndexOutOfBounds {
                index: 3,
                vertices: 3
            })
        ));
    }

    fn skin_model(bones: &[u16]) -> Vec<u8> {
        let mut mesh = triangle_model([0, 1, 2], 32)[8..].to_vec();
        mesh[12..16].copy_from_slice(&super::GEOMETRY_TYPE_SKIN.to_le_bytes());
        mesh[16..20].copy_from_slice(b"Skin");
        mesh[32..36].copy_from_slice(b"Test");
        mesh[144..148].copy_from_slice(&1.0_f32.to_le_bytes());
        let mut influences = Vec::new();
        for bone in bones {
            influences.extend(bone.to_le_bytes());
            influences.extend([0; 6]);
        }
        mesh.extend(chunk(0x0e, &influences, false));
        let normals: Vec<_> = [1.0_f32, 0.0, 0.0]
            .repeat(3)
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        mesh.extend(chunk(3, &normals, false));
        let mut header = [0; 40];
        header[4..8].copy_from_slice(&1_u32.to_le_bytes());
        header[24..31].copy_from_slice(b"Fixture");
        let mut lod = chunk(0x701, &header, false);
        let mut array = chunk(0x703, &1_u32.to_le_bytes(), false);
        let mut binding = [0; 36];
        binding[4..13].copy_from_slice(b"Test.Skin");
        array.extend(chunk(0x704, &binding, false));
        lod.extend(chunk(0x702, &array, true));
        let mut data = chunk(0, &mesh, true);
        data.extend(chunk(0x700, &lod, true));
        data
    }

    fn skeleton(name: &str) -> Vec<u8> {
        let mut header = [0; 36];
        header[4..4 + name.len()].copy_from_slice(name.as_bytes());
        header[20..24].copy_from_slice(&2_u32.to_le_bytes());
        let mut pivots = [0; 120];
        pivots[..4].copy_from_slice(b"ROOT");
        pivots[60..69].copy_from_slice(b"MuzzleFX1");
        pivots[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        pivots[20..24].copy_from_slice(&100.0_f32.to_le_bytes());
        pivots[56..60].copy_from_slice(&1.0_f32.to_le_bytes());
        for (offset, value) in [(80, 10.0_f32), (84, 20.0), (88, 30.0)] {
            pivots[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let half_turn = std::f32::consts::FRAC_1_SQRT_2.to_le_bytes();
        pivots[112..116].copy_from_slice(&half_turn);
        pivots[116..120].copy_from_slice(&half_turn);
        let mut body = chunk(0x101, &header, false);
        body.extend(chunk(0x102, &pivots, false));
        chunk(0x100, &body, true)
    }

    #[test]
    fn external_skin_uses_vertex_bones_and_rotates_normals_without_translation() {
        let bytes = skin_model(&[0, 1, 1]);
        assert_eq!(
            Model::hierarchy_name(&bytes).unwrap().as_deref(),
            Some("Fixture")
        );
        let hierarchy = skeleton("Fixture");
        let model = Model::parse_with_hierarchy(&bytes, Some(&hierarchy)).unwrap();
        let mesh = &model.meshes[0];
        assert_eq!(mesh.bone_name.as_deref(), Some("ROOT"));
        assert_eq!(model.bone_names, ["ROOT", "MuzzleFX1"]);
        for (actual, expected) in
            mesh.positions
                .iter()
                .zip([[0.0; 3], [10.0, 21.0, 30.0], [10.0, 20.0, 30.0]])
        {
            for (actual, expected) in actual.iter().zip(expected) {
                assert!((actual - expected).abs() < 0.00001);
            }
        }
        let normal = mesh.normals.as_ref().unwrap()[1];
        assert!(normal[0].abs() < 0.00001);
        assert!((normal[1] - 1.0).abs() < 0.00001);
        assert!(normal[2].abs() < 0.00001);
    }

    #[test]
    fn skin_rejects_missing_mismatched_and_invalid_hierarchy_references() {
        let bytes = skin_model(&[0, 1, 1]);
        assert!(matches!(Model::parse(&bytes), Err(Error::InvalidChunk(_))));
        assert!(matches!(
            Model::parse_with_hierarchy(&bytes, Some(&skeleton("Wrong"))),
            Err(Error::InvalidChunk(_))
        ));
        for bones in [&[0, 1, 2][..], &[0, 1][..]] {
            assert!(matches!(
                Model::parse_with_hierarchy(&skin_model(bones), Some(&skeleton("Fixture"))),
                Err(Error::InvalidChunk(_))
            ));
        }
    }
    #[test]
    fn hierarchy_alone_does_not_make_a_mesh_an_hlod() {
        let mut bytes = triangle_model([0, 1, 2], 32);
        bytes.extend(skeleton("Fixture"));
        let model = Model::parse(&bytes).unwrap();
        assert!(model.hierarchy.is_some());
        assert!(!model.hlod);
        assert!(!Model::null().hlod);
    }

    #[test]
    fn meshless_hlod_retains_hierarchy_and_boundbox_without_invented_vertices() {
        let mut bytes = skeleton("Fixture");
        let mut bounds = [0; 68];
        bounds[8..23].copy_from_slice(b"Fixture.PICKBOX");
        for (axis, extent) in [2.0_f32, 3.0, 4.0].into_iter().enumerate() {
            bounds[56 + axis * 4..60 + axis * 4].copy_from_slice(&extent.to_le_bytes());
        }
        bytes.extend(chunk(0x740, &bounds, false));
        let mut header = [0; 40];
        header[4..8].copy_from_slice(&1_u32.to_le_bytes());
        header[24..31].copy_from_slice(b"Fixture");
        let mut lod = chunk(0x701, &header, false);
        let mut array = chunk(0x703, &1_u32.to_le_bytes(), false);
        let mut binding = [0; 36];
        binding[4..19].copy_from_slice(b"Fixture.PICKBOX");
        array.extend(chunk(0x704, &binding, false));
        lod.extend(chunk(0x702, &array, true));
        bytes.extend(chunk(0x700, &lod, true));
        let model = Model::parse(&bytes).unwrap();
        assert!(model.meshes.is_empty());
        assert!(model.hlod);
        assert_eq!(model.bone_names, ["ROOT", "MuzzleFX1"]);
        assert!(model.bone_position("MuzzleFX1").is_some());
        // HTreeClass::Base_Update assigns the object matrix to pivot zero;
        // its authored translation is not an additional root-bone offset.
        assert_eq!(
            model.bounds.min.map(f32::to_bits),
            [-2.0_f32, -3.0, -4.0].map(f32::to_bits)
        );
        assert_eq!(
            model.bounds.max.map(f32::to_bits),
            [2.0_f32, 3.0, 4.0].map(f32::to_bits)
        );
        assert!(Model::parse(&[]).is_err());
        assert!(Model::parse(&chunk(0xdead, &[0; 8], false)).is_err());
        // A bare HTree is not a render prototype (assetmgr.cpp664).
        assert!(Model::parse(&skeleton("Fixture")).is_err());
    }
}
