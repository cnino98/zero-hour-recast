//! Original Drawable veterancy insignia and Object/InGameUI level-gain animation.
use crate::{
    control_bar::UiData,
    playtest::{CameraRig, Match},
    playtest_view::{self, WorldCamera},
    skirmish_data::{definition, duration_frames, field, number, scalar, text, unsigned},
    source_audio::SourceAudio,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::visibility::Status;
use std::error::Error;

const INSIGNIA: [&str; 3] = ["SCVeter1", "SCVeter2", "SCVeter3"];

mod verification;
pub use verification::VeterancyInspection;

#[derive(Resource)]
pub struct VeterancyFeedback {
    images: Vec<String>,
    delay: u32,
    duration: u32,
    rise: f32,
}

impl VeterancyFeedback {
    pub fn images(&self) -> impl Iterator<Item = &str> {
        INSIGNIA
            .into_iter()
            .chain(self.images.iter().map(String::as_str))
    }
}

pub fn load(assets: &VirtualAssets) -> Result<VeterancyFeedback, Box<dyn Error>> {
    let files = [
        content::scan(assets.read("data/ini/gamedata.ini")?)?,
        content::scan(assets.read("data/ini/animation2d.ini")?)?,
    ];
    parse(&files)
}

fn parse(files: &[content::ContentFile]) -> Result<VeterancyFeedback, Box<dyn Error>> {
    let global = &definition(files, "GameData", "")?.lines;
    let animation = &definition(
        files,
        "Animation",
        text(global, 0, "LevelGainAnimationName")?,
    )?
    .lines;
    if text(animation, 0, "AnimationMode")? != "LOOP"
        || field(animation, 0, "RandomizeStartFrame")
            .map(scalar)
            .transpose()?
            .is_some_and(|value| !value.eq_ignore_ascii_case("No"))
        || field(animation, 0, "ImageSequence").is_some()
    {
        return Err("unrepresented original level-gain animation mode".into());
    }
    let images = animation
        .iter()
        .filter(|line| {
            line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "Image")
        })
        .map(|line| scalar(line).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let duration = number(global, 0, "LevelGainAnimationTime")?;
    let rise = number(global, 0, "LevelGainAnimationZRise")?;
    if images.is_empty()
        || images.len() != usize::try_from(unsigned(animation, 0, "NumberImages")?)?
        || !duration.is_finite()
        || duration <= 0.0
        || !rise.is_finite()
    {
        return Err("invalid original level-gain animation values".into());
    }
    // InGameUI::addWorldAnimation converts seconds directly to unsigned frames.
    let duration = (duration * 30.0).trunc().to_string().parse::<u32>()?;
    Ok(VeterancyFeedback {
        images,
        delay: duration_frames(
            field(animation, 0, "AnimationDelay").ok_or("missing animation delay")?,
        )?,
        duration,
        rise,
    })
}

#[derive(Component)]
pub struct Promotion {
    generation: u32,
    expires: u64,
    position: Vec3,
    map_position: [f32; 2],
    frame: usize,
    last_update: u64,
}

impl Promotion {
    fn advance(&mut self, rules: &VeterancyFeedback, tick: u64, elapsed: f32) {
        // Anim2D updates once when due, including in fog; it does not catch up
        // by skipping image frames when several logic frames elapsed.
        if tick - self.last_update >= u64::from(rules.delay) {
            self.frame = (self.frame + 1) % rules.images.len();
            self.last_update = tick;
        }
        // SuperHackers decouples the original per-render rise from FPS.
        let rise = rules.rise / 30.0 * (elapsed * 30.0) * 0.1;
        self.position.y += rise;
    }
}

pub fn record(
    mut commands: Commands,
    mut battle: ResMut<Match>,
    rules: Res<VeterancyFeedback>,
    data: Res<UiData>,
    mut audio: ResMut<SourceAudio>,
) {
    let rules = rules.into_inner();
    let data = data.into_inner();
    if !battle.started || battle.paused {
        return;
    }
    let viewed = if battle.simulation.defeated[0] {
        battle.observer_player
    } else {
        Some(0)
    };
    for event in std::mem::take(&mut battle.simulation.presentation.promotion_events) {
        let clear = matches!(
            battle
                .simulation
                .visibility_cell(viewed.unwrap_or(0), [event.position[0], event.position[1]]),
            Status::Clear
        );
        audio.promoted(
            event.object,
            event.position,
            battle.simulation.entities[event.object].owner,
            viewed,
            clear,
        );
        commands.spawn((
            Promotion {
                generation: battle.generation,
                expires: event.frame + u64::from(rules.duration),
                position: Vec3::new(event.position[0], event.position[2], -event.position[1]) * 0.1,
                map_position: [event.position[0], event.position[1]],
                frame: 0,
                // Anim2D's constructor resets this after reset/setCurrentFrame.
                last_update: 0,
            },
            data.image(&rules.images[0]),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            FocusPolicy::Pass,
            ZIndex(2),
        ));
    }
}

#[derive(SystemParam)]
pub struct Projection<'w, 's> {
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    rig: Res<'w, CameraRig>,
    terrain: Res<'w, Terrain>,
}

struct ScreenProjection<'a> {
    camera: (&'a Camera, &'a GlobalTransform),
    scale: f32,
    zoom: f32,
}

