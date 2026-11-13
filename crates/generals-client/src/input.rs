use crate::{
    playtest::Match,
    playtest_ui,
    source_audio::{Voice, VoiceRequest},
};
use bevy::{
    input::keyboard::{KeyboardFocusLost, KeyboardInput},
    prelude::*,
    window::PrimaryWindow,
};
use generals_assets::{content, virtual_assets::VirtualAssets};
use generals_sim::skirmish::Kind;
use std::{error::Error, path::Path, time::Instant};

pub mod attack;
#[cfg(test)]
mod attack_tests;
pub mod bomb_truck;
pub mod burton;
pub mod capture;
pub mod car_bomb;
pub mod cleanup;
pub mod contamination;
pub mod hacker;
mod idle_workers;
mod matching;
pub mod picking;
pub mod pilot;
pub mod placement;
pub mod sabotage;
pub mod salvage;
mod selection;
mod selection_data;
mod timing;
pub use timing::system_interval as system_double_click_interval;
pub mod hijacker;
use picking::View;
pub use selection::keyboard;

#[derive(Resource)]
pub struct Mouse {
    pub dragging: bool,
    pub hovered: Option<usize>,
    pub world_position: Option<Vec3>,
    pub waypoint_mode: bool,
    pub pending_command: CommandMode,
    pub move_hints: Vec<[f32; 2]>,
    pub voices: Vec<VoiceRequest>,
    pub idle_workers: Vec<usize>,
    generation: u32,
    groups: [Vec<usize>; 10],
    group_keys: [KeyCode; 10],
    selection: selection_data::SelectionData,
    clicks: timing::Clicks,
    placement_modifiers: placement::ForceAttack,
    anchor: Option<Vec2>,
    right: Option<(Vec2, Instant, Vec3)>,
    drag_tolerance: f32,
    drag_tolerance_3d: f32,
    drag_seconds: f64,
    preferences: ControlPreferences,
    guard_hint: Option<(Vec2, u8)>,
}

struct ControlPreferences {
    alternate: bool,
    double_click_attack_move: bool,
}

impl Mouse {
    pub const fn force_attack_mode(&self) -> bool {
        self.placement_modifiers.active
    }

    pub const fn alternate_mouse(&self) -> bool {
        self.preferences.alternate
    }

    pub const fn set_alternate_mouse(&mut self, alternate: bool) {
        self.preferences.alternate = alternate;
    }

    pub const fn set_double_click_attack_move(&mut self, enabled: bool) {
        self.preferences.double_click_attack_move = enabled;
        if !enabled {
            self.guard_hint = None;
        }
    }

    pub const fn guard_hint(&self) -> Option<(Vec2, u8)> {
        self.guard_hint
    }

    pub const fn advance_guard_hint(&mut self) {
        if let Some((_, frames)) = &mut self.guard_hint {
            *frames -= 1;
            if *frames == 0 {
                self.guard_hint = None;
            }
        }
    }

    fn reset_generation(&mut self, generation: u32) {
        if self.generation != generation {
            for group in &mut self.groups {
                group.clear();
            }
            self.generation = generation;
            self.pending_command = CommandMode::Normal;
            self.anchor = None;
            self.right = None;
            self.dragging = false;
            self.hovered = None;
            self.move_hints.clear();
            self.voices.clear();
            self.idle_workers.clear();
            self.clicks.clear();
            self.guard_hint = None;
            self.placement_modifiers.reset();
        }
    }
}

pub enum CommandMode {
    Normal,
    AttackMove,
    CombatDrop,
    Guard,
    GuardFlying,
    Rally,
    Capture,
    LaserLock,
    DisableBuilding,
    Hack(usize),
    Cleanup,
    FireWeapon(usize),
    ClearMines(usize),
    SnipeVehicle(usize),
    Burton(burton::TargetCommand),
    BombTruckDisguise,
    CarBomb,
    Hijack,
    HelixBomb,
    Sabotage,
    Power {
        caster: usize,
        name: String,
        target_object: bool,
    },
}

#[derive(Clone, Copy)]
pub enum PickTarget {
    Unit(usize),
    Supply(usize),
}

impl PickTarget {
    const fn unit(self) -> Option<usize> {
        match self {
            Self::Unit(id) => Some(id),
            Self::Supply(_) => None,
        }
    }
}

impl Mouse {
    pub fn selection_voice(&mut self, battle: &Match) {
        self.voices.push(VoiceRequest::new(
            &battle.simulation,
            &battle.selected,
            Voice::Select,
            self.waypoint_mode,
        ));
    }
}

#[derive(Component)]
pub struct SelectionRegion;

