//! W3D fixed-function material records, matching `meshmdlio.cpp` at 2bfaea60.
use super::{Chunks, Error, read_float, read_name, read_u32, set_once};

pub struct VertexMaterial {
    pub name: String,
    pub attributes: u32,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub shininess: f32,
    pub opacity: f32,
    pub linear_mapper: Option<LinearMapper>,
}

/// Stage-zero `LinearOffsetTextureMapperClass` constructor arguments.
// Render instances retain their own mapper parameters.
#[derive(Clone, Copy)]
pub struct LinearMapper {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
    pub per_second: [f32; 2],
    pub clamp: bool,
}

fn linear_mapper(attributes: u32, bytes: &[u8]) -> Result<Option<LinearMapper>, Error> {
    if attributes & 0x00ff_0000 != 0x0004_0000 {
        return Ok(None);
    }
    let text = std::str::from_utf8(bytes.split(|byte| *byte == 0).next().unwrap_or_default())
        .map_err(|_| Error::InvalidChunk("mapper arguments encoding"))?;
    let mut result = LinearMapper {
        scale: [1.0; 2],
        offset: [0.0; 2],
        per_second: [0.0; 2],
        clamp: false,
    };
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or_default().trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let target = match key.trim().to_ascii_lowercase().as_str() {
            "uscale" => &mut result.scale[0],
            "vscale" => &mut result.scale[1],
            "uoffset" => &mut result.offset[0],
            "voffset" => &mut result.offset[1],
            "upersec" => &mut result.per_second[0],
            "vpersec" => &mut result.per_second[1],
            "clampfix" => {
                result.clamp = matches!(
                    value.as_bytes().first().map(u8::to_ascii_uppercase),
                    Some(b'Y' | b'T' | b'1')
                );
                continue;
            }
            _ => continue,
        };
        *target = value
            .split('%')
            .next()
            .unwrap_or_default()
            .trim()
            .parse()
            .map_err(|_| Error::InvalidChunk("linear mapper number"))?;
        if value.contains('%') {
            *target /= 100.0;
        }
        if !target.is_finite() {
            return Err(Error::InvalidChunk("nonfinite linear mapper"));
        }
    }
    Ok(Some(result))
}

pub struct Texture {
    pub name: String,
    pub attributes: u16,
}

pub struct TextureStage {
    pub texture_ids: Vec<u32>,
    /// Source `read_stage_texcoords` converts V to 1-V on loading. Empty means
    /// the mesh has no UV arrays; omitted stages otherwise reuse source array 0.
    pub coordinates: Vec<[f32; 2]>,
    pub face_coordinate_ids: Option<Vec<[u32; 3]>>,
}

pub struct Pass {
    pub vertex_material_ids: Vec<u32>,
    pub shader_ids: Vec<u32>,
    pub colors: Option<Vec<[u8; 4]>>,
    pub stages: Vec<TextureStage>,
}

pub struct Materials {
    pub vertex_materials: Vec<VertexMaterial>,
    /// Bytes in the exact `W3dShaderStruct` field order.
    pub shaders: Vec<[u8; 16]>,
    pub textures: Vec<Texture>,
    pub passes: Vec<Pass>,
}

