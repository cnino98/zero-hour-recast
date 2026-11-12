//! `CrateCollide`'s original `MoneyPickUp` world animation, separate from cash text.
use crate::{
    control_bar::UiData,
    playtest::{CameraRig, Match},
    playtest_view::WorldCamera,
    skirmish_data::{definition, duration_frames, field, scalar},
    source_audio::SourceAudio,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::visibility::Status;
use std::error::Error;

#[derive(Resource)]
pub struct Feedback {
    images: Vec<String>,
    delay: u32,
}
impl Feedback {
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.images.iter().map(String::as_str)
    }
}

pub fn load(assets: &VirtualAssets) -> Result<Feedback, Box<dyn Error>> {
    let files = [content::scan(assets.read("data/ini/animation2d.ini")?)?];
    let lines = &definition(&files, "Animation", "MoneyPickUp")?.lines;
    if field(lines, 0, "AnimationMode").map(scalar).transpose()? != Some("LOOP")
        || field(lines, 0, "RandomizeStartFrame")
            .map(scalar)
            .transpose()?
            != Some("No")
    {
        return Err("unsupported original money pickup animation mode".into());
    }
    let images = lines
        .iter()
        .filter(|line| {
            line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "Image")
        })
        .map(|line| scalar(line).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let count = field(lines, 0, "NumberImages")
        .map(scalar)
        .transpose()?
        .ok_or("money pickup frame count absent")?
        .parse::<usize>()?;
    if images.is_empty() || images.len() != count {
        return Err("money pickup image sequence incomplete".into());
    }
    let delay =
        duration_frames(field(lines, 0, "AnimationDelay").ok_or("money pickup delay absent")?)?;
    Ok(Feedback { images, delay })
}

#[derive(Component)]
pub struct CashAnimation {
    generation: u32,
    expires: u64,
    position: Vec3,
    map_position: [f32; 2],
    frame: usize,
    last_update: u64,
    rise: f32,
}

pub fn record(
    mut commands: Commands,
    mut battle: ResMut<Match>,
    data: Res<UiData>,
    feedback: Res<Feedback>,
    mut audio: ResMut<SourceAudio>,
) {
    let data = data.into_inner();
    let feedback = feedback.into_inner();
    if !battle.started || battle.paused {
        return;
    }
    for event in std::mem::take(&mut battle.simulation.presentation.money_crate_events) {
        audio.salvage(
            &generals_sim::skirmish::salvage::Reward::Money(event.amount),
            event.collector,
            battle.simulation.object_position(event.collector),
        );
        let rules = battle.simulation.catalog[battle.simulation.entities[event.item].type_index]
            .behavior
            .money_crate
            .as_ref()
            .expect("money pickup source module");
        if rules.animation.is_empty() {
            continue;
        }
        let duration = (rules.animation_seconds * 30.0)
            .trunc()
            .to_string()
            .parse::<u64>()
            .expect("validated money animation duration");
        commands.spawn((
            CashAnimation {
                generation: battle.generation,
                expires: event.frame + duration,
                position: Vec3::new(event.position[0], event.position[2], -event.position[1]) * 0.1,
                map_position: [event.position[0], event.position[1]],
                frame: 0,
                last_update: 0,
                rise: rules.animation_rise,
            },
            data.image(&feedback.images[0]),
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

pub fn animations(
    mut commands: Commands,
    battle: Res<Match>,
    feedback: Res<Feedback>,
    data: Res<UiData>,
    time: Res<Time>,
    projection: Projection,
    mut animations: Query<(Entity, &mut CashAnimation, &mut Node, &mut ImageNode)>,
) {
    let battle = battle.into_inner();
    let feedback = feedback.into_inner();
    let data = data.into_inner();
    let time = time.into_inner();
    let Projection {
        camera,
        window,
        rig,
        terrain,
    } = projection;
    let camera = camera.into_inner();
    let window = window.into_inner();
    let rig = rig.into_inner();
    let terrain = terrain.into_inner();
    let game = &battle.simulation;
    let scale = window.scale_factor();
    let ground = terrain.sample(rig.focus);
    let zoom = (ground + rig.distance) / (ground + rig.settings.max_height);
    for (entity, mut animation, mut node, mut image) in &mut animations {
        if !battle.started
            || animation.generation != battle.generation
            || game.tick >= animation.expires
        {
            commands.entity(entity).despawn();
            continue;
        }
        if game.tick - animation.last_update >= u64::from(feedback.delay) {
            animation.frame = (animation.frame + 1) % feedback.images.len();
            animation.last_update = game.tick;
        }
        if !battle.paused {
            let rise = animation.rise;
            let distance = rise / 30.0 * (time.delta_secs() * 30.0) * 0.1;
            animation.position.y += distance;
        }
        *image = data.image(&feedback.images[animation.frame]);
        let remaining = u8::try_from(animation.expires.saturating_sub(game.tick).min(30))
            .expect("bounded world animation fade");
        image.color =
            Color::WHITE.with_alpha((255.0 * f32::from(remaining) / 30.0).trunc() / 255.0);
        node.display = Display::None;
        if !matches!(
            game.visibility_cell(battle.observer_player.unwrap_or(0), animation.map_position),
            Status::Clear
        ) {
            continue;
        }
        let Ok(point) = camera.0.world_to_viewport(camera.1, animation.position) else {
            continue;
        };
        if !camera
            .0
            .logical_viewport_rect()
            .is_some_and(|viewport| viewport.contains(point))
        {
            continue;
        }
        let size = (data.image_size(&feedback.images[animation.frame]) * (1.3 / zoom)).floor();
        let origin = (point * scale).trunc() - (size * 0.5).floor();
        *node = Node {
            position_type: PositionType::Absolute,
            left: px(origin.x / scale),
            top: px(origin.y / scale),
            width: px(size.x / scale),
            height: px(size.y / scale),
            ..default()
        };
    }
}

#[cfg(test)]
mod tests {
    #[ignore = "requires original game installation"]
    #[test]
    fn installed_money_pickup_uses_all_thirty_one_original_sprite_frames() {
        let root = &generals_assets::game_root().unwrap();
        let assets = generals_assets::virtual_assets::VirtualAssets::for_zero_hour(
            root.join("GameData/Command and Conquer Generals"),
            root.join("GameData/Command and Conquer Generals Zero Hour"),
        )
        .unwrap();
        let feedback = super::load(&assets).unwrap();
        assert_eq!(feedback.images.len(), 31);
        assert_eq!(feedback.images.first().unwrap(), "SCPDollar000");
        assert_eq!(feedback.images.last().unwrap(), "SCPDollar030");
        assert_eq!(feedback.delay, 1);
    }
}