pub fn load(
    assets: &VirtualAssets,
    options: &Path,
    catalog: &[generals_sim::skirmish::UnitType],
) -> Result<Mouse, Box<dyn Error>> {
    let file = content::scan(assets.read("data/ini/mouse.ini")?)?;
    let declaration = file
        .declarations
        .iter()
        .find(|item| item.kind == "Mouse")
        .ok_or("Mouse.ini has no Mouse definition")?;
    let number = |key: &str| -> Result<u16, Box<dyn Error>> {
        let line = declaration
            .lines
            .iter()
            .find(|line| {
                line.depth == Some(0) && line.tokens.first().is_some_and(|value| value == key)
            })
            .ok_or_else(|| format!("Mouse.ini is missing {key}"))?;
        let [_, value] = line.tokens.as_slice() else {
            return Err(format!("Mouse.ini has invalid {key}").into());
        };
        Ok(value.parse()?)
    };
    // GlobalData's original default is false; local Options.ini overrides it.
    let mut alternate = false;
    if options.is_file() {
        for line in std::fs::read_to_string(options)?.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case("UseAlternateMouse") {
                alternate = match value.trim().to_ascii_lowercase().as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return Err("invalid Options.ini UseAlternateMouse".into()),
                };
            }
        }
    }
    Ok(Mouse {
        dragging: false,
        hovered: None,
        world_position: None,
        waypoint_mode: false,
        pending_command: CommandMode::Normal,
        move_hints: Vec::new(),
        voices: Vec::new(),
        idle_workers: Vec::new(),
        generation: 0,
        groups: std::array::from_fn(|_| Vec::new()),
        group_keys: selection::load_group_keys(assets)?,
        selection: selection_data::SelectionData::load(assets, catalog)?,
        clicks: timing::Clicks::new(timing::system_interval()),
        placement_modifiers: placement::ForceAttack::new(),
        anchor: None,
        right: None,
        drag_tolerance: f32::from(number("DragTolerance")?),
        drag_tolerance_3d: f32::from(number("DragTolerance3D")?) * 0.1,
        drag_seconds: f64::from(number("DragToleranceMS")?) / 1000.0,
        preferences: ControlPreferences {
            alternate,
            double_click_attack_move: false,
        },
        guard_hint: None,
    })
}

pub fn setup(mut commands: Commands) {
    // W3DInGameUI::drawSelectionRegion: 2px, ARGB 0x9933FF33.
    commands.spawn((
        SelectionRegion,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(Color::srgba_u8(0x33, 0xff, 0x33, 0x99)),
        ZIndex(10),
    ));
}

pub fn pointer(
    mut mouse: ResMut<Mouse>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    view: View,
    mut battle: ResMut<Match>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut focus_lost: MessageReader<KeyboardFocusLost>,
) {
    view.update_pointer(
        &mut mouse,
        buttons.into_inner(),
        keys.into_inner(),
        &mut battle,
    );
    // Like GameEngine::update, update the preview before dispatching the next
    // modifier messages shared by placement snapping and force-attack commands.
    for event in keyboard.read() {
        mouse
            .placement_modifiers
            .event(event.key_code, event.state.is_pressed(), event.repeat);
    }
    if focus_lost.read().next().is_some() {
        mouse.placement_modifiers.reset();
        focus_lost.clear();
    }
}

impl View<'_, '_> {
    fn update_pointer(
        self,
        mouse: &mut Mouse,
        buttons: &ButtonInput<MouseButton>,
        keys: &ButtonInput<KeyCode>,
        battle: &mut Match,
    ) {
        mouse.reset_generation(battle.generation);
        if !battle.started || battle.paused || battle.simulation.end_game_at.is_some() {
            mouse.anchor = None;
            mouse.right = None;
            mouse.dragging = false;
            mouse.hovered = None;
            mouse.world_position = None;
            mouse.waypoint_mode = false;
            return;
        }
        mouse.waypoint_mode = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
        let Some(cursor) = self.window.cursor_position() else {
            mouse.hovered = None;
            mouse.world_position = None;
            if !buttons.pressed(MouseButton::Left) {
                mouse.anchor = None;
                mouse.dragging = false;
            }
            if !buttons.pressed(MouseButton::Right) {
                mouse.right = None;
            }
            return;
        };
        let over_ui =
            playtest_ui::captures_pointer(battle, &self.window, cursor, &self.ui, &self.promotions);
        if playtest_ui::over_promotions(battle, &self.window, cursor, &self.ui, &self.promotions) {
            battle.placing = None;
        }
        let picked = if over_ui {
            None
        } else {
            self.pick_context(battle, cursor, mouse.force_attack_mode())
        };
        mouse.hovered = picked.and_then(PickTarget::unit);
        mouse.world_position = (!over_ui)
            .then(|| self.world_point(battle, cursor))
            .flatten();
        let placing = placement::pointer(battle, mouse, &self, buttons, cursor, over_ui);
        if !placing && buttons.just_pressed(MouseButton::Left) && !over_ui {
            mouse.clicks.left_down(
                Instant::now(),
                cursor * self.window.scale_factor(),
                mouse.drag_tolerance,
            );
            mouse.anchor = Some(mouse.clicks.origin / self.window.scale_factor());
            mouse.dragging = false;
        }
        if buttons.just_pressed(MouseButton::Right) && !over_ui {
            let now = Instant::now();
            mouse.clicks.right_down(
                now,
                cursor * self.window.scale_factor(),
                mouse.drag_tolerance,
            );
            mouse.right = Some((cursor, now, self.camera.1.translation()));
        }
        if !placing {
            left_pointer(mouse, buttons, keys, &self, battle, cursor);
        }
        if buttons.just_released(MouseButton::Right)
            && let Some((anchor, time, position)) = mouse.right.take()
            && time.elapsed().as_secs_f64() <= mouse.drag_seconds
            && cursor.distance(anchor) * self.window.scale_factor() <= mouse.drag_tolerance
            && self.camera.1.translation().distance(position) <= mouse.drag_tolerance_3d
            && !over_ui
        {
            if mouse.preferences.alternate
                && mouse.preferences.double_click_attack_move
                && mouse.clicks.double_right
                && battle.placing.is_none()
            {
                let area = click_region(
                    mouse.clicks.right_origin / self.window.scale_factor(),
                    cursor,
                    self.window.scale_factor(),
                    mouse.drag_tolerance,
                );
                let point = self.world_point(battle, area.min);
                double_click_guard(battle, mouse, point, self.terrain_point(cursor));
                return;
            }
            let point = self.world_point(battle, cursor);
            right_click(battle, mouse, picked, point);
        }
    }
}

