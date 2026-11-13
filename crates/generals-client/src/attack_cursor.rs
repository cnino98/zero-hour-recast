//! Original attack, boarding and Laser Lock cursors (`SuperHackers` 2bfaea60).
//! `createAttackHint` is empty: the target is the cursor, never a timed world decal.
use crate::{
    input::{CommandMode, Mouse},
    playtest::Match,
};
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    ui::FocusPolicy,
    window::{CursorOptions, PrimaryWindow},
};
use generals_assets::{ani, content, virtual_assets::VirtualAssets};
use std::{error::Error, time::Duration};

pub mod verification;

#[derive(Resource)]
pub struct AttackCursor {
    cursors: [Cursor; 21],
    current: Option<usize>,
    started: Option<Duration>,
}

struct Cursor {
    animation: ani::Animation,
    frames: Vec<CursorFrame>,
}

struct CursorFrame {
    image: Handle<Image>,
    hotspot: Vec2,
    size: Vec2,
}

#[derive(Component)]
pub struct AttackPointer;

#[derive(bevy::ecs::system::SystemParam)]
pub struct CursorUi<'w> {
    scale: Res<'w, UiScale>,
    data: Res<'w, crate::control_bar::UiData>,
    promotions: Res<'w, crate::playtest_ui::PromotionTransition>,
    results: Res<'w, crate::playtest_ui::ResultsScreen>,
    radar: Res<'w, crate::radar::Radar>,
}

pub fn load(assets: &VirtualAssets) -> Result<AttackCursor, Box<dyn Error>> {
    let mut files = vec![content::scan(assets.read("data/ini/mouse.ini")?)?];
    for path in assets.list("data/ini/mouse")? {
        if path.to_ascii_lowercase().ends_with(".ini") {
            files.push(content::scan(assets.read(&path)?)?);
        }
    }
    let texture = |name: &str| {
        files
            .iter()
            .flat_map(|file| &file.declarations)
            .filter(|item| item.kind == "MouseCursor" && item.name.eq_ignore_ascii_case(name))
            .flat_map(|item| &item.lines)
            .rfind(|line| {
                line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == "Texture")
            })
            .and_then(|line| line.tokens.get(1))
            .ok_or_else(|| format!("Mouse.ini lacks {name} Texture"))
    };
    let attack = texture("AttackObj")?;
    // Both source hint types intentionally use SCCAttack in the installed data.
    // Different artwork would require the full getCanAttackObject range result;
    // do not silently replace such an override with a fabricated range test.
    if !attack.eq_ignore_ascii_case(texture("OutRange")?) {
        return Err("distinct attack/out-of-range cursor artwork is not yet supported".into());
    }
    let load = |name: &str| -> Result<Cursor, Box<dyn Error>> {
        let texture = texture(name)?;
        let bytes = assets.read(&format!("data/cursors/{texture}.ani"))?.bytes;
        Ok(Cursor {
            animation: ani::Animation::parse(&bytes)?,
            frames: Vec::new(),
        })
    };
    Ok(AttackCursor {
        cursors: [
            load("AttackObj")?,
            load("EnterFriendly")?,
            load("LaserGuidedMissiles")?,
            load("GenericInvalid")?,
            load("AttackMove")?,
            load("Target")?,
            load("Hack")?,
            load("CaptureBuilding")?,
            load("SetRallyPoint")?,
            load("Move")?,
            load("EnterAggressive")?,
            load("SnipeVehicle")?,
            load("ForceAttackGround")?,
            load("ForceAttackObj")?,
            load("ParticleUplinkCannon")?,
            load("StabAttack")?,
            load("StabAttackInvalid")?,
            load("PlaceTimedCharge")?,
            load("PlaceRemoteCharge")?,
            load("PlaceChargeInvalid")?,
            load("DisguiseAsVehicle")?,
        ],
        current: None,
        started: None,
    })
}

pub fn verify(assets: &VirtualAssets) -> Result<(), Box<dyn Error>> {
    let cursor = load(assets)?;
    let frame = &cursor.cursors[0].animation.frames[0];
    println!(
        "Original attack cursor decoded: {} frames, {}x{} pixels, hotspot {:?}",
        cursor.cursors[0].animation.frames.len(),
        frame.width,
        frame.height,
        frame.hotspot
    );
    Ok(())
}