impl<'w> Projection<'w, '_> {
    fn into_view(self) -> ScreenProjection<'w> {
        let rig = self.rig.into_inner();
        let terrain = self.terrain.into_inner();
        let ground = terrain.sample(rig.focus);
        ScreenProjection {
            camera: self.camera.into_inner(),
            scale: self.window.into_inner().scale_factor(),
            zoom: (ground + rig.distance) / (ground + rig.settings.max_height),
        }
    }
}

impl ScreenProjection<'_> {
    fn screen(&self, position: Vec3) -> Option<Vec2> {
        let point = self
            .camera
            .0
            .world_to_viewport(self.camera.1, position)
            .ok()?;
        self.camera
            .0
            .logical_viewport_rect()?
            .contains(point)
            .then_some(point * self.scale)
    }
}

pub fn animations(
    mut commands: Commands,
    battle: Res<Match>,
    rules: Res<VeterancyFeedback>,
    data: Res<UiData>,
    time: Res<Time>,
    projection: Projection,
    mut promotions: Query<(Entity, &mut Promotion, &mut Node, &mut ImageNode)>,
) {
    let battle = battle.into_inner();
    let rules = rules.into_inner();
    let data = data.into_inner();
    let time = time.into_inner();
    let projection = projection.into_view();
    let tick = battle.simulation.tick;
    let scale = projection.scale;
    for (entity, mut promotion, mut node, mut image) in &mut promotions {
        if !battle.started || promotion.generation != battle.generation || tick >= promotion.expires
        {
            commands.entity(entity).despawn();
            continue;
        }
        promotion.advance(
            rules,
            tick,
            if battle.paused {
                0.0
            } else {
                time.delta_secs()
            },
        );
        *image = data.image(&rules.images[promotion.frame]);
        image.color = Color::WHITE.with_alpha(promotion_alpha(tick, promotion.expires));
        node.display = Display::None;
        if !matches!(
            battle
                .simulation
                .visibility_cell(battle.observer_player.unwrap_or(0), promotion.map_position),
            Status::Clear
        ) {
            continue;
        }
        let Some(point) = projection.screen(promotion.position) else {
            continue;
        };
        let size = data.image_size(&rules.images[promotion.frame]);
        *node = image_node(animation_area(point, size, projection.zoom), scale);
    }
}

fn promotion_alpha(tick: u64, expires: u64) -> f32 {
    let alpha =
        f32::from(u8::try_from(expires.saturating_sub(tick).min(30)).expect("bounded fade")) / 30.0;
    // Anim2D::draw passes 255 * alpha to GameMakeColor's integer channel.
    (255.0 * alpha).trunc() / 255.0
}