fn right_click(
    battle: &mut Match,
    mouse: &mut Mouse,
    picked: Option<PickTarget>,
    point: Option<Vec3>,
) {
    if mouse.preferences.alternate && mouse.force_attack_mode() && battle.placing.is_none() {
        attack::force_order(battle, picked.and_then(PickTarget::unit), point, mouse);
        return;
    }
    if !matches!(mouse.pending_command, CommandMode::Normal) {
        mouse.pending_command = CommandMode::Normal;
        return;
    }
    if battle.placing.take().is_some() {
        return;
    }
    if mouse.preferences.alternate {
        context_order_world(battle, point, picked, mouse);
    } else {
        battle.clear_drawable_selection();
    }
}

fn left_pointer(
    mouse: &mut Mouse,
    buttons: &ButtonInput<MouseButton>,
    keys: &ButtonInput<KeyCode>,
    view: &View,
    battle: &mut Match,
    cursor: Vec2,
) {
    let Some(anchor) = mouse.anchor else {
        return;
    };
    // SelectionXlat::onRawMousePosition uses per-axis physical pixel tolerance.
    let delta = (cursor - anchor).abs() * view.window.scale_factor();
    mouse.dragging |= battle.placing.is_none()
        && matches!(mouse.pending_command, CommandMode::Normal)
        && delta.max_element() > mouse.drag_tolerance;
    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    let extend = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let area = click_region(
        anchor,
        cursor,
        view.window.scale_factor(),
        mouse.drag_tolerance,
    );
    let point_selection = area.min == area.max;
    if mouse.clicks.double_left && battle.placing.is_none() {
        if !playtest_ui::captures_pointer(battle, &view.window, cursor, &view.ui, &view.promotions)
        {
            let selected = point_selection
                && !keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
                && matching::double_click(
                    battle,
                    mouse,
                    view,
                    view.pick(battle, area.min).and_then(PickTarget::unit),
                    extend,
                    mouse.waypoint_mode,
                );
            if !selected
                && !mouse.preferences.alternate
                && mouse.preferences.double_click_attack_move
            {
                let point = view.world_point(battle, area.min);
                double_click_guard(battle, mouse, point, view.terrain_point(cursor));
            }
        }
    } else if !point_selection
        && battle.placing.is_none()
        && matches!(mouse.pending_command, CommandMode::Normal)
    {
        if selection::region(battle, area, extend, view, mouse) {
            mouse.selection_voice(battle);
        }
    } else if !playtest_ui::captures_pointer(
        battle,
        &view.window,
        cursor,
        &view.ui,
        &view.promotions,
    ) {
        let point = view.world_point(battle, area.min);
        let force_pick = mouse.force_attack_mode()
            && (!mouse.preferences.alternate || attack::left_gui_command(&mouse.pending_command));
        let picked = view.pick_context(battle, area.min, force_pick);
        left_click(battle, point, picked, extend, mouse);
    }
    mouse.anchor = None;
    mouse.dragging = false;
}

fn double_click_guard(
    battle: &mut Match,
    mouse: &mut Mouse,
    point: Option<Vec3>,
    hint: Option<Vec2>,
) {
    let Some(point) = point else {
        return;
    };
    let response = VoiceRequest::new(&battle.simulation, &battle.selected, Voice::Guard, false);
    let members = selection::command_units(battle);
    if battle
        .simulation
        .group_guard_world(&members, point.to_array(), false)
        .is_ok()
    {
        mouse.voices.push(response);
    }
    // InGameUI's timer is decremented by createCommandHint, once per client update.
    if let Some(position) = hint {
        mouse.guard_hint = Some((position, 11));
    }
}

fn click_region(anchor: Vec2, cursor: Vec2, scale: f32, tolerance: f32) -> Rect {
    // MetaEventTranslator collapses final deltas strictly below tolerance;
    // the point remains the region's low corner, not the release coordinate.
    let mut area = Rect::from_corners(anchor, cursor);
    let delta = (cursor - anchor).abs() * scale;
    if delta.x < tolerance && delta.y < tolerance {
        area.max = area.min;
    }
    area
}

