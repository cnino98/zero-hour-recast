use super::*;
use crate::{virtual_assets::VirtualAssets, w3d::Model};

fn original() -> Vec<u8> {
    let root = crate::game_root().unwrap();
    let assets = VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap();
    assets.read("art/w3d/EXHydrant.w3d").unwrap().bytes
}

fn edit(id: u32, offset: usize, value: [u8; 4]) -> Vec<u8> {
    let mut bytes = original();
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let found = read_u32(&bytes, at).unwrap();
        let size = usize::try_from(read_u32(&bytes, at + 4).unwrap() & 0x7fff_ffff).unwrap();
        if found == id {
            bytes[at + 8 + offset..at + 12 + offset].copy_from_slice(&value);
            return bytes;
        }
        at += 8 + size;
    }
    panic!("missing original emitter record {id:x}");
}

#[test]
#[ignore = "requires original game installation"]
fn installed_hydrant_is_a_typed_emitter_prototype_with_native_point_bounds() {
    let model = Model::parse(&original()).unwrap();
    let emitter = model.emitter.as_ref().unwrap();
    assert_eq!(emitter.name, "EXHydrant");
    assert_eq!(emitter.texture, "exhydrant.tga");
    assert!(model.meshes.is_empty() && model.hierarchy.is_none());
    assert_eq!(
        model.bounds.min.map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    assert_eq!(
        model.scene_sphere.as_ref().unwrap().radius.to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(emitter.emission.burst, 5);
    assert_eq!(emitter.emission.lifetime.to_bits(), 1.5_f32.to_bits());
    assert_eq!(emitter.emission.rate.to_bits(), 30.303_03_f32.to_bits());
    assert_eq!(
        emitter.motion.position_box.map(f32::to_bits),
        [0.1_f32; 3].map(f32::to_bits)
    );
    assert_eq!(
        emitter.motion.velocity_box.map(f32::to_bits),
        [0.0_f32, 0.0, 0.1].map(f32::to_bits)
    );
    assert_eq!(
        emitter.motion.velocity[2].to_bits(),
        80.000_084_f32.to_bits()
    );
    assert_eq!(
        emitter.motion.acceleration[2].to_bits(),
        (-100.0_f32).to_bits()
    );
    assert_eq!(emitter.motion.outward.to_bits(), 7.0_f32.to_bits());
    assert_eq!(
        emitter
            .properties
            .colors
            .iter()
            .map(|key| key.color)
            .collect::<Vec<_>>(),
        [[154, 235, 255], [213, 250, 255]]
    );
    assert_eq!(
        emitter.properties.opacity[0].value.to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(emitter.properties.size_random.to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        emitter
            .properties
            .size
            .iter()
            .map(|key| key.value.to_bits())
            .collect::<Vec<_>>(),
        [1.5_f32, 1.0].map(f32::to_bits)
    );
    assert!(Model::null().emitter.is_none());
    assert_eq!(
        Model::null().scene_sphere.unwrap().radius.to_bits(),
        0.1_f32.to_bits()
    );
}

#[test]
#[ignore = "requires original game installation"]
fn emitter_malformed_and_unimplemented_forms_do_not_become_empty_geometry_success() {
    for bytes in [
        edit(0x501, 0, 0x10000_u32.to_le_bytes()),
        edit(0x504, 4, 1_u32.to_le_bytes()),
        edit(0x504, 92, 2_u32.to_le_bytes()),
        edit(0x504, 88, [0; 4]),
        edit(0x50a, 4, 1.0_f32.to_le_bytes()),
        edit(0x505, 0, 0_u32.to_le_bytes()),
        edit(0x505, 48, 0.0_f32.to_le_bytes()),
        edit(0x503, 268, f32::NAN.to_le_bytes()),
    ] {
        assert!(Model::parse(&bytes).is_err());
    }
    let bytes = original();
    assert!(Model::parse(&bytes[..bytes.len() - 1]).is_err());
    assert!(Model::parse(&[]).is_err());
    let mut mixed = vec![0, 0, 0, 0, 0, 0, 0, 128];
    mixed.extend_from_slice(&bytes);
    assert!(Model::parse(&mixed).is_err());
}

#[test]
#[ignore = "requires original game installation"]
fn native_start_property_time_and_reserved_words_do_not_change_start_value() {
    let bytes = edit(0x505, 40, 9.0_f32.to_le_bytes());
    let model = Model::parse(&bytes).unwrap();
    assert_eq!(
        model.emitter.unwrap().properties.colors[0].time.to_bits(),
        0.0_f32.to_bits()
    );
    let bytes = edit(0x50a, 12, 123_u32.to_le_bytes());
    assert!(Model::parse(&bytes).is_ok());
}
