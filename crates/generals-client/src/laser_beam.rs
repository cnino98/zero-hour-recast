//! Authored `LaserBeam`: `LaserUpdate` endpoints and single-segment `W3DLaserDraw`.
mod native;
mod pulses;
mod streams;
pub use pulses::{PulseInspection, PulseVisual};
pub use streams::StreamVisual;
pub mod verification;

use crate::{
    model_animation::{Animations, Players},
    playtest::Match,
    playtest_view::{UnitVisual, WorldCamera},
    source_material::SourceMaterial,
};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use generals_assets::virtual_assets::VirtualAssets;
use std::error::Error;

pub struct Beam {
    pub width: f32,
    pub color: [f32; 3],
}
#[derive(Resource)]
pub struct Lasers {
    native: native::Lasers,
    pulses: pulses::Pulses,
    streams: Vec<streams::Streams>,
}

pub fn load(
    assets: &VirtualAssets,
    catalog: &[generals_sim::skirmish::UnitType],
) -> Result<Lasers, Box<dyn Error>> {
    Ok(Lasers {
        native: native::Lasers::new(),
        pulses: pulses::load(assets)?,
        streams: streams::load_feedback(assets, catalog)?,
    })
}

impl Lasers {
    pub fn retire_native(&mut self, object: usize, particles: &mut crate::particles::Particles) {
        self.native.retire(object, particles);
    }

    pub fn initialize_native(
        &mut self,
        battle: &Match,
        index: usize,
        animations: &mut Animations,
        particles: &mut crate::particles::Particles,
        visuals: &Query<(&UnitVisual, &Players, &Transform)>,
    ) {
        self.native.initialize(
            battle,
            &battle.simulation.presentation.laser_initializations[index],
            animations,
            particles,
            visuals,
        );
    }

    pub fn record_pulse(
        &mut self,
        battle: &Match,
        index: usize,
        animations: &mut Animations,
        particles: &mut crate::particles::Particles,
        models: &Query<(&UnitVisual, &Players, &Transform)>,
        now: std::time::Duration,
    ) {
        self.pulses.record(
            battle,
            &battle.simulation.presentation.laser_shots[index],
            animations,
            particles,
            models,
            now,
        );
    }
}

pub fn color(line: &generals_assets::content::ContentLine) -> Result<[f32; 4], Box<dyn Error>> {
    let mut result = [0.0; 4];
    for (index, key) in ["R", "G", "B", "A"].into_iter().enumerate() {
        let value = line
            .tokens
            .iter()
            .find_map(|token| {
                token
                    .split_once(':')
                    .filter(|(name, _)| *name == key)
                    .map(|(_, value)| value)
            })
            .ok_or("incomplete laser color")?
            .parse::<u8>()?;
        result[index] = f32::from(value) / 255.0;
    }
    Ok(result)
}

pub fn beam(index: u16, count: u16, widths: [f32; 2], inner: [f32; 4], outer: [f32; 4]) -> Beam {
    if count == 1 {
        return Beam {
            width: widths[0],
            color: std::array::from_fn(|axis| inner[axis] * inner[3]),
        };
    }
    let scale = f32::from(index) / (f32::from(count) - 1.0);
    let width_delta = scale * (widths[1] - widths[0]);
    Beam {
        width: widths[0] + width_delta,
        color: std::array::from_fn(|axis| {
            let scaled_delta = scale * (outer[axis] - inner[axis]);
            let alpha_delta = scaled_delta * inner[3];
            inner[axis] + alpha_delta
        }),
    }
}