fn left_click(
    battle: &mut Match,
    point: Option<Vec3>,
    picked: Option<PickTarget>,
    extend: bool,
    mouse: &mut Mouse,
) {
    let nearest = picked.and_then(PickTarget::unit);
    if mouse.force_attack_mode()
        && (!mouse.preferences.alternate || attack::left_gui_command(&mouse.pending_command))
    {
        attack::force_order(battle, nearest, point, mouse);
        return;
    }
    if !matches!(mouse.pending_command, CommandMode::Normal) {
        if let Some(point) = point {
            targeted_order_world(battle, mouse, point, nearest);
        }
        return;
    }
    // Prefer-selection overrides own-object context, not enemy/terrain commands.
    let prefer_own = extend && nearest.is_some_and(|id| battle.simulation.entities[id].owner == 0);
    if !mouse.preferences.alternate
        && !prefer_own
        && context_order_world(battle, point, picked, mouse)
    {
        return;
    }
    if let Some(id) = nearest {
        let id = battle.simulation.entities[id].spawn_master().unwrap_or(id);
        mouse.clicks.manual_selection();
        let unit = &battle.simulation.entities[id];
        if unit.crate_present() {
            return;
        }
        if extend && battle.selected.contains(&id) {
            let removed: Vec<_> = battle
                .selected
                .iter()
                .copied()
                .filter(|&selected| {
                    selected == id
                        || battle.simulation.entities[selected].spawn_master() == Some(id)
                })
                .collect();
            battle.selection_remove(&removed);
            battle
                .selected
                .retain(|selected| !removed.contains(selected));
            return;
        }
        let replace = !extend
            || unit.owner != 0
            || battle.simulation.catalog[unit.type_index]
                .kind
                .is_building()
            || selection::has_building_or_enemy(battle);
        battle.select_objects(&[id], replace);
        mouse.selection_voice(battle);
    } else if mouse.preferences.alternate && !extend && !mouse.waypoint_mode {
        battle.clear_drawable_selection();
        mouse.clicks.manual_selection();
    }
}

pub fn targeted_order(battle: &mut Match, mouse: &mut Mouse, point: Vec2, nearest: Option<usize>) {
    let point = point.extend(battle.simulation.ground_height(point.to_array()));
    targeted_order_world(battle, mouse, point, nearest);
}

fn targeted_order_world(
    battle: &mut Match,
    mouse: &mut Mouse,
    world: Vec3,
    nearest: Option<usize>,
) {
    let point = world.truncate();
    if matches!(mouse.pending_command, CommandMode::CombatDrop) {
        combat_drop(battle, mouse, world, nearest);
        return;
    }
    if matches!(mouse.pending_command, CommandMode::HelixBomb) {
        targeted_helix_bomb(battle, mouse, point);
        return;
    }
    if matches!(mouse.pending_command, CommandMode::Hijack) {
        if let Some(target) = nearest {
            hijacker::issue(battle, mouse, target);
        }
        return;
    }
    if matches!(mouse.pending_command, CommandMode::Sabotage) {
        sabotage::targeted(battle, mouse, point, nearest);
        return;
    }
    if let CommandMode::Burton(command) = mouse.pending_command {
        if let Some(target) = nearest {
            burton::issue(battle, mouse, command, target);
        }
        return;
    }
    if matches!(mouse.pending_command, CommandMode::BombTruckDisguise) {
        if let Some(target) = nearest {
            bomb_truck::issue_disguise(battle, mouse, target);
        }
        return;
    }
    if let CommandMode::SnipeVehicle(slot) = mouse.pending_command {
        if let Some(target) = nearest {
            pilot::issue(battle, mouse, target, slot);
        }
        return;
    }
    if matches!(mouse.pending_command, CommandMode::CarBomb) {
        if mouse.waypoint_mode {
            context_order(battle, Some(point), nearest.map(PickTarget::Unit), mouse);
        } else if let Some(target) = nearest {
            car_bomb::issue(battle, mouse, target, false);
        }
        return;
    }
    if let CommandMode::FireWeapon(slot) = mouse.pending_command {
        contamination::issue(battle, mouse, slot, point.to_array());
        return;
    }
    if let CommandMode::ClearMines(slot) = mouse.pending_command {
        contamination::clear_mines(battle, mouse, slot, point.to_array());
        return;
    }
    if matches!(mouse.pending_command, CommandMode::Cleanup) {
        cleanup::issue(battle, mouse, point.to_array());
        return;
    }
    if matches!(mouse.pending_command, CommandMode::Rally) {
        rally_order(battle, mouse, world);
        return;
    }
    if let CommandMode::Hack(ability) = mouse.pending_command {
        if let Some(target) = nearest {
            hacker::issue_finite(battle, mouse, target, ability);
        }
        return;
    }
    if matches!(mouse.pending_command, CommandMode::DisableBuilding) {
        if let Some(target) = nearest {
            hacker::issue(battle, mouse, target, false);
        }
        return;
    }
    if matches!(mouse.pending_command, CommandMode::LaserLock) {
        targeted_laser(battle, mouse, nearest);
        return;
    }
    if matches!(mouse.pending_command, CommandMode::Capture) {
        if let Some(target) = nearest {
            capture::issue(battle, mouse, target);
        }
        return;
    }
    if matches!(mouse.pending_command, CommandMode::Power { .. }) {
        targeted_power(battle, mouse, world, nearest);
        return;
    }
    targeted_movement(battle, mouse, world);
}