fn animation_area(point: Vec2, size: Vec2, zoom: f32) -> Rect {
    let size = (size * (1.3 / zoom)).floor();
    let origin = point.trunc() - (size * 0.5).floor();
    Rect::from_corners(origin, origin + size)
}

#[derive(Component)]
pub struct Insignia {
    object: usize,
    generation: u32,
}

impl Insignia {
    fn retain(
        &self,
        commands: &mut Commands,
        entity: Entity,
        generation: u32,
        alive: bool,
    ) -> bool {
        if self.generation == generation && alive {
            return true;
        }
        commands.entity(entity).despawn();
        false
    }
}

pub fn insignia(
    mut commands: Commands,
    battle: Res<Match>,
    data: Res<UiData>,
    projection: Projection,
    mut icons: Query<(Entity, &Insignia, &mut Node, &mut ImageNode)>,
    mut existing: Local<Vec<bool>>,
) {
    let battle = battle.into_inner();
    let data = data.into_inner();
    let projection = projection.into_view();
    existing.clear();
    existing.resize(battle.simulation.entities.len(), false);
    let display = |id: usize| -> Option<(Rect, &str)> {
        let unit = battle.simulation.entities.get(id)?;
        let stats = &battle.simulation.catalog[unit.type_index];
        if !battle.started
            || !unit.alive()
            || unit.masked()
            || stats.progression.ignored_in_gui
            || !matches!(
                battle
                    .simulation
                    .visibility_status(battle.observer_player.unwrap_or(0), id),
                Status::Clear | Status::Partial
            )
        {
            return None;
        }
        let name = *INSIGNIA.get(usize::from(unit.veterancy).checked_sub(1)?)?;
        let position = playtest_view::unit_position(&battle.simulation, id)
            + Vec3::Y * (stats.geometry_bounds.max_height + 10.0) * 0.1;
        let point = projection.screen(position)?;
        let width = 2.0 * (stats.geometry.major + stats.geometry.minor).clamp(20.0, 150.0);
        Some((
            insignia_area(point, data.image_size(name), width, projection.zoom),
            name,
        ))
    };
    let scale = projection.scale;
    for (entity, icon, mut node, mut image) in &mut icons {
        let alive = battle
            .simulation
            .entities
            .get(icon.object)
            .is_some_and(generals_sim::skirmish::Entity::alive);
        if !icon.retain(&mut commands, entity, battle.generation, alive) {
            continue;
        }
        existing[icon.object] = true;
        node.display = Display::None;
        if let Some((area, name)) = display(icon.object) {
            *node = image_node(area, scale);
            *image = data.image(name);
        }
    }
    for (id, &has_icon) in existing.iter().enumerate() {
        if !has_icon && let Some((area, name)) = display(id) {
            commands.spawn((
                Insignia {
                    object: id,
                    generation: battle.generation,
                },
                image_node(area, scale),
                data.image(name),
                FocusPolicy::Pass,
                ZIndex(1),
            ));
        }
    }
}

fn insignia_area(point: Vec2, size: Vec2, width: f32, zoom: f32) -> Rect {
    // SCALE_ICONS_WITH_ZOOM_ML is not defined in the pinned source profile.
    // Zoom moves the badge outward but its original pixel dimensions stay fixed.
    let offset = width * (1.3 / zoom) * 0.5;
    let origin = Vec2::new((point.x.trunc() + offset).trunc(), point.y.trunc()) + Vec2::ONE;
    Rect::from_corners(origin, origin + size)
}

