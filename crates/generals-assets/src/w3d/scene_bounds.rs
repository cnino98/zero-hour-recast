//! `HLodClass::Update_Obj_Space_Bounding_Volumes` (base pose, including PICKBOX).
use super::{Bounds, Error, Mesh, Pivot, base_bound_transform, read_name, read_u32, read_vector};

pub(super) fn load(
    meshes: &[Mesh],
    boxes: &[&[u8]],
    bindings: Option<&[(String, usize)]>,
    pivots: &[Pivot],
) -> Result<Bounds, Error> {
    let mut volumes = Vec::with_capacity(meshes.len() + boxes.len());
    for mesh in meshes {
        volumes.push(transform(&mesh.bounds, mesh.bound_transform));
    }
    for data in boxes {
        if data.len() != 68 {
            return Err(Error::InvalidChunk("W3D box size"));
        }
        let name = read_name(&data[8..40])?;
        let Some((_, bone)) = bindings.and_then(|bindings| {
            bindings
                .iter()
                .find(|(bound, _)| bound.eq_ignore_ascii_case(&name))
        }) else {
            continue;
        };
        if read_u32(data, 4)? & 1 != 0
            && name
                .rsplit('.')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("BOUNDINGBOX"))
        {
            return Err(Error::Unsupported("animated HLOD BOUNDINGBOX override"));
        }
        let center = read_vector(data, 44)?;
        let extent = read_vector(data, 56)?;
        let bounds = Bounds {
            min: std::array::from_fn(|axis| center[axis] - extent[axis]),
            max: std::array::from_fn(|axis| center[axis] + extent[axis]),
        };
        volumes.push(transform(&bounds, base_bound_transform(*bone, pivots)?));
    }
    let mut volumes = volumes.into_iter();
    let mut result = volumes.next().unwrap_or(Bounds {
        min: [0.0; 3],
        max: [0.0; 3],
    });
    for bounds in volumes {
        for axis in 0..3 {
            result.min[axis] = result.min[axis].min(bounds.min[axis]);
            result.max[axis] = result.max[axis].max(bounds.max[axis]);
        }
    }
    Ok(result)
}

