//! Original anchored building placement from `PlaceEventTranslator` and `InGameUI`.

use super::{Mouse, picking::View};
use crate::{
    playtest::Match,
    source_audio::{Voice, VoiceRequest},
};
use bevy::prelude::*;

pub(super) struct ForceAttack {
    held: u8,
    pub(super) active: bool,
}

impl ForceAttack {
    pub(super) const fn new() -> Self {
        Self {
            held: 0,
            active: false,
        }
    }

    pub(super) const fn reset(&mut self) {
        self.held = 0;
        self.active = false;
    }

    pub(super) const fn event(&mut self, key: KeyCode, pressed: bool, repeat: bool) {
        let bit = match key {
            KeyCode::ControlLeft => 1,
            KeyCode::ControlRight => 2,
            KeyCode::ShiftLeft => 4,
            KeyCode::ShiftRight => 8,
            KeyCode::AltLeft => 16,
            KeyCode::AltRight => 32,
            _ => return,
        };
        if pressed {
            self.held |= bit;
            // MetaEvent's KEY_NONE DOWN mapping matches exactly CTRL. Adding
            // Shift/Alt later does not emit the separate END_FORCEATTACK event.
            if !repeat && self.held & 3 != 0 && self.held & !3 == 0 {
                self.active = true;
            }
        } else {
            self.held &= !bit;
            // onKeyModStateRemoved releases the recorded CTRL mapping as soon
            // as CTRL is absent, independently of other modifier release order.
            let control_keys = self.held & 3;
            if control_keys == 0 {
                self.active = false;
            }
        }
    }
}

pub struct Placement {
    pub type_index: usize,
    builder: usize,
    angle: f32,
    anchor: Option<[IVec2; 2]>,
    position: Option<Vec2>,
    legal: bool,
    power: Option<&'static str>,
}

impl Placement {
    pub const fn new(type_index: usize, builder: usize, angle: f32) -> Self {
        Self {
            type_index,
            builder,
            angle,
            anchor: None,
            position: None,
            legal: true,
            power: None,
        }
    }

    pub const fn special_power(
        type_index: usize,
        caster: usize,
        angle: f32,
        name: &'static str,
    ) -> Self {
        Self {
            power: Some(name),
            ..Self::new(type_index, caster, angle)
        }
    }

    pub fn pose(&self) -> Option<(Vec2, f32, bool)> {
        self.position
            .map(|position| (position, self.angle, self.legal))
    }

    pub fn orientation_indicator(&self) -> Option<(Vec2, f32, bool)> {
        let [start, end] = self.anchor?;
        let delta = (end - start).as_vec2();
        let squared = [delta.x * delta.x, delta.y * delta.y];
        self.position
            .map(|point| (point, self.angle, (squared[0] + squared[1]).sqrt() >= 5.0))
    }

    const fn press(&mut self, pixel: IVec2, over_ui: bool, terrain: Option<Vec2>) -> bool {
        if self.anchor.is_none() && !over_ui {
            if terrain.is_none() {
                return false;
            }
            self.anchor = Some([pixel, pixel]);
        }
        true
    }

    fn motion(&mut self, pixel: IVec2, over_ui: bool) {
        if !over_ui && let Some([start, end]) = &mut self.anchor {
            let delta = (pixel - *start).as_vec2();
            let squared = [delta.x * delta.x, delta.y * delta.y];
            if (squared[0] + squared[1]).sqrt() >= 5.0 {
                *end = pixel;
            }
        }
    }

    fn update(
        &mut self,
        cursor: IVec2,
        force_attack: bool,
        frame: u64,
        mut project: impl FnMut(IVec2) -> Option<Vec2>,
        legal: impl FnOnce(Vec2, f32) -> bool,
    ) {
        let mut angle = self.angle;
        let location = if let Some([start, end]) = self.anchor {
            if start != end
                && let Some(world_start) = project(start)
                && let Some(world_end) = project(end)
            {
                angle = facing(world_end - world_start);
                if force_attack {
                    angle = snap_facing(angle);
                }
            }
            start
        } else {
            cursor
        };
        if let Some(position) = project(location) {
            self.position = Some(position);
            self.angle = angle;
            // GameClient's frame is advanced by the logic clock, not render frames.
            if frame & 1 != 0 {
                self.legal = legal(position, angle);
            }
        }
    }

    fn release(&self, mut project: impl FnMut(IVec2) -> Option<Vec2>) -> Option<Vec2> {
        // WindowTranslator forwards anchored button-up even over a GUI window.
        let [start, _] = self.anchor?;
        project(start)
    }
}

fn facing(direction: Vec2) -> f32 {
    // Zero Hour selects Coord2D::toAngle's division branch (RTS_GENERALS=0).
    // Reciprocal multiplication and atan2 have different source rounding.
    let squared = [direction.x * direction.x, direction.y * direction.y];
    let distance = (squared[0] + squared[1]).sqrt();
    if distance == 0.0 {
        return 0.0;
    }
    let angle = (direction.x / distance).clamp(-1.0, 1.0).acos();
    if direction.y < 0.0 { -angle } else { angle }
}

fn snap_facing(angle: f32) -> f32 {
    // WWMath::Round rounds negative half ties toward positive infinity.
    let steps = angle / std::f32::consts::FRAC_PI_4;
    (steps + 0.5).floor() * std::f32::consts::FRAC_PI_4
}