pub(super) fn parse(
    bytes: &[u8],
    vertices: usize,
    triangles: usize,
) -> Result<Option<Materials>, Error> {
    let (mut info, mut shaders, mut vertex_materials, mut textures) = (None, None, None, None);
    let mut passes = Vec::new();
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0x28 => set_once(&mut info, chunk.body)?,
            0x29 => set_once(&mut shaders, chunk.body)?,
            0x2a => set_once(&mut vertex_materials, chunk.body)?,
            0x30 => set_once(&mut textures, chunk.body)?,
            0x38 => {
                if passes.len() == 4 {
                    return Err(Error::LimitExceeded("material pass count"));
                }
                passes.push(chunk.body);
            }
            _ => {}
        }
    }
    let Some(info) = info else {
        if shaders.is_some()
            || vertex_materials.is_some()
            || textures.is_some()
            || !passes.is_empty()
        {
            return Err(Error::InvalidChunk("materials lack count header"));
        }
        return Ok(None);
    };
    if info.len() != 16 {
        return Err(Error::InvalidChunk("material count header size"));
    }
    let mut counts = [0; 4];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = usize::try_from(read_u32(info, index * 4)?)
            .map_err(|_| Error::LimitExceeded("materials"))?;
        if *count > 4096 {
            return Err(Error::LimitExceeded("materials"));
        }
    }
    let vertex_materials = parse_vertex_materials(vertex_materials.unwrap_or(&[]))?;
    let shader_bytes = shaders.unwrap_or(&[]);
    if shader_bytes.len() % 16 != 0 {
        return Err(Error::InvalidChunk("shader record size"));
    }
    let shaders = shader_bytes
        .chunks_exact(16)
        .map(|data| {
            data.try_into()
                .map_err(|_| Error::InvalidChunk("shader record size"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let textures = parse_textures(textures.unwrap_or(&[]))?;
    if counts
        != [
            passes.len(),
            vertex_materials.len(),
            shaders.len(),
            textures.len(),
        ]
    {
        return Err(Error::InvalidChunk("material counts do not match records"));
    }
    let mut passes = passes
        .into_iter()
        .map(|data| parse_pass(data, vertices, triangles, &counts))
        .collect::<Result<Vec<_>, _>>()?;
    resolve_coordinates(&mut passes);
    Ok(Some(Materials {
        vertex_materials,
        shaders,
        textures,
        passes,
    }))
}

fn parse_vertex_materials(bytes: &[u8]) -> Result<Vec<VertexMaterial>, Error> {
    let mut materials = Vec::new();
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id != 0x2b {
            return Err(Error::Unsupported("vertex material child"));
        }
        let (mut name, mut info, mut mapper) = (None, None, None);
        for field in Chunks::new(chunk.body) {
            let field = field?;
            match field.id {
                0x2c => set_once(&mut name, field.body)?,
                0x2d => set_once(&mut info, field.body)?,
                0x2e => set_once(&mut mapper, field.body)?,
                _ => {}
            }
        }
        let info = info.ok_or(Error::InvalidChunk("missing vertex material info"))?;
        if info.len() != 32 {
            return Err(Error::InvalidChunk("vertex material size"));
        }
        let color =
            |offset: usize| std::array::from_fn(|index| f32::from(info[offset + index]) / 255.0);
        let opacity = read_float(info, 24)?;
        if !(0.0..=1.0).contains(&opacity) {
            return Err(Error::InvalidChunk("material opacity"));
        }
        materials.push(VertexMaterial {
            name: read_name(name.unwrap_or(&[]))?,
            attributes: read_u32(info, 0)?,
            linear_mapper: linear_mapper(read_u32(info, 0)?, mapper.unwrap_or_default())?,
            ambient: color(4),
            diffuse: color(8),
            specular: color(12),
            emissive: color(16),
            shininess: read_float(info, 20)?,
            opacity,
        });
    }
    Ok(materials)
}

fn parse_textures(bytes: &[u8]) -> Result<Vec<Texture>, Error> {
    let mut textures = Vec::new();
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        if chunk.id != 0x31 {
            return Err(Error::Unsupported("texture child"));
        }
        let (mut name, mut info) = (None, None);
        for field in Chunks::new(chunk.body) {
            let field = field?;
            match field.id {
                0x32 => set_once(&mut name, field.body)?,
                0x33 => set_once(&mut info, field.body)?,
                _ => {}
            }
        }
        let attributes = if let Some(info) = info {
            if info.len() != 12 {
                return Err(Error::InvalidChunk("texture info size"));
            }
            u16::from_le_bytes([info[0], info[1]])
        } else {
            0
        };
        textures.push(Texture {
            name: read_name(name.ok_or(Error::InvalidChunk("missing texture name"))?)?,
            attributes,
        });
    }
    Ok(textures)
}

fn ids(bytes: &[u8], count: usize, limit: usize, nullable: bool) -> Result<Vec<u32>, Error> {
    if bytes.len() != 4 && bytes.len() != count * 4 {
        return Err(Error::InvalidChunk("material ID array length"));
    }
    bytes
        .chunks_exact(4)
        .map(|data| {
            let index = read_u32(data, 0)?;
            if !(nullable && index == u32::MAX)
                && usize::try_from(index).map_or(true, |index| index >= limit)
            {
                return Err(Error::InvalidChunk("material ID outside declared table"));
            }
            Ok(index)
        })
        .collect()
}

fn parse_pass(
    bytes: &[u8],
    vertices: usize,
    triangles: usize,
    counts: &[usize; 4],
) -> Result<Pass, Error> {
    let (mut materials, mut shaders, mut colors) = (None, None, None);
    let mut stages = Vec::new();
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0x39 => set_once(&mut materials, chunk.body)?,
            0x3a => set_once(&mut shaders, chunk.body)?,
            0x3b => set_once(&mut colors, chunk.body)?,
            0x48 => {
                if stages.len() == 2 {
                    return Err(Error::LimitExceeded("texture stages"));
                }
                stages.push(parse_stage(chunk.body, vertices, triangles, counts[3])?);
            }
            _ => {}
        }
    }
    let colors = colors
        .map(|bytes| {
            if bytes.len() != vertices * 4 {
                return Err(Error::InvalidChunk("vertex color count"));
            }
            bytes
                .chunks_exact(4)
                .map(|bytes| bytes.try_into().map_err(|_| Error::Truncated))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(Pass {
        vertex_material_ids: ids(
            materials.ok_or(Error::InvalidChunk("missing material IDs"))?,
            vertices,
            counts[1],
            false,
        )?,
        shader_ids: ids(
            shaders.ok_or(Error::InvalidChunk("missing shader IDs"))?,
            triangles,
            counts[2],
            false,
        )?,
        colors,
        stages,
    })
}

fn parse_stage(
    bytes: &[u8],
    vertices: usize,
    triangles: usize,
    texture_count: usize,
) -> Result<TextureStage, Error> {
    let (mut textures, mut coordinates, mut faces) = (None, None, None);
    for chunk in Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0x49 => set_once(&mut textures, chunk.body)?,
            0x05 | 0x4a => set_once(&mut coordinates, chunk.body)?,
            0x4b => set_once(&mut faces, chunk.body)?,
            _ => {}
        }
    }
    let data = coordinates.unwrap_or(&[]);
    if data.len() % 8 != 0
        || (coordinates.is_some() && faces.is_none() && data.len() != vertices * 8)
    {
        return Err(Error::InvalidChunk("texture coordinate count"));
    }
    let coordinates = data
        .chunks_exact(8)
        // MeshModelClass::read_stage_texcoords preserves authored float bits.
        // Original Strategy Center turret UVs include NaNs; geometry remains finite-checked.
        .map(|data| {
            Ok([
                f32::from_bits(read_u32(data, 0)?),
                1.0 - f32::from_bits(read_u32(data, 4)?),
            ])
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let face_coordinate_ids = faces
        .map(|bytes| {
            if bytes.len() != triangles * 12 {
                return Err(Error::InvalidChunk("face coordinate count"));
            }
            bytes
                .chunks_exact(12)
                .map(|face| {
                    let indices = [read_u32(face, 0)?, read_u32(face, 4)?, read_u32(face, 8)?];
                    if indices.iter().any(|&index| {
                        usize::try_from(index).map_or(true, |index| index >= coordinates.len())
                    }) {
                        return Err(Error::InvalidChunk("face texture coordinate index"));
                    }
                    Ok(indices)
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(TextureStage {
        texture_ids: ids(
            textures.ok_or(Error::InvalidChunk("missing texture IDs"))?,
            triangles,
            texture_count,
            true,
        )?,
        coordinates,
        face_coordinate_ids,
    })
}

// MeshMatDesc::Configure_Material maps an unassigned UVSource (-1) to global
// array 0. Install_UV_Array gives that slot to the first actual coordinates.
fn resolve_coordinates(passes: &mut [Pass]) {
    let Some(first) = passes
        .iter()
        .flat_map(|pass| &pass.stages)
        .find(|stage| !stage.coordinates.is_empty())
    else {
        return;
    };
    let coordinates = first.coordinates.clone();
    let faces = first.face_coordinate_ids.clone();
    for stage in passes.iter_mut().flat_map(|pass| &mut pass.stages) {
        if stage.coordinates.is_empty() {
            stage.coordinates.clone_from(&coordinates);
            stage.face_coordinate_ids.clone_from(&faces);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Pass, ids, parse_stage, resolve_coordinates};

    fn chunk(kind: u32, bytes: &[u8]) -> Vec<u8> {
        let mut result = kind.to_le_bytes().to_vec();
        result.extend_from_slice(
            &u32::try_from(bytes.len())
                .expect("small fixture")
                .to_le_bytes(),
        );
        result.extend_from_slice(bytes);
        result
    }

    #[test]
    fn linear_mapper_retains_native_scale_start_rate_and_clamp_defaults() {
        let mapper = super::linear_mapper(0x40000, b"UScale=2\nVScale=0.5\nUOffset=-0.25\nVOffset=0.75\nUPerSec=3\nVPerSec=-2\nClampFix=Yes\0ignored").unwrap().unwrap();
        assert_eq!(
            mapper.scale.map(f32::to_bits),
            [2.0_f32, 0.5].map(f32::to_bits)
        );
        assert_eq!(
            mapper.offset.map(f32::to_bits),
            [-0.25_f32, 0.75].map(f32::to_bits)
        );
        assert_eq!(
            mapper.per_second.map(f32::to_bits),
            [3.0_f32, -2.0].map(f32::to_bits)
        );
        assert!(mapper.clamp);
        let default = super::linear_mapper(0x40000, b"").unwrap().unwrap();
        assert_eq!(
            default.scale.map(f32::to_bits),
            [1.0_f32; 2].map(f32::to_bits)
        );
        assert_eq!(
            default.offset.map(f32::to_bits),
            [0.0_f32; 2].map(f32::to_bits)
        );
        assert_eq!(
            default.per_second.map(f32::to_bits),
            [0.0_f32; 2].map(f32::to_bits)
        );
        assert!(!default.clamp);
        assert!(super::linear_mapper(0, b"UScale=2").unwrap().is_none());
    }

    #[test]
    fn texture_ids_reject_out_of_table_and_accept_explicit_null() {
        assert!(ids(&3_u32.to_le_bytes(), 1, 3, true).is_err());
        assert!(ids(&u32::MAX.to_le_bytes(), 1, 3, false).is_err());
        assert_eq!(
            ids(&u32::MAX.to_le_bytes(), 1, 3, true).unwrap(),
            vec![u32::MAX]
        );
    }

    #[test]
    fn alternate_face_coordinates_flip_v_and_validate_indices() {
        let mut bytes = chunk(0x49, &0_u32.to_le_bytes());
        let coordinates = [0.25_f32, 0.75, 0.5, 0.25];
        bytes.extend(chunk(
            0x4a,
            &coordinates
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        ));
        let faces = [0_u32, 1, 0];
        bytes.extend(chunk(
            0x4b,
            &faces
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ));
        let stage = parse_stage(&bytes, 3, 1, 1).expect("valid independent texture vertices");
        assert_eq!(stage.coordinates, vec![[0.25, 0.25], [0.5, 0.75]]);
        let last = bytes.len() - 4;
        bytes[last..].copy_from_slice(&2_u32.to_le_bytes());
        assert!(matches!(
            parse_stage(&bytes, 3, 1, 1),
            Err(Error::InvalidChunk("face texture coordinate index"))
        ));
    }

    #[test]
    fn original_strategy_turret_nan_uv_is_preserved_without_rejecting_its_mesh() {
        let mut bytes = chunk(0x49, &0_u32.to_le_bytes());
        let coordinates = [
            0.25_f32.to_bits(),
            u32::MAX,
            0.5_f32.to_bits(),
            0.75_f32.to_bits(),
        ];
        bytes.extend(chunk(
            0x4a,
            &coordinates
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ));
        let stage = parse_stage(&bytes, 2, 1, 1).unwrap();
        assert_eq!(stage.coordinates[0][0].to_bits(), 0.25_f32.to_bits());
        assert!(stage.coordinates[0][1].is_nan());
        assert_eq!(
            stage.coordinates[1].map(f32::to_bits),
            [0.5_f32, 0.25].map(f32::to_bits)
        );
    }

    #[test]
    fn truncated_coordinate_pair_is_rejected() {
        let mut bytes = chunk(0x49, &0_u32.to_le_bytes());
        bytes.extend(chunk(0x4a, &[0; 7]));
        assert!(matches!(
            parse_stage(&bytes, 1, 1, 1),
            Err(Error::InvalidChunk("texture coordinate count"))
        ));
    }
    #[test]
    fn omitted_stage_uvs_keep_texture_and_reuse_first_global_array_when_present() {
        let ids = chunk(0x49, &0_u32.to_le_bytes());
        let omitted = parse_stage(&ids, 1, 1, 1).unwrap();
        assert_eq!(omitted.texture_ids, [0]);
        assert!(omitted.coordinates.is_empty());
        let mut legacy = ids;
        legacy.extend(chunk(
            0x05,
            &[0.25_f32, 0.75]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        ));
        let specified = parse_stage(&legacy, 1, 1, 1).unwrap();
        let mut passes = [Pass {
            vertex_material_ids: vec![0],
            shader_ids: vec![0],
            colors: None,
            stages: vec![omitted, specified],
        }];
        resolve_coordinates(&mut passes);
        assert_eq!(passes[0].stages[0].coordinates, [[0.25, 0.25]]);
        assert_eq!(passes[0].stages[1].coordinates, [[0.25, 0.25]]);
        let mut invalid = chunk(0x49, &0_u32.to_le_bytes());
        invalid.extend(chunk(0x4b, &[0; 12]));
        assert!(matches!(
            parse_stage(&invalid, 1, 1, 1),
            Err(Error::InvalidChunk("face texture coordinate index"))
        ));
    }
}
