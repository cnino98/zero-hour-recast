//! Source selection flash and health display (`SuperHackers` 2bfaea60).
//! See Drawable.cpp:computeHealthRegion/drawHealthBar/TintEnvelope and
//! Object.cpp:getHealthBoxPosition/getHealthBoxDimensions.
use crate::{
    input::Mouse,
    playtest::{CameraRig, Match},
    playtest_view::{self, UnitVisual, WorldCamera},
    source_material::SourceMaterial,
    terrain::Terrain,
};
use bevy::{ecs::system::SystemParam, prelude::*, window::PrimaryWindow};
use generals_assets::{content, virtual_assets::VirtualAssets};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

#[derive(Resource)]
pub struct Feedback {
    show_health: bool,
    damaged: f32,
    really_damaged: f32,
    flash_colors: Vec<Vec3>,
    capture_colors: Vec<Vec3>,
    seen: BTreeSet<(u64, usize, u8)>,
    previous: Vec<usize>,
    flashes: BTreeMap<usize, Flash>,
    disabled: BTreeSet<usize>,
    frenzy: BTreeSet<usize>,
    gaining_subdual: BTreeSet<usize>,
    status_tints: BTreeMap<usize, Flash>,
    generation: u32,
}

enum FlashPhase {
    Attack,
    Decay,
    Sustain,
}

struct Flash {
    peak: Vec3,
    current: Vec3,
    attack: Vec3,
    decay: Vec3,
    sustain: f64,
    phase: FlashPhase,
}

impl Flash {
    fn new(peak: Vec3) -> Self {
        Self {
            peak,
            current: Vec3::ZERO,
            attack: peak,
            decay: peak * -0.25,
            sustain: 0.0,
            phase: FlashPhase::Attack,
        }
    }

    fn play(&mut self, peak: Vec3) {
        self.play_timed(peak, 1.0, 4.0, 0.0);
    }

    fn play_timed(&mut self, peak: Vec3, attack: f32, decay: f32, sustain: f64) {
        self.peak = peak;
        self.attack = (peak - self.current) / attack.max(1.0);
        self.decay = peak * (-1.0 / decay.max(1.0));
        self.sustain = sustain;
        self.phase = if (self.current - peak).length() <= 0.001 {
            FlashPhase::Sustain
        } else {
            FlashPhase::Attack
        };
    }

    fn advance(&mut self, time_scale: f32) -> bool {
        // TintEnvelope::update uses logic-time / render-FPS, not render frames.
        match self.phase {
            FlashPhase::Decay => {
                let decrement = self.decay * time_scale;
                if decrement.length() > self.current.length() || self.current.length() <= 0.001 {
                    return false;
                }
                self.current += decrement;
            }
            FlashPhase::Attack => {
                let increment = self.attack * time_scale;
                let delta = self.current - self.peak;
                if increment.length() > delta.length() || delta.length() <= 0.001 {
                    self.phase = if self.sustain == 0.0 {
                        FlashPhase::Decay
                    } else {
                        FlashPhase::Sustain
                    };
                } else {
                    self.current += increment;
                }
            }
            FlashPhase::Sustain => {
                if self.sustain > 0.0 {
                    self.sustain -= f64::from(time_scale);
                } else {
                    self.phase = FlashPhase::Decay;
                }
            }
        }
        true
    }
}

