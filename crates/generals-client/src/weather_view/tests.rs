use super::{data::Settings, geometry};
use bevy::{mesh::VertexAttributeValues, prelude::*};

fn settings() -> Settings {
    Settings {
        enabled: true,
        texture: "EXRainDrop.tga".into(),
        frequency: [1.0; 2],
        amplitude: 0.1,
        point_size: 0.3,
        point_bounds: [0.0, 64.0],
        quad_size: 0.1,
        dimension: 100.0,
        density: 1.0,
        velocity: 25.0,
        points: true,
    }
}

#[test]
fn original_weather_noise_uses_crt_sequence_and_wrapped_camera_lattice() {
    let mut seed = 1;
    let heights = geometry::noise(100.0, &mut seed);
    assert_eq!(heights.len(), 4096);
    assert_eq!(&heights[..4], &[41.0, 67.0, 34.0, 0.0]);
    assert!(heights.iter().all(|height| (0.0..100.0).contains(height)));
}

#[test]
fn map_weather_override_advances_noise_without_resetting_time_or_reseeding_restart() {
    let mut baseline = settings();
    baseline.enabled = false;
    baseline.texture = "EXSnowFlake.tga".into();
    baseline.dimension = 200.0;
    let mut rain = super::WeatherView::configure(baseline.clone(), Some(settings()), None);
    // Initialization consumes the first 4096 CRT values before map.ini invokes
    // updateIniSettings. The first rain map starts at the second noise block.
    assert_eq!(&rain.noise[..4], &[58.0, 22.0, 59.0, 97.0]);
    rain.time = 2.75;
    let clear = super::WeatherView::configure(baseline.clone(), None, Some(&rain));
    assert!(!clear.settings.enabled);
    assert_eq!(clear.settings.texture, "EXSnowFlake.tga");
    assert_eq!(clear.settings.dimension.to_bits(), 100.0_f32.to_bits());
    assert_eq!(clear.noise, rain.noise);
    assert_eq!(clear.random, rain.random);
    assert_eq!(clear.time.to_bits(), 2.75_f32.to_bits());
    let restarted = super::WeatherView::configure(baseline, Some(settings()), Some(&clear));
    assert_eq!(&restarted.noise[..4], &[95.0, 84.0, 28.0, 94.0]);
    assert_eq!(restarted.time.to_bits(), 2.75_f32.to_bits());
    assert_eq!(rain.random, 613_392_385);
    assert_eq!(restarted.random, 1_826_058_241);
    rain.begin_generation(1);
    assert_eq!(rain.random, 613_392_385);
    rain.begin_generation(2);
    assert_eq!(rain.noise, restarted.noise);
    assert_eq!(rain.time.to_bits(), 2.75_f32.to_bits());
    rain.begin_generation(2);
    assert_eq!(rain.random, restarted.random);
}

#[test]
fn point_sprites_preserve_native_pixel_attenuation_and_hardware_size_limits() {
    let settings = settings();
    assert_eq!(
        geometry::point_pixels(&settings, 600.0, 30.0).to_bits(),
        6.0_f32.to_bits()
    );
    assert_eq!(
        geometry::point_pixels(&settings, 600.0, 1.0).to_bits(),
        64.0_f32.to_bits()
    );
    assert_eq!(
        geometry::point_pixels(&settings, 1200.0, 60.0).to_bits(),
        6.0_f32.to_bits()
    );
}

#[test]
fn weather_lattice_moves_down_with_source_velocity_and_quad_fallback_keeps_authored_size() {
    let mut settings = settings();
    settings.points = false;
    settings.amplitude = 0.0;
    settings.density = 0.1;
    settings.dimension = 20.0;
    // A nonzero noise height puts the lattice below the camera. With zero
    // noise at t=.4 every vertex lies in/behind the original fixture camera
    // plane, so the source-facing cull correctly emitted no visible quads.
    let heights = vec![5.0; 4096];
    let camera =
        Transform::from_xyz(0.0, 10.0, 5.0).looking_at(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z);
    let projection = Mat4::perspective_rh(1.0, 1.0, 0.1, 1000.0);
    let mesh = geometry::mesh(
        &settings,
        &heights,
        0.4,
        &camera,
        projection,
        Vec2::splat(600.0),
    );
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("weather positions")
    };
    assert_eq!(points.len(), 16);
    for quad in points.chunks_exact(4) {
        let width = Vec3::from_array(quad[3]).distance(Vec3::from_array(quad[0]));
        assert!((width - 0.01).abs() < 0.000_001);
    }
    let later = geometry::mesh(
        &settings,
        &heights,
        0.5,
        &camera,
        projection,
        Vec2::splat(600.0),
    );
    let Some(VertexAttributeValues::Float32x3(later_points)) =
        later.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("later weather positions")
    };
    assert_eq!(later_points.len(), points.len());
    for (first, second) in points.iter().zip(later_points) {
        // 25 native units/second * .1 second * .1 backend scale.
        assert!((first[1] - second[1] - 0.25).abs() < 0.000_001);
        assert_eq!(first[0].to_bits(), second[0].to_bits());
        assert_eq!(first[2].to_bits(), second[2].to_bits());
    }
}