fn targeted_helix_bomb(battle: &mut Match, mouse: &mut Mouse, point: Vec2) {
    let response = VoiceRequest::special_power(
        &battle.simulation,
        &battle.selected,
        &["SPECIAL_HELIX_NAPALM_BOMB"],
    );
    let mut issued = false;
    for id in selection::command_units(battle) {
        if locally_controlled(battle, id) && battle.simulation.helix_bomb_ready(id) {
            issued |= battle.simulation.helix_bomb(id, point.to_array()).is_ok();
        }
    }
    if issued {
        mouse.voices.extend(response);
        mouse.pending_command = CommandMode::Normal;
    }
}

fn combat_drop(battle: &mut Match, mouse: &mut Mouse, position: Vec3, target: Option<usize>) {
    let response = VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        Voice::CombatDrop,
        false,
    );
    let mut issued = false;
    for id in battle.command_message_members() {
        if locally_controlled(battle, id) {
            issued |= battle
                .simulation
                .combat_drop(id, target, position.to_array())
                .is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
}

pub fn targeted_movement(battle: &mut Match, mouse: &mut Mouse, point: Vec3) {
    let voice = match mouse.pending_command {
        CommandMode::AttackMove => Voice::Move,
        CommandMode::Guard | CommandMode::GuardFlying => Voice::Guard,
        _ => return,
    };
    let response = VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        voice,
        mouse.waypoint_mode,
    );
    let members = selection::command_units(battle);
    let position = point.to_array();
    let result = match mouse.pending_command {
        CommandMode::AttackMove => battle
            .simulation
            .group_attack_move_world(&members, position),
        CommandMode::Guard | CommandMode::GuardFlying => battle.simulation.group_guard_world(
            &members,
            position,
            matches!(mouse.pending_command, CommandMode::GuardFlying),
        ),
        _ => return,
    };
    if let Err(error) = result {
        eprintln!("Targeted order rejected: {error}");
    } else {
        mouse.voices.push(response);
        if matches!(mouse.pending_command, CommandMode::AttackMove) {
            mouse.move_hints.push(point.truncate().to_array());
        }
        mouse.pending_command = CommandMode::Normal;
    }
}

fn targeted_power(battle: &mut Match, mouse: &mut Mouse, point: Vec3, nearest: Option<usize>) {
    let CommandMode::Power {
        caster,
        name,
        target_object,
    } = &mouse.pending_command
    else {
        return;
    };
    let target = if *target_object {
        let Some(id) = nearest else {
            return;
        };
        generals_sim::skirmish::PowerTarget::Object(id)
    } else {
        generals_sim::skirmish::PowerTarget::Position3D(point.to_array())
    };
    drop(battle.command_message_members());
    match battle.simulation.cast_power(*caster, name, target) {
        Ok(()) => {
            mouse.pending_command = CommandMode::Normal;
            apply_spectre_selections(battle);
        }
        Err(error) => eprintln!("Special power rejected: {error}"),
    }
}

/// `GameLogic::selectObject` replaces the logic selection, but `selectDrawable`
/// prepends the ship without clearing other selected drawables or playing a voice.
pub fn apply_spectre_selections(battle: &mut Match) {
    for added in std::mem::take(&mut battle.simulation.spawn_selection_events) {
        for &id in &added {
            if !battle.selected.contains(&id) {
                battle.selected.insert(0, id);
            }
        }
        battle.selection_create(&added, false);
    }

    for id in std::mem::take(&mut battle.simulation.deselected_objects) {
        battle.selected.retain(|&selected| selected != id);
        battle.logic_selection = battle.simulation.selected_objects(0).to_vec();
    }
    for id in std::mem::take(&mut battle.simulation.spectre_selections) {
        if battle
            .simulation
            .entities
            .get(id)
            .is_some_and(|object| object.exists() && object.owner == 0)
        {
            battle.logic_selection.clear();
            battle.logic_selection.push(id);
            if !battle.selected.contains(&id) {
                battle.selected.insert(0, id);
            }
        }
    }
}

pub fn rally_order(battle: &mut Match, mouse: &mut Mouse, point: Vec3) -> bool {
    let mut issued = false;
    for id in selection::command_units(battle) {
        if locally_controlled(battle, id) {
            issued |= battle
                .simulation
                .set_rally_point(id, point.to_array())
                .is_ok();
        }
    }
    if issued {
        mouse.pending_command = CommandMode::Normal;
    }
    issued
}

fn targeted_laser(battle: &mut Match, mouse: &mut Mouse, nearest: Option<usize>) {
    let Some(target) = nearest else { return };
    if !battle.simulation.visible_to(0, target) {
        return;
    }
    let response = VoiceRequest::new(
        &battle.simulation,
        &battle.selected,
        Voice::LaserAttack,
        false,
    );
    let mut issued = false;
    for id in selection::command_units(battle) {
        if battle
            .simulation
            .entities
            .get(id)
            .is_some_and(|actor| actor.owner == 0)
            && battle.simulation.can_laser_lock(id, target)
        {
            issued |= battle.simulation.laser_lock(id, target).is_ok();
        }
    }
    if issued {
        mouse.voices.push(response);
        mouse.pending_command = CommandMode::Normal;
    }
}