pub fn load(assets: &VirtualAssets) -> Result<Feedback, Box<dyn Error>> {
    let global = content::scan(assets.read("data/ini/gamedata.ini")?)?;
    let lines = &global
        .declarations
        .iter()
        .find(|item| item.kind == "GameData")
        .ok_or("GameData settings missing")?
        .lines;
    let value = |name: &str| {
        lines.iter().rev().find_map(|line| {
            (line.depth == Some(0) && line.tokens.first().is_some_and(|key| key == name))
                .then(|| line.tokens.get(1).map(String::as_str))
                .flatten()
        })
    };
    let boolean = |name: &str, fallback: bool| -> Result<bool, Box<dyn Error>> {
        match value(name) {
            Some(value) if value.eq_ignore_ascii_case("yes") => Ok(true),
            Some(value) if value.eq_ignore_ascii_case("no") => Ok(false),
            None => Ok(fallback),
            _ => Err(format!("invalid GameData {name}").into()),
        }
    };
    let number = |name: &str, fallback: f32| -> Result<f32, Box<dyn Error>> {
        let parsed = value(name).map_or(Ok(fallback), str::parse::<f32>)?;
        if !parsed.is_finite() {
            return Err(format!("invalid GameData {name}").into());
        }
        Ok(parsed)
    };
    let saturation = number("SelectionFlashSaturationFactor", 0.5)?;
    let colors = if boolean("SelectionFlashHouseColor", false)? {
        playtest_view::player_colors(assets)?
            .into_iter()
            .map(Vec3::from_array)
            .collect()
    } else {
        vec![Vec3::ONE; 8]
    };
    Ok(Feedback {
        show_health: boolean("ShowObjectHealth", false)?,
        damaged: number("UnitDamagedThreshold", 0.5)?,
        really_damaged: number("UnitReallyDamagedThreshold", 0.1)?,
        // Drawable::saturateRGB deliberately permits negative components.
        flash_colors: colors
            .into_iter()
            .map(|color| color * saturation - Vec3::splat(saturation * 0.5))
            .collect(),
        capture_colors: playtest_view::player_colors(assets)?
            .into_iter()
            .map(|color| Vec3::from_array(color) * saturation - Vec3::splat(saturation * 0.5))
            .collect(),
        seen: BTreeSet::new(),
        previous: Vec::new(),
        flashes: BTreeMap::new(),
        disabled: BTreeSet::new(),
        frenzy: BTreeSet::new(),
        gaining_subdual: BTreeSet::new(),
        status_tints: BTreeMap::new(),
        generation: u32::MAX,
    })
}

impl Feedback {
    fn reset_generation(&mut self, generation: u32) {
        if self.generation != generation {
            self.previous.clear();
            self.flashes.clear();
            self.disabled.clear();
            self.frenzy.clear();
            self.gaining_subdual.clear();
            self.status_tints.clear();
            self.seen.clear();
            self.generation = generation;
        }
    }

    pub(crate) fn ability_capture_flash(&mut self, battle: &Match, target: usize, owner: usize) {
        self.reset_generation(battle.generation);
        let color = battle
            .simulation
            .player_colors
            .get(owner)
            .copied()
            .flatten()
            .map_or(Vec3::ONE, |color| self.capture_colors[color]);
        self.flashes
            .entry(target)
            .and_modify(|flash| flash.play(color))
            .or_insert_with(|| Flash::new(color));
    }

    pub(crate) fn flash_as_selected(&mut self, battle: &Match, target: usize) {
        self.reset_generation(battle.generation);
        let game = &battle.simulation;
        let color = game
            .player_colors
            .get(game.entities[target].owner)
            .copied()
            .flatten()
            .map_or(Vec3::ONE, |color| self.flash_colors[color]);
        self.flashes
            .entry(target)
            .and_modify(|flash| flash.play(color))
            .or_insert_with(|| Flash::new(color));
    }
}

pub fn update_flashes(mut feedback: ResMut<Feedback>, battle: Res<Match>, time: Res<Time>) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    feedback.reset_generation(battle.generation);
    for &index in &battle.selected {
        if !feedback.previous.contains(&index)
            && let Some(unit) = battle.simulation.entities.get(index)
        {
            let color = battle
                .simulation
                .player_colors
                .get(unit.owner)
                .copied()
                .flatten()
                .map_or(Vec3::ONE, |color| feedback.flash_colors[color]);
            feedback
                .flashes
                .entry(index)
                .and_modify(|flash| flash.play(color))
                .or_insert_with(|| Flash::new(color));
        }
    }
    feedback.previous.clone_from(&battle.selected);
    feedback
        .seen
        .retain(|&(frame, _, _)| frame + 1 >= battle.simulation.tick);
    for event in &battle.simulation.presentation.capture_events {
        use generals_sim::skirmish::CaptureEffect;
        let (kind, explicit) = match event.effect {
            CaptureEffect::Captured => (1, false),
            CaptureEffect::Detected => (2, true),
            CaptureEffect::DefectorTick => (3, false),
            _ => continue,
        };
        if !feedback.seen.insert((event.frame, event.object, kind)) {
            continue;
        }
        let color = if kind == 2 {
            Vec3::ONE
        } else {
            let colors = if explicit {
                &feedback.capture_colors
            } else {
                &feedback.flash_colors
            };
            battle
                .simulation
                .player_colors
                .get(event.owner)
                .copied()
                .flatten()
                .map_or(Vec3::ONE, |index| colors[index])
        };
        feedback
            .flashes
            .entry(event.target)
            .and_modify(|flash| flash.play(color))
            .or_insert_with(|| Flash::new(color));
    }
    update_disabled(&mut feedback, &battle.simulation);
    if !battle.paused {
        feedback
            .status_tints
            .retain(|_, tint| tint.advance(time.delta_secs() * 30.0));
        feedback
            .flashes
            .retain(|_, flash| flash.advance(time.delta_secs() * 30.0));
    }
}

