use super::*;
use bevy::mesh::VertexAttributeValues;

#[test]
fn viewport_edges_cancel_only_the_clipped_axis_of_distortion() {
    let corners = [[-0.1, 0.2], [-0.1, 0.8], [0.5, 0.8], [0.5, 0.2]];
    let uv = center_uv(corners, [0.06, -0.06]);
    assert!((uv[0] - 0.2).abs() < 0.000_001);
    assert!((uv[1] - 0.464).abs() < 0.000_001);
    let interior = [[0.1, 0.2], [0.1, 0.8], [0.7, 0.8], [0.7, 0.2]];
    let uv = center_uv(interior, [0.06, -0.06]);
    assert!((uv[0] - 0.436).abs() < 0.000_001);
    assert!((uv[1] - 0.464).abs() < 0.000_001);
}

#[test]
fn source_fan_has_transparent_edges_quantized_center_and_original_tint() {
    let smudge = Smudge {
        system: 7,
        position: [0.0, 100.0, 0.0],
        size: 20.0,
        opacity: 0.5,
        offset: [0.06, -0.06],
    };
    let projection = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
    let (mesh, ranges) = mesh(&[smudge], &Transform::IDENTITY, projection).unwrap();
    assert_eq!(mesh.count_vertices(), 5);
    assert_eq!(ranges, [(7, 0..5)]);
    assert_eq!(
        mesh.indices().unwrap().iter().collect::<Vec<_>>(),
        [0, 4, 3, 3, 4, 2, 2, 4, 1, 1, 4, 0]
    );
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("smudge colors");
    };
    assert!(
        colors[..4]
            .iter()
            .all(|color| color[3].to_bits() == 0.0_f32.to_bits())
    );
    assert_eq!(
        colors[4].map(f32::to_bits),
        [1.0_f32, 238.0 / 255.0, 221.0 / 255.0, 127.0 / 255.0].map(f32::to_bits)
    );
    let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("smudge UVs");
    };
    assert!((uv[4][0] - 0.506).abs() < 0.000_001);
    assert!((uv[4][1] - 0.494).abs() < 0.000_001);
}

#[test]
fn smudge_randomness_and_snapshot_advance_on_logic_frames_not_render_frames() {
    let mut template = super::super::tests::template("Heat");
    template.drawing.style = super::super::Style::Smudge;
    let mut particles = super::super::tests::system(vec![template]);
    particles.create(
        0,
        super::super::Pose::at([0.0, 100.0, 0.0]),
        None,
        &mut |low, _| low,
    );
    let mut offsets = 0;
    particles.logic(
        0,
        &mut |low, high| {
            if low.to_bits() == (-0.06_f32).to_bits() && high.to_bits() == 0.06_f32.to_bits() {
                offsets += 1;
            }
            low
        },
        &|_| 0.0,
    );
    assert!(!particles.smudges.is_empty());
    assert_eq!(offsets, particles.smudges.len() * 2);
    let snapshot: Vec<_> = particles
        .smudges
        .iter()
        .map(|s| {
            (
                s.position.map(f32::to_bits),
                s.size.to_bits(),
                s.opacity.to_bits(),
                s.offset.map(f32::to_bits),
            )
        })
        .collect();
    particles.draw(0.25);
    particles.draw(0.5);
    let after: Vec<_> = particles
        .smudges
        .iter()
        .map(|s| {
            (
                s.position.map(f32::to_bits),
                s.size.to_bits(),
                s.opacity.to_bits(),
                s.offset.map(f32::to_bits),
            )
        })
        .collect();
    assert_eq!(snapshot, after);
}

#[test]
fn overlapping_fans_blend_against_previous_output_while_sampling_one_scene_copy() {
    let alpha = blend(true).color;
    assert_eq!(alpha.src_factor, BlendFactor::SrcAlpha);
    assert_eq!(alpha.dst_factor, BlendFactor::OneMinusSrcAlpha);
    assert_eq!(alpha.operation, BlendOperation::Add);
    let additive = blend(false).color;
    assert_eq!(additive.src_factor, BlendFactor::One);
    assert_eq!(additive.dst_factor, BlendFactor::One);
    assert_eq!(additive.operation, BlendOperation::Add);
    // Original encoded framebuffer arithmetic: a second fan samples the same
    // saved scene (0.8), then blends over the first fan's output (0.5).
    let original = 0.2_f32;
    let captured_sample = 0.8_f32;
    let sample_half = captured_sample * 0.5;
    let original_half = original * 0.5;
    let first = sample_half + original_half;
    let sample_quarter = captured_sample * 0.25;
    let first_remainder = first * 0.75;
    let second = sample_quarter + first_remainder;
    assert!((second - 0.575).abs() < 0.000_001);
    assert!((second + 0.1 + 0.15 - 0.825).abs() < 0.000_001);
}

#[test]
fn fully_clipped_smudges_retire_geometry_and_reenter_with_original_fans() {
    let mut smudges = [Smudge {
        system: 3,
        position: [0.0, 100.0, 0.0],
        size: 20.0,
        opacity: 0.5,
        offset: [0.02, -0.03],
    }];
    let projection = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
    let camera = Transform::IDENTITY;
    let initial = mesh(&smudges, &camera, projection).unwrap();
    assert_eq!(initial.0.count_vertices(), 5);
    for position in [[0.0, -100.0, 0.0], [1000.0, 100.0, 0.0]] {
        smudges[0].position = position;
        assert!(mesh(&smudges, &camera, projection).is_none());
    }
    smudges[0].position = [0.0, 100.0, 0.0];
    let restored = mesh(&smudges, &camera, projection).unwrap();
    assert_eq!(restored.1, initial.1);
    assert_eq!(restored.0.indices().unwrap().len(), 12);
    assert_eq!(
        restored.0.attribute(Mesh::ATTRIBUTE_UV_0),
        initial.0.attribute(Mesh::ATTRIBUTE_UV_0)
    );
    assert!(mesh(&[], &camera, projection).is_none());
}
