//! Render diagnostic using projected geometry, engine pointer input and real commands.
use super::AttackPointer;
use crate::{
    input::{self, Mouse, PickTarget, picking::View},
    playtest::Match,
    playtest_view::{self, Capture},
};
use bevy::{
    app::AppExit,
    prelude::*,
    window::{CursorOptions, PrimaryWindow},
};
use generals_sim::skirmish::{Faction, Kind, Order};
use std::error::Error;

#[derive(Resource)]
pub struct TargetCapture {
    target: usize,
    phase: Phase,
    point: Option<Vec2>,
}

enum Phase {
    Target,
    Empty,
    Return,
    Done,
}

pub fn prepare(battle: &mut Match) -> Result<TargetCapture, Box<dyn Error>> {
    crate::infantry_verification::prepare(battle, Faction::Usa)?;
    Ok(TargetCapture {
        target: crate::skirmish_verification::friendly_entity(
            &battle.simulation,
            Kind::CommandCenter,
            1,
        )?,
        phase: Phase::Target,
        point: None,
    })
}

pub fn register(app: &mut App, scenario: TargetCapture) {
    app.insert_resource(scenario)
        .add_systems(
            Update,
            (position, move_pointer).chain().before(input::pointer),
        )
        .add_systems(Update, inspect.after(super::update));
}

fn position(
    view: View,
    battle: Res<Match>,
    mut scenario: ResMut<TargetCapture>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let battle = battle.into_inner();
    let time = time.into_inner();
    if matches!(scenario.phase, Phase::Done) {
        return;
    }
    if time.elapsed_secs() > 45.0 {
        error!("Original targeting cursor did not complete projected-pointer verification");
        exit.write(AppExit::error());
        return;
    }
    scenario.point = view.target_cursor_point(battle, &scenario);
}

impl View<'_, '_> {
    fn target_cursor_point(self, battle: &Match, scenario: &TargetCapture) -> Option<Vec2> {
        if matches!(scenario.phase, Phase::Empty) {
            empty_point(&self, battle)
        } else {
            target_point(&self, battle, scenario.target)
        }
    }
}

fn move_pointer(
    scenario: Res<TargetCapture>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut clock: ResMut<Time<Virtual>>,
) {
    let scenario = scenario.into_inner();
    if let Some(point) = scenario.point {
        // This exercises Bevy's window pointer state and the ordinary pick path;
        // it does not assert delivery of a physical macOS mouse event.
        window.set_cursor_position(Some(point));
        clock.pause();
    }
}

fn target_point(view: &View, battle: &Match, target: usize) -> Option<Vec2> {
    let actor = &battle.simulation.entities[target];
    let stats = &battle.simulation.catalog[actor.type_index];
    let base = playtest_view::unit_position(&battle.simulation, target);
    for fraction in [0.5, 0.25, 0.75] {
        let point = view
            .camera
            .0
            .world_to_viewport(
                view.camera.1,
                base + Vec3::Y * (stats.geometry_bounds.max_height * fraction * 0.1),
            )
            .ok()?;
        if point.x >= 0.0
            && point.y >= 0.0
            && point.x < view.window.width()
            && point.y < view.window.height()
            && !crate::playtest_ui::captures_pointer(
                battle,
                &view.window,
                point,
                &view.ui,
                &view.promotions,
            )
            && matches!(view.pick(battle, point), Some(PickTarget::Unit(id)) if id == target)
        {
            return Some(point);
        }
    }
    None
}

fn empty_point(view: &View, battle: &Match) -> Option<Vec2> {
    for fraction in [
        Vec2::new(0.2, 0.2),
        Vec2::new(0.8, 0.2),
        Vec2::new(0.5, 0.2),
    ] {
        let point = view.window.size() * fraction;
        if view.terrain_point(point).is_some() && view.pick(battle, point).is_none() {
            return Some(point);
        }
    }
    None
}

fn inspect(
    mut scenario: ResMut<TargetCapture>,
    mut battle: ResMut<Match>,
    mut mouse: ResMut<Mouse>,
    pointer: Single<&Node, With<AttackPointer>>,
    window: Single<&CursorOptions, With<PrimaryWindow>>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let pointer = pointer.into_inner();
    let window = window.into_inner();
    let visible = pointer.display != Display::None;
    match scenario.phase {
        Phase::Target if mouse.hovered == Some(scenario.target) && visible && !window.visible => {
            let target = scenario.target;
            let issued = input::context_order(
                &mut battle,
                None,
                Some(PickTarget::Unit(target)),
                &mut mouse,
            );
            let commanded = battle.selected.iter().any(|id| {
                matches!(battle.simulation.entities[*id].ai.order, Order::Attack(victim) if victim == target)
            });
            if !issued || !commanded {
                error!("Visible target cursor did not dispatch the ordinary attack command");
                exit.write(AppExit::error());
                return;
            }
            scenario.phase = Phase::Empty;
        }
        Phase::Empty if mouse.hovered.is_none() && !visible && window.visible => {
            scenario.phase = Phase::Return;
        }
        Phase::Return if mouse.hovered == Some(scenario.target) && visible && !window.visible => {
            println!(
                "Original attack cursor: projected enemy picked, normal attack dispatched, empty-world cursor restored, target cursor reentered"
            );
            scenario.phase = Phase::Done;
            capture.finish_scenario();
        }
        _ => {}
    }
}