// Object::setDisabledUntil/clearDisabled feed Drawable's edge-triggered tint.
// Held/script/unmanned do not tint; the represented remaining reasons are below.
fn update_disabled(feedback: &mut Feedback, game: &generals_sim::skirmish::Skirmish) {
    for (index, actor) in game.entities.iter().enumerate() {
        let disabled = actor.drawable_disabled_tint();
        let frenzy = actor.frenzy_tint();
        let frenzy_changed = if frenzy {
            feedback.frenzy.insert(index)
        } else {
            feedback.frenzy.remove(&index)
        };
        let subdual = actor.gaining_subdual_damage();
        let subdual_changed = if subdual {
            feedback.gaining_subdual.insert(index)
        } else {
            feedback.gaining_subdual.remove(&index)
        };
        let changed = frenzy_changed || subdual_changed;
        let was_disabled = feedback.disabled.contains(&index);
        // Drawable reruns the active tint when any tint-status bit changes.
        if changed && disabled {
            feedback.disabled.remove(&index);
        }
        feedback.set_disabled(index, disabled);
        if !disabled && (changed || was_disabled) {
            if subdual {
                feedback
                    .status_tints
                    .entry(index)
                    .or_insert_with(|| Flash::new(Vec3::ZERO))
                    .play_timed(Vec3::new(-0.2, -0.2, 0.8), 150.0, 150.0, 4_294_967_294.0);
            } else if frenzy {
                let infantry = game.catalog[actor.type_index]
                    .kind_of
                    .iter()
                    .any(|kind| kind == "INFANTRY");
                let color = if infantry {
                    Vec3::new(0.0, -0.7, -0.7)
                } else {
                    Vec3::new(0.2, -0.2, -0.2)
                };
                feedback
                    .status_tints
                    .entry(index)
                    .or_insert_with(|| Flash::new(Vec3::ZERO))
                    .play_timed(color, 30.0, 30.0, 4_294_967_294.0);
            } else if let Some(tint) = feedback.status_tints.get_mut(&index) {
                tint.phase = FlashPhase::Decay;
            }
        }
    }
}

impl Feedback {
    fn set_disabled(&mut self, index: usize, disabled: bool) {
        if disabled {
            if self.disabled.insert(index) {
                let tint = self
                    .status_tints
                    .entry(index)
                    .or_insert_with(|| Flash::new(Vec3::ZERO));
                // Drawable::DARK_GRAY_DISABLED_COLOR and SUSTAIN_INDEFINITELY.
                tint.play_timed(Vec3::splat(-0.5), 30.0, 30.0, 4_294_967_294.0);
            }
        } else if self.disabled.remove(&index)
            && let Some(tint) = self.status_tints.get_mut(&index)
        {
            tint.phase = FlashPhase::Decay;
        }
    }

    fn tint(&self, index: usize) -> Option<Vec3> {
        let selected = self.flashes.get(&index);
        let status = self.status_tints.get(&index);
        (selected.is_some() || status.is_some()).then(|| {
            // W3DScene::CustomizedRender adds the independent envelopes.
            selected.map_or(Vec3::ZERO, |tint| tint.current)
                + status.map_or(Vec3::ZERO, |tint| tint.current)
        })
    }
}

#[derive(Component)]
pub struct SelectionTint {
    original: Handle<SourceMaterial>,
    temporary: Handle<SourceMaterial>,
}

type TintPart = (
    Entity,
    &'static ChildOf,
    &'static mut MeshMaterial3d<SourceMaterial>,
    Option<&'static mut SelectionTint>,
    Option<&'static crate::light_environment::Bounds>,
    Option<&'static crate::model_render::RenderOrder>,
    Option<&'static mut bevy::camera::visibility::RenderLayers>,
);

