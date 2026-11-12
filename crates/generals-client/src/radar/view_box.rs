//! Native camera footprint at the map's average terrain height.

use super::{Display, Radar, full_area, projection};
use crate::{control_bar::UiData, playtest_ui::Layer, playtest_view::WorldCamera};
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};

#[derive(Component)]
pub struct Backdrop;

#[derive(Component)]
pub struct Edge(usize);

type BackdropNode = (&'static mut Node, &'static mut Visibility);
type BackdropFilter = (With<Backdrop>, Without<Display>, Without<Edge>);
type EdgeNode = (
    &'static Edge,
    &'static mut Node,
    &'static mut UiTransform,
    &'static mut Visibility,
);

pub(super) fn setup(commands: &mut Commands, images: &mut Assets<Image>) {
    let top = [225, 225, 0, 255];
    let bottom = [158, 158, 0, 255];
    commands.spawn((
        Backdrop,
        Layer::Game,
        Node::default(),
        BackgroundColor(Color::BLACK),
        ZIndex(1),
    ));
    for (index, colors) in [[top, top], [top, bottom], [bottom, bottom], [bottom, top]]
        .into_iter()
        .enumerate()
    {
        let mut image = Image::new(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            colors.concat(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::linear();
        commands.spawn((
            Edge(index),
            Layer::Game,
            ImageNode::new(images.add(image)),
            Node::default(),
            UiTransform::default(),
            ZIndex(3),
        ));
    }
}

pub fn update(
    radar: Res<Radar>,
    data: Res<UiData>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<WorldCamera>>,
    display: Single<&Visibility, With<Display>>,
    mut backdrop: Single<BackdropNode, BackdropFilter>,
    mut edges: Query<EdgeNode, (Without<Backdrop>, Without<Display>)>,
) {
    let radar = radar.into_inner();
    let data = data.into_inner();
    let window = window.into_inner();
    let camera = camera.into_inner();
    let display = display.into_inner();
    let factor = window.scale_factor();
    let bounds = full_area(data, window);
    *backdrop.0 = rect_node(Rect::from_corners(bounds.min / factor, bounds.max / factor));
    *backdrop.1 = *display;
    let fitted = projection::fit(
        Rect::from_corners(bounds.min + Vec2::ONE, bounds.max - Vec2::ONE),
        radar.world,
    );
    let corners = camera_points(camera, radar.average_z)
        .map(|points| points.map(|point| projection::world_pixel(point, fitted, radar.world)));
    for (edge, mut node, mut transform, mut visibility) in &mut edges {
        *visibility = Visibility::Hidden;
        if !matches!(*display, Visibility::Visible) {
            continue;
        }
        let Some(points) = corners else { continue };
        let Some([start, end]) =
            projection::clip([points[edge.0], points[(edge.0 + 1) % 4]], bounds)
        else {
            continue;
        };
        let start = start / factor;
        let end = end / factor;
        let displacement = end - start;
        let length = displacement.length();
        if length == 0.0 {
            continue;
        }
        let center = (start + end) * 0.5;
        *node = rect_node(Rect::from_center_size(
            center,
            Vec2::new(length, 1.0 / factor),
        ));
        transform.rotation = Rot2::radians(displacement.y.atan2(displacement.x));
        *visibility = Visibility::Visible;
    }
}

fn rect_node(rect: Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(rect.min.x),
        top: px(rect.min.y),
        width: px(rect.width()),
        height: px(rect.height()),
        ..default()
    }
}

fn camera_points(
    (camera, transform): (&Camera, &GlobalTransform),
    average_z: f32,
) -> Option<[Vec2; 4]> {
    let bounds = camera.logical_viewport_rect()?;
    let pixels = [
        bounds.min,
        Vec2::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Vec2::new(bounds.min.x, bounds.max.y),
    ];
    let mut points = [Vec2::ZERO; 4];
    for (point, pixel) in points.iter_mut().zip(pixels) {
        let ray = camera.viewport_to_world(transform, pixel).ok()?;
        let distance =
            ray.intersect_plane(Vec3::Y * (average_z * 0.1), InfinitePlane3d::new(Vec3::Y))?;
        let world = ray.get_point(distance);
        *point = Vec2::new(world.x, -world.z) * 10.0;
    }
    Some(points)
}