fn image_node(area: Rect, scale: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(area.min.x / scale),
        top: px(area.min.y / scale),
        width: px(area.width() / scale),
        height: px(area.height() / scale),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_unit_and_reused_object_id_after_restart_retire_actual_badge_entities() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let dying = world
            .spawn(Insignia {
                object: 0,
                generation: 1,
            })
            .id();
        let survivor = world
            .spawn(Insignia {
                object: 1,
                generation: 1,
            })
            .id();
        let old_match = world
            .spawn(Insignia {
                object: 1,
                generation: 0,
            })
            .id();
        world
            .run_system_once(
                |mut commands: Commands, icons: Query<(Entity, &Insignia)>| {
                    for (entity, icon) in &icons {
                        icon.retain(&mut commands, entity, 1, icon.object != 0);
                    }
                },
            )
            .unwrap();
        assert!(world.get_entity(dying).is_err());
        assert!(world.get_entity(old_match).is_err());
        assert!(world.get_entity(survivor).is_ok());
        world
            .run_system_once(
                |mut commands: Commands, icons: Query<(Entity, &Insignia)>| {
                    for (entity, icon) in &icons {
                        icon.retain(&mut commands, entity, 2, true);
                    }
                },
            )
            .unwrap();
        assert!(world.get_entity(survivor).is_err());
    }

    #[test]
    fn world_animation_advances_one_frame_when_due_and_rises_by_elapsed_logic_time() {
        let rules = VeterancyFeedback {
            images: vec!["first".into(), "second".into(), "third".into()],
            delay: 1,
            duration: 120,
            rise: 15.0,
        };
        let mut promotion = Promotion {
            generation: 0,
            expires: 200,
            position: Vec3::ZERO,
            map_position: [0.0; 2],
            frame: 0,
            last_update: 0,
        };
        promotion.advance(&rules, 100, 1.0 / 60.0);
        assert_eq!(promotion.frame, 1);
        assert_eq!(promotion.position.y.to_bits(), 0.025_f32.to_bits());
        promotion.advance(&rules, 100, 0.0);
        assert_eq!(promotion.frame, 1);
        assert_eq!(promotion.position.y.to_bits(), 0.025_f32.to_bits());
        promotion.advance(&rules, 105, 1.0 / 60.0);
        assert_eq!(promotion.frame, 2);
        assert_eq!(promotion.position.y.to_bits(), 0.05_f32.to_bits());
        promotion.advance(&rules, 106, 0.0);
        assert_eq!(promotion.frame, 0);
    }

    #[test]
    fn veterancy_badge_keeps_native_size_and_uses_source_zoom_offset() {
        let point = Vec2::new(100.9, 200.9);
        let size = Vec2::new(12.0, 9.0);
        let normal = insignia_area(point, size, 40.0, 1.0);
        let close = insignia_area(point, size, 40.0, 0.5);
        assert_eq!(normal, Rect::new(127.0, 201.0, 139.0, 210.0));
        assert_eq!(close, Rect::new(153.0, 201.0, 165.0, 210.0));
        assert_eq!(normal.size(), close.size());
    }

    #[test]
    fn level_gain_fades_only_in_last_second_and_centers_scaled_integer_dimensions() {
        assert_eq!(promotion_alpha(89, 120).to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            promotion_alpha(105, 120).to_bits(),
            (127.0_f32 / 255.0).to_bits()
        );
        assert_eq!(promotion_alpha(120, 120).to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            animation_area(Vec2::new(100.9, 200.9), Vec2::new(21.0, 20.0), 1.0),
            Rect::new(87.0, 187.0, 114.0, 213.0)
        );
    }

    #[test]
    fn incomplete_image_sequences_and_randomized_promotions_are_rejected() {
        let parse_fixture = |images: &str| {
            let file = content::scan(generals_assets::virtual_assets::Asset {
                path: "promotion.ini".into(), origin: "fixture".into(),
                bytes: format!("GameData\nLevelGainAnimationName=LevelGain\nLevelGainAnimationTime=4\nLevelGainAnimationZRise=15\nEnd\nAnimation LevelGain\nAnimationMode=LOOP\nAnimationDelay=30\nNumberImages=2\n{images}\nEnd").into_bytes(),
            }).unwrap();
            parse(&[file])
        };
        assert!(parse_fixture("Image=A").is_err());
        assert!(parse_fixture("Image=A\nImage=B\nRandomizeStartFrame=Yes").is_err());
        let rules = parse_fixture("Image=A\nImage=B").unwrap();
        assert_eq!(rules.duration, 120);
        assert_eq!(rules.delay, 1);
        assert_eq!(rules.images, ["A", "B"]);
    }
}