#[derive(SystemParam)]
pub struct TintView<'w, 's> {
    feedback: Res<'w, Feedback>,
    lighting: crate::light_environment::DrawLights<'w>,
    cloaks: Res<'w, crate::stealth_view::Cloaks>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    parents: Query<
        'w,
        's,
        (
            &'static UnitVisual,
            Option<&'static crate::light_environment::DrawPose>,
        ),
    >,
    parts: Query<'w, 's, TintPart, Without<crate::stealth_view::HeatPart>>,
}

pub fn tint_models(mut commands: Commands, view: TintView) {
    let TintView {
        feedback,
        lighting,
        cloaks,
        mut materials,
        parents,
        mut parts,
    } = view;
    let feedback = feedback.into_inner();
    let cloaks = cloaks.into_inner();
    for (entity, parent, mut material, mut tint, bounds, order, layers) in &mut parts {
        let Ok((unit, pose)) = parents.get(parent.parent()) else {
            continue;
        };
        let flash = feedback
            .tint(unit.index)
            .filter(|_| unit.generation == feedback.generation);
        let opacity = cloaks.base_opacity(unit.index);
        if let (Some(order), Some(mut layers)) = (order, layers) {
            *layers =
                bevy::camera::visibility::RenderLayers::layer(order.layer(opacity.unwrap_or(1.0)));
        }
        let lit = lighting.affects(unit, pose, bounds);
        if flash.is_some() || opacity.is_some() || lit {
            // Ownership/model changes can replace the material during a live flash.
            // Preserve that new base rather than restoring the old owner's skin.
            if let Some(tint) = tint.as_mut()
                && material.0 != tint.temporary
            {
                tint.original = material.0.clone();
                material.0 = tint.temporary.clone();
            }
            let original = tint.as_ref().map_or(&material.0, |tint| &tint.original);
            let Some(mut colored) = materials.get(original).cloned() else {
                continue;
            };
            if let Some(flash) = flash {
                colored.parameters.scene_ambient += flash.extend(0.0);
                for light in &mut colored.parameters.light_color[..3] {
                    *light += flash.extend(0.0);
                }
            }
            if let Some(opacity) = opacity {
                colored.parameters.opacity = opacity;
                colored.alpha_mode = AlphaMode::Blend;
                if opacity == 0.0 {
                    colored.depth_write = false;
                }
            }
            lighting.apply(unit, pose, bounds, &mut colored.parameters);
            if let Some(tint) = tint {
                if materials
                    .get(&tint.temporary)
                    .is_some_and(|existing| *existing != colored)
                    && let Some(mut existing) = materials.get_mut(&tint.temporary)
                {
                    *existing = colored;
                }
            } else {
                let original = material.0.clone();
                let temporary = materials.add(colored);
                material.0 = temporary.clone();
                commands.entity(entity).insert(SelectionTint {
                    original,
                    temporary,
                });
            }
        } else if let Some(tint) = tint {
            if material.0 == tint.temporary {
                material.0 = tint.original.clone();
            }
            commands.entity(entity).remove::<SelectionTint>();
        }
    }
}

#[derive(Component)]
pub struct HealthBar {
    index: usize,
    generation: u32,
}

#[derive(SystemParam)]
pub struct ProjectionView<'w, 's> {
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    rig: Res<'w, CameraRig>,
    terrain: Res<'w, Terrain>,
}

struct HealthView<'a> {
    camera: (&'a Camera, &'a GlobalTransform),
    scale: f32,
    zoom: f32,
}