pub fn record(
    battle: Res<Match>,
    mut lasers: ResMut<Lasers>,
    mut particles: ResMut<crate::particles::Particles>,
    timings: Option<Res<crate::render_performance::Timings>>,
) {
    let timings = timings.map(Res::into_inner);
    let _span = timings.map(|timings| timings.span("laser_record"));
    let battle = battle.into_inner();
    lasers.pulses.maintain(battle, &mut particles);
    let Lasers {
        native, streams, ..
    } = &mut *lasers;
    for stream in streams {
        stream.record_native(battle, native);
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct RenderResources<'w> {
    terrain: Res<'w, crate::terrain::Terrain>,
    time: Res<'w, Time>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    scene_materials: ResMut<'w, Assets<crate::particles::smudge::SceneMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    timings: Option<Res<'w, crate::render_performance::Timings>>,
}

/// `GameClient` updates `Drawable` client modules before drawing new skeletons.
pub fn update_native(
    battle: Res<Match>,
    mut lasers: ResMut<Lasers>,
    mut particles: ResMut<crate::particles::Particles>,
    models: Query<(&UnitVisual, &Players, &Transform)>,
) {
    lasers
        .native
        .update(battle.into_inner(), &mut particles, &models);
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct FeedbackVisuals<'w, 's> {
    streams: Query<'w, 's, (Entity, &'static mut streams::StreamVisual, &'static Mesh3d)>,
    pulses: pulses::Presentation<'w, 's>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut lasers: ResMut<Lasers>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut resources: RenderResources,
    feedback: FeedbackVisuals,
) {
    let FeedbackVisuals {
        mut streams,
        pulses,
    } = feedback;
    let timings = resources.timings.take();
    let _span = timings.as_deref().map(|timings| timings.span("laser_draw"));
    let battle = battle.into_inner();
    let Lasers {
        native,
        streams: sources,
        ..
    } = &mut *lasers;
    for stream in sources {
        stream.record_native(battle, native);
    }
    let delta = resources.time.delta_secs();
    let now = resources.time.elapsed();
    let camera = camera.into_inner();
    lasers
        .pulses
        .render(&mut commands, battle, camera, &mut resources, &pulses, now);
    for stream in &mut lasers.streams {
        stream.render(
            &mut commands,
            battle,
            camera,
            &mut resources,
            &mut streams,
            delta,
        );
    }
}

pub fn geometry(
    beams: &[Beam],
    points: [[f32; 3]; 2],
    camera: &Transform,
) -> Result<Mesh, &'static str> {
    let world = camera.compute_affine();
    let inverse = world.inverse();
    let points = points.map(|p| inverse.transform_point3(Vec3::new(p[0], p[2], -p[1]) / 10.0));
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut uv = Vec::new();
    let mut indices = Vec::new();
    for beam in beams {
        // SegLineRenderer and StreakRenderer share the two-endpoint silhouette
        // planes. SegLine's default tiling factor gives V=0 and V=1 at the ends.
        let strip = crate::particles::streak::geometry(
            &mut points.clone(),
            &[beam.width / 20.0; 2],
            &[0; 2],
        )?;
        if strip.vertices.len() != 4 {
            return Err("single laser segment changed vertex count");
        }
        let start = u32::try_from(positions.len()).map_err(|_| "laser mesh too large")?;
        positions.extend(
            strip
                .vertices
                .into_iter()
                .map(|point| world.transform_point3(point).to_array()),
        );
        colors.extend([[beam.color[0], beam.color[1], beam.color[2], 1.0]; 4]);
        uv.extend([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
        indices.extend(strip.indices.into_iter().map(|index| start + index));
    }
    Ok(Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; colors.len()])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices)))
}

#[cfg(test)]
mod tests {
    use super::{beam, geometry};
    use bevy::prelude::*;

    #[test]
    fn authored_three_beams_keep_native_inner_alpha_color_equation_and_full_width() {
        let inner = [1.0, 1.0, 1.0, 250.0 / 255.0];
        let outer = [1.0, 0.0, 0.0, 150.0 / 255.0];
        let outer_beam = beam(2, 3, [0.4, 1.2], inner, outer);
        assert_eq!(outer_beam.width.to_bits(), 1.2f32.to_bits());
        assert_eq!(outer_beam.color[0].to_bits(), 1.0f32.to_bits());
        assert_eq!(
            outer_beam.color[1].to_bits(),
            (1.0f32 - 250.0 / 255.0).to_bits()
        );
        let beam = beam(0, 3, [0.4, 1.2], inner, outer);
        assert_eq!(beam.color.map(f32::to_bits), [1.0f32.to_bits(); 3]);
    }

    #[test]
    fn coincident_endpoints_use_original_perturbation_and_produce_finite_segment_mesh() {
        let beam = beam(0, 1, [0.4, 1.2], [1.0; 4], [1.0; 4]);
        let camera = Transform::from_xyz(0.0, 100.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        let mesh = geometry(&[beam], [[100.0, 200.0, 0.0]; 2], &camera).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("laser positions missing");
        };
        assert_eq!(points.len(), 4);
        assert!(points.iter().flatten().all(|value| value.is_finite()));
    }
}