fn context_voice(
    battle: &mut Match,
    nearest: Option<usize>,
    waypoints: bool,
    supply: bool,
) -> Voice {
    if waypoints {
        Voice::Move
    } else if let Some(target) = nearest {
        if attack::ordinary_selection_target(battle, target) {
            let unit = &battle.simulation.entities[target];
            let kind = &battle.simulation.catalog[unit.type_index];
            if unit.is_using_airborne_locomotor(kind) {
                Voice::AttackAir
            } else {
                Voice::Attack
            }
        } else if battle
            .selected
            .iter()
            .any(|&id| battle.simulation.can_repair(id, target))
        {
            if battle.selected.iter().any(|&id| {
                battle.simulation.catalog[battle.simulation.entities[id].type_index]
                    .aircraft
                    .is_some()
            }) {
                Voice::Move
            } else {
                Voice::Repair
            }
        } else if !battle.simulation.entities[target].complete() {
            Voice::Build
        } else {
            Voice::Supply
        }
    } else if supply {
        Voice::Supply
    } else {
        Voice::Move
    }
}

pub fn context_order(
    battle: &mut Match,
    point: Option<Vec2>,
    picked: Option<PickTarget>,
    mouse: &mut Mouse,
) -> bool {
    let point = point.map(|point| point.extend(battle.simulation.ground_height(point.to_array())));
    context_order_world(battle, point, picked, mouse)
}

fn object_context(
    game: &mut generals_sim::skirmish::Skirmish,
    id: usize,
    target: usize,
    can_dock: bool,
) -> Option<Result<(), &'static str>> {
    let unit = &game.entities[id];
    let stats = &game.catalog[unit.type_index];
    let victim = &game.entities[target];
    let result = if game.can_repair(id, target) {
        game.repair(id, target)
    } else if game.player_attack_target_reachable(id, target) {
        game.attack(id, target)
    } else if victim.owner == unit.owner && !victim.complete() && stats.kind == Kind::Builder {
        game.assist_build(id, target)
    } else if victim.owner == unit.owner
        && victim.complete()
        && can_dock
        && stats.supply.capacity > 0
        && game.catalog[victim.type_index].kind == Kind::SupplyCenter
    {
        game.deliver_supplies(id, target)
    } else {
        return None;
    };
    Some(result)
}

fn ordinary_context_eligible(
    battle: &Match,
    nearest: Option<usize>,
    supply: Option<usize>,
    can_dock: bool,
    group_point: Option<Vec3>,
    waypoints: bool,
) -> bool {
    battle.selected.iter().copied().any(|id| {
        let unit = &battle.simulation.entities[id];
        let stats = &battle.simulation.catalog[unit.type_index];
        if unit.owner != 0 || !unit.complete() {
            return false;
        }
        if let Some(target) = nearest {
            let victim = &battle.simulation.entities[target];
            return battle.simulation.can_repair(id, target)
                || battle.simulation.player_attack_target_reachable(id, target)
                || (victim.owner == unit.owner
                    && !victim.complete()
                    && stats.kind == Kind::Builder)
                || (victim.owner == unit.owner
                    && victim.complete()
                    && can_dock
                    && stats.supply.capacity > 0
                    && battle.simulation.catalog[victim.type_index].kind == Kind::SupplyCenter);
        }
        if supply.is_some() {
            return can_dock && stats.supply.capacity > 0;
        }
        group_point.is_some()
            && !waypoints
            && stats.kind.is_building()
            && (stats.behavior.production_exit.is_some()
                || stats.parking.is_some()
                || battle.simulation.is_tunnel(id))
    })
}

fn context_order_world(
    battle: &mut Match,
    point: Option<Vec3>,
    picked: Option<PickTarget>,
    mouse: &mut Mouse,
) -> bool {
    if !mouse.waypoint_mode
        && let Some(position) = point
    {
        let mut overridden = false;
        for id in selection::command_units(battle) {
            if locally_controlled(battle, id) {
                overridden |= battle.simulation.override_special_power_destination(
                    id,
                    position.to_array(),
                    0,
                );
            }
        }
        if overridden {
            return true;
        }
    }
    if mouse.force_attack_mode() {
        attack::force_order(battle, picked.and_then(PickTarget::unit), point, mouse);
        return true;
    }
    let waypoints = mouse.waypoint_mode;
    // CommandXlat docks at the picked source drawable, not the terrain beneath
    // its roof/crates. The projected terrain point may be outside its footprint.
    let nearest = picked.and_then(PickTarget::unit);
    if let Some(issued) = special_context(battle, mouse, nearest) {
        return issued;
    }
    if let Some(target) = nearest
        && salvage::issue(battle, mouse, target)
    {
        return true;
    }
    if !waypoints
        && let Some(carrier) = nearest
        && board_context(battle, mouse, carrier)
    {
        return true;
    }
    let supply = match picked {
        Some(PickTarget::Supply(id)) => Some(id),
        _ => None,
    };
    // CommandXlat uses SELECTION_ALL for docking rather than splitting a mixed
    // selection into a gather command and an unrelated movement command.
    let can_dock = can_dock_at(battle, picked);
    let voice = context_voice(battle, nearest, waypoints, supply.is_some() && can_dock);
    let response = VoiceRequest::new(&battle.simulation, &battle.selected, voice, waypoints);
    let group_point = nearest
        .map(|id| Vec3::from_array(battle.simulation.object_position(id)))
        .or_else(|| {
            supply.map(|id| {
                let point = battle.simulation.supply_position(id);
                Vec3::new(point[0], point[1], battle.simulation.ground_height(point))
            })
        })
        .or(point);
    if (waypoints || picked.is_none())
        && let Some(destination) = group_point
        && move_group(battle, destination, waypoints)
    {
        mouse.move_hints.push(destination.truncate().to_array());
        mouse.voices.push(response);
        return true;
    }
    if let Some(target) =
        nearest.filter(|&target| attack::ordinary_selection_target(battle, target))
    {
        let members = selection::command_units(battle);
        if let Err(error) = battle
            .simulation
            .group_attack_objects(&members, target, false)
        {
            eprintln!("Attack order rejected: {error}");
        }
        mouse.voices.push(response);
        return true;
    }
    if !ordinary_context_eligible(battle, nearest, supply, can_dock, group_point, waypoints) {
        return false;
    }
    let mut issued = false;
    let mut unit_order = false;
    for id in selection::command_units(battle) {
        let Some((rally, result)) = actor_context_order(
            &mut battle.simulation,
            id,
            group_point,
            picked,
            can_dock,
            waypoints,
        ) else {
            continue;
        };
        issued = true;
        unit_order |= !rally;
        if let Err(error) = result {
            eprintln!("Context order rejected: {error}");
        }
    }
    if unit_order {
        mouse.voices.push(response);
    }
    issued
}