pub fn health_bars(
    mut commands: Commands,
    battle: Res<Match>,
    mouse: Res<Mouse>,
    feedback: Res<Feedback>,
    view: ProjectionView,
    mut bars: Query<(Entity, &HealthBar, &mut Node, &mut BorderColor, &Children)>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), Without<HealthBar>>,
) {
    let battle = battle.into_inner();
    let mouse = mouse.into_inner();
    let feedback = feedback.into_inner();
    let ProjectionView {
        camera,
        window,
        rig,
        terrain,
    } = view;
    let window = window.into_inner();
    let rig = rig.into_inner();
    let terrain = terrain.into_inner();
    let ground = terrain.sample(rig.focus);
    let view = HealthView {
        camera: camera.into_inner(),
        scale: window.scale_factor(),
        zoom: (ground + rig.distance) / (ground + rig.settings.max_height),
    };
    let visible = |index| {
        battle.started
            && feedback.show_health
            && matches!(
                battle.simulation.visibility_status(0, index),
                generals_sim::skirmish::visibility::Status::Clear
                    | generals_sim::skirmish::visibility::Status::Partial
            )
            && (battle.selected.contains(&index) || mouse.hovered == Some(index))
    };
    for (entity, bar, mut node, mut border, children) in &mut bars {
        if bar.generation != battle.generation {
            commands.entity(entity).despawn();
            continue;
        }
        node.display = Display::None;
        if !visible(bar.index) {
            continue;
        }
        let Some((area, ratio, fill, outline)) = health_display(battle, bar.index, feedback, &view)
        else {
            continue;
        };
        *node = health_node(area, view.scale);
        *border = BorderColor::all(outline);
        if let Some(&child) = children.first()
            && let Ok((mut node, mut color)) = fills.get_mut(child)
        {
            node.width = percent(ratio * 100.0);
            color.0 = fill;
        }
    }
    for index in battle.selected.iter().copied().chain(
        mouse
            .hovered
            .filter(|index| !battle.selected.contains(index)),
    ) {
        if !visible(index)
            || bars
                .iter()
                .any(|(_, bar, _, _, _)| bar.index == index && bar.generation == battle.generation)
        {
            continue;
        }
        let Some((area, ratio, fill, outline)) = health_display(battle, index, feedback, &view)
        else {
            continue;
        };
        commands
            .spawn((
                HealthBar {
                    index,
                    generation: battle.generation,
                },
                health_node(area, view.scale),
                BorderColor::all(outline),
                ZIndex(0),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        width: percent(ratio * 100.0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(fill),
                ));
            });
    }
}

fn health_node(area: Rect, scale: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(area.min.x / scale),
        top: px(area.min.y / scale),
        width: px(area.width() / scale),
        height: px(area.height() / scale),
        border: UiRect::all(px(1.0 / scale)),
        ..default()
    }
}

fn health_display(
    battle: &Match,
    index: usize,
    feedback: &Feedback,
    view: &HealthView,
) -> Option<(Rect, f32, Color, Color)> {
    let unit = battle.simulation.entities.get(index)?;
    if !unit.alive() || (unit.masked() && !unit.spawn_nexus()) {
        return None;
    }
    let stats = &battle.simulation.catalog[unit.type_index];
    if stats.progression.ignored_in_gui {
        return None;
    }
    let geometry = battle.simulation.body_geometry(index);
    let bounds = battle.simulation.body_geometry_bounds(index);
    let (camera, transform) = view.camera;
    let position = if unit.spawn_nexus() {
        let position = battle.simulation.object_position(index);
        let offset = unit.health_box_offset();
        let center = std::array::from_fn(|axis| position[axis] + offset[axis]);
        let position = mob_health_position(
            center,
            battle.simulation.object_position(index),
            bounds.max_height,
        );
        Vec3::new(position[0] * 0.1, position[2] * 0.1, -position[1] * 0.1)
    } else {
        playtest_view::unit_position(&battle.simulation, index)
            + Vec3::Y * (bounds.max_height + 10.0) * 0.1
    };
    let point = camera.world_to_viewport(transform, position).ok()?;
    if !camera.logical_viewport_rect()?.contains(point) {
        return None;
    }
    let point = point * view.scale;
    let zoom = view.zoom;
    let width = 2.0 * (geometry.major + geometry.minor).clamp(20.0, 150.0) / zoom;
    let x = (point.x.trunc() - width * 0.45).trunc();
    let y = (point.y.trunc() - 1.5).trunc();
    let area = Rect::new(x, y, (x + width).trunc(), y + 3.0);
    let ratio = unit.health / unit.max_health;
    let (fill, outline) = health_colors(
        ratio,
        !unit.complete(),
        feedback.damaged,
        feedback.really_damaged,
    );
    Some((area, ratio, fill, outline))
}

// Object::getHealthBoxPosition adds the spawner's averaged offset and original +20 Z.
fn mob_health_position(center: [f32; 3], parent: [f32; 3], max_height: f32) -> [f32; 3] {
    let offset = std::array::from_fn::<_, 3, _>(|axis| center[axis] - parent[axis]);
    let mut position = parent;
    position[2] += max_height + 10.0;
    for axis in 0..3 {
        position[axis] += offset[axis];
    }
    position[2] += 20.0;
    position
}