pub fn setup(
    mut commands: Commands,
    mut cursor: ResMut<AttackCursor>,
    mut images: ResMut<Assets<Image>>,
) {
    for art in &mut cursor.cursors {
        for frame in std::mem::take(&mut art.animation.frames) {
            let mut image = Image::new(
                Extent3d {
                    width: u32::from(frame.width),
                    height: u32::from(frame.height),
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                frame.rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.sampler = ImageSampler::nearest();
            art.frames.push(CursorFrame {
                size: Vec2::new(f32::from(frame.width), f32::from(frame.height)),
                hotspot: Vec2::new(f32::from(frame.hotspot[0]), f32::from(frame.hotspot[1])),
                image: images.add(image),
            });
        }
    }
    commands.spawn((
        AttackPointer,
        ImageNode::new(cursor.cursors[0].frames[0].image.clone()),
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            ..default()
        },
        GlobalZIndex(i32::MAX),
        FocusPolicy::Pass,
    ));
}

pub fn update(
    mut cursor: ResMut<AttackCursor>,
    battle: ResMut<Match>,
    mouse: Res<Mouse>,
    time: Res<Time<Real>>,
    ui: CursorUi,
    window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    pointer: Single<(&mut Node, &mut ImageNode), With<AttackPointer>>,
) {
    let battle = battle.into_inner();
    let mouse = mouse.into_inner();
    let time = time.into_inner();
    let CursorUi {
        scale,
        data,
        promotions,
        results,
        radar,
    } = ui;
    let scale = scale.into_inner();
    let data = data.into_inner();
    let promotions = promotions.into_inner();
    let results = results.into_inner();
    let (window, mut options) = window.into_inner();
    let (mut node, mut image) = pointer.into_inner();
    let point = window.cursor_position().filter(|&point| {
        window.focused
            && (mouse.guard_hint().is_some()
                || (battle.simulation.has_radar(0)
                    && radar.mouse_world(data, window, point).is_some())
                || !crate::playtest_ui::captures_pointer(battle, window, point, data, promotions))
    });
    // InGameUI leaves its cursor unchanged while area selection is active.
    let desired = if mouse.dragging {
        cursor.current
    } else {
        let world = point
            .and_then(|point| {
                battle
                    .simulation
                    .has_radar(0)
                    .then(|| radar.mouse_world(data, window, point))
                    .flatten()
            })
            .map(|point| point.extend(battle.simulation.ground_height(point.to_array())))
            .or(mouse.world_position);
        context_at(battle, mouse, world)
    };
    if cursor.current != desired {
        cursor.started = None;
        cursor.current = desired;
    }
    let active = point.is_some() && desired.is_some();
    let elapsed = animation_elapsed(&mut cursor.started, active, time.elapsed());
    node.display = if active { Display::Flex } else { Display::None };
    options.visible = !active && (battle.simulation.end_game_at.is_none() || results.scores());
    if let (Some(point), Some(elapsed)) = (point, elapsed) {
        let art = &cursor.cursors[desired.expect("active cursor")];
        let frame = &art.frames[art.animation.frame_at(elapsed)];
        image.image = frame.image.clone();
        // Native cursor resources use physical screen pixels. Compensate for
        // Bevy's window/UI scale rather than scaling the 32px cursor with the HUD.
        let physical_scale = window.scale_factor() * scale.0;
        let origin = point / scale.0 - frame.hotspot / physical_scale;
        node.left = px(origin.x);
        node.top = px(origin.y);
        node.width = px(frame.size.x / physical_scale);
        node.height = px(frame.size.y / physical_scale);
    }
}

// Source CommandXlat checks entry before ordinary attack; targeted special powers
// use their command's CursorName/InvalidCursorName even over empty ground.
pub fn context(battle: &mut Match, mouse: &Mouse) -> Option<usize> {
    context_at(battle, mouse, mouse.world_position)
}

fn context_at(battle: &mut Match, mouse: &Mouse, position: Option<Vec3>) -> Option<usize> {
    if !battle.started
        || battle.paused
        || battle.simulation.end_game_at.is_some()
        || battle.placing.is_some()
    {
        return None;
    }
    if !mouse.waypoint_mode {
        if let CommandMode::Power { caster, name, .. } = &mouse.pending_command
            && battle
                .simulation
                .entities
                .get(*caster)
                .is_some_and(|object| {
                    battle.simulation.catalog[object.type_index]
                        .special_powers
                        .iter()
                        .any(|rules| rules.name == *name)
                })
        {
            return Some(
                if position.is_some_and(|position| {
                    battle
                        .simulation
                        .can_use_power_at_location(*caster, name, position.to_array())
                }) {
                    5
                } else {
                    3
                },
            );
        }
        if matches!(mouse.pending_command, CommandMode::Normal)
            && position.is_some_and(|position| {
                battle.selected.iter().any(|&id| {
                    crate::input::locally_controlled(battle, id)
                        && battle
                            .simulation
                            .can_override_special_power_destination(id, position.to_array())
                })
            })
        {
            return Some(14);
        }
    }
    if mouse.force_attack_mode() {
        return match crate::input::attack::force_result(battle, mouse.hovered, position) {
            generals_sim::skirmish::CanAttackResult::Possible
            | generals_sim::skirmish::CanAttackResult::PossibleAfterMoving => {
                Some(if mouse.hovered.is_some() { 13 } else { 12 })
            }
            generals_sim::skirmish::CanAttackResult::InvalidShot => Some(3),
            generals_sim::skirmish::CanAttackResult::NotPossible => None,
        };
    }
    if mouse.waypoint_mode {
        return None;
    }
    if mouse.guard_hint().is_some() {
        return Some(12);
    }
    if !matches!(mouse.pending_command, CommandMode::Normal) {
        return targeted(battle, mouse);
    }
    let target = mouse.hovered?;
    if crate::input::hijacker::context_target(battle, mouse, Some(target)).is_some() {
        return Some(10);
    }
    if crate::input::car_bomb::context_target(battle, mouse, Some(target)).is_some() {
        return Some(10);
    }
    if crate::input::sabotage::context_target(battle, mouse, Some(target)).is_some() {
        // InGameUI's MSG_SABOTAGE_HINT selects ENTER_AGGRESSIVELY.
        return Some(10);
    }
    if crate::input::salvage::can_target(battle, target) {
        return Some(9);
    }
    if crate::input::hacker::finite_context_target(battle, mouse, Some(target)).is_some()
        || crate::input::hacker::context_target(battle, mouse, Some(target)).is_some()
    {
        return Some(6);
    }
    if battle.selected.iter().any(|&id| {
        crate::input::locally_controlled(battle, id) && battle.simulation.can_enter(id, target)
    }) {
        return Some(1);
    }
    battle.simulation.entities.get(target)?;
    crate::input::attack::ordinary_selection_target(battle, target)
        .then_some(0)
        .or_else(|| {
            crate::input::capture::context_target(battle, mouse, Some(target))
                .map(|_| crate::input::capture::cursor(battle))
        })
}

fn targeted(battle: &mut Match, mouse: &Mouse) -> Option<usize> {
    if let Some(cursor) = targeted_object(battle, mouse) {
        return Some(cursor);
    }
    match mouse.pending_command {
        CommandMode::HelixBomb => Some(
            if battle
                .selected
                .iter()
                .any(|&id| battle.simulation.helix_bomb_ready(id))
            {
                5
            } else {
                3
            },
        ),
        CommandMode::AttackMove => Some(4),
        CommandMode::CombatDrop => Some(
            if mouse.hovered.is_none_or(|target| {
                battle.selected.iter().any(|&id| {
                    crate::input::locally_controlled(battle, id)
                        && battle.simulation.can_combat_drop_into(id, target)
                })
            }) {
                10
            } else {
                3
            },
        ),
        // Mouse::CROSS maps to the "Target" INI name, shared by both commands.
        CommandMode::Guard
        | CommandMode::GuardFlying
        | CommandMode::FireWeapon(_)
        | CommandMode::ClearMines(_) => Some(5),
        CommandMode::Rally => Some(8),
        CommandMode::Cleanup => Some(
            if battle.selected.iter().any(|&id| {
                crate::input::locally_controlled(battle, id) && battle.simulation.cleanup_ready(id)
            }) {
                5
            } else {
                3
            },
        ),
        CommandMode::Capture => Some(
            if mouse
                .hovered
                .is_some_and(|target| crate::input::capture::can_target(battle, target))
            {
                crate::input::capture::cursor(battle)
            } else {
                3
            },
        ),
        CommandMode::Hack(ability) => {
            let mut allowed = false;
            if let Some(target) = mouse
                .hovered
                .filter(|&target| battle.simulation.visible_to(0, target))
            {
                for index in 0..battle.selected.len() {
                    let id = battle.selected[index];
                    if crate::input::locally_controlled(battle, id)
                        && battle.simulation.can_hack(id, target, ability)
                    {
                        allowed = true;
                        break;
                    }
                }
            }
            Some(if allowed { 6 } else { 3 })
        }
        CommandMode::DisableBuilding => {
            let Some(target) = mouse
                .hovered
                .filter(|&target| battle.simulation.visible_to(0, target))
            else {
                return Some(3);
            };
            for index in 0..battle.selected.len() {
                let id = battle.selected[index];
                if crate::input::locally_controlled(battle, id)
                    && battle.simulation.can_disable_building(id, target)
                {
                    return Some(6);
                }
            }
            Some(3)
        }
        _ => None,
    }
}

fn targeted_object(battle: &mut Match, mouse: &Mouse) -> Option<usize> {
    match mouse.pending_command {
        CommandMode::LaserLock => Some(
            if mouse.hovered.is_some_and(|target| {
                battle.simulation.visible_to(0, target)
                    && battle.selected.iter().any(|&id| {
                        crate::input::locally_controlled(battle, id)
                            && battle.simulation.can_laser_lock(id, target)
                    })
            }) {
                2
            } else {
                3
            },
        ),
        CommandMode::Hijack => Some(
            if mouse
                .hovered
                .is_some_and(|target| crate::input::hijacker::can_target(battle, target))
            {
                10
            } else {
                3
            },
        ),
        CommandMode::CarBomb => Some(
            if mouse
                .hovered
                .is_some_and(|target| crate::input::car_bomb::can_target(battle, target, false))
            {
                10
            } else {
                3
            },
        ),
        CommandMode::Sabotage => Some(
            if mouse
                .hovered
                .is_some_and(|target| crate::input::sabotage::can_target(battle, target))
            {
                10
            } else {
                3
            },
        ),
        CommandMode::Burton(command) => {
            use crate::input::burton::TargetCommand;
            use generals_sim::skirmish::special_ability::BurtonCharge;
            let valid = mouse
                .hovered
                .is_some_and(|target| crate::input::burton::can_target(battle, command, target));
            Some(match (command, valid) {
                (TargetCommand::Knife, true) => 15,
                (TargetCommand::Knife, false) => 16,
                (TargetCommand::Charge(BurtonCharge::Timed), true) => 17,
                (TargetCommand::Charge(BurtonCharge::Remote), true) => 18,
                (TargetCommand::Charge(_), false) => 19,
            })
        }
        CommandMode::BombTruckDisguise => Some(
            if mouse
                .hovered
                .is_some_and(|target| crate::input::bomb_truck::can_target(battle, target))
            {
                20
            } else {
                3
            },
        ),
        CommandMode::SnipeVehicle(slot) => Some(
            if mouse
                .hovered
                .is_some_and(|target| crate::input::pilot::can_target(battle, target, slot))
            {
                11
            } else {
                3
            },
        ),
        _ => None,
    }
}

fn animation_elapsed(
    started: &mut Option<Duration>,
    active: bool,
    now: Duration,
) -> Option<Duration> {
    if !active {
        *started = None;
        return None;
    }
    Some(now.saturating_sub(*started.get_or_insert(now)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_animation_continues_across_commands_and_resets_only_on_reentry() {
        let mut started = None;
        assert_eq!(
            animation_elapsed(&mut started, true, Duration::from_secs(5)),
            Some(Duration::ZERO)
        );
        assert_eq!(
            animation_elapsed(&mut started, true, Duration::from_millis(5200)),
            Some(Duration::from_millis(200))
        );
        assert_eq!(
            animation_elapsed(&mut started, false, Duration::from_millis(5250)),
            None
        );
        assert_eq!(
            animation_elapsed(&mut started, true, Duration::from_secs(6)),
            Some(Duration::ZERO)
        );
    }
}