pub fn pointer(
    battle: &mut Match,
    mouse: &mut Mouse,
    view: &View,
    buttons: &ButtonInput<MouseButton>,
    cursor: Vec2,
    over_ui: bool,
) -> bool {
    let Some(mut placement) = battle.placing.take() else {
        return false;
    };
    mouse.anchor = None;
    mouse.dragging = false;
    let scale = view.window.scale_factor();
    let pixel = (cursor * scale).as_ivec2();
    let project = |point: IVec2| view.terrain_point(point.as_vec2() / scale);
    let index = placement.type_index;
    let builder = placement.builder;
    let power = placement.power;
    // GameEngine updates the client's place icon before propagating messages.
    // A release therefore commits the displayed angle, even if its raw motion
    // is received in the same update as the release.
    placement.update(
        pixel,
        mouse.placement_modifiers.active,
        battle.simulation.tick,
        project,
        |point, angle| {
            let point = point.to_array();
            if power.is_some() {
                let height = battle.simulation.ground_height(point);
                return super::locally_controlled(battle, builder)
                    && battle.simulation.can_place_sneak_attack(
                        builder,
                        index,
                        [point[0], point[1], height],
                        angle,
                        false,
                    );
            }
            battle
                .simulation
                .can_build_facing(builder, index, point, angle)
        },
    );
    if buttons.just_pressed(MouseButton::Left) && !over_ui && placement.anchor.is_none() {
        let builder_exists = battle
            .simulation
            .entities
            .get(placement.builder)
            .is_some_and(generals_sim::skirmish::Entity::alive);
        if !builder_exists || !placement.press(pixel, false, project(pixel)) {
            // These source cancellation branches retain the raw down message.
            return false;
        }
    }
    placement.motion(pixel, over_ui);
    if buttons.just_released(MouseButton::Left)
        && let Some(position) = placement.release(project)
        && commit(battle, mouse, &mut placement, position)
    {
        return true;
    }
    battle.placing = Some(placement);
    true
}

fn commit(battle: &mut Match, mouse: &mut Mouse, placement: &mut Placement, point: Vec2) -> bool {
    if let Some(name) = placement.power {
        return commit_power(battle, mouse, placement, point, name);
    }
    // MSG_DOZER_CONSTRUCT is materialized once at commit, never by preview queries.
    if let Err(error) = battle
        .simulation
        .construction_admission(placement.builder, placement.type_index)
    {
        eprintln!("Building placement rejected before logic message: {error}");
        let retry = matches!(
            error,
            "Not enough money" | "Production queue is full" | "Unit type limit reached"
        );
        if !retry {
            mouse.pending_command = super::CommandMode::Normal;
        }
        return !retry;
    }
    if !battle.simulation.can_build_facing(
        placement.builder,
        placement.type_index,
        point.to_array(),
        placement.angle,
    ) {
        placement.anchor = None;
        mouse.pending_command = super::CommandMode::Normal;
        return false;
    }
    if !battle.commands.executing {
        super::commands::enqueue(
            battle,
            Some(mouse),
            super::commands::Kind::Build {
                unit: placement.type_index,
                point,
                angle: placement.angle,
            },
        );
        mouse.voices.push(VoiceRequest::new(
            &battle.simulation,
            &battle.selected,
            Voice::Build,
            mouse.waypoint_mode,
        ));
        mouse.pending_command = super::CommandMode::Normal;
        return true;
    }
    drop(battle.command_message_members());
    match battle.simulation.build_facing(
        placement.builder,
        placement.type_index,
        point.to_array(),
        placement.angle,
    ) {
        Ok(_) => {
            mouse.voices.push(VoiceRequest::new(
                &battle.simulation,
                &battle.selected,
                Voice::Build,
                mouse.waypoint_mode,
            ));
            mouse.pending_command = super::CommandMode::Normal;
            true
        }
        Err(error) => {
            eprintln!("Building placement rejected: {error}");
            match error {
                // canMakeUnit leaves the anchor intact if funds were spent
                // after opening placement. Location failures unhook only it.
                "Not enough money" => false,
                "Building site is obstructed or outside the map"
                | "Builder cannot reach this site" => {
                    placement.anchor = None;
                    mouse.pending_command = super::CommandMode::Normal;
                    false
                }
                _ => true,
            }
        }
    }
}

fn commit_power(
    battle: &mut Match,
    mouse: &mut Mouse,
    placement: &mut Placement,
    point: Vec2,
    name: &str,
) -> bool {
    if !super::locally_controlled(battle, placement.builder) {
        mouse.pending_command = super::CommandMode::Normal;
        return true;
    }
    let point = point.to_array();
    let height = battle.simulation.ground_height(point);
    let position = [point[0], point[1], height];
    if let Some(failure) = battle.simulation.sneak_attack_placement_failure(
        placement.builder,
        placement.type_index,
        position,
        placement.angle,
        true,
    ) {
        placement.anchor = None;
        // The release keeps placement active, with the original legal-build feedback.
        mouse
            .sneak_placement_rejections
            .push((battle.simulation.tick, failure));
        mouse.voices.push(VoiceRequest::new(
            &battle.simulation,
            &[placement.builder],
            Voice::NoBuild,
            false,
        ));
        return false;
    }
    // PlaceEventTranslator dismisses the placement icon when it sends the
    // request. ActionManager subsequently checks the reference's default angle.
    super::commands::enqueue(
        battle,
        Some(mouse),
        super::commands::Kind::Power {
            caster: placement.builder,
            name: name.to_owned(),
            specific: true,
            target: generals_sim::skirmish::PowerTarget::Construction {
                position,
                angle: placement.angle,
            },
        },
    );
    mouse.pending_command = super::CommandMode::Normal;
    true
}

#[cfg(test)]
pub mod tests;