fn actor_context_order(
    game: &mut generals_sim::skirmish::Skirmish,
    id: usize,
    point: Option<Vec3>,
    picked: Option<PickTarget>,
    can_dock: bool,
    waypoints: bool,
) -> Option<(bool, Result<(), &'static str>)> {
    let nearest = picked.and_then(PickTarget::unit);
    let supply = match picked {
        Some(PickTarget::Supply(id)) => Some(id),
        _ => None,
    };
    let unit = &game.entities[id];
    if unit.owner != 0 || !unit.complete() {
        return None;
    }
    let stats = &game.catalog[unit.type_index];
    let rally = !waypoints
        && picked.is_none()
        && stats.kind.is_building()
        && (stats.behavior.production_exit.is_some()
            || stats.parking.is_some()
            || game.is_tunnel(id));
    let result = if rally {
        let point = point?;
        game.set_rally_point(id, point.to_array())
    } else if let Some(target) = nearest {
        object_context(game, id, target, can_dock)?
    } else if let Some(supply) = supply.filter(|_| can_dock) {
        game.harvest(id, supply)
    } else {
        return None;
    };
    Some((rally, result))
}

fn special_context(battle: &mut Match, mouse: &mut Mouse, nearest: Option<usize>) -> Option<bool> {
    if let Some(target) = hijacker::context_target(battle, mouse, nearest) {
        return Some(hijacker::issue(battle, mouse, target));
    }
    if let Some(target) = car_bomb::context_target(battle, mouse, nearest) {
        return Some(car_bomb::issue(battle, mouse, target, true));
    }
    if let Some(target) = sabotage::context_target(battle, mouse, nearest) {
        return Some(sabotage::issue(battle, mouse, target));
    }
    if let Some(target) = capture::context_target(battle, mouse, nearest) {
        return Some(capture::issue(battle, mouse, target));
    }
    if let Some((target, ability)) = hacker::finite_context_target(battle, mouse, nearest) {
        return Some(hacker::issue_finite_context(battle, mouse, target, ability));
    }
    hacker::context_target(battle, mouse, nearest)
        .map(|target| hacker::issue(battle, mouse, target, true))
}

// CommandXlat uses SELECTION_ANY for boarding before attack/repair/build.
fn board_context(battle: &mut Match, mouse: &mut Mouse, carrier: usize) -> bool {
    if !battle
        .selected
        .iter()
        .any(|&id| locally_controlled(battle, id) && battle.simulation.can_enter(id, carrier))
    {
        return false;
    }
    let response = VoiceRequest::new(&battle.simulation, &battle.selected, Voice::Enter, false);
    for id in selection::command_units(battle) {
        if locally_controlled(battle, id)
            && battle.simulation.can_enter(id, carrier)
            && let Err(error) = battle.simulation.enter(id, carrier)
        {
            eprintln!("Boarding rejected: {error}");
        }
    }
    mouse.voices.push(response);
    true
}

pub fn locally_controlled(battle: &Match, id: usize) -> bool {
    battle
        .simulation
        .entities
        .get(id)
        .is_some_and(|actor| actor.owner == 0)
}

fn move_group(battle: &mut Match, destination: Vec3, waypoints: bool) -> bool {
    let eligible = battle.selected.iter().copied().any(|id| {
        let unit = &battle.simulation.entities[id];
        unit.owner == 0 && unit.complete() && battle.simulation.catalog[unit.type_index].speed > 0.0
    });
    if !eligible {
        return false;
    }
    let members: Vec<_> = selection::command_units(battle)
        .into_iter()
        .filter(|&id| {
            let unit = &battle.simulation.entities[id];
            unit.complete() && battle.simulation.catalog[unit.type_index].speed > 0.0
        })
        .collect();
    if let Err(error) =
        battle
            .simulation
            .group_move_to_world(&members, destination.to_array(), waypoints)
    {
        eprintln!("Group order rejected: {error}");
    }
    true
}

