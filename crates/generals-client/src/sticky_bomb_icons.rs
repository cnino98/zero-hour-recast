//! `Drawable::drawBombed` and the authored `Anim2D` charge icons, SH 2bfaea60.
use crate::{
    control_bar::UiData,
    particles::{drawable_in_region, drawable_region},
    playtest::Match,
    playtest_view::WorldCamera,
    selection_feedback::{ProjectionView, health_region},
    skirmish_data::{definition, duration_frames, field, scalar, text, unsigned},
    terrain::Terrain,
};
use bevy::{prelude::*, ui::FocusPolicy};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::visibility::Status;
use num_traits::ToPrimitive;
use std::{collections::BTreeSet, error::Error};

#[derive(Resource)]
pub struct Art {
    remote: Animation,
    timed: Animation,
}

struct Animation {
    frames: Vec<String>,
    delay: u32,
}

impl Art {
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.remote
            .frames
            .iter()
            .chain(&self.timed.frames)
            .map(String::as_str)
    }
}

pub fn load(assets: &VirtualAssets) -> Result<Art, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/animation2d.ini")?)?;
    let parse = |name, mode| -> Result<Animation, Box<dyn Error>> {
        let lines = &definition(std::slice::from_ref(&file), "Animation", name)?.lines;
        let frames = lines
            .iter()
            .filter(|line| {
                line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "Image")
            })
            .map(|line| scalar(line).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        if text(lines, 0, "AnimationMode")? != mode
            || text(lines, 0, "RandomizeStartFrame")? != "No"
            || field(lines, 0, "ImageSequence").is_some()
            || frames.is_empty()
            || frames.len() != usize::try_from(unsigned(lines, 0, "NumberImages")?)?
        {
            return Err(format!("unsupported original {name} animation").into());
        }
        let delay = duration_frames(
            field(lines, 0, "AnimationDelay").ok_or("missing charge icon animation delay")?,
        )?;
        Ok(Animation { frames, delay })
    };
    Ok(Art {
        remote: parse("BombRemote", "LOOP")?,
        timed: parse("BombTimed", "ONCE")?,
    })
}

#[derive(Component)]
pub struct Icon {
    pub(crate) bomb: usize,
    pub(crate) generation: u32,
    timed: bool,
    frame: usize,
    last_update: u32,
    keep_till: u32,
}

impl Icon {
    fn new(
        bomb: usize,
        generation: u32,
        deadline: u32,
        timed: bool,
        now: u32,
        animation: &Animation,
    ) -> Self {
        let (frame, last_update) = if timed {
            (timed_start(deadline, now, animation.frames.len()), now)
        } else {
            // Anim2D constructor zeroes its last-update frame after reset.
            (0, 0)
        };
        Self {
            bomb,
            generation,
            timed,
            frame,
            last_update,
            keep_till: now.wrapping_add(1),
        }
    }

    const fn advance(&mut self, animation: &Animation, now: u32) {
        if now.wrapping_sub(self.last_update) < animation.delay {
            return;
        }
        let end = animation.frames.len() - 1;
        if self.frame < end {
            self.frame += 1;
            self.last_update = now;
        } else if !self.timed {
            self.frame = 0;
            self.last_update = now;
        }
    }

    const fn expired(&self, callback_ran: bool, now: u32) -> bool {
        callback_ran && self.keep_till <= now
    }
}

fn timed_start(deadline: u32, now: u32, frames: usize) -> usize {
    let remaining = deadline
        .wrapping_sub(now)
        .to_f32()
        .expect("native frame count");
    let seconds = (remaining * (1.0 / 30.0))
        .ceil()
        .to_usize()
        .unwrap_or(usize::MAX);
    (frames - 1).saturating_sub(seconds)
}

type RenderedIcons<'a> = (Entity, &'a mut Icon, &'a mut Node, &'a mut ImageNode);

