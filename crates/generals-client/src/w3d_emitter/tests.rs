use super::{buffer::Buffer, pose::Pose, random::Random, runtime::Scene};
use generals_assets::{
    virtual_assets::VirtualAssets,
    w3d::{Model, emitter::Definition},
};
use std::sync::Arc;

fn assets() -> VirtualAssets {
    let root = &generals_assets::game_root().unwrap();
    VirtualAssets::for_zero_hour(
        root.join("GameData/Command and Conquer Generals"),
        root.join("GameData/Command and Conquer Generals Zero Hour"),
    )
    .unwrap()
}

fn original() -> Arc<Definition> {
    let model = Model::parse(&assets().read("art/w3d/EXHydrant.w3d").unwrap().bytes).unwrap();
    Arc::new(model.emitter.unwrap())
}

fn pose() -> Pose {
    Pose::new(
        [100.0, 200.0, 10.0],
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    )
}

fn create(scene: &mut Scene, object: usize, validate_logic: bool) {
    let binding = super::Binding {
        name: "WaterSpray".into(),
        definition: original(),
        texture: 0,
    };
    scene.create(object, 0, &binding, pose(), 0, validate_logic);
}

#[ignore = "requires original game installation"]
#[test]
fn original_first_install_burst_queue_and_next_visibility_pass_preserve_native_timing() {
    let mut scene = Scene::new();
    create(&mut scene, 7, true);
    assert_eq!(scene.emitters[0].buffer.capacity, 378);
    for now in [0, 33, 66] {
        scene.update(now, u32::from(now != 0) * 33, |_| Some((pose(), false)));
        assert!(scene.emitters[0].buffer.particles.is_empty());
    }
    assert_eq!(scene.emitters[0].buffer.pending.len(), 5);
    assert!(
        scene.emitters[0]
            .buffer
            .pending
            .iter()
            .all(|particle| particle.born == 33)
    );
    scene.update(99, 33, |_| Some((pose(), false)));
    let buffer = &scene.emitters[0].buffer;
    assert_eq!(buffer.particles.len(), 5);
    assert_eq!(buffer.pending.len(), 5);
    // Independent source_oracle.py interprets signed Random3 and the x87 stack.
    assert_eq!(
        buffer.particles[0].position.map(f32::to_bits),
        [0x42c7_599f, 0x4348_66e6, 0x416c_f4ec]
    );
    let (minimum, maximum) = buffer.bounds().unwrap();
    assert!(minimum[0] < 100.0 && maximum[0] > 100.0);
    assert!(maximum[2] > 10.0);
    scene.destroy(7);
    assert_eq!(scene.emitters[0].buffer.particles.len(), 5);
    scene.update(132, 33, |_| None);
    assert_eq!(scene.emitters[0].buffer.particles.len(), 10);
    assert!(scene.emitters[0].buffer.pending.is_empty());
    scene.update(1566, 1434, |_| None);
    assert!(scene.emitters.is_empty());
}

#[ignore = "requires original game installation"]
#[test]
fn hidden_start_stop_preserves_remainder_and_never_emits_while_stopped() {
    let mut scene = Scene::new();
    create(&mut scene, 1, true);
    scene.update(0, 0, |_| Some((pose(), true)));
    scene.update(33, 33, |_| Some((pose(), false)));
    scene.update(66, 33, |_| Some((pose(), false)));
    assert!(scene.emitters[0].buffer.pending.is_empty());
    scene.update(99, 33, |_| Some((pose(), true)));
    assert!(scene.emitters[0].buffer.pending.is_empty());
    scene.update(132, 33, |_| Some((pose(), false)));
    assert_eq!(scene.emitters[0].buffer.pending.len(), 5);
    assert!(
        scene.emitters[0]
            .buffer
            .pending
            .iter()
            .all(|particle| particle.born == 99)
    );
}

#[ignore = "requires original game installation"]
#[test]
fn scene_head_insertion_orders_multiple_emitters_before_shared_volume_random_draws() {
    let mut scene = Scene::new();
    create(&mut scene, 10, true);
    create(&mut scene, 11, true);
    for now in [0, 33, 66] {
        scene.update(now, u32::from(now != 0) * 33, |_| Some((pose(), false)));
    }
    // Newest emitter installs first, then older emitter's buffer becomes HEAD.
    // Hence the older emitter gets the process stream's first volume sample.
    let first = scene.emitters[0].buffer.pending[0].position;
    let second = scene.emitters[1].buffer.pending[0].position;
    let mut random = Random::new();
    let local = random.box_vector([0.1; 3]);
    let expected: [f32; 3] = std::array::from_fn(|axis| pose().position[axis] + local[axis]);
    assert_eq!(first.map(f32::to_bits), expected.map(f32::to_bits));
    assert_ne!(first.map(f32::to_bits), second.map(f32::to_bits));
}