fn can_dock_at(battle: &Match, picked: Option<PickTarget>) -> bool {
    let center = match picked {
        Some(PickTarget::Supply(id)) if battle.simulation.supplies[id].boxes > 0 => false,
        Some(PickTarget::Unit(id)) => {
            let target = &battle.simulation.entities[id];
            if target.owner != 0
                || !target.complete()
                || battle.simulation.catalog[target.type_index]
                    .neutral_tech
                    .repair_dock
                    .is_some()
                || battle.simulation.catalog[target.type_index]
                    .supply
                    .dock
                    .is_none()
            {
                return false;
            }
            true
        }
        _ => return false,
    };
    !battle.selected.is_empty()
        && battle.selected.iter().all(|&id| {
            let unit = &battle.simulation.entities[id];
            unit.owner == 0
                && unit.complete()
                && battle.simulation.catalog[unit.type_index].supply.capacity > 0
                && (!center || unit.cargo > 0)
        })
}

pub fn draw_region(
    mouse: Res<Mouse>,
    window: Single<&Window, With<PrimaryWindow>>,
    region: Single<&mut Node, With<SelectionRegion>>,
) {
    let mouse = mouse.into_inner();
    let window = window.into_inner();
    let mut node = region.into_inner();
    let area = mouse
        .anchor
        .zip(window.cursor_position())
        .map(|(anchor, cursor)| Rect::from_corners(anchor, cursor));
    if mouse.dragging
        && let Some(area) = area
    {
        node.display = Display::Flex;
        node.left = px(area.min.x);
        node.top = px(area.min.y);
        node.width = px(area.width());
        node.height = px(area.height());
        node.border = UiRect::all(px(2.0 / window.scale_factor()));
    } else {
        node.display = Display::None;
    }
}

#[cfg(test)]
mod tests {
    use super::click_region;
    use bevy::prelude::*;

    #[test]
    fn click_region_preserves_source_exact_threshold_and_low_corner() {
        let anchor = Vec2::new(100.0, 100.0);
        assert_eq!(
            click_region(anchor, Vec2::new(110.0, 112.0), 2.0, 25.0),
            Rect::from_corners(anchor, anchor)
        );
        assert_eq!(
            click_region(anchor, Vec2::new(112.5, 100.0), 2.0, 25.0),
            Rect::from_corners(anchor, Vec2::new(112.5, 100.0))
        );
        let low = Vec2::new(96.0, 98.0);
        assert_eq!(
            click_region(anchor, low, 2.0, 25.0),
            Rect::from_corners(low, low)
        );
    }
    #[ignore = "requires original game installation"]
    #[test]
    fn double_click_issues_guard_and_hint_expires_in_client_updates() {
        let (assets, mut battle) = crate::cleanup_verification::tests::installed();
        let root = &generals_assets::game_root().unwrap();
        let mut mouse = super::load(
            &assets,
            &root.join("UserData/Options.ini"),
            &battle.simulation.catalog,
        )
        .unwrap();
        let id = battle
            .simulation
            .entities
            .iter()
            .position(|actor| {
                actor.owner == 0 && battle.simulation.catalog[actor.type_index].speed > 0.0
            })
            .unwrap();
        battle.select_objects(&[id], true);
        let position = Vec2::from_array(battle.simulation.entities[id].position);
        let world = position.extend(battle.simulation.ground_height(position.to_array()));
        mouse.set_double_click_attack_move(true);
        super::double_click_guard(
            &mut battle,
            &mut mouse,
            Some(world),
            Some(position + Vec2::ONE),
        );
        assert!(
            matches!(battle.simulation.entities[id].ai.order, generals_sim::skirmish::Order::Guard(point) if point.map(f32::to_bits) == position.to_array().map(f32::to_bits))
        );
        for remaining in (1..=10).rev() {
            mouse.advance_guard_hint();
            assert_eq!(mouse.guard_hint(), Some((position + Vec2::ONE, remaining)));
        }
        super::double_click_guard(&mut battle, &mut mouse, Some(world), None);
        assert_eq!(mouse.guard_hint(), Some((position + Vec2::ONE, 1)));
        mouse.advance_guard_hint();
        assert!(mouse.guard_hint().is_none());
        super::double_click_guard(
            &mut battle,
            &mut mouse,
            Some(world),
            Some(position + Vec2::ONE),
        );
        mouse.set_double_click_attack_move(false);
        assert!(mouse.guard_hint().is_none());
    }

    #[ignore = "requires original game installation"]
    #[test]
    fn native_spectre_selection_replaces_logic_and_prepends_only_local_drawable() {
        let (_, mut battle) = crate::cleanup_verification::tests::installed();
        let local: Vec<_> = battle
            .simulation
            .entities
            .iter()
            .enumerate()
            .filter(|(_, object)| object.owner == 0 && object.alive())
            .map(|(id, _)| id)
            .take(2)
            .collect();
        let foreign = battle
            .simulation
            .entities
            .iter()
            .position(|object| object.owner == 1 && object.alive())
            .unwrap();
        battle.selected = vec![local[0]];
        battle.logic_selection = vec![local[0]];
        battle
            .simulation
            .spectre_selections
            .extend([local[1], foreign]);
        super::apply_spectre_selections(&mut battle);
        assert_eq!(battle.logic_selection, [local[1]]);
        assert_eq!(battle.selected, [local[1], local[0]]);
        assert!(battle.simulation.spectre_selections.is_empty());
    }
}
