//! Original `LeftHUD` radar: native terrain samples, shroud, blips and pointer commands.

use crate::{
    control_bar::UiData,
    input::{CommandMode, Mouse},
    playtest::{CameraRig, Match},
    playtest_ui::Layer,
    terrain::Terrain,
};
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use generals_assets::{map::PolygonTrigger, virtual_assets::VirtualAssets};
use generals_sim::skirmish::visibility::{RadarPriority, Status};
use std::error::Error;

pub mod bridge_destruction_capture;
mod bridges;
mod events;
pub mod projection;
mod view_box;
pub use events::Edge as EventEdge;
pub use events::update as event_update;
pub use view_box::update as view_box;

#[derive(Resource)]
pub struct Radar {
    terrain: Vec<u8>,
    bridges: bridges::Paint,
    overlay: Vec<u8>,
    pixels: Vec<u8>,
    colors: Vec<[u8; 3]>,
    world: Vec2,
    average_z: f32,
    image: Option<Handle<Image>>,
    generation: Option<u32>,
    last_overlay: Option<u64>,
    viewer: Option<usize>,
    events: events::Events,
}

#[derive(Component)]
pub struct Display;

impl Radar {
    pub(crate) fn construction(&mut self, position: [f32; 3], frame: u64, generation: u32) {
        self.events.reset(generation);
        self.events
            .construction(Vec2::new(position[0], position[1]), self.world, frame);
    }

    pub(crate) fn information(&mut self, position: [f32; 3], frame: u64, generation: u32) {
        self.events.reset(generation);
        self.events
            .information(Vec2::new(position[0], position[1]), self.world, frame);
    }

    pub(crate) fn infiltration(&mut self, position: [f32; 3], frame: u64, generation: u32) {
        self.events.reset(generation);
        self.events
            .infiltrate(Vec2::new(position[0], position[1]), self.world, frame);
    }

    pub fn load(
        assets: &VirtualAssets,
        terrain: &Terrain,
        polygons: &[PolygonTrigger],
    ) -> Result<Self, Box<dyn Error>> {
        let (pixels, average_z) = terrain.radar_pixels(assets, polygons)?;
        let colors = crate::playtest_view::player_colors(assets)?
            .into_iter()
            .map(|color| color.map(|value| byte(value * 255.0)))
            .collect();
        Ok(Self {
            terrain: pixels,
            bridges: bridges::Paint::new(),
            overlay: vec![0; 128 * 128 * 4],
            pixels: vec![0; 128 * 128 * 4],
            colors,
            world: Vec2::new(f32::from(terrain.width), f32::from(terrain.height)) * 10.0,
            average_z,
            image: None,
            generation: None,
            last_overlay: None,
            viewer: None,
            events: events::Events::new(),
        })
    }
}

pub fn setup(
    mut commands: Commands,
    mut radar: ResMut<Radar>,
    data: Res<UiData>,
    mut images: ResMut<Assets<Image>>,
) {
    let data = data.into_inner();
    view_box::setup(&mut commands, &mut images);
    events::setup(&mut commands, &mut images);
    let mut image = Image::new(
        Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        radar.terrain.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    let image = images.add(image);
    radar.image = Some(image.clone());
    let area = data.windows["ControlBar.wnd:LeftHUD"];
    commands.spawn((
        Display,
        Layer::Game,
        ImageNode::new(image).with_mode(NodeImageMode::Stretch),
        node(area),
        ZIndex(2),
    ));
}

fn node(area: Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent(area.min.x / 8.0),
        top: percent(area.min.y / 6.0),
        width: percent(area.width() / 8.0),
        height: percent(area.height() / 6.0),
        ..default()
    }
}