fn transform(bounds: &Bounds, transform: [[f32; 3]; 4]) -> Bounds {
    let center: [f32; 3] = std::array::from_fn(|axis| (bounds.max[axis] + bounds.min[axis]) * 0.5);
    let extent: [f32; 3] = std::array::from_fn(|axis| (bounds.max[axis] - bounds.min[axis]) * 0.5);
    let transformed: [f32; 3] = std::array::from_fn(|axis| {
        let x = transform[0][axis] * center[0];
        let y = transform[1][axis] * center[1];
        let z = transform[2][axis] * center[2];
        x + y + z + transform[3][axis]
    });
    let half: [f32; 3] = std::array::from_fn(|axis| {
        let x = transform[0][axis].abs() * extent[0];
        let y = transform[1][axis].abs() * extent[1];
        let z = transform[2][axis].abs() * extent[2];
        x + y + z
    });
    Bounds {
        min: std::array::from_fn(|axis| transformed[axis] - half[axis]),
        max: std::array::from_fn(|axis| transformed[axis] + half[axis]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_pickbox_expands_scene_bound_without_renderable_mesh_vertices() {
        let mut data = [0; 68];
        data[8..23].copy_from_slice(b"UIMOB01.PICKBOX");
        for (offset, point) in [(44, [0.0_f32, 0.0, 10.0]), (56, [5.0, 3.0, 10.0])] {
            for (axis, value) in point.into_iter().enumerate() {
                data[offset + axis * 4..offset + axis * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        let pivots = [
            Pivot {
                name: "ROOTTRANSFORM".into(),
                parent: None,
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
            Pivot {
                name: "BOX".into(),
                parent: Some(0),
                translation: [20.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
        ];
        let bindings = [("UIMOB01.PICKBOX".into(), 1)];
        let bounds = load(&[], &[&data], Some(&bindings), &pivots).unwrap();
        assert_eq!(
            bounds.min.map(f32::to_bits),
            [15.0_f32, -3.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(
            bounds.max.map(f32::to_bits),
            [25.0_f32, 3.0, 20.0].map(f32::to_bits)
        );
        let unbound = load(&[], &[&data], Some(&[]), &pivots).unwrap();
        assert_eq!(
            unbound.max.map(f32::to_bits),
            [0.0_f32; 3].map(f32::to_bits)
        );
    }
}

/// `HLodClass::Get_Obj_Space_Bounding_Sphere` derives its result from the
/// complete object box. The internal union of child spheres is not returned by
/// this virtual method. Every LOD and aggregate prototype must be resolved.
pub(super) fn sphere(
    meshes: &[Mesh],
    boxes: &[&[u8]],
    lod: Option<&[u8]>,
    pivots: &[Pivot],
    absent: &[String],
) -> Option<super::Sphere> {
    let Some(lod) = lod else {
        let [mesh] = meshes else {
            return None;
        };
        return Some(super::Sphere {
            center: mesh.sphere_center,
            radius: mesh.sphere_radius,
        });
    };
    let mut combined: Option<Bounds> = None;
    for (name, bone) in all_bindings(lod).ok()? {
        // HLodClass constructor adds a subobject only after Create_Render_Obj succeeds.
        if absent
            .iter()
            .any(|missing| missing.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        let (center, extent) = if name.eq_ignore_ascii_case("NULL") {
            ([0.0; 3], [0.1; 3])
        } else if let Some(mesh) = meshes
            .iter()
            .find(|mesh| mesh.name.eq_ignore_ascii_case(&name))
        {
            (
                std::array::from_fn(|axis| (mesh.bounds.max[axis] + mesh.bounds.min[axis]) * 0.5),
                std::array::from_fn(|axis| (mesh.bounds.max[axis] - mesh.bounds.min[axis]) * 0.5),
            )
        } else {
            let data = boxes.iter().find(|data| {
                data.len() == 68
                    && read_name(&data[8..40]).is_ok_and(|bound| bound.eq_ignore_ascii_case(&name))
            })?;
            if read_u32(data, 4).ok()? & 1 != 0
                && name
                    .rsplit('.')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("BOUNDINGBOX"))
            {
                return None;
            }
            (read_vector(data, 44).ok()?, read_vector(data, 56).ok()?)
        };
        let bounds = child_box(
            center,
            extent,
            base_bound_transform(bone, pivots).ok()?,
            combined.is_none(),
        );
        if let Some(combined) = &mut combined {
            for axis in 0..3 {
                combined.min[axis] = combined.min[axis].min(bounds.min[axis]);
                combined.max[axis] = combined.max[axis].max(bounds.max[axis]);
            }
        } else {
            combined = Some(bounds);
        }
    }
    let bounds = combined.unwrap_or(Bounds {
        min: [0.0; 3],
        max: [0.0; 3],
    });
    let center = std::array::from_fn(|axis| (bounds.max[axis] + bounds.min[axis]) * 0.5);
    let extent = std::array::from_fn(|axis| (bounds.max[axis] - bounds.min[axis]) * 0.5);
    Some(super::Sphere {
        center,
        radius: length(extent),
    })
}

// The first HLOD child is converted to MinMaxAABox before transform; later
// children transform their AABox center/extent before union (HLod.cpp3440).
fn child_box(center: [f32; 3], extent: [f32; 3], matrix: [[f32; 3]; 4], first: bool) -> Bounds {
    let mut min = matrix[3];
    let mut max = matrix[3];
    for axis in 0..3 {
        if first {
            for component in 0..3 {
                let a = matrix[component][axis] * (center[component] - extent[component]);
                let b = matrix[component][axis] * (center[component] + extent[component]);
                min[axis] += a.min(b);
                max[axis] += a.max(b);
            }
        } else {
            let mut transformed = matrix[3][axis];
            let mut half = 0.0;
            for component in 0..3 {
                let position = matrix[component][axis] * center[component];
                transformed += position;
                let size = matrix[component][axis] * extent[component];
                half += size.abs();
            }
            min[axis] = transformed - half;
            max[axis] = transformed + half;
        }
    }
    Bounds { min, max }
}

pub(super) fn all_bindings(bytes: &[u8]) -> Result<Vec<(String, usize)>, Error> {
    let mut lods = Vec::new();
    let mut aggregates = Vec::new();
    let mut expected_lods = None;
    for chunk in super::Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0x701 => expected_lods = Some(read_u32(chunk.body, 4)?),
            0x702 | 0x705 => {
                chunk.require_container()?;
                let bindings = binding_array(chunk.body)?;
                if chunk.id == 0x702 {
                    lods.push(bindings);
                } else {
                    aggregates.extend(bindings);
                }
            }
            _ => {}
        }
    }
    if expected_lods != u32::try_from(lods.len()).ok() {
        return Err(Error::InvalidChunk("HLOD array count"));
    }
    let mut bindings: Vec<_> = lods.into_iter().flatten().collect();
    bindings.extend(aggregates);
    Ok(bindings)
}

fn binding_array(bytes: &[u8]) -> Result<Vec<(String, usize)>, Error> {
    let mut bindings = Vec::new();
    let mut expected = None;
    for chunk in super::Chunks::new(bytes) {
        let chunk = chunk?;
        match chunk.id {
            0x703 => {
                expected = Some(super::read_count(
                    chunk.body,
                    0,
                    super::MAX_MESHES,
                    "HLOD count",
                )?);
            }
            0x704 => {
                if chunk.body.len() != 36 {
                    return Err(Error::InvalidChunk("HLOD binding"));
                }
                if bindings.len() == super::MAX_MESHES {
                    return Err(Error::LimitExceeded("HLOD count"));
                }
                let bone = usize::try_from(read_u32(chunk.body, 0)?)
                    .map_err(|_| Error::InvalidChunk("HLOD pivot"))?;
                bindings.push((read_name(&chunk.body[4..])?, bone));
            }
            _ => {}
        }
    }
    if expected != Some(bindings.len()) {
        return Err(Error::InvalidChunk("HLOD binding count"));
    }
    Ok(bindings)
}

fn length(delta: [f32; 3]) -> f32 {
    let x = delta[0] * delta[0];
    let y = delta[1] * delta[1];
    let z = delta[2] * delta[2];
    let xy = x + y;
    (xy + z).sqrt()
}

#[cfg(test)]
mod sphere_tests;
