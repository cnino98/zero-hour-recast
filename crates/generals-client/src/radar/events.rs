//! Original 64-entry Radar event ring and W3D generic triangle drawing.
use super::{Display, Radar, area, projection};
use crate::{control_bar::UiData, playtest::Match, playtest_ui::Layer, source_audio::SourceAudio};
use bevy::{
    asset::RenderAssetUsages,
    ecs::system::SystemParam,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::FocusPolicy,
    window::PrimaryWindow,
};

enum Kind {
    Construction,
    Information,
    Infiltration,
}

struct Event {
    created: u64,
    radar: Vec2,
    sound_played: bool,
    kind: Kind,
}
pub(super) struct Events {
    slots: [Option<Event>; 64],
    next: usize,
    generation: Option<u32>,
}
impl Events {
    pub(super) fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            next: 0,
            generation: None,
        }
    }
    pub(super) fn reset(&mut self, generation: u32) {
        if self.generation != Some(generation) {
            self.slots.iter_mut().for_each(|slot| *slot = None);
            self.next = 0;
            self.generation = Some(generation);
        }
    }
    pub(super) fn infiltrate(&mut self, point: Vec2, world: Vec2, frame: u64) {
        self.create(point, world, frame, Kind::Infiltration);
    }
    pub(super) fn information(&mut self, point: Vec2, world: Vec2, frame: u64) {
        self.create(point, world, frame, Kind::Information);
    }
    pub(super) fn construction(&mut self, point: Vec2, world: Vec2, frame: u64) {
        self.create(point, world, frame, Kind::Construction);
    }
    fn create(&mut self, point: Vec2, world: Vec2, frame: u64, kind: Kind) {
        self.slots[self.next] = Some(Event {
            created: frame,
            radar: (point / (world / 128.0))
                .trunc()
                .clamp(Vec2::ZERO, Vec2::splat(127.0)),
            sound_played: false,
            kind,
        });
        self.next = (self.next + 1) % self.slots.len();
    }
    fn expire(&mut self, frame: u64) {
        for slot in &mut self.slots {
            // Radar::update uses > dieFrame and ignores zero createFrame.
            if slot
                .as_ref()
                .is_some_and(|event| event.created != 0 && frame > event.created + 120)
            {
                *slot = None;
            }
        }
    }
}
#[derive(Component)]
pub struct Edge {
    slot: usize,
    side: usize,
    infiltration: Handle<Image>,
    information: Handle<Image>,
    construction: Handle<Image>,
}
#[derive(Component)]
struct Clip;
pub(super) fn setup(commands: &mut Commands, images: &mut Assets<Image>) {
    let parent = commands
        .spawn((
            Clip,
            Layer::Game,
            Node {
                overflow: Overflow::clip(),
                ..default()
            },
            Visibility::Hidden,
            FocusPolicy::Pass,
            ZIndex(5),
        ))
        .id();
    let make_image = |pixels| {
        let mut image = Image::new(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::linear();
        image
    };
    let image = images.add(make_image(vec![0, 255, 255, 255, 128, 255, 255, 255]));
    let information = images.add(make_image(vec![255, 255, 0, 255, 255, 255, 128, 255]));
    let construction = images.add(make_image(vec![128, 128, 255, 255, 128, 255, 255, 255]));
    for slot in 0..64 {
        for side in 0..3 {
            commands.spawn((
                Edge {
                    slot,
                    side,
                    infiltration: image.clone(),
                    information: information.clone(),
                    construction: construction.clone(),
                },
                Layer::Game,
                ImageNode::new(image.clone()).with_mode(NodeImageMode::Stretch),
                Node::default(),
                UiTransform::default(),
                Visibility::Hidden,
                FocusPolicy::Pass,
                ZIndex(5),
                ChildOf(parent),
            ));
        }
    }
}

type EdgeView = (
    &'static Edge,
    &'static mut Node,
    &'static mut UiTransform,
    &'static mut ImageNode,
    &'static mut Visibility,
);
type ClipView = (&'static mut Node, &'static mut Visibility);

#[derive(SystemParam)]
pub struct Output<'w, 's> {
    parent: Single<'w, 's, ClipView, (With<Clip>, Without<Display>)>,
    edges: Query<'w, 's, EdgeView, (Without<Display>, Without<Clip>)>,
}

pub fn update(
    mut radar: ResMut<Radar>,
    battle: Res<Match>,
    data: Res<UiData>,
    window: Single<&Window, With<PrimaryWindow>>,
    display: Single<&Visibility, With<Display>>,
    mut audio: ResMut<SourceAudio>,
    output: Output,
) {
    let Output {
        mut parent,
        mut edges,
    } = output;
    let window = window.into_inner();
    let battle = battle.into_inner();
    let data = data.into_inner();
    let display = display.into_inner();
    let factor = window.scale_factor();
    let logical = area(data, window, radar.world);
    let bounds = Rect::from_corners(logical.min * factor, logical.max * factor);
    radar.events.reset(battle.generation);
    radar.events.expire(battle.simulation.tick);
    let visible = battle.started && *display == Visibility::Visible;
    *parent.0 = Node {
        position_type: PositionType::Absolute,
        left: px(logical.min.x),
        top: px(logical.min.y),
        width: px(logical.width()),
        height: px(logical.height()),
        overflow: Overflow::clip(),
        ..default()
    };
    *parent.1 = if visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if visible {
        for event in radar.events.slots.iter_mut().flatten() {
            if !event.sound_played {
                audio.ui("RadarEvent");
                event.sound_played = true;
            }
        }
    }
    for (edge, mut node, mut transform, mut image, mut visibility) in &mut edges {
        *visibility = Visibility::Hidden;
        if !visible {
            continue;
        }
        let Some(event) = &radar.events.slots[edge.slot] else {
            continue;
        };
        let (points, alpha) = triangle(event.radar, event.created, battle.simulation.tick, bounds);
        let Some([start, end]) =
            projection::clip([points[edge.side], points[(edge.side + 1) % 3]], bounds)
        else {
            continue;
        };
        let start = (start - bounds.min) / factor;
        let end = (end - bounds.min) / factor;
        let delta = end - start;
        if delta.length_squared() == 0.0 {
            continue;
        }
        let rect =
            Rect::from_center_size((start + end) * 0.5, Vec2::new(delta.length(), 1.0 / factor));
        *node = Node {
            position_type: PositionType::Absolute,
            left: px(rect.min.x),
            top: px(rect.min.y),
            width: px(rect.width()),
            height: px(rect.height()),
            ..default()
        };
        transform.rotation = Rot2::radians(delta.y.atan2(delta.x));
        image.image = match event.kind {
            Kind::Construction => edge.construction.clone(),
            Kind::Information => edge.information.clone(),
            Kind::Infiltration => edge.infiltration.clone(),
        };
        image.color = Color::srgba_u8(255, 255, 255, alpha);
        *visibility = Visibility::Visible;
    }
}

fn triangle(center: Vec2, created: u64, frame: u64, bounds: Rect) -> ([Vec2; 3], u8) {
    let age = u16::try_from(frame.saturating_sub(created).min(u64::from(u16::MAX)))
        .expect("bounded radar event age");
    let fraction = f32::from(age) / 45.0;
    let size = (bounds.width().trunc() / 2.0 * (1.0 - fraction))
        .trunc()
        .max(6.0);
    let rotation = std::f32::consts::TAU * fraction;
    let points = [
        0.0,
        std::f32::consts::TAU / 3.0,
        -std::f32::consts::TAU / 3.0,
    ]
    .map(|angle| {
        let angle = angle - rotation;
        let point = (center + Vec2::new(angle.cos(), angle.sin()) * size).trunc();
        bounds.min + (Vec2::new(point.x, 127.0 - point.y) * bounds.size().trunc() / 128.0).trunc()
    });
    let alpha = if age <= 105 {
        255
    } else {
        super::byte(255.0 * (1.0 - f32::from(age - 105) / 15.0))
    };
    (points, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    fn scaled_marker_world() -> (World, Rect) {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let map = generals_assets::map::MapData::parse(
            &assets.read(crate::playtest::MAP_NAME).unwrap().bytes,
        )
        .unwrap();
        let terrain = crate::terrain::Terrain::new(map.heightmap, map.tiles, true, true).unwrap();
        let options = generals_assets::game_root()
            .unwrap()
            .join("UserData/Options.ini");
        let ui = crate::control_bar::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let audio =
            crate::source_audio::load(&assets, &battle.simulation.catalog, &options).unwrap();
        let mut radar = Radar::load(&assets, &terrain, &map.triggers).unwrap();
        battle.started = true;
        battle.simulation.tick = 12;
        let frame = battle.simulation.tick;
        radar.events.reset(battle.generation);
        radar
            .events
            .infiltrate(radar.world * 0.5, radar.world, frame - 12);
        let mut window = Window::default();
        window.resolution.set_scale_factor_override(Some(2.0));
        let logical = area(&ui, &window, radar.world);
        let mut world = World::new();
        world.insert_resource(battle);
        world.insert_resource(ui);
        world.insert_resource(audio);
        world.insert_resource(radar);
        world.insert_resource(Assets::<Image>::default());
        world.spawn((window, PrimaryWindow));
        world.run_system_once(super::super::setup).unwrap();
        *world
            .query_filtered::<&mut Visibility, With<Display>>()
            .single_mut(&mut world)
            .unwrap() = Visibility::Visible;
        world.run_system_once(update).unwrap();
        (world, logical)
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn marker_update_keeps_rotated_lines_local_to_clipped_radar_at_double_scale() {
        let (mut world, logical) = scaled_marker_world();
        let mut map = world.query_filtered::<&ImageNode, With<Display>>();
        // W3DRadar fits the world aspect once, then stretches its 128-square
        // texture across those bounds shared by events and the view rectangle.
        assert_eq!(
            map.single(&world).unwrap().image_mode,
            NodeImageMode::Stretch
        );
        let mut parents = world.query_filtered::<&Node, With<Clip>>();
        let parent = parents.single(&world).unwrap();
        assert_eq!(parent.left, px(logical.min.x));
        assert_eq!(parent.top, px(logical.min.y));
        assert_eq!(parent.width, px(logical.width()));
        assert_eq!(parent.height, px(logical.height()));
        assert_eq!(parent.overflow, Overflow::clip());
        let mut lines = world
            .query_filtered::<(&Node, &UiTransform, &Visibility, &ChildOf, &ImageNode), With<Edge>>(
            );
        let mut visible = 0;
        let mut long_lines = 0;
        for (node, transform, visibility, parent, image) in lines.iter(&world) {
            if *visibility != Visibility::Visible {
                continue;
            }
            visible += 1;
            assert!(world.get::<Clip>(parent.parent()).is_some());
            let (Val::Px(left), Val::Px(top), Val::Px(width), Val::Px(height)) =
                (node.left, node.top, node.width, node.height)
            else {
                panic!("radar line must use pixel coordinates")
            };
            // Auto preserves the two-pixel texture's aspect and extracts only
            // a two-pixel dash. Stretch must fill the original line rectangle.
            assert_eq!(image.image_mode, NodeImageMode::Stretch);
            assert_eq!(height.to_bits(), 0.5_f32.to_bits());
            if width > 1.0 {
                long_lines += 1;
            }
            let center = Vec2::new(width.mul_add(0.5, left), height.mul_add(0.5, top));
            let delta = transform.rotation * Vec2::new(width * 0.5, 0.0);
            for point in [center - delta, center + delta] {
                // Rotation reconstructs the clipped endpoints within float rounding.
                assert!(point.cmpge(Vec2::splat(-0.0001)).all());
                assert!(point.cmple(logical.size() + Vec2::splat(0.0001)).all());
            }
        }
        assert!(visible > 0);
        assert!(long_lines > 0);
    }
    #[test]
    fn infiltration_outside_map_clamps_to_original_radar_boundary() {
        let mut events = Events::new();
        events.infiltrate(Vec2::new(-20.0, 1040.0), Vec2::splat(1000.0), 2);
        assert_eq!(
            events.slots[0].as_ref().unwrap().radar,
            Vec2::new(0.0, 127.0)
        );
    }
    #[test]
    fn ring_replaces_oldest_at_sixty_four_and_exact_deadline_stays_active() {
        let mut events = Events::new();
        events.reset(7);
        for frame in 1..=65 {
            events.infiltrate(Vec2::splat(500.0), Vec2::splat(1000.0), frame);
        }
        assert_eq!(events.slots.iter().flatten().count(), 64);
        assert_eq!(events.slots[0].as_ref().unwrap().created, 65);
        events.expire(122);
        assert_eq!(events.slots.iter().flatten().count(), 64);
        events.expire(123);
        assert_eq!(events.slots.iter().flatten().count(), 63);
        events.reset(8);
        assert!(events.slots.iter().all(Option::is_none));
        assert_eq!(events.next, 0);
    }
    #[test]
    fn marker_clips_original_integer_vertices_and_fades_only_during_last_half_second() {
        let bounds = Rect::new(100.0, 200.0, 228.0, 328.0);
        let (points, alpha) = triangle(Vec2::splat(64.0), 10, 10, bounds);
        assert_eq!(points[0], Vec2::new(228.0, 263.0));
        assert_eq!(alpha, 255);
        for side in 0..3 {
            let line = projection::clip([points[side], points[(side + 1) % 3]], bounds).unwrap();
            assert!(
                line.into_iter()
                    .all(|point| point.cmpge(bounds.min).all() && point.cmple(bounds.max).all())
            );
        }
        assert_eq!(triangle(Vec2::splat(64.0), 10, 115, bounds).1, 255);
        assert_eq!(triangle(Vec2::splat(64.0), 10, 116, bounds).1, 238);
        assert_eq!(triangle(Vec2::splat(64.0), 10, 130, bounds).1, 0);
    }
}