#[derive(bevy::ecs::system::SystemParam)]
pub struct View<'w, 's> {
    projection: ProjectionView<'w, 's>,
    camera: Single<
        'w,
        's,
        (&'static Transform, &'static Projection, &'static Camera),
        With<WorldCamera>,
    >,
    terrain: Res<'w, Terrain>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    art: Res<Art>,
    data: Res<UiData>,
    view: View,
    mut icons: Query<RenderedIcons>,
) {
    let battle = battle.into_inner();
    let art = art.into_inner();
    let data = data.into_inner();
    let View {
        projection,
        camera,
        terrain,
    } = view;
    let (transform, perspective, camera) = camera.into_inner();
    let drawable_region = drawable_region(
        transform,
        perspective,
        camera,
        terrain.into_inner().draw_region(),
    );
    let view = projection.into_view();
    let game = &battle.simulation;
    let now = u32::try_from(game.tick & u64::from(u32::MAX)).expect("native frame");
    let viewer = battle.observer_player.unwrap_or(0);
    let world_icons = battle.started && game.draw_icon_ui && game.map_script_fade().is_none();
    let callback_ran = |id: usize| {
        let bomb = game.entities.get(id)?;
        (world_icons
            && bomb.exists()
            && !bomb.is_effectively_dead()
            && bomb.has_drawable()
            && !bomb.drawable_effectively_hidden()
            && !game.catalog[bomb.type_index].progression.ignored_in_gui
            && drawable_in_region(drawable_region, bomb.position)
            && matches!(
                game.visibility_status(viewer, id),
                Status::Clear | Status::Partial
            ))
        .then_some(bomb)
    };
    let area = |id: usize, bomb: &generals_sim::skirmish::Entity| {
        let (target, deadline) = bomb.sticky_bomb_icon()?;
        if !game
            .entities
            .get(target)
            .is_some_and(generals_sim::skirmish::Entity::exists)
        {
            return None;
        }
        // The source invokes computeHealthRegion on the bomb's own Drawable.
        health_region(battle, id, &view).map(|area| (area, deadline))
    };
    let mut existing = BTreeSet::new();
    for (entity, mut icon, mut node, mut image) in &mut icons {
        if icon.generation != battle.generation
            || !game
                .entities
                .get(icon.bomb)
                .is_some_and(|bomb| bomb.exists() && bomb.has_drawable())
        {
            commands.entity(entity).despawn();
            continue;
        }
        let called = callback_ran(icon.bomb);
        let region = called.and_then(|bomb| area(icon.bomb, bomb));
        let animation = if icon.timed { &art.timed } else { &art.remote };
        icon.advance(animation, now);
        if let Some((region, deadline)) = region {
            existing.insert((icon.bomb, icon.timed));
            icon.keep_till = now.wrapping_add(1);
            let (next_image, dimensions) = icon_image(data, art, &icon, deadline, now);
            if image.image != next_image.image || image.rect != next_image.rect {
                *image = next_image;
            }
            *node = icon_node(region, dimensions, view.scale);
        } else if icon.expired(called.is_some(), now) {
            commands.entity(entity).despawn();
        } else {
            node.display = Display::None;
        }
    }
    for (id, region, deadline) in (0..game.entities.len()).filter_map(|id| {
        callback_ran(id)
            .and_then(|bomb| area(id, bomb))
            .map(|(region, deadline)| (id, region, deadline))
    }) {
        for timed in [false, true] {
            if timed && deadline == 0 || existing.contains(&(id, timed)) {
                continue;
            }
            let animation = if timed { &art.timed } else { &art.remote };
            let icon = Icon::new(id, battle.generation, deadline, timed, now, animation);
            let (image, dimensions) = icon_image(data, art, &icon, deadline, now);
            commands.spawn((
                icon,
                image,
                icon_node(region, dimensions, view.scale),
                FocusPolicy::Pass,
                ZIndex(1 + i32::from(timed)),
            ));
        }
    }
}

fn icon_image(data: &UiData, art: &Art, icon: &Icon, deadline: u32, now: u32) -> (ImageNode, Vec2) {
    let animation = if icon.timed { &art.timed } else { &art.remote };
    let name = &animation.frames[icon.frame];
    let dimensions = if deadline != 0 && !icon.timed {
        // The countdown's mapped images share the same dimensions in the installed atlas.
        data.image_size(&art.timed.frames[timed_start(deadline, now, art.timed.frames.len())])
    } else {
        data.image_size(name)
    };
    (data.image(name), dimensions)
}

fn icon_node(region: Rect, dimensions: Vec2, scale: f32) -> Node {
    let width = (region.width() * 0.65).trunc();
    let height = ((width / dimensions.x) * dimensions.y).trunc();
    let half_width = region.width() * 0.5;
    let half_height = region.height() * 0.5;
    let x = (region.min.x + half_width - width * 0.5).trunc();
    let y = (region.min.y + half_height).trunc() + 5.0;
    Node {
        position_type: PositionType::Absolute,
        left: px(x / scale),
        top: px(y / scale),
        width: px(width / scale),
        height: px(height / scale),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_icon_starts_at_ceiling_remaining_seconds_and_advances_once() {
        let animation = Animation {
            frames: vec![String::new(); 21],
            delay: 30,
        };
        let mut timed = Icon::new(5, 1, 451, true, 300, &animation);
        assert_eq!(timed.frame, 14);
        timed.advance(&animation, 329);
        assert_eq!(timed.frame, 14);
        timed.advance(&animation, 361);
        assert_eq!(timed.frame, 15);
        assert_eq!(timed_start(901, 300, 21), 0);
    }

    #[test]
    fn offscreen_icons_keep_their_collection_clock_and_expire_only_on_a_callback() {
        let art = Animation {
            frames: vec![String::new(); 21],
            delay: 30,
        };
        let mut icon = Icon::new(5, 1, 900, true, 300, &art);
        icon.advance(&art, 331);
        assert!(!icon.expired(false, 331));
        icon.advance(&art, 363);
        assert!(!icon.expired(false, 363));
        // Reentry continues from the prior sequence, with the old cadence.
        // Recreating from deadline would also reset the next animation tick.
        icon.advance(&art, 393);
        assert_eq!((icon.frame, icon.last_update), (3, 393));
        assert!(icon.expired(true, 393));
    }
}