#[ignore = "requires original game installation"]
#[test]
fn genuine_original_default_emitter_draw_loads_texture_and_rejects_model_replacement() {
    let assets = assets();
    let mut files = crate::skirmish_data::source_files::metadata_baseline(&assets).unwrap();
    let object = crate::skirmish_data::definition(&files, "Object", "WaterSpray").unwrap();
    let model = crate::skirmish_data::normal_model(&assets, object).unwrap();
    assert!(model.emitter.is_some());
    assert!(model.meshes.is_empty());
    let definition = model.emitter.as_ref().unwrap();
    super::data::validate(definition).unwrap();
    crate::source_texture::load(&assets, &definition.texture).unwrap();
    let changed = files
        .iter_mut()
        .flat_map(|file| &mut file.declarations)
        .find(|object| object.kind == "Object" && object.name == "WaterSpray")
        .unwrap();
    let end = changed
        .lines
        .iter()
        .position(|line| {
            line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "End")
        })
        .unwrap();
    let extra = [
        (vec!["ConditionState", "DAMAGED"], 1, false),
        (vec!["Model", "NULL"], 2, true),
        (vec!["End"], 1, false),
    ]
    .into_iter()
    .map(
        |(tokens, depth, assigned)| generals_assets::content::ContentLine {
            line: 0,
            raw: tokens.join(" "),
            tokens: tokens.into_iter().map(str::to_owned).collect(),
            assigned,
            depth: Some(depth),
        },
    );
    changed.lines.splice(end..end, extra);
    assert!(crate::skirmish_data::normal_model(&assets, changed).is_err());
}

#[ignore = "requires original game installation"]
#[test]
fn native_quad_size_is_full_width_and_live_bounds_survive_origin_point_prototype() {
    let definition = original();
    let mut random = Random::new();
    let mut buffer = Buffer::new(definition, &mut random, 0);
    buffer.enqueue([100.0, 200.0, 10.0], [0.0; 3], 0);
    buffer.update(1);
    let size = buffer.size(&buffer.particles[0], 1);
    let mesh = super::render::geometry(&buffer, 1, bevy::prelude::Vec3::X, bevy::prelude::Vec3::Y);
    let bevy::mesh::VertexAttributeValues::Float32x3(positions) = mesh
        .attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
        .unwrap()
    else {
        panic!("particle positions")
    };
    let width = positions[2][0] - positions[0][0];
    assert!((width - size * 0.1).abs() < 0.000_01);
    assert_eq!(positions[0][2].to_bits(), (-20.0_f32).to_bits());
    assert!(buffer.bounds().unwrap().1[2] > 10.0);
}

#[ignore = "requires original game installation"]
#[test]
fn actual_constructor_validation_allocates_once_and_process_random_survives_scene_reset() {
    let mut outside_logic = Scene::new();
    create(&mut outside_logic, 1, false);
    assert_eq!(
        outside_logic.random.property().to_ne_bytes(),
        0xbe8b_a252_u32.to_ne_bytes()
    );
    let mut valid = Scene::new();
    create(&mut valid, 1, true);
    valid.destroy(1);
    valid.update(0, 0, |_| None);
    assert!(valid.emitters.is_empty());
    // The surviving original template keeps its validated pristine state, while
    // the next direct Create still constructs a fresh buffer/table.
    valid.reset();
    create(&mut valid, 2, true);
    assert_eq!(
        valid.random.property().to_ne_bytes(),
        0x4af1_405c_u32.to_ne_bytes()
    );
}

#[ignore = "requires original game installation"]
#[test]
fn unsupported_completion_and_extended_exponent_volumes_remain_explicit_errors() {
    let mut definition = (*original()).clone();
    definition.emission.maximum = 1.0;
    assert!(super::data::validate(&definition).is_err());
    definition.emission.maximum = 0.0;
    definition.motion.position_box = [f32::from_bits(0x2b80_0000); 3];
    assert!(super::data::validate(&definition).is_err());
}