fn health_colors(
    ratio: f32,
    constructing: bool,
    damaged: f32,
    really_damaged: f32,
) -> (Color, Color) {
    if constructing {
        return (
            Color::srgb(0.0, (ratio * 255.0).trunc() / 255.0, 1.0),
            Color::srgb(0.0, (ratio * 128.0).trunc() / 255.0, 128.0 / 255.0),
        );
    }
    let mut red = if ratio >= 0.5 {
        1.0 - (ratio - 0.5) / 0.5
    } else {
        1.0
    };
    let mut green = if ratio >= 0.5 {
        1.0
    } else {
        1.0 - (0.5 - ratio) / 0.5
    };
    let outline = Color::srgb(
        (red * 127.5).trunc() / 255.0,
        (green * 127.5).trunc() / 255.0,
        0.0,
    );
    if ratio <= really_damaged {
        red = (1.0 + red) * 0.5;
        green *= 0.5;
    } else if ratio > damaged {
        green = (1.0 + green) * 0.5;
        red *= 0.5;
    }
    (
        Color::srgb(
            (red * 255.0).trunc() / 255.0,
            (green * 255.0).trunc() / 255.0,
            0.0,
        ),
        outline,
    )
}

#[cfg(test)]
mod tests {
    use super::{Flash, health_colors};
    use bevy::prelude::*;

