use super::{Pivot, sphere};

fn chunk(id: u32, body: &[u8], container: bool) -> Vec<u8> {
    let mut bytes = id.to_le_bytes().to_vec();
    let count = u32::try_from(body.len()).unwrap();
    bytes.extend((count | if container { 0x8000_0000 } else { 0 }).to_le_bytes());
    bytes.extend(body);
    bytes
}

fn array(names: &[(&str, u32)]) -> Vec<u8> {
    let mut bytes = chunk(
        0x703,
        &u32::try_from(names.len()).unwrap().to_le_bytes(),
        false,
    );
    for (name, bone) in names {
        let mut binding = [0; 36];
        binding[..4].copy_from_slice(&bone.to_le_bytes());
        binding[4..4 + name.len()].copy_from_slice(name.as_bytes());
        bytes.extend(chunk(0x704, &binding, false));
    }
    bytes
}

fn lod(arrays: &[&[(&str, u32)]], aggregate: &[(&str, u32)]) -> Vec<u8> {
    let mut header = [0; 40];
    header[4..8].copy_from_slice(&u32::try_from(arrays.len()).unwrap().to_le_bytes());
    let mut bytes = chunk(0x701, &header, false);
    for names in arrays {
        bytes.extend(chunk(0x702, &array(names), true));
    }
    if !aggregate.is_empty() {
        bytes.extend(chunk(0x705, &array(aggregate), true));
    }
    bytes
}

fn box_data(name: &str, center: [f32; 3], extent: [f32; 3]) -> [u8; 68] {
    let mut bytes = [0; 68];
    bytes[8..8 + name.len()].copy_from_slice(name.as_bytes());
    for (offset, values) in [(44, center), (56, extent)] {
        for (axis, value) in values.into_iter().enumerate() {
            bytes[offset + axis * 4..offset + axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn pivots() -> [Pivot; 2] {
    [
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
    ]
}

#[test]
fn pickbox_sphere_uses_base_pose_and_extent_length() {
    let data = box_data("PROP.PICKBOX", [0.0, 0.0, 10.0], [3.0, 4.0, 0.0]);
    let lod = lod(&[&[("PROP.PICKBOX", 1)]], &[]);
    let bounds = sphere(&[], &[&data], Some(&lod), &pivots(), &[]).unwrap();
    assert_eq!(
        bounds.center.map(f32::to_bits),
        [20.0_f32, 0.0, 10.0].map(f32::to_bits)
    );
    assert_eq!(bounds.radius.to_bits(), 5.0_f32.to_bits());
}

#[test]
fn unresolved_other_lod_or_aggregate_does_not_guess_a_sphere() {
    let data = box_data("PROP.BOX", [0.0; 3], [1.0, 0.0, 0.0]);
    for lod in [
        lod(&[&[("PROP.BOX", 0)], &[("EXTERNAL.MODEL", 0)]], &[]),
        lod(&[&[("PROP.BOX", 0)]], &[("EXTERNAL.MODEL", 0)]),
    ] {
        assert!(sphere(&[], &[&data], Some(&lod), &pivots(), &[]).is_none());
    }
    let mut named = box_data("PROP.BOUNDINGBOX", [0.0; 3], [1.0; 3]);
    named[4..8].copy_from_slice(&1_u32.to_le_bytes());
    let lod = lod(&[&[("PROP.BOUNDINGBOX", 0)]], &[]);
    assert!(sphere(&[], &[&named], Some(&lod), &pivots(), &[]).is_none());
}

#[test]
fn every_lod_then_aggregate_box_drives_virtual_hlod_sphere() {
    let a = box_data("PROP.A", [0.0; 3], [1.0, 0.0, 0.0]);
    let b = box_data("PROP.B", [10.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    let c = box_data("PROP.C", [0.0, 10.0, 0.0], [1.0, 0.0, 0.0]);
    let d = box_data("PROP.D", [-10.0, -10.0, 0.0], [2.0, 0.0, 0.0]);
    let cases = [
        lod(&[&[("PROP.A", 0), ("PROP.B", 0)], &[("PROP.C", 0)]], &[]),
        lod(&[&[("PROP.C", 0), ("PROP.B", 0)], &[("PROP.A", 0)]], &[]),
        lod(
            &[&[("PROP.A", 0), ("PROP.B", 0)], &[("PROP.C", 0)]],
            &[("PROP.D", 0)],
        ),
    ];
    let expected = [
        ([5.0_f32, 5.0, 0.0], 61.0_f32.sqrt()),
        ([5.0_f32, 5.0, 0.0], 61.0_f32.sqrt()),
        ([-0.5_f32, 0.0, 0.0], 232.25_f32.sqrt()),
    ];
    for (bytes, (center, radius)) in cases.iter().zip(expected) {
        let bounds = sphere(&[], &[&d, &c, &b, &a], Some(bytes), &pivots(), &[]).unwrap();
        assert_eq!(bounds.center.map(f32::to_bits), center.map(f32::to_bits));
        assert_eq!(bounds.radius.to_bits(), radius.to_bits());
    }
}

#[test]
fn builtin_null_child_contributes_its_actual_box_at_bound_bone() {
    let lod = lod(&[&[("NULL", 1)]], &[]);
    let sphere = sphere(&[], &[], Some(&lod), &pivots(), &[]).unwrap();
    assert_eq!(
        sphere.center.map(f32::to_bits),
        [20.0_f32, 0.0, 0.0].map(f32::to_bits)
    );
    // Native box transform rounds min/max around translation20 before the
    // HLOD virtual sphere derives its extent, rather than returning radius0.1.
    let half_x = ((20.0_f32 + 0.1) - (20.0_f32 - 0.1)) * 0.5;
    let x = half_x * half_x;
    let y = 0.1_f32 * 0.1;
    let z = 0.1_f32 * 0.1;
    assert_eq!(sphere.radius.to_bits(), ((x + y) + z).sqrt().to_bits());
}

#[test]
fn authored_box_extent_survives_large_center_before_child_transform() {
    let a = box_data("PROP.A", [0.0; 3], [0.0; 3]);
    let b = box_data("PROP.B", [10_000_000.0, 0.0, 0.0], [0.25, 0.0, 0.0]);
    let lod = lod(&[&[("PROP.A", 0), ("PROP.B", 1)]], &[]);
    let mut pivots = pivots();
    pivots[1].translation = [-10_000_000.0, 0.0, 0.0];
    let sphere = sphere(&[], &[&a, &b], Some(&lod), &pivots, &[]).unwrap();
    assert_eq!(
        sphere.center.map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    assert_eq!(sphere.radius.to_bits(), 0.25_f32.to_bits());
}
