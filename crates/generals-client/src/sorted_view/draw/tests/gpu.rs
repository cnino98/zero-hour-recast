//! Read back the actual transparent phase with two runs sharing one mesh allocation.
use crate::sorted_view::{SortedBatch, SortedRange, draw};
use bevy::{
    app::PluginsState,
    asset::RenderAssetUsages,
    audio::AudioPlugin,
    camera::RenderTarget,
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
    render::{
        RenderApp, RenderPlugin,
        batching::NoAutomaticBatching,
        gpu_readback::{Readback, ReadbackComplete},
        mesh::allocator::MeshAllocator,
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::{PrimitiveTopology, TextureFormat, TextureUsages},
        view::NoIndirectDrawing,
    },
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use std::{
    ops::Range,
    time::{Duration, Instant},
};

#[derive(Resource, Default)]
struct Pixels(Vec<u8>);

fn scene() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>()
            .disable::<AudioPlugin>()
            .disable::<PipelinedRenderingPlugin>(),
    );
    draw::register(&mut app);
    let start = Instant::now();
    while app.plugins_state() == PluginsState::Adding {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "GPU initialization timed out"
        );
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    app.init_resource::<Pixels>();
    let mut image = Image::new_target_texture(64, 64, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.world_mut().spawn((
        Camera3d::default(),
        RenderTarget::Image(target.clone().into()),
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        NoIndirectDrawing,
        Msaa::Off,
        Tonemapping::None,
    ));
    app.world_mut().spawn(Readback::texture(target)).observe(
        |event: On<ReadbackComplete>, mut pixels: ResMut<Pixels>| {
            pixels.0.clone_from(&event.data);
        },
    );
    app
}

fn triangles(swapped: bool, extra: usize) -> Mesh {
    let left = [[-1.1, -0.7, 0.0], [-0.1, -0.7, 0.0], [-0.6, 0.8, 0.0]];
    let right = [[0.1, -0.7, 0.0], [1.1, -0.7, 0.0], [0.6, 0.8, 0.0]];
    let mut points: Vec<_> = if swapped {
        [right, left]
    } else {
        [left, right]
    }
    .into_iter()
    .flatten()
    .collect();
    points.resize(points.len() + extra, [0.0, 0.0, 0.0]);
    let normals = vec![[0.0, 0.0, 1.0]; points.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

fn slab(app: &App, mesh: &Handle<Mesh>) -> Option<Range<u32>> {
    app.sub_app(RenderApp)
        .world()
        .resource::<MeshAllocator>()
        .mesh_vertex_slice(&mesh.id())
        .map(|slice| slice.range)
}

fn wait(app: &mut App, condition: impl Fn(&App) -> bool) {
    let start = Instant::now();
    loop {
        app.update();
        if condition(app) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(45),
            "sorted GPU readback timed out"
        );
        std::thread::yield_now();
    }
}

fn correct_pixels(app: &App, swapped: bool) -> bool {
    let bytes = &app.world().resource::<Pixels>().0;
    // A 64-pixel RGBA row is exactly the GPU copy alignment.
    let pixel = |x| bytes.get((32 * 64 + x) * 4..(32 * 64 + x) * 4 + 3);
    let (Some(left), Some(right)) = (pixel(17_usize), pixel(47_usize)) else {
        return false;
    };
    let (red, green) = if swapped {
        (right, left)
    } else {
        (left, right)
    };
    red[0] > 96 && red[1] < 32 && green[1] > 96 && green[0] < 32
}

#[ignore = "requires GPU device"]
#[test]
fn direct_draws_keep_neighboring_materials_separate_after_shared_slab_growth() {
    let mut app = scene();
    let prefix = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(triangles(false, 30));
    wait(&mut app, |app| slab(app, &prefix).is_some());
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(triangles(false, 0));
    for (range, color) in [
        (0..3, Color::srgb(1.0, 0.0, 0.0)),
        (3..6, Color::srgb(0.0, 1.0, 0.0)),
    ] {
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: color,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            });
        app.world_mut().spawn((
            SortedBatch,
            SortedRange(range),
            NoAutomaticBatching,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::IDENTITY,
        ));
    }
    wait(&mut app, |app| correct_pixels(app, false));
    let original = slab(&app, &mesh).unwrap();
    assert!(
        original.start > 0,
        "shared mesh must follow a live slab neighbor"
    );
    let neighbor = slab(&app, &prefix).unwrap();
    *app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&mesh)
        .unwrap() = triangles(true, 6000);
    wait(&mut app, |app| correct_pixels(app, true));
    let grown = slab(&app, &mesh).unwrap();
    assert!(grown.end - grown.start >= 6006);
    assert_eq!(slab(&app, &prefix).unwrap(), neighbor);
}