    #[test]
    fn damage_state_tints_fill_without_tinting_outline() {
        let (fill, outline) = health_colors(0.5, false, 0.5, 0.1);
        assert_eq!(fill, Color::srgb(1.0, 1.0, 0.0));
        assert_eq!(outline, Color::srgb(127.0 / 255.0, 127.0 / 255.0, 0.0));
        let (fill, _) = health_colors(0.1, false, 0.5, 0.1);
        assert!(fill.to_srgba().green < 0.11);
        let (fill, _) = health_colors(0.5, true, 0.5, 0.1);
        assert!((fill.to_srgba().blue - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn selection_flash_has_source_attack_transition_and_four_step_decay() {
        let mut flash = Flash::new(Vec3::splat(0.25));
        assert!(flash.advance(1.0));
        assert_eq!(flash.current, Vec3::splat(0.25));
        assert!(flash.advance(1.0));
        for expected in [0.1875, 0.125, 0.0625, 0.0] {
            assert!(flash.advance(1.0));
            assert_eq!(flash.current, Vec3::splat(expected));
        }
        assert!(!flash.advance(1.0));
    }
}

#[cfg(test)]
mod capture_tests {
    use super::{Feedback, Flash, SourceMaterial, UnitVisual};
    use bevy::prelude::*;

    fn feedback() -> Feedback {
        use std::collections::{BTreeMap, BTreeSet};
        Feedback {
            show_health: false,
            damaged: 0.5,
            really_damaged: 0.1,
            flash_colors: Vec::new(),
            capture_colors: Vec::new(),
            seen: BTreeSet::new(),
            previous: Vec::new(),
            flashes: BTreeMap::new(),
            disabled: BTreeSet::new(),
            frenzy: BTreeSet::new(),
            gaining_subdual: BTreeSet::new(),
            status_tints: BTreeMap::new(),
            generation: 0,
        }
    }

    fn visual(world: &mut World, index: usize, material: Handle<SourceMaterial>) -> Entity {
        let parent = world
            .spawn(UnitVisual {
                index,
                generation: 0,
            })
            .id();
        world
            .spawn((ChildOf(parent), MeshMaterial3d(material)))
            .id()
    }

    #[test]
    fn retrigger_retains_current_tint_and_uses_fixed_delta_attack() {
        let mut flash = Flash::new(Vec3::splat(0.25));
        flash.advance(1.0);
        flash.advance(1.0);
        flash.advance(1.0);
        assert_eq!(flash.current, Vec3::splat(0.1875));
        flash.play(Vec3::new(-0.25, 0.25, -0.25));
        flash.advance(0.5);
        assert_eq!(flash.current, Vec3::new(-0.03125, 0.21875, -0.03125));
        flash.advance(0.5);
        assert_eq!(flash.current, Vec3::new(-0.25, 0.25, -0.25));
        flash.play(Vec3::new(-0.25, 0.25, -0.25));
        flash.advance(0.5);
        assert_eq!(flash.current, Vec3::new(-0.25, 0.25, -0.25));
        flash.advance(0.5);
        assert_eq!(flash.current, Vec3::new(-0.21875, 0.21875, -0.21875));
    }

    #[test]
    fn disabled_tint_sustains_releases_and_keeps_selection_independent() {
        let mut feedback = feedback();
        feedback.set_disabled(7, true);
        feedback.status_tints.get_mut(&7).unwrap().advance(15.0);
        let halfway = feedback.status_tints[&7].current;
        assert!((halfway.x + 0.25).abs() < 0.000_001);
        feedback.set_disabled(7, true);
        feedback.status_tints.get_mut(&7).unwrap().advance(15.0);
        feedback.status_tints.get_mut(&7).unwrap().advance(60.0);
        let disabled = feedback.status_tints[&7].current;
        assert!((disabled.x + 0.5).abs() < 0.000_001);
        let mut selected = Flash::new(Vec3::splat(0.25));
        selected.advance(1.0);
        feedback.flashes.insert(7, selected);
        assert_eq!(feedback.tint(7), Some(disabled + Vec3::splat(0.25)));
        feedback.flashes.clear();
        assert_eq!(feedback.tint(7), Some(disabled));
        feedback.set_disabled(7, false);
        feedback.status_tints.get_mut(&7).unwrap().advance(15.0);
        assert!((feedback.status_tints[&7].current.x + 0.25).abs() < 0.000_001);
        // Disabling again during the release attacks from the current color.
        feedback.set_disabled(7, true);
        feedback.status_tints.get_mut(&7).unwrap().advance(15.0);
        assert!((feedback.status_tints[&7].current.x + 0.375).abs() < 0.000_001);
        feedback.set_disabled(7, false);
        feedback.status_tints.retain(|_, tint| tint.advance(30.0));
        assert!(feedback.tint(7).is_none());
    }

    #[test]
    fn disabled_sustain_preserves_upstream_double_precision_countdown() {
        let mut tint = Flash::new(Vec3::ZERO);
        tint.play_timed(Vec3::splat(-0.5), 30.0, 30.0, 4_294_967_294.0);
        tint.advance(15.0);
        tint.advance(15.0);
        tint.advance(0.0);
        tint.advance(1.0);
        assert_eq!(tint.sustain.to_bits(), 4_294_967_293.0_f64.to_bits());
    }

    #[test]
    fn target_flash_is_isolated_and_restores_the_new_owners_material() {
        use super::{SelectionTint, tint_models};
        let mut app = App::new();
        let mut flash = Flash::new(Vec3::splat(0.25));
        flash.advance(1.0);
        let mut feedback = feedback();
        feedback.flashes.insert(0, flash);
        app.init_resource::<Assets<SourceMaterial>>()
            .insert_resource(feedback)
            .insert_resource(crate::stealth_view::Cloaks::new())
            .add_systems(Update, tint_models);
        let mut material =
            SourceMaterial::new([1.0; 3], [1.0; 3], [0.0; 3], 1.0, AlphaMode::Opaque, false);
        material.parameters.scene_ambient = Vec4::splat(0.2);
        let old = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(material.clone());
        material.parameters.scene_ambient = Vec4::splat(0.6);
        let new = app
            .world_mut()
            .resource_mut::<Assets<SourceMaterial>>()
            .add(material);
        let target = visual(app.world_mut(), 0, old.clone());
        let other = visual(app.world_mut(), 1, old.clone());
        {
            let mut feedback = app.world_mut().resource_mut::<Feedback>();
            feedback.set_disabled(0, true);
            feedback.status_tints.get_mut(&0).unwrap().advance(15.0);
        }
        app.update();
        let handle = &app
            .world()
            .get::<MeshMaterial3d<SourceMaterial>>(target)
            .unwrap()
            .0;
        let materials = app.world().resource::<Assets<SourceMaterial>>();
        let expected = materials.get(&old).unwrap().parameters.scene_ambient
            + app
                .world()
                .resource::<Feedback>()
                .tint(0)
                .unwrap()
                .extend(0.0);
        assert_eq!(
            materials.get(handle).unwrap().parameters.scene_ambient,
            expected
        );
        assert_ne!(
            app.world()
                .get::<MeshMaterial3d<SourceMaterial>>(target)
                .unwrap()
                .0,
            old
        );
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<SourceMaterial>>(other)
                .unwrap()
                .0,
            old
        );
        app.world_mut()
            .get_mut::<MeshMaterial3d<SourceMaterial>>(target)
            .unwrap()
            .0 = new.clone();
        app.update();
        assert_eq!(
            app.world().get::<SelectionTint>(target).unwrap().original,
            new
        );
        {
            let mut feedback = app.world_mut().resource_mut::<Feedback>();
            feedback.flashes.clear();
            feedback.status_tints.clear();
        }
        app.update();
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<SourceMaterial>>(target)
                .unwrap()
                .0,
            new
        );
        assert!(app.world().get::<SelectionTint>(target).is_none());
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<SourceMaterial>>(other)
                .unwrap()
                .0,
            old
        );
    }
}

