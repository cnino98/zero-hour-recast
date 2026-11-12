use super::*;
use crate::particles::{
    Pose, Style, render,
    tests::{system, template},
};
use bevy::{
    math::Affine3A,
    mesh::VertexAttributeValues,
    prelude::{Mesh, Vec3},
};

fn fixture(depth: u8, ground: bool, count: f32) -> crate::particles::Particles {
    let mut rules = template("volume");
    rules.drawing.style = Style::Volume(depth);
    rules.drawing.ground_aligned = ground;
    rules.burst_count.low = count;
    rules.burst_count.high = count;
    let mut particles = system(vec![rules]);
    particles.create(0, Pose::at([0.0; 3]), None, &mut |_, high| high);
    particles.logic(0, &mut |_, high| high, &|_| 0.0);
    particles
}

fn vertices(mesh: &Mesh) -> &[[f32; 3]] {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("position attribute")
    };
    points
}

#[test]
fn volume_layers_use_first_particle_size_without_accumulation_or_color_attenuation() {
    let mut particles = fixture(6, false, 2.0);
    let emitter = &mut particles.emitters[0];
    for (index, particle) in emitter.particles.iter_mut().enumerate() {
        particle.position = [0.0, 100.0, 0.0];
        particle.size = if index == 0 { 60.0 } else { 120.0 };
        particle.color = [0.5, 0.25, 0.75];
        particle.alpha = 0.75;
    }
    assert_eq!(
        position([1.0, 2.0, 3.0], [4.0, 6.0, 3.0], 60.0, 6, 5).map(f32::to_bits),
        [4.000_000_5_f32, 6.000_000_5, 3.0].map(f32::to_bits)
    );
    let camera = Affine3A::from_translation(Vec3::new(0.0, 0.0, 5.0));
    let mesh = render::mesh(emitter, &particles.templates[0], camera.inverse(), camera).unwrap();
    let points = vertices(&mesh);
    assert_eq!(points.len(), 48);
    for (layer, quads) in (0..6_u8).zip(points.chunks_exact(8)) {
        let center = position([0.0, 100.0, 0.0], [0.0, -50.0, 0.0], 60.0, 6, layer);
        let expected_z = -center[1] * 0.1;
        assert!(
            quads
                .iter()
                .all(|point| (point[2] - expected_z).abs() < 0.000_001)
        );
        assert!((quads[2][0] - quads[0][0] - 6.0).abs() < f32::EPSILON);
        assert!((quads[6][0] - quads[4][0] - 12.0).abs() < f32::EPSILON);
    }
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("color attribute")
    };
    assert!(
        colors
            .iter()
            .all(|color| color.map(f32::to_bits) == colors[0].map(f32::to_bits))
    );
    assert_eq!(
        colors[0].map(f32::to_bits),
        [
            127.0_f32 / 255.0,
            63.0 / 255.0,
            191.0 / 255.0,
            191.0 / 255.0
        ]
        .map(f32::to_bits)
    );
    // Input order stays layer-major until the shared pool combines all effects.
    assert_eq!(
        mesh.indices().unwrap().iter().take(12).collect::<Vec<_>>(),
        [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]
    );
}

#[test]
fn volume_ground_layers_coincide_and_one_layer_matches_ordinary_billboards() {
    let mut particles = fixture(6, true, 1.0);
    let emitter = &particles.emitters[0];
    let mesh = render::mesh(
        emitter,
        &particles.templates[0],
        Affine3A::IDENTITY,
        Affine3A::IDENTITY,
    )
    .unwrap();
    let points = vertices(&mesh);
    for quad in points.chunks_exact(4) {
        assert_eq!(
            quad.iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>(),
            points[..4]
                .iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>()
        );
    }
    particles.templates[0].drawing.ground_aligned = false;
    particles.templates[0].drawing.style = Style::Volume(1);
    let volume = render::mesh(
        emitter,
        &particles.templates[0],
        Affine3A::IDENTITY,
        Affine3A::IDENTITY,
    )
    .unwrap();
    particles.templates[0].drawing.style = Style::Billboard;
    let ordinary = render::mesh(
        emitter,
        &particles.templates[0],
        Affine3A::IDENTITY,
        Affine3A::IDENTITY,
    )
    .unwrap();
    assert_eq!(
        vertices(&volume)
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect::<Vec<_>>(),
        vertices(&ordinary)
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        volume.indices().unwrap().iter().collect::<Vec<_>>(),
        ordinary.indices().unwrap().iter().collect::<Vec<_>>()
    );
    assert_eq!(
        position([1.0; 3], [1.0; 3], 8.0, 6, 5).map(f32::to_bits),
        [1.0_f32; 3].map(f32::to_bits)
    );
}

#[test]
fn volume_buffer_retains_native_512_point_cap_instead_of_batching_remainder() {
    let particles = fixture(2, false, 513.0);
    let mesh = render::mesh(
        &particles.emitters[0],
        &particles.templates[0],
        Affine3A::IDENTITY,
        Affine3A::IDENTITY,
    )
    .unwrap();
    assert_eq!(vertices(&mesh).len(), 512 * 2 * 4);
}