pub fn update(
    mut radar: ResMut<Radar>,
    battle: ResMut<Match>,
    data: Res<UiData>,
    results: Res<crate::playtest_ui::ResultsScreen>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut display: Single<(&mut Node, &mut Visibility), With<Display>>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let results = results.into_inner();
    let window = window.into_inner();
    let game = &mut battle.simulation;
    radar.refresh_bridge_terrain(game, battle.generation);
    let viewed = battle.observer_player.unwrap_or(0);
    let visible = battle.started && !results.scores() && game.has_radar(0);
    display.1.set_if_neq(if visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    if !visible {
        return;
    }
    let bounds = area(data, window, radar.world);
    display.0.set_if_neq(Node {
        position_type: PositionType::Absolute,
        left: px(bounds.min.x),
        top: px(bounds.min.y),
        width: px(bounds.width()),
        height: px(bounds.height()),
        ..default()
    });
    let radar = radar.as_mut();
    if radar.generation != Some(battle.generation)
        || radar.viewer != Some(viewed)
        || (game.tick.is_multiple_of(6) && radar.last_overlay != Some(game.tick))
    {
        radar.generation = Some(battle.generation);
        radar.last_overlay = Some(game.tick);
        radar.viewer = Some(viewed);
        radar.refresh_objects(game, viewed);
    }
    let Some(handle) = radar.image.as_ref() else {
        return;
    };
    for y in 0..128_u16 {
        for x in 0..128_u16 {
            let index = (usize::from(127 - y) * 128 + usize::from(x)) * 4;
            let point = [
                f32::from(x) * radar.world.x / 128.0,
                f32::from(y) * radar.world.y / 128.0,
            ];
            let factor = match game.visibility_cell(viewed, point) {
                Status::Clear | Status::Partial => 255_u16,
                Status::Fogged => 128,
                Status::Shrouded => 0,
            };
            let color = if radar.overlay[index + 3] != 0 {
                [
                    radar.overlay[index],
                    radar.overlay[index + 1],
                    radar.overlay[index + 2],
                ]
            } else {
                radar.bridges.terrain_color(&radar.terrain, index)
            };
            for (channel, value) in color.into_iter().enumerate() {
                radar.pixels[index + channel] =
                    u8::try_from(u16::from(value) * factor / 255).expect("radar blend");
            }
            radar.pixels[index + 3] = 255;
        }
    }
    upload(&mut images, handle, &radar.pixels);
}

fn upload(images: &mut Assets<Image>, handle: &Handle<Image>, pixels: &[u8]) {
    if images
        .get(handle)
        .is_some_and(|image| image.data.as_deref() != Some(pixels))
    {
        images
            .get_mut(handle)
            .expect("existing radar image")
            .data
            .as_mut()
            .expect("CPU radar texture")
            .copy_from_slice(pixels);
    }
}

impl Radar {
    fn refresh_objects(&mut self, game: &generals_sim::skirmish::Skirmish, viewed: usize) {
        self.overlay.fill(0);
        // Radar lists prepend newly registered objects; local list draws last.
        for local in [false, true] {
            let mut ordered: Vec<_> = game.entities.iter().enumerate().rev().collect();
            ordered.sort_by_key(|(_, actor)| {
                priority(&game.catalog[actor.type_index].sight.radar_priority)
            });
            for (id, actor) in ordered {
                let unit = &game.catalog[actor.type_index];
                if !actor.alive()
                    || (actor.owner == viewed) != local
                    || matches!(unit.sight.radar_priority, RadarPriority::None)
                    || (matches!(unit.sight.radar_priority, RadarPriority::LocalUnit)
                        && actor.owner != viewed)
                    || !matches!(
                        game.visibility_status(viewed, id),
                        Status::Clear | Status::Partial
                    )
                {
                    continue;
                }
                let color = game
                    .player_colors
                    .get(actor.owner)
                    .copied()
                    .flatten()
                    .and_then(|color| self.colors.get(color))
                    .copied()
                    .unwrap_or([255; 3]);
                let point = Vec2::from_array(actor.position) / self.world * 128.0;
                let Some([x, y]) = pixel(point) else {
                    continue;
                };
                for [column, row] in [[x, y], [x, y + 1], [x + 1, y + 1], [x + 1, y]] {
                    if column < 128 && row < 128 {
                        let index = (usize::from(127 - row) * 128 + usize::from(column)) * 4;
                        self.overlay[index..index + 4]
                            .copy_from_slice(&[color[0], color[1], color[2], 255]);
                    }
                }
            }
        }
    }
}

const fn priority(priority: &RadarPriority) -> u8 {
    match priority {
        RadarPriority::None => 0,
        RadarPriority::Structure => 1,
        RadarPriority::Unit => 2,
        RadarPriority::LocalUnit => 3,
    }
}

pub fn input(
    mut battle: ResMut<Match>,
    mut pointer: ResMut<Mouse>,
    mut rig: ResMut<CameraRig>,
    radar: Res<Radar>,
    data: Res<UiData>,
    window: Single<&Window, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<MouseButton>>,
) {
    let radar = radar.into_inner();
    let data = data.into_inner();
    let window = window.into_inner();
    let buttons = buttons.into_inner();
    if !battle.started
        || battle.paused
        || battle.simulation.end_game_at.is_some()
        || !battle.simulation.has_radar(0)
        || buttons.pressed(MouseButton::Middle)
    {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Some(point) = radar.mouse_world(data, window, cursor) else {
        return;
    };
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let look = battle.selected.is_empty()
        || (pointer.alternate_mouse() && left)
        || (!pointer.alternate_mouse() && right);
    if look {
        rig.focus = point * 0.1;
        return;
    }
    if !battle.simulation.player_active(0) {
        return;
    }
    if matches!(pointer.pending_command, CommandMode::Rally) {
        let height = battle.simulation.ground_height(point.to_array());
        crate::input::rally_order(&mut battle, &mut pointer, point.extend(height));
        return;
    }
    let ids = battle.selected.clone();
    let response = crate::source_audio::VoiceRequest::new(
        &battle.simulation,
        &ids,
        crate::source_audio::Voice::Move,
        false,
    );
    let succeeded = if matches!(pointer.pending_command, CommandMode::AttackMove) {
        let mut success = false;
        for id in ids {
            success |= battle.simulation.attack_move(id, point.to_array()).is_ok();
        }
        success
    } else if matches!(pointer.pending_command, CommandMode::Normal) {
        battle
            .simulation
            .group_move_to(&ids, point.to_array(), false)
            .is_ok()
    } else {
        false
    };
    if succeeded {
        pointer.voices.push(response);
        pointer.move_hints.push(point.to_array());
        pointer.pending_command = CommandMode::Normal;
    }
}

impl Radar {
    pub fn mouse_world(&self, data: &UiData, window: &Window, cursor: Vec2) -> Option<Vec2> {
        projection::mouse_world(
            cursor * window.scale_factor(),
            full_area(data, window),
            self.world,
        )
    }
}

fn area(data: &UiData, window: &Window, world: Vec2) -> Rect {
    let full = full_area(data, window);
    let fitted = projection::fit(
        Rect::from_corners(full.min + Vec2::ONE, full.max - Vec2::ONE),
        world,
    );
    Rect::from_corners(
        fitted.min / window.scale_factor(),
        fitted.max / window.scale_factor(),
    )
}

fn full_area(data: &UiData, window: &Window) -> Rect {
    let source = data.windows["ControlBar.wnd:LeftHUD"];
    let scale = Vec2::new(window.width() / 800.0, window.height() / 600.0) * window.scale_factor();
    Rect::from_corners((source.min * scale).trunc(), (source.max * scale).trunc())
}

fn pixel(point: Vec2) -> Option<[u16; 2]> {
    if point.min_element() < 0.0 || point.max_element() >= 128.0 {
        return None;
    }
    Some([u16::from(byte(point.x)), u16::from(byte(point.y))])
}

fn byte(value: f32) -> u8 {
    let (mut low, mut high) = (0_u8, 255_u8);
    while low < high {
        let middle = low + (high - low) / 2 + 1;
        if f32::from(middle) <= value {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::{AssetApp, AssetPlugin};

    #[test]
    fn unchanged_radar_pixels_do_not_upload_but_blip_and_fog_changes_do() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>();
        let original = [100, 80, 60, 255, 0, 0, 0, 255];
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new(
                Extent3d {
                    width: 2,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                original.to_vec(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ));
        app.update();
        app.world_mut()
            .resource_mut::<Messages<AssetEvent<Image>>>()
            .clear();
        for pixels in [
            original,
            [255, 0, 0, 255, 0, 0, 0, 255],
            [127, 0, 0, 255, 0, 0, 0, 255],
        ] {
            let changed = app
                .world()
                .resource::<Assets<Image>>()
                .get(&image)
                .unwrap()
                .data
                .as_deref()
                != Some(pixels.as_slice());
            upload(&mut app.world_mut().resource_mut(), &image, &pixels);
            app.update();
            let events = app
                .world_mut()
                .resource_mut::<Messages<AssetEvent<Image>>>()
                .drain()
                .filter(|event| matches!(event, AssetEvent::Modified { id } if *id == image.id()))
                .count();
            assert_eq!(events, usize::from(changed));
            assert_eq!(
                app.world()
                    .resource::<Assets<Image>>()
                    .get(&image)
                    .unwrap()
                    .data
                    .as_deref(),
                Some(pixels.as_slice())
            );
            upload(&mut app.world_mut().resource_mut(), &image, &pixels);
            app.update();
            assert!(
                !app.world_mut()
                    .resource_mut::<Messages<AssetEvent<Image>>>()
                    .drain()
                    .any(|event| matches!(event, AssetEvent::Modified { .. }))
            );
        }
    }
}