type CapturedTintPart = (
    &'static ChildOf,
    &'static MeshMaterial3d<SourceMaterial>,
    &'static SelectionTint,
    &'static ViewVisibility,
    Option<&'static crate::light_environment::Bounds>,
);

#[derive(SystemParam)]
pub struct CaptureTintInspection<'w, 's> {
    feedback: Res<'w, Feedback>,
    materials: Res<'w, Assets<SourceMaterial>>,
    lighting: crate::light_environment::DrawLights<'w>,
    parents: Query<
        'w,
        's,
        (
            &'static UnitVisual,
            Option<&'static crate::light_environment::DrawPose>,
        ),
    >,
    parts: Query<'w, 's, CapturedTintPart>,
}

impl CaptureTintInspection<'_, '_> {
    pub fn visible(&self, target: usize, generation: u32) -> bool {
        if !self
            .feedback
            .flashes
            .get(&target)
            .is_some_and(|flash| flash.current.length() > 0.001)
        {
            return false;
        }
        self.parts
            .iter()
            .any(|(parent, material, tint, visibility, bounds)| {
                let Ok((unit, pose)) = self.parents.get(parent.parent()) else {
                    return false;
                };
                if unit.index != target
                    || unit.generation != generation
                    || !visibility.get()
                    || material.0 != tint.temporary
                {
                    return false;
                }
                self.materials
                    .get(&material.0)
                    .zip(self.materials.get(&tint.original))
                    .is_some_and(|(actual, original)| {
                        let mut expected = original.parameters.clone();
                        let flash = self.feedback.tint(target).unwrap_or(Vec3::ZERO).extend(0.0);
                        expected.scene_ambient += flash;
                        for color in &mut expected.light_color[..3] {
                            *color += flash;
                        }
                        self.lighting.apply(unit, pose, bounds, &mut expected);
                        actual.parameters.scene_ambient == expected.scene_ambient
                            && actual.parameters.light_color == expected.light_color
                            && actual.parameters.light_direction == expected.light_direction
                            && actual.parameters.point_position == expected.point_position
                            && actual.parameters.point_ambient == expected.point_ambient
                    })
            })
    }
}

#[cfg(test)]
mod sabotage_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[ignore = "requires original game installation"]
    #[test]
    fn crate_flash_survives_first_render_generation_reset_and_uses_selection_color() {
        let (assets, battle) = crate::cleanup_verification::tests::installed();
        let mut feedback = load(&assets).unwrap();
        let target = crate::skirmish_verification::friendly_entity(
            &battle.simulation,
            generals_sim::skirmish::Kind::CommandCenter,
            0,
        )
        .unwrap();
        let expected = battle.simulation.player_colors[0]
            .map_or(Vec3::ONE, |index| feedback.flash_colors[index]);
        feedback.flash_as_selected(&battle, target);
        assert_eq!(feedback.flashes[&target].peak, expected);
        assert_eq!(feedback.flashes[&target].decay, expected * -0.25);
        let mut world = World::new();
        world.insert_resource(feedback);
        world.insert_resource(battle);
        world.insert_resource(Time::<()>::default());
        world.run_system_once(update_flashes).unwrap();
        assert!(world.resource::<Feedback>().flashes.contains_key(&target));
    }
    #[test]
    fn mob_health_box_uses_child_centroid_and_original_extra_twenty_height() {
        let position = mob_health_position([240.0, 260.0, 20.0], [200.0, 200.0, 10.0], 1.0);
        assert_eq!(
            position.map(f32::to_bits),
            [240.0_f32, 260.0, 51.0].map(f32::to_bits)
        );
    }
}
